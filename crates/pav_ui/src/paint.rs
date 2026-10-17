//! Vector painting helpers shared by the screens and the HUD: text with outlines, gradients,
//! torn parchment, rope, rings, ticks, the keg-tap emblem, the logo and the icon set. No
//! images: everything is egui shapes and meshes.
//!
//! Angles for arcs and icons are compass-style: radians, 0 = up, clockwise.

use std::f32::consts::{PI, TAU};

use egui::epaint::{Mesh, PathShape, PathStroke, Shadow};
use egui::{Align2, Color32, FontId, Painter, Pos2, Rect, Shape, Stroke, StrokeKind, Vec2, pos2, vec2};

use crate::theme::{Theme, alpha, mix};

// ---------------------------------------------------------------- noise

/// A hash of (i, seed) in 0..1.
pub fn hash(i: u32, seed: u32) -> f32 {
    let mut x = i.wrapping_mul(0x9e37_79b9) ^ seed.wrapping_mul(0x85eb_ca6b).rotate_left(13);
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^= x >> 16;
    (x & 0x00ff_ffff) as f32 / 16_777_216.0
}

/// Smooth 1-D value noise in 0..1.
pub fn noise(t: f32, seed: u32) -> f32 {
    let i = t.floor();
    let f = t - i;
    let a = hash(i as i32 as u32, seed);
    let b = hash((i as i32 + 1) as u32, seed);
    let s = f * f * (3.0 - 2.0 * f);
    a + (b - a) * s
}

/// A point on a circle, compass angle (0 = up, clockwise).
pub fn polar(c: Pos2, r: f32, a: f32) -> Pos2 {
    pos2(c.x + r * a.sin(), c.y - r * a.cos())
}

// ---------------------------------------------------------------- text

/// Lays out one line and paints it anchored at `pos`; returns its rect.
pub fn text(p: &Painter, pos: Pos2, align: Align2, s: &str, font: FontId, color: Color32) -> Rect {
    p.text(pos, align, s, font, color)
}

/// The size of one line of text.
pub fn text_size(p: &Painter, s: &str, font: FontId) -> Vec2 {
    p.layout_no_wrap(s.to_string(), font, Color32::WHITE).size()
}

/// Text with a dark outline (legible over the live 3D scene).
pub fn text_outlined(
    p: &Painter,
    pos: Pos2,
    align: Align2,
    s: &str,
    font: FontId,
    color: Color32,
    outline: Color32,
    width: f32,
) -> Rect {
    let galley = p.layout_no_wrap(s.to_string(), font, color);
    let rect = align.anchor_size(pos, galley.size());
    if outline.a() > 0 && width > 0.0 {
        let n = 12;
        for k in 0..n {
            let a = k as f32 / n as f32 * TAU;
            let o = vec2(a.cos(), a.sin()) * width;
            p.galley_with_override_text_color(rect.min + o, galley.clone(), outline);
        }
    }
    p.galley(rect.min, galley, color);
    rect
}

/// Text with a soft drop shadow.
pub fn text_shadowed(p: &Painter, pos: Pos2, align: Align2, s: &str, font: FontId, color: Color32, shadow: f32) -> Rect {
    let galley = p.layout_no_wrap(s.to_string(), font.clone(), color);
    let rect = align.anchor_size(pos, galley.size());
    let d = (font.size * 0.06).max(1.5);
    p.galley_with_override_text_color(rect.min + vec2(d, d * 1.2), galley.clone(), alpha(Color32::BLACK, shadow));
    p.galley(rect.min, galley, color);
    rect
}

/// Wrapped text with its top-left at `pos`, at most `width` wide; returns its rect.
pub fn text_wrapped(p: &Painter, pos: Pos2, width: f32, s: &str, font: FontId, color: Color32) -> Rect {
    let galley = p.layout(s.to_string(), font, color, width);
    let rect = Rect::from_min_size(pos, galley.size());
    p.galley(pos, galley, color);
    rect
}

/// The height of `s` wrapped at `width`.
pub fn wrapped_height(p: &Painter, s: &str, font: FontId, width: f32) -> f32 {
    p.layout(s.to_string(), font, Color32::WHITE, width).size().y
}

/// Text with extra space between letters (small caps-like headings).
pub fn text_spaced(
    p: &Painter,
    pos: Pos2,
    align: Align2,
    s: &str,
    font: FontId,
    color: Color32,
    spacing: f32,
    outline: Option<(Color32, f32)>,
) -> Rect {
    let mut job = egui::text::LayoutJob::default();
    job.append(s, 0.0, egui::TextFormat { font_id: font, color, extra_letter_spacing: spacing, ..Default::default() });
    let galley = p.layout_job(job);
    let rect = align.anchor_size(pos, galley.size());
    if let Some((oc, w)) = outline {
        for k in 0..12 {
            let a = k as f32 / 12.0 * TAU;
            p.galley_with_override_text_color(rect.min + vec2(a.cos(), a.sin()) * w, galley.clone(), oc);
        }
    }
    p.galley(rect.min, galley, color);
    rect
}

/// Rounds a font size so animated scales don't fill the font atlas with in-between sizes.
pub fn quantize(size: f32) -> f32 {
    (size * 0.5).round() * 2.0
}

// ---------------------------------------------------------------- fills

/// Triangulates a simple polygon (any winding, concave allowed) by ear clipping.
pub fn triangulate(pts: &[Pos2]) -> Vec<[u32; 3]> {
    let n = pts.len();
    if n < 3 {
        return Vec::new();
    }
    let area: f32 = (0..n)
        .map(|i| {
            let (a, b) = (pts[i], pts[(i + 1) % n]);
            a.x * b.y - b.x * a.y
        })
        .sum();
    let sign = if area >= 0.0 { 1.0 } else { -1.0 };
    let mut idx: Vec<u32> = (0..n as u32).collect();
    let mut out = Vec::with_capacity(n - 2);
    let cross = |a: Pos2, b: Pos2, c: Pos2| (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
    let mut guard = 0;
    while idx.len() > 3 && guard < n * n {
        guard += 1;
        let m = idx.len();
        let mut found = false;
        for i in 0..m {
            let (ia, ib, ic) = (idx[(i + m - 1) % m], idx[i], idx[(i + 1) % m]);
            let (a, b, c) = (pts[ia as usize], pts[ib as usize], pts[ic as usize]);
            if cross(a, b, c) * sign <= 1e-6 {
                continue; // reflex or flat
            }
            let inside = idx.iter().any(|&j| {
                if j == ia || j == ib || j == ic {
                    return false;
                }
                let q = pts[j as usize];
                cross(a, b, q) * sign > 0.0 && cross(b, c, q) * sign > 0.0 && cross(c, a, q) * sign > 0.0
            });
            if inside {
                continue;
            }
            out.push([ia, ib, ic]);
            idx.remove(i);
            found = true;
            break;
        }
        if !found {
            // Self-intersecting or degenerate: drop a vertex and carry on.
            idx.remove(0);
        }
    }
    if idx.len() == 3 {
        out.push([idx[0], idx[1], idx[2]]);
    }
    out
}

/// Fills any simple polygon.
pub fn fill_poly(p: &Painter, pts: &[Pos2], color: Color32) {
    if pts.len() < 3 || color.a() == 0 {
        return;
    }
    let mut mesh = Mesh::default();
    for &q in pts {
        mesh.colored_vertex(q, color);
    }
    for t in triangulate(pts) {
        mesh.add_triangle(t[0], t[1], t[2]);
    }
    p.add(Shape::mesh(mesh));
}

/// Fills a convex polygon (anti-aliased edge).
pub fn fill_convex(p: &Painter, pts: Vec<Pos2>, color: Color32) {
    p.add(Shape::convex_polygon(pts, color, Stroke::NONE));
}

/// A triangle fan from `center` to the closed outline `pts`, colours blending from the centre.
pub fn fan(p: &Painter, center: Pos2, pts: &[Pos2], c_center: Color32, c_edge: Color32) {
    let mut mesh = Mesh::default();
    mesh.colored_vertex(center, c_center);
    for &q in pts {
        mesh.colored_vertex(q, c_edge);
    }
    let n = pts.len() as u32;
    for i in 0..n {
        mesh.add_triangle(0, 1 + i, 1 + (i + 1) % n);
    }
    p.add(Shape::mesh(mesh));
}

/// A rectangle with a vertical gradient.
pub fn vgradient(p: &Painter, rect: Rect, top: Color32, bottom: Color32) {
    let mut mesh = Mesh::default();
    mesh.colored_vertex(rect.left_top(), top);
    mesh.colored_vertex(rect.right_top(), top);
    mesh.colored_vertex(rect.right_bottom(), bottom);
    mesh.colored_vertex(rect.left_bottom(), bottom);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    p.add(Shape::mesh(mesh));
}

/// A rectangle with a horizontal gradient.
pub fn hgradient(p: &Painter, rect: Rect, left: Color32, right: Color32) {
    let mut mesh = Mesh::default();
    mesh.colored_vertex(rect.left_top(), left);
    mesh.colored_vertex(rect.right_top(), right);
    mesh.colored_vertex(rect.right_bottom(), right);
    mesh.colored_vertex(rect.left_bottom(), left);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    p.add(Shape::mesh(mesh));
}

/// The largest corner radius epaint can blur cleanly: it adds half the blur to the corner,
/// and once that reaches half the rectangle's size its feathered mesh folds over itself.
fn blur_corner(rect: Rect, corner: f32, blur: f32) -> f32 {
    let max = rect.size().min_elem() * 0.32 - blur * 0.5;
    corner.min(max).clamp(0.0, 255.0)
}

/// A soft glow (a blurred rounded rectangle).
pub fn glow(p: &Painter, rect: Rect, corner: f32, blur: f32, color: Color32) {
    let blur = blur.clamp(0.0, 255.0).min(rect.size().min_elem() - 2.0).max(0.0);
    let s = Shadow { offset: [0, 0], blur: blur as u8, spread: 0, color };
    p.add(s.as_shape(rect, blur_corner(rect, corner, blur)));
}

/// A drop shadow under a rectangle.
pub fn drop_shadow(p: &Painter, rect: Rect, corner: f32, offset: Vec2, blur: f32, strength: f32) {
    let blur = blur.clamp(0.0, 255.0).min(rect.size().min_elem() - 2.0).max(0.0);
    let s = Shadow {
        offset: [offset.x.clamp(-127.0, 127.0) as i8, offset.y.clamp(-127.0, 127.0) as i8],
        blur: blur as u8,
        spread: 0,
        color: alpha(Color32::BLACK, strength),
    };
    p.add(s.as_shape(rect, blur_corner(rect, corner, blur)));
}

/// A soft round blob (stains, light pools): a radial gradient mesh, solid in the middle and
/// fading out at `radius`.
pub fn blob(p: &Painter, center: Pos2, radius: f32, color: Color32) {
    let n = 40;
    let mut mesh = Mesh::default();
    mesh.colored_vertex(center, color);
    let rings = [(0.45f32, 0.8f32), (0.75, 0.35), (1.0, 0.0)];
    for (r, a) in rings {
        for k in 0..n {
            let ang = k as f32 / n as f32 * TAU;
            mesh.colored_vertex(polar(center, radius * r, ang), alpha(color, a));
        }
    }
    let n32 = n as u32;
    for k in 0..n32 {
        mesh.add_triangle(0, 1 + k, 1 + (k + 1) % n32);
    }
    for ring in 0..2u32 {
        let (i0, o0) = (1 + ring * n32, 1 + (ring + 1) * n32);
        for k in 0..n32 {
            let k1 = (k + 1) % n32;
            mesh.add_triangle(i0 + k, o0 + k, o0 + k1);
            mesh.add_triangle(i0 + k, o0 + k1, i0 + k1);
        }
    }
    p.add(Shape::mesh(mesh));
}

/// A radial vignette: transparent inside an ellipse, `edge` at the screen border.
pub fn vignette(p: &Painter, rect: Rect, inner: f32, edge: Color32) {
    let n = 48;
    let c = rect.center();
    let mut mesh = Mesh::default();
    let clear = Color32::TRANSPARENT;
    for k in 0..n {
        let a = k as f32 / n as f32 * TAU;
        let d = vec2(a.sin(), -a.cos());
        // Inner ellipse.
        mesh.colored_vertex(c + vec2(d.x * rect.width() * 0.5 * inner, d.y * rect.height() * 0.5 * inner), clear);
        // Outer: the rectangle's border along the same direction.
        let tx = if d.x.abs() > 1e-6 { rect.width() * 0.5 / d.x.abs() } else { f32::INFINITY };
        let ty = if d.y.abs() > 1e-6 { rect.height() * 0.5 / d.y.abs() } else { f32::INFINITY };
        mesh.colored_vertex(c + d * tx.min(ty) * 1.02, edge);
    }
    // Corners need full coverage: extend with the 4 corners via the outer ring.
    for k in 0..n as u32 {
        let (i0, o0, i1, o1) = (2 * k, 2 * k + 1, 2 * ((k + 1) % n as u32), 2 * ((k + 1) % n as u32) + 1);
        mesh.add_triangle(i0, o0, o1);
        mesh.add_triangle(i0, o1, i1);
    }
    p.add(Shape::mesh(mesh));
    // The outer ring follows the border, so the corners are already covered.
}

// ---------------------------------------------------------------- parchment and rope

/// The outline of a torn sheet inside `rect`, clockwise: a rounded rectangle resampled every
/// `step` points and pushed inward by noise (smooth normals, so strokes never spike).
pub fn torn_outline(rect: Rect, seed: u32, amp: f32, step: f32) -> Vec<Pos2> {
    let base = rounded_rect_points(rect, amp * 3.0, 6);
    let c = rect.center();
    let n = base.len();
    let mut out = Vec::new();
    let mut k = 0u32;
    for i in 0..n {
        let (a, b) = (base[i], base[(i + 1) % n]);
        let len = (b - a).length();
        if len < 1e-3 {
            continue;
        }
        let t = (b - a) / len;
        let mut nrm = vec2(-t.y, t.x);
        if nrm.dot(c - a) < 0.0 {
            nrm = -nrm;
        }
        let steps = (len / step).ceil().max(1.0) as usize;
        for j in 0..steps {
            let q = a + (b - a) * (j as f32 / steps as f32);
            let wobble = noise(k as f32 * 0.37, seed) * 0.7 + hash(k, seed ^ 0x51) * 0.3;
            let notch = if hash(k, seed ^ 0xbeef) > 0.94 { 1.2 } else { 0.0 };
            out.push(q + nrm * amp * (wobble + notch));
            k += 1;
        }
    }
    out
}

/// Points along a rounded rectangle, clockwise from the top-left.
pub fn rounded_rect_points(rect: Rect, r: f32, seg: usize) -> Vec<Pos2> {
    let r = r.min(rect.width() * 0.5).min(rect.height() * 0.5).max(0.0);
    let mut pts = Vec::new();
    let centers = [
        (pos2(rect.right() - r, rect.top() + r), 0.0),
        (pos2(rect.right() - r, rect.bottom() - r), PI * 0.5),
        (pos2(rect.left() + r, rect.bottom() - r), PI),
        (pos2(rect.left() + r, rect.top() + r), PI * 1.5),
    ];
    for (c, a0) in centers {
        for i in 0..=seg {
            let a = a0 + i as f32 / seg as f32 * PI * 0.5;
            pts.push(polar(c, r, a));
        }
    }
    pts
}

/// A parchment sheet with torn edges, aged colour toward the edges, stains and a drop
/// shadow. `seed` varies the tears.
pub fn parchment(p: &Painter, rect: Rect, theme: &Theme, seed: u32) {
    drop_shadow(p, rect.shrink(4.0), 10.0, vec2(0.0, 10.0), 30.0, 0.55);
    let amp = (rect.width().min(rect.height()) * 0.012).clamp(2.5, 7.0);
    let outer = torn_outline(rect, seed, amp, 11.0);
    let c = rect.center();
    let inset = (rect.width().min(rect.height()) * 0.22).min(90.0);
    let inner_rect = rect.shrink(inset);
    // Ring from the torn edge (aged) to an inner rectangle (clean), then the inner fill.
    let mut mesh = Mesh::default();
    let edge = theme.parchment_dark;
    let mid = mix(theme.parchment, theme.parchment_dark, 0.15);
    let n = outer.len();
    for (i, &q) in outer.iter().enumerate() {
        mesh.colored_vertex(q, edge);
        // Matching point on the inner rectangle: project toward the centre, clamped.
        let d = q - c;
        let sx = if d.x.abs() > 1e-3 { (inner_rect.width() * 0.5 / d.x.abs()).min(1.0) } else { 1.0 };
        let sy = if d.y.abs() > 1e-3 { (inner_rect.height() * 0.5 / d.y.abs()).min(1.0) } else { 1.0 };
        let s = sx.min(sy);
        let qi = c + d * s;
        let tint = hash(i as u32 / 6, seed ^ 0x77) * 0.12;
        mesh.colored_vertex(qi, mix(mid, edge, tint));
    }
    for i in 0..n as u32 {
        let j = (i + 1) % n as u32;
        mesh.add_triangle(2 * i, 2 * j, 2 * j + 1);
        mesh.add_triangle(2 * i, 2 * j + 1, 2 * i + 1);
    }
    // Inner: a fan from the centre.
    let ci = mesh.vertices.len() as u32;
    mesh.colored_vertex(c, theme.parchment);
    for i in 0..n as u32 {
        let j = (i + 1) % n as u32;
        mesh.add_triangle(ci, 2 * i + 1, 2 * j + 1);
    }
    p.add(Shape::mesh(mesh));
    // A few stains.
    for k in 0..4 {
        let x = rect.left() + rect.width() * (0.15 + 0.7 * hash(k, seed ^ 0x13));
        let y = rect.top() + rect.height() * (0.15 + 0.7 * hash(k, seed ^ 0x29));
        let r = rect.width().min(rect.height()) * (0.08 + 0.12 * hash(k, seed ^ 0x3b));
        blob(p, pos2(x, y), r, alpha(theme.rope_dark, 0.05));
    }
    // A burnt edge line.
    p.add(PathShape::closed_line(outer, Stroke::new(2.0, alpha(theme.rope_dark, 0.55))));
}

/// A rope along `pts`: a dark core with twisted strands.
pub fn rope(p: &Painter, pts: &[Pos2], closed: bool, width: f32, theme: &Theme) {
    if pts.len() < 2 {
        return;
    }
    let mut path: Vec<Pos2> = pts.to_vec();
    if closed {
        path.push(pts[0]);
    }
    p.add(PathShape::line(path.clone(), PathStroke::new(width + 2.0, alpha(theme.rope_dark, 0.9))));
    p.add(PathShape::line(path.clone(), PathStroke::new(width * 0.8, theme.rope)));
    // Strands: short diagonal strokes every `pitch` along the path.
    let pitch = width * 0.85;
    let mut carry = 0.0;
    for w in path.windows(2) {
        let (a, b) = (w[0], w[1]);
        let seg = (b - a).length();
        if seg < 1e-3 {
            continue;
        }
        let t = (b - a) / seg;
        let n = vec2(-t.y, t.x);
        let mut s = carry;
        while s < seg {
            let q = a + t * s;
            let h = width * 0.42;
            let d1 = q - n * h - t * h * 0.55;
            let d2 = q + n * h + t * h * 0.55;
            p.line_segment([d1, d2], Stroke::new(width * 0.22, theme.rope_dark));
            let o = t * width * 0.18;
            p.line_segment([d1 + o + n * 1.0, d2 + o - n * 0.0], Stroke::new(width * 0.14, theme.rope_light));
            s += pitch;
        }
        carry = s - seg;
    }
}

/// A rope frame inset into `rect`.
pub fn rope_frame(p: &Painter, rect: Rect, inset: f32, width: f32, theme: &Theme) {
    let r = rect.shrink(inset);
    let pts = rounded_rect_points(r, width * 2.2, 5);
    rope(p, &pts, true, width, theme);
}

/// A parchment panel with a rope frame.
pub fn panel(p: &Painter, rect: Rect, theme: &Theme, seed: u32) {
    parchment(p, rect, theme, seed);
    rope_frame(p, rect, 16.0, 7.0, theme);
}

/// A dark translucent panel with a thin gold double rule (HUD and overlays).
pub fn ink_panel(p: &Painter, rect: Rect, theme: &Theme, opacity: f32) {
    let corner = 10.0;
    drop_shadow(p, rect, corner, vec2(0.0, 4.0), 16.0, 0.35 * opacity);
    p.rect_filled(rect, corner, alpha(theme.ink, 0.82 * opacity));
    p.rect_stroke(rect.shrink(3.0), corner - 2.0, Stroke::new(1.5, alpha(theme.gold, 0.55 * opacity)), StrokeKind::Inside);
    p.rect_stroke(rect.shrink(7.0), corner - 4.0, Stroke::new(1.0, alpha(theme.gold, 0.25 * opacity)), StrokeKind::Inside);
}

/// The focus treatment for a row or button: amber glow, a warm band and an accent bar on the
/// left (`strength` 0..1 animates it).
pub fn focus_band(p: &Painter, rect: Rect, theme: &Theme, strength: f32, on_dark: bool) {
    if strength <= 0.0 {
        return;
    }
    let k = strength.clamp(0.0, 1.0);
    glow(p, rect.expand(4.0), 12.0, 26.0, alpha(theme.amber_bright, if on_dark { 0.45 } else { 0.38 } * k));
    let band = if on_dark { alpha(theme.amber, 0.30 * k) } else { alpha(theme.amber_bright, 0.40 * k) };
    let fade = Color32::TRANSPARENT;
    let r = rect.shrink2(vec2(0.0, 2.0));
    hgradient(p, Rect::from_min_max(r.min, pos2(r.left() + r.width() * 0.75, r.bottom())), band, alpha(band, 0.6));
    hgradient(p, Rect::from_min_max(pos2(r.left() + r.width() * 0.75, r.top()), r.max), alpha(band, 0.6), fade);
    let bar = Rect::from_min_size(pos2(rect.left() - 2.0, rect.top() + 4.0), vec2(9.0, rect.height() - 8.0));
    p.rect_filled(bar, 3.0, alpha(theme.amber_bright, k));
    p.rect_filled(bar.shrink2(vec2(2.5, 3.0)), 2.0, alpha(Color32::from_rgb(255, 226, 160), k));
}

// ---------------------------------------------------------------- marks

/// Points of an arc (compass angles, radians).
pub fn arc_points(c: Pos2, r: f32, a0: f32, a1: f32, n: usize) -> Vec<Pos2> {
    (0..=n).map(|i| polar(c, r, a0 + (a1 - a0) * i as f32 / n as f32)).collect()
}

/// A progress ring: a track and a clockwise arc from the top covering `frac`.
pub fn ring(p: &Painter, c: Pos2, r: f32, width: f32, frac: f32, color: Color32, track: Color32) {
    if track.a() > 0 {
        p.circle_stroke(c, r, Stroke::new(width, track));
    }
    let f = frac.clamp(0.0, 1.0);
    if f > 0.001 {
        let n = ((f * 64.0).ceil() as usize).max(2);
        p.add(PathShape::line(arc_points(c, r, 0.0, f * TAU, n), PathStroke::new(width, color)));
    }
}

/// An arc band (thick arc) between compass angles.
pub fn arc_band(p: &Painter, c: Pos2, r: f32, width: f32, a0: f32, a1: f32, color: Color32) {
    let n = (((a1 - a0).abs() / TAU * 64.0).ceil() as usize).max(2);
    p.add(PathShape::line(arc_points(c, r, a0, a1, n), PathStroke::new(width, color)));
}

/// A tick (✓).
pub fn tick(p: &Painter, c: Pos2, s: f32, color: Color32) {
    let pts = vec![c + vec2(-0.42, 0.02) * s, c + vec2(-0.12, 0.32) * s, c + vec2(0.45, -0.36) * s];
    p.add(PathShape::line(pts, PathStroke::new(s * 0.16, color)));
}

/// A cross (✕) as two strokes.
pub fn cross(p: &Painter, c: Pos2, s: f32, color: Color32, width: f32) {
    let h = s * 0.5;
    p.line_segment([c + vec2(-h, -h), c + vec2(h, h)], Stroke::new(width, color));
    p.line_segment([c + vec2(-h, h), c + vec2(h, -h)], Stroke::new(width, color));
}

/// A filled triangle pointing at compass angle `a` (◀ = -π/2, ▶ = π/2).
pub fn arrow_head(p: &Painter, c: Pos2, s: f32, a: f32, color: Color32) {
    let pts = vec![polar(c, s * 0.55, a), polar(c, s * 0.5, a + 2.3), polar(c, s * 0.5, a - 2.3)];
    fill_convex(p, pts, color);
}

/// A chevron (‹ ›) stroke pointing at compass angle `a`.
pub fn chevron(p: &Painter, c: Pos2, s: f32, a: f32, color: Color32, width: f32) {
    let tip = polar(c, s * 0.3, a);
    let pts = vec![polar(tip, s * 0.55, a + PI - 0.75), tip, polar(tip, s * 0.55, a + PI + 0.75)];
    p.add(PathShape::line(pts, PathStroke::new(width, color)));
}

/// A diamond.
pub fn diamond(p: &Painter, c: Pos2, s: f32, fill: Color32, stroke: Stroke) {
    let h = s * 0.5;
    p.add(Shape::convex_polygon(
        vec![c + vec2(0.0, -h), c + vec2(h * 0.78, 0.0), c + vec2(0.0, h), c + vec2(-h * 0.78, 0.0)],
        fill,
        stroke,
    ));
}

/// The ☰ menu icon.
pub fn hamburger(p: &Painter, rect: Rect, color: Color32) {
    let w = rect.width() * 0.56;
    let t = (rect.height() * 0.085).max(2.0);
    for k in -1..=1 {
        let y = rect.center().y + k as f32 * rect.height() * 0.2;
        let r = Rect::from_center_size(pos2(rect.center().x, y), vec2(w, t));
        p.rect_filled(r, t * 0.5, color);
    }
}

/// A speaker icon (sound on/off).
pub fn speaker(p: &Painter, c: Pos2, s: f32, color: Color32, on: bool) {
    let body = vec![
        c + vec2(-0.42, -0.14) * s,
        c + vec2(-0.2, -0.14) * s,
        c + vec2(0.05, -0.38) * s,
        c + vec2(0.05, 0.38) * s,
        c + vec2(-0.2, 0.14) * s,
        c + vec2(-0.42, 0.14) * s,
    ];
    fill_poly(p, &body, color);
    if on {
        for (k, r) in [0.22f32, 0.38].iter().enumerate() {
            let pts = arc_points(c + vec2(0.06, 0.0) * s, r * s, PI * 0.25, PI * 0.75, 10);
            p.add(PathShape::line(pts, PathStroke::new(s * (0.07 - k as f32 * 0.01), color)));
        }
    } else {
        cross(p, c + vec2(0.3, 0.0) * s, s * 0.3, color, s * 0.07);
    }
}

// ---------------------------------------------------------------- emblem and logo

/// The keg-tap emblem: a banded keg with a brass tap and a drop of rum, crossed blades
/// behind. `h` is the overall height.
pub fn keg_tap(p: &Painter, c: Pos2, h: f32, theme: &Theme) {
    let s = h;
    let ink = theme.ink;
    // Crossed blades behind the keg.
    for side in [-1.0f32, 1.0] {
        let a = c + vec2(-0.40 * side, 0.36) * s;
        let b = c + vec2(0.36 * side, -0.46) * s;
        let dir = (b - a).normalized();
        let n = vec2(-dir.y, dir.x);
        let blade = vec![
            a + n * 0.034 * s,
            b + n * 0.016 * s - dir * 0.05 * s,
            b + dir * 0.03 * s,
            b - n * 0.016 * s - dir * 0.05 * s,
            a - n * 0.034 * s,
        ];
        fill_convex(p, blade.clone(), Color32::from_rgb(206, 210, 216));
        // A fuller along the blade.
        p.line_segment([a + dir * 0.08 * s, b - dir * 0.12 * s], Stroke::new(s * 0.01, Color32::from_rgb(150, 156, 166)));
        p.add(PathShape::closed_line(blade, Stroke::new(s * 0.014, ink)));
        // Guard and grip.
        let g = a + dir * 0.06 * s;
        p.line_segment([g - n * 0.07 * s, g + n * 0.07 * s], Stroke::new(s * 0.028, theme.brass));
        p.line_segment([a, a - dir * 0.09 * s], Stroke::new(s * 0.035, theme.wood));
        p.circle_filled(a - dir * 0.1 * s, s * 0.022, theme.brass);
    }
    // The keg: bulged staves.
    let kc = c + vec2(-0.06, 0.02) * s;
    let (kw, kh) = (0.50 * s, 0.62 * s);
    let n = 16;
    let mut outline = Vec::new();
    for i in 0..=n {
        let t = i as f32 / n as f32;
        let y = kc.y - kh * 0.5 + kh * t;
        let bulge = 0.84 + 0.16 * (PI * t).sin();
        outline.push(pos2(kc.x + kw * 0.5 * bulge, y));
    }
    for i in (0..=n).rev() {
        let t = i as f32 / n as f32;
        let y = kc.y - kh * 0.5 + kh * t;
        let bulge = 0.84 + 0.16 * (PI * t).sin();
        outline.push(pos2(kc.x - kw * 0.5 * bulge, y));
    }
    fill_convex(p, outline.clone(), theme.wood);
    // Shading: a lighter middle stave band.
    let mut band = Vec::new();
    for i in 0..=n {
        let t = i as f32 / n as f32;
        let y = kc.y - kh * 0.5 + kh * t;
        let bulge = 0.84 + 0.16 * (PI * t).sin();
        band.push(pos2(kc.x - kw * 0.12 * bulge, y));
    }
    for i in (0..=n).rev() {
        let t = i as f32 / n as f32;
        let y = kc.y - kh * 0.5 + kh * t;
        let bulge = 0.84 + 0.16 * (PI * t).sin();
        band.push(pos2(kc.x - kw * 0.34 * bulge, y));
    }
    fill_convex(p, band, alpha(Color32::from_rgb(150, 100, 58), 0.8));
    // Stave lines.
    for k in [-0.3f32, -0.05, 0.2] {
        let pts: Vec<Pos2> = (0..=n)
            .map(|i| {
                let t = i as f32 / n as f32;
                let bulge = 0.84 + 0.16 * (PI * t).sin();
                pos2(kc.x + kw * k * bulge, kc.y - kh * 0.5 + kh * t)
            })
            .collect();
        p.add(PathShape::line(pts, PathStroke::new(s * 0.01, alpha(ink, 0.6))));
    }
    // Brass hoops.
    for t in [0.14f32, 0.86] {
        let bulge = 0.84 + 0.16 * (PI * t).sin();
        let y = kc.y - kh * 0.5 + kh * t;
        let half = kw * 0.5 * bulge;
        let pts: Vec<Pos2> = (0..=12)
            .map(|i| {
                let u = i as f32 / 12.0 * 2.0 - 1.0;
                pos2(kc.x + half * u, y + (1.0 - u * u) * s * 0.025)
            })
            .collect();
        p.add(PathShape::line(pts.clone(), PathStroke::new(s * 0.05, theme.ink)));
        p.add(PathShape::line(pts, PathStroke::new(s * 0.032, theme.brass)));
    }
    p.add(PathShape::closed_line(outline, Stroke::new(s * 0.018, ink)));
    // Top rim.
    let rim = Rect::from_center_size(pos2(kc.x, kc.y - kh * 0.5), vec2(kw * 0.84, s * 0.07));
    p.add(Shape::ellipse_filled(rim.center(), rim.size() * 0.5, Color32::from_rgb(84, 54, 30)));
    p.add(Shape::ellipse_stroke(rim.center(), rim.size() * 0.5, Stroke::new(s * 0.014, ink)));
    // The tap: from the keg's right side, lever up, nozzle down.
    let base = pos2(kc.x + kw * 0.47, kc.y + kh * 0.16);
    let spout_end = base + vec2(0.20 * s, 0.0);
    let brass_dark = Color32::from_rgb(120, 86, 30);
    p.line_segment([base, spout_end], Stroke::new(s * 0.085, ink));
    p.line_segment([base, spout_end], Stroke::new(s * 0.06, theme.brass));
    p.line_segment(
        [base + vec2(0.0, -0.012 * s), spout_end + vec2(0.0, -0.012 * s)],
        Stroke::new(s * 0.016, Color32::from_rgb(246, 214, 130)),
    );
    let nozzle_top = spout_end + vec2(-0.02 * s, 0.0);
    let nozzle_bot = nozzle_top + vec2(0.0, 0.10 * s);
    p.line_segment([nozzle_top, nozzle_bot], Stroke::new(s * 0.075, ink));
    p.line_segment([nozzle_top, nozzle_bot], Stroke::new(s * 0.05, theme.brass));
    p.circle_filled(base, s * 0.055, ink);
    p.circle_filled(base, s * 0.04, brass_dark);
    p.circle_filled(base, s * 0.024, theme.brass);
    // Lever (T handle).
    let lever_base = base + vec2(0.11 * s, -0.03 * s);
    let lever_top = lever_base + vec2(0.03 * s, -0.16 * s);
    p.line_segment([lever_base, lever_top], Stroke::new(s * 0.05, ink));
    p.line_segment([lever_base, lever_top], Stroke::new(s * 0.03, theme.brass));
    let bar = [lever_top + vec2(-0.07 * s, -0.012 * s), lever_top + vec2(0.07 * s, 0.012 * s)];
    p.line_segment(bar, Stroke::new(s * 0.06, ink));
    p.line_segment(bar, Stroke::new(s * 0.038, theme.brass));
    // A drop of rum.
    let d = nozzle_bot + vec2(0.0, 0.10 * s);
    let r = s * 0.038;
    let drop: Vec<Pos2> = (0..=20)
        .map(|i| {
            let a = i as f32 / 20.0 * TAU;
            let q = polar(d, r, a);
            // Pull the top into a point.
            if a.cos() > 0.3 { q + vec2(0.0, -(a.cos() - 0.3) * r * 1.6) } else { q }
        })
        .collect();
    fill_convex(p, drop.clone(), theme.amber_bright);
    p.add(PathShape::closed_line(drop, Stroke::new(s * 0.01, ink)));
    p.circle_filled(d + vec2(-0.3, 0.1) * r, r * 0.3, alpha(Color32::WHITE, 0.6));
}

/// The emblem inside a dark medallion with a gold ring (small logos and badges).
pub fn medallion(p: &Painter, c: Pos2, r: f32, theme: &Theme) {
    drop_shadow(p, Rect::from_center_size(c, Vec2::splat(r * 2.0)), r, vec2(0.0, 4.0), 14.0, 0.5);
    p.circle_filled(c, r, theme.ink);
    p.circle_stroke(c, r * 0.94, Stroke::new(r * 0.06, theme.gold));
    p.circle_stroke(c, r * 0.84, Stroke::new(r * 0.02, alpha(theme.gold, 0.6)));
    keg_tap(p, c + vec2(-0.02 * r, 0.04 * r), r * 1.35, theme);
}

/// How the logo is laid out.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LogoStyle {
    /// Big and centred: emblem above, two lines, tagline (title screen).
    Title,
    /// Small: medallion at the left, two lines beside it (menus, attract corner).
    Corner,
}

/// Draws the logo; `anchor` is the centre for [`LogoStyle::Title`] and the top-left for
/// [`LogoStyle::Corner`]. `title` is "Assassin's Keg — Black Tap": the part after the dash goes
/// on the second line. Returns the logo's rect.
pub fn logo(
    p: &Painter,
    anchor: Pos2,
    scale: f32,
    theme: &Theme,
    title: &str,
    tagline: &str,
    style: LogoStyle,
    opacity: f32,
) -> Rect {
    let (line1, line2) = split_title(title);
    let o = opacity.clamp(0.0, 1.0);
    let outline = alpha(theme.ink, 0.92 * o);
    match style {
        LogoStyle::Title => {
            let emblem_h = 236.0 * scale;
            let top = anchor.y - 262.0 * scale;
            glow(
                p,
                Rect::from_center_size(pos2(anchor.x, top + emblem_h * 0.5), vec2(emblem_h * 1.2, emblem_h * 0.9)),
                80.0,
                80.0,
                alpha(Color32::BLACK, 0.35 * o),
            );
            keg_tap(p, pos2(anchor.x, top + emblem_h * 0.5), emblem_h, theme);
            let size1 = quantize(theme.sizes.logo * scale);
            let y1 = top + emblem_h + 6.0 * scale;
            let r1 = text_outlined(
                p,
                pos2(anchor.x, y1),
                Align2::CENTER_TOP,
                &line1,
                theme.display(size1),
                alpha(theme.parchment, o),
                outline,
                5.0 * scale,
            );
            let size2 = quantize(theme.sizes.logo * 0.62 * scale);
            let y2 = r1.bottom() - size1 * 0.12;
            let r2 = text_spaced(
                p,
                pos2(anchor.x, y2),
                Align2::CENTER_TOP,
                &line2.to_uppercase(),
                theme.display(size2),
                alpha(theme.gold, o),
                size2 * 0.12,
                Some((outline, 4.0 * scale)),
            );
            // Flourishes either side of the second line.
            for side in [-1.0f32, 1.0] {
                let x0 = anchor.x + side * (r2.width() * 0.5 + 24.0 * scale);
                let x1 = x0 + side * 150.0 * scale;
                let y = r2.center().y + 4.0 * scale;
                p.line_segment([pos2(x0, y), pos2(x1, y)], Stroke::new(7.0 * scale, outline));
                p.line_segment([pos2(x0, y), pos2(x1, y)], Stroke::new(3.0 * scale, alpha(theme.gold, o)));
                diamond(
                    p,
                    pos2(x1 + side * 10.0 * scale, y),
                    22.0 * scale,
                    alpha(theme.gold, o),
                    Stroke::new(2.5 * scale, outline),
                );
                diamond(p, pos2(x0 - side * 2.0, y), 12.0 * scale, alpha(theme.gold, o), Stroke::NONE);
            }
            let mut rect = r1.union(r2);
            if !tagline.is_empty() {
                let r3 = text_outlined(
                    p,
                    pos2(anchor.x, r2.bottom() + 18.0 * scale),
                    Align2::CENTER_TOP,
                    tagline,
                    italic_font(theme, quantize(36.0 * scale)),
                    alpha(theme.parchment_light, o),
                    alpha(theme.ink, 0.8 * o),
                    3.0 * scale,
                );
                rect = rect.union(r3);
            }
            rect.union(Rect::from_center_size(pos2(anchor.x, top + emblem_h * 0.5), Vec2::splat(emblem_h)))
        }
        LogoStyle::Corner => {
            let r = 46.0 * scale;
            let mc = anchor + vec2(r, r);
            medallion(p, mc, r, theme);
            let x = anchor.x + r * 2.0 + 16.0 * scale;
            let size1 = quantize(56.0 * scale);
            let r1 = text_outlined(
                p,
                pos2(x, mc.y + 6.0 * scale),
                Align2::LEFT_BOTTOM,
                &line1,
                theme.display(size1),
                alpha(theme.parchment, o),
                outline,
                3.0 * scale,
            );
            let size2 = quantize(32.0 * scale);
            let r2 = text_spaced(
                p,
                pos2(x + 2.0, r1.bottom() - 6.0 * scale),
                Align2::LEFT_TOP,
                &line2.to_uppercase(),
                theme.display(size2),
                alpha(theme.gold, o),
                size2 * 0.15,
                Some((outline, 2.5 * scale)),
            );
            Rect::from_min_max(anchor, pos2(r1.right().max(r2.right()), (anchor.y + r * 2.0).max(r2.bottom())))
        }
    }
}

/// Synthetic italics of the body face (taglines, captions).
pub fn italic_font(theme: &Theme, size: f32) -> FontId {
    theme.body(size)
}

/// Splits "Assassin's Keg — Black Tap" / "Assassin's Keg: Black Tap" into two lines.
pub fn split_title(title: &str) -> (String, String) {
    for sep in [" — ", " – ", ": ", " - "] {
        if let Some((a, b)) = title.split_once(sep) {
            return (a.trim().to_string(), b.trim().to_string());
        }
    }
    (title.to_string(), String::new())
}

// ---------------------------------------------------------------- icons

/// Draws a named vector icon centred at `c`, about `s` points across. Unknown names draw a
/// lettered diamond. Names: hull, shot, chain, sail, keg, kegs, repair, cargo, coin, reales,
/// timber, pistol, dart, blade, smoke, bottle, grog, heal, whistle, map, spyglass, chest,
/// sheet, perch, perch_done, tap, fort, tavern, flotsam, ship, enemy_ship, player, objective,
/// waypoint, skull, rope, swivel, wind, star.
pub fn icon(p: &Painter, name: &str, c: Pos2, s: f32, color: Color32, theme: &Theme) {
    let ink = theme.ink;
    let w = (s * 0.07).max(1.5);
    let line = |a: Vec2, b: Vec2, width: f32, col: Color32| {
        p.line_segment([c + a * s, c + b * s], Stroke::new(width, col));
    };
    let poly = |pts: &[(f32, f32)]| pts.iter().map(|&(x, y)| c + vec2(x, y) * s).collect::<Vec<_>>();
    match name {
        "hull" | "ship" | "enemy_ship" => {
            let hull = poly(&[(-0.48, -0.02), (0.5, -0.02), (0.36, 0.26), (-0.36, 0.26)]);
            fill_convex(p, hull.clone(), color);
            p.add(PathShape::closed_line(hull, Stroke::new(w * 0.7, ink)));
            line(vec2(0.0, -0.02), vec2(0.0, -0.46), w, ink);
            let sail = poly(&[(0.03, -0.42), (0.34, -0.12), (0.03, -0.1)]);
            fill_convex(p, sail, if name == "enemy_ship" { theme.red } else { theme.parchment_light });
            let sail2 = poly(&[(-0.03, -0.38), (-0.03, -0.1), (-0.3, -0.12)]);
            fill_convex(p, sail2, if name == "enemy_ship" { theme.red } else { theme.parchment_light });
            line(vec2(-0.46, 0.34), vec2(0.46, 0.34), w * 0.8, alpha(theme.teal, 0.9));
        }
        "shot" => {
            p.circle_filled(c, s * 0.34, ink);
            p.circle_filled(c, s * 0.3, color);
            p.circle_filled(c + vec2(-0.1, -0.1) * s, s * 0.08, alpha(Color32::WHITE, 0.55));
        }
        "chain" => {
            for x in [-0.28f32, 0.28] {
                p.circle_filled(c + vec2(x, 0.0) * s, s * 0.17, color);
                p.circle_stroke(c + vec2(x, 0.0) * s, s * 0.17, Stroke::new(w * 0.6, ink));
            }
            for k in 0..3 {
                let x = -0.12 + k as f32 * 0.12;
                p.add(Shape::ellipse_stroke(c + vec2(x, 0.0) * s, vec2(0.07, 0.045) * s, Stroke::new(w * 0.7, color)));
            }
        }
        "sail" => {
            line(vec2(0.0, -0.46), vec2(0.0, 0.42), w, ink);
            line(vec2(-0.32, -0.32), vec2(0.32, -0.32), w, ink);
            let sail = poly(&[(-0.3, -0.3), (0.3, -0.3), (0.36, 0.16), (0.0, 0.24), (-0.36, 0.16)]);
            fill_convex(p, sail.clone(), color);
            p.add(PathShape::closed_line(sail, Stroke::new(w * 0.6, ink)));
            fill_convex(p, poly(&[(0.0, -0.46), (0.22, -0.41), (0.0, -0.36)]), theme.red);
        }
        "keg" | "kegs" => {
            let pts: Vec<Pos2> = (0..=10)
                .map(|i| {
                    let t = i as f32 / 10.0;
                    c + vec2(0.3 * (0.84 + 0.16 * (PI * t).sin()), -0.38 + 0.76 * t) * s
                })
                .chain((0..=10).rev().map(|i| {
                    let t = i as f32 / 10.0;
                    c + vec2(-0.3 * (0.84 + 0.16 * (PI * t).sin()), -0.38 + 0.76 * t) * s
                }))
                .collect();
            fill_convex(p, pts.clone(), color);
            p.add(PathShape::closed_line(pts, Stroke::new(w * 0.6, ink)));
            for y in [-0.24f32, 0.24] {
                line(vec2(-0.29, y), vec2(0.29, y), w, theme.brass);
            }
            if name == "kegs" {
                // A fuse.
                p.add(PathShape::line(poly(&[(0.0, -0.38), (0.08, -0.48), (0.18, -0.46)]), PathStroke::new(w * 0.6, ink)));
                p.circle_filled(c + vec2(0.2, -0.47) * s, s * 0.05, theme.amber_bright);
            }
        }
        "repair" => {
            // A hammer over a plank.
            let plank = Rect::from_center_size(c + vec2(0.0, 0.22) * s, vec2(0.86, 0.18) * s);
            p.rect_filled(plank, s * 0.03, theme.wood);
            p.rect_stroke(plank, s * 0.03, Stroke::new(w * 0.5, ink), StrokeKind::Inside);
            line(vec2(-0.3, 0.12), vec2(0.18, -0.36), w * 1.6, ink);
            line(vec2(-0.3, 0.12), vec2(0.18, -0.36), w, color);
            let head = poly(&[(0.04, -0.44), (0.24, -0.24), (0.36, -0.36), (0.16, -0.56)]);
            fill_convex(p, head, theme.parchment_dark);
        }
        "cargo" | "chest" => {
            let r = Rect::from_center_size(c + vec2(0.0, 0.04) * s, vec2(0.78, 0.6) * s);
            p.rect_filled(r, s * 0.05, if name == "chest" { theme.wood } else { color });
            p.rect_stroke(r, s * 0.05, Stroke::new(w * 0.7, ink), StrokeKind::Inside);
            if name == "chest" {
                line(vec2(-0.39, -0.08), vec2(0.39, -0.08), w * 0.7, ink);
                p.rect_filled(Rect::from_center_size(c + vec2(0.0, -0.04) * s, vec2(0.12, 0.16) * s), 2.0, theme.gold);
                for x in [-0.25f32, 0.25] {
                    line(vec2(x, -0.26), vec2(x, 0.34), w * 0.8, theme.brass);
                }
            } else {
                line(vec2(-0.36, -0.24), vec2(0.36, 0.32), w * 0.7, ink);
                line(vec2(-0.36, 0.32), vec2(0.36, -0.24), w * 0.7, ink);
            }
        }
        "coin" | "reales" => {
            p.circle_filled(c, s * 0.38, theme.ink);
            p.circle_filled(c, s * 0.34, theme.gold);
            p.circle_stroke(c, s * 0.25, Stroke::new(w * 0.5, Color32::from_rgb(150, 105, 30)));
            line(vec2(-0.12, 0.0), vec2(0.12, 0.0), w * 0.8, Color32::from_rgb(150, 105, 30));
            line(vec2(0.0, -0.13), vec2(0.0, 0.13), w * 0.8, Color32::from_rgb(150, 105, 30));
        }
        "timber" | "flotsam" => {
            for (k, y) in [-0.18f32, 0.12].iter().enumerate() {
                let x0 = if k == 0 { -0.36 } else { -0.3 };
                let r = Rect::from_min_size(c + vec2(x0, *y) * s, vec2(0.68, 0.22) * s);
                p.rect_filled(r, s * 0.1, theme.wood);
                p.rect_stroke(r, s * 0.1, Stroke::new(w * 0.5, ink), StrokeKind::Inside);
                p.circle_filled(r.right_center() - vec2(s * 0.08, 0.0), s * 0.07, theme.rope_light);
            }
            if name == "flotsam" {
                line(vec2(-0.46, 0.42), vec2(0.46, 0.42), w * 0.8, alpha(theme.teal, 0.9));
            }
        }
        "pistol" => {
            let body = poly(&[
                (-0.44, -0.16),
                (0.36, -0.16),
                (0.36, -0.04),
                (-0.08, -0.02),
                (-0.18, 0.36),
                (-0.36, 0.36),
                (-0.26, -0.02),
                (-0.44, -0.02),
            ]);
            fill_poly(p, &body, color);
            p.add(PathShape::closed_line(body, Stroke::new(w * 0.5, ink)));
            p.circle_filled(c + vec2(-0.14, -0.22) * s, s * 0.05, ink);
        }
        "dart" | "blade" => {
            let blade = poly(&[(-0.4, 0.4), (0.3, -0.3), (0.42, -0.42), (0.36, -0.24), (-0.32, 0.46)]);
            fill_convex(p, blade, Color32::from_rgb(214, 216, 220));
            line(vec2(-0.22, 0.12), vec2(-0.06, 0.28), w * 1.2, theme.brass);
            line(vec2(-0.4, 0.4), vec2(-0.48, 0.48), w * 1.5, theme.wood);
        }
        "smoke" => {
            for (x, y, r) in [(-0.18f32, 0.08f32, 0.2f32), (0.12, 0.1, 0.22), (0.0, -0.14, 0.24)] {
                p.circle_filled(c + vec2(x, y) * s, r * s, color);
                p.circle_stroke(c + vec2(x, y) * s, r * s, Stroke::new(w * 0.4, alpha(ink, 0.5)));
            }
        }
        "bottle" | "grog" | "heal" => {
            let body = poly(&[
                (-0.08, -0.46),
                (0.08, -0.46),
                (0.08, -0.22),
                (0.22, -0.06),
                (0.22, 0.42),
                (-0.22, 0.42),
                (-0.22, -0.06),
                (-0.08, -0.22),
            ]);
            fill_poly(p, &body, if name == "bottle" { alpha(theme.teal, 0.9) } else { color });
            p.add(PathShape::closed_line(body, Stroke::new(w * 0.6, ink)));
            p.rect_filled(Rect::from_center_size(c + vec2(0.0, 0.12) * s, vec2(0.34, 0.18) * s), 2.0, theme.parchment);
            if name == "bottle" {
                // A rolled letter inside.
                line(vec2(-0.1, 0.12), vec2(0.1, 0.12), w * 0.5, ink);
            }
        }
        "whistle" => {
            p.circle_filled(c + vec2(0.1, 0.08) * s, s * 0.24, color);
            p.rect_filled(Rect::from_min_size(c + vec2(-0.44, -0.14) * s, vec2(0.5, 0.18) * s), 3.0, color);
            p.circle_filled(c + vec2(0.1, 0.08) * s, s * 0.08, ink);
            for k in 0..3 {
                let a = -0.6 + k as f32 * 0.35;
                line(
                    vec2(0.0, -0.3) + vec2(a.sin(), -a.cos()) * 0.06,
                    vec2(0.0, -0.3) + vec2(a.sin(), -a.cos()) * 0.16,
                    w * 0.6,
                    color,
                );
            }
        }
        "map" | "sheet" => {
            let r = Rect::from_center_size(c, vec2(0.62, 0.78) * s);
            p.rect_filled(r, 3.0, theme.parchment);
            p.rect_stroke(r, 3.0, Stroke::new(w * 0.6, ink), StrokeKind::Inside);
            if name == "sheet" {
                for k in 0..4 {
                    let y = -0.24 + k as f32 * 0.14;
                    line(vec2(-0.22, y), vec2(0.22, y), w * 0.4, ink);
                }
                p.circle_filled(c + vec2(0.06, 0.24) * s, s * 0.05, ink);
                line(vec2(0.11, 0.24), vec2(0.11, 0.06), w * 0.4, ink);
            } else {
                p.add(PathShape::line(
                    poly(&[(-0.2, 0.22), (-0.05, 0.0), (0.08, 0.1), (0.2, -0.2)]),
                    PathStroke::new(w * 0.5, theme.red),
                ));
                cross(p, c + vec2(0.2, -0.22) * s, s * 0.12, theme.red, w * 0.6);
            }
        }
        "spyglass" => {
            line(vec2(-0.42, 0.24), vec2(0.4, -0.2), w * 3.2, ink);
            line(vec2(-0.42, 0.24), vec2(0.4, -0.2), w * 2.4, theme.brass);
            line(vec2(-0.42, 0.24), vec2(-0.06, 0.05), w * 3.6, theme.wood);
        }
        "perch" | "perch_done" => {
            // A gull: two arched wings.
            let left = arc_points(c + vec2(-0.2, 0.08) * s, s * 0.22, -1.4, 0.9, 10);
            let right = arc_points(c + vec2(0.2, 0.08) * s, s * 0.22, -0.9, 1.4, 10);
            let col = if name == "perch_done" { theme.gold } else { color };
            p.add(PathShape::line(left, PathStroke::new(w * 1.4, col)));
            p.add(PathShape::line(right, PathStroke::new(w * 1.4, col)));
        }
        "tap" => {
            p.circle_filled(c + vec2(-0.2, 0.0) * s, s * 0.14, theme.brass);
            line(vec2(-0.2, 0.0), vec2(0.28, 0.0), w * 1.6, theme.brass);
            line(vec2(0.24, 0.0), vec2(0.24, 0.24), w * 1.4, theme.brass);
            line(vec2(0.02, -0.02), vec2(0.06, -0.32), w * 1.2, theme.brass);
            p.circle_filled(c + vec2(0.24, 0.38) * s, s * 0.07, theme.amber_bright);
        }
        "fort" => {
            let body = poly(&[(-0.3, 0.42), (-0.26, -0.22), (0.26, -0.22), (0.3, 0.42)]);
            fill_convex(p, body.clone(), color);
            p.add(PathShape::closed_line(body, Stroke::new(w * 0.6, ink)));
            for k in 0..3 {
                let x = -0.26 + k as f32 * 0.21;
                p.rect_filled(Rect::from_min_size(c + vec2(x, -0.36) * s, vec2(0.1, 0.14) * s), 1.0, color);
            }
            p.rect_filled(Rect::from_center_size(c + vec2(0.0, 0.26) * s, vec2(0.14, 0.3) * s), 4.0, ink);
        }
        "tavern" => {
            let mug = Rect::from_center_size(c + vec2(-0.06, 0.06) * s, vec2(0.46, 0.6) * s);
            p.rect_filled(mug, 4.0, theme.amber);
            p.rect_stroke(mug, 4.0, Stroke::new(w * 0.6, ink), StrokeKind::Inside);
            p.add(Shape::ellipse_stroke(c + vec2(0.24, 0.06) * s, vec2(0.12, 0.16) * s, Stroke::new(w, ink)));
            for x in [-0.18f32, -0.04, 0.1] {
                p.circle_filled(c + vec2(x, -0.26) * s, s * 0.09, theme.parchment_light);
            }
        }
        "player" => {
            let pts = poly(&[(0.0, -0.46), (0.32, 0.36), (0.0, 0.2), (-0.32, 0.36)]);
            fill_poly(p, &pts, color);
            p.add(PathShape::closed_line(pts, Stroke::new(w * 0.7, ink)));
        }
        "objective" | "star" => {
            if name == "star" {
                let pts: Vec<Pos2> =
                    (0..10).map(|i| polar(c, if i % 2 == 0 { s * 0.46 } else { s * 0.2 }, i as f32 * TAU / 10.0)).collect();
                fill_poly(p, &pts, color);
                p.add(PathShape::closed_line(pts, Stroke::new(w * 0.5, ink)));
            } else {
                diamond(p, c, s * 0.9, color, Stroke::new(w * 0.8, ink));
                diamond(p, c, s * 0.34, ink, Stroke::NONE);
            }
        }
        "waypoint" => {
            let pts: Vec<Pos2> = arc_points(c + vec2(0.0, -0.12) * s, s * 0.26, -2.3, 2.3, 16)
                .into_iter()
                .chain([c + vec2(0.0, 0.46) * s])
                .collect();
            fill_poly(p, &pts, color);
            p.add(PathShape::closed_line(pts, Stroke::new(w * 0.6, ink)));
            p.circle_filled(c + vec2(0.0, -0.12) * s, s * 0.1, ink);
        }
        "skull" => {
            p.circle_filled(c + vec2(0.0, -0.06) * s, s * 0.3, color);
            p.rect_filled(Rect::from_center_size(c + vec2(0.0, 0.24) * s, vec2(0.32, 0.2) * s), 3.0, color);
            for x in [-0.12f32, 0.12] {
                p.circle_filled(c + vec2(x, -0.04) * s, s * 0.08, ink);
            }
            line(vec2(-0.06, 0.24), vec2(-0.06, 0.32), w * 0.5, ink);
            line(vec2(0.06, 0.24), vec2(0.06, 0.32), w * 0.5, ink);
        }
        "rope" => {
            for r in [0.32f32, 0.2] {
                p.circle_stroke(c, s * r, Stroke::new(w * 1.4, color));
            }
        }
        "swivel" => {
            line(vec2(-0.3, 0.1), vec2(0.4, -0.12), w * 2.4, ink);
            line(vec2(-0.3, 0.1), vec2(0.4, -0.12), w * 1.6, color);
            line(vec2(0.0, 0.0), vec2(0.0, 0.4), w * 1.2, ink);
            p.circle_filled(c, s * 0.08, theme.brass);
        }
        "wind" => {
            for (k, y) in [-0.2f32, 0.0, 0.2].iter().enumerate() {
                let len = 0.36 + 0.1 * (k as f32 - 1.0).abs();
                p.add(PathShape::line(poly(&[(-0.42, *y), (len - 0.42, *y), (len - 0.32, y - 0.08)]), PathStroke::new(w, color)));
            }
        }
        _ => {
            diamond(p, c, s * 0.9, color, Stroke::new(w * 0.6, ink));
            let letter: String = name.chars().next().map(|ch| ch.to_uppercase().collect()).unwrap_or_default();
            p.text(c, Align2::CENTER_CENTER, letter, theme.bold(quantize(s * 0.42)), ink);
        }
    }
}
