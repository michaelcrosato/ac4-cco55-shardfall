//! Signal-processing building blocks: seeded random numbers and noise, band-limited
//! oscillators, the state-variable filter, envelopes, LFOs, Karplus-Strong strings, formant
//! banks, and the master effects (reverb, echo, limiter). Everything is deterministic: no
//! clock, no threads, no allocation after construction (except string buffers on spawn).

#![allow(clippy::needless_range_loop)]

use std::f32::consts::{PI, TAU};

use serde::Deserialize;

// ---------------------------------------------------------------- random

/// xorshift64*: small, fast, seeded. Used for every random choice (never the clock).
#[derive(Clone, Copy, Debug)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng((seed ^ 0x9E37_79B9_7F4A_7C15) | 1)
    }
    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    pub fn next_u32(&mut self) -> u32 {
        (self.next_u64() >> 32) as u32
    }
    /// Uniform in [0, 1).
    pub fn unit(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 * (1.0 / 16_777_216.0)
    }
    /// Uniform in [-1, 1).
    pub fn bi(&mut self) -> f32 {
        self.unit() * 2.0 - 1.0
    }
    pub fn range(&mut self, a: f32, b: f32) -> f32 {
        a + (b - a) * self.unit()
    }
    pub fn below(&mut self, n: usize) -> usize {
        if n == 0 { 0 } else { (self.next_u32() as usize) % n }
    }
}

/// FNV-1a, for seeds derived from names.
pub fn hash_str(s: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

pub fn midi_hz(m: f32) -> f32 {
    440.0 * 2f32.powf((m - 69.0) / 12.0)
}

pub fn db(d: f32) -> f32 {
    10f32.powf(d / 20.0)
}

/// Rational tanh: a cheap soft clipper (exact enough below |x| = 3, clamps above).
#[inline]
pub fn soft(x: f32) -> f32 {
    let x = x.clamp(-3.0, 3.0);
    x * (27.0 + x * x) / (27.0 + 9.0 * x * x)
}

// ---------------------------------------------------------------- noise

/// White, pink (Kellet), brown (leaky integrator) and dust (sparse impulses), each scaled to
/// roughly the same loudness (RMS about 0.3).
#[derive(Clone, Copy, Debug, Default)]
pub struct Noise {
    s: u32,
    pink: [f32; 7],
    brown: f32,
}

impl Noise {
    pub fn new(seed: u32) -> Self {
        Self { s: seed | 1, pink: [0.0; 7], brown: 0.0 }
    }
    #[inline]
    pub fn white(&mut self) -> f32 {
        self.s ^= self.s << 13;
        self.s ^= self.s >> 17;
        self.s ^= self.s << 5;
        (self.s as i32) as f32 * (1.0 / 2_147_483_648.0)
    }
    #[inline]
    pub fn unit(&mut self) -> f32 {
        self.white() * 0.5 + 0.5
    }
    #[inline]
    pub fn pink(&mut self) -> f32 {
        let w = self.white();
        let b = &mut self.pink;
        b[0] = 0.99886 * b[0] + w * 0.055_517_9;
        b[1] = 0.99332 * b[1] + w * 0.075_075_9;
        b[2] = 0.969 * b[2] + w * 0.153_852;
        b[3] = 0.8665 * b[3] + w * 0.310_485_6;
        b[4] = 0.55 * b[4] + w * 0.532_952_2;
        b[5] = -0.7616 * b[5] - w * 0.016_898;
        let out = b[0] + b[1] + b[2] + b[3] + b[4] + b[5] + b[6] + w * 0.5362;
        b[6] = w * 0.115_926;
        out * 0.105
    }
    #[inline]
    pub fn brown(&mut self) -> f32 {
        let w = self.white();
        self.brown = (self.brown + 0.02 * w) * (1.0 / 1.02);
        self.brown * 5.0
    }
    /// Sparse random impulses: `p` is the chance per sample.
    #[inline]
    pub fn dust(&mut self, p: f32) -> f32 {
        let u = self.unit();
        if u < p {
            let a = self.white();
            a.signum() * (0.4 + 0.6 * a.abs()) * 2.0
        } else {
            0.0
        }
    }
}

// ---------------------------------------------------------------- oscillators

#[inline]
fn blep(t: f32, dt: f32) -> f32 {
    if t < dt {
        let t = t / dt;
        t + t - t * t - 1.0
    } else if t > 1.0 - dt {
        let t = (t - 1.0) / dt;
        t * t + t + t + 1.0
    } else {
        0.0
    }
}

/// Band-limited (PolyBLEP) saw for phase `t` in [0,1) and increment `dt`.
#[inline]
pub fn saw(t: f32, dt: f32) -> f32 {
    2.0 * t - 1.0 - blep(t, dt)
}

/// Band-limited (PolyBLEP) pulse with duty cycle `duty`.
#[inline]
pub fn pulse(t: f32, dt: f32, duty: f32) -> f32 {
    let naive = if t < duty { 1.0 } else { -1.0 };
    let mut t2 = t + 1.0 - duty;
    if t2 >= 1.0 {
        t2 -= 1.0;
    }
    naive + blep(t, dt) - blep(t2, dt)
}

#[inline]
pub fn tri(t: f32) -> f32 {
    1.0 - 4.0 * (t - 0.5).abs()
}

#[inline]
pub fn sine(t: f32) -> f32 {
    (t * TAU).sin()
}

// ---------------------------------------------------------------- filters

/// Coefficients of a TPT state-variable filter (Simper): stable under fast modulation.
#[derive(Clone, Copy, Debug, Default)]
pub struct SvfCoef {
    a1: f32,
    a2: f32,
    a3: f32,
    pub k: f32,
}

impl SvfCoef {
    pub fn new(fc: f32, q: f32, sr: f32) -> Self {
        let fc = fc.clamp(8.0, sr * 0.47);
        let g = (PI * fc / sr).tan();
        let k = 1.0 / q.max(0.05);
        let a1 = 1.0 / (1.0 + g * (g + k));
        let a2 = g * a1;
        let a3 = g * a2;
        Self { a1, a2, a3, k }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Svf {
    ic1: f32,
    ic2: f32,
}

impl Svf {
    /// Returns (low-pass, band-pass, high-pass).
    #[inline]
    pub fn tick(&mut self, c: &SvfCoef, v0: f32) -> (f32, f32, f32) {
        let v3 = v0 - self.ic2;
        let v1 = c.a1 * self.ic1 + c.a2 * v3;
        let v2 = self.ic2 + c.a2 * self.ic1 + c.a3 * v3;
        self.ic1 = 2.0 * v1 - self.ic1;
        self.ic2 = 2.0 * v2 - self.ic2;
        (v2, v1, v0 - c.k * v1 - v2)
    }
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

/// One-pole low-pass.
#[derive(Clone, Copy, Debug, Default)]
pub struct OnePole {
    pub y: f32,
}

impl OnePole {
    pub fn coef(fc: f32, sr: f32) -> f32 {
        1.0 - (-TAU * fc.clamp(1.0, sr * 0.49) / sr).exp()
    }
    #[inline]
    pub fn tick(&mut self, a: f32, x: f32) -> f32 {
        self.y += a * (x - self.y);
        self.y
    }
}

// ---------------------------------------------------------------- envelope

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Curve {
    /// Exponential decay and release (natural; the times are to about -48 dB).
    #[default]
    Exp,
    /// Straight lines (the original synth's shape).
    Lin,
}

const ENV_K: f32 = 5.5;

/// Attack (linear) → decay toward sustain → hold → release.
#[derive(Clone, Copy, Debug, Default)]
pub struct Env {
    stage: u8,
    pub level: f32,
    t: f32,
    att_inc: f32,
    dec_coef: f32,
    rel_coef: f32,
    dec_inc: f32,
    rel_inc: f32,
    sustain: f32,
    auto_release: f32,
    lin: bool,
}

const ATTACK: u8 = 0;
const DECAY: u8 = 1;
const RELEASE: u8 = 2;
const DONE: u8 = 3;

impl Env {
    /// `auto` = release by itself after attack + decay + hold (one-shots); otherwise it holds
    /// the sustain level until [`Env::release`].
    pub fn new(attack: f32, decay: f32, sustain: f32, hold: f32, release: f32, curve: Curve, auto: bool, sr: f32) -> Self {
        let dt = 1.0 / sr;
        let lin = curve == Curve::Lin;
        let mut e = Env {
            stage: ATTACK,
            level: 0.0,
            t: 0.0,
            att_inc: if attack <= dt { 1.0 } else { dt / attack },
            dec_coef: (-ENV_K * dt / decay.max(dt)).exp(),
            rel_coef: (-ENV_K * dt / release.max(dt)).exp(),
            dec_inc: dt / decay.max(dt),
            rel_inc: dt / release.max(dt),
            sustain: sustain.clamp(0.0, 1.0),
            auto_release: if auto { decay.max(0.0) + hold.max(0.0) } else { f32::INFINITY },
            lin,
        };
        if attack <= dt {
            e.level = 1.0;
            e.stage = DECAY;
        }
        e
    }

    #[inline]
    pub fn next(&mut self, dt: f32) -> f32 {
        match self.stage {
            ATTACK => {
                self.level += self.att_inc;
                if self.level >= 1.0 {
                    self.level = 1.0;
                    self.stage = DECAY;
                }
            }
            DECAY => {
                if self.lin {
                    if self.level > self.sustain {
                        self.level = (self.level - self.dec_inc * (1.0 - self.sustain)).max(self.sustain);
                    }
                } else {
                    self.level = self.sustain + (self.level - self.sustain) * self.dec_coef;
                }
                self.t += dt;
                if self.t >= self.auto_release {
                    self.stage = RELEASE;
                    self.rel_inc *= self.level.max(1e-3);
                }
            }
            RELEASE => {
                if self.lin {
                    self.level -= self.rel_inc;
                    if self.level <= 0.0 {
                        self.level = 0.0;
                        self.stage = DONE;
                    }
                } else {
                    self.level *= self.rel_coef;
                    if self.level < 2e-4 {
                        self.level = 0.0;
                        self.stage = DONE;
                    }
                }
            }
            _ => {}
        }
        self.level
    }

    pub fn release(&mut self) {
        if self.stage < RELEASE {
            self.rel_inc *= self.level.max(1e-3);
            self.stage = RELEASE;
        }
    }
    /// Fast fade (used when a voice is cut or stolen).
    pub fn kill(&mut self, sr: f32) {
        self.rel_coef = (-ENV_K / (0.012 * sr)).exp();
        self.rel_inc = self.level.max(1e-3) / (0.012 * sr);
        self.stage = RELEASE;
    }
    pub fn done(&self) -> bool {
        self.stage == DONE
    }
    pub fn released(&self) -> bool {
        self.stage >= RELEASE
    }
}

// ---------------------------------------------------------------- LFO

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LfoTo {
    #[default]
    None,
    /// Semitones.
    Pitch,
    /// 0..1 dip depth.
    Amp,
    /// Octaves.
    Cutoff,
    /// Pan offset.
    Pan,
    /// FM index multiplier.
    Fm,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LfoShape {
    #[default]
    Sine,
    Triangle,
    Square,
    Saw,
    /// Smooth random wander.
    Random,
    /// Stepped random (sample and hold).
    Hold,
}

/// A low-frequency oscillator: `to` what, `rate` Hz, `depth` (units per target), `delay` seconds
/// of fade-in (delayed vibrato), and `phase` (0..1; omitted = random per play).
#[derive(Clone, Copy, Debug, Default, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Lfo {
    pub to: LfoTo,
    pub rate: f32,
    pub depth: f32,
    pub shape: LfoShape,
    pub delay: f32,
    pub phase: Option<f32>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct LfoState {
    pub phase: f32,
    a: f32,
    b: f32,
}

impl LfoState {
    pub fn new(l: &Lfo, rng: &mut Rng) -> Self {
        Self { phase: l.phase.unwrap_or_else(|| rng.unit()), a: rng.bi(), b: rng.bi() }
    }
    /// Advances by `dt` seconds and returns the value in [-1, 1] (times the fade-in).
    pub fn step(&mut self, l: &Lfo, dt: f32, t: f32, noise: &mut Noise) -> f32 {
        self.phase += l.rate * dt;
        if self.phase >= 1.0 {
            self.phase -= self.phase.floor();
            self.a = self.b;
            self.b = noise.white();
        }
        let p = self.phase;
        let v = match l.shape {
            LfoShape::Sine => sine(p),
            LfoShape::Triangle => tri(p),
            LfoShape::Square => {
                if p < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            }
            LfoShape::Saw => 2.0 * p - 1.0,
            LfoShape::Random => {
                let s = 0.5 - 0.5 * (p * PI).cos();
                self.a + (self.b - self.a) * s
            }
            LfoShape::Hold => self.a,
        };
        let fade = if l.delay > 0.0 { ((t - l.delay * 0.5) / (l.delay * 0.5)).clamp(0.0, 1.0) } else { 1.0 };
        v * fade
    }
}

// ---------------------------------------------------------------- Karplus-Strong

/// A plucked string: a delay line one period long with a damped averaging loop.
#[derive(Clone, Debug, Default)]
pub struct Ks {
    buf: Vec<f32>,
    w: usize,
    delay: f32,
    prev: f32,
    loss: f32,
    s: f32,
}

impl Ks {
    /// `t60` = seconds to fade 60 dB; `bright` 0..1.
    pub fn start(&mut self, freq: f32, t60: f32, bright: f32, sr: f32, noise: &mut Noise) {
        let freq = freq.clamp(20.0, sr * 0.25);
        let s = 0.5 + 0.45 * bright.clamp(0.0, 1.0);
        // The averaging filter adds (1 - s) samples of delay.
        let delay = (sr / freq - (1.0 - s)).max(2.0);
        let n = delay.ceil() as usize + 3;
        self.buf.clear();
        self.buf.resize(n, 0.0);
        // Excitation: filtered noise, brighter when `bright` is high, DC removed.
        let a = 0.2 + 0.8 * bright.clamp(0.0, 1.0);
        let mut lp = 0.0;
        let len = delay as usize;
        let mut mean = 0.0;
        for i in 0..len {
            lp += a * (noise.white() - lp);
            self.buf[i] = lp;
            mean += lp;
        }
        mean /= len.max(1) as f32;
        let mut peak = 1e-6f32;
        for v in &mut self.buf[..len] {
            *v -= mean;
            peak = peak.max(v.abs());
        }
        for v in &mut self.buf[..len] {
            *v /= peak;
        }
        self.w = len;
        self.delay = delay;
        self.prev = 0.0;
        self.s = s;
        self.loss = 10f32.powf(-3.0 / (t60.max(0.02) * freq));
    }
    /// Damps the string (a hand on it).
    pub fn mute(&mut self, freq: f32) {
        self.loss = self.loss.min(10f32.powf(-3.0 / (0.08 * freq.max(20.0))));
    }
    #[inline]
    pub fn tick(&mut self) -> f32 {
        let n = self.buf.len();
        if n < 4 {
            return 0.0;
        }
        let r = self.w as f32 - self.delay;
        let r = if r < 0.0 { r + n as f32 } else { r };
        let i0 = r as usize % n;
        let i1 = (i0 + 1) % n;
        let f = r - r.floor();
        let out = self.buf[i0] + (self.buf[i1] - self.buf[i0]) * f;
        let y = self.loss * (self.s * out + (1.0 - self.s) * self.prev);
        self.prev = out;
        self.buf[self.w] = y;
        self.w = (self.w + 1) % n;
        out
    }
}

// ---------------------------------------------------------------- formants

/// Vowels (and instrument bodies) for the formant bank.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Formant {
    #[default]
    Ah,
    Eh,
    Ee,
    Oh,
    Oo,
    Uh,
    /// Closed-mouth hum.
    Mm,
    /// Fiddle body resonances.
    Violin,
    /// Cello body.
    Cello,
    /// Brass bell.
    Horn,
}

impl Formant {
    pub const VOWELS: [Formant; 6] = [Formant::Ah, Formant::Eh, Formant::Ee, Formant::Oh, Formant::Oo, Formant::Uh];

    pub fn parse(s: &str) -> Option<Formant> {
        Some(match s {
            "ah" | "a" => Formant::Ah,
            "eh" | "e" => Formant::Eh,
            "ee" | "i" => Formant::Ee,
            "oh" | "o" => Formant::Oh,
            "oo" | "u" => Formant::Oo,
            "uh" => Formant::Uh,
            "mm" | "m" | "hum" => Formant::Mm,
            "violin" => Formant::Violin,
            "cello" => Formant::Cello,
            "horn" => Formant::Horn,
            _ => return None,
        })
    }

    /// Four (frequency Hz, bandwidth Hz, gain dB) resonances. Vowels follow the classic
    /// tenor formant tables; bodies are broad humps.
    pub fn table(self) -> [(f32, f32, f32); 4] {
        match self {
            Formant::Ah => [(650.0, 80.0, 0.0), (1080.0, 90.0, -6.0), (2650.0, 120.0, -7.0), (2900.0, 130.0, -8.0)],
            Formant::Eh => [(400.0, 70.0, 0.0), (1700.0, 80.0, -14.0), (2600.0, 100.0, -12.0), (3200.0, 120.0, -14.0)],
            Formant::Ee => [(290.0, 40.0, 0.0), (1870.0, 90.0, -15.0), (2800.0, 100.0, -18.0), (3250.0, 120.0, -20.0)],
            Formant::Oh => [(400.0, 40.0, 0.0), (800.0, 80.0, -10.0), (2600.0, 100.0, -12.0), (2800.0, 120.0, -12.0)],
            Formant::Oo => [(350.0, 40.0, 0.0), (600.0, 60.0, -20.0), (2700.0, 100.0, -17.0), (2900.0, 120.0, -14.0)],
            Formant::Uh => [(600.0, 70.0, 0.0), (1170.0, 90.0, -8.0), (2450.0, 110.0, -14.0), (2950.0, 120.0, -18.0)],
            Formant::Mm => [(250.0, 60.0, 0.0), (1000.0, 120.0, -24.0), (2300.0, 150.0, -32.0), (3000.0, 180.0, -36.0)],
            Formant::Violin => [(290.0, 140.0, 0.0), (520.0, 260.0, -3.0), (1600.0, 900.0, -5.0), (3100.0, 1400.0, -2.0)],
            Formant::Cello => [(180.0, 110.0, 0.0), (420.0, 260.0, -4.0), (1100.0, 700.0, -8.0), (2400.0, 1200.0, -10.0)],
            Formant::Horn => [(450.0, 300.0, 0.0), (1150.0, 500.0, -2.0), (2300.0, 900.0, -9.0), (3500.0, 1400.0, -18.0)],
        }
    }
}

/// Four parallel resonant band-passes (unity peak gain), for two channels.
#[derive(Clone, Copy, Debug, Default)]
pub struct FormantBank {
    coefs: [SvfCoef; 4],
    gains: [f32; 4],
    st: [[Svf; 4]; 2],
}

impl FormantBank {
    /// Sets the resonances by morphing `a` → `b` by `k` (0..1), frequencies times `shift`.
    pub fn set(&mut self, a: Formant, b: Formant, k: f32, shift: f32, sr: f32) {
        let (ta, tb) = (a.table(), b.table());
        for i in 0..4 {
            let f = (ta[i].0 + (tb[i].0 - ta[i].0) * k) * shift;
            let bw = (ta[i].1 + (tb[i].1 - ta[i].1) * k) * shift.sqrt();
            let g = ta[i].2 + (tb[i].2 - ta[i].2) * k;
            self.coefs[i] = SvfCoef::new(f, (f / bw.max(10.0)).clamp(0.3, 40.0), sr);
            self.gains[i] = db(g);
        }
    }
    #[inline]
    pub fn tick(&mut self, ch: usize, x: f32) -> f32 {
        let mut y = 0.0;
        for i in 0..4 {
            let c = &self.coefs[i];
            let (_, bp, _) = self.st[ch][i].tick(c, x);
            y += bp * c.k * self.gains[i];
        }
        y
    }
    pub fn reset(&mut self) {
        self.st = Default::default();
    }
}

// ---------------------------------------------------------------- reverb

#[derive(Clone, Debug)]
struct Comb {
    buf: Vec<f32>,
    i: usize,
    store: f32,
}

impl Comb {
    fn new(n: usize) -> Self {
        Self { buf: vec![0.0; n.max(1)], i: 0, store: 0.0 }
    }
    #[inline]
    fn tick(&mut self, x: f32, fb: f32, damp: f32) -> f32 {
        let out = self.buf[self.i];
        self.store = out * (1.0 - damp) + self.store * damp;
        self.buf[self.i] = x + self.store * fb;
        self.i += 1;
        if self.i >= self.buf.len() {
            self.i = 0;
        }
        out
    }
}

#[derive(Clone, Debug)]
struct Allpass {
    buf: Vec<f32>,
    i: usize,
}

impl Allpass {
    fn new(n: usize) -> Self {
        Self { buf: vec![0.0; n.max(1)], i: 0 }
    }
    #[inline]
    fn tick(&mut self, x: f32) -> f32 {
        let b = self.buf[self.i];
        self.buf[self.i] = x + b * 0.5;
        self.i += 1;
        if self.i >= self.buf.len() {
            self.i = 0;
        }
        b - x
    }
}

/// Freeverb (Jezar's tuning) with a short pre-delay: a warm deck-and-harbour space.
#[derive(Clone, Debug)]
pub struct Reverb {
    combs: [Vec<Comb>; 2],
    aps: [Vec<Allpass>; 2],
    pre: Vec<f32>,
    pre_i: usize,
    pub feedback: f32,
    pub damp: f32,
    pub wet: f32,
    hp: [f32; 2],
}

impl Reverb {
    pub fn new(sr: f32) -> Self {
        const COMBS: [usize; 8] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
        const APS: [usize; 4] = [556, 441, 341, 225];
        let k = sr / 44100.0;
        let mk = |spread: usize| -> (Vec<Comb>, Vec<Allpass>) {
            (
                COMBS.iter().map(|&n| Comb::new(((n + spread) as f32 * k) as usize)).collect(),
                APS.iter().map(|&n| Allpass::new(((n + spread) as f32 * k) as usize)).collect(),
            )
        };
        let (cl, al) = mk(0);
        let (cr, ar) = mk(23);
        Self {
            combs: [cl, cr],
            aps: [al, ar],
            pre: vec![0.0; (0.018 * sr) as usize + 1],
            pre_i: 0,
            feedback: 0.62 * 0.28 + 0.7,
            damp: 0.32,
            wet: 1.0,
            hp: [0.0; 2],
        }
    }
    /// Takes the send (stereo, summed to mono like Freeverb) and returns the wet signal.
    #[inline]
    pub fn tick(&mut self, l: f32, r: f32) -> (f32, f32) {
        let x = self.pre[self.pre_i];
        self.pre[self.pre_i] = (l + r) * 0.5;
        self.pre_i += 1;
        if self.pre_i >= self.pre.len() {
            self.pre_i = 0;
        }
        let input = x * 0.03;
        let mut out = [0.0f32; 2];
        for ch in 0..2 {
            let mut s = 0.0;
            for c in &mut self.combs[ch] {
                s += c.tick(input, self.feedback, self.damp);
            }
            for a in &mut self.aps[ch] {
                s = a.tick(s);
            }
            // Keep the rumble out of the tail.
            self.hp[ch] += 0.004 * (s - self.hp[ch]);
            out[ch] = (s - self.hp[ch]) * self.wet;
        }
        (out[0], out[1])
    }
}

// ---------------------------------------------------------------- echo

/// A ping-pong delay with a darkening, thinning feedback loop (slap-back across water).
#[derive(Clone, Debug)]
pub struct Echo {
    l: Vec<f32>,
    r: Vec<f32>,
    i: usize,
    dl: usize,
    dr: usize,
    pub feedback: f32,
    lp: [f32; 2],
    hp: [f32; 2],
    a_lp: f32,
    a_hp: f32,
}

impl Echo {
    pub fn new(sr: f32) -> Self {
        let n = (sr * 1.0) as usize;
        Self {
            l: vec![0.0; n],
            r: vec![0.0; n],
            i: 0,
            dl: (0.34 * sr) as usize,
            dr: (0.47 * sr) as usize,
            feedback: 0.34,
            lp: [0.0; 2],
            hp: [0.0; 2],
            a_lp: OnePole::coef(2600.0, sr),
            a_hp: OnePole::coef(180.0, sr),
        }
    }
    #[inline]
    pub fn tick(&mut self, xl: f32, xr: f32) -> (f32, f32) {
        let n = self.l.len();
        let yl = self.l[(self.i + n - self.dl) % n];
        let yr = self.r[(self.i + n - self.dr) % n];
        // Darken and thin the feedback.
        self.lp[0] += self.a_lp * (yr - self.lp[0]);
        self.lp[1] += self.a_lp * (yl - self.lp[1]);
        self.hp[0] += self.a_hp * (self.lp[0] - self.hp[0]);
        self.hp[1] += self.a_hp * (self.lp[1] - self.hp[1]);
        let fl = self.lp[0] - self.hp[0];
        let fr = self.lp[1] - self.hp[1];
        self.l[self.i] = xl + fl * self.feedback;
        self.r[self.i] = xr * 0.6 + fr * self.feedback;
        self.i = (self.i + 1) % n;
        (yl, yr)
    }
}

// ---------------------------------------------------------------- limiter

/// Peak limiter on the master: instant attack (never over the ceiling), smooth release,
/// after a DC blocker.
#[derive(Clone, Copy, Debug)]
pub struct Limiter {
    pub threshold: f32,
    gain: f32,
    rel: f32,
    dc_x: [f32; 2],
    dc_y: [f32; 2],
    pub reduction_peak: f32,
}

impl Limiter {
    pub fn new(sr: f32) -> Self {
        Self {
            threshold: db(-1.0),
            gain: 1.0,
            rel: 1.0 - (-1.0 / (0.15 * sr)).exp(),
            dc_x: [0.0; 2],
            dc_y: [0.0; 2],
            reduction_peak: 1.0,
        }
    }
    #[inline]
    pub fn tick(&mut self, l: f32, r: f32) -> (f32, f32) {
        let l = if l.is_finite() { l } else { 0.0 };
        let r = if r.is_finite() { r } else { 0.0 };
        let mut x = [l, r];
        for ch in 0..2 {
            let y = x[ch] - self.dc_x[ch] + 0.9995 * self.dc_y[ch];
            self.dc_x[ch] = x[ch];
            self.dc_y[ch] = y;
            x[ch] = y;
        }
        let peak = x[0].abs().max(x[1].abs());
        let target = (self.threshold / peak.max(1e-9)).min(1.0);
        if target < self.gain {
            self.gain = target;
        } else {
            self.gain += (target - self.gain) * self.rel;
        }
        self.reduction_peak = self.reduction_peak.min(self.gain);
        let c = 0.985;
        ((x[0] * self.gain).clamp(-c, c), (x[1] * self.gain).clamp(-c, c))
    }
}
