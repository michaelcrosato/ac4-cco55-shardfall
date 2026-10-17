//! What the front end needs from the devices each frame ([`UiInput`]), filled by the app.
//!
//! The app maps devices to [`Nav`] events with PLAN §10.3:
//!
//! | Nav | DualSense / Xbox | Keyboard / mouse |
//! |---|---|---|
//! | Up Down Left Right | D-pad, left stick (repeat: [`NavRepeat`]) | arrows, W A S D |
//! | Confirm | ✕ / A | Enter, Space (left click: `click`) |
//! | Back | ◯ / B | Esc, Backspace (right click: `back_click`) |
//! | TabPrev / TabNext | L1 / R1 (LB / RB) | Q / E |
//! | Context | △ / Y | Tab |
//! | Start | Options / Menu | Esc (in play: pause; in the pause menu: close) |
//! | Map | Touchpad / View | M |
//!
//! Esc sends both `Back` and `Start`: the front end uses whichever the screen needs (Start
//! opens and closes the pause menu; Back leaves other screens).
//!
//! `any` is true for any key, mouse button, wheel notch, mouse move over 8 px, pad button,
//! stick past 0.35, trigger past 0.2 or (desktop) touch: it exits attract mode and resets the
//! idle timers. `pointer` is in egui points (physical pixels / pixels_per_point).

use egui::Pos2;
use glam::Vec2;

use crate::glyph::GlyphFamily;

/// A menu navigation event.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Nav {
    Up,
    Down,
    Left,
    Right,
    Confirm,
    Back,
    TabPrev,
    TabNext,
    Context,
    Start,
    Map,
}

/// One frame of UI input.
#[derive(Clone, Debug, PartialEq)]
pub struct UiInput {
    /// Navigation events this frame, in order (key repeat already applied).
    pub nav: Vec<Nav>,
    /// Any input at all this frame (see the module docs).
    pub any: bool,
    /// The mouse or touch position, in points.
    pub pointer: Option<Pos2>,
    /// The pointer moved this frame (hover moves the focus).
    pub pointer_moved: bool,
    /// Left click or tap released this frame (confirms what is under the pointer).
    pub click: bool,
    /// Right click this frame (Back).
    pub back_click: bool,
    /// Left stick (x right, y up), dead zone applied: pans the map.
    pub stick: Vec2,
    /// Map zoom this frame in steps: +1 per wheel notch in, -1 out; triggers give
    /// (R2 - L2) × dt × 4. The map multiplies its scale by 1.25^zoom.
    pub zoom: f32,
    /// The device family that produced the last input (glyphs follow it).
    pub device: GlyphFamily,
    /// This frame's input came from a touch screen (mobile: doesn't exit attract mode).
    pub touch: bool,
    /// The browser hasn't allowed audio yet (shows "press any key to enable sound").
    pub audio_locked: bool,
}

impl Default for UiInput {
    fn default() -> Self {
        Self {
            nav: Vec::new(),
            any: false,
            pointer: None,
            pointer_moved: false,
            click: false,
            back_click: false,
            stick: Vec2::ZERO,
            zoom: 0.0,
            device: GlyphFamily::PlayStation,
            touch: false,
            audio_locked: false,
        }
    }
}

impl UiInput {
    /// One navigation event from a pad (sets `any`).
    pub fn press(nav: Nav) -> Self {
        Self { nav: vec![nav], any: true, ..Default::default() }
    }

    /// A key press that isn't a navigation key (sets `any` only), from the keyboard.
    pub fn any_key() -> Self {
        Self { any: true, device: GlyphFamily::Keyboard, ..Default::default() }
    }

    /// A tap at `pos` on a touch screen.
    pub fn tap(pos: Pos2) -> Self {
        Self { any: true, pointer: Some(pos), pointer_moved: true, click: true, touch: true, ..Default::default() }
    }

    /// A mouse click at `pos`.
    pub fn click_at(pos: Pos2) -> Self {
        Self {
            any: true,
            pointer: Some(pos),
            pointer_moved: true,
            click: true,
            device: GlyphFamily::Keyboard,
            ..Default::default()
        }
    }

    /// Whether `nav` happened this frame.
    pub fn has(&self, nav: Nav) -> bool {
        self.nav.contains(&nav)
    }
}

/// Turns held directions (D-pad, stick, arrow keys) into repeated [`Nav`] events: one on
/// press, then after 350 ms one every 90 ms (PLAN §10.3).
#[derive(Clone, Debug, Default)]
pub struct NavRepeat {
    held: Option<Nav>,
    timer: f32,
}

impl NavRepeat {
    pub const FIRST: f32 = 0.35;
    pub const NEXT: f32 = 0.09;

    /// `held` is the direction held now (None when released); pushes events into `out`.
    pub fn update(&mut self, dt: f32, held: Option<Nav>, out: &mut Vec<Nav>) {
        match (self.held, held) {
            (_, None) => {
                self.held = None;
            }
            (prev, Some(n)) if prev != Some(n) => {
                self.held = Some(n);
                self.timer = Self::FIRST;
                out.push(n);
            }
            (_, Some(n)) => {
                self.timer -= dt;
                // At most a few repeats per frame, even on very slow frames.
                let mut k = 0;
                while self.timer <= 0.0 && k < 3 {
                    out.push(n);
                    self.timer += Self::NEXT;
                    k += 1;
                }
                if self.timer <= 0.0 {
                    self.timer = Self::NEXT;
                }
            }
        }
    }

    /// The direction of a stick (x right, y up) past `dead` (0.5 suits menus).
    pub fn stick_direction(v: Vec2, dead: f32) -> Option<Nav> {
        if v.length() < dead {
            return None;
        }
        Some(if v.x.abs() > v.y.abs() {
            if v.x > 0.0 { Nav::Right } else { Nav::Left }
        } else if v.y > 0.0 {
            Nav::Up
        } else {
            Nav::Down
        })
    }
}
