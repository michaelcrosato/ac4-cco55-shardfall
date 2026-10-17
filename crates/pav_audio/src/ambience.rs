//! Ambience (`ambience.toml`): named layers the game sets a level for each frame. A layer
//! has looping beds (held layers, faded by the level) and random-timed events (cues from the
//! sound bank or inline layers). Timing is seeded, so offline renders repeat exactly.
//!
//! ```toml
//! [layer.wind]
//! about = "band-passed noise; louder and higher with wind speed"
//! level_cutoff = 1.2           # octaves of filter sweep from level 0 to 1
//! [[layer.wind.bed]]
//! wave = "pink"
//! filter = "bp"
//! cutoff = 700
//! stereo = true
//! [[layer.gulls.event]]
//! cue = "gull_cry"
//! every = [3.0, 9.0]
//! ```

use std::collections::BTreeMap;
use std::sync::Arc;

use anyhow::{Context, bail};
use serde::Deserialize;

use crate::bank::Layer;
use crate::dsp::{Rng, hash_str};

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AmbEvent {
    /// A cue from the sound bank...
    pub cue: Option<String>,
    /// ...or inline layers.
    pub layer: Vec<Layer>,
    /// Seconds between events (min, max), at full level.
    pub every: [f32; 2],
    /// Below this level the event never fires.
    pub min_level: f32,
    /// More frequent at higher levels.
    pub by_level: bool,
    pub gain: f32,
    pub gain_rand: f32,
    pub pitch_rand: f32,
    /// Random pan ±.
    pub spread: f32,
}

impl Default for AmbEvent {
    fn default() -> Self {
        Self {
            cue: None,
            layer: Vec::new(),
            every: [4.0, 10.0],
            min_level: 0.05,
            by_level: true,
            gain: 1.0,
            gain_rand: 2.0,
            pitch_rand: 1.0,
            spread: 0.7,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AmbLayer {
    pub about: String,
    pub bed: Vec<Layer>,
    pub event: Vec<AmbEvent>,
    pub gain: f32,
    /// Seconds for the level to follow the game's value.
    pub fade: f32,
    /// Gain = level ^ curve.
    pub curve: f32,
    /// Octaves of pitch and of filter cutoff moved across level 0..1 (centred on 0.5).
    pub level_pitch: f32,
    pub level_cutoff: f32,
}

impl Default for AmbLayer {
    fn default() -> Self {
        Self {
            about: String::new(),
            bed: Vec::new(),
            event: Vec::new(),
            gain: 1.0,
            fade: 1.0,
            curve: 1.0,
            level_pitch: 0.0,
            level_cutoff: 0.0,
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AmbFile {
    #[serde(default)]
    layer: BTreeMap<String, AmbLayer>,
}

/// Parsed `ambience.toml`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AmbienceBank {
    pub layers: Vec<(String, AmbLayer)>,
}

impl AmbienceBank {
    pub fn parse(text: &str) -> anyhow::Result<AmbienceBank> {
        let f: AmbFile = toml::from_str(text).context("ambience.toml")?;
        for (name, l) in &f.layer {
            if l.bed.is_empty() && l.event.is_empty() {
                bail!("ambience layer {name}: no beds or events");
            }
            if l.bed.len() > 15 {
                bail!("ambience layer {name}: at most 15 beds");
            }
            for e in &l.event {
                if e.cue.is_none() && e.layer.is_empty() {
                    bail!("ambience layer {name}: an event needs a cue or layers");
                }
                if !(e.every[0] > 0.0 && e.every[1] >= e.every[0]) {
                    bail!("ambience layer {name}: every = [min, max] seconds, min > 0");
                }
            }
        }
        Ok(AmbienceBank { layers: f.layer.into_iter().collect() })
    }
    pub fn index(&self, name: &str) -> Option<usize> {
        self.layers.iter().position(|(n, _)| n == name)
    }
    /// Cues the events refer to (to check against the sound bank).
    pub fn cues(&self) -> Vec<String> {
        let mut v: Vec<String> = self.layers.iter().flat_map(|(_, l)| l.event.iter().filter_map(|e| e.cue.clone())).collect();
        v.sort();
        v.dedup();
        v
    }
}

#[derive(Clone, Debug, Default)]
struct Rt {
    level: f32,
    target: f32,
    started: bool,
    next: Vec<f32>,
}

/// What the ambience wants the mixer to do this block.
#[derive(Clone, Debug)]
pub enum AmbAction {
    /// Start bed `bed` of layer `layer` (held).
    Bed { layer: usize, bed: usize },
    /// Release the beds of a layer.
    Stop { layer: usize },
    /// A one-shot: (layer, event index, gain, pitch semitones, pan).
    Event { layer: usize, event: usize, gain: f32, semis: f32, pan: f32 },
}

#[derive(Clone, Debug)]
pub struct AmbiencePlayer {
    pub bank: Option<Arc<AmbienceBank>>,
    rt: Vec<Rt>,
    rng: Rng,
    pub warned: Vec<String>,
}

impl Default for AmbiencePlayer {
    fn default() -> Self {
        Self { bank: None, rt: Vec::new(), rng: Rng::new(0xa3b1_e2c4), warned: Vec::new() }
    }
}

impl AmbiencePlayer {
    pub fn load(&mut self, bank: Arc<AmbienceBank>) {
        let old: Vec<(String, f32)> = match &self.bank {
            Some(b) => b.layers.iter().zip(&self.rt).map(|((n, _), r)| (n.clone(), r.target)).collect(),
            None => Vec::new(),
        };
        self.rt = bank
            .layers
            .iter()
            .map(|(name, l)| Rt {
                level: 0.0,
                target: old.iter().find(|(n, _)| n == name).map(|o| o.1).unwrap_or(0.0),
                started: false,
                next: vec![f32::NAN; l.event.len()],
            })
            .collect();
        self.rng = Rng::new(0xa3b1_e2c4);
        self.bank = Some(bank);
    }

    /// Sets the levels: layers not listed go to 0.
    pub fn set_levels(&mut self, levels: &[(String, f32)]) {
        let Some(bank) = self.bank.clone() else { return };
        for r in &mut self.rt {
            r.target = 0.0;
        }
        for (name, lvl) in levels {
            match bank.index(name) {
                Some(i) => self.rt[i].target = lvl.clamp(0.0, 1.0),
                None => {
                    if !self.warned.iter().any(|w| w == name) {
                        log::warn!("ambience: no layer {name:?}");
                        self.warned.push(name.clone());
                    }
                }
            }
        }
    }

    pub fn set_level(&mut self, name: &str, level: f32) {
        if let Some(i) = self.bank.as_ref().and_then(|b| b.index(name)) {
            self.rt[i].target = level.clamp(0.0, 1.0);
        }
    }

    /// (gain, pitch multiplier, cutoff multiplier) for a layer's beds.
    pub fn bed_mod(&self, layer: usize) -> Option<(f32, f32, f32)> {
        let bank = self.bank.as_ref()?;
        let (_, l) = bank.layers.get(layer)?;
        let r = self.rt.get(layer)?;
        let x = r.level - 0.5;
        Some((r.level.powf(l.curve.max(0.1)) * l.gain, 2f32.powf(x * l.level_pitch), 2f32.powf(x * l.level_cutoff)))
    }

    pub fn process(&mut self, n: usize, sr: f32, alive: &[u8], out: &mut Vec<AmbAction>) {
        let Some(bank) = self.bank.clone() else { return };
        for (i, (name, l)) in bank.layers.iter().enumerate() {
            let r = &mut self.rt[i];
            let k = 1.0 - (-(n as f32) / (sr * l.fade.max(0.02))).exp();
            r.level += (r.target - r.level) * k;
            if (r.level - r.target).abs() < 1e-4 {
                r.level = r.target;
            }
            if r.level > 0.002 {
                if !l.bed.is_empty() {
                    for b in 0..l.bed.len() {
                        let a = alive.get(i * 16 + b).copied().unwrap_or(0);
                        if a == 0 {
                            out.push(AmbAction::Bed { layer: i, bed: b });
                        }
                    }
                }
                r.started = true;
            } else if r.started && r.target <= 0.0 {
                r.started = false;
                out.push(AmbAction::Stop { layer: i });
            }
            for (ei, e) in l.event.iter().enumerate() {
                if r.level < e.min_level {
                    r.next[ei] = f32::NAN;
                    continue;
                }
                if r.next[ei].is_nan() {
                    // First time: somewhere in the interval (seeded by name).
                    let mut seed = Rng::new(hash_str(name) ^ ei as u64 ^ self.rng.next_u64());
                    r.next[ei] = seed.range(e.every[0] * 0.3, e.every[1]);
                    continue;
                }
                // Count down in full-rate seconds: busier at higher levels.
                let rate = if e.by_level { r.level.clamp(0.25, 1.0) } else { 1.0 };
                r.next[ei] -= n as f32 / sr * rate;
                if r.next[ei] <= 0.0 {
                    let gain = e.gain * r.level.powf(l.curve.max(0.1)) * l.gain * crate::dsp::db(self.rng.bi() * e.gain_rand);
                    let semis = self.rng.bi() * e.pitch_rand;
                    let pan = self.rng.bi() * e.spread;
                    out.push(AmbAction::Event { layer: i, event: ei, gain, semis, pan });
                    r.next[ei] = self.rng.range(e.every[0], e.every[1]);
                }
            }
        }
    }
}
