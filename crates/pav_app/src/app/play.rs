//! The game's own front end in the window: when the start scene is a game with a front end
//! (`Entry::front_end`) and the app is not in developer mode (`--dev`, `?dev=1`), the window
//! runs this path instead of the engine demo's: title, menus, settings and pause from
//! `pav_ui::frontend`, attract mode (the sim host's autopilot plays the game's bot), the HUD
//! from the game's `UiModel`, the game's camera, its music and ambience, and its bindings
//! (`bindings.toml`) for keyboard, mouse and gamepad.

use std::collections::BTreeSet;

use glam::{Vec2, Vec3};
use pav_core::play::{CameraHint, FrontEndInfo, PlayFrame};
use pav_core::{InputFrame, Sim};
use pav_ui::bindings::{Bindings, PadButton};
use pav_ui::frontend::{Command, FrontEnd, Mode, Platform};
use pav_ui::glyph::GlyphFamily;
use pav_ui::input::{Nav, UiInput as UiIn};
use pav_ui::settings::Settings as UiSettings;
use pav_ui::theme::Theme;
use web_time::Instant;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{KeyCode, PhysicalKey};

use super::App;
use crate::input::key_from_name;

/// Which device the player last used (prompts show its glyphs).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Dev {
    Keyboard,
    Pad,
    Touch,
}

/// The front end's state in the window.
pub(crate) struct PlayApp {
    fe: FrontEnd,
    info: FrontEndInfo,
    theme: Theme,
    bindings: Bindings,
    store: crate::store::AppStore,
    save_key: String,
    /// The game's last camera: the move stick turns relative to it.
    last_hint: Option<CameraHint>,
    dev: Dev,
    family: GlyphFamily,
    /// Keys down now, and keys and buttons pressed since the last frame.
    keys: BTreeSet<KeyCode>,
    key_taps: Vec<KeyCode>,
    mouse: BTreeSet<u8>,
    mouse_taps: Vec<u8>,
    wheel: f32,
    /// Mouse movement since the last frame (px), and the point "any input" measures from.
    mouse_delta: Vec2,
    cursor: Vec2,
    any_anchor: Option<Vec2>,
    touch_any: bool,
    /// Pad buttons held last frame, the stick direction held for menu repeat and its timer.
    pad_prev: BTreeSet<PadButton>,
    stick_dir: Option<Nav>,
    stick_timer: f32,
    prev_held: u32,
    audio_locked: bool,
    banks_loaded: bool,
    mobile: bool,
    started: bool,
    last: Instant,
    /// Seconds since the scene in the sim was requested (the title fades in after a cut).
    since_load: f32,
    /// Remembered progress (game JSON).
    progress: Option<serde_json::Value>,
    /// Raw mouse motion (pointer locked) since the last frame, and whether the pointer is ours.
    mouse_raw: Vec2,
    grabbed: bool,
    theme_installed: bool,
    /// The mode last published to the page / log (browser checks read it).
    published: String,
    /// Wall-clock seconds since the last frame, uncapped (idle timers count real time even
    /// when frames are slow, as on SwiftShader).
    wall_dt: f32,
}

/// `MobileMenu` → `mobile_menu`.
fn snake(mode: Mode) -> String {
    let mut out = String::new();
    for (i, c) in format!("{mode:?}").chars().enumerate() {
        if c.is_ascii_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// Tells the page (and the log) which screen the front end shows.
fn publish_mode(name: &str, menu_button: Option<(f32, f32)>) {
    log::info!("frontend: {name}");
    #[cfg(target_arch = "wasm32")]
    if let Some(body) = web_sys::window().and_then(|w| w.document()).and_then(|d| d.body()) {
        let _ = body.set_attribute("data-frontend", name);
        if let Some((x, y)) = menu_button {
            let _ = body.set_attribute("data-menu-x", &format!("{x:.0}"));
            let _ = body.set_attribute("data-menu-y", &format!("{y:.0}"));
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    let _ = menu_button;
}

const NAV_KEYS: &[(KeyCode, Nav)] = &[
    (KeyCode::ArrowUp, Nav::Up),
    (KeyCode::KeyW, Nav::Up),
    (KeyCode::ArrowDown, Nav::Down),
    (KeyCode::KeyS, Nav::Down),
    (KeyCode::ArrowLeft, Nav::Left),
    (KeyCode::KeyA, Nav::Left),
    (KeyCode::ArrowRight, Nav::Right),
    (KeyCode::KeyD, Nav::Right),
    (KeyCode::Enter, Nav::Confirm),
    (KeyCode::Space, Nav::Confirm),
    (KeyCode::NumpadEnter, Nav::Confirm),
    (KeyCode::Backspace, Nav::Back),
    (KeyCode::KeyQ, Nav::TabPrev),
    (KeyCode::KeyE, Nav::TabNext),
    (KeyCode::Tab, Nav::Context),
    (KeyCode::KeyM, Nav::Map),
];

fn mouse_code(b: MouseButton) -> u8 {
    match b {
        MouseButton::Left => 0,
        MouseButton::Right => 1,
        MouseButton::Middle => 2,
        _ => 3,
    }
}

fn mouse_name(n: &str) -> Option<u8> {
    match n {
        "Left" => Some(0),
        "Right" => Some(1),
        "Middle" => Some(2),
        _ => None,
    }
}

fn gilrs_button(b: PadButton) -> Option<gilrs::Button> {
    use gilrs::Button as G;
    Some(match b {
        PadButton::South => G::South,
        PadButton::East => G::East,
        PadButton::North => G::North,
        PadButton::West => G::West,
        PadButton::LeftTrigger => G::LeftTrigger,
        PadButton::RightTrigger => G::RightTrigger,
        PadButton::LeftTrigger2 => G::LeftTrigger2,
        PadButton::RightTrigger2 => G::RightTrigger2,
        PadButton::LeftThumb => G::LeftThumb,
        PadButton::RightThumb => G::RightThumb,
        PadButton::DPadUp => G::DPadUp,
        PadButton::DPadDown => G::DPadDown,
        PadButton::DPadLeft => G::DPadLeft,
        PadButton::DPadRight => G::DPadRight,
        PadButton::Start => G::Start,
        PadButton::Select => G::Select,
        PadButton::Touchpad => G::Mode,
    })
}

const ALL_PAD: [PadButton; 17] = [
    PadButton::South,
    PadButton::East,
    PadButton::North,
    PadButton::West,
    PadButton::LeftTrigger,
    PadButton::RightTrigger,
    PadButton::LeftTrigger2,
    PadButton::RightTrigger2,
    PadButton::LeftThumb,
    PadButton::RightThumb,
    PadButton::DPadUp,
    PadButton::DPadDown,
    PadButton::DPadLeft,
    PadButton::DPadRight,
    PadButton::Start,
    PadButton::Select,
    PadButton::Touchpad,
];

/// One frame of the gamepad.
#[derive(Default)]
struct PadNow {
    held: BTreeSet<PadButton>,
    left: Vec2,
    right: Vec2,
    triggers: [f32; 2],
    name: Option<String>,
    any: bool,
}

fn deadzone(v: Vec2, dz: f32) -> Vec2 {
    let l = v.length();
    if l < dz { Vec2::ZERO } else { v / l * ((l - dz) / (1.0 - dz)).min(1.0) }
}

impl PlayApp {
    /// The front end for the start scene's game, unless in developer mode or the scene has none.
    pub(crate) fn for_scene(scene: &str, dev: bool, kiosk: bool) -> Option<Self> {
        if dev {
            return None;
        }
        let key = scene.split('/').next().unwrap_or(scene);
        let info = (pav_core::games::find(key)?.front_end?)();
        let theme = Theme::from_toml(&info.theme).unwrap_or_else(|e| {
            log::warn!("ui theme: {e:#}");
            Theme::default()
        });
        let bindings = Bindings::from_toml(&info.bindings).unwrap_or_else(|e| {
            log::warn!("bindings: {e:#}");
            Bindings::default()
        });
        let store = crate::store::AppStore;
        let save_key = if info.save_key.is_empty() { key.to_string() } else { info.save_key.clone() };
        use pav_core::persist::Store;
        let (settings, err) = match store.get(&format!("{save_key}_settings")) {
            Some(text) => UiSettings::load(&text),
            None => (UiSettings::default(), None),
        };
        if let Some(e) = err {
            log::warn!("settings were reset: {e}");
        }
        let progress = store.get(&format!("{save_key}_progress")).and_then(|t| serde_json::from_str(&t).ok());
        let mobile = crate::platform::touch_screen();
        let platform = Platform { mobile, kiosk, can_quit: cfg!(not(target_arch = "wasm32")) };
        let fe = FrontEnd::new(info.clone(), settings, platform, progress.is_some());
        Some(Self {
            fe,
            info,
            theme,
            bindings,
            store,
            save_key,
            last_hint: None,
            dev: if mobile { Dev::Touch } else { Dev::Keyboard },
            family: GlyphFamily::Keyboard,
            keys: BTreeSet::new(),
            key_taps: Vec::new(),
            mouse: BTreeSet::new(),
            mouse_taps: Vec::new(),
            wheel: 0.0,
            mouse_delta: Vec2::ZERO,
            cursor: Vec2::ZERO,
            any_anchor: None,
            touch_any: false,
            pad_prev: BTreeSet::new(),
            stick_dir: None,
            stick_timer: 0.0,
            prev_held: 0,
            audio_locked: cfg!(target_arch = "wasm32"),
            banks_loaded: false,
            mobile,
            started: false,
            last: Instant::now(),
            since_load: 0.0,
            progress,
            mouse_raw: Vec2::ZERO,
            grabbed: false,
            theme_installed: false,
            published: String::new(),
            wall_dt: 0.0,
        })
    }

    /// Raw mouse motion (device events): turns the camera while the pointer is locked.
    pub(crate) fn mouse_motion(&mut self, d: Vec2) {
        if self.grabbed {
            self.mouse_raw += d;
        }
    }

    /// The scene the window starts in (the title backdrop, or attract on phones and kiosks).
    pub(crate) fn start_scene(&self) -> String {
        self.info.title_scene.clone()
    }

    pub(crate) fn window_title(&self) -> String {
        format!("{} — {}", self.info.title, self.info.tagline)
    }

    fn save_settings(&mut self) {
        use pav_core::persist::Store;
        if let Ok(text) = serde_json::to_string(self.fe.settings()) {
            self.store.set(&format!("{}_settings", self.save_key), &text);
        }
    }

    fn save_progress(&mut self, p: &serde_json::Value) {
        use pav_core::persist::Store;
        self.progress = Some(p.clone());
        self.store.set(&format!("{}_progress", self.save_key), &p.to_string());
    }

    /// The payload a freshly built game scene gets (`Game::restore`).
    fn restore_payload(&self, demo: bool) -> serde_json::Value {
        serde_json::json!({
            "settings": self.fe.settings(),
            "progress": self.progress.clone().unwrap_or(serde_json::Value::Null),
            "demo": demo,
        })
    }
}

impl App {
    /// Window events while the game's front end runs. Returns true when handled.
    pub(super) fn play_window_event(&mut self, el: &ActiveEventLoop, event: &WindowEvent) -> bool {
        let Some(p) = self.play.as_mut() else { return false };
        match event {
            WindowEvent::KeyboardInput { event, .. } => {
                #[cfg(target_arch = "wasm32")]
                {
                    p.audio_locked = false;
                }
                if let PhysicalKey::Code(code) = event.physical_key {
                    match event.state {
                        ElementState::Pressed => {
                            if !event.repeat {
                                p.key_taps.push(code);
                            }
                            p.keys.insert(code);
                            p.dev = Dev::Keyboard;
                        }
                        ElementState::Released => {
                            p.keys.remove(&code);
                        }
                    }
                }
                #[cfg(target_arch = "wasm32")]
                self.resume_audio();
                true
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let c = mouse_code(*button);
                match state {
                    ElementState::Pressed => {
                        p.mouse_taps.push(c);
                        p.mouse.insert(c);
                        if !p.mobile {
                            p.dev = Dev::Keyboard;
                        }
                    }
                    ElementState::Released => {
                        p.mouse.remove(&c);
                    }
                }
                #[cfg(target_arch = "wasm32")]
                {
                    p.audio_locked = false;
                    self.resume_audio();
                }
                false // egui also gets clicks (menus)
            }
            WindowEvent::MouseWheel { delta, .. } => {
                p.wheel += match delta {
                    MouseScrollDelta::LineDelta(_, y) => *y,
                    MouseScrollDelta::PixelDelta(d) => d.y as f32 / 40.0,
                };
                false
            }
            WindowEvent::CursorMoved { position, .. } => {
                let pos = Vec2::new(position.x as f32, position.y as f32);
                p.mouse_delta += pos - p.cursor;
                p.cursor = pos;
                false
            }
            WindowEvent::Touch(_) => {
                p.touch_any = true;
                p.dev = Dev::Touch;
                #[cfg(target_arch = "wasm32")]
                {
                    p.audio_locked = false;
                    self.resume_audio();
                }
                false
            }
            WindowEvent::CloseRequested => {
                el.exit();
                true
            }
            _ => false,
        }
    }

    fn play_load(&mut self, scene: &str, seed: u64, autopilot: bool) {
        let Some(host) = &self.host else { return };
        let Some(p) = self.play.as_mut() else { return };
        let payload = p.restore_payload(autopilot);
        let scene_name = scene.to_string();
        host.exec(move |sim| match Sim::new(&scene_name, seed) {
            Ok(mut fresh) => {
                if let Some(g) = fresh.state.play.as_mut() {
                    g.restore(&payload);
                }
                *sim = fresh;
            }
            Err(e) => log::error!("could not load scene '{scene_name}': {e:#}"),
        });
        host.set_control(|c| {
            c.autopilot = autopilot;
            c.paused = false;
            c.rewinding = false;
        });
        *host.shared.input.lock().unwrap() = InputFrame::default();
        self.builder = pav_view::ViewBuilder::new();
        if let Some(g) = &mut self.gfx {
            g.renderer.clear_particles();
        }
        p.since_load = 0.0;
        p.last_hint = None;
        self.scene_name = scene.to_string();
        p.fe.on_scene_loaded();
    }

    /// Reads the first gamepad.
    fn play_pad(&mut self) -> PadNow {
        let mut now = PadNow::default();
        let Some(g) = &mut self.input.gilrs else { return now };
        while let Some(ev) = g.next_event() {
            if let gilrs::EventType::ButtonPressed(..) = ev.event {
                now.any = true;
            }
        }
        if let Some((_, gp)) = g.gamepads().next() {
            now.name = Some(gp.name().to_string());
            for b in ALL_PAD {
                if let Some(gb) = gilrs_button(b) {
                    let pressed = if matches!(b, PadButton::LeftTrigger2 | PadButton::RightTrigger2) {
                        gp.button_data(gb).map(|d| d.value()).unwrap_or(0.0) > 0.35
                    } else {
                        gp.is_pressed(gb)
                    };
                    if pressed {
                        now.held.insert(b);
                    }
                }
            }
            use gilrs::Axis;
            now.left = deadzone(Vec2::new(gp.value(Axis::LeftStickX), gp.value(Axis::LeftStickY)), 0.18);
            now.right = deadzone(Vec2::new(gp.value(Axis::RightStickX), gp.value(Axis::RightStickY)), 0.2);
            let t = |b| gp.button_data(b).map(|d| d.value()).unwrap_or(0.0);
            now.triggers = [t(gilrs::Button::LeftTrigger2), t(gilrs::Button::RightTrigger2)];
            if !now.held.is_empty() || now.left.length() > 0.35 || now.right.length() > 0.35 || now.triggers.iter().any(|&v| v > 0.2) {
                now.any = true;
            }
        }
        now
    }

    /// One frame of the game's front end.
    pub(super) fn frame_play(&mut self) {
        if self.gfx.is_none() || self.host.is_none() {
            return;
        }
        let now = Instant::now();
        let dt = {
            let p = self.play.as_mut().unwrap();
            let wall = (now - p.last).as_secs_f32();
            p.wall_dt = wall.min(2.0);
            p.last = now;
            wall.min(0.1)
        };
        let pad = self.play_pad();
        let p = self.play.as_mut().unwrap();
        if !p.started {
            p.started = true;
            let scene = p.start_scene();
            let title = p.window_title();
            if let Some(g) = &self.gfx {
                g.window.set_title(&title);
            }
            // Phones and kiosks start in attract: the front end asks for its first segment.
            let _ = scene;
        }
        p.since_load += dt;

        // ---- devices → the front end's input
        if pad.any {
            p.dev = Dev::Pad;
            p.family = pad.name.as_deref().map(pav_ui::glyph::family_from_pad_name).unwrap_or(GlyphFamily::Xbox);
        }
        let mut ui = UiIn { stick: pad.left, ..Default::default() };
        let in_play = matches!(p.fe.mode(), Mode::Play);
        for &code in &p.key_taps {
            if code == KeyCode::Escape {
                ui.nav.push(if in_play { Nav::Start } else { Nav::Back });
            } else if let Some((_, n)) = NAV_KEYS.iter().find(|(k, _)| *k == code) {
                ui.nav.push(*n);
            }
        }
        let pad_taps: Vec<PadButton> = pad.held.difference(&p.pad_prev).copied().collect();
        for b in &pad_taps {
            ui.nav.push(match b {
                PadButton::DPadUp => Nav::Up,
                PadButton::DPadDown => Nav::Down,
                PadButton::DPadLeft => Nav::Left,
                PadButton::DPadRight => Nav::Right,
                PadButton::South => Nav::Confirm,
                PadButton::East => Nav::Back,
                PadButton::LeftTrigger => Nav::TabPrev,
                PadButton::RightTrigger => Nav::TabNext,
                PadButton::North => Nav::Context,
                PadButton::Start => Nav::Start,
                PadButton::Select | PadButton::Touchpad => Nav::Map,
                _ => continue,
            });
        }
        // The left stick moves the focus too, repeating after 350 ms then every 90 ms.
        let stick = pad.left;
        let dir = if stick.length() < 0.5 {
            None
        } else if stick.x.abs() > stick.y.abs() {
            Some(if stick.x > 0.0 { Nav::Right } else { Nav::Left })
        } else {
            Some(if stick.y > 0.0 { Nav::Up } else { Nav::Down })
        };
        if dir != p.stick_dir {
            p.stick_dir = dir;
            p.stick_timer = 0.35;
            if let Some(d) = dir {
                ui.nav.push(d);
            }
        } else if let Some(d) = dir {
            p.stick_timer -= dt;
            if p.stick_timer <= 0.0 {
                p.stick_timer = 0.09;
                ui.nav.push(d);
            }
        }
        let moved = match p.any_anchor {
            Some(a) => (p.cursor - a).length() > 8.0,
            None => {
                p.any_anchor = Some(p.cursor);
                false
            }
        };
        if moved {
            p.any_anchor = Some(p.cursor);
        }
        ui.any = !p.key_taps.is_empty()
            || !p.mouse_taps.is_empty()
            || p.wheel != 0.0
            || moved
            || pad.any
            || (p.touch_any && !p.mobile);
        ui.touch = p.dev == Dev::Touch;
        let ppp = self.gfx.as_ref().map(|g| g.size().1 as f32 / 1080.0).unwrap_or(1.0).max(0.1);
        ui.pointer = Some(egui::pos2(p.cursor.x / ppp, p.cursor.y / ppp));
        ui.pointer_moved = p.mouse_delta.length() > 0.5;
        ui.click = p.mouse_taps.contains(&0);
        ui.back_click = p.mouse_taps.contains(&1);
        ui.zoom = p.wheel + (pad.triggers[1] - pad.triggers[0]);
        let forced = p.fe.settings().controls.prompts;
        ui.device = match (forced, p.dev) {
            (pav_ui::settings::Prompts::PlayStation, _) => GlyphFamily::PlayStation,
            (pav_ui::settings::Prompts::Xbox, _) => GlyphFamily::Xbox,
            (pav_ui::settings::Prompts::Keyboard, _) => GlyphFamily::Keyboard,
            (_, Dev::Pad) => p.family,
            _ => GlyphFamily::Keyboard,
        };
        ui.audio_locked = p.audio_locked && self.audio.is_some();

        // ---- the front end decides
        let host = self.host.as_ref().unwrap();
        let (_, curr, _, _) = host.frames();
        let cmds = p.fe.update(p.wall_dt, &ui, curr.play.as_deref());
        let game_input = p.fe.game_input();
        let name = snake(p.fe.mode());
        if name != p.published {
            let ppp_css = self.gfx.as_ref().map(|g| g.size().1 as f32 / 1080.0 / g.window.scale_factor() as f32).unwrap_or(1.0);
            let button = p.fe.menu_button().map(|c| (c.x * ppp_css, c.y * ppp_css));
            publish_mode(&name, button);
            p.published = name;
        }

        // ---- game input (only while playing)
        let mut frame = InputFrame::default();
        if game_input {
            let k = |c: KeyCode| p.keys.contains(&c) as i32 as f32;
            let kb = Vec2::new(k(KeyCode::KeyD) - k(KeyCode::KeyA), k(KeyCode::KeyW) - k(KeyCode::KeyS));
            let axis = if kb != Vec2::ZERO { kb.normalize() } else { pad.left };
            frame.move_dir = match &p.last_hint {
                Some(h) => h.relative_move(axis),
                None => Vec2::new(axis.x, -axis.y),
            }
            .clamp_length_max(1.0);
            let s = &p.fe.settings().controls;
            let sens = s.sensitivity.clamp(0.2, 3.0);
            let inv = Vec2::new(if s.invert_x { -1.0 } else { 1.0 }, if s.invert_y { -1.0 } else { 1.0 });
            // Stick: up to 150 °/s; the sim ticks at 60 Hz.
            frame.look = pad.right * 150.0 / 60.0 * sens * inv;
            frame.triggers = pad.triggers;
            let mut held = 0u32;
            for a in &p.bindings.actions {
                let bit = 1u32 << a.bit.min(31);
                let down = a.keys.iter().filter_map(|n| key_from_name(n).or_else(|| extra_key(n))).any(|c| p.keys.contains(&c))
                    || a.pad.iter().any(|b| pad.held.contains(b))
                    || a.mouse.iter().filter_map(|n| mouse_name(n)).any(|m| p.mouse.contains(&m));
                if down {
                    held |= bit;
                }
            }
            let mut pressed = held & !p.prev_held;
            for a in &p.bindings.actions {
                let bit = 1u32 << a.bit.min(31);
                if a.keys.iter().filter_map(|n| key_from_name(n).or_else(|| extra_key(n))).any(|c| p.key_taps.contains(&c))
                    || a.mouse.iter().filter_map(|n| mouse_name(n)).any(|m| p.mouse_taps.contains(&m))
                {
                    pressed |= bit;
                }
            }
            p.prev_held = held;
            frame.held = held | pressed;
            frame.pressed = pressed;
            // Mouse look (pointer locked): 0.15 °/px, delivered once (an impulse, not a rate).
            let impulse = p.mouse_raw * 0.15 * sens * inv * Vec2::new(1.0, -1.0);
            if p.grabbed && impulse != Vec2::ZERO {
                *host.shared.look_impulse.lock().unwrap() += impulse;
            }
        } else {
            p.prev_held = 0;
        }
        p.mouse_raw = Vec2::ZERO;
        // Lock the pointer for mouse look while playing with keyboard and mouse (in the
        // browser only on a click: pointer lock needs a user gesture).
        let want = game_input && p.dev == Dev::Keyboard && !p.mobile;
        let may = !cfg!(target_arch = "wasm32") || !p.mouse_taps.is_empty() || !want;
        if want != p.grabbed && may {
            if let Some(g) = &self.gfx {
                let mode = if want { winit::window::CursorGrabMode::Locked } else { winit::window::CursorGrabMode::None };
                let ok = g.window.set_cursor_grab(mode).is_ok()
                    || (want && g.window.set_cursor_grab(winit::window::CursorGrabMode::Confined).is_ok());
                g.window.set_cursor_visible(!want);
                p.grabbed = want && ok;
            }
        }
        {
            let mut i = host.shared.input.lock().unwrap();
            let pressed = i.pressed | frame.pressed;
            *i = frame;
            i.pressed = pressed;
        }
        p.key_taps.clear();
        p.mouse_taps.clear();
        p.wheel = 0.0;
        p.mouse_delta = Vec2::ZERO;
        p.touch_any = false;
        p.pad_prev = pad.held;

        // ---- commands
        for c in cmds {
            self.play_command(c);
        }

        // ---- the simulation, the picture
        let host = self.host.as_ref().unwrap();
        host.pump();
        let (prev, curr, curr_at, tick_wall) = host.frames();
        let alpha = ((now - curr_at).as_secs_f32() / tick_wall.max(1e-4)).clamp(0.0, 1.0);
        let p = self.play.as_mut().unwrap();
        if let Some(h) = curr.play.as_ref().and_then(|f| f.camera) {
            p.last_hint = Some(h);
            self.rig.snap(h.target);
        } else {
            self.rig.update(curr.focus, dt);
        }
        if let Some(g) = &curr.play {
            if let Some(req) = &g.ui.request {
                if *req == pav_core::play::UiRequest::Save {
                    if let Some(prog) = host.query(|sim| sim.play().and_then(|g| g.progress())).flatten() {
                        p.save_progress(&prog);
                    }
                }
            }
        }
        let (w, h) = self.gfx.as_ref().unwrap().size();
        let aspect = w as f32 / h.max(1) as f32;
        let mut view = self.view.clone();
        let quality = match p.fe.settings().display.quality {
            pav_ui::settings::Quality::Low => crate::quality::Quality::Low,
            pav_ui::settings::Quality::Medium => crate::quality::Quality::Medium,
            pav_ui::settings::Quality::High => crate::quality::Quality::High,
        };
        quality.trim(self.touch_screen, &mut view);
        self.builder.now = self.started.elapsed().as_secs_f64();
        let events: Vec<_> = std::mem::take(&mut *host.shared.events.lock().unwrap());
        self.builder.add_events(&events);
        let focus = prev.focus.lerp(curr.focus, alpha);
        let scene = self.builder.build(&prev, &curr, alpha, &self.rig, aspect, &view, focus);

        // ---- sound
        if let Some(a) = &self.audio {
            if !p.banks_loaded {
                p.banks_loaded = true;
                load_banks(a, &p.info);
            }
            let s = p.fe.settings().audio.clone();
            a.set_bus(pav_audio::Bus::Master, s.master);
            a.set_bus(pav_audio::Bus::Music, s.music);
            a.set_bus(pav_audio::Bus::Sfx, s.sfx);
            a.set_bus(pav_audio::Bus::Ambience, s.ambience);
            a.set_bus(pav_audio::Bus::Voice, s.voice);
            a.set_bus(pav_audio::Bus::Ui, s.ui);
            let target = p.last_hint.map(|h| h.target).unwrap_or(curr.focus);
            let right = p.last_hint.map(|h| {
                let f = h.forward_xz();
                Vec3::new(-f.y, 0.0, f.x)
            });
            let l = pav_audio::Listener { pos: target, right: right.unwrap_or(Vec3::X) };
            if let Some(g) = &curr.play {
                a.apply(&g.audio, &l);
            }
            for e in &events {
                a.play_event(e, &l, 1.0);
            }
        }

        // ---- UI
        let gfx = self.gfx.as_mut().unwrap();
        let state = self.egui_state.as_mut().unwrap();
        let native = gfx.window.scale_factor() as f32;
        self.egui_ctx.set_zoom_factor(ppp / native.max(0.1));
        let raw = state.take(&gfx.window);
        let p = self.play.as_mut().unwrap();
        let proj = pav_ui::Projector {
            view_proj: scene.camera.proj * scene.camera.view,
            size: egui::vec2(w as f32 / ppp, h as f32 / ppp),
        };
        let settings = p.fe.settings().clone();
        let env = pav_ui::hud::HudEnv {
            theme: &p.theme,
            bindings: &p.bindings,
            family: ui.device,
            settings: &settings,
            proj,
            time: self.started.elapsed().as_secs_f32(),
        };
        let play_frame: Option<&PlayFrame> = curr.play.as_deref();
        let fe = &mut p.fe;
        let theme = p.theme.clone();
        if !p.theme_installed {
            p.theme_installed = true;
            pav_ui::theme::install(&self.egui_ctx, &theme);
        }
        let out = self.egui_ctx.run_ui(raw, |ui| fe.draw(ui.ctx(), play_frame, &env));
        state.platform_output(&gfx.window, out.platform_output);
        let ppp_out = out.pixels_per_point;
        let prims = self.egui_ctx.tessellate(out.shapes, ppp_out);
        let mut textures = out.textures_delta;
        gfx.draw(&scene, Some((&prims, &mut textures, ppp_out)));
        if self.boot_done_at.is_none() {
            self.boot_done_at = Some(Instant::now());
            log::info!("boot complete in {:.0} ms", crate::boot::diag().start.elapsed().as_secs_f64() * 1000.0);
        }
    }

    fn play_command(&mut self, c: Command) {
        match c {
            Command::LoadScene { scene, seed, autopilot } => self.play_load(&scene, seed, autopilot),
            Command::SetPaused(on) => {
                if let Some(h) = &self.host {
                    h.set_control(|c| c.paused = on);
                }
            }
            Command::RestartCheckpoint => {
                if let Some(h) = &self.host {
                    h.exec(|sim| {
                        sim.checkpoint_load();
                    });
                    h.set_control(|c| c.paused = false);
                }
            }
            Command::Quit => self.quit = true,
            Command::SaveSettings => {
                if let Some(p) = self.play.as_mut() {
                    p.save_settings();
                }
            }
            Command::SettingsChanged => {
                // Applied each frame from the front end's settings (volumes, quality, look).
            }
            Command::Sound(name) => {
                if let Some(a) = &self.audio {
                    a.ui(&name);
                }
            }
            Command::UnlockAudio => {
                #[cfg(target_arch = "wasm32")]
                self.resume_audio();
                if let Some(p) = self.play.as_mut() {
                    p.audio_locked = false;
                }
            }
            Command::ToggleSound(on) => {
                if let Some(p) = self.play.as_mut() {
                    p.fe.settings_mut().audio.master = if on { 0.8 } else { 0.0 };
                    p.save_settings();
                }
            }
            Command::StartGame | Command::NewGame => {}
        }
    }
}

/// Keys the engine's name table lacks.
fn extra_key(n: &str) -> Option<KeyCode> {
    Some(match n {
        "Escape" => KeyCode::Escape,
        "Backspace" => KeyCode::Backspace,
        _ => return None,
    })
}

fn load_banks(a: &pav_audio::AudioOut, info: &FrontEndInfo) {
    for (name, text) in &info.banks {
        if text.is_empty() {
            continue;
        }
        let r = match name.as_str() {
            "sounds" => a.load_bank(text),
            "ambience" => a.load_ambience(text),
            "music" => a.load_music(text, &info.songs),
            _ => Ok(()),
        };
        if let Err(e) = r {
            log::warn!("audio bank '{name}': {e:#}");
        }
    }
}
