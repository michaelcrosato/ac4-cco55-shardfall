//! The data-driven sound bank (`sounds.toml`): cue name → layers of synthesized sound, plus
//! dialog speakers. Every field is optional; unknown fields are errors (typos fail loudly).
//!
//! ```toml
//! [cue.cannon_fire]
//! about = "a long gun: boom, blast and a slap-back off the water"
//! range = 70                     # metres to -6 dB
//! rumble = { low = 0.9, high = 0.5, ms = 120 }
//! [[cue.cannon_fire.layer]]
//! wave = "sine"                  # sine square saw triangle noise pink brown dust pluck voice
//! freq = 70
//! freq_end = 34
//! decay = 0.9
//! drive = 0.4
//! gain = 0.8
//! [[cue.cannon_fire.layer]]
//! wave = "brown"
//! filter = "lp"
//! cutoff = 2400
//! cutoff_end = 180
//! lfo = [{ to = "amp", rate = 9, depth = 0.4, shape = "random" }]
//!
//! [speaker.bung]
//! pitch = 92
//! vowels = ["ah", "oh", "uh"]
//! ```

use std::collections::BTreeMap;

use anyhow::{Context, bail};
use serde::{Deserialize, Deserializer};

use crate::dsp::{Curve, Formant, Lfo, LfoTo};

/// Mixer buses. Each has a volume (`set_bus`); everything sums into `Master`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Bus {
    Master,
    Music,
    #[default]
    Sfx,
    Ambience,
    Voice,
    Ui,
}

impl Bus {
    pub const ALL: [Bus; 6] = [Bus::Master, Bus::Music, Bus::Sfx, Bus::Ambience, Bus::Voice, Bus::Ui];
    pub fn index(self) -> usize {
        self as usize
    }
    pub fn name(self) -> &'static str {
        ["master", "music", "sfx", "ambience", "voice", "ui"][self as usize]
    }
    pub fn parse(s: &str) -> Option<Bus> {
        Bus::ALL.into_iter().find(|b| b.name() == s)
    }
}

/// Oscillator or source of a layer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Wave {
    #[default]
    Sine,
    Square,
    Saw,
    Triangle,
    /// White noise.
    #[serde(alias = "white")]
    Noise,
    Pink,
    Brown,
    /// Sparse random clicks (`density` per second): crackle, drips, ticks.
    Dust,
    /// Karplus-Strong plucked string (`pluck_decay`, `pluck_bright`).
    Pluck,
    /// Glottal buzz for formant voices (use with `formant`).
    Voice,
}

impl Wave {
    pub fn is_noise(self) -> bool {
        matches!(self, Wave::Noise | Wave::Pink | Wave::Brown | Wave::Dust)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Filter {
    #[default]
    None,
    Lp,
    Bp,
    Hp,
    Notch,
}

/// A pitch contour: up to six (seconds, semitones) points, linearly joined.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PitchCurve {
    pub n: u8,
    pub pts: [[f32; 2]; 6],
}

impl PitchCurve {
    pub fn at(&self, t: f32) -> f32 {
        let n = self.n as usize;
        if n == 0 {
            return 0.0;
        }
        let p = &self.pts[..n];
        if t <= p[0][0] {
            return p[0][1];
        }
        for w in p.windows(2) {
            if t < w[1][0] {
                let k = (t - w[0][0]) / (w[1][0] - w[0][0]).max(1e-6);
                return w[0][1] + (w[1][1] - w[0][1]) * k;
            }
        }
        p[n - 1][1]
    }
}

fn de_curve<'de, D: Deserializer<'de>>(d: D) -> Result<PitchCurve, D::Error> {
    let v: Vec<[f32; 2]> = Vec::deserialize(d)?;
    if v.len() > 6 {
        return Err(serde::de::Error::custom("pitch_curve: at most 6 points"));
    }
    let mut c = PitchCurve { n: v.len() as u8, ..Default::default() };
    for (i, p) in v.into_iter().enumerate() {
        c.pts[i] = p;
    }
    Ok(c)
}

fn de_lfos<'de, D: Deserializer<'de>>(d: D) -> Result<[Lfo; 3], D::Error> {
    let v: Vec<Lfo> = Vec::deserialize(d)?;
    if v.len() > 3 {
        return Err(serde::de::Error::custom("lfo: at most 3 per layer"));
    }
    let mut out = [Lfo::default(); 3];
    for (i, l) in v.into_iter().enumerate() {
        out[i] = l;
    }
    Ok(out)
}

/// One synthesized layer: source → (formants) → filter → drive → envelope.
/// Times are seconds, frequencies Hz, gains linear, `pitch_rand` semitones, `gain_rand` dB.
#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Layer {
    pub wave: Wave,
    pub freq: f32,
    /// Glide target; reached after `glide` seconds (default: attack + decay), exponentially.
    pub freq_end: Option<f32>,
    pub glide: f32,
    /// (seconds, semitones) points on top of `freq` (overrides nothing; adds).
    #[serde(deserialize_with = "de_curve")]
    pub pitch_curve: PitchCurve,
    /// Square duty cycle.
    pub duty: f32,
    /// Detuned copies (1..4) spread over `detune` cents, each wandering by `drift` cents.
    pub unison: u8,
    pub detune: f32,
    pub drift: f32,
    /// A second oscillator at `ratio2` × the pitch, mixed at `gain2`.
    pub wave2: Option<Wave>,
    pub ratio2: f32,
    pub gain2: f32,
    pub detune2: f32,
    /// Phase modulation (sine/triangle carriers): modulator at `fm_ratio` × pitch, index
    /// gliding from `fm_index` to `fm_index_end` over `fm_time` (default: attack + decay).
    pub fm_ratio: f32,
    pub fm_index: f32,
    pub fm_index_end: Option<f32>,
    pub fm_time: f32,
    /// White noise mixed into a tonal source (breath, bow hair, spit).
    pub noise: f32,
    pub attack: f32,
    pub decay: f32,
    pub sustain: f32,
    pub hold: f32,
    pub release: f32,
    pub curve: Curve,
    pub filter: Filter,
    pub cutoff: f32,
    pub cutoff_end: Option<f32>,
    /// Seconds for the cutoff sweep (default: the whole sound).
    pub cutoff_time: f32,
    /// Resonance 0..1 (or `q` directly).
    pub res: f32,
    pub q: Option<f32>,
    /// Cutoff follows pitch: 1 = fully (relative to middle C).
    pub key_track: f32,
    /// Cutoff opens by this many octaves at full envelope.
    pub env_cutoff: f32,
    /// Vowel or body resonances, morphing to `formant_end` over `formant_time`.
    pub formant: Option<Formant>,
    pub formant_end: Option<Formant>,
    pub formant_time: f32,
    pub formant_shift: f32,
    /// Unfiltered signal kept alongside the formants.
    pub formant_dry: f32,
    pub pluck_decay: f32,
    pub pluck_bright: f32,
    /// Dust clicks per second.
    pub density: f32,
    #[serde(deserialize_with = "de_lfos")]
    pub lfo: [Lfo; 3],
    /// Saturation 0..1.
    pub drive: f32,
    pub gain: f32,
    pub delay: f32,
    pub pan: f32,
    /// Random pan ± per play.
    pub spread: f32,
    /// Independent noise per channel (wide beds).
    pub stereo: bool,
    pub pitch_rand: f32,
    pub gain_rand: f32,
    pub delay_rand: f32,
    /// Play this layer `repeat` times, `repeat_gap` apart (each re-randomised).
    pub repeat: u8,
    pub repeat_gap: f32,
    /// 0 = always; otherwise one variant number is picked per play.
    pub variant: u8,
    pub bus: Option<Bus>,
    pub reverb: Option<f32>,
    pub echo: Option<f32>,
}

impl Default for Layer {
    fn default() -> Self {
        Self {
            wave: Wave::Sine,
            freq: 440.0,
            freq_end: None,
            glide: 0.0,
            pitch_curve: PitchCurve::default(),
            duty: 0.5,
            unison: 1,
            detune: 0.0,
            drift: 0.0,
            wave2: None,
            ratio2: 1.0,
            gain2: 0.5,
            detune2: 0.0,
            fm_ratio: 1.0,
            fm_index: 0.0,
            fm_index_end: None,
            fm_time: 0.0,
            noise: 0.0,
            attack: 0.002,
            decay: 0.2,
            sustain: 0.0,
            hold: 0.0,
            release: 0.08,
            curve: Curve::Exp,
            filter: Filter::None,
            cutoff: 2000.0,
            cutoff_end: None,
            cutoff_time: 0.0,
            res: 0.0,
            q: None,
            key_track: 0.0,
            env_cutoff: 0.0,
            formant: None,
            formant_end: None,
            formant_time: 0.0,
            formant_shift: 1.0,
            formant_dry: 0.0,
            pluck_decay: 1.0,
            pluck_bright: 0.5,
            density: 30.0,
            lfo: [Lfo::default(); 3],
            drive: 0.0,
            gain: 0.3,
            delay: 0.0,
            pan: 0.0,
            spread: 0.0,
            stereo: false,
            pitch_rand: 0.0,
            gain_rand: 0.0,
            delay_rand: 0.0,
            repeat: 1,
            repeat_gap: 0.05,
            variant: 0,
            bus: None,
            reverb: None,
            echo: None,
        }
    }
}

impl Layer {
    /// Seconds a one-shot lasts (to about -74 dB).
    pub fn length(&self) -> f32 {
        let tail = if self.curve == Curve::Exp { 1.4 } else { 1.0 };
        let body = self.attack + self.decay + self.hold + self.release * tail;
        if self.wave == Wave::Pluck { body.max(self.pluck_decay.min(8.0)) } else { body }
    }
    pub fn q_value(&self) -> f32 {
        self.q.unwrap_or(0.5 / (1.0 - self.res.clamp(0.0, 0.98)))
    }
    pub fn has_lfo(&self, to: LfoTo) -> bool {
        self.lfo.iter().any(|l| l.to == to && l.depth != 0.0)
    }
    fn check(&mut self, what: &str) -> anyhow::Result<()> {
        self.unison = self.unison.clamp(1, 4);
        self.repeat = self.repeat.clamp(1, 32);
        for (name, v) in [("attack", self.attack), ("decay", self.decay), ("hold", self.hold), ("release", self.release)] {
            if !(0.0..=60.0).contains(&v) {
                bail!("{what}: {name} = {v} (0..60 s)");
            }
        }
        if self.freq.is_nan() || self.freq <= 0.0 {
            bail!("{what}: freq must be > 0");
        }
        if self.gain < 0.0 || self.gain > 8.0 {
            bail!("{what}: gain = {} (0..8)", self.gain);
        }
        Ok(())
    }
}

/// Controller rumble for a cue: low/high motor strength (0..1) and duration.
#[derive(Clone, Copy, Debug, Default, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Rumble {
    pub low: f32,
    pub high: f32,
    pub ms: u32,
}

/// A named sound: layers plus how it is placed and randomised.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Cue {
    pub about: String,
    pub layer: Vec<Layer>,
    pub gain: f32,
    pub bus: Bus,
    /// Distance (m) at which it is 6 dB down.
    pub range: f32,
    /// Panned and attenuated by the listener (default: true on sfx, false elsewhere).
    pub positional: Option<bool>,
    /// Whole-cue randomisation per play (semitones, dB).
    pub pitch_rand: f32,
    pub gain_rand: f32,
    pub rumble: Option<Rumble>,
    /// A music stinger (music.toml) to play with it.
    pub stinger: Option<String>,
    /// Minimum seconds between plays (spam guard).
    pub cooldown: f32,
    /// Default reverb / echo sends for its layers.
    pub reverb: Option<f32>,
    pub echo: Option<f32>,
    #[serde(skip)]
    pub variants: u8,
}

impl Default for Cue {
    fn default() -> Self {
        Self {
            about: String::new(),
            layer: Vec::new(),
            gain: 1.0,
            bus: Bus::Sfx,
            range: 14.0,
            positional: None,
            pitch_rand: 0.0,
            gain_rand: 0.0,
            rumble: None,
            stinger: None,
            cooldown: 0.0,
            reverb: None,
            echo: None,
            variants: 0,
        }
    }
}

impl Cue {
    pub fn is_positional(&self) -> bool {
        self.positional.unwrap_or(self.bus == Bus::Sfx)
    }
    /// Longest layer (with delays and repeats), seconds.
    pub fn length(&self) -> f32 {
        self.layer
            .iter()
            .map(|l| l.delay + l.delay_rand + l.repeat_gap * (l.repeat as f32 - 1.0) + l.length())
            .fold(0.0, f32::max)
    }
}

/// A dialog voice: formant blips at `pitch` Hz on `vowels`.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Speaker {
    pub about: String,
    pub pitch: f32,
    pub vowels: Vec<Formant>,
    /// Formant scale (>1 = smaller, brighter head).
    pub shift: f32,
    /// Gravel: pitch jitter and breath (0..1).
    pub rough: f32,
    pub breath: f32,
    /// Blip length scale (1 = about 90 ms).
    pub speed: f32,
    /// Pitch variation between blips, semitones.
    pub range: f32,
    pub gain: f32,
    /// Singers in unison (a crowd).
    pub unison: u8,
    /// Use this cue instead of a formant blip (the parrot).
    pub cue: Option<String>,
}

impl Default for Speaker {
    fn default() -> Self {
        Self {
            about: String::new(),
            pitch: 130.0,
            vowels: vec![Formant::Ah, Formant::Eh, Formant::Oh],
            shift: 1.0,
            rough: 0.15,
            breath: 0.05,
            speed: 1.0,
            range: 3.0,
            gain: 1.0,
            unison: 1,
            cue: None,
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BankFile {
    #[serde(default)]
    cue: BTreeMap<String, Cue>,
    #[serde(default)]
    speaker: BTreeMap<String, Speaker>,
}

/// Parsed `sounds.toml`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Bank {
    pub cues: BTreeMap<String, Cue>,
    pub speakers: BTreeMap<String, Speaker>,
}

impl Bank {
    pub fn parse(text: &str) -> anyhow::Result<Bank> {
        let f: BankFile = toml::from_str(text).context("sound bank")?;
        let mut cues = f.cue;
        for (name, cue) in &mut cues {
            if cue.layer.is_empty() && cue.stinger.is_none() {
                bail!("cue {name}: no layers");
            }
            for (i, l) in cue.layer.iter_mut().enumerate() {
                l.check(&format!("cue {name} layer {}", i + 1))?;
            }
            cue.variants = cue.layer.iter().map(|l| l.variant).max().unwrap_or(0);
        }
        for (name, s) in &f.speaker {
            if s.cue.is_none() && s.vowels.is_empty() {
                bail!("speaker {name}: no vowels");
            }
            if let Some(c) = &s.cue {
                if !cues.contains_key(c) {
                    bail!("speaker {name}: cue {c} is not in the bank");
                }
            }
        }
        Ok(Bank { cues, speakers: f.speaker })
    }
    pub fn cue(&self, name: &str) -> Option<&Cue> {
        self.cues.get(name)
    }
    pub fn rumble(&self, name: &str) -> Option<Rumble> {
        self.cues.get(name).and_then(|c| c.rumble)
    }
}
