//! # pav_ui — the controller-first "10-foot" UI kit
//!
//! Theme and fonts, controller glyphs, settings, the front-end state machine (title, menus,
//! settings, credits, pause, attract mode, mobile overlay), the HUD drawn from
//! [`pav_core::play::UiModel`], and a CPU rasteriser so tools capture the UI headlessly.
//! No window, GPU or clock: the app gives it egui contexts, input and `dt`.
//!
//! ## Coordinates
//! Everything is designed in points at 1080p. Set
//! `ctx.set_pixels_per_point(screen_height_px / 1080.0)`: the screen is then
//! (1080 × aspect) × 1080 points and the UI scales with screen height. A 5% safe-area margin is
//! respected ([`Theme::safe_area`]). Pointer positions in [`input::UiInput`] are in points.
//!
//! ## Driving it from the app, every frame
//! ```ignore
//! // Once:
//! let theme = Theme::from_toml(&info.theme)?;              // games/blacktap/ui/theme.toml
//! let bindings = Bindings::from_toml(&info.bindings)?;     // games/blacktap/bindings.toml
//! pav_core::input::buttons::register_names(&names_as_str(bindings.names()));
//! pav_ui::theme::install(&egui_ctx, &theme);
//! let (settings, err) = Settings::load(&store.get("blacktap_settings").unwrap_or_default());
//! let mut fe = FrontEnd::new(info, settings, Platform { mobile, kiosk, can_quit }, has_progress);
//!
//! // Each frame:
//! let input = UiInput { nav, any, pointer, pointer_moved, click, back_click, stick, zoom,
//!                       device, touch, audio_locked };     // see input.rs for the mapping
//! let frame: Option<&PlayFrame> = latest_frame.as_ref();
//! for cmd in fe.update(dt, &input, frame) {
//!     match cmd {
//!         Command::LoadScene { scene, seed, autopilot } => { /* build the scene; autopilot =
//!             the sim host plays Sim::bot_input() (attract). Call fe.on_scene_loaded() when
//!             ready. */ }
//!         Command::SetPaused(p) => host.pause(p),
//!         Command::RestartCheckpoint => host.restore_checkpoint(),
//!         Command::Quit => exit(),
//!         Command::SaveSettings => store.set("blacktap_settings", &fe.settings().to_json()),
//!         Command::SettingsChanged => apply(fe.settings()),   // audio buses, quality...
//!         Command::Sound(name) => audio.ui(&name),            // "ui_move", "ui_confirm"...
//!         Command::UnlockAudio => audio.unlock(),
//!         Command::ToggleSound(on) => audio.mute(!on),
//!         Command::StartGame => continue_next = true,         // the next LoadScene is a
//!                                                             // Continue: restore progress
//!         Command::NewGame => progress.reset(),               // before the new game's LoadScene
//!     }
//! }
//! let game_gets_input = fe.game_input();                  // only in Play
//! // Inside the egui pass:
//! let env = HudEnv { theme: &theme, bindings: &bindings, family: input.device,
//!                    settings: fe.settings(), proj: Projector { view_proj, size: screen_pts },
//!                    time: clock };
//! fe.draw(&egui_ctx, frame, &env);                        // HUD + current screen
//! ```
//! `FrontEnd::draw` must follow `update` each frame: it lays out the focusable items whose
//! rectangles the next `update` uses for hover and click.
//!
//! Contract details:
//! - The first `update` emits the `LoadScene` for the starting screen (the title backdrop, or
//!   the first attract segment): the app needn't load anything by itself. Every later scene
//!   change is a `LoadScene` too (title after attract or Quit to Title, chapters, training).
//! - `dt` is real wall-clock time; large steps (2 s at 0.5 fps) are fine: every timer
//!   accumulates (idle → attract 10 s + 0.5 s fade, kiosk 60 s, segment time, 3 s card).
//! - Attract segments count time only after [`FrontEnd::on_scene_loaded`] (a 30 s watchdog
//!   carries on if it never comes). A segment ends on `frame.ui.demo.done` or its `seconds`.
//! - `UiInput.any` from a real device in attract mode returns to the title on that same
//!   update, consumed. On a phone (`Platform::mobile`) touch never exits; a keyboard or pad
//!   does, and the front end then behaves as desktop.
//! - Esc should send `[Nav::Back, Nav::Start]`: after an input changes the screen the rest
//!   of that frame's input is dropped, so it acts once (pause in play, back elsewhere).
//! - In `Play`, only `Nav::Start` is used (pause); everything else is game input. The game's
//!   own overlays (`UiModel::overlay`) are drawn by the HUD and driven by the game.
//! - `UiRequest::Title` / `Replay` from the game are handled; `UiRequest::Save` is the app's.
//! - [`FrontEnd::mode`]`().name()` gives snake_case names ("mobile_menu", "attract_card");
//!   [`FrontEnd::menu_button`] is the ☰ button's centre on phones; [`FrontEnd::focus_id`] /
//!   [`FrontEnd::focusables`] describe focus for tests and tools; [`FrontEnd::family`] is the
//!   glyph family in use (the prompts setting, else the last device).
//! - Layers: the HUD and screens paint on one `Order::Background` egui layer
//!   ([`hud::layer`]), so app debug windows stay on top.
//!
//! ## Headless
//! [`raster::render`] runs an egui frame and rasterises it on the CPU (RGBA8);
//! [`raster::encode_png`] writes it. `FrontEnd::show(mode)` jumps to any screen for captures.
//! [`sample_ui_model`] and friends give rich sample data for galleries.

pub mod bindings;
pub mod frontend;
pub mod glyph;
pub mod hud;
pub mod input;
pub mod map;
pub mod paint;
pub mod raster;
pub mod settings;
pub mod theme;

mod samples;

pub use bindings::{Action, Bindings, PadButton};
pub use frontend::{Command, FrontEnd, Mode, Platform};
pub use glyph::{Glyph, GlyphFamily};
pub use hud::HudEnv;
pub use input::{Nav, UiInput};
pub use samples::{sample_front_end_info, sample_overlays, sample_projector, sample_ui_model};
pub use settings::Settings;
pub use theme::Theme;

use glam::{Mat4, Vec3, Vec4};

/// World → screen points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Projector {
    /// The camera's projection × view (wgpu clip space: x, y in -1..1, y up).
    pub view_proj: Mat4,
    /// The screen size in points.
    pub size: egui::Vec2,
}

impl Default for Projector {
    fn default() -> Self {
        Self { view_proj: Mat4::IDENTITY, size: egui::vec2(1920.0, 1080.0) }
    }
}

impl Projector {
    pub fn new(view_proj: Mat4, size: egui::Vec2) -> Self {
        Self { view_proj, size }
    }

    /// A perspective camera at `eye` looking at `target` (vertical fov in degrees).
    pub fn look_at(eye: Vec3, target: Vec3, fov_deg: f32, size: egui::Vec2) -> Self {
        let view = glam::camera::rh::view::look_at_mat4(eye, target, Vec3::Y);
        let proj = glam::camera::rh::proj::directx::perspective(fov_deg.to_radians(), size.x / size.y.max(1.0), 0.1, 3000.0);
        Self { view_proj: proj * view, size }
    }

    fn clip(&self, p: Vec3) -> Vec4 {
        self.view_proj * p.extend(1.0)
    }

    /// The screen point of a world position, or None behind the camera.
    pub fn project(&self, p: Vec3) -> Option<egui::Pos2> {
        let c = self.clip(p);
        if c.w <= 1e-4 {
            return None;
        }
        let ndc = c.truncate() / c.w;
        Some(egui::pos2((ndc.x * 0.5 + 0.5) * self.size.x, (0.5 - ndc.y * 0.5) * self.size.y))
    }

    /// The direction on screen (from the centre, y down) toward a world position, also for
    /// points behind the camera (off-screen pips). None if it is exactly at the eye.
    pub fn screen_direction(&self, p: Vec3) -> Option<egui::Vec2> {
        let c = self.clip(p);
        let w = c.w.abs().max(1e-4);
        let mut d = egui::vec2(c.x / w * self.size.x, -c.y / w * self.size.y);
        if c.w <= 1e-4 {
            // Behind: push the pip to the bottom edge side it is on.
            d = egui::vec2(d.x, d.y.abs().max(self.size.y));
        }
        (d.length() > 1e-6).then_some(d)
    }

    /// The camera position, recovered from the matrix (None for orthographic cameras).
    pub fn eye(&self) -> Option<Vec3> {
        let m = self.view_proj;
        let r = |i: usize| m.row(i);
        let (r0, r1, r3) = (r(0), r(1), r(3));
        let a = glam::Mat3::from_cols(
            glam::Vec3::new(r0.x, r1.x, r3.x),
            glam::Vec3::new(r0.y, r1.y, r3.y),
            glam::Vec3::new(r0.z, r1.z, r3.z),
        );
        if a.determinant().abs() < 1e-9 {
            return None;
        }
        let b = -glam::Vec3::new(r0.w, r1.w, r3.w);
        Some(a.inverse() * b)
    }

    /// Distance from the camera to a world point (for marker labels).
    pub fn distance_hint(&self, p: Vec3) -> Option<f32> {
        self.eye().map(|e| e.distance(p))
    }
}
