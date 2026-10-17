//! Player settings (PLAN §9.16): versioned JSON with migration, and the rows the Settings
//! screen shows ([`Row`], [`TABS`]): each row knows how to show, step and reset its value,
//! so the screen, the tests and tools share one description.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The current settings format version.
pub const VERSION: u32 = 2;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub version: u32,
    pub audio: Audio,
    pub display: Display,
    pub controls: Controls,
    pub accessibility: Accessibility,
    pub gameplay: Gameplay,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: VERSION,
            audio: Audio::default(),
            display: Display::default(),
            controls: Controls::default(),
            accessibility: Accessibility::default(),
            gameplay: Gameplay::default(),
        }
    }
}

/// Bus volumes, 0..1.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Audio {
    pub master: f32,
    pub music: f32,
    pub sfx: f32,
    pub ambience: f32,
    pub voice: f32,
    pub ui: f32,
}

impl Default for Audio {
    fn default() -> Self {
        Self { master: 0.8, music: 0.7, sfx: 0.8, ambience: 0.7, voice: 0.9, ui: 0.6 }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Quality {
    Low,
    #[default]
    Medium,
    High,
}

/// The picture's look (PLAN §9.17): the game's own, or one of the selectable looks
/// (`pav_view`'s looks.toml by [`VisualStyle::look_name`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum VisualStyle {
    #[default]
    Classic,
    PaintedSeas,
    MemoryMist,
    Storm,
}

impl VisualStyle {
    pub const ALL: [VisualStyle; 4] =
        [VisualStyle::Classic, VisualStyle::PaintedSeas, VisualStyle::MemoryMist, VisualStyle::Storm];

    /// The name shown in the menu.
    pub fn label(self) -> &'static str {
        match self {
            VisualStyle::Classic => "Classic",
            VisualStyle::PaintedSeas => "Painted Seas",
            VisualStyle::MemoryMist => "Memory Mist",
            VisualStyle::Storm => "Storm",
        }
    }

    /// The look drawn (a `[[look]]` name in pav_view's looks.toml); None: the game's own.
    pub fn look_name(self) -> Option<&'static str> {
        match self {
            VisualStyle::Classic => None,
            s => Some(s.label()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Display {
    pub quality: Quality,
    /// The look the picture is drawn in.
    pub style: VisualStyle,
    /// 0.5..1.5 (1 = neutral).
    pub brightness: f32,
    /// Camera shake, 0..1.
    pub shake: f32,
    /// Camera motion (sway, blur, bob), 0..1.
    pub motion: f32,
}

impl Default for Display {
    fn default() -> Self {
        Self { quality: Quality::Medium, style: VisualStyle::Classic, brightness: 1.0, shake: 1.0, motion: 1.0 }
    }
}

/// Which glyph family prompts use.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Prompts {
    /// The last-used device.
    #[default]
    Auto,
    PlayStation,
    Xbox,
    Keyboard,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Controls {
    pub invert_x: bool,
    pub invert_y: bool,
    /// Look sensitivity multiplier, 0.2..2.
    pub sensitivity: f32,
    pub vibration: bool,
    /// Toggle (true) or hold (false).
    pub crouch_toggle: bool,
    pub aim_toggle: bool,
    pub gull_toggle: bool,
    pub prompts: Prompts,
}

impl Default for Controls {
    fn default() -> Self {
        Self {
            invert_x: false,
            invert_y: false,
            sensitivity: 1.0,
            vibration: true,
            crouch_toggle: true,
            aim_toggle: false,
            gull_toggle: false,
            prompts: Prompts::Auto,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SubtitleSize {
    S,
    #[default]
    M,
    L,
}

impl SubtitleSize {
    /// Body size in points at 1080p.
    pub fn points(self) -> f32 {
        match self {
            SubtitleSize::S => 30.0,
            SubtitleSize::M => 36.0,
            SubtitleSize::L => 44.0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Colorblind {
    #[default]
    Off,
    Protanopia,
    Deuteranopia,
    Tritanopia,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Accessibility {
    pub subtitles: bool,
    pub subtitle_size: SubtitleSize,
    pub speaker_names: bool,
    pub colorblind: Colorblind,
    pub high_contrast: bool,
    pub aim_assist: bool,
    pub vision_cones: bool,
}

impl Default for Accessibility {
    fn default() -> Self {
        Self {
            subtitles: true,
            subtitle_size: SubtitleSize::M,
            speaker_names: true,
            colorblind: Colorblind::Off,
            high_contrast: false,
            aim_assist: true,
            vision_cones: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Difficulty {
    Landlubber,
    #[default]
    Sailor,
    Captain,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Gameplay {
    pub difficulty: Difficulty,
    pub show_compass: bool,
    pub show_objective: bool,
    pub show_prompts: bool,
}

impl Default for Gameplay {
    fn default() -> Self {
        Self { difficulty: Difficulty::Sailor, show_compass: true, show_objective: true, show_prompts: true }
    }
}

impl Settings {
    /// Reads saved settings. Missing fields take their defaults, older versions are migrated,
    /// and corrupt text gives the defaults plus an error message for the host to show
    /// ("settings were reset").
    pub fn load(json: &str) -> (Settings, Option<String>) {
        let mut value: Value = match serde_json::from_str(json) {
            Ok(v) => v,
            Err(e) => return (Settings::default(), Some(format!("settings: {e}; using defaults"))),
        };
        if !value.is_object() {
            return (Settings::default(), Some("settings: not a JSON object; using defaults".into()));
        }
        let version = value.get("version").and_then(Value::as_u64).unwrap_or(1) as u32;
        if version > VERSION {
            log::warn!("settings: version {version} is newer than {VERSION}; reading what is known");
        }
        if version < 2 {
            migrate_v1(&mut value);
        }
        match serde_json::from_value::<Settings>(value) {
            Ok(mut s) => {
                s.version = VERSION;
                s.sanitize();
                (s, None)
            }
            Err(e) => (Settings::default(), Some(format!("settings: {e}; using defaults"))),
        }
    }

    /// The JSON text to save.
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    /// Clamps every number into its range (hand-edited files).
    pub fn sanitize(&mut self) {
        let a = &mut self.audio;
        for v in [&mut a.master, &mut a.music, &mut a.sfx, &mut a.ambience, &mut a.voice, &mut a.ui] {
            *v = if v.is_finite() { v.clamp(0.0, 1.0) } else { 0.8 };
        }
        let d = &mut self.display;
        d.brightness = if d.brightness.is_finite() { d.brightness.clamp(0.5, 1.5) } else { 1.0 };
        d.shake = if d.shake.is_finite() { d.shake.clamp(0.0, 1.0) } else { 1.0 };
        d.motion = if d.motion.is_finite() { d.motion.clamp(0.0, 1.0) } else { 1.0 };
        let c = &mut self.controls;
        c.sensitivity = if c.sensitivity.is_finite() { c.sensitivity.clamp(0.2, 2.0) } else { 1.0 };
    }
}

/// Version 1 (the walking skeleton's format) kept flat volume names, `effects` for the sound
/// bus, a boolean colour-blind switch and `prompt_style`. Rewrites them in place.
fn migrate_v1(v: &mut Value) {
    let Some(root) = v.as_object_mut() else { return };
    // Flat volumes → audio.
    let mut audio = root.remove("audio").and_then(|a| a.as_object().cloned()).unwrap_or_default();
    for (old, new) in [("master_volume", "master"), ("music_volume", "music"), ("sfx_volume", "sfx")] {
        if let Some(x) = root.remove(old) {
            audio.insert(new.into(), x);
        }
    }
    if let Some(x) = audio.remove("effects") {
        audio.insert("sfx".into(), x);
    }
    root.insert("audio".into(), Value::Object(audio));
    if let Some(acc) = root.get_mut("accessibility").and_then(Value::as_object_mut) {
        if let Some(b) = acc.get("colorblind").and_then(Value::as_bool) {
            acc.insert("colorblind".into(), Value::String(if b { "Deuteranopia" } else { "Off" }.into()));
        }
    }
    if let Some(ctl) = root.get_mut("controls").and_then(Value::as_object_mut) {
        if let Some(p) = ctl.remove("prompt_style") {
            let s = p.as_str().unwrap_or("Auto").to_ascii_lowercase();
            let name = match s.as_str() {
                "playstation" | "ps" | "dualsense" => "PlayStation",
                "xbox" => "Xbox",
                "keyboard" | "kbm" => "Keyboard",
                _ => "Auto",
            };
            ctl.insert("prompts".into(), Value::String(name.into()));
        }
    }
    root.insert("version".into(), Value::from(2));
}

// ---------------------------------------------------------------- rows of the Settings screen

/// One row of the Settings screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Row {
    Prompts,
    Vibration,
    CrouchToggle,
    AimToggle,
    GullToggle,
    /// Opens the button layout pages.
    ButtonLayout,
    Sensitivity,
    InvertX,
    InvertY,
    Shake,
    Motion,
    Master,
    Music,
    Sfx,
    Ambience,
    Voice,
    UiVolume,
    Quality,
    Style,
    Brightness,
    Subtitles,
    SubtitleSize,
    SpeakerNames,
    Colorblind,
    HighContrast,
    VisionCones,
    AimAssist,
    Difficulty,
    ShowCompass,
    ShowObjective,
    ShowPrompts,
    /// Resets every row of the tab.
    Defaults,
}

/// The tabs and their rows (≤ 7 each, "Defaults" last).
pub const TABS: &[(&str, &[Row])] = &[
    (
        "Controls",
        &[Row::Prompts, Row::Vibration, Row::CrouchToggle, Row::AimToggle, Row::GullToggle, Row::ButtonLayout, Row::Defaults],
    ),
    ("Camera", &[Row::Sensitivity, Row::InvertX, Row::InvertY, Row::Shake, Row::Motion, Row::Defaults]),
    ("Audio", &[Row::Master, Row::Music, Row::Sfx, Row::Ambience, Row::Voice, Row::UiVolume, Row::Defaults]),
    ("Display", &[Row::Quality, Row::Style, Row::Brightness, Row::Defaults]),
    (
        "Accessibility",
        &[
            Row::Subtitles,
            Row::SubtitleSize,
            Row::SpeakerNames,
            Row::Colorblind,
            Row::HighContrast,
            Row::VisionCones,
            Row::Defaults,
        ],
    ),
    ("Gameplay", &[Row::Difficulty, Row::AimAssist, Row::ShowCompass, Row::ShowObjective, Row::ShowPrompts, Row::Defaults]),
];

/// How a row shows its value.
#[derive(Clone, Debug, PartialEq)]
pub enum RowValue {
    /// A slider: fraction of the range (0..1) and the text ("80%").
    Slider(f32, String),
    /// A ◀ value ▶ picker.
    Choice(String),
    /// On / off.
    Toggle(bool),
    /// An action row (Defaults, Button Layout).
    Action,
}

/// What a step did (for the sound).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Changed {
    No,
    Slider,
    Toggle,
}

impl Row {
    pub fn label(self) -> &'static str {
        match self {
            Row::Prompts => "Button Prompts",
            Row::Vibration => "Vibration",
            Row::CrouchToggle => "Crouch",
            Row::AimToggle => "Aim",
            Row::GullToggle => "Gull Sight",
            Row::ButtonLayout => "Button Layout",
            Row::Sensitivity => "Look Sensitivity",
            Row::InvertX => "Invert Horizontal",
            Row::InvertY => "Invert Vertical",
            Row::Shake => "Camera Shake",
            Row::Motion => "Camera Motion",
            Row::Master => "Master Volume",
            Row::Music => "Music",
            Row::Sfx => "Effects",
            Row::Ambience => "Ambience",
            Row::Voice => "Voices",
            Row::UiVolume => "Menu Sounds",
            Row::Quality => "Graphics Quality",
            Row::Style => "Visual Style",
            Row::Brightness => "Brightness",
            Row::Subtitles => "Subtitles",
            Row::SubtitleSize => "Subtitle Size",
            Row::SpeakerNames => "Speaker Names",
            Row::Colorblind => "Colour-blind Mode",
            Row::HighContrast => "High-contrast HUD",
            Row::VisionCones => "Show Vision Cones",
            Row::AimAssist => "Aim Assist",
            Row::Difficulty => "Difficulty",
            Row::ShowCompass => "Compass",
            Row::ShowObjective => "Objective",
            Row::ShowPrompts => "Button Prompts on HUD",
            Row::Defaults => "Restore Defaults",
        }
    }

    /// The one-line description shown at the bottom of the screen.
    pub fn description(self) -> &'static str {
        match self {
            Row::Prompts => "Which button symbols to show. Auto follows the last device you touched.",
            Row::Vibration => "Controller rumble for cannon fire, hits and storms.",
            Row::CrouchToggle => "Press once to sneak, or hold the button to stay low.",
            Row::AimToggle => "Press once to aim, or hold the button while aiming.",
            Row::GullToggle => "Press once for Gull Sight, or hold the button to keep it on.",
            Row::ButtonLayout => "See what every button does, on foot and at the helm.",
            Row::Sensitivity => "How fast the camera turns with the right stick or mouse.",
            Row::InvertX => "Reverse left and right camera turning.",
            Row::InvertY => "Reverse up and down camera turning.",
            Row::Shake => "How much the camera shakes from cannons and blows.",
            Row::Motion => "Camera sway at sea and motion effects. Lower it if you feel queasy.",
            Row::Master => "The volume of everything.",
            Row::Music => "Shanties and the score.",
            Row::Sfx => "Cannons, steel, waves and footsteps.",
            Row::Ambience => "Wind, gulls, crowds and rain.",
            Row::Voice => "The crew, Old Bung and every scoundrel ashore.",
            Row::UiVolume => "The clicks and chimes of these menus.",
            Row::Quality => "Lower it for a smoother frame rate; raise it for longer views.",
            Row::Style => "Classic, Painted Seas (a painted world, crisp heroes), Memory Mist or Storm.",
            Row::Brightness => "Raise it until the left lantern is just visible in the dark.",
            Row::Subtitles => "Show spoken lines as text at the bottom of the screen.",
            Row::SubtitleSize => "How big subtitle text is.",
            Row::SpeakerNames => "Show who is speaking, in their colour.",
            Row::Colorblind => "Change marker and Gull Sight colours to stay distinct.",
            Row::HighContrast => "Brighter HUD colours with solid backgrounds.",
            Row::VisionCones => "Draw guards' sight on the ground while sneaking.",
            Row::AimAssist => "Pistol and cannon aim drift toward nearby targets.",
            Row::Difficulty => "Landlubber: forgiving guards. Captain: tight windows, no regen ashore.",
            Row::ShowCompass => "The compass bar at the top of the screen.",
            Row::ShowObjective => "The current objective at the top right.",
            Row::ShowPrompts => "Contextual button prompts at the bottom right.",
            Row::Defaults => "Put every setting on this tab back the way it shipped.",
        }
    }

    /// The row's current value for display.
    pub fn value(self, s: &Settings) -> RowValue {
        let pct = |v: f32| RowValue::Slider(v, format!("{}%", (v * 100.0).round() as i32));
        let ht = |toggle: bool| RowValue::Choice(if toggle { "Toggle" } else { "Hold" }.into());
        match self {
            Row::Prompts => RowValue::Choice(
                match s.controls.prompts {
                    Prompts::Auto => "Auto",
                    Prompts::PlayStation => "PlayStation",
                    Prompts::Xbox => "Xbox",
                    Prompts::Keyboard => "Keyboard",
                }
                .into(),
            ),
            Row::Vibration => RowValue::Toggle(s.controls.vibration),
            Row::CrouchToggle => ht(s.controls.crouch_toggle),
            Row::AimToggle => ht(s.controls.aim_toggle),
            Row::GullToggle => ht(s.controls.gull_toggle),
            Row::ButtonLayout | Row::Defaults => RowValue::Action,
            Row::Sensitivity => {
                let v = s.controls.sensitivity;
                RowValue::Slider((v - 0.2) / 1.8, format!("{v:.1}×"))
            }
            Row::InvertX => RowValue::Toggle(s.controls.invert_x),
            Row::InvertY => RowValue::Toggle(s.controls.invert_y),
            Row::Shake => pct(s.display.shake),
            Row::Motion => pct(s.display.motion),
            Row::Master => pct(s.audio.master),
            Row::Music => pct(s.audio.music),
            Row::Sfx => pct(s.audio.sfx),
            Row::Ambience => pct(s.audio.ambience),
            Row::Voice => pct(s.audio.voice),
            Row::UiVolume => pct(s.audio.ui),
            Row::Quality => RowValue::Choice(format!("{:?}", s.display.quality)),
            Row::Style => RowValue::Choice(s.display.style.label().into()),
            Row::Brightness => {
                let v = s.display.brightness;
                RowValue::Slider(v - 0.5, format!("{}%", (v * 100.0).round() as i32))
            }
            Row::Subtitles => RowValue::Toggle(s.accessibility.subtitles),
            Row::SubtitleSize => RowValue::Choice(
                match s.accessibility.subtitle_size {
                    SubtitleSize::S => "Small",
                    SubtitleSize::M => "Medium",
                    SubtitleSize::L => "Large",
                }
                .into(),
            ),
            Row::SpeakerNames => RowValue::Toggle(s.accessibility.speaker_names),
            Row::Colorblind => RowValue::Choice(format!("{:?}", s.accessibility.colorblind).replace("Off", "Off")),
            Row::HighContrast => RowValue::Toggle(s.accessibility.high_contrast),
            Row::VisionCones => RowValue::Toggle(s.accessibility.vision_cones),
            Row::AimAssist => RowValue::Toggle(s.accessibility.aim_assist),
            Row::Difficulty => RowValue::Choice(format!("{:?}", s.gameplay.difficulty)),
            Row::ShowCompass => RowValue::Toggle(s.gameplay.show_compass),
            Row::ShowObjective => RowValue::Toggle(s.gameplay.show_objective),
            Row::ShowPrompts => RowValue::Toggle(s.gameplay.show_prompts),
        }
    }

    /// Steps the value left (`dir` < 0) or right. Sliders stop at their ends; pickers wrap.
    pub fn step(self, s: &mut Settings, dir: i32) -> Changed {
        fn slide(v: &mut f32, dir: i32, step: f32, min: f32, max: f32) -> Changed {
            let n = ((*v + dir as f32 * step) / step).round() * step;
            let n = n.clamp(min, max);
            if (n - *v).abs() < 1e-4 {
                Changed::No
            } else {
                *v = n;
                Changed::Slider
            }
        }
        fn flip(v: &mut bool) -> Changed {
            *v = !*v;
            Changed::Toggle
        }
        fn cycle<T: Copy + PartialEq>(v: &mut T, all: &[T], dir: i32) -> Changed {
            let i = all.iter().position(|x| x == v).unwrap_or(0) as i32;
            *v = all[(i + dir).rem_euclid(all.len() as i32) as usize];
            Changed::Toggle
        }
        let dir = dir.signum();
        if dir == 0 {
            return Changed::No;
        }
        match self {
            Row::Prompts => {
                cycle(&mut s.controls.prompts, &[Prompts::Auto, Prompts::PlayStation, Prompts::Xbox, Prompts::Keyboard], dir)
            }
            Row::Vibration => flip(&mut s.controls.vibration),
            Row::CrouchToggle => flip(&mut s.controls.crouch_toggle),
            Row::AimToggle => flip(&mut s.controls.aim_toggle),
            Row::GullToggle => flip(&mut s.controls.gull_toggle),
            Row::ButtonLayout | Row::Defaults => Changed::No,
            Row::Sensitivity => slide(&mut s.controls.sensitivity, dir, 0.1, 0.2, 2.0),
            Row::InvertX => flip(&mut s.controls.invert_x),
            Row::InvertY => flip(&mut s.controls.invert_y),
            Row::Shake => slide(&mut s.display.shake, dir, 0.1, 0.0, 1.0),
            Row::Motion => slide(&mut s.display.motion, dir, 0.1, 0.0, 1.0),
            Row::Master => slide(&mut s.audio.master, dir, 0.1, 0.0, 1.0),
            Row::Music => slide(&mut s.audio.music, dir, 0.1, 0.0, 1.0),
            Row::Sfx => slide(&mut s.audio.sfx, dir, 0.1, 0.0, 1.0),
            Row::Ambience => slide(&mut s.audio.ambience, dir, 0.1, 0.0, 1.0),
            Row::Voice => slide(&mut s.audio.voice, dir, 0.1, 0.0, 1.0),
            Row::UiVolume => slide(&mut s.audio.ui, dir, 0.1, 0.0, 1.0),
            Row::Quality => cycle(&mut s.display.quality, &[Quality::Low, Quality::Medium, Quality::High], dir),
            Row::Style => cycle(&mut s.display.style, &VisualStyle::ALL, dir),
            Row::Brightness => slide(&mut s.display.brightness, dir, 0.05, 0.5, 1.5),
            Row::Subtitles => flip(&mut s.accessibility.subtitles),
            Row::SubtitleSize => {
                cycle(&mut s.accessibility.subtitle_size, &[SubtitleSize::S, SubtitleSize::M, SubtitleSize::L], dir)
            }
            Row::SpeakerNames => flip(&mut s.accessibility.speaker_names),
            Row::Colorblind => cycle(
                &mut s.accessibility.colorblind,
                &[Colorblind::Off, Colorblind::Protanopia, Colorblind::Deuteranopia, Colorblind::Tritanopia],
                dir,
            ),
            Row::HighContrast => flip(&mut s.accessibility.high_contrast),
            Row::VisionCones => flip(&mut s.accessibility.vision_cones),
            Row::AimAssist => flip(&mut s.accessibility.aim_assist),
            Row::Difficulty => {
                cycle(&mut s.gameplay.difficulty, &[Difficulty::Landlubber, Difficulty::Sailor, Difficulty::Captain], dir)
            }
            Row::ShowCompass => flip(&mut s.gameplay.show_compass),
            Row::ShowObjective => flip(&mut s.gameplay.show_objective),
            Row::ShowPrompts => flip(&mut s.gameplay.show_prompts),
        }
    }

    /// Puts the row's value back to its default. Returns whether anything changed.
    pub fn reset(self, s: &mut Settings) -> bool {
        let d = Settings::default();
        let before = s.clone();
        match self {
            Row::Prompts => s.controls.prompts = d.controls.prompts,
            Row::Vibration => s.controls.vibration = d.controls.vibration,
            Row::CrouchToggle => s.controls.crouch_toggle = d.controls.crouch_toggle,
            Row::AimToggle => s.controls.aim_toggle = d.controls.aim_toggle,
            Row::GullToggle => s.controls.gull_toggle = d.controls.gull_toggle,
            Row::ButtonLayout | Row::Defaults => {}
            Row::Sensitivity => s.controls.sensitivity = d.controls.sensitivity,
            Row::InvertX => s.controls.invert_x = d.controls.invert_x,
            Row::InvertY => s.controls.invert_y = d.controls.invert_y,
            Row::Shake => s.display.shake = d.display.shake,
            Row::Motion => s.display.motion = d.display.motion,
            Row::Master => s.audio.master = d.audio.master,
            Row::Music => s.audio.music = d.audio.music,
            Row::Sfx => s.audio.sfx = d.audio.sfx,
            Row::Ambience => s.audio.ambience = d.audio.ambience,
            Row::Voice => s.audio.voice = d.audio.voice,
            Row::UiVolume => s.audio.ui = d.audio.ui,
            Row::Quality => s.display.quality = d.display.quality,
            Row::Style => s.display.style = d.display.style,
            Row::Brightness => s.display.brightness = d.display.brightness,
            Row::Subtitles => s.accessibility.subtitles = d.accessibility.subtitles,
            Row::SubtitleSize => s.accessibility.subtitle_size = d.accessibility.subtitle_size,
            Row::SpeakerNames => s.accessibility.speaker_names = d.accessibility.speaker_names,
            Row::Colorblind => s.accessibility.colorblind = d.accessibility.colorblind,
            Row::HighContrast => s.accessibility.high_contrast = d.accessibility.high_contrast,
            Row::VisionCones => s.accessibility.vision_cones = d.accessibility.vision_cones,
            Row::AimAssist => s.accessibility.aim_assist = d.accessibility.aim_assist,
            Row::Difficulty => s.gameplay.difficulty = d.gameplay.difficulty,
            Row::ShowCompass => s.gameplay.show_compass = d.gameplay.show_compass,
            Row::ShowObjective => s.gameplay.show_objective = d.gameplay.show_objective,
            Row::ShowPrompts => s.gameplay.show_prompts = d.gameplay.show_prompts,
        }
        *s != before
    }

    /// A stable identifier ("audio/master") for tests and tools.
    pub fn id(self) -> String {
        format!("{self:?}").to_ascii_lowercase()
    }
}

/// Resets every row of tab `tab`. Returns whether anything changed.
pub fn reset_tab(s: &mut Settings, tab: usize) -> bool {
    let mut any = false;
    if let Some((_, rows)) = TABS.get(tab) {
        for r in rows.iter() {
            any |= r.reset(s);
        }
    }
    any
}
