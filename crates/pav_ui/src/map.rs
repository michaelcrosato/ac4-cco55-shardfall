//! The full-screen parchment map (PLAN §9.15): coastlines and contours from
//! [`MapData`] (closed lines also get a land tint, so stacked contours shade the relief),
//! fills, labels, live icons, a compass rose, a scale bar and a legend.

use std::collections::BTreeSet;
use std::sync::Arc;

use egui::epaint::{Mesh, PathShape, PathStroke};
use egui::{Align2, Color32, Painter, Pos2, Rect, Shape, Stroke, pos2, vec2};
use pav_core::play::{MapData, MapIcon};

use crate::paint::{self, quantize};
use crate::theme::{Theme, alpha, hex, mix};

/// What part of the map is shown.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MapView {
    /// World point (x, z) at the centre of the map area.
    pub center: glam::Vec2,
    /// 1 = the whole map fits; up to [`MapView::MAX_ZOOM`].
    pub zoom: f32,
    pub legend: bool,
}

impl Default for MapView {
    fn default() -> Self {
        Self { center: glam::Vec2::ZERO, zoom: 1.0, legend: false }
    }
}

impl MapView {
    pub const MAX_ZOOM: f32 = 6.0;

    /// Centres on the player icon (or the map's middle).
    pub fn centered(map: Option<&MapData>, icons: &[MapIcon]) -> Self {
        let center = icons
            .iter()
            .find(|i| i.icon == "player")
            .map(|i| i.at)
            .or_else(|| map.map(|m| (m.min + m.max) * 0.5))
            .unwrap_or(glam::Vec2::ZERO);
        Self { center, zoom: 1.0, legend: false }
    }

    /// Pans with a stick (x right, y up = north) and zooms by `zoom_steps` (×1.25 each),
    /// keeping the centre inside the map.
    pub fn steer(&mut self, map: Option<&MapData>, stick: glam::Vec2, zoom_steps: f32, dt: f32) {
        self.zoom = (self.zoom * 1.25f32.powf(zoom_steps)).clamp(1.0, Self::MAX_ZOOM);
        if let Some(m) = map {
            let span = (m.max - m.min).max_element().max(1.0);
            let speed = span * 0.45 / self.zoom;
            self.center += glam::Vec2::new(stick.x, -stick.y) * speed * dt.min(0.25);
            self.center = self.center.clamp(m.min, m.max);
        }
    }
}

struct Xform {
    center: glam::Vec2,
    scale: f32,
    screen: Pos2,
}

impl Xform {
    fn to_screen(&self, w: glam::Vec2) -> Pos2 {
        let d = (w - self.center) * self.scale;
        self.screen + vec2(d.x, d.y)
    }
}

/// Triangulations of a map's closed lines and fills, cached per map.
#[derive(Default)]
struct Cache {
    key: usize,
    land: Vec<(usize, Vec<[u32; 3]>)>,
    fills: Vec<Vec<[u32; 3]>>,
}

fn is_closed(pts: &[glam::Vec2]) -> bool {
    pts.len() >= 4 && pts[0].distance(pts[pts.len() - 1]) < 1e-3 * (1.0 + pts[0].length())
}

fn cache_for(ctx: &egui::Context, map: &Arc<MapData>) -> Arc<Cache> {
    let key = Arc::as_ptr(map) as usize;
    let id = egui::Id::new("pav_ui_map_cache");
    if let Some(c) = ctx.data(|d| d.get_temp::<Arc<Cache>>(id)) {
        if c.key == key {
            return c;
        }
    }
    let tri = |pts: &[glam::Vec2]| {
        let p: Vec<Pos2> = pts.iter().map(|v| pos2(v.x, v.y)).collect();
        paint::triangulate(&p)
    };
    let mut c = Cache { key, ..Default::default() };
    for (i, (pts, _, _)) in map.lines.iter().enumerate() {
        if is_closed(pts) {
            c.land.push((i, tri(&pts[..pts.len() - 1])));
        }
    }
    c.fills = map.fills.iter().map(|(pts, _)| tri(pts)).collect();
    let c = Arc::new(c);
    ctx.data_mut(|d| d.insert_temp(id, c.clone()));
    c
}

/// A display name for a map icon kind (the legend).
pub fn icon_name(icon: &str) -> &str {
    match icon {
        "player" => "You",
        "objective" => "Objective",
        "perch" => "Lookout (unsurveyed)",
        "perch_done" => "Lookout (surveyed)",
        "chest" => "Chest",
        "sheet" => "Shanty sheet",
        "bottle" => "Message in a bottle",
        "tap" => "Black Tap",
        "flotsam" => "Flotsam",
        "ship" => "Ship",
        "enemy_ship" => "Enemy ship",
        "waypoint" => "Waypoint",
        "fort" => "Fort",
        "tavern" => "Tavern",
        other => other,
    }
}

/// Draws the map into `rect` (a parchment sheet fills it). `cursor` draws a crosshair at a
/// world point (the in-play map overlay).
pub fn draw(
    p: &Painter,
    rect: Rect,
    map: Option<&Arc<MapData>>,
    icons: &[MapIcon],
    view: &MapView,
    theme: &Theme,
    cursor: Option<glam::Vec2>,
) {
    paint::parchment(p, rect, theme, 0x3a9);
    paint::rope_frame(p, rect, 18.0, 8.0, theme);
    let area = rect.shrink(46.0);
    let Some(map) = map else {
        p.text(area.center(), Align2::CENTER_CENTER, "No chart of these waters yet", theme.display(56.0), theme.ink_soft);
        return;
    };
    let span = (map.max - map.min).max(glam::Vec2::splat(1.0));
    let fit = (area.width() / span.x).min(area.height() / span.y);
    let x = Xform { center: view.center, scale: fit * view.zoom.clamp(1.0, MapView::MAX_ZOOM), screen: area.center() };
    let mp = p.with_clip_rect(area.intersect(p.clip_rect()));
    // Sea wash.
    paint::vgradient(&mp, area, alpha(theme.teal, 0.20), alpha(theme.teal, 0.12));
    // Faint grid.
    let step = nice_step(span.max_element() / view.zoom / 6.0);
    let grid = Stroke::new(1.0, alpha(theme.ink, 0.10));
    let mut gx = (map.min.x / step).floor() * step;
    while gx <= map.max.x {
        let a = x.to_screen(glam::Vec2::new(gx, map.min.y));
        let b = x.to_screen(glam::Vec2::new(gx, map.max.y));
        mp.line_segment([a, b], grid);
        gx += step;
    }
    let mut gz = (map.min.y / step).floor() * step;
    while gz <= map.max.y {
        let a = x.to_screen(glam::Vec2::new(map.min.x, gz));
        let b = x.to_screen(glam::Vec2::new(map.max.x, gz));
        mp.line_segment([a, b], grid);
        gz += step;
    }
    let cache = cache_for(p.ctx(), map);
    // Land tints under closed lines (stacked contours darken inland).
    let land = mix(theme.parchment, Color32::from_rgb(196, 170, 112), 0.6);
    for (i, tris) in &cache.land {
        let pts = &map.lines[*i].0;
        let mut mesh = Mesh::default();
        for v in &pts[..pts.len() - 1] {
            mesh.colored_vertex(x.to_screen(*v), alpha(land, 0.55));
        }
        for t in tris {
            mesh.add_triangle(t[0], t[1], t[2]);
        }
        mp.add(Shape::mesh(mesh));
    }
    // Fills.
    for ((pts, color), tris) in map.fills.iter().zip(&cache.fills) {
        let col = hex(color).unwrap_or(theme.ink_soft);
        let mut mesh = Mesh::default();
        for v in pts {
            mesh.colored_vertex(x.to_screen(*v), col);
        }
        for t in tris {
            mesh.add_triangle(t[0], t[1], t[2]);
        }
        mp.add(Shape::mesh(mesh));
    }
    // Lines (coasts get a soft water ripple outside).
    let wscale = view.zoom.sqrt();
    for (pts, color, width) in &map.lines {
        let col = hex(color).unwrap_or(theme.ink);
        let sp: Vec<Pos2> = pts.iter().map(|v| x.to_screen(*v)).collect();
        if is_closed(pts) && *width >= 2.0 {
            mp.add(PathShape::line(sp.clone(), PathStroke::new(width * wscale * 4.0, alpha(theme.teal, 0.12))));
        }
        mp.add(PathShape::line(sp, PathStroke::new((width * wscale).max(1.0), col)));
    }
    // Labels.
    let label_font = theme.display(quantize(34.0));
    for (at, text) in &map.labels {
        let q = x.to_screen(*at);
        paint::text_outlined(
            &mp,
            q,
            Align2::CENTER_CENTER,
            text,
            label_font.clone(),
            theme.ink,
            alpha(theme.parchment_light, 0.85),
            3.0,
        );
    }
    // Icons.
    for ic in icons {
        let q = x.to_screen(ic.at);
        if !area.expand(20.0).contains(q) {
            continue;
        }
        if ic.icon == "player" {
            player_arrow(&mp, q, 42.0, ic.yaw, theme);
        } else {
            let col = match ic.icon.as_str() {
                "objective" => theme.gold,
                "enemy_ship" | "fort" => theme.red,
                "waypoint" => theme.teal,
                "perch" => theme.ink_soft,
                _ => theme.parchment_light,
            };
            mp.circle_filled(q + vec2(0.0, 2.0), 22.0, alpha(Color32::BLACK, 0.18));
            paint::icon(&mp, &ic.icon, q, 38.0, col, theme);
        }
    }
    if let Some(c) = cursor {
        let q = x.to_screen(c);
        let s = Stroke::new(3.0, theme.red);
        mp.circle_stroke(q, 22.0, s);
        for k in 0..4 {
            let a = k as f32 * std::f32::consts::FRAC_PI_2;
            mp.line_segment([paint::polar(q, 14.0, a), paint::polar(q, 34.0, a)], s);
        }
    }
    // Compass rose (bottom right) and scale bar (bottom left).
    compass_rose(p, area.right_bottom() + vec2(-90.0, -90.0), 70.0, theme);
    let bar_world = nice_step(160.0 / x.scale);
    let bar_px = bar_world * x.scale;
    let b0 = area.left_bottom() + vec2(30.0, -36.0);
    p.line_segment([b0, b0 + vec2(bar_px, 0.0)], Stroke::new(6.0, theme.ink));
    p.line_segment([b0 + vec2(bar_px * 0.5, 0.0), b0 + vec2(bar_px, 0.0)], Stroke::new(3.0, theme.parchment));
    for t in [0.0f32, 0.5, 1.0] {
        p.line_segment([b0 + vec2(bar_px * t, -9.0), b0 + vec2(bar_px * t, 9.0)], Stroke::new(2.5, theme.ink));
    }
    p.text(
        b0 + vec2(bar_px + 14.0, 0.0),
        Align2::LEFT_CENTER,
        format!("{} m", bar_world.round() as i64),
        theme.bold(26.0),
        theme.ink,
    );
    if view.legend {
        legend(p, area, icons, theme);
    }
}

/// The legend panel (toggled with △).
fn legend(p: &Painter, area: Rect, icons: &[MapIcon], theme: &Theme) {
    let kinds: BTreeSet<&str> = icons.iter().map(|i| i.icon.as_str()).collect();
    let rows: Vec<&str> = kinds.into_iter().take(9).collect();
    let h = 70.0 + rows.len() as f32 * 50.0;
    let r = Rect::from_min_size(pos2(area.right() - 440.0, area.top() + 10.0), vec2(420.0, h));
    paint::ink_panel(p, r, theme, 1.0);
    p.text(r.left_top() + vec2(24.0, 18.0), Align2::LEFT_TOP, "Legend", theme.display(40.0), theme.gold);
    for (i, k) in rows.iter().enumerate() {
        let y = r.top() + 88.0 + i as f32 * 50.0;
        let c = pos2(r.left() + 46.0, y);
        if *k == "player" {
            player_arrow(p, c, 34.0, 0.0, theme);
        } else {
            paint::icon(p, k, c, 34.0, theme.parchment_light, theme);
        }
        p.text(pos2(r.left() + 84.0, y), Align2::LEFT_CENTER, icon_name(k), theme.body(28.0), theme.parchment);
    }
}

fn player_arrow(p: &Painter, c: Pos2, s: f32, yaw_deg: f32, theme: &Theme) {
    let a = yaw_deg.to_radians();
    let pts = vec![
        paint::polar(c, s * 0.55, a),
        paint::polar(c, s * 0.45, a + 2.5),
        paint::polar(c, s * 0.2, a + std::f32::consts::PI),
        paint::polar(c, s * 0.45, a - 2.5),
    ];
    let shadow: Vec<Pos2> = pts.iter().map(|q| *q + vec2(0.0, 3.0)).collect();
    paint::fill_poly(p, &shadow, alpha(Color32::BLACK, 0.35));
    paint::fill_poly(p, &pts, theme.amber_bright);
    p.add(PathShape::closed_line(pts, Stroke::new(3.0, theme.ink)));
}

/// A 16-point compass rose with N.
pub fn compass_rose(p: &Painter, c: Pos2, r: f32, theme: &Theme) {
    p.circle_stroke(c, r * 0.78, Stroke::new(2.0, alpha(theme.ink, 0.7)));
    p.circle_stroke(c, r * 0.7, Stroke::new(1.0, alpha(theme.ink, 0.5)));
    for k in 0..16 {
        let a = k as f32 * std::f32::consts::TAU / 16.0;
        let len = if k % 4 == 0 {
            r
        } else if k % 2 == 0 {
            r * 0.62
        } else {
            r * 0.42
        };
        let wd = if k % 4 == 0 { 0.13 } else { 0.1 };
        let tip = paint::polar(c, len, a);
        let l = paint::polar(c, r * wd, a - std::f32::consts::FRAC_PI_2);
        let rr = paint::polar(c, r * wd, a + std::f32::consts::FRAC_PI_2);
        let dark = if k % 4 == 0 { theme.ink } else { theme.ink_soft };
        paint::fill_convex(p, vec![c, l, tip], dark);
        paint::fill_convex(p, vec![c, tip, rr], if k == 0 { theme.red } else { theme.parchment_light });
        p.add(PathShape::closed_line(vec![l, tip, rr], Stroke::new(1.0, alpha(theme.ink, 0.8))));
    }
    p.circle_filled(c, r * 0.08, theme.gold);
    paint::text_outlined(
        p,
        paint::polar(c, r * 1.22, 0.0),
        Align2::CENTER_CENTER,
        "N",
        theme.bold(38.0),
        theme.ink,
        alpha(theme.parchment_light, 0.9),
        2.5,
    );
}

fn nice_step(x: f32) -> f32 {
    let x = x.max(1e-3);
    let e = 10f32.powf(x.log10().floor());
    let m = x / e;
    let n = if m < 1.5 {
        1.0
    } else if m < 3.5 {
        2.0
    } else if m < 7.5 {
        5.0
    } else {
        10.0
    };
    n * e
}
