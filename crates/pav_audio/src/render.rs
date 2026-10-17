//! Offline rendering for tests and tools: the same [`Mixer`] as the live callback, driven by a
//! timeline of commands. Output is interleaved stereo f32 at [`SAMPLE_RATE`].

use std::sync::Arc;

use glam::Vec3;
use pav_core::play::{AudioState, MusicCue};

use crate::Listener;
use crate::ambience::AmbienceBank;
use crate::bank::Bank;
use crate::mixer::{Cmd, Mixer, Stats};
use crate::music::MusicBank;

pub const SAMPLE_RATE: f32 = 48000.0;

/// An offline [`crate::AudioOut`]: queue commands at times, then render.
pub struct Renderer {
    pub mixer: Mixer,
    timeline: Vec<(u64, Cmd)>,
    pos: u64,
}

impl Default for Renderer {
    fn default() -> Self {
        Self::new()
    }
}

impl Renderer {
    pub fn new() -> Self {
        Self::with_rate(SAMPLE_RATE)
    }
    pub fn with_rate(sample_rate: f32) -> Self {
        Self { mixer: Mixer::new(sample_rate), timeline: Vec::new(), pos: 0 }
    }
    /// Queues a command `t` seconds from the start (applied on that exact sample).
    pub fn at(&mut self, t: f64, cmd: Cmd) -> &mut Self {
        let s = (t * self.mixer.sample_rate as f64).round().max(0.0) as u64;
        let i = self.timeline.partition_point(|(ts, _)| *ts <= s);
        self.timeline.insert(i, (s, cmd));
        self
    }
    /// Renders the next `seconds`, running queued commands on time.
    pub fn render(&mut self, seconds: f32) -> Vec<f32> {
        let total = (seconds * self.mixer.sample_rate) as u64;
        let mut out = vec![0.0f32; total as usize * 2];
        let end = self.pos + total;
        let mut done = 0u64;
        while self.pos < end {
            while let Some((t, _)) = self.timeline.first() {
                if *t > self.pos {
                    break;
                }
                let (_, c) = self.timeline.remove(0);
                self.mixer.handle(c);
            }
            let next = self.timeline.first().map(|(t, _)| *t).unwrap_or(u64::MAX).min(end);
            let n = (next - self.pos).max(1).min(end - self.pos);
            let a = done as usize * 2;
            let b = (done + n) as usize * 2;
            self.mixer.render(&mut out[a..b]);
            done += n;
            self.pos += n;
        }
        out
    }
    pub fn stats(&self) -> Stats {
        self.mixer.stats()
    }
}

/// The audio state that plays a music state or song at an intensity.
pub fn music_state(song: &str, intensity: f32) -> AudioState {
    AudioState { music: Some(MusicCue { song: song.to_string(), intensity }), ..Default::default() }
}

/// Renders `seconds` of a music state or song.
pub fn render_song(music: &Arc<MusicBank>, song: &str, intensity: f32, seconds: f32) -> Vec<f32> {
    let mut r = Renderer::new();
    r.mixer.load_music(music.clone());
    r.at(0.0, Cmd::Apply(Box::new(music_state(song, intensity)), Listener::default()));
    r.render(seconds)
}

/// Renders cues at times (seconds), heard from the origin.
pub fn render_cues(bank: &Arc<Bank>, cues: &[(f32, &str)], seconds: f32) -> Vec<f32> {
    let mut r = Renderer::new();
    r.mixer.load_bank(bank.clone());
    for (t, name) in cues {
        let c = Cmd::Cue {
            name: name.to_string(),
            pos: Vec3::new(0.0, 0.0, -2.0),
            gain: 1.0,
            pitch: 1.0,
            listener: Listener::default(),
        };
        r.at(*t as f64, c);
    }
    r.render(seconds)
}

/// Renders ambience layers at fixed levels.
pub fn render_ambience(amb: &Arc<AmbienceBank>, bank: &Arc<Bank>, levels: &[(&str, f32)], seconds: f32) -> Vec<f32> {
    let mut r = Renderer::new();
    r.mixer.load_bank(bank.clone());
    r.mixer.load_ambience(amb.clone());
    let st = AudioState { ambience: levels.iter().map(|(n, l)| (n.to_string(), *l)).collect(), ..Default::default() };
    r.at(0.0, Cmd::Apply(Box::new(st), Listener::default()));
    r.render(seconds)
}
