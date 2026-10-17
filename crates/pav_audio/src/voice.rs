//! A voice: one playing [`Layer`]. Control-rate work (pitch, LFOs, filter coefficients) runs
//! every [`CONTROL`] samples; oscillators, envelope and filters run per sample.

#![allow(clippy::needless_range_loop)]

use std::f32::consts::TAU;

use glam::Vec3;

use crate::bank::{Bus, Filter, Layer, Wave};
use crate::dsp::*;

pub const CONTROL: usize = 16;
const MAX_OSC: usize = 12;

/// What a voice belongs to, so its gain can follow a music stem, an ambience level or a handle.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Group {
    #[default]
    None,
    Music {
        track: u32,
        part: u16,
    },
    /// Ambience layer index × 16 + bed index.
    Amb(u16),
    /// A held cue started through a handle.
    Handle(u32),
}

/// A sung consonant before the vowel (choir and dialog blips).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Consonant {
    #[default]
    None,
    S,
    Sh,
    T,
    K,
    P,
    H,
    F,
}

impl Consonant {
    /// (duration s, filter, centre Hz, q, gain)
    fn params(self) -> Option<(f32, Filter, f32, f32, f32)> {
        Some(match self {
            Consonant::None => return None,
            Consonant::S => (0.075, Filter::Hp, 5200.0, 0.8, 0.36),
            Consonant::Sh => (0.08, Filter::Bp, 2600.0, 1.2, 0.45),
            Consonant::T => (0.018, Filter::Bp, 3800.0, 1.0, 0.7),
            Consonant::K => (0.024, Filter::Bp, 1900.0, 1.4, 0.7),
            Consonant::P => (0.012, Filter::Lp, 900.0, 0.7, 1.0),
            Consonant::H => (0.06, Filter::Bp, 1300.0, 0.6, 0.35),
            Consonant::F => (0.06, Filter::Hp, 6500.0, 0.7, 0.25),
        })
    }
}

/// Everything needed to start a voice.
#[derive(Clone, Copy, Debug)]
pub struct Spawn {
    pub layer: Layer,
    /// Multiplies every frequency of the layer (a note: its Hz with a `freq = 1` template).
    pub pitch: f32,
    pub gain: f32,
    pub pan: f32,
    /// Samples before it starts.
    pub delay: u32,
    /// Samples until release (sequenced notes); `None` = one-shot or held.
    pub gate: Option<u32>,
    /// Sustain until stopped.
    pub held: bool,
    pub pos: Option<Vec3>,
    pub range: f32,
    pub bus: Bus,
    pub group: Group,
    /// Chord: extra notes as frequency ratios to the first.
    pub chord: [f32; 4],
    pub nchord: u8,
    pub vowel: Option<(Formant, Formant)>,
    pub shift: f32,
    pub consonant: Consonant,
    pub reverb: f32,
    pub echo: f32,
    /// Seconds on a shared clock for LFO phases (bellows that breathe together); NaN = free.
    pub lfo_time: f32,
}

impl Spawn {
    pub fn new(layer: Layer) -> Self {
        Self {
            layer,
            pitch: 1.0,
            gain: 1.0,
            pan: 0.0,
            delay: 0,
            gate: None,
            held: false,
            pos: None,
            range: 14.0,
            bus: Bus::Sfx,
            group: Group::None,
            chord: [1.0; 4],
            nchord: 1,
            vowel: None,
            shift: 1.0,
            consonant: Consonant::None,
            reverb: 0.0,
            echo: 0.0,
            lfo_time: f32::NAN,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Voice {
    pub active: bool,
    /// Being cut (stolen); it fades out in a few ms.
    pub dying: bool,
    pub id: u32,
    pub l: Layer,
    pub bus: Bus,
    pub group: Group,
    pub pos: Option<Vec3>,
    pub range: f32,
    pub pan: f32,
    pub pan_mod: f32,
    pub reverb: f32,
    pub echo: f32,
    pub amp: f32,
    /// Group gain (set by the mixer each block) and the gains last applied (for ramps).
    pub ext: f32,
    pub ext_pitch: f32,
    pub ext_cut: f32,
    pub gl: f32,
    pub gr: f32,
    pub ramped: bool,
    pub started: bool,
    pub dist_a: f32,
    pub dist_gain: f32,
    dist_lp: [OnePole; 2],
    delay: u32,
    gate: Option<u32>,
    pub held: bool,
    env: Env,
    t: f32,
    base: f32,
    fe_ratio: f32,
    glide: f32,
    chord: [f32; 4],
    nchord: usize,
    nosc: usize,
    unison_norm: f32,
    det: [f32; 4],
    phase: [f32; MAX_OSC],
    inc: [f32; MAX_OSC],
    drift: [f32; MAX_OSC],
    phase2: [f32; 4],
    inc2: [f32; 4],
    fm_phase: f32,
    fm_inc: f32,
    fm_index: f32,
    noise: [Noise; 3],
    svf: [Svf; 2],
    coef: SvfCoef,
    q: f32,
    cut_ratio: f32,
    cut_time: f32,
    formants: FormantBank,
    vowel: (Formant, Formant),
    has_formant: bool,
    form_k: f32,
    shift: f32,
    form_time: f32,
    tilt: OnePole,
    tilt_a: f32,
    ks: Ks,
    lfos: [LfoState; 3],
    amp_prev: f32,
    amp_now: f32,
    dust_p: f32,
    drive_pre: f32,
    drive_norm: f32,
    cons_len: u32,
    cons_i: u32,
    cons_coef: SvfCoef,
    cons_mode: Filter,
    cons_svf: Svf,
    cons_gain: f32,
}

impl Voice {
    pub fn start(&mut self, s: &Spawn, id: u32, sr: f32, rng: &mut Rng) {
        let l = s.layer;
        self.active = true;
        self.dying = false;
        self.id = id;
        self.l = l;
        self.bus = s.bus;
        self.group = s.group;
        self.pos = s.pos;
        self.range = s.range.max(0.5);
        self.pan = s.pan;
        self.pan_mod = 0.0;
        self.reverb = s.reverb;
        self.echo = s.echo;
        self.amp = s.gain * l.gain;
        self.ext = 1.0;
        self.ext_pitch = 1.0;
        self.ext_cut = 1.0;
        self.gl = 0.0;
        self.gr = 0.0;
        self.ramped = false;
        self.started = false;
        self.dist_a = 1.0;
        self.dist_gain = 1.0;
        self.dist_lp = Default::default();
        self.delay = s.delay;
        self.gate = s.gate;
        self.held = s.held;
        let auto = s.gate.is_none() && !s.held;
        self.env = Env::new(l.attack, l.decay, l.sustain, l.hold, l.release, l.curve, auto, sr);
        self.t = 0.0;
        self.base = l.freq * s.pitch;
        self.fe_ratio = l.freq_end.map(|fe| fe / l.freq).unwrap_or(1.0);
        self.glide = if l.glide > 0.0 { l.glide } else { (l.attack + l.decay).max(0.01) };
        self.nchord = (s.nchord as usize).clamp(1, 4);
        self.chord = s.chord;
        self.chord[0] = 1.0;
        let unison = l.unison.clamp(1, 4) as usize;
        self.nosc = (self.nchord * unison).min(MAX_OSC);
        self.unison_norm = 1.0 / (unison as f32).sqrt();
        for u in 0..4 {
            let off = if unison > 1 { (u as f32 / (unison - 1) as f32) * 2.0 - 1.0 } else { 0.0 };
            self.det[u] = 2f32.powf(off * l.detune * 0.5 / 1200.0);
        }
        for k in 0..MAX_OSC {
            self.phase[k] = rng.unit();
            self.drift[k] = rng.bi() * l.drift;
        }
        for k in 0..4 {
            self.phase2[k] = rng.unit();
        }
        self.fm_phase = 0.0;
        self.noise = [Noise::new(rng.next_u32()), Noise::new(rng.next_u32()), Noise::new(rng.next_u32())];
        self.svf = Default::default();
        self.q = l.q_value();
        self.cut_ratio = l.cutoff_end.map(|c| c / l.cutoff.max(1.0)).unwrap_or(1.0);
        self.cut_time = if l.cutoff_time > 0.0 { l.cutoff_time } else { l.length().max(0.01) };
        self.vowel = match s.vowel {
            Some(v) => v,
            None => {
                let a = l.formant.unwrap_or(Formant::Ah);
                (a, l.formant_end.unwrap_or(a))
            }
        };
        self.has_formant = l.formant.is_some() || s.vowel.is_some() || l.wave == Wave::Voice;
        self.shift = l.formant_shift * s.shift;
        self.form_time = if l.formant_time > 0.0 { l.formant_time } else { (l.attack + l.decay + l.hold).max(0.05) };
        self.form_k = -1.0;
        self.formants.reset();
        self.tilt = OnePole::default();
        if l.wave == Wave::Pluck {
            let (ks, noise) = (&mut self.ks, &mut self.noise[0]);
            ks.start(self.base, l.pluck_decay, l.pluck_bright, sr, noise);
        }
        for k in 0..3 {
            let mut st = LfoState::new(&l.lfo[k], rng);
            if s.lfo_time.is_finite() && l.lfo[k].phase.is_none() {
                st.phase = (s.lfo_time * l.lfo[k].rate).fract();
            }
            self.lfos[k] = st;
        }
        self.amp_prev = 1.0;
        self.amp_now = 1.0;
        self.dust_p = (l.density / sr).clamp(0.0, 1.0);
        self.drive_pre = 1.0 + 6.0 * l.drive.max(0.0);
        self.drive_norm = 0.5 / soft(0.5 * self.drive_pre);
        self.cons_i = 0;
        self.cons_len = 0;
        if let Some((d, mode, f, q, g)) = s.consonant.params() {
            self.cons_len = (d * sr) as u32;
            self.cons_coef = SvfCoef::new(f * self.shift.sqrt(), q, sr);
            self.cons_mode = mode;
            self.cons_svf = Svf::default();
            self.cons_gain = g;
        }
        self.control(0, sr);
        self.amp_prev = self.amp_now;
    }

    /// Loudness estimate for voice stealing.
    pub fn loudness(&self) -> f32 {
        if self.delay > 0 {
            return self.amp * self.ext;
        }
        self.env.level * self.amp * self.ext
    }
    pub fn released(&self) -> bool {
        self.env.released() || self.dying
    }
    pub fn release(&mut self) {
        self.held = false;
        self.gate = None;
        self.env.release();
        if self.l.wave == Wave::Pluck {
            self.ks.mute(self.base);
        }
        if self.delay > 0 {
            self.active = false;
        }
    }
    pub fn kill(&mut self, sr: f32) {
        self.dying = true;
        self.env.kill(sr);
        if self.delay > 0 {
            self.active = false;
        }
    }
    pub fn waiting(&self) -> bool {
        self.delay > 0
    }

    fn control(&mut self, m: usize, sr: f32) {
        let dtb = m as f32 / sr;
        let t = self.t;
        let (mut lp, mut la, mut lc, mut lpan, mut lfm) = (0.0f32, 1.0f32, 0.0f32, 0.0f32, 1.0f32);
        for k in 0..3 {
            let lfo = self.l.lfo[k];
            if lfo.to == LfoTo::None || lfo.depth == 0.0 {
                continue;
            }
            let v = self.lfos[k].step(&lfo, dtb, t, &mut self.noise[2]);
            match lfo.to {
                LfoTo::Pitch => lp += v * lfo.depth,
                LfoTo::Amp => la *= 1.0 - lfo.depth.clamp(0.0, 1.0) * (0.5 - 0.5 * v),
                LfoTo::Cutoff => lc += v * lfo.depth,
                LfoTo::Pan => lpan += v * lfo.depth,
                LfoTo::Fm => lfm *= (1.0 + v * lfo.depth).max(0.0),
                LfoTo::None => {}
            }
        }
        self.amp_prev = self.amp_now;
        self.amp_now = la;
        self.pan_mod = lpan;

        let mut f = self.base;
        if self.fe_ratio != 1.0 {
            f *= self.fe_ratio.powf((t / self.glide).min(1.0));
        }
        let semis = self.l.pitch_curve.at(t) + lp;
        if semis != 0.0 {
            f *= 2f32.powf(semis / 12.0);
        }
        f *= self.ext_pitch;
        let inc = (f / sr).min(0.45);
        let unison = (self.l.unison.clamp(1, 4) as usize).min(MAX_OSC / self.nchord);
        if self.l.drift > 0.0 && m > 0 {
            let a = 1.0 - (-dtb / 0.15).exp();
            let s = self.l.drift * 2.2;
            for k in 0..self.nosc {
                let w = self.noise[2].white();
                self.drift[k] += a * (w * s - self.drift[k]);
            }
        }
        for c in 0..self.nchord {
            for u in 0..unison {
                let k = c * unison + u;
                let d = 1.0 + self.drift[k] * 0.000_577_8;
                self.inc[k] = (inc * self.chord[c] * self.det[u] * d).min(0.45);
            }
        }
        if self.l.wave2.is_some() {
            let d2 = 2f32.powf(self.l.detune2 / 1200.0);
            for c in 0..self.nchord {
                self.inc2[c] = (inc * self.chord[c] * self.l.ratio2 * d2).min(0.45);
            }
        }
        if self.l.fm_index > 0.0 || self.l.fm_index_end.is_some() {
            self.fm_inc = (inc * self.l.fm_ratio).min(0.45);
            let i0 = self.l.fm_index;
            let i1 = self.l.fm_index_end.unwrap_or(i0);
            let ft = if self.l.fm_time > 0.0 { self.l.fm_time } else { (self.l.attack + self.l.decay).max(0.01) };
            let k = (t / ft).min(1.0);
            self.fm_index = (i0 + (i1 - i0) * k) * lfm;
        }
        if self.l.filter != Filter::None {
            let mut c = self.l.cutoff;
            if self.cut_ratio != 1.0 {
                c *= self.cut_ratio.powf((t / self.cut_time).min(1.0));
            }
            let oct = lc + self.l.env_cutoff * self.env.level;
            if oct != 0.0 {
                c *= 2f32.powf(oct);
            }
            if self.l.key_track != 0.0 {
                c *= (f / 261.63).max(0.01).powf(self.l.key_track);
            }
            c *= self.ext_cut;
            self.coef = SvfCoef::new(c, self.q, sr);
        }
        if self.has_formant {
            let k = (t / self.form_time).clamp(0.0, 1.0);
            let k = k * k * (3.0 - 2.0 * k);
            if self.form_k < 0.0 || (self.vowel.0 != self.vowel.1 && (k - self.form_k).abs() > 0.004) {
                self.form_k = k;
                self.formants.set(self.vowel.0, self.vowel.1, k, self.shift, sr);
            }
        }
        if self.l.wave == Wave::Voice {
            self.tilt_a = OnePole::coef((f * 2.5).max(350.0), sr);
        }
    }

    #[inline]
    fn osc(&mut self) -> f32 {
        let n = self.nosc;
        let mut s = 0.0;
        match self.l.wave {
            Wave::Sine | Wave::Triangle => {
                let m = if self.fm_index > 0.0 {
                    self.fm_phase += self.fm_inc;
                    if self.fm_phase >= 1.0 {
                        self.fm_phase -= 1.0;
                    }
                    self.fm_index * sine(self.fm_phase) / TAU
                } else {
                    0.0
                };
                let triangle = self.l.wave == Wave::Triangle;
                for k in 0..n {
                    let mut p = self.phase[k] + self.inc[k];
                    if p >= 1.0 {
                        p -= 1.0;
                    }
                    self.phase[k] = p;
                    let q = p + m;
                    let q = q - q.floor();
                    s += if triangle { tri(q) } else { sine(q) };
                }
            }
            Wave::Saw | Wave::Voice => {
                for k in 0..n {
                    let mut p = self.phase[k] + self.inc[k];
                    if p >= 1.0 {
                        p -= 1.0;
                    }
                    self.phase[k] = p;
                    s += saw(p, self.inc[k]);
                }
            }
            Wave::Square => {
                let duty = self.l.duty.clamp(0.05, 0.95);
                for k in 0..n {
                    let mut p = self.phase[k] + self.inc[k];
                    if p >= 1.0 {
                        p -= 1.0;
                    }
                    self.phase[k] = p;
                    s += pulse(p, self.inc[k], duty);
                }
            }
            _ => {}
        }
        s *= self.unison_norm;
        if let Some(w2) = self.l.wave2 {
            let duty = self.l.duty.clamp(0.05, 0.95);
            let mut s2 = 0.0;
            for c in 0..self.nchord {
                let mut p = self.phase2[c] + self.inc2[c];
                if p >= 1.0 {
                    p -= 1.0;
                }
                self.phase2[c] = p;
                let dt = self.inc2[c];
                s2 += match w2 {
                    Wave::Sine => sine(p),
                    Wave::Triangle => tri(p),
                    Wave::Saw | Wave::Voice => saw(p, dt),
                    Wave::Square => pulse(p, dt, duty),
                    _ => self.noise[2].white(),
                };
            }
            s += s2 * self.l.gain2;
        }
        s
    }

    #[inline]
    fn filt(mode: Filter, svf: &mut Svf, c: &SvfCoef, x: f32) -> f32 {
        let (lp, bp, hp) = svf.tick(c, x);
        match mode {
            Filter::Lp => lp,
            Filter::Bp => bp * c.k,
            Filter::Hp => hp,
            Filter::Notch => lp + hp,
            Filter::None => x,
        }
    }

    /// Renders into `ol` (and `or` for stereo layers), adding nothing else. Returns false when
    /// the voice has finished.
    pub fn render(&mut self, ol: &mut [f32], or: &mut [f32], sr: f32) -> bool {
        let n = ol.len();
        let mut i = 0;
        if self.delay > 0 {
            let d = (self.delay as usize).min(n);
            self.delay -= d as u32;
            i = d;
            if i >= n {
                return true;
            }
        }
        self.started = true;
        let dt = 1.0 / sr;
        let wave = self.l.wave;
        let stereo = self.l.stereo;
        let noise_mix = self.l.noise;
        let fmode = self.l.filter;
        let drive = self.l.drive > 0.0;
        let fdry = self.l.formant_dry;
        let has_f = self.has_formant;
        let dist = self.dist_a < 0.999;
        while i < n {
            let m = CONTROL.min(n - i);
            self.control(m, sr);
            let (a0, a1) = (self.amp_prev, self.amp_now);
            let da = (a1 - a0) / m as f32;
            for j in 0..m {
                if let Some(g) = self.gate {
                    if g == 0 {
                        self.gate = None;
                        self.env.release();
                        if wave == Wave::Pluck {
                            self.ks.mute(self.base);
                        }
                    } else {
                        self.gate = Some(g - 1);
                    }
                }
                let e = self.env.next(dt);
                let amp = (a0 + da * j as f32) * e * self.amp;
                let (mut x, mut y) = match wave {
                    Wave::Noise => (self.noise[0].white(), if stereo { self.noise[1].white() } else { 0.0 }),
                    Wave::Pink => (self.noise[0].pink(), if stereo { self.noise[1].pink() } else { 0.0 }),
                    Wave::Brown => (self.noise[0].brown(), if stereo { self.noise[1].brown() } else { 0.0 }),
                    Wave::Dust => {
                        let p = self.dust_p;
                        (self.noise[0].dust(p), if stereo { self.noise[1].dust(p) } else { 0.0 })
                    }
                    Wave::Pluck => (self.ks.tick(), 0.0),
                    _ => (self.osc(), 0.0),
                };
                if wave == Wave::Voice {
                    x = self.tilt.tick(self.tilt_a, x) * 2.5;
                }
                if noise_mix > 0.0 {
                    x += self.noise[2].white() * noise_mix;
                }
                if !stereo {
                    y = x;
                }
                if has_f {
                    let fx = self.formants.tick(0, x);
                    x = fx + x * fdry;
                    if stereo {
                        let fy = self.formants.tick(1, y);
                        y = fy + y * fdry;
                    }
                }
                if fmode != Filter::None {
                    let c = self.coef;
                    x = Self::filt(fmode, &mut self.svf[0], &c, x);
                    if stereo {
                        y = Self::filt(fmode, &mut self.svf[1], &c, y);
                    }
                }
                if drive {
                    x = soft(x * self.drive_pre) * self.drive_norm;
                    if stereo {
                        y = soft(y * self.drive_pre) * self.drive_norm;
                    }
                }
                let mut c = 0.0;
                if self.cons_i < self.cons_len {
                    let k = self.cons_i as f32 / self.cons_len as f32;
                    let shape = (k * 12.0).min(1.0) * (1.0 - k) * (1.0 - k);
                    let w = self.noise[2].white();
                    let cc = self.cons_coef;
                    c = Self::filt(self.cons_mode, &mut self.cons_svf, &cc, w) * shape * self.cons_gain * self.amp;
                    self.cons_i += 1;
                }
                let mut ox = x * amp + c;
                let mut oy = if stereo { y * amp + c } else { 0.0 };
                if dist {
                    ox = self.dist_lp[0].tick(self.dist_a, ox);
                    if stereo {
                        oy = self.dist_lp[1].tick(self.dist_a, oy);
                    }
                }
                ol[i + j] = ox;
                if stereo {
                    or[i + j] = oy;
                }
            }
            self.t += m as f32 * dt;
            i += m;
            if self.env.done() && self.cons_i >= self.cons_len {
                return false;
            }
        }
        if !self.held && self.gate.is_none() && self.t > 120.0 {
            return false;
        }
        true
    }
}
