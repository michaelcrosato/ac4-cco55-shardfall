//! The front end (PLAN §11.3, §12): title, main menu, Replay a Round, settings, credits,
//! confirm dialogs, pause menu (map, objectives, settings), attract mode with its title card,
//! kiosk and the mobile overlay, as a state machine driven by [`FrontEnd::update`] and drawn
//! by [`FrontEnd::draw`].
//!
//! Everything is testable from `update` alone: it takes `dt`, the frame's [`UiInput`] and the
//! game's latest [`PlayFrame`], and returns [`Command`]s for the app. `draw` paints the
//! current screen and records the focusable rectangles the next `update` uses for mouse hover
//! and clicks.
//!
//! Rules the machine keeps:
//! - Exactly one focused element on every screen; lists wrap; a sound on every move.
//! - After an input changes the screen, the rest of that frame's input is dropped (a press
//!   never acts twice).
//! - Idle 10 s on Title/Menu/Chapters/Settings/Credits (or a menu's Confirm) → a 0.5 s fade
//!   → Attract. Never from play or the pause menu (except kiosk: 60 s anywhere).
//! - Attract: any input → Title on the same update (consumed). Mobile: touch never exits;
//!   a keyboard or pad does (and the device then behaves as desktop).

mod draw;

use egui::{Pos2, Rect};
use pav_core::play::{FrontEndInfo, PlayFrame, UiRequest};

use crate::glyph::GlyphFamily;
use crate::input::{Nav, UiInput};
use crate::map::MapView;
use crate::settings::{self, Changed, Quality, Row, Settings, TABS};

/// Seconds of no input on a front-end screen before attract mode.
pub const ATTRACT_IDLE: f32 = 10.0;
/// The fade to attract mode.
pub const ATTRACT_FADE: f32 = 0.5;
/// Kiosk: seconds of no input anywhere before attract mode.
pub const KIOSK_IDLE: f32 = 60.0;
/// The attract title card.
pub const CARD_SECONDS: f32 = 3.0;
/// The title's fade-in after leaving attract mode.
pub const TITLE_FADE: f32 = 0.2;
/// If the app never reports a scene loaded, attract mode carries on after this.
pub const LOAD_WATCHDOG: f32 = 30.0;

/// The screens.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Mode {
    Boot,
    Title,
    Menu,
    Chapters,
    Settings,
    Credits,
    Confirm,
    Attract,
    AttractCard,
    Play,
    Paused,
    PauseMap,
    PauseObjectives,
    PauseSettings,
    MobileMenu,
    MobileAbout,
}

impl Mode {
    pub const ALL: [Mode; 16] = [
        Mode::Boot,
        Mode::Title,
        Mode::Menu,
        Mode::Chapters,
        Mode::Settings,
        Mode::Credits,
        Mode::Confirm,
        Mode::Attract,
        Mode::AttractCard,
        Mode::Play,
        Mode::Paused,
        Mode::PauseMap,
        Mode::PauseObjectives,
        Mode::PauseSettings,
        Mode::MobileMenu,
        Mode::MobileAbout,
    ];

    /// The snake_case name ("mobile_menu", "attract_card", "pause_map"...).
    pub fn name(self) -> &'static str {
        match self {
            Mode::Boot => "boot",
            Mode::Title => "title",
            Mode::Menu => "menu",
            Mode::Chapters => "chapters",
            Mode::Settings => "settings",
            Mode::Credits => "credits",
            Mode::Confirm => "confirm",
            Mode::Attract => "attract",
            Mode::AttractCard => "attract_card",
            Mode::Play => "play",
            Mode::Paused => "paused",
            Mode::PauseMap => "pause_map",
            Mode::PauseObjectives => "pause_objectives",
            Mode::PauseSettings => "pause_settings",
            Mode::MobileMenu => "mobile_menu",
            Mode::MobileAbout => "mobile_about",
        }
    }

    /// The attract family: the playlist keeps running underneath.
    pub fn is_attract(self) -> bool {
        matches!(self, Mode::Attract | Mode::AttractCard | Mode::MobileMenu | Mode::MobileAbout)
    }

    /// The pause family (the game is paused).
    pub fn is_paused(self) -> bool {
        matches!(self, Mode::Paused | Mode::PauseMap | Mode::PauseObjectives | Mode::PauseSettings)
    }
}

/// What the platform allows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Platform {
    /// A phone or tablet: boots into attract mode and stays there (☰ overlay only).
    pub mobile: bool,
    /// A store display: starts in attract mode and returns there after 60 s idle anywhere.
    pub kiosk: bool,
    /// Native builds can quit (the browser can't).
    pub can_quit: bool,
}

/// What the app should do.
#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    /// Build this scene. `autopilot` = the sim host plays the bot's input (attract mode).
    /// Call [`FrontEnd::on_scene_loaded`] when it's ready.
    LoadScene {
        scene: String,
        seed: u64,
        autopilot: bool,
    },
    /// Pause or resume the simulation.
    SetPaused(bool),
    /// Restore the last checkpoint (then the pause menu closes).
    RestartCheckpoint,
    Quit,
    /// Persist `FrontEnd::settings()` now.
    SaveSettings,
    /// A setting changed: apply `FrontEnd::settings()` (volumes, quality...).
    SettingsChanged,
    /// Play a UI sound: "ui_move", "ui_confirm", "ui_back", "ui_slider", "ui_toggle".
    Sound(String),
    /// Unlock audio (browser autoplay rules: emitted on a user gesture).
    UnlockAudio,
    /// Mobile: sound on or off.
    ToggleSound(bool),
    /// Continue: comes right before the `LoadScene` of the saved game's scene; restore the
    /// saved progress once it's loaded (the app may load the progress's own scene instead).
    StartGame,
    /// New Game: comes right before the new game's `LoadScene`; wipe progress.
    NewGame,
}

/// Main menu entries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MenuItem {
    Continue,
    NewGame,
    Replay,
    Training,
    Settings,
    Credits,
    Quit,
}

impl MenuItem {
    pub(crate) fn label(self) -> &'static str {
        match self {
            MenuItem::Continue => "Continue",
            MenuItem::NewGame => "New Game",
            MenuItem::Replay => "Replay a Round",
            MenuItem::Training => "Training",
            MenuItem::Settings => "Settings",
            MenuItem::Credits => "Credits & About",
            MenuItem::Quit => "Quit",
        }
    }

    pub(crate) fn description(self) -> &'static str {
        match self {
            MenuItem::Continue => "Pick up where the tap last ran dry.",
            MenuItem::NewGame => "Sign on with the Black Tap and start from the first Round.",
            MenuItem::Replay => "Sail any Round again with your upgrades, for a Full Pour.",
            MenuItem::Training => "The Drydock: practise every move with no one watching.",
            MenuItem::Settings => "Controls, camera, audio, display, accessibility and gameplay.",
            MenuItem::Credits => "Who made this, the fonts and songs, and where it comes from.",
            MenuItem::Quit => "Close the game.",
        }
    }

    fn id(self) -> &'static str {
        match self {
            MenuItem::Continue => "continue",
            MenuItem::NewGame => "new_game",
            MenuItem::Replay => "replay",
            MenuItem::Training => "training",
            MenuItem::Settings => "settings",
            MenuItem::Credits => "credits",
            MenuItem::Quit => "quit",
        }
    }
}

/// Pause menu entries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PauseItem {
    Resume,
    Map,
    Objectives,
    Settings,
    Restart,
    QuitToTitle,
}

pub(crate) const PAUSE_ITEMS: [PauseItem; 6] =
    [PauseItem::Resume, PauseItem::Map, PauseItem::Objectives, PauseItem::Settings, PauseItem::Restart, PauseItem::QuitToTitle];

impl PauseItem {
    pub(crate) fn label(self) -> &'static str {
        match self {
            PauseItem::Resume => "Resume",
            PauseItem::Map => "Map",
            PauseItem::Objectives => "Objectives",
            PauseItem::Settings => "Settings",
            PauseItem::Restart => "Restart from Checkpoint",
            PauseItem::QuitToTitle => "Quit to Title",
        }
    }

    fn id(self) -> &'static str {
        match self {
            PauseItem::Resume => "resume",
            PauseItem::Map => "map",
            PauseItem::Objectives => "objectives",
            PauseItem::Settings => "settings",
            PauseItem::Restart => "restart",
            PauseItem::QuitToTitle => "quit_to_title",
        }
    }
}

/// What a confirm dialog asks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ConfirmKind {
    NewGame,
    Quit,
    QuitToTitle,
}

impl ConfirmKind {
    /// (question, detail, safe button, risky button)
    pub(crate) fn text(self) -> (&'static str, &'static str, &'static str, &'static str) {
        match self {
            ConfirmKind::NewGame => {
                ("Start a new game?", "Your saved progress will be poured out for good.", "Keep my save", "Start over")
            }
            ConfirmKind::Quit => ("Leave the Black Tap?", "Your progress is saved at the last checkpoint.", "Stay", "Quit"),
            ConfirmKind::QuitToTitle => {
                ("Quit to the title?", "You'll lose progress since the last checkpoint.", "Keep playing", "Quit to Title")
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Confirm {
    pub(crate) kind: ConfirmKind,
    /// 0 = the safe choice (the default), 1 = the risky one.
    pub(crate) focus: usize,
    pub(crate) back: Mode,
}

/// Mobile overlay rows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MobileItem {
    Sound,
    Quality,
    About,
    Close,
}

pub(crate) const MOBILE_ITEMS: [MobileItem; 4] = [MobileItem::Sound, MobileItem::Quality, MobileItem::About, MobileItem::Close];

/// Something a pointer can hit, recorded by `draw`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Target {
    /// A list item / row / button by index.
    Item(usize),
    /// A row's ◀ / ▶ arrows.
    Left(usize),
    Right(usize),
    Tab(usize),
    PagePrev,
    PageNext,
    /// The mobile ☰ button.
    Hamburger,
    /// Outside a dialog (closes the mobile overlay).
    Outside,
    /// Inside a dialog panel (swallows taps).
    Panel,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Hit {
    pub(crate) rect: Rect,
    pub(crate) target: Target,
}

/// The attract playlist's progress.
#[derive(Clone, Debug, Default)]
pub(crate) struct Attract {
    pub(crate) segment: usize,
    /// Completed playlists (picks the seed set).
    pub(crate) loops: u64,
    /// Seconds into the segment (counted only once its scene is loaded).
    pub(crate) elapsed: f32,
    pub(crate) loaded: bool,
    /// Seconds waiting for the scene.
    pub(crate) waiting: f32,
    /// Seconds since the scene was loaded (the fade-in).
    pub(crate) shown: f32,
    /// In the title card: seconds into it.
    pub(crate) card: Option<f32>,
}

/// One input action after mouse and pads are merged.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Act {
    Nav(Nav),
    /// Click on an item: focus it, then confirm.
    Click(Target),
}

/// The front end.
#[derive(Clone, Debug)]
pub struct FrontEnd {
    pub(crate) info: FrontEndInfo,
    pub(crate) settings: Settings,
    pub(crate) platform: Platform,
    pub(crate) has_progress: bool,
    pub(crate) mode: Mode,
    started: bool,
    /// A clock for pulses (wraps every 600 s).
    pub(crate) clock: f32,
    idle: f32,
    kiosk_idle: f32,
    /// The fade toward attract mode (seconds into it).
    pub(crate) fade_out: Option<f32>,
    /// Seconds of the title's fade-in left.
    pub(crate) title_fade: f32,
    pub(crate) menu_focus: usize,
    pub(crate) chapter_focus: usize,
    pub(crate) tab: usize,
    pub(crate) row: usize,
    pub(crate) settings_back: Mode,
    /// The button layout pages (inside Settings › Controls).
    pub(crate) layout_page: Option<usize>,
    pub(crate) credits_page: usize,
    pub(crate) pause_focus: usize,
    pub(crate) confirm: Option<Confirm>,
    pub(crate) mobile_focus: usize,
    pub(crate) about_page: usize,
    pub(crate) map_view: MapView,
    pub(crate) attract: Attract,
    /// Mobile behaviour (cleared once a keyboard or pad is used).
    pub(crate) mobile: bool,
    pub(crate) sound_on: bool,
    pub(crate) device: Option<GlyphFamily>,
    pub(crate) hits: Vec<Hit>,
    pub(crate) hits_mode: Option<Mode>,
    pub(crate) hamburger: Option<Pos2>,
    /// Seconds since the focus last moved (focus animation).
    pub(crate) focus_age: f32,
    /// The browser hasn't unlocked audio yet (from the last input).
    pub(crate) audio_locked: bool,
    /// How many actions the bindings have (button layout pages).
    pub(crate) layout_actions: usize,
    /// Credits and About pages measured by the last draw (blocks per page).
    pub(crate) credit_layout: Option<Vec<Vec<usize>>>,
    pub(crate) about_layout: Option<Vec<Vec<usize>>>,
    /// Drawing a screen under a dialog: no hint bar.
    pub(crate) suppress_hints: bool,
}

impl FrontEnd {
    /// Starts in Title, or Attract on mobile and kiosk platforms. The first `update` emits the
    /// starting screen's `LoadScene`.
    pub fn new(info: FrontEndInfo, settings: Settings, platform: Platform, has_progress: bool) -> Self {
        let mode = if platform.mobile || platform.kiosk { Mode::Attract } else { Mode::Title };
        let info_bindings = info.bindings.clone();
        Self {
            info,
            settings,
            platform,
            has_progress,
            mode,
            started: false,
            clock: 0.0,
            idle: 0.0,
            kiosk_idle: 0.0,
            fade_out: None,
            title_fade: 0.0,
            menu_focus: 0,
            chapter_focus: 0,
            tab: 0,
            row: 0,
            settings_back: Mode::Menu,
            layout_page: None,
            credits_page: 0,
            pause_focus: 0,
            confirm: None,
            mobile_focus: 0,
            about_page: 0,
            map_view: MapView::default(),
            attract: Attract::default(),
            mobile: platform.mobile,
            sound_on: !platform.mobile,
            device: None,
            hits: Vec::new(),
            hits_mode: None,
            hamburger: None,
            focus_age: 0.0,
            audio_locked: false,
            credit_layout: None,
            about_layout: None,
            suppress_hints: false,
            layout_actions: crate::bindings::Bindings::from_toml(&info_bindings).map(|b| b.actions.len()).unwrap_or(0),
        }
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// True only in Play with no menu over it: the game gets the player's input.
    pub fn game_input(&self) -> bool {
        self.mode == Mode::Play
    }

    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    /// Changing settings from outside: call `update` as usual; nothing else is needed.
    pub fn settings_mut(&mut self) -> &mut Settings {
        &mut self.settings
    }

    pub fn info(&self) -> &FrontEndInfo {
        &self.info
    }

    /// Whether a save exists (Continue shows, New Game asks first).
    pub fn set_has_progress(&mut self, has: bool) {
        self.has_progress = has;
        self.menu_focus = self.menu_focus.min(self.menu_items().len() - 1);
    }

    /// Mobile: whether sound is on.
    pub fn sound_on(&self) -> bool {
        self.sound_on
    }

    /// Mobile: the ☰ button's centre in points while it is shown (mobile attract), else None.
    /// Known after the first `draw`; before it, the default 16:9 position.
    pub fn menu_button(&self) -> Option<Pos2> {
        if !(self.mobile && matches!(self.mode, Mode::Attract | Mode::AttractCard)) {
            return None;
        }
        Some(
            self.hamburger.unwrap_or_else(|| {
                draw::hamburger_center(egui::Rect::from_min_size(Pos2::ZERO, egui::vec2(1920.0, 1080.0)), 0.05)
            }),
        )
    }

    /// The glyph family the screens use: the prompts setting, else the last device.
    pub fn family(&self, fallback: GlyphFamily) -> GlyphFamily {
        crate::glyph::resolve_family(self.settings.controls.prompts, self.device.unwrap_or(fallback))
    }

    /// Jumps to a screen without commands (tools render each screen headlessly).
    pub fn show(&mut self, mode: Mode) {
        self.started = true;
        self.fade_out = None;
        self.title_fade = 0.0;
        match mode {
            Mode::Confirm => {
                if self.confirm.is_none() {
                    self.confirm = Some(Confirm { kind: ConfirmKind::NewGame, focus: 0, back: Mode::Menu });
                }
            }
            Mode::Settings => self.settings_back = Mode::Menu,
            Mode::PauseSettings => self.settings_back = Mode::Paused,
            Mode::Attract => {
                self.attract.card = None;
                self.attract.loaded = true;
                self.attract.shown = 10.0;
            }
            Mode::AttractCard => self.attract.card = Some(1.0),
            Mode::MobileMenu | Mode::MobileAbout => {
                self.mobile = true;
                self.attract.loaded = true;
                self.attract.shown = 10.0;
            }
            _ => {}
        }
        self.mode = mode;
    }

    /// The app reports that the last requested scene is ready (starts attract timing).
    pub fn on_scene_loaded(&mut self) {
        self.attract.loaded = true;
        self.attract.shown = 0.0;
    }

    /// Debug: the size of the internal state (for the "no growth" test).
    #[doc(hidden)]
    pub fn footprint(&self) -> usize {
        self.hits.capacity()
            + std::mem::size_of::<Self>()
            + self.credit_layout.as_ref().map(|l| l.iter().map(|p| p.capacity()).sum::<usize>()).unwrap_or(0)
            + self.about_layout.as_ref().map(|l| l.iter().map(|p| p.capacity()).sum::<usize>()).unwrap_or(0)
    }

    pub(crate) fn menu_items(&self) -> Vec<MenuItem> {
        let mut v = Vec::with_capacity(7);
        if self.has_progress {
            v.push(MenuItem::Continue);
        }
        v.push(MenuItem::NewGame);
        if !self.info.chapters.is_empty() {
            v.push(MenuItem::Replay);
        }
        if self.info.training_scene.is_some() {
            v.push(MenuItem::Training);
        }
        v.push(MenuItem::Settings);
        v.push(MenuItem::Credits);
        if self.platform.can_quit {
            v.push(MenuItem::Quit);
        }
        v
    }

    pub(crate) fn rows(&self) -> &'static [Row] {
        TABS.get(self.tab).map(|t| t.1).unwrap_or(&[])
    }

    /// The pages of the credits (blocks per page), estimated without fonts.
    pub(crate) fn credit_pages(&self) -> Vec<Vec<usize>> {
        if let Some(l) = &self.credit_layout {
            return l.clone();
        }
        paginate(self.info.credits.iter().map(|(h, b)| (h.len(), b.len())), 560.0, 80.0)
    }

    /// Mobile About pages.
    pub(crate) fn about_pages(&self) -> Vec<Vec<usize>> {
        if let Some(l) = &self.about_layout {
            return l.clone();
        }
        paginate(self.info.credits.iter().map(|(h, b)| (h.len(), b.len())), 520.0, 62.0)
    }

    /// Button layout pages (6 actions per page, plus movement, camera and pause first).
    pub(crate) fn layout_pages(&self, bindings_len: usize) -> usize {
        (bindings_len + 3).div_ceil(6).max(1)
    }

    fn title_scene(&self) -> Command {
        Command::LoadScene { scene: self.info.title_scene.clone(), seed: 0, autopilot: false }
    }

    fn sound(out: &mut Vec<Command>, name: &str) {
        out.push(Command::Sound(name.to_string()));
    }

    /// Advances the machine by `dt` seconds with this frame's input and the game's latest
    /// frame (None while no scene runs). Returns what the app should do, in order.
    pub fn update(&mut self, dt: f32, input: &UiInput, frame: Option<&PlayFrame>) -> Vec<Command> {
        let dt = if dt.is_finite() { dt.clamp(0.0, 10.0) } else { 0.0 };
        let mut out = Vec::new();
        self.clock = (self.clock + dt) % 600.0;
        self.focus_age = (self.focus_age + dt).min(10.0);
        self.title_fade = (self.title_fade - dt).max(0.0);
        if input.any && !input.touch {
            self.device = Some(input.device);
        }
        self.audio_locked = input.audio_locked;
        if self.mode == Mode::Boot {
            // A boot card shown with `show(Mode::Boot)`: the first update moves on to the start.
            self.mode = if self.platform.mobile || self.platform.kiosk { Mode::Attract } else { Mode::Title };
            self.started = false;
        }
        if !self.started {
            // The starting screen's scene; the first frame's input still counts below.
            self.started = true;
            if self.mode.is_attract() {
                self.enter_attract(&mut out);
                return out;
            } else if self.mode == Mode::Title {
                out.push(self.title_scene());
            }
        }

        // ---- attract family
        if self.mode.is_attract() {
            if input.any {
                let mobile_touch = self.mobile && input.touch;
                if !mobile_touch {
                    // Any input (a keyboard or pad on a phone, too) → the title, now.
                    self.mobile = false;
                    self.mode = Mode::Title;
                    self.title_fade = TITLE_FADE;
                    self.idle = 0.0;
                    self.kiosk_idle = 0.0;
                    self.attract.card = None;
                    out.push(self.title_scene());
                    if input.audio_locked {
                        out.push(Command::UnlockAudio);
                    }
                    return out;
                }
            }
            // A phone's taps aren't "any input" (they don't leave attract): the ☰ button and its
            // overlay take them here.
            self.mobile_tap(input, &mut out);
            self.advance_attract(dt, frame, &mut out);
            return out;
        }

        // ---- idle timers
        if input.any {
            self.idle = 0.0;
            self.kiosk_idle = 0.0;
            self.fade_out = None;
            if input.audio_locked {
                out.push(Command::UnlockAudio);
            }
        } else {
            self.idle += dt;
            self.kiosk_idle += dt;
        }
        if self.platform.kiosk && self.kiosk_idle >= KIOSK_IDLE {
            if self.mode.is_paused() || (self.mode == Mode::Confirm && self.confirm.is_some_and(|c| c.back.is_paused())) {
                out.push(Command::SetPaused(false));
            }
            if matches!(self.mode, Mode::Settings | Mode::PauseSettings) {
                out.push(Command::SaveSettings);
            }
            self.enter_attract(&mut out);
            return out;
        }
        let eligible = match self.mode {
            Mode::Title | Mode::Menu | Mode::Chapters | Mode::Settings | Mode::Credits => true,
            Mode::Confirm => self.confirm.is_some_and(|c| !c.back.is_paused()),
            _ => false,
        };
        if !eligible {
            self.fade_out = None;
        } else if let Some(t) = self.fade_out.as_mut() {
            *t += dt;
            if *t >= ATTRACT_FADE {
                if self.mode == Mode::Settings {
                    out.push(Command::SaveSettings);
                }
                self.enter_attract(&mut out);
                return out;
            }
        } else if self.idle >= ATTRACT_IDLE {
            self.fade_out = Some((self.idle - ATTRACT_IDLE).min(ATTRACT_FADE * 0.99));
        }

        // ---- play: watch for pause and the game's requests
        if self.mode == Mode::Play {
            if let Some(req) = frame.and_then(|f| f.ui.request.as_ref()) {
                match req {
                    UiRequest::Title => {
                        self.go_title(&mut out);
                        return out;
                    }
                    UiRequest::Replay => {
                        self.mode = Mode::Chapters;
                        self.chapter_focus = 0;
                        out.push(self.title_scene());
                        return out;
                    }
                    UiRequest::Save => {}
                }
            }
            if input.has(Nav::Start) {
                self.mode = Mode::Paused;
                self.pause_focus = 0;
                out.push(Command::SetPaused(true));
                Self::sound(&mut out, "ui_confirm");
            }
            return out;
        }

        // ---- title: any input opens the menu (consumed)
        if self.mode == Mode::Title {
            if input.any {
                self.mode = Mode::Menu;
                self.menu_focus = 0;
                self.focus_age = 0.0;
                Self::sound(&mut out, "ui_confirm");
            }
            return out;
        }

        // ---- the map pans every frame
        if self.mode == Mode::PauseMap {
            let map = frame.and_then(|f| f.ui.map.as_deref());
            self.map_view.steer(map, input.stick, input.zoom, dt);
        }

        // ---- pointer
        let mut acts: Vec<Act> = Vec::new();
        let hit = input.pointer.and_then(|p| self.hit_at(p));
        if input.pointer_moved {
            if let Some(t) = hit {
                self.hover(t, &mut out);
            }
        }
        if input.click {
            if let Some(t) = hit {
                acts.push(Act::Click(t));
            }
        }
        if input.back_click {
            acts.push(Act::Nav(Nav::Back));
        }
        acts.extend(input.nav.iter().map(|n| Act::Nav(*n)));
        for a in acts {
            let before = (self.mode, self.layout_page.is_some());
            self.act(a, frame, &mut out);
            if (self.mode, self.layout_page.is_some()) != before {
                break; // the screen changed: drop the rest of this frame's input
            }
        }
        out
    }

    fn hit_at(&self, p: Pos2) -> Option<Target> {
        if self.hits_mode != Some(self.mode) {
            return None;
        }
        self.hits.iter().rev().find(|h| h.rect.contains(p)).map(|h| h.target)
    }

    /// Hovering moves the focus.
    fn hover(&mut self, t: Target, out: &mut Vec<Command>) {
        let i = match t {
            Target::Item(i) | Target::Left(i) | Target::Right(i) => i,
            _ => return,
        };
        let slot = match self.mode {
            Mode::Menu => &mut self.menu_focus,
            Mode::Chapters => &mut self.chapter_focus,
            Mode::Settings | Mode::PauseSettings if self.layout_page.is_none() => &mut self.row,
            Mode::Paused => &mut self.pause_focus,
            Mode::Confirm => match self.confirm.as_mut() {
                Some(c) => &mut c.focus,
                None => return,
            },
            Mode::MobileMenu => &mut self.mobile_focus,
            _ => return,
        };
        if *slot != i {
            *slot = i;
            self.focus_age = 0.0;
            Self::sound(out, "ui_move");
        }
    }

    fn enter_attract(&mut self, out: &mut Vec<Command>) {
        self.mode = Mode::Attract;
        self.fade_out = None;
        self.idle = 0.0;
        self.kiosk_idle = 0.0;
        self.confirm = None;
        self.layout_page = None;
        self.attract.segment = 0;
        self.attract.card = None;
        if self.info.demo.is_empty() {
            self.attract.card = Some(0.0);
            self.mode = Mode::AttractCard;
        } else {
            self.load_segment(out);
        }
    }

    fn load_segment(&mut self, out: &mut Vec<Command>) {
        let Some(seg) = self.info.demo.get(self.attract.segment) else { return };
        let seed = if seg.seeds.is_empty() { 0 } else { seg.seeds[(self.attract.loops % seg.seeds.len() as u64) as usize] };
        out.push(Command::LoadScene { scene: seg.scene.clone(), seed, autopilot: true });
        self.attract.loaded = false;
        self.attract.waiting = 0.0;
        self.attract.elapsed = 0.0;
        self.attract.shown = 0.0;
    }

    fn advance_attract(&mut self, dt: f32, frame: Option<&PlayFrame>, out: &mut Vec<Command>) {
        if let Some(t) = self.attract.card.as_mut() {
            *t += dt;
            if *t >= CARD_SECONDS {
                self.attract.card = None;
                self.attract.loops = self.attract.loops.wrapping_add(1);
                self.attract.segment = 0;
                if self.info.demo.is_empty() {
                    self.attract.card = Some(0.0);
                } else {
                    self.load_segment(out);
                }
            }
        } else if !self.info.demo.is_empty() {
            if !self.attract.loaded {
                self.attract.waiting += dt;
                if self.attract.waiting >= LOAD_WATCHDOG {
                    log::warn!("attract: no on_scene_loaded after {LOAD_WATCHDOG} s; carrying on");
                    self.attract.loaded = true;
                }
            } else {
                self.attract.elapsed += dt;
                self.attract.shown = (self.attract.shown + dt).min(10.0);
                let seg = &self.info.demo[self.attract.segment.min(self.info.demo.len() - 1)];
                let done = frame.and_then(|f| f.ui.demo.as_ref()).is_some_and(|d| d.done);
                if done || (seg.seconds > 0.0 && self.attract.elapsed >= seg.seconds) {
                    if self.attract.segment + 1 < self.info.demo.len() {
                        self.attract.segment += 1;
                        self.load_segment(out);
                    } else {
                        self.attract.card = Some(0.0);
                    }
                }
            }
        }
        if matches!(self.mode, Mode::Attract | Mode::AttractCard) {
            self.mode = if self.attract.card.is_some() { Mode::AttractCard } else { Mode::Attract };
        }
    }

    /// The attract mode the mobile overlay returns to.
    fn attract_mode(&self) -> Mode {
        if self.attract.card.is_some() { Mode::AttractCard } else { Mode::Attract }
    }

    /// Taps on a phone: the ☰ button and the overlay.
    fn mobile_tap(&mut self, input: &UiInput, out: &mut Vec<Command>) {
        if !input.click {
            return;
        }
        let Some(pos) = input.pointer else { return };
        // Before the first draw, the ☰ button's default place still works.
        let fallback = (matches!(self.mode, Mode::Attract | Mode::AttractCard) && self.hits_mode.is_none())
            .then(|| self.menu_button())
            .flatten()
            .filter(|c| (*c - pos).length() < 70.0)
            .map(|_| Target::Hamburger);
        let Some(t) = self.hit_at(pos).or(fallback) else { return };
        match (self.mode, t) {
            (Mode::Attract | Mode::AttractCard, Target::Hamburger) => {
                self.mode = Mode::MobileMenu;
                self.mobile_focus = 0;
                Self::sound(out, "ui_confirm");
            }
            (Mode::MobileMenu, Target::Item(i)) => {
                self.mobile_focus = i.min(MOBILE_ITEMS.len() - 1);
                match MOBILE_ITEMS[self.mobile_focus] {
                    MobileItem::Sound => {
                        self.sound_on = !self.sound_on;
                        if self.sound_on {
                            out.push(Command::UnlockAudio);
                        }
                        out.push(Command::ToggleSound(self.sound_on));
                        Self::sound(out, "ui_toggle");
                    }
                    MobileItem::Quality => {
                        let q = &mut self.settings.display.quality;
                        *q = match q {
                            Quality::Low => Quality::Medium,
                            Quality::Medium => Quality::High,
                            Quality::High => Quality::Low,
                        };
                        out.push(Command::SettingsChanged);
                        out.push(Command::SaveSettings);
                        Self::sound(out, "ui_toggle");
                    }
                    MobileItem::About => {
                        self.mode = Mode::MobileAbout;
                        self.about_page = 0;
                        Self::sound(out, "ui_confirm");
                    }
                    MobileItem::Close => {
                        self.mode = self.attract_mode();
                        Self::sound(out, "ui_back");
                    }
                }
            }
            (Mode::MobileMenu | Mode::MobileAbout, Target::Hamburger | Target::Outside) => {
                self.mode = self.attract_mode();
                Self::sound(out, "ui_back");
            }
            (Mode::MobileAbout, Target::PageNext) => {
                let n = self.about_pages().len().max(1);
                self.about_page = (self.about_page + 1) % n;
                Self::sound(out, "ui_move");
            }
            (_, Target::Panel) => {}
            (Mode::MobileAbout, Target::Item(_)) => {
                self.mode = Mode::MobileMenu;
                Self::sound(out, "ui_back");
            }
            _ => {}
        }
    }

    fn go_title(&mut self, out: &mut Vec<Command>) {
        self.mode = Mode::Title;
        self.idle = 0.0;
        self.confirm = None;
        out.push(self.title_scene());
    }

    fn start_play(&mut self, scene: String, out: &mut Vec<Command>) {
        out.push(Command::LoadScene { scene, seed: 0, autopilot: false });
        self.mode = Mode::Play;
        self.confirm = None;
    }

    /// Moves a list focus with wrap; returns whether it moved.
    fn step_focus(focus: &mut usize, len: usize, dir: i32) -> bool {
        if len <= 1 {
            return false;
        }
        *focus = ((*focus as i32 + dir).rem_euclid(len as i32)) as usize;
        true
    }

    fn act(&mut self, a: Act, frame: Option<&PlayFrame>, out: &mut Vec<Command>) {
        match self.mode {
            Mode::Menu => self.act_menu(a, out),
            Mode::Chapters => self.act_chapters(a, out),
            Mode::Settings | Mode::PauseSettings => self.act_settings(a, out),
            Mode::Credits => self.act_credits(a, out),
            Mode::Confirm => self.act_confirm(a, out),
            Mode::Paused => self.act_pause(a, frame, out),
            Mode::PauseMap => match a {
                Act::Nav(Nav::Back | Nav::Map) => {
                    self.mode = Mode::Paused;
                    Self::sound(out, "ui_back");
                }
                Act::Nav(Nav::Start) => self.resume(out),
                Act::Nav(Nav::Context) => {
                    self.map_view.legend = !self.map_view.legend;
                    Self::sound(out, "ui_toggle");
                }
                _ => {}
            },
            Mode::PauseObjectives => match a {
                Act::Nav(Nav::Back | Nav::Confirm) => {
                    self.mode = Mode::Paused;
                    Self::sound(out, "ui_back");
                }
                Act::Nav(Nav::Start) => self.resume(out),
                _ => {}
            },
            Mode::Boot => {}
            _ => {}
        }
    }

    fn resume(&mut self, out: &mut Vec<Command>) {
        if matches!(self.mode, Mode::PauseSettings) {
            out.push(Command::SaveSettings);
        }
        self.mode = Mode::Play;
        self.layout_page = None;
        out.push(Command::SetPaused(false));
        Self::sound(out, "ui_back");
    }

    fn list_nav(focus: &mut usize, len: usize, a: Act, focus_age: &mut f32, out: &mut Vec<Command>) -> Option<Act> {
        match a {
            Act::Nav(Nav::Up) => {
                if Self::step_focus(focus, len, -1) {
                    *focus_age = 0.0;
                    Self::sound(out, "ui_move");
                }
                None
            }
            Act::Nav(Nav::Down) => {
                if Self::step_focus(focus, len, 1) {
                    *focus_age = 0.0;
                    Self::sound(out, "ui_move");
                }
                None
            }
            Act::Click(Target::Item(i)) if i < len => {
                *focus = i;
                Some(Act::Nav(Nav::Confirm))
            }
            other => Some(other),
        }
    }

    fn act_menu(&mut self, a: Act, out: &mut Vec<Command>) {
        let items = self.menu_items();
        self.menu_focus = self.menu_focus.min(items.len() - 1);
        let Some(a) = Self::list_nav(&mut self.menu_focus, items.len(), a, &mut self.focus_age, out) else { return };
        match a {
            Act::Nav(Nav::Confirm) => {
                Self::sound(out, "ui_confirm");
                match items[self.menu_focus] {
                    MenuItem::Continue => {
                        out.push(Command::StartGame);
                        self.start_play(self.info.new_game_scene.clone(), out);
                    }
                    MenuItem::NewGame => {
                        if self.has_progress {
                            self.confirm = Some(Confirm { kind: ConfirmKind::NewGame, focus: 0, back: Mode::Menu });
                            self.mode = Mode::Confirm;
                        } else {
                            out.push(Command::NewGame);
                            self.start_play(self.info.new_game_scene.clone(), out);
                        }
                    }
                    MenuItem::Replay => {
                        self.mode = Mode::Chapters;
                        self.chapter_focus = 0;
                    }
                    MenuItem::Training => {
                        if let Some(s) = self.info.training_scene.clone() {
                            self.start_play(s, out);
                        }
                    }
                    MenuItem::Settings => {
                        self.mode = Mode::Settings;
                        self.settings_back = Mode::Menu;
                        self.tab = 0;
                        self.row = 0;
                        self.layout_page = None;
                    }
                    MenuItem::Credits => {
                        self.mode = Mode::Credits;
                        self.credits_page = 0;
                    }
                    MenuItem::Quit => {
                        self.confirm = Some(Confirm { kind: ConfirmKind::Quit, focus: 0, back: Mode::Menu });
                        self.mode = Mode::Confirm;
                    }
                }
                self.focus_age = 0.0;
            }
            Act::Nav(Nav::Back) => {
                self.mode = Mode::Title;
                Self::sound(out, "ui_back");
            }
            _ => {}
        }
    }

    fn act_chapters(&mut self, a: Act, out: &mut Vec<Command>) {
        let n = self.info.chapters.len().max(1);
        let Some(a) = Self::list_nav(&mut self.chapter_focus, n, a, &mut self.focus_age, out) else { return };
        match a {
            Act::Nav(Nav::Confirm) => {
                if let Some((_, scene)) = self.info.chapters.get(self.chapter_focus).cloned() {
                    Self::sound(out, "ui_confirm");
                    self.start_play(scene, out);
                }
            }
            Act::Nav(Nav::Back) => {
                self.mode = Mode::Menu;
                if let Some(i) = self.menu_items().iter().position(|m| *m == MenuItem::Replay) {
                    self.menu_focus = i;
                }
                Self::sound(out, "ui_back");
            }
            _ => {}
        }
    }

    fn leave_settings(&mut self, out: &mut Vec<Command>) {
        out.push(Command::SaveSettings);
        self.mode = self.settings_back;
        if self.mode == Mode::Menu {
            if let Some(i) = self.menu_items().iter().position(|m| *m == MenuItem::Settings) {
                self.menu_focus = i;
            }
        } else {
            self.pause_focus = PAUSE_ITEMS.iter().position(|p| *p == PauseItem::Settings).unwrap_or(0);
        }
        Self::sound(out, "ui_back");
    }

    fn act_settings(&mut self, a: Act, out: &mut Vec<Command>) {
        // The button layout pages.
        if let Some(page) = self.layout_page {
            let n = self.layout_pages(self.layout_actions);
            match a {
                Act::Nav(Nav::Left | Nav::TabPrev) | Act::Click(Target::PagePrev) => {
                    self.layout_page = Some((page + n - 1) % n);
                    Self::sound(out, "ui_move");
                }
                Act::Nav(Nav::Right | Nav::TabNext | Nav::Confirm) | Act::Click(Target::PageNext) => {
                    self.layout_page = Some((page + 1) % n);
                    Self::sound(out, "ui_move");
                }
                Act::Nav(Nav::Back) => {
                    self.layout_page = None;
                    Self::sound(out, "ui_back");
                }
                Act::Nav(Nav::Start) if self.mode == Mode::PauseSettings => self.resume(out),
                _ => {}
            }
            return;
        }
        let rows = self.rows();
        self.row = self.row.min(rows.len().saturating_sub(1));
        let step = |me: &mut Self, dir: i32, out: &mut Vec<Command>| {
            let row = me.rows()[me.row];
            match row.step(&mut me.settings, dir) {
                Changed::No => {}
                Changed::Slider => {
                    out.push(Command::SettingsChanged);
                    Self::sound(out, "ui_slider");
                }
                Changed::Toggle => {
                    out.push(Command::SettingsChanged);
                    Self::sound(out, "ui_toggle");
                }
            }
        };
        match a {
            Act::Nav(Nav::Up) => {
                if Self::step_focus(&mut self.row, rows.len(), -1) {
                    self.focus_age = 0.0;
                    Self::sound(out, "ui_move");
                }
            }
            Act::Nav(Nav::Down) => {
                if Self::step_focus(&mut self.row, rows.len(), 1) {
                    self.focus_age = 0.0;
                    Self::sound(out, "ui_move");
                }
            }
            Act::Nav(Nav::Left) => step(self, -1, out),
            Act::Nav(Nav::Right) => step(self, 1, out),
            Act::Click(Target::Left(i)) if i < rows.len() => {
                self.row = i;
                step(self, -1, out);
            }
            Act::Click(Target::Right(i)) if i < rows.len() => {
                self.row = i;
                step(self, 1, out);
            }
            Act::Nav(Nav::TabPrev | Nav::TabNext) | Act::Click(Target::Tab(_)) => {
                let n = TABS.len();
                self.tab = match a {
                    Act::Click(Target::Tab(i)) => i.min(n - 1),
                    Act::Nav(Nav::TabPrev) => (self.tab + n - 1) % n,
                    _ => (self.tab + 1) % n,
                };
                self.row = 0;
                self.focus_age = 0.0;
                Self::sound(out, "ui_move");
            }
            Act::Nav(Nav::Context) => {
                if rows[self.row].reset(&mut self.settings) {
                    out.push(Command::SettingsChanged);
                }
                Self::sound(out, "ui_toggle");
            }
            Act::Nav(Nav::Confirm) | Act::Click(Target::Item(_)) => {
                if let Act::Click(Target::Item(i)) = a {
                    if i >= rows.len() {
                        return;
                    }
                    self.row = i;
                }
                match rows[self.row] {
                    Row::Defaults => {
                        if settings::reset_tab(&mut self.settings, self.tab) {
                            out.push(Command::SettingsChanged);
                        }
                        Self::sound(out, "ui_confirm");
                    }
                    Row::ButtonLayout => {
                        self.layout_page = Some(0);
                        Self::sound(out, "ui_confirm");
                    }
                    r => {
                        if matches!(r.value(&self.settings), settings::RowValue::Slider(..)) {
                            // Sliders change with left/right only.
                        } else {
                            step(self, 1, out);
                        }
                    }
                }
            }
            Act::Nav(Nav::Back) => self.leave_settings(out),
            Act::Nav(Nav::Start) if self.mode == Mode::PauseSettings => self.resume(out),
            _ => {}
        }
    }

    fn act_credits(&mut self, a: Act, out: &mut Vec<Command>) {
        let n = self.credit_pages().len().max(1);
        match a {
            Act::Nav(Nav::Left | Nav::TabPrev | Nav::Up) | Act::Click(Target::PagePrev) => {
                if n > 1 {
                    self.credits_page = (self.credits_page + n - 1) % n;
                    Self::sound(out, "ui_move");
                }
            }
            Act::Nav(Nav::Right | Nav::TabNext | Nav::Down | Nav::Confirm) | Act::Click(Target::PageNext) => {
                if n > 1 {
                    self.credits_page = (self.credits_page + 1) % n;
                    Self::sound(out, "ui_move");
                }
            }
            Act::Nav(Nav::Back) => {
                self.mode = Mode::Menu;
                if let Some(i) = self.menu_items().iter().position(|m| *m == MenuItem::Credits) {
                    self.menu_focus = i;
                }
                Self::sound(out, "ui_back");
            }
            _ => {}
        }
    }

    fn act_confirm(&mut self, a: Act, out: &mut Vec<Command>) {
        let Some(mut c) = self.confirm else {
            self.mode = Mode::Menu;
            return;
        };
        match a {
            Act::Nav(Nav::Left | Nav::Right | Nav::Up | Nav::Down) => {
                c.focus = 1 - c.focus.min(1);
                self.focus_age = 0.0;
                Self::sound(out, "ui_move");
                self.confirm = Some(c);
            }
            Act::Click(Target::Item(i)) => {
                c.focus = i.min(1);
                self.confirm = Some(c);
                self.act_confirm(Act::Nav(Nav::Confirm), out);
            }
            Act::Nav(Nav::Confirm) => {
                if c.focus == 0 {
                    self.cancel_confirm(c, out);
                    return;
                }
                Self::sound(out, "ui_confirm");
                self.confirm = None;
                match c.kind {
                    ConfirmKind::NewGame => {
                        out.push(Command::NewGame);
                        self.start_play(self.info.new_game_scene.clone(), out);
                    }
                    ConfirmKind::Quit => {
                        out.push(Command::Quit);
                        self.mode = Mode::Menu;
                    }
                    ConfirmKind::QuitToTitle => {
                        out.push(Command::SetPaused(false));
                        self.go_title(out);
                    }
                }
            }
            Act::Nav(Nav::Back) => self.cancel_confirm(c, out),
            Act::Nav(Nav::Start) if c.back.is_paused() => {
                self.confirm = None;
                self.mode = Mode::Paused;
                self.resume(out);
            }
            _ => {}
        }
    }

    fn cancel_confirm(&mut self, c: Confirm, out: &mut Vec<Command>) {
        self.confirm = None;
        self.mode = c.back;
        Self::sound(out, "ui_back");
    }

    fn act_pause(&mut self, a: Act, frame: Option<&PlayFrame>, out: &mut Vec<Command>) {
        let n = PAUSE_ITEMS.len();
        let Some(a) = Self::list_nav(&mut self.pause_focus, n, a, &mut self.focus_age, out) else { return };
        match a {
            Act::Nav(Nav::Back | Nav::Start) => self.resume(out),
            Act::Nav(Nav::Map) => self.open_map(frame, out),
            Act::Nav(Nav::Confirm) => {
                Self::sound(out, "ui_confirm");
                match PAUSE_ITEMS[self.pause_focus] {
                    PauseItem::Resume => {
                        self.mode = Mode::Play;
                        out.push(Command::SetPaused(false));
                    }
                    PauseItem::Map => self.open_map(frame, out),
                    PauseItem::Objectives => self.mode = Mode::PauseObjectives,
                    PauseItem::Settings => {
                        self.mode = Mode::PauseSettings;
                        self.settings_back = Mode::Paused;
                        self.tab = 0;
                        self.row = 0;
                        self.layout_page = None;
                    }
                    PauseItem::Restart => {
                        out.push(Command::RestartCheckpoint);
                        out.push(Command::SetPaused(false));
                        self.mode = Mode::Play;
                    }
                    PauseItem::QuitToTitle => {
                        self.confirm = Some(Confirm { kind: ConfirmKind::QuitToTitle, focus: 0, back: Mode::Paused });
                        self.mode = Mode::Confirm;
                    }
                }
            }
            _ => {}
        }
    }

    fn open_map(&mut self, frame: Option<&PlayFrame>, out: &mut Vec<Command>) {
        let ui = frame.map(|f| &f.ui);
        self.map_view = MapView::centered(ui.and_then(|u| u.map.as_deref()), ui.map(|u| u.map_icons.as_slice()).unwrap_or(&[]));
        self.map_view.zoom = 1.6;
        self.mode = Mode::PauseMap;
        let _ = out;
    }

    // ---------------------------------------------------------------- introspection

    /// A stable id of the focused element ("menu/new_game", "settings/audio/master",
    /// "confirm/safe", "pause/map", "credits/page/1"...), for tests and tools.
    pub fn focus_id(&self) -> String {
        self.focusables().get(self.focus_index()).cloned().unwrap_or_else(|| self.mode.name().to_string())
    }

    fn focus_index(&self) -> usize {
        match self.mode {
            Mode::Menu => self.menu_focus,
            Mode::Chapters => self.chapter_focus,
            Mode::Settings | Mode::PauseSettings => {
                if let Some(p) = self.layout_page {
                    p
                } else {
                    self.row
                }
            }
            Mode::Credits => self.credits_page,
            Mode::Confirm => self.confirm.map(|c| c.focus).unwrap_or(0),
            Mode::Paused => self.pause_focus,
            Mode::MobileMenu => self.mobile_focus,
            Mode::MobileAbout => self.about_page,
            _ => 0,
        }
    }

    /// Every focusable element of the current screen (ids as in [`FrontEnd::focus_id`]).
    pub fn focusables(&self) -> Vec<String> {
        match self.mode {
            Mode::Menu => self.menu_items().iter().map(|m| format!("menu/{}", m.id())).collect(),
            Mode::Chapters => (0..self.info.chapters.len().max(1)).map(|i| format!("chapters/{i}")).collect(),
            Mode::Settings | Mode::PauseSettings => {
                let tab = TABS[self.tab].0.to_ascii_lowercase();
                if self.layout_page.is_some() {
                    (0..self.layout_pages(self.layout_actions)).map(|i| format!("settings/layout/page/{i}")).collect()
                } else {
                    self.rows().iter().map(|r| format!("settings/{tab}/{}", r.id())).collect()
                }
            }
            Mode::Credits => (0..self.credit_pages().len().max(1)).map(|i| format!("credits/page/{i}")).collect(),
            Mode::Confirm => vec!["confirm/safe".into(), "confirm/risky".into()],
            Mode::Paused => PAUSE_ITEMS.iter().map(|p| format!("pause/{}", p.id())).collect(),
            Mode::MobileMenu => MOBILE_ITEMS.iter().map(|m| format!("mobile/{m:?}").to_ascii_lowercase()).collect(),
            Mode::MobileAbout => (0..self.about_pages().len().max(1)).map(|i| format!("mobile_about/page/{i}")).collect(),
            m => vec![m.name().to_string()],
        }
    }
}

/// Greedy pages of (heading chars, body chars) blocks under a height budget.
fn paginate(blocks: impl Iterator<Item = (usize, usize)>, budget: f32, chars_per_line: f32) -> Vec<Vec<usize>> {
    let mut pages: Vec<Vec<usize>> = vec![Vec::new()];
    let mut h = 0.0;
    for (i, (_, body)) in blocks.enumerate() {
        let lines = (body as f32 / chars_per_line).ceil().max(1.0);
        let bh = 76.0 + lines * 42.0 + 28.0;
        if h + bh > budget && !pages.last().is_some_and(|p| p.is_empty()) {
            pages.push(Vec::new());
            h = 0.0;
        }
        pages.last_mut().expect("page").push(i);
        h += bh;
    }
    pages
}
