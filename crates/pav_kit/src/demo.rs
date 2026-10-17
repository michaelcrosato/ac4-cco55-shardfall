//! Attract mode: the playlist (data) and the showcase tracker a game runs during a demo
//! segment (which mechanics it has shown, the watchdog for stalls, when the segment is done).

use std::collections::BTreeMap;

use pav_core::play::{DemoSegment, DemoState};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
struct PlaylistFile {
    #[serde(default)]
    segment: Vec<DemoSegment>,
}

/// Parses a playlist (`[[segment]]` tables: name, scene, caption, seeds, seconds).
pub fn parse_playlist(text: &str) -> anyhow::Result<Vec<DemoSegment>> {
    let f: PlaylistFile = toml::from_str(text)?;
    Ok(f.segment)
}

/// What a demo segment has shown so far.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Showcase {
    pub caption: String,
    /// Showcase events by mechanic ("broadside", "counter", "leap"...).
    pub events: BTreeMap<String, u32>,
    pub elapsed: f32,
    /// Seconds since the last showcase event or progress.
    pub since_progress: f32,
    /// Time limit (s) and watchdog (s without progress).
    pub limit: f32,
    pub watchdog: f32,
    pub done: bool,
    pub stalled: bool,
}

impl Showcase {
    pub fn new(caption: &str, limit: f32) -> Self {
        Self { caption: caption.to_string(), limit, watchdog: 25.0, ..Default::default() }
    }

    /// A mechanic was shown (counts as progress).
    pub fn note(&mut self, mechanic: &str) {
        *self.events.entry(mechanic.to_string()).or_default() += 1;
        self.since_progress = 0.0;
    }

    /// Something moved the segment forward without being a showcase event (an objective).
    pub fn progress(&mut self) {
        self.since_progress = 0.0;
    }

    /// Ends the segment (its story is told).
    pub fn finish(&mut self) {
        self.done = true;
    }

    pub fn tick(&mut self, dt: f32) {
        if self.done {
            return;
        }
        self.elapsed += dt;
        self.since_progress += dt;
        if self.since_progress > self.watchdog {
            self.stalled = true;
            self.done = true;
            log::warn!("demo_stall: '{}' made no progress for {:.0} s", self.caption, self.watchdog);
        } else if self.elapsed >= self.limit {
            self.done = true;
        }
    }

    pub fn total(&self) -> u32 {
        self.events.values().sum()
    }

    pub fn state(&self) -> DemoState {
        DemoState {
            caption: self.caption.clone(),
            done: self.done,
            events: self.total(),
            mechanics: self.events.keys().cloned().collect(),
            stalled: self.stalled,
        }
    }
}
