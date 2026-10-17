//! Dialog lines and barks as data, a queue that plays them as subtitles with timing and
//! priorities, and seeded bark variety with cooldowns.
//!
//! ```toml
//! [speaker.bung]
//! name = "Old Bung"
//! color = "#e2b04a"
//! voice = "bung"
//! [line.bung_sails]
//! speaker = "bung"
//! text = "Raise the sails, Captain! Push forward to let out more canvas."
//! [bark.guard_suspicious]
//! speaker = "guard"
//! lines = ["Who goes there?", "Must've been the rum…"]
//! cooldown = 6.0
//! ```

use std::collections::{BTreeMap, VecDeque};

use pav_core::play::Subtitle;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Speaker {
    pub name: String,
    /// sRGB hex for the subtitle name.
    pub color: String,
    /// Voice blip set (the audio bank's `voice_blip` speaker).
    pub voice: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Line {
    pub speaker: String,
    pub text: String,
    /// Seconds on screen (0 = from the length of the text).
    pub duration: f32,
    /// Higher interrupts lower.
    pub priority: i32,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Bark {
    pub speaker: String,
    pub lines: Vec<String>,
    pub cooldown: f32,
}

/// Everything from `dialog.toml`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DialogBank {
    pub speaker: BTreeMap<String, Speaker>,
    pub line: BTreeMap<String, Line>,
    pub bark: BTreeMap<String, Bark>,
}

impl DialogBank {
    pub fn parse(text: &str) -> anyhow::Result<Self> {
        Ok(toml::from_str(text)?)
    }

    /// Seconds a line stays up: its own, or ~14 characters a second with a floor.
    pub fn duration(&self, id: &str) -> f32 {
        self.line.get(id).map(|l| if l.duration > 0.0 { l.duration } else { (l.text.chars().count() as f32 / 14.0).clamp(2.2, 7.0) }).unwrap_or(2.5)
    }
}

/// A line on screen or waiting.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Playing {
    pub speaker: String,
    pub text: String,
    pub left: f32,
    pub priority: i32,
}

/// The dialog runner (game state).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Dialog {
    pub current: Option<Playing>,
    pub queue: VecDeque<Playing>,
    /// Bark cooldowns by bark id, and a counter for seeded variety.
    pub cooldowns: BTreeMap<String, f32>,
    pub picks: u64,
    /// Lines started this tick (for voice blips): (voice, text).
    #[serde(skip)]
    pub started: Vec<(String, String)>,
}

impl Dialog {
    /// Queues a line by id; a higher priority one cuts in.
    pub fn say(&mut self, bank: &DialogBank, id: &str) {
        let Some(l) = bank.line.get(id) else {
            log::warn!("dialog: unknown line '{id}'");
            return;
        };
        let p = Playing { speaker: l.speaker.clone(), text: l.text.clone(), left: bank.duration(id), priority: l.priority };
        if self.current.as_ref().is_some_and(|c| p.priority > c.priority) {
            self.current = None;
        }
        if self.queue.len() < 8 {
            self.queue.push_back(p);
        }
    }

    /// A free-form line (a bark already chosen, a generated contract).
    pub fn say_text(&mut self, speaker: &str, text: &str, seconds: f32) {
        if self.queue.len() < 8 {
            self.queue.push_back(Playing { speaker: speaker.into(), text: text.into(), left: seconds, priority: 0 });
        }
    }

    /// Plays a bark if its cooldown allows; variety from a counter (deterministic).
    pub fn bark(&mut self, bank: &DialogBank, id: &str, seed: u64) -> bool {
        if self.cooldowns.get(id).is_some_and(|&t| t > 0.0) {
            return false;
        }
        let Some(b) = bank.bark.get(id) else { return false };
        if b.lines.is_empty() {
            return false;
        }
        self.picks = self.picks.wrapping_add(1);
        let i = ((seed ^ self.picks.wrapping_mul(0x9E37_79B9_7F4A_7C15)) >> 33) as usize % b.lines.len();
        self.cooldowns.insert(id.to_string(), b.cooldown.max(1.0));
        if self.current.is_none() || self.queue.is_empty() {
            let text = b.lines[i].clone();
            let secs = (text.chars().count() as f32 / 14.0).clamp(1.6, 4.0);
            self.say_text(&b.speaker, &text, secs);
        }
        true
    }

    pub fn tick(&mut self, bank: &DialogBank, dt: f32) {
        self.started.clear();
        for t in self.cooldowns.values_mut() {
            *t -= dt;
        }
        self.cooldowns.retain(|_, t| *t > 0.0);
        if let Some(c) = &mut self.current {
            c.left -= dt;
            if c.left <= 0.0 {
                self.current = None;
            }
        }
        if self.current.is_none() {
            if let Some(next) = self.queue.pop_front() {
                let voice = bank.speaker.get(&next.speaker).map(|s| s.voice.clone()).unwrap_or_else(|| next.speaker.clone());
                self.started.push((voice, next.text.clone()));
                self.current = Some(next);
            }
        }
    }

    /// Lines in the queue or on screen.
    pub fn busy(&self) -> bool {
        self.current.is_some() || !self.queue.is_empty()
    }

    pub fn subtitles(&self, bank: &DialogBank) -> Vec<Subtitle> {
        self.current
            .iter()
            .map(|c| {
                let s = bank.speaker.get(&c.speaker);
                Subtitle {
                    speaker: s.map(|s| s.name.clone()).unwrap_or_else(|| c.speaker.clone()),
                    color: s.map(|s| s.color.clone()).unwrap_or_else(|| "#f1e3c6".into()),
                    text: c.text.clone(),
                }
            })
            .collect()
    }
}
