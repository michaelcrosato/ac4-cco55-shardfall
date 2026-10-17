//! A game's input bindings as data (`games/blacktap/bindings.toml`): named actions, each a
//! button bit of `pav_core::input::InputFrame`, mapped to pad buttons (gilrs names), keys
//! (winit `KeyCode` names: "Space", "KeyE", "ShiftLeft", "Digit1"...) and mouse buttons
//! ("Left", "Right", "Middle", "WheelUp", "WheelDown"). Movement and look are axes, and
//! Options/Esc is the system pause: neither is an action.
//!
//! ```toml
//! [[action]]
//! name = "jump"
//! bit = 0
//! label = "Jump · Parkour Up · Climb"
//! pad = ["South"]
//! keys = ["Space"]
//! ```

use anyhow::{Context as _, Result, bail};
use serde::{Deserialize, Serialize};

/// Gamepad buttons with gilrs' names. Comments give the DualSense and Xbox names.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum PadButton {
    /// ✕ / A
    South,
    /// ◯ / B
    East,
    /// △ / Y
    North,
    /// □ / X
    West,
    /// L1 / LB
    LeftTrigger,
    /// R1 / RB
    RightTrigger,
    /// L2 / LT (analog; pressed past the threshold)
    LeftTrigger2,
    /// R2 / RT
    RightTrigger2,
    /// L3 / LS click
    LeftThumb,
    /// R3 / RS click
    RightThumb,
    DPadUp,
    DPadDown,
    DPadLeft,
    DPadRight,
    /// Options / Menu
    Start,
    /// Create / View
    Select,
    /// The DualSense touchpad click (other pads: none; bind Select as well).
    Touchpad,
}

impl PadButton {
    pub const ALL: [PadButton; 17] = [
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

    /// Parses a gilrs-style name; also accepts the common aliases ("L1", "RB", "Cross", "A",
    /// "Options", "Create"...).
    pub fn from_name(name: &str) -> Option<PadButton> {
        let n = name.trim().to_ascii_lowercase().replace(['_', '-', ' '], "");
        Some(match n.as_str() {
            "south" | "cross" | "a" => PadButton::South,
            "east" | "circle" | "b" => PadButton::East,
            "north" | "triangle" | "y" => PadButton::North,
            "west" | "square" | "x" => PadButton::West,
            "lefttrigger" | "l1" | "lb" => PadButton::LeftTrigger,
            "righttrigger" | "r1" | "rb" => PadButton::RightTrigger,
            "lefttrigger2" | "l2" | "lt" => PadButton::LeftTrigger2,
            "righttrigger2" | "r2" | "rt" => PadButton::RightTrigger2,
            "leftthumb" | "l3" | "ls" => PadButton::LeftThumb,
            "rightthumb" | "r3" | "rs" => PadButton::RightThumb,
            "dpadup" | "up" => PadButton::DPadUp,
            "dpaddown" | "down" => PadButton::DPadDown,
            "dpadleft" | "left" => PadButton::DPadLeft,
            "dpadright" | "right" => PadButton::DPadRight,
            "start" | "options" | "menu" => PadButton::Start,
            "select" | "create" | "view" | "share" | "back" => PadButton::Select,
            "touchpad" => PadButton::Touchpad,
            _ => return None,
        })
    }

    /// The gilrs name ("South", "LeftTrigger2"...).
    pub fn name(self) -> &'static str {
        match self {
            PadButton::South => "South",
            PadButton::East => "East",
            PadButton::North => "North",
            PadButton::West => "West",
            PadButton::LeftTrigger => "LeftTrigger",
            PadButton::RightTrigger => "RightTrigger",
            PadButton::LeftTrigger2 => "LeftTrigger2",
            PadButton::RightTrigger2 => "RightTrigger2",
            PadButton::LeftThumb => "LeftThumb",
            PadButton::RightThumb => "RightThumb",
            PadButton::DPadUp => "DPadUp",
            PadButton::DPadDown => "DPadDown",
            PadButton::DPadLeft => "DPadLeft",
            PadButton::DPadRight => "DPadRight",
            PadButton::Start => "Start",
            PadButton::Select => "Select",
            PadButton::Touchpad => "Touchpad",
        }
    }
}

/// One named action.
#[derive(Clone, Debug, PartialEq)]
pub struct Action {
    pub name: String,
    /// Bit number in `InputFrame::buttons` (0..31).
    pub bit: u32,
    /// What it does, for the button layout screen.
    pub label: String,
    pub pad: Vec<PadButton>,
    /// winit `KeyCode` names.
    pub keys: Vec<String>,
    /// "Left", "Right", "Middle", "WheelUp", "WheelDown". A mouse binding may carry a
    /// condition after a colon: "Left:aim" means "Left while the `aim` action is held".
    pub mouse: Vec<String>,
}

impl Action {
    /// The mouse button name without its condition ("Left:aim" → "Left").
    pub fn mouse_button(entry: &str) -> &str {
        entry.split(':').next().unwrap_or(entry)
    }

    /// The action that must be held for a mouse binding to count ("Left:aim" → Some("aim")).
    pub fn mouse_condition(entry: &str) -> Option<&str> {
        entry.split_once(':').map(|(_, c)| c)
    }
}

/// The analog thresholds and the action list.
#[derive(Clone, Debug, PartialEq)]
pub struct Bindings {
    pub actions: Vec<Action>,
    /// L2/R2 count as pressed past this (0..1).
    pub trigger_threshold: f32,
}

impl Default for Bindings {
    fn default() -> Self {
        Self { actions: Vec::new(), trigger_threshold: 0.5 }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    #[serde(default)]
    trigger_threshold: Option<f32>,
    #[serde(default)]
    action: Vec<ActionFile>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ActionFile {
    name: String,
    bit: u32,
    #[serde(default)]
    label: String,
    #[serde(default)]
    pad: Vec<String>,
    #[serde(default)]
    keys: Vec<String>,
    #[serde(default)]
    mouse: Vec<String>,
}

impl Bindings {
    /// Reads a bindings file. Errors name the action: unknown pad buttons or mouse buttons,
    /// bits ≥ 32, a bit or name used twice.
    pub fn from_toml(text: &str) -> Result<Bindings> {
        let file: File = toml::from_str(text).context("bindings.toml")?;
        let mut out =
            Bindings { actions: Vec::new(), trigger_threshold: file.trigger_threshold.unwrap_or(0.5).clamp(0.05, 0.95) };
        for a in file.action {
            if a.bit >= 32 {
                bail!("bindings.toml: action `{}` has bit {} (must be 0..31)", a.name, a.bit);
            }
            if out.actions.iter().any(|b| b.bit == a.bit) {
                bail!("bindings.toml: bit {} is used twice (`{}`)", a.bit, a.name);
            }
            if out.actions.iter().any(|b| b.name.eq_ignore_ascii_case(&a.name)) {
                bail!("bindings.toml: action `{}` is defined twice", a.name);
            }
            let mut pad = Vec::new();
            for p in &a.pad {
                pad.push(
                    PadButton::from_name(p)
                        .with_context(|| format!("bindings.toml: action `{}`: unknown pad button `{p}`", a.name))?,
                );
            }
            for m in &a.mouse {
                let b = Action::mouse_button(m);
                if !matches!(b, "Left" | "Right" | "Middle" | "WheelUp" | "WheelDown" | "Back" | "Forward") {
                    bail!("bindings.toml: action `{}`: unknown mouse button `{m}`", a.name);
                }
            }
            let label = if a.label.is_empty() { a.name.clone() } else { a.label };
            out.actions.push(Action { name: a.name, bit: a.bit, label, pad, keys: a.keys, mouse: a.mouse });
        }
        Ok(out)
    }

    /// The bit number of an action by name (case-insensitive).
    pub fn bit(&self, name: &str) -> Option<u32> {
        self.action(name).map(|a| a.bit)
    }

    /// An action by name (case-insensitive).
    pub fn action(&self, name: &str) -> Option<&Action> {
        self.actions.iter().find(|a| a.name.eq_ignore_ascii_case(name))
    }

    /// (name, bit) pairs, e.g. for `pav_core::input::buttons::register_names`.
    pub fn names(&self) -> Vec<(String, u32)> {
        self.actions.iter().map(|a| (a.name.clone(), a.bit)).collect()
    }

    /// The `InputFrame::buttons` mask of the actions bound to `button`.
    pub fn pad_mask(&self, button: PadButton) -> u32 {
        self.actions.iter().filter(|a| a.pad.contains(&button)).fold(0, |m, a| m | (1 << a.bit))
    }

    /// The mask of the actions bound to a winit key name ("KeyE").
    pub fn key_mask(&self, key: &str) -> u32 {
        self.actions.iter().filter(|a| a.keys.iter().any(|k| k == key)).fold(0, |m, a| m | (1 << a.bit))
    }

    /// The mask of the actions bound to a mouse button, given the buttons already held
    /// (for conditional bindings like "Left:aim").
    pub fn mouse_mask(&self, button: &str, held: u32) -> u32 {
        let mut mask = 0;
        // Conditional bindings win over plain ones when their condition holds.
        let mut conditional = false;
        for a in &self.actions {
            for m in &a.mouse {
                if Action::mouse_button(m) != button {
                    continue;
                }
                if let Some(c) = Action::mouse_condition(m) {
                    if self.bit(c).is_some_and(|b| held & (1 << b) != 0) {
                        mask |= 1 << a.bit;
                        conditional = true;
                    }
                }
            }
        }
        if conditional {
            return mask;
        }
        for a in &self.actions {
            if a.mouse.iter().any(|m| m == button) {
                mask |= 1 << a.bit;
            }
        }
        mask
    }
}
