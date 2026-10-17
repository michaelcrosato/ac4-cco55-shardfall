//! Vector button glyphs: DualSense (✕ ◯ □ △ in their colours, L1/R1/L2/R2, L3/R3, Options,
//! Create, touchpad, sticks, D-pad), Xbox (A B X Y, LB/RB/LT/RT, Menu/View), keyboard keycaps
//! and mouse buttons. [`paint`] draws one; [`size`] measures it for layout;
//! [`glyph_for_action`] picks the glyph of a binding for the device family.

use std::f32::consts::PI;

use egui::epaint::{PathShape, PathStroke};
use egui::{Align2, Color32, Painter, Pos2, Rect, Shape, Stroke, StrokeKind, Vec2, pos2, vec2};

use crate::bindings::{Action, Bindings, PadButton};
use crate::input::Nav;
use crate::paint::{self, quantize};
use crate::settings::Prompts;
use crate::theme::{Theme, alpha};

/// Which glyph set to draw.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum GlyphFamily {
    #[default]
    PlayStation,
    Xbox,
    Keyboard,
}

/// One glyph.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Glyph {
    Pad(PadButton),
    LeftStick,
    RightStick,
    /// The whole D-pad (move focus).
    DPad,
    /// D-pad left and right (change a value).
    DPadHorizontal,
    /// A keycap: a winit `KeyCode` name ("KeyE", "Space", "ShiftLeft", "ArrowLeft") or a label.
    Key(String),
    MouseLeft,
    MouseRight,
    MouseMiddle,
    MouseWheel,
    MouseMove,
}

impl Glyph {
    pub fn key(name: &str) -> Glyph {
        Glyph::Key(name.to_string())
    }
}

/// The family for a gamepad's name (gilrs `Gamepad::name`): Sony pads get PlayStation glyphs,
/// everything else Xbox.
pub fn family_from_pad_name(name: &str) -> GlyphFamily {
    let n = name.to_ascii_lowercase();
    if ["dualsense", "dualshock", "wireless controller", "playstation", "sony"].iter().any(|k| n.contains(k)) {
        return GlyphFamily::PlayStation;
    }
    let ps_word = n
        .split(|c: char| !c.is_ascii_alphanumeric())
        .any(|w| w == "ps" || (w.starts_with("ps") && w[2..].chars().all(|c| c.is_ascii_digit()) && w.len() > 2));
    if ps_word { GlyphFamily::PlayStation } else { GlyphFamily::Xbox }
}

/// The family prompts use: the setting when it forces one, else the device's.
pub fn resolve_family(setting: Prompts, device: GlyphFamily) -> GlyphFamily {
    match setting {
        Prompts::Auto => device,
        Prompts::PlayStation => GlyphFamily::PlayStation,
        Prompts::Xbox => GlyphFamily::Xbox,
        Prompts::Keyboard => GlyphFamily::Keyboard,
    }
}

/// The glyph of a game action for a device family. Keyboard shows the first key, else the
/// first mouse button; pads show the first pad button. Unknown actions draw a "?" key.
pub fn glyph_for_action(bindings: &Bindings, action: &str, family: GlyphFamily) -> Glyph {
    let Some(a) = bindings.action(action) else { return Glyph::key("?") };
    let mouse = |a: &Action| {
        a.mouse.first().map(|m| match Action::mouse_button(m) {
            "Right" => Glyph::MouseRight,
            "Middle" => Glyph::MouseMiddle,
            "WheelUp" | "WheelDown" => Glyph::MouseWheel,
            _ => Glyph::MouseLeft,
        })
    };
    match family {
        GlyphFamily::Keyboard => a
            .keys
            .first()
            .map(|k| Glyph::Key(k.clone()))
            .or_else(|| mouse(a))
            .or_else(|| a.pad.first().map(|b| Glyph::Pad(*b)))
            .unwrap_or(Glyph::key("?")),
        _ => {
            let pad = if family == GlyphFamily::Xbox {
                // Xbox pads have no touchpad: prefer another binding.
                a.pad.iter().find(|b| **b != PadButton::Touchpad).or(a.pad.first())
            } else {
                a.pad.first()
            };
            pad.map(|b| Glyph::Pad(*b)).or_else(|| a.keys.first().map(|k| Glyph::Key(k.clone()))).unwrap_or(Glyph::key("?"))
        }
    }
}

/// The glyph for a menu action (PLAN §10.3).
pub fn nav_glyph(nav: Nav, family: GlyphFamily) -> Glyph {
    let kb = family == GlyphFamily::Keyboard;
    match nav {
        Nav::Up | Nav::Down => {
            if kb {
                Glyph::key("↑↓")
            } else {
                Glyph::DPad
            }
        }
        Nav::Left | Nav::Right => {
            if kb {
                Glyph::key("←→")
            } else {
                Glyph::DPadHorizontal
            }
        }
        Nav::Confirm => {
            if kb {
                Glyph::key("Enter")
            } else {
                Glyph::Pad(PadButton::South)
            }
        }
        Nav::Back => {
            if kb {
                Glyph::key("Esc")
            } else {
                Glyph::Pad(PadButton::East)
            }
        }
        Nav::TabPrev => {
            if kb {
                Glyph::key("Q")
            } else {
                Glyph::Pad(PadButton::LeftTrigger)
            }
        }
        Nav::TabNext => {
            if kb {
                Glyph::key("E")
            } else {
                Glyph::Pad(PadButton::RightTrigger)
            }
        }
        Nav::Context => {
            if kb {
                Glyph::key("Tab")
            } else {
                Glyph::Pad(PadButton::North)
            }
        }
        Nav::Start => {
            if kb {
                Glyph::key("Esc")
            } else {
                Glyph::Pad(PadButton::Start)
            }
        }
        Nav::Map => match family {
            GlyphFamily::Keyboard => Glyph::key("M"),
            GlyphFamily::Xbox => Glyph::Pad(PadButton::Select),
            GlyphFamily::PlayStation => Glyph::Pad(PadButton::Touchpad),
        },
    }
}

/// The text on a keycap for a winit key name.
pub fn key_label(name: &str) -> String {
    if let Some(k) = name.strip_prefix("Key") {
        if k.len() == 1 {
            return k.to_string();
        }
    }
    if let Some(d) = name.strip_prefix("Digit") {
        return d.to_string();
    }
    if let Some(d) = name.strip_prefix("Numpad") {
        return format!("Num {d}");
    }
    match name {
        "ShiftLeft" | "ShiftRight" => "Shift".into(),
        "ControlLeft" | "ControlRight" => "Ctrl".into(),
        "AltLeft" | "AltRight" => "Alt".into(),
        "Escape" => "Esc".into(),
        "Backspace" => "Bksp".into(),
        "Enter" | "NumpadEnter" => "Enter".into(),
        "Space" => "Space".into(),
        "CapsLock" => "Caps".into(),
        other => other.to_string(),
    }
}

/// Arrow keys draw a vector arrow instead of text: compass angle of the arrow.
fn key_arrow(name: &str) -> Option<&'static [f32]> {
    match name {
        "ArrowLeft" => Some(&[-PI * 0.5]),
        "ArrowRight" => Some(&[PI * 0.5]),
        "ArrowUp" => Some(&[0.0]),
        "ArrowDown" => Some(&[PI]),
        "↑↓" => Some(&[0.0, PI]),
        "←→" => Some(&[-PI * 0.5, PI * 0.5]),
        _ => None,
    }
}

/// The size of a glyph drawn `height` points tall.
pub fn size(p: &Painter, glyph: &Glyph, height: f32, family: GlyphFamily, theme: &Theme) -> Vec2 {
    let h = height;
    match glyph {
        Glyph::Pad(b) => match b {
            PadButton::South | PadButton::East | PadButton::North | PadButton::West => vec2(h, h),
            PadButton::LeftTrigger | PadButton::RightTrigger => vec2(h * 1.45, h * 0.82),
            PadButton::LeftTrigger2 | PadButton::RightTrigger2 => vec2(h * 1.3, h),
            PadButton::LeftThumb | PadButton::RightThumb => vec2(h, h),
            PadButton::DPadUp | PadButton::DPadDown | PadButton::DPadLeft | PadButton::DPadRight => vec2(h, h),
            PadButton::Start | PadButton::Select => {
                if family == GlyphFamily::Xbox {
                    vec2(h, h)
                } else {
                    vec2(h * 0.75, h)
                }
            }
            PadButton::Touchpad => vec2(h * 1.7, h * 0.85),
        },
        Glyph::LeftStick | Glyph::RightStick | Glyph::DPad | Glyph::DPadHorizontal => vec2(h, h),
        Glyph::Key(name) => {
            if let Some(arrows) = key_arrow(name) {
                return vec2(h * (0.6 + 0.45 * arrows.len() as f32), h);
            }
            let label = key_label(name);
            let w = paint::text_size(p, &label, theme.bold(key_text(h, &label))).x;
            vec2((w + h * 0.5).max(h), h)
        }
        Glyph::MouseLeft | Glyph::MouseRight | Glyph::MouseMiddle | Glyph::MouseWheel => vec2(h * 0.7, h),
        Glyph::MouseMove => vec2(h * 1.1, h),
    }
}

fn key_text(h: f32, label: &str) -> f32 {
    quantize(if label.chars().count() <= 1 { h * 0.58 } else { h * 0.48 })
}

const PS_CROSS: Color32 = Color32::from_rgb(124, 166, 232);
const PS_CIRCLE: Color32 = Color32::from_rgb(234, 104, 118);
const PS_SQUARE: Color32 = Color32::from_rgb(228, 140, 206);
const PS_TRIANGLE: Color32 = Color32::from_rgb(78, 204, 168);
const XB_A: Color32 = Color32::from_rgb(110, 196, 74);
const XB_B: Color32 = Color32::from_rgb(232, 76, 62);
const XB_X: Color32 = Color32::from_rgb(64, 142, 228);
const XB_Y: Color32 = Color32::from_rgb(244, 196, 48);
const PAD_BODY: Color32 = Color32::from_rgb(28, 28, 34);
const PAD_RIM: Color32 = Color32::from_rgb(92, 92, 104);
const PAD_TEXT: Color32 = Color32::from_rgb(238, 236, 230);

/// Draws `glyph` centred at `center`, `height` points tall; returns its rect.
pub fn paint(p: &Painter, center: Pos2, height: f32, glyph: &Glyph, family: GlyphFamily, theme: &Theme) -> Rect {
    let sz = size(p, glyph, height, family, theme);
    let rect = Rect::from_center_size(center, sz);
    let h = height;
    let c = center;
    let rim = Stroke::new((h * 0.05).max(1.2), PAD_RIM);
    let face = |col_body: Color32| {
        p.circle_filled(c + vec2(0.0, h * 0.04), h * 0.5, alpha(Color32::BLACK, 0.45));
        p.circle_filled(c, h * 0.48, col_body);
        p.circle_stroke(c, h * 0.46, rim);
    };
    let label = |s: &str, size: f32, col: Color32, at: Pos2| {
        p.text(at + vec2(0.0, size * 0.04), Align2::CENTER_CENTER, s, theme.bold(quantize(size)), col);
    };
    let xbox = family == GlyphFamily::Xbox;
    match glyph {
        Glyph::Pad(b) => match b {
            PadButton::South | PadButton::East | PadButton::North | PadButton::West => {
                face(PAD_BODY);
                if xbox {
                    let (t, col) = match b {
                        PadButton::South => ("A", XB_A),
                        PadButton::East => ("B", XB_B),
                        PadButton::North => ("Y", XB_Y),
                        _ => ("X", XB_X),
                    };
                    label(t, h * 0.62, col, c);
                } else {
                    let w = (h * 0.1).max(1.8);
                    match b {
                        PadButton::South => paint::cross(p, c, h * 0.42, PS_CROSS, w),
                        PadButton::East => {
                            p.circle_stroke(c, h * 0.22, Stroke::new(w, PS_CIRCLE));
                        }
                        PadButton::North => {
                            let pts = vec![c + vec2(0.0, -0.25) * h, c + vec2(0.23, 0.15) * h, c + vec2(-0.23, 0.15) * h];
                            p.add(PathShape::closed_line(pts, Stroke::new(w, PS_TRIANGLE)));
                        }
                        _ => {
                            p.rect_stroke(
                                Rect::from_center_size(c, Vec2::splat(h * 0.4)),
                                1.0,
                                Stroke::new(w, PS_SQUARE),
                                StrokeKind::Middle,
                            );
                        }
                    }
                }
            }
            PadButton::LeftTrigger | PadButton::RightTrigger => {
                let r = rect;
                p.rect_filled(r.translate(vec2(0.0, h * 0.04)), h * 0.22, alpha(Color32::BLACK, 0.45));
                p.rect_filled(r, h * 0.22, PAD_BODY);
                p.rect_stroke(r.shrink(h * 0.02), h * 0.2, rim, StrokeKind::Inside);
                let left = *b == PadButton::LeftTrigger;
                let t = if xbox {
                    if left { "LB" } else { "RB" }
                } else if left {
                    "L1"
                } else {
                    "R1"
                };
                label(t, h * 0.48, PAD_TEXT, c);
            }
            PadButton::LeftTrigger2 | PadButton::RightTrigger2 => {
                // A trigger: rounded top, flat bottom.
                let r = rect;
                let top = h * 0.42;
                let pts: Vec<Pos2> = paint::arc_points(pos2(c.x, r.top() + top), r.width() * 0.5, -PI * 0.5, PI * 0.5, 16)
                    .into_iter()
                    .map(|q| pos2(q.x, r.top() + top - (r.top() + top - q.y) * (top / (r.width() * 0.5))))
                    .chain([r.right_bottom() - vec2(h * 0.06, 0.0), r.left_bottom() + vec2(h * 0.06, 0.0)])
                    .collect();
                let shadow: Vec<Pos2> = pts.iter().map(|q| *q + vec2(0.0, h * 0.04)).collect();
                paint::fill_convex(p, shadow, alpha(Color32::BLACK, 0.45));
                p.add(Shape::convex_polygon(pts, PAD_BODY, rim));
                let left = *b == PadButton::LeftTrigger2;
                let t = if xbox {
                    if left { "LT" } else { "RT" }
                } else if left {
                    "L2"
                } else {
                    "R2"
                };
                label(t, h * 0.46, PAD_TEXT, c + vec2(0.0, h * 0.06));
            }
            PadButton::LeftThumb | PadButton::RightThumb => {
                stick(p, c, h, true);
                let left = *b == PadButton::LeftThumb;
                let t = if xbox {
                    if left { "LS" } else { "RS" }
                } else if left {
                    "L3"
                } else {
                    "R3"
                };
                label(t, h * 0.4, PAD_TEXT, c);
            }
            PadButton::DPadUp => dpad(p, c, h, &[0]),
            PadButton::DPadRight => dpad(p, c, h, &[1]),
            PadButton::DPadDown => dpad(p, c, h, &[2]),
            PadButton::DPadLeft => dpad(p, c, h, &[3]),
            PadButton::Start | PadButton::Select => {
                let start = *b == PadButton::Start;
                if xbox {
                    face(PAD_BODY);
                    if start {
                        paint::hamburger(p, Rect::from_center_size(c, Vec2::splat(h * 0.62)), PAD_TEXT);
                    } else {
                        let a = Rect::from_center_size(c + vec2(-0.06, -0.05) * h, Vec2::splat(h * 0.26));
                        let b2 = Rect::from_center_size(c + vec2(0.06, 0.05) * h, Vec2::splat(h * 0.26));
                        p.rect_stroke(a, 2.0, Stroke::new(h * 0.05, PAD_TEXT), StrokeKind::Middle);
                        p.rect_filled(b2, 2.0, PAD_BODY);
                        p.rect_stroke(b2, 2.0, Stroke::new(h * 0.05, PAD_TEXT), StrokeKind::Middle);
                    }
                } else {
                    // A small pill (Options / Create).
                    let r = Rect::from_center_size(c, vec2(h * 0.5, h * 0.86));
                    p.rect_filled(r.translate(vec2(0.0, h * 0.04)), h * 0.25, alpha(Color32::BLACK, 0.45));
                    p.rect_filled(r, h * 0.25, PAD_BODY);
                    p.rect_stroke(r.shrink(h * 0.02), h * 0.23, rim, StrokeKind::Inside);
                    if start {
                        paint::hamburger(p, Rect::from_center_size(c, vec2(h * 0.46, h * 0.5)), PAD_TEXT);
                    } else {
                        // Create: a little burst.
                        for k in 0..3 {
                            let a = -0.6 + k as f32 * 0.6;
                            let o = paint::polar(c + vec2(0.0, h * 0.12), h * 0.1, a);
                            let e = paint::polar(c + vec2(0.0, h * 0.12), h * 0.28, a);
                            p.line_segment([o, e], Stroke::new(h * 0.05, PAD_TEXT));
                        }
                    }
                }
            }
            PadButton::Touchpad => {
                let r = rect;
                p.rect_filled(r.translate(vec2(0.0, h * 0.04)), h * 0.18, alpha(Color32::BLACK, 0.45));
                p.rect_filled(
                    r,
                    egui::CornerRadius { nw: (h * 0.12) as u8, ne: (h * 0.12) as u8, sw: (h * 0.32) as u8, se: (h * 0.32) as u8 },
                    PAD_BODY,
                );
                p.rect_stroke(r.shrink(h * 0.02), (h * 0.12) as u8, rim, StrokeKind::Inside);
                p.circle_filled(c + vec2(0.0, h * 0.05), h * 0.09, PAD_TEXT);
            }
        },
        Glyph::LeftStick | Glyph::RightStick => {
            stick(p, c, h, false);
            let t = if *glyph == Glyph::LeftStick { "L" } else { "R" };
            label(t, h * 0.42, PAD_TEXT, c);
            // Little arrows around: "move".
            for k in 0..4 {
                let a = k as f32 * PI * 0.5;
                paint::arrow_head(p, paint::polar(c, h * 0.4, a), h * 0.12, a, PAD_TEXT);
            }
        }
        Glyph::DPad => dpad(p, c, h, &[0, 2]),
        Glyph::DPadHorizontal => dpad(p, c, h, &[1, 3]),
        Glyph::Key(name) => {
            let r = rect;
            let cap = Color32::from_rgb(236, 228, 210);
            let edge = Color32::from_rgb(150, 136, 110);
            p.rect_filled(r.translate(vec2(0.0, h * 0.05)), h * 0.16, alpha(Color32::BLACK, 0.45));
            p.rect_filled(r, h * 0.16, edge);
            let top = Rect::from_min_max(r.min + vec2(h * 0.05, h * 0.04), r.max - vec2(h * 0.05, h * 0.13));
            p.rect_filled(top, h * 0.12, cap);
            let tc = top.center();
            if let Some(arrows) = key_arrow(name) {
                let n = arrows.len() as f32;
                for (i, a) in arrows.iter().enumerate() {
                    let x = tc.x + (i as f32 - (n - 1.0) * 0.5) * h * 0.42;
                    paint::arrow_head(p, pos2(x, tc.y), h * 0.34, *a, theme.ink);
                }
            } else {
                let l = key_label(name);
                p.text(tc + vec2(0.0, h * 0.02), Align2::CENTER_CENTER, l.as_str(), theme.bold(key_text(h, &l)), theme.ink);
            }
        }
        Glyph::MouseLeft | Glyph::MouseRight | Glyph::MouseMiddle | Glyph::MouseWheel | Glyph::MouseMove => {
            let mc = if *glyph == Glyph::MouseMove { c + vec2(-h * 0.2, 0.0) } else { c };
            let body = Rect::from_center_size(mc, vec2(h * 0.62, h * 0.94));
            let rr = h * 0.3;
            let cap = Color32::from_rgb(236, 228, 210);
            p.rect_filled(body.translate(vec2(0.0, h * 0.04)), rr, alpha(Color32::BLACK, 0.45));
            p.rect_filled(body, rr, cap);
            let split = body.top() + body.height() * 0.42;
            let hl = theme.amber;
            match glyph {
                Glyph::MouseLeft => {
                    let r = Rect::from_min_max(body.min, pos2(body.center().x, split));
                    p.rect_filled(r, egui::CornerRadius { nw: rr as u8, ..Default::default() }, hl);
                }
                Glyph::MouseRight => {
                    let r = Rect::from_min_max(pos2(body.center().x, body.top()), pos2(body.right(), split));
                    p.rect_filled(r, egui::CornerRadius { ne: rr as u8, ..Default::default() }, hl);
                }
                _ => {}
            }
            p.rect_stroke(body, rr, Stroke::new(h * 0.05, theme.ink), StrokeKind::Inside);
            p.line_segment(
                [pos2(body.left() + h * 0.03, split), pos2(body.right() - h * 0.03, split)],
                Stroke::new(h * 0.04, theme.ink),
            );
            p.line_segment([pos2(body.center().x, body.top()), pos2(body.center().x, split)], Stroke::new(h * 0.04, theme.ink));
            let wheel =
                Rect::from_center_size(pos2(body.center().x, body.top() + body.height() * 0.24), vec2(h * 0.12, h * 0.22));
            let wheel_col = if matches!(glyph, Glyph::MouseMiddle | Glyph::MouseWheel) { hl } else { theme.ink_soft };
            p.rect_filled(wheel, h * 0.06, wheel_col);
            if *glyph == Glyph::MouseWheel {
                paint::arrow_head(p, wheel.center_top() - vec2(0.0, h * 0.1), h * 0.14, 0.0, theme.ink);
                paint::arrow_head(p, wheel.center_bottom() + vec2(0.0, h * 0.1), h * 0.14, PI, theme.ink);
            }
            if *glyph == Glyph::MouseMove {
                let ac = c + vec2(h * 0.36, 0.0);
                for k in 0..4 {
                    let a = k as f32 * PI * 0.5;
                    paint::arrow_head(p, paint::polar(ac, h * 0.18, a), h * 0.16, a, theme.parchment);
                }
            }
        }
    }
    rect
}

/// A stick: a ringed disc (`pressed` adds a bold ring for L3/R3).
fn stick(p: &Painter, c: Pos2, h: f32, pressed: bool) {
    p.circle_filled(c + vec2(0.0, h * 0.04), h * 0.5, alpha(Color32::BLACK, 0.45));
    p.circle_filled(c, h * 0.48, PAD_BODY);
    p.circle_stroke(c, h * 0.46, Stroke::new(h * 0.05, PAD_RIM));
    p.circle_stroke(c, h * 0.3, Stroke::new(h * if pressed { 0.07 } else { 0.035 }, if pressed { PAD_TEXT } else { PAD_RIM }));
}

/// A D-pad with the arms `lit` (0 up, 1 right, 2 down, 3 left) highlighted.
fn dpad(p: &Painter, c: Pos2, h: f32, lit: &[usize]) {
    let a = h * 0.17; // half arm width
    let l = h * 0.48; // arm length from centre
    let cross = vec![
        c + vec2(-a, -l),
        c + vec2(a, -l),
        c + vec2(a, -a),
        c + vec2(l, -a),
        c + vec2(l, a),
        c + vec2(a, a),
        c + vec2(a, l),
        c + vec2(-a, l),
        c + vec2(-a, a),
        c + vec2(-l, a),
        c + vec2(-l, -a),
        c + vec2(-a, -a),
    ];
    let shadow: Vec<Pos2> = cross.iter().map(|q| *q + vec2(0.0, h * 0.04)).collect();
    paint::fill_poly(p, &shadow, alpha(Color32::BLACK, 0.45));
    paint::fill_poly(p, &cross, PAD_BODY);
    p.add(PathShape::closed_line(cross, Stroke::new(h * 0.04, PAD_RIM)));
    for k in 0..4 {
        let ang = k as f32 * PI * 0.5;
        let col = if lit.contains(&k) { PAD_TEXT } else { alpha(PAD_RIM, 0.9) };
        paint::arrow_head(p, paint::polar(c, h * 0.3, ang), h * 0.2, ang, col);
    }
    let _ = PathStroke::NONE;
}

/// A row of glyphs and text drawn left to right from `left_center` (its vertical centre).
/// Returns the row's rect.
#[derive(Clone, Debug, PartialEq)]
pub enum Piece {
    Text(String),
    Glyph(Glyph),
    /// A gap in points.
    Gap(f32),
}

/// Measures a row of pieces at text size `font_size` and glyph height `glyph_h`.
pub fn row_width(p: &Painter, pieces: &[Piece], font: &egui::FontId, glyph_h: f32, family: GlyphFamily, theme: &Theme) -> f32 {
    let gap = font.size * 0.3;
    let mut w = 0.0;
    for (i, piece) in pieces.iter().enumerate() {
        if i > 0 && !matches!(piece, Piece::Gap(_)) && !matches!(pieces[i - 1], Piece::Gap(_)) {
            w += gap;
        }
        w += match piece {
            Piece::Text(t) => paint::text_size(p, t, font.clone()).x,
            Piece::Glyph(g) => size(p, g, glyph_h, family, theme).x,
            Piece::Gap(g) => *g,
        };
    }
    w
}

/// Draws a row of pieces; `anchor` is placed per `align` (horizontal part used; vertical is
/// the row's centre line). Returns the rect.
pub fn paint_row(
    p: &Painter,
    anchor: Pos2,
    align: Align2,
    pieces: &[Piece],
    font: egui::FontId,
    color: Color32,
    glyph_h: f32,
    family: GlyphFamily,
    theme: &Theme,
) -> Rect {
    let total = row_width(p, pieces, &font, glyph_h, family, theme);
    let x0 = match align.x() {
        egui::Align::Min => anchor.x,
        egui::Align::Center => anchor.x - total * 0.5,
        egui::Align::Max => anchor.x - total,
    };
    let gap = font.size * 0.3;
    let mut x = x0;
    let y = anchor.y;
    for (i, piece) in pieces.iter().enumerate() {
        if i > 0 && !matches!(piece, Piece::Gap(_)) && !matches!(pieces[i - 1], Piece::Gap(_)) {
            x += gap;
        }
        match piece {
            Piece::Text(t) => {
                let r = p.text(pos2(x, y), Align2::LEFT_CENTER, t, font.clone(), color);
                x = r.right();
            }
            Piece::Glyph(g) => {
                let s = size(p, g, glyph_h, family, theme);
                paint(p, pos2(x + s.x * 0.5, y), glyph_h, g, family, theme);
                x += s.x;
            }
            Piece::Gap(g) => x += *g,
        }
    }
    Rect::from_min_max(pos2(x0, y - glyph_h.max(font.size) * 0.5), pos2(x, y + glyph_h.max(font.size) * 0.5))
}
