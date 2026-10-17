//! The in-play HUD (PLAN §11.4), drawn from the game's [`UiModel`] each frame:
//! health and heat (top left), compass (top centre), objective and toasts (top right),
//! prompts (bottom right), ship widget (bottom left at sea), subtitles and progress bars
//! (bottom centre), world markers and awareness arcs (projected), full-screen effects,
//! banners, and the game's overlays (tiles, tool wheel, results, map, cinematic skip).
//!
//! Stateless apart from a small fade memory kept in the egui context; animation comes from
//! the model's ages and [`HudEnv::time`].

use std::f32::consts::{PI, TAU};

use egui::epaint::{PathShape, PathStroke};
use egui::{Align2, Color32, Id, LayerId, Order, Painter, Pos2, Rect, Stroke, StrokeKind, Vec2, pos2, vec2};
use pav_core::play::{
    Awareness, AwarenessArc, Banner, Compass, HudItem, MarkerKind, Objective, Overlay, Prompt, Results, ScreenFx, Subtitle, Tile,
    Toast, UiModel, WeaponPip, WorldMarker,
};

use crate::Projector;
use crate::bindings::Bindings;
use crate::glyph::{self, Glyph, GlyphFamily, Piece};
use crate::input::Nav;
use crate::map::{self, MapView};
use crate::paint::{self, quantize};
use crate::settings::Settings;
use crate::theme::{Theme, alpha, hex, mix};

/// Everything the HUD needs besides the model.
#[derive(Clone, Copy)]
pub struct HudEnv<'a> {
    pub theme: &'a Theme,
    pub bindings: &'a Bindings,
    /// The last-used device's glyph family (the prompts setting may override it).
    pub family: GlyphFamily,
    pub settings: &'a Settings,
    /// World → screen points for markers.
    pub proj: Projector,
    /// A running clock (s) for pulses.
    pub time: f32,
}

/// The layer the HUD and the front end paint on.
pub fn layer() -> LayerId {
    LayerId::new(Order::Background, Id::new("pav_ui"))
}

/// The painter the HUD and the front end share (one layer, painted in call order).
pub fn painter(ctx: &egui::Context) -> Painter {
    ctx.layer_painter(layer())
}

/// The screen rectangle in points.
pub fn screen(ctx: &egui::Context) -> Rect {
    ctx.viewport_rect()
}

/// The safe area (5% margin by default).
pub fn safe(screen: Rect, theme: &Theme) -> Rect {
    let m = theme.safe_area;
    screen.shrink2(vec2(screen.width() * m, screen.height() * m))
}

#[derive(Clone, Copy, Default)]
struct Memory {
    /// Last time health was below full or heat above zero.
    health_seen: f32,
    initialized: bool,
}

/// Draws the HUD for one frame.
pub fn draw(ctx: &egui::Context, ui: &UiModel, env: &HudEnv) {
    let p = painter(ctx);
    let scr = screen(ctx);
    draw_on(&p, scr, ui, env);
}

/// Draws the HUD with a given painter and screen rect.
pub fn draw_on(p: &Painter, scr: Rect, ui: &UiModel, env: &HudEnv) {
    let theme = env.theme;
    let family = glyph::resolve_family(env.settings.controls.prompts, env.family);
    let env = HudEnv { family, ..*env };
    let safe = safe(scr, theme);
    screen_fx_under(p, scr, &ui.screen, theme, env.time);
    let cinematic = matches!(ui.overlay, Some(Overlay::Cinematic { .. })) || ui.screen.letterbox > 0.5;
    let full_overlay = matches!(ui.overlay, Some(Overlay::Tiles { .. } | Overlay::Results(_) | Overlay::Map { .. }));
    let wheel = matches!(ui.overlay, Some(Overlay::Wheel { .. }));
    // In the world.
    if !full_overlay && !wheel {
        for m in &ui.markers {
            marker(p, scr, safe, m, &env);
        }
        for a in &ui.awareness {
            awareness(p, scr, safe, a, &env);
        }
    }
    if !cinematic && !full_overlay {
        top_left(p, safe, &ui.hud, &env);
        if env.settings.gameplay.show_compass {
            if let Some(c) = &ui.compass {
                compass(p, safe, c, &env);
            }
        }
        let mut y = safe.top();
        if env.settings.gameplay.show_objective {
            if let Some(o) = &ui.objective {
                y = objective(p, safe, o, &env);
            }
        }
        toasts(p, safe, y + 16.0, &ui.toasts, &env);
        if env.settings.gameplay.show_prompts && !wheel {
            prompts(p, safe, &ui.prompts, &env);
        }
        bottom_left(p, safe, &ui.hud, &env);
    }
    if full_overlay {
        if let Some(o) = &ui.overlay {
            overlay(p, scr, safe, o, ui, &env);
        }
        // Subtitles stay readable above the overlay and its hint bar.
        subtitles(p, safe.with_max_y(safe.bottom() - 62.0), &ui.subtitles, &env);
    } else {
        let sub_top = subtitles(p, safe, &ui.subtitles, &env);
        if !wheel {
            bars(p, safe, sub_top, &ui.hud, &env);
        }
        if let Some(b) = &ui.banner {
            banner(p, scr, b, &env);
        }
        if let Some(o) = &ui.overlay {
            overlay(p, scr, safe, o, ui, &env);
        }
    }
    screen_fx_over(p, scr, &ui.screen);
}

// ---------------------------------------------------------------- full screen

fn screen_fx_under(p: &Painter, scr: Rect, fx: &ScreenFx, theme: &Theme, time: f32) {
    if fx.desaturate > 0.0 {
        // The renderer desaturates the scene; the UI adds a cold wash and a heartbeat.
        let beat = 0.5 + 0.5 * (time * TAU * 1.1).sin().powi(8);
        p.rect_filled(scr, 0.0, alpha(Color32::from_rgb(40, 44, 52), 0.22 * fx.desaturate));
        paint::vignette(p, scr, 0.55, alpha(Color32::from_rgb(20, 8, 8), (0.35 + 0.25 * beat) * fx.desaturate));
    }
    if fx.vignette > 0.0 {
        paint::vignette(p, scr, 0.62, alpha(mix(theme.red, Color32::BLACK, 0.35), 0.75 * fx.vignette.min(1.0)));
    }
    if fx.letterbox > 0.0 {
        let h = scr.height() * 0.11 * fx.letterbox.min(1.0);
        p.rect_filled(Rect::from_min_size(scr.min, vec2(scr.width(), h)), 0.0, Color32::BLACK);
        p.rect_filled(Rect::from_min_max(pos2(scr.left(), scr.bottom() - h), scr.max), 0.0, Color32::BLACK);
    }
}

fn screen_fx_over(p: &Painter, scr: Rect, fx: &ScreenFx) {
    if fx.white > 0.0 {
        p.rect_filled(scr, 0.0, alpha(Color32::from_rgb(250, 248, 240), fx.white.min(1.0)));
    }
    if fx.fade > 0.0 {
        p.rect_filled(scr, 0.0, alpha(Color32::BLACK, fx.fade.min(1.0)));
    }
}

// ---------------------------------------------------------------- top left

fn top_left(p: &Painter, safe: Rect, items: &[HudItem], env: &HudEnv) {
    let theme = env.theme;
    let hc = env.settings.accessibility.high_contrast;
    let mut y = safe.top();
    for it in items {
        if let HudItem::Health { frac, segments, heat } = it {
            let mem_id = Id::new("pav_ui_hud_health");
            let mut mem = p.ctx().data(|d| d.get_temp::<Memory>(mem_id)).unwrap_or_default();
            if !mem.initialized || *frac < 0.999 || *heat > 0 || env.time < mem.health_seen {
                mem.health_seen = env.time;
                mem.initialized = true;
            }
            p.ctx().data_mut(|d| d.insert_temp(mem_id, mem));
            let idle = env.time - mem.health_seen;
            let a = 1.0 - ((idle - 4.0) / 0.6).clamp(0.0, 1.0);
            if a <= 0.0 {
                continue;
            }
            let n = (*segments).max(1) as usize;
            let seg_w = 52.0;
            let gap = 7.0;
            let h = 20.0;
            let x0 = safe.left();
            let total = n as f32 * seg_w + (n - 1) as f32 * gap;
            if hc {
                p.rect_filled(
                    Rect::from_min_size(pos2(x0 - 10.0, y - 8.0), vec2(total + 20.0, h + 16.0)),
                    6.0,
                    alpha(theme.ink, 0.9 * a),
                );
            }
            let low = *frac < 0.3;
            let pulse = if low { 0.6 + 0.4 * (env.time * TAU * 1.4).sin().abs() } else { 1.0 };
            let fill = if low { theme.danger(env.settings.accessibility.colorblind) } else { theme.parchment_light };
            for i in 0..n {
                let x = x0 + i as f32 * (seg_w + gap);
                let skew = 6.0;
                let quad = |x0: f32, x1: f32| vec![pos2(x0 + skew, y), pos2(x1 + skew, y), pos2(x1, y + h), pos2(x0, y + h)];
                let back = quad(x, x + seg_w);
                let shadow: Vec<Pos2> = back.iter().map(|q| *q + vec2(0.0, 3.0)).collect();
                paint::fill_convex(p, shadow, alpha(Color32::BLACK, 0.45 * a));
                paint::fill_convex(p, back.clone(), alpha(theme.ink, 0.75 * a));
                let f = (frac * n as f32 - i as f32).clamp(0.0, 1.0);
                if f > 0.0 {
                    paint::fill_convex(p, quad(x, x + seg_w * f), alpha(fill, a * pulse));
                }
                p.add(PathShape::closed_line(back, Stroke::new(1.5, alpha(theme.ink, a))));
            }
            if *heat > 0 {
                wanted_poster(p, pos2(x0 + total + 44.0, y + h * 0.5 + 8.0), *heat, a, env);
            }
            y += h + 30.0;
        }
    }
    for it in items {
        if let HudItem::Counter { icon, label: _, value } = it {
            let c = pos2(safe.left() + 20.0, y + 20.0);
            if hc {
                p.rect_filled(
                    Rect::from_min_size(pos2(safe.left() - 8.0, y - 4.0), vec2(200.0, 48.0)),
                    6.0,
                    alpha(theme.ink, 0.9),
                );
            }
            paint::icon(p, icon, c, 36.0, theme.gold, theme);
            paint::text_shadowed(
                p,
                pos2(c.x + 30.0, c.y),
                Align2::LEFT_CENTER,
                value,
                theme.bold(32.0),
                theme.parchment_light,
                0.8,
            );
            y += 50.0;
        }
    }
}

fn wanted_poster(p: &Painter, c: Pos2, heat: u8, a: f32, env: &HudEnv) {
    let theme = env.theme;
    let r = Rect::from_center_size(c, vec2(56.0, 70.0));
    let pts = paint::torn_outline(r, 0x51 + heat as u32, 2.0, 7.0);
    let shadow: Vec<Pos2> = pts.iter().map(|q| *q + vec2(2.0, 3.0)).collect();
    paint::fill_poly(p, &shadow, alpha(Color32::BLACK, 0.4 * a));
    paint::fill_poly(p, &pts, alpha(theme.parchment, a));
    p.add(PathShape::closed_line(pts, Stroke::new(1.5, alpha(theme.rope_dark, a))));
    paint::icon(p, "skull", c + vec2(0.0, -8.0), 34.0, alpha(theme.ink, a), theme);
    let danger = theme.danger(env.settings.accessibility.colorblind);
    for i in 0..3 {
        let q = pos2(c.x - 15.0 + i as f32 * 15.0, r.bottom() - 11.0);
        if i < heat {
            p.circle_filled(q, 6.0, alpha(danger, a));
            p.circle_stroke(q, 6.0, Stroke::new(1.5, alpha(theme.ink, a)));
        } else {
            p.circle_stroke(q, 5.0, Stroke::new(1.5, alpha(theme.ink_soft, a)));
        }
    }
}

// ---------------------------------------------------------------- compass

fn compass(p: &Painter, safe: Rect, c: &Compass, env: &HudEnv) {
    let theme = env.theme;
    let acc = &env.settings.accessibility;
    let w = 780.0;
    let h = 52.0;
    let cx = safe.center().x;
    let rect = Rect::from_center_size(pos2(cx, safe.top() + h * 0.5), vec2(w, h));
    let band = alpha(theme.ink, if acc.high_contrast { 0.9 } else { 0.55 });
    let half = Rect::from_min_max(rect.min, pos2(cx, rect.bottom()));
    paint::hgradient(p, half, Color32::TRANSPARENT, band);
    paint::hgradient(p, Rect::from_min_max(pos2(cx, rect.top()), rect.max), band, Color32::TRANSPARENT);
    p.hline(rect.x_range(), rect.bottom(), Stroke::new(1.5, alpha(theme.gold, 0.35)));
    let span = 95.0; // degrees each side
    let to_x = |rel: f32| cx + rel / span * (w * 0.5);
    let fade = |rel: f32| (1.0 - (rel.abs() / span).powi(3)).clamp(0.0, 1.0);
    let rel = |bearing: f32| {
        let mut d = (bearing - c.heading).rem_euclid(360.0);
        if d > 180.0 {
            d -= 360.0;
        }
        d
    };
    let mut deg = 0;
    while deg < 360 {
        let r = rel(deg as f32);
        if r.abs() <= span {
            let x = to_x(r);
            let a = fade(r);
            let (label, big) = match deg {
                0 => ("N", true),
                90 => ("E", true),
                180 => ("S", true),
                270 => ("W", true),
                45 => ("NE", false),
                135 => ("SE", false),
                225 => ("SW", false),
                315 => ("NW", false),
                _ => ("", false),
            };
            if !label.is_empty() {
                let col = if deg == 0 { theme.gold } else { theme.parchment_light };
                let font = if big { theme.bold(34.0) } else { theme.bold(24.0) };
                paint::text_shadowed(
                    p,
                    pos2(x, rect.center().y - 2.0),
                    Align2::CENTER_CENTER,
                    label,
                    font,
                    alpha(col, a),
                    0.7 * a,
                );
            } else {
                let len = if deg % 15 == 0 { 12.0 } else { 7.0 };
                p.vline(x, (rect.bottom() - len)..=rect.bottom() - 2.0, Stroke::new(2.0, alpha(theme.parchment, 0.7 * a)));
            }
        }
        deg += 5;
    }
    // The heading pointer.
    paint::fill_convex(
        p,
        vec![pos2(cx - 9.0, rect.top() - 4.0), pos2(cx + 9.0, rect.top() - 4.0), pos2(cx, rect.top() + 9.0)],
        theme.gold,
    );
    for m in &c.marks {
        let r = rel(m.bearing);
        let col = theme.marker_color(m.kind, acc.colorblind, acc.high_contrast);
        if m.kind == MarkerKind::Threat {
            if r.abs() <= span {
                let x = to_x(r);
                paint::fill_convex(
                    p,
                    vec![pos2(x - 8.0, rect.bottom() + 14.0), pos2(x + 8.0, rect.bottom() + 14.0), pos2(x, rect.bottom() + 2.0)],
                    col,
                );
            }
            continue;
        }
        let edge = r.abs() > span * 0.92;
        if edge && !matches!(m.kind, MarkerKind::Objective | MarkerKind::Waypoint | MarkerKind::Target) {
            continue;
        }
        let x = to_x(r.clamp(-span * 0.92, span * 0.92));
        let q = pos2(x, rect.center().y);
        marker_icon(p, q, 30.0, m.kind, col, theme);
        if edge {
            let dir = if r < 0.0 { -PI * 0.5 } else { PI * 0.5 };
            paint::arrow_head(p, q + vec2(if r < 0.0 { -24.0 } else { 24.0 }, 0.0), 16.0, dir, col);
        }
        if m.distance > 0.0
            && matches!(m.kind, MarkerKind::Objective | MarkerKind::Waypoint | MarkerKind::Target | MarkerKind::Collectible)
        {
            paint::text_shadowed(
                p,
                pos2(x, rect.bottom() + 6.0),
                Align2::CENTER_TOP,
                &distance(m.distance),
                theme.bold(24.0),
                theme.parchment_light,
                0.8,
            );
        }
    }
}

fn distance(m: f32) -> String {
    if m >= 1000.0 { format!("{:.1} km", m / 1000.0) } else { format!("{} m", m.round() as i32) }
}

/// The small icon for a marker kind.
fn marker_icon(p: &Painter, c: Pos2, s: f32, kind: MarkerKind, col: Color32, theme: &Theme) {
    let ink = Stroke::new(2.0, alpha(theme.ink, 0.9));
    match kind {
        MarkerKind::Objective => {
            paint::diamond(p, c, s, col, ink);
            paint::diamond(p, c, s * 0.36, theme.ink, Stroke::NONE);
        }
        MarkerKind::Target => {
            paint::diamond(p, c, s, col, ink);
            paint::icon(p, "skull", c, s * 0.6, theme.ink, theme);
        }
        MarkerKind::Waypoint => paint::icon(p, "waypoint", c, s, col, theme),
        MarkerKind::Enemy | MarkerKind::Threat => {
            let pts = vec![c + vec2(-0.45, -0.35) * s, c + vec2(0.45, -0.35) * s, c + vec2(0.0, 0.4) * s];
            p.add(egui::Shape::convex_polygon(pts, col, ink));
        }
        MarkerKind::Ally => {
            p.circle_filled(c, s * 0.34, col);
            p.circle_stroke(c, s * 0.34, ink);
        }
        MarkerKind::Collectible => paint::icon(p, "star", c, s, col, theme),
        MarkerKind::Interact => {
            p.circle_filled(c, s * 0.22, col);
            p.circle_stroke(c, s * 0.36, Stroke::new(2.0, col));
        }
    }
}

// ---------------------------------------------------------------- top right

/// Returns the bottom of the objective block.
fn objective(p: &Painter, safe: Rect, o: &Objective, env: &HudEnv) -> f32 {
    let theme = env.theme;
    let right = safe.right();
    let mut y = safe.top();
    let hc = env.settings.accessibility.high_contrast;
    let title_a = if o.age > 6.0 { 0.75 } else { 1.0 };
    let tf = theme.bold(24.0);
    let title = o.title.to_uppercase();
    let text_font = theme.bold(32.0);
    let tw = paint::text_size(p, &title, tf.clone()).x.max(paint::text_size(p, &o.text, text_font.clone()).x + 40.0);
    if hc {
        p.rect_filled(Rect::from_min_size(pos2(right - tw - 24.0, y - 10.0), vec2(tw + 34.0, 92.0)), 8.0, alpha(theme.ink, 0.92));
    } else {
        let r = Rect::from_min_size(pos2(right - tw - 30.0, y - 4.0), vec2(tw + 40.0, 80.0));
        paint::glow(p, r, 30.0, 60.0, alpha(Color32::BLACK, 0.45));
    }
    paint::text_spaced(p, pos2(right, y), Align2::RIGHT_TOP, &title, tf, alpha(theme.gold, title_a), 1.5, None);
    y += 32.0;
    let r = paint::text_shadowed(p, pos2(right, y), Align2::RIGHT_TOP, &o.text, text_font, theme.parchment_light, 0.85);
    paint::diamond(p, pos2(r.left() - 22.0, r.center().y + 1.0), 22.0, theme.gold, Stroke::new(2.0, theme.ink));
    y = r.bottom() + 6.0;
    // Optional objectives appear when the objective changes, then fade.
    let a = 1.0 - ((o.age - 6.0) / 0.8).clamp(0.0, 1.0);
    if a > 0.0 && !o.optional.is_empty() {
        for (text, done) in &o.optional {
            let font = theme.body(28.0);
            let col = if *done { alpha(theme.parchment, 0.75 * a) } else { alpha(theme.parchment_light, a) };
            let rr = paint::text_shadowed(p, pos2(right, y + 4.0), Align2::RIGHT_TOP, text, font, col, 0.8 * a);
            let bx = Rect::from_center_size(pos2(rr.left() - 20.0, rr.center().y), Vec2::splat(20.0));
            p.rect_filled(bx, 3.0, alpha(theme.ink, 0.7 * a));
            p.rect_stroke(bx, 3.0, Stroke::new(2.0, alpha(theme.parchment, a)), StrokeKind::Inside);
            if *done {
                paint::tick(p, bx.center() + vec2(2.0, -2.0), 26.0, alpha(theme.green_tick(), a));
            }
            y = rr.bottom() + 2.0;
        }
    }
    y
}

trait Tick {
    fn green_tick(&self) -> Color32;
}

impl Tick for Theme {
    fn green_tick(&self) -> Color32 {
        mix(self.green, Color32::WHITE, 0.25)
    }
}

fn toasts(p: &Painter, safe: Rect, top: f32, list: &[Toast], env: &HudEnv) {
    let theme = env.theme;
    let mut y = top;
    for t in list.iter().take(3) {
        let a_in = (t.age / 0.25).clamp(0.0, 1.0);
        let a_out = 1.0 - ((t.age - 4.0) / 0.5).clamp(0.0, 1.0);
        let a = a_in.min(a_out);
        if a <= 0.0 {
            continue;
        }
        let slide = (1.0 - a_in) * 60.0;
        let font = theme.body(28.0);
        let s = paint::text_size(p, &t.text, font.clone());
        let r = Rect::from_min_size(pos2(safe.right() - s.x - 76.0 + slide, y), vec2(s.x + 76.0, s.y + 22.0));
        paint::ink_panel(p, r, theme, a);
        p.rect_filled(Rect::from_min_size(r.min + vec2(6.0, 8.0), vec2(5.0, r.height() - 16.0)), 2.0, alpha(theme.gold, a));
        paint::icon(p, "star", pos2(r.left() + 34.0, r.center().y), 26.0, alpha(theme.gold, a), theme);
        p.text(pos2(r.left() + 56.0, r.center().y), Align2::LEFT_CENTER, &t.text, font, alpha(theme.parchment_light, a));
        y = r.bottom() + 10.0;
    }
}

// ---------------------------------------------------------------- bottom right

fn prompts(p: &Painter, safe: Rect, list: &[Prompt], env: &HudEnv) {
    let theme = env.theme;
    let gh = 48.0;
    let mut y = safe.bottom() - gh * 0.5;
    for pr in list.iter().take(3) {
        let g = glyph::glyph_for_action(env.bindings, &pr.action, env.family);
        let font = theme.bold(30.0);
        let mut pieces = vec![Piece::Glyph(g.clone())];
        if pr.hold {
            pieces.push(Piece::Text("Hold".into()));
        }
        pieces.push(Piece::Text(pr.label.clone()));
        let w = glyph::row_width(p, &pieces, &font, gh, env.family, theme);
        let bg =
            Rect::from_min_max(pos2(safe.right() - w - 28.0, y - gh * 0.5 - 8.0), pos2(safe.right() + 14.0, y + gh * 0.5 + 8.0));
        let hc = env.settings.accessibility.high_contrast;
        p.rect_filled(bg, bg.height() * 0.5, alpha(theme.ink, if hc { 0.92 } else { 0.6 }));
        let x0 = safe.right() - w;
        let gs = glyph::size(p, &g, gh, env.family, theme);
        let gc = pos2(x0 + gs.x * 0.5, y);
        glyph::paint(p, gc, gh, &g, env.family, theme);
        if pr.hold {
            let r = gs.max_elem() * 0.5 + 6.0;
            p.circle_stroke(gc, r, Stroke::new(3.0, alpha(theme.amber_bright, 0.5)));
            let sweep = (env.time * 0.8).fract();
            paint::arc_band(p, gc, r, 3.5, 0.0, sweep * TAU, theme.amber_bright);
        }
        let mut x = x0 + gs.x + font.size * 0.3;
        if pr.hold {
            let r = p.text(pos2(x, y), Align2::LEFT_CENTER, "Hold", font.clone(), theme.amber_bright);
            x = r.right() + font.size * 0.3;
        }
        p.text(pos2(x, y), Align2::LEFT_CENTER, &pr.label, font, theme.parchment_light);
        y -= gh + 26.0;
    }
}

// ---------------------------------------------------------------- bottom left

fn bottom_left(p: &Painter, safe: Rect, items: &[HudItem], env: &HudEnv) {
    let theme = env.theme;
    let mut ship_drawn = false;
    for it in items {
        if let HudItem::Ship { hull, sails: _, wind_rel, wind, sail, knots, weapons } = it {
            ship_widget(p, safe, *hull, *wind_rel, *wind, *sail, *knots, weapons, env);
            ship_drawn = true;
        }
    }
    for it in items {
        if let HudItem::Tool { name, count } = it {
            let y = if ship_drawn { safe.bottom() - 372.0 } else { safe.bottom() - 40.0 };
            let c = pos2(safe.left() + 40.0, y);
            p.circle_filled(c, 40.0, alpha(theme.ink, 0.7));
            p.circle_stroke(c, 37.0, Stroke::new(2.5, alpha(theme.gold, 0.8)));
            let icon = tool_icon(name);
            paint::icon(p, icon, c, 46.0, theme.parchment_light, theme);
            paint::text_shadowed(
                p,
                pos2(c.x + 56.0, c.y - 14.0),
                Align2::LEFT_CENTER,
                name,
                theme.bold(30.0),
                theme.parchment_light,
                0.8,
            );
            paint::text_shadowed(
                p,
                pos2(c.x + 56.0, c.y + 18.0),
                Align2::LEFT_CENTER,
                &format!("× {count}"),
                theme.bold(28.0),
                theme.gold,
                0.8,
            );
        }
    }
}

fn tool_icon(name: &str) -> &'static str {
    let n = name.to_ascii_lowercase();
    if n.contains("smoke") {
        "smoke"
    } else if n.contains("pistol") || n.contains("shot") {
        "pistol"
    } else if n.contains("dart") || n.contains("knife") || n.contains("blade") {
        "dart"
    } else if n.contains("rope") {
        "rope"
    } else if n.contains("keg") {
        "kegs"
    } else if n.contains("grog") || n.contains("bottle") {
        "grog"
    } else {
        "star"
    }
}

fn weapon_arc(name: &str) -> Option<(f32, f32, f32)> {
    // (centre angle, half width, radius factor)
    let n = name.to_ascii_lowercase();
    Some(if n.contains("port") {
        (-PI * 0.5, 0.75, 1.0)
    } else if n.contains("star") {
        (PI * 0.5, 0.75, 1.0)
    } else if n.contains("chain") {
        (0.0, 0.32, 1.0)
    } else if n.contains("keg") || n.contains("mortar") || n.contains("stern") {
        (PI, 0.32, 1.0)
    } else if n.contains("swivel") {
        (0.0, 0.28, 0.72)
    } else {
        return None;
    })
}

fn ship_widget(
    p: &Painter,
    safe: Rect,
    hull: f32,
    wind_rel: f32,
    wind: f32,
    sail: u8,
    knots: f32,
    weapons: &[WeaponPip],
    env: &HudEnv,
) {
    let theme = env.theme;
    let r = 118.0;
    let c = pos2(safe.left() + r + 6.0, safe.bottom() - r - 52.0);
    // The rose.
    p.circle_filled(c, r + 10.0, alpha(theme.ink, if env.settings.accessibility.high_contrast { 0.92 } else { 0.55 }));
    p.circle_stroke(c, r + 6.0, Stroke::new(2.0, alpha(theme.gold, 0.6)));
    for k in 0..32 {
        let a = k as f32 * TAU / 32.0;
        let len = if k % 8 == 0 {
            14.0
        } else if k % 2 == 0 {
            8.0
        } else {
            4.0
        };
        p.line_segment(
            [paint::polar(c, r + 4.0, a), paint::polar(c, r + 4.0 - len, a)],
            Stroke::new(1.5, alpha(theme.parchment, 0.6)),
        );
    }
    // Weapon arcs around the ship.
    let wr = 74.0;
    let mut unknown = 0;
    for w in weapons {
        let (a0, half, rf) = weapon_arc(&w.name).unwrap_or_else(|| {
            unknown += 1;
            (PI * 0.25 + unknown as f32 * 0.5, 0.2, 0.72)
        });
        let rr = wr * rf;
        let width = if w.active { 13.0 } else { 9.0 };
        paint::arc_band(p, c, rr, width + 3.0, a0 - half, a0 + half, alpha(Color32::BLACK, 0.5));
        paint::arc_band(p, c, rr, width, a0 - half, a0 + half, alpha(theme.parchment_dark, 0.35));
        let ready = w.ready.clamp(0.0, 1.0);
        let col = if ready >= 1.0 { if w.active { theme.amber_bright } else { theme.gold } } else { alpha(theme.parchment, 0.7) };
        if ready > 0.0 {
            paint::arc_band(p, c, rr, width, a0 - half, a0 - half + 2.0 * half * ready, col);
        }
        if w.active {
            let tip = paint::polar(c, rr + 22.0, a0);
            paint::arrow_head(p, tip, 18.0, a0, theme.amber_bright);
        }
    }
    // The ship, bow up (top view).
    let hull_pts: Vec<Pos2> = [(0.0, -0.5), (0.2, -0.22), (0.22, 0.2), (0.14, 0.46), (-0.14, 0.46), (-0.22, 0.2), (-0.2, -0.22)]
        .iter()
        .map(|&(x, y)| c + vec2(x, y) * 92.0)
        .collect();
    paint::fill_convex(p, hull_pts.clone(), theme.wood);
    p.add(PathShape::closed_line(hull_pts, Stroke::new(2.5, theme.ink)));
    // Masts and sails: yards that belly forward with the sail level.
    let bulge = [0.0, 0.05, 0.11][sail.min(2) as usize];
    for (k, y) in [-0.22f32, 0.02, 0.24].iter().enumerate() {
        let half = if k == 0 { 0.17 } else { 0.21 };
        let pts: Vec<Pos2> = (0..=12)
            .map(|i| {
                let u = i as f32 / 12.0 * 2.0 - 1.0;
                c + vec2(u * half, y - bulge * (1.0 - u * u)) * 92.0
            })
            .collect();
        if sail > 0 {
            p.add(PathShape::line(pts.clone(), PathStroke::new(9.0, alpha(theme.ink, 0.6))));
            p.add(PathShape::line(pts, PathStroke::new(6.0, theme.parchment_light)));
        } else {
            p.add(PathShape::line(pts, PathStroke::new(3.0, theme.parchment_dark)));
        }
        p.circle_filled(c + vec2(0.0, *y) * 92.0, 4.5, theme.ink);
    }
    // Wind: an arrow from where it blows, toward the centre.
    let wa = wind_rel.to_radians();
    let len = 26.0 + 30.0 * wind.clamp(0.0, 1.0);
    let from = paint::polar(c, r - 4.0, wa);
    let to = paint::polar(c, r - 4.0 - len, wa);
    let wind_col = mix(theme.parchment_light, Color32::from_rgb(140, 220, 230), 0.5);
    p.line_segment([from, to], Stroke::new(8.0, alpha(Color32::BLACK, 0.5)));
    p.line_segment([from, to], Stroke::new(5.0, wind_col));
    paint::arrow_head(p, to, 22.0, wa + PI, wind_col);
    // Sail pips, speed, hull.
    let tx = c.x + r + 30.0;
    let label = ["Furled", "Half Sail", "Full Sail"][sail.min(2) as usize];
    for i in 0..2 {
        let q = Rect::from_center_size(pos2(tx + 10.0 + i as f32 * 26.0, c.y - 44.0), vec2(20.0, 28.0));
        let lit = (i as u8) < sail;
        p.rect_filled(q, 4.0, if lit { theme.parchment_light } else { alpha(theme.ink, 0.7) });
        p.rect_stroke(q, 4.0, Stroke::new(2.0, theme.parchment), StrokeKind::Inside);
    }
    paint::text_shadowed(
        p,
        pos2(tx + 66.0, c.y - 44.0),
        Align2::LEFT_CENTER,
        label,
        theme.bold(28.0),
        theme.parchment_light,
        0.8,
    );
    paint::text_shadowed(
        p,
        pos2(tx, c.y + 2.0),
        Align2::LEFT_CENTER,
        &format!("{knots:.1} kn"),
        theme.bold(34.0),
        theme.gold,
        0.8,
    );
    let bar = Rect::from_min_size(pos2(tx + 62.0, c.y + 58.0), vec2(200.0, 16.0));
    paint::text_shadowed(p, pos2(tx, bar.center().y), Align2::LEFT_CENTER, "Hull", theme.body(26.0), theme.parchment, 0.8);
    p.rect_filled(bar.expand(3.0), 4.0, alpha(Color32::BLACK, 0.6));
    let hcol = if hull < 0.3 { theme.danger(env.settings.accessibility.colorblind) } else { theme.parchment_light };
    p.rect_filled(Rect::from_min_size(bar.min, vec2(bar.width() * hull.clamp(0.0, 1.0), bar.height())), 2.0, hcol);
    p.text(
        bar.right_center() + vec2(12.0, 0.0),
        Align2::LEFT_CENTER,
        format!("{}%", (hull * 100.0).round() as i32),
        theme.bold(24.0),
        theme.parchment,
    );
}

// ---------------------------------------------------------------- bottom centre

/// Returns the top of the subtitle block.
fn subtitles(p: &Painter, safe: Rect, list: &[Subtitle], env: &HudEnv) -> f32 {
    let theme = env.theme;
    let acc = &env.settings.accessibility;
    let mut bottom = safe.bottom() - 20.0;
    if !acc.subtitles || list.is_empty() {
        return bottom;
    }
    let size = acc.subtitle_size.points();
    let max_w = (safe.width() * 0.62).min(1300.0);
    for s in list.iter().rev().take(2) {
        let mut job = egui::text::LayoutJob {
            wrap: egui::text::TextWrapping { max_width: max_w, ..Default::default() },
            ..Default::default()
        };
        job.halign = egui::Align::Center;
        if acc.speaker_names && !s.speaker.is_empty() {
            let col = hex(&s.color).unwrap_or(theme.gold);
            job.append(
                &format!("{}: ", s.speaker),
                0.0,
                egui::TextFormat { font_id: theme.bold(size), color: col, ..Default::default() },
            );
        }
        job.append(
            &s.text,
            0.0,
            egui::TextFormat { font_id: theme.body(size), color: theme.parchment_light, ..Default::default() },
        );
        let galley = p.layout_job(job);
        let sz = galley.size();
        let box_r = Rect::from_center_size(pos2(safe.center().x, bottom - sz.y * 0.5 - 12.0), sz + vec2(40.0, 24.0));
        p.rect_filled(box_r, 10.0, alpha(Color32::BLACK, if acc.high_contrast { 0.92 } else { 0.68 }));
        // With halign Center the galley's rect is centred on its pos.x.
        p.galley(pos2(safe.center().x, box_r.top() + 12.0), galley, theme.parchment_light);
        bottom = box_r.top() - 10.0;
    }
    bottom
}

fn bars(p: &Painter, safe: Rect, bottom: f32, items: &[HudItem], env: &HudEnv) {
    let theme = env.theme;
    let mut y = bottom - 16.0;
    for it in items.iter().rev() {
        if let HudItem::Bar { label, frac, color } = it {
            let col = hex(color).unwrap_or(theme.red);
            let w = 560.0;
            let bar = Rect::from_center_size(pos2(safe.center().x, y - 10.0), vec2(w, 18.0));
            p.rect_filled(bar.expand(4.0), 5.0, alpha(Color32::BLACK, 0.65));
            p.rect_stroke(bar.expand(4.0), 5.0, Stroke::new(1.5, alpha(theme.gold, 0.6)), StrokeKind::Inside);
            p.rect_filled(Rect::from_min_size(bar.min, vec2(w * frac.clamp(0.0, 1.0), bar.height())), 3.0, col);
            for k in 1..10 {
                let x = bar.left() + w * k as f32 / 10.0;
                p.vline(x, bar.y_range(), Stroke::new(1.0, alpha(Color32::BLACK, 0.35)));
            }
            paint::text_shadowed(
                p,
                pos2(bar.center().x, bar.top() - 8.0),
                Align2::CENTER_BOTTOM,
                label,
                theme.bold(28.0),
                theme.parchment_light,
                0.85,
            );
            y = bar.top() - 52.0;
        }
    }
}

// ---------------------------------------------------------------- in the world

/// Where an off-screen pip goes: on an ellipse around the screen centre (clear of the HUD's
/// corners and the subtitles), pointing toward the target.
fn edge_point(scr: Rect, safe: Rect, dir: Vec2) -> (Pos2, f32) {
    let c = scr.center();
    let d = if dir.length() < 1e-4 { vec2(0.0, 1.0) } else { dir.normalized() };
    let (rx, ry) = (safe.width() * 0.40, scr.height() * 0.27);
    let k = 1.0 / ((d.x / rx).powi(2) + (d.y / ry).powi(2)).sqrt();
    let q = c + d * k;
    let ang = d.x.atan2(-d.y);
    (q, ang)
}

fn marker(p: &Painter, scr: Rect, safe: Rect, m: &WorldMarker, env: &HudEnv) {
    let theme = env.theme;
    let acc = &env.settings.accessibility;
    let col = theme.marker_color(m.kind, acc.colorblind, acc.high_contrast);
    let on = env.proj.project(m.pos).filter(|q| safe.contains(*q));
    let dist = env.proj.distance_hint(m.pos);
    match on {
        Some(q) => {
            match m.kind {
                MarkerKind::Interact => {
                    let g = glyph::glyph_for_action(env.bindings, "interact", env.family);
                    glyph::paint(p, q, 40.0, &g, env.family, theme);
                    if !m.label.is_empty() {
                        paint::text_shadowed(
                            p,
                            q + vec2(0.0, 28.0),
                            Align2::CENTER_TOP,
                            &m.label,
                            theme.bold(26.0),
                            theme.parchment_light,
                            0.9,
                        );
                    }
                }
                MarkerKind::Enemy | MarkerKind::Ally => {
                    marker_icon(p, q, 24.0, m.kind, col, theme);
                    if !m.label.is_empty() {
                        paint::text_shadowed(
                            p,
                            q + vec2(0.0, -20.0),
                            Align2::CENTER_BOTTOM,
                            &m.label,
                            theme.bold(24.0),
                            theme.parchment_light,
                            0.9,
                        );
                    }
                }
                _ => {
                    marker_icon(p, q, 34.0, m.kind, col, theme);
                    let mut y = q.y + 24.0;
                    if m.distance {
                        if let Some(d) = dist {
                            let r = paint::text_shadowed(
                                p,
                                pos2(q.x, y),
                                Align2::CENTER_TOP,
                                &distance(d),
                                theme.bold(24.0),
                                theme.parchment_light,
                                0.9,
                            );
                            y = r.bottom();
                        }
                    }
                    if !m.label.is_empty() && m.kind != MarkerKind::Collectible {
                        paint::text_shadowed(
                            p,
                            pos2(q.x, y),
                            Align2::CENTER_TOP,
                            &m.label,
                            theme.body(24.0),
                            theme.parchment,
                            0.9,
                        );
                    }
                }
            }
            if let Some(b) = m.bar {
                let bar = Rect::from_center_size(
                    q + vec2(0.0, if m.kind == MarkerKind::Interact { 64.0 } else { 26.0 }),
                    vec2(110.0, 10.0),
                );
                p.rect_filled(bar.expand(2.5), 3.0, alpha(Color32::BLACK, 0.7));
                p.rect_filled(
                    Rect::from_min_size(bar.min, vec2(bar.width() * b.clamp(0.0, 1.0), bar.height())),
                    2.0,
                    theme.danger(acc.colorblind),
                );
            }
        }
        None => {
            // Off screen: objective-like markers stick to the safe-area edge with an arrow.
            if !matches!(m.kind, MarkerKind::Objective | MarkerKind::Waypoint | MarkerKind::Target) {
                return;
            }
            let Some(dir) = env.proj.screen_direction(m.pos) else { return };
            let (q, ang) = edge_point(scr, safe, dir);
            marker_icon(p, q, 30.0, m.kind, col, theme);
            paint::arrow_head(p, paint::polar(q, 30.0, ang), 18.0, ang, col);
        }
    }
}

fn awareness(p: &Painter, scr: Rect, safe: Rect, a: &AwarenessArc, env: &HudEnv) {
    let theme = env.theme;
    let cb = env.settings.accessibility.colorblind;
    let (col, sym) = match a.state {
        Awareness::Unaware => (theme.parchment_light, ""),
        Awareness::Suspicious | Awareness::Investigating | Awareness::Searching => (theme.warning(cb), "?"),
        Awareness::Alerted | Awareness::Combat => (theme.danger(cb), "!"),
    };
    if a.state == Awareness::Unaware && a.fill <= 0.01 {
        return;
    }
    match env.proj.project(a.pos).filter(|q| scr.contains(*q)) {
        Some(q) => {
            // An arc over the head that fills, then the symbol.
            let r = 26.0;
            let (a0, a1) = (-1.25f32, 1.25f32);
            paint::arc_band(p, q, r, 9.0, a0, a1, alpha(Color32::BLACK, 0.55));
            let f = if sym.is_empty() { a.fill.clamp(0.0, 1.0) } else { 1.0 };
            let half = (a1 - a0) * 0.5 * f;
            if f > 0.0 {
                paint::arc_band(p, q, r, 6.0, -half, half, col);
            }
            if !sym.is_empty() {
                let pulse = if a.state == Awareness::Searching { 0.6 + 0.4 * (env.time * 6.0).sin().abs() } else { 1.0 };
                paint::text_outlined(
                    p,
                    q + vec2(0.0, -4.0),
                    Align2::CENTER_CENTER,
                    sym,
                    theme.bold(44.0),
                    alpha(col, pulse),
                    alpha(Color32::BLACK, 0.85),
                    3.0,
                );
            }
        }
        None => {
            let Some(dir) = env.proj.screen_direction(a.pos) else { return };
            let (q, ang) = edge_point(scr, safe, dir);
            let pts = vec![paint::polar(q, 22.0, ang), paint::polar(q, 16.0, ang + 2.4), paint::polar(q, 16.0, ang - 2.4)];
            p.add(egui::Shape::convex_polygon(pts, col, Stroke::new(2.0, alpha(Color32::BLACK, 0.8))));
            if !sym.is_empty() {
                paint::text_outlined(
                    p,
                    paint::polar(q, -8.0, ang),
                    Align2::CENTER_CENTER,
                    sym,
                    theme.bold(30.0),
                    col,
                    alpha(Color32::BLACK, 0.85),
                    2.5,
                );
            }
        }
    }
}

// ---------------------------------------------------------------- banners

fn banner(p: &Painter, scr: Rect, b: &Banner, env: &HudEnv) {
    let theme = env.theme;
    let a_in = (b.age / 0.35).clamp(0.0, 1.0);
    let a_out = 1.0 - ((b.age - 2.6) / 0.5).clamp(0.0, 1.0);
    let a = a_in.min(a_out);
    if a <= 0.0 {
        return;
    }
    let ease = 1.0 - (1.0 - a_in).powi(3);
    let col = hex(&b.color).unwrap_or(theme.gold);
    let c = pos2(scr.center().x, scr.top() + scr.height() * 0.34);
    let band = Rect::from_center_size(c + vec2(0.0, 18.0), vec2(scr.width() * (0.4 + 0.6 * ease), 230.0));
    let dark = alpha(Color32::BLACK, 0.55 * a);
    paint::hgradient(p, Rect::from_min_max(band.min, pos2(band.center().x, band.bottom())), Color32::TRANSPARENT, dark);
    paint::hgradient(p, Rect::from_min_max(pos2(band.center().x, band.top()), band.max), dark, Color32::TRANSPARENT);
    let size = quantize(env.theme.sizes.banner);
    let r = paint::text_spaced(
        p,
        c,
        Align2::CENTER_CENTER,
        &b.text,
        theme.display(size),
        alpha(col, a),
        6.0,
        Some((alpha(theme.ink, 0.95 * a), 5.0)),
    );
    let w = r.width() * 0.5 * ease;
    for side in [-1.0f32, 1.0] {
        let y = r.bottom() + 8.0;
        p.line_segment([pos2(c.x, y), pos2(c.x + side * w, y)], Stroke::new(3.0, alpha(col, a)));
    }
    paint::diamond(p, pos2(c.x, r.bottom() + 8.0), 18.0, alpha(col, a), Stroke::new(2.0, alpha(theme.ink, a)));
    if !b.sub.is_empty() {
        paint::text_shadowed(
            p,
            pos2(c.x, r.bottom() + 26.0),
            Align2::CENTER_TOP,
            &b.sub,
            theme.body(36.0),
            alpha(theme.parchment_light, a),
            0.9 * a,
        );
    }
}

// ---------------------------------------------------------------- overlays

fn overlay(p: &Painter, scr: Rect, safe: Rect, o: &Overlay, ui: &UiModel, env: &HudEnv) {
    let theme = env.theme;
    match o {
        Overlay::Tiles { title, subtitle, tiles: list, focus } => {
            p.rect_filled(scr, 0.0, alpha(Color32::BLACK, 0.62));
            let tr = paint::text_outlined(
                p,
                pos2(scr.center().x, safe.top() + 40.0),
                Align2::CENTER_TOP,
                title,
                theme.display(theme.sizes.title),
                theme.parchment,
                alpha(theme.ink, 0.9),
                4.0,
            );
            if !subtitle.is_empty() {
                paint::text_shadowed(
                    p,
                    pos2(scr.center().x, tr.bottom() + 6.0),
                    Align2::CENTER_TOP,
                    subtitle,
                    theme.body(32.0),
                    theme.parchment_dark,
                    0.8,
                );
            }
            tile_row(p, pos2(scr.center().x, scr.center().y + 30.0), list, *focus, env, 1.0);
            hint_bar(
                p,
                scr,
                safe,
                &nav_hints(&[(Nav::Left, "Choose"), (Nav::Confirm, "Take"), (Nav::Back, "Leave")], env.family),
                env.family,
                theme,
            );
        }
        Overlay::Wheel { slots, selected } => wheel(p, scr, safe, slots, *selected, env),
        Overlay::Results(r) => results(p, scr, safe, r, env),
        Overlay::Map { cursor, zoom } => {
            p.rect_filled(scr, 0.0, alpha(Color32::BLACK, 0.7));
            let view = MapView { center: *cursor, zoom: *zoom, legend: false };
            map::draw(
                p,
                safe.shrink2(vec2(0.0, 30.0)).translate(vec2(0.0, -20.0)),
                ui.map.as_ref(),
                &ui.map_icons,
                &view,
                theme,
                Some(*cursor),
            );
            hint_bar(p, scr, safe, &map_hints(env.family, false, "Close"), env.family, theme);
        }
        Overlay::Cinematic { skip } => {
            let g = glyph::glyph_for_action(env.bindings, "drop", env.family);
            let font = theme.bold(28.0);
            let gh = 40.0;
            let c = pos2(safe.right() - 40.0, scr.bottom() - scr.height() * 0.055);
            let label = "Hold to skip";
            let tw = paint::text_size(p, label, font.clone()).x;
            paint::text_shadowed(
                p,
                pos2(c.x - 40.0, c.y),
                Align2::RIGHT_CENTER,
                label,
                font,
                alpha(theme.parchment_light, 0.9),
                0.8,
            );
            let _ = tw;
            glyph::paint(p, c, gh, &g, env.family, theme);
            p.circle_stroke(c, 30.0, Stroke::new(4.0, alpha(theme.parchment, 0.3)));
            paint::arc_band(p, c, 30.0, 5.0, 0.0, skip.clamp(0.0, 1.0) * TAU, theme.amber_bright);
        }
    }
}

/// A centred row of 2–4 tiles; returns their rects.
pub fn tile_row(p: &Painter, center: Pos2, list: &[Tile], focus: usize, env: &HudEnv, opacity: f32) -> Vec<Rect> {
    let theme = env.theme;
    let n = list.len().clamp(1, 4);
    let (w, h) = (360.0, 450.0);
    let gap = 48.0;
    let total = n as f32 * w + (n - 1) as f32 * gap;
    let mut rects = Vec::new();
    for (i, t) in list.iter().take(4).enumerate() {
        let x = center.x - total * 0.5 + i as f32 * (w + gap) + w * 0.5;
        let focused = i == focus;
        let s = if focused { theme.focus_scale } else { 1.0 };
        let r = Rect::from_center_size(pos2(x, center.y), vec2(w, h) * s);
        rects.push(r);
        tile(p, r, t, focused, env, i as u32, opacity);
    }
    rects
}

fn tile(p: &Painter, r: Rect, t: &Tile, focused: bool, env: &HudEnv, seed: u32, _opacity: f32) {
    let theme = env.theme;
    if focused {
        paint::glow(p, r.expand(10.0), 20.0, 40.0, alpha(theme.amber_bright, 0.7));
    }
    paint::parchment(p, r, theme, 0x71 + seed * 13);
    paint::rope_frame(p, r, 14.0, if focused { 8.0 } else { 6.0 }, theme);
    let ink = if t.disabled { alpha(theme.ink, 0.45) } else { theme.ink };
    let ic = pos2(r.center().x, r.top() + r.height() * 0.28);
    p.circle_filled(ic, 78.0, alpha(theme.ink, if t.disabled { 0.25 } else { 0.85 }));
    p.circle_stroke(ic, 72.0, Stroke::new(3.0, alpha(theme.gold, 0.8)));
    paint::icon(p, &t.icon, ic, 96.0, if t.disabled { theme.parchment_dark } else { theme.parchment_light }, theme);
    let name_y = r.top() + r.height() * 0.53;
    let name_r = paint::text_wrapped(
        p,
        pos2(r.left() + 34.0, name_y),
        r.width() - 68.0,
        &t.name,
        theme.display(quantize(46.0 * r.width() / 360.0).min(52.0)),
        ink,
    );
    let _ = name_r;
    let desc_y = name_y + 64.0;
    paint::text_wrapped(
        p,
        pos2(r.left() + 34.0, desc_y),
        r.width() - 68.0,
        &t.desc,
        theme.body(28.0),
        if t.disabled { alpha(theme.ink_soft, 0.5) } else { theme.ink_soft },
    );
    if !t.cost.is_empty() {
        let cr = paint::text(
            p,
            pos2(r.right() - 36.0, r.bottom() - 40.0),
            Align2::RIGHT_CENTER,
            &t.cost,
            theme.bold(30.0),
            if t.disabled { theme.red } else { theme.ink },
        );
        paint::icon(p, "coin", pos2(cr.left() - 22.0, cr.center().y), 30.0, theme.gold, theme);
    }
    if focused {
        let bar = Rect::from_min_size(pos2(r.left() + 30.0, r.bottom() - 18.0), vec2(r.width() - 60.0, 7.0));
        p.rect_filled(bar, 3.0, theme.amber);
    }
}

fn wheel(p: &Painter, scr: Rect, safe: Rect, slots: &[Tile], selected: usize, env: &HudEnv) {
    let theme = env.theme;
    let c = scr.center();
    paint::vignette(p, scr, 0.2, alpha(Color32::BLACK, 0.65));
    let r_out = 320.0;
    let r_in = 128.0;
    let n = slots.len().clamp(1, 8);
    let step = TAU / n as f32;
    for (i, t) in slots.iter().take(n).enumerate() {
        let a = i as f32 * step;
        let sel = i == selected;
        let a0 = a - step * 0.5 + 0.03;
        let a1 = a + step * 0.5 - 0.03;
        let outer = if sel { r_out + 20.0 } else { r_out };
        let mut pts = paint::arc_points(c, outer, a0, a1, 18);
        pts.extend(paint::arc_points(c, r_in, a1, a0, 10));
        let fill = if sel { theme.amber } else { alpha(theme.ink, 0.95) };
        paint::fill_poly(p, &pts, fill);
        p.add(PathShape::closed_line(
            pts,
            Stroke::new(if sel { 4.0 } else { 2.0 }, if sel { theme.amber_bright } else { alpha(theme.gold, 0.6) }),
        ));
        // Icon, name and count stacked upright at the slot's middle.
        let mid = paint::polar(c, (r_in + outer) * 0.5 + 4.0, a);
        let col = if t.disabled {
            theme.parchment_dark
        } else if sel {
            theme.ink
        } else {
            theme.parchment_light
        };
        paint::icon(p, &t.icon, mid - vec2(0.0, 30.0), if sel { 76.0 } else { 64.0 }, col, theme);
        let tc = if sel { theme.ink } else { theme.parchment };
        p.text(mid + vec2(0.0, 28.0), Align2::CENTER_CENTER, &t.name, theme.bold(26.0), tc);
        if !t.cost.is_empty() {
            p.text(
                mid + vec2(0.0, 56.0),
                Align2::CENTER_CENTER,
                &t.cost,
                theme.bold(24.0),
                if sel { theme.ink } else { theme.gold },
            );
        }
    }
    p.circle_filled(c, r_in - 10.0, alpha(theme.ink, 0.92));
    p.circle_stroke(c, r_in - 14.0, Stroke::new(3.0, theme.gold));
    if let Some(t) = slots.get(selected) {
        p.text(c + vec2(0.0, -14.0), Align2::CENTER_CENTER, &t.name, theme.display(40.0), theme.parchment_light);
        if !t.desc.is_empty() {
            paint::text_wrapped(p, c + vec2(-90.0, 14.0), 180.0, &t.desc, theme.body(24.0), theme.parchment_dark);
        }
    }
    let choose = if env.family == GlyphFamily::Keyboard { Glyph::MouseMove } else { Glyph::RightStick };
    let release = glyph::glyph_for_action(env.bindings, "tools", env.family);
    hint_bar(p, scr, safe, &[Hint::new(vec![choose], "Choose"), Hint::new(vec![release], "Release")], env.family, theme);
}

fn results(p: &Painter, scr: Rect, safe: Rect, r: &Results, env: &HudEnv) {
    let theme = env.theme;
    p.rect_filled(scr, 0.0, alpha(Color32::BLACK, 0.7));
    let panel = Rect::from_min_max(
        pos2(safe.left() + safe.width() * 0.06, safe.top()),
        pos2(safe.right() - safe.width() * 0.06, safe.bottom() - 168.0),
    );
    paint::panel(p, panel, theme, 0x9e5);
    let title_r = p.text(
        pos2(panel.center().x, panel.top() + 42.0),
        Align2::CENTER_TOP,
        &r.title,
        theme.display(theme.sizes.title),
        theme.ink,
    );
    p.hline(
        (title_r.left() - 40.0)..=(title_r.right() + 40.0),
        title_r.bottom() + 6.0,
        Stroke::new(2.0, alpha(theme.rope_dark, 0.6)),
    );
    // Pour % ring (left).
    let rc = pos2(panel.left() + panel.width() * 0.2, panel.top() + 370.0);
    let rr = 140.0;
    p.circle_filled(rc, rr + 24.0, alpha(theme.ink, 0.9));
    paint::ring(p, rc, rr, 22.0, r.pour / 100.0, theme.amber_bright, alpha(theme.parchment_dark, 0.35));
    p.text(
        rc + vec2(0.0, -10.0),
        Align2::CENTER_CENTER,
        format!("{}%", r.pour.round() as i32),
        theme.display(96.0),
        theme.parchment_light,
    );
    p.text(rc + vec2(0.0, 58.0), Align2::CENTER_CENTER, "POUR", theme.bold(28.0), theme.gold);
    if r.pour >= 100.0 {
        p.text(rc + vec2(0.0, rr + 50.0), Align2::CENTER_TOP, "Full Pour!", theme.display(48.0), theme.amber);
    }
    // Checklist (middle).
    let cx = panel.left() + panel.width() * 0.38;
    let mut y = panel.top() + 190.0;
    p.text(pos2(cx, y), Align2::LEFT_TOP, "Full Pour", theme.display(48.0), theme.ink);
    y += 70.0;
    for (text, done) in r.checklist.iter().take(6) {
        let bx = Rect::from_min_size(pos2(cx, y + 4.0), Vec2::splat(30.0));
        p.rect_stroke(bx, 4.0, Stroke::new(3.0, theme.ink), StrokeKind::Inside);
        if *done {
            paint::tick(p, bx.center() + vec2(3.0, -3.0), 40.0, theme.green);
        }
        p.text(
            pos2(cx + 48.0, y + 19.0),
            Align2::LEFT_CENTER,
            text,
            theme.body(30.0),
            if *done { theme.ink } else { theme.ink_soft },
        );
        y += 52.0;
    }
    // Stats (right).
    let sx = panel.left() + panel.width() * 0.7;
    let sw = panel.right() - 70.0 - sx;
    let mut y = panel.top() + 210.0;
    for (label, value) in r.stats.iter().take(7) {
        p.text(pos2(sx, y + 18.0), Align2::LEFT_CENTER, label, theme.body(30.0), theme.ink_soft);
        p.text(pos2(sx + sw, y + 18.0), Align2::RIGHT_CENTER, value, theme.bold(32.0), theme.ink);
        p.hline(sx..=sx + sw, y + 42.0, Stroke::new(1.0, alpha(theme.rope_dark, 0.35)));
        y += 52.0;
    }
    // Choices.
    if !r.tiles.is_empty() {
        choice_buttons(p, pos2(panel.center().x, panel.bottom() - 84.0), &r.tiles, r.focus, env);
    }
    hint_bar(p, scr, safe, &nav_hints(&[(Nav::Left, "Choose"), (Nav::Confirm, "Select")], env.family), env.family, theme);
}

/// Wide buttons in a row (results' choices).
fn choice_buttons(p: &Painter, center: Pos2, list: &[Tile], focus: usize, env: &HudEnv) {
    let theme = env.theme;
    let n = list.len().clamp(1, 4);
    let (w, h) = (400.0, 84.0);
    let gap = 36.0;
    let total = n as f32 * w + (n - 1) as f32 * gap;
    for (i, t) in list.iter().take(4).enumerate() {
        let x = center.x - total * 0.5 + i as f32 * (w + gap) + w * 0.5;
        let focused = i == focus;
        let r = Rect::from_center_size(pos2(x, center.y), vec2(w, h) * if focused { theme.focus_scale } else { 1.0 });
        button(p, r, &t.name, focused, theme, i as u32);
    }
}

/// A big button: ink on darker parchment with a rope edge; focused = amber glow and fill.
pub fn button(p: &Painter, r: Rect, label: &str, focused: bool, theme: &Theme, seed: u32) {
    let _ = seed;
    button_sized(p, r, label, focused, theme, if focused { 36.0 } else { 34.0 });
}

/// [`button`] with a given text size (touch screens want bigger text).
pub fn button_sized(p: &Painter, r: Rect, label: &str, focused: bool, theme: &Theme, text: f32) {
    if focused {
        paint::glow(p, r.expand(6.0), 16.0, 30.0, alpha(theme.amber_bright, 0.75));
    }
    paint::drop_shadow(p, r, 12.0, vec2(0.0, 5.0), 12.0, 0.45);
    let fill = if focused { theme.amber } else { theme.parchment_dark };
    p.rect_filled(r, 12.0, fill);
    let pts = paint::rounded_rect_points(r.shrink(7.0), 9.0, 4);
    paint::rope(p, &pts, true, if focused { 6.0 } else { 5.0 }, theme);
    p.text(r.center(), Align2::CENTER_CENTER, label, theme.bold(quantize(text)), theme.ink);
}

// ---------------------------------------------------------------- hint bar

/// One entry of the hint bar: glyphs and a label.
#[derive(Clone, Debug, PartialEq)]
pub struct Hint {
    pub glyphs: Vec<Glyph>,
    pub label: String,
}

impl Hint {
    /// The glyphs of a menu action (L1/R1 shows both shoulder buttons).
    pub fn nav(nav: Nav, label: &str, family: GlyphFamily) -> Hint {
        let glyphs = match nav {
            Nav::TabPrev | Nav::TabNext => vec![glyph::nav_glyph(Nav::TabPrev, family), glyph::nav_glyph(Nav::TabNext, family)],
            n => vec![glyph::nav_glyph(n, family)],
        };
        Hint { glyphs, label: label.to_string() }
    }

    pub fn new(glyphs: Vec<Glyph>, label: &str) -> Hint {
        Hint { glyphs, label: label.to_string() }
    }
}

/// The map's hints: pan with the left stick, zoom with the triggers, legend, back.
pub fn map_hints(family: GlyphFamily, legend: bool, back: &str) -> Vec<Hint> {
    use crate::bindings::PadButton;
    let kb = family == GlyphFamily::Keyboard;
    let pan = if kb { vec![Glyph::key("WASD")] } else { vec![Glyph::LeftStick] };
    let zoom = if kb {
        vec![Glyph::MouseWheel]
    } else {
        vec![Glyph::Pad(PadButton::LeftTrigger2), Glyph::Pad(PadButton::RightTrigger2)]
    };
    vec![
        Hint::new(pan, "Pan"),
        Hint::new(zoom, "Zoom"),
        Hint::nav(Nav::Context, if legend { "Hide Legend" } else { "Legend" }, family),
        Hint::nav(Nav::Back, back, family),
    ]
}

/// Hints from (menu action, label) pairs.
pub fn nav_hints(list: &[(Nav, &str)], family: GlyphFamily) -> Vec<Hint> {
    list.iter().map(|(n, l)| Hint::nav(*n, l, family)).collect()
}

/// The bottom hint bar ("✕ Select  ◯ Back  L1/R1 Tabs") with the device's glyphs,
/// right-aligned in the safe area over a soft dark band. Returns the bar's rect.
pub fn hint_bar(p: &Painter, scr: Rect, safe: Rect, hints: &[Hint], family: GlyphFamily, theme: &Theme) -> Rect {
    let gh = 46.0;
    let y = safe.bottom() - gh * 0.5;
    let band = Rect::from_min_max(pos2(scr.left(), y - gh * 0.5 - 30.0), scr.max);
    paint::vgradient(p, band, Color32::TRANSPARENT, alpha(Color32::BLACK, 0.6));
    let font = theme.body(theme.sizes.hint);
    let mut pieces = Vec::new();
    for (i, h) in hints.iter().enumerate() {
        if i > 0 {
            pieces.push(Piece::Gap(40.0));
        }
        for g in &h.glyphs {
            pieces.push(Piece::Glyph(g.clone()));
        }
        pieces.push(Piece::Text(h.label.clone()));
    }
    glyph::paint_row(p, pos2(safe.right(), y), Align2::RIGHT_CENTER, &pieces, font, theme.parchment_light, gh, family, theme)
}

/// A glyph by itself, for callers that only have the HUD module.
pub fn paint_glyph(p: &Painter, c: Pos2, h: f32, g: &Glyph, family: GlyphFamily, theme: &Theme) -> Rect {
    glyph::paint(p, c, h, g, family, theme)
}

#[allow(dead_code)]
fn _unused(_: PathStroke) {}
