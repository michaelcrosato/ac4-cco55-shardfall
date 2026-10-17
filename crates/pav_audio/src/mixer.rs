//! The mixer: voice pools (sfx 48, music 24, ambience 8, voice 8, ui 8, so music is never
//! stolen by effects), buses with volumes, reverb and echo sends, a master limiter, the music
//! sequencer and ambience. The same code runs in the audio callback and offline.

#![allow(clippy::needless_range_loop)]

use std::collections::{BTreeMap, BTreeSet};
use std::f32::consts::FRAC_PI_4;
use std::sync::Arc;

use glam::Vec3;
use pav_core::SimEvent;
use pav_core::play::AudioState;

use crate::ambience::{AmbAction, AmbienceBank, AmbiencePlayer};
use crate::bank::{Bank, Bus, Cue, Filter, Layer, Speaker, Wave};
use crate::dsp::{Echo, Formant, Limiter, OnePole, Reverb, Rng, db, hash_str};
use crate::music::{MusicBank, MusicPlayer, NoteOn};
use crate::voice::{Consonant, Group, Spawn, Voice};
use crate::{Listener, Patch, sounds_for};

/// Frames per internal block (control and scheduling granularity; notes are still placed on
/// exact samples inside a block).
pub const BLOCK: usize = 128;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pool {
    Sfx = 0,
    Music = 1,
    Ambience = 2,
    Voice = 3,
    Ui = 4,
}

pub const POOL_SIZES: [usize; 5] = [48, 24, 8, 8, 8];
const SPARE: usize = 4;

fn pool_for(bus: Bus, held: bool) -> Pool {
    match bus {
        Bus::Music => Pool::Music,
        Bus::Voice => Pool::Voice,
        Bus::Ui => Pool::Ui,
        Bus::Ambience if held => Pool::Ambience,
        _ => Pool::Sfx,
    }
}

fn default_sends(bus: Bus) -> (f32, f32) {
    match bus {
        Bus::Music => (0.25, 0.0),
        Bus::Sfx => (0.12, 0.0),
        Bus::Ambience => (0.08, 0.0),
        Bus::Voice => (0.07, 0.0),
        Bus::Ui | Bus::Master => (0.0, 0.0),
    }
}

/// Loudness per bus since the last reset (master is after the limiter).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Stats {
    pub rms: [f32; 6],
    pub peak: [f32; 6],
    pub seconds: f32,
    /// Lowest limiter gain seen (1 = never limited).
    pub limiter: f32,
    /// Voices playing now per pool (sfx, music, ambience, voice, ui).
    pub voices: [usize; 5],
}

impl Stats {
    pub fn rms(&self, b: Bus) -> f32 {
        self.rms[b.index()]
    }
    pub fn peak(&self, b: Bus) -> f32 {
        self.peak[b.index()]
    }
    pub fn rms_db(&self, b: Bus) -> f32 {
        20.0 * self.rms(b).max(1e-9).log10()
    }
    pub fn peak_db(&self, b: Bus) -> f32 {
        20.0 * self.peak(b).max(1e-9).log10()
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct StatAcc {
    sq: [f64; 6],
    peak: [f32; 6],
    frames: u64,
    limiter: f32,
}

/// A message to the mixer (what [`crate::AudioOut`] sends to the audio callback).
#[derive(Clone, Debug)]
pub enum Cmd {
    Legacy(Patch, f32, f32, f32),
    Bank(Arc<Bank>),
    Music(Arc<MusicBank>),
    Ambience(Arc<AmbienceBank>),
    Apply(Box<AudioState>, Listener),
    Cue { name: String, pos: Vec3, gain: f32, pitch: f32, listener: Listener },
    Event(Box<SimEvent>, Listener, f32),
    Blip(String),
    Ui(String),
    Bus(Bus, f32),
    StartLoop { handle: u32, cue: String, pos: Option<Vec3>, gain: f32 },
    StopLoop(u32),
    LoopGain(u32, f32),
    LoopPos(u32, Vec3),
    Stinger(String),
    Skip(i32),
    Listener(Listener),
}

pub struct Mixer {
    pub sample_rate: f32,
    /// Legacy master gain (multiplies the master bus).
    pub master: f32,
    pools: Vec<Vec<Voice>>,
    bus_vol: [f32; 6],
    bus_cur: [f32; 6],
    pub bank: Arc<Bank>,
    pub music: MusicPlayer,
    pub amb: AmbiencePlayer,
    listener: Listener,
    rng: Rng,
    next_id: u32,
    reverb: Reverb,
    echo: Echo,
    limiter: Limiter,
    bufs: Vec<Vec<f32>>,
    stats: StatAcc,
    time: u64,
    warned: BTreeSet<String>,
    handles: BTreeMap<u32, f32>,
    cue_last: BTreeMap<String, u64>,
    notes: Vec<NoteOn>,
    amb_actions: Vec<AmbAction>,
    amb_alive: Vec<u8>,
    duck: f32,
    voice_energy: f32,
}

// Scratch buffer indices: 5 buses × 2, reverb ×2, echo ×2, voice ×2.
const B_REV: usize = 10;
const B_VOICE: usize = 14;

impl Mixer {
    pub fn new(sample_rate: f32) -> Self {
        let pools = POOL_SIZES.iter().map(|&n| (0..n + SPARE).map(|_| Voice::default()).collect()).collect();
        Self {
            sample_rate,
            master: 1.0,
            pools,
            bus_vol: [0.8, 0.85, 1.0, 0.9, 1.0, 0.8],
            bus_cur: [0.8, 0.85, 1.0, 0.9, 1.0, 0.8],
            bank: Arc::new(Bank::default()),
            music: MusicPlayer::default(),
            amb: AmbiencePlayer::default(),
            listener: Listener::default(),
            rng: Rng::new(0x9e37_79b9),
            next_id: 1,
            reverb: Reverb::new(sample_rate),
            echo: Echo::new(sample_rate),
            limiter: Limiter::new(sample_rate),
            bufs: (0..16).map(|_| vec![0.0; BLOCK]).collect(),
            stats: StatAcc { limiter: 1.0, ..Default::default() },
            time: 0,
            warned: BTreeSet::new(),
            handles: BTreeMap::new(),
            cue_last: BTreeMap::new(),
            notes: Vec::new(),
            amb_actions: Vec::new(),
            amb_alive: Vec::new(),
            duck: 1.0,
            voice_energy: 0.0,
        }
    }

    // ------------------------------------------------------------ loading

    pub fn load_bank(&mut self, bank: Arc<Bank>) {
        self.bank = bank;
    }
    pub fn load_bank_text(&mut self, text: &str) -> anyhow::Result<()> {
        self.bank = Arc::new(Bank::parse(text)?);
        Ok(())
    }
    pub fn load_music(&mut self, music: Arc<MusicBank>) {
        self.music.load(music);
    }
    pub fn load_music_text(&mut self, music_toml: &str, songs: &[(String, String)]) -> anyhow::Result<()> {
        self.music.load(Arc::new(MusicBank::parse(music_toml, songs)?));
        Ok(())
    }
    pub fn load_ambience(&mut self, amb: Arc<AmbienceBank>) {
        self.amb.load(amb);
    }
    pub fn load_ambience_text(&mut self, text: &str) -> anyhow::Result<()> {
        self.amb.load(Arc::new(AmbienceBank::parse(text)?));
        Ok(())
    }

    // ------------------------------------------------------------ control

    pub fn handle(&mut self, c: Cmd) {
        match c {
            Cmd::Legacy(p, pan, gain, delay) => self.play(p, pan, gain, delay),
            Cmd::Bank(b) => self.load_bank(b),
            Cmd::Music(m) => self.load_music(m),
            Cmd::Ambience(a) => self.load_ambience(a),
            Cmd::Apply(s, l) => self.apply(&s, &l),
            Cmd::Cue { name, pos, gain, pitch, listener } => {
                self.play_cue(&name, pos, gain, pitch, &listener);
            }
            Cmd::Event(e, l, sfx) => self.play_event(&e, &l, sfx),
            Cmd::Blip(s) => self.voice_blip(&s),
            Cmd::Ui(s) => self.ui(&s),
            Cmd::Bus(b, v) => self.set_bus(b, v),
            Cmd::StartLoop { handle, cue, pos, gain } => self.start_loop(handle, &cue, pos, gain),
            Cmd::StopLoop(h) => self.stop_loop(h),
            Cmd::LoopGain(h, g) => self.set_loop_gain(h, g),
            Cmd::LoopPos(h, p) => self.set_loop_pos(h, p),
            Cmd::Stinger(s) => {
                self.stinger(&s);
            }
            Cmd::Skip(d) => self.music_skip(d),
            Cmd::Listener(l) => self.set_listener(&l),
        }
    }

    pub fn set_listener(&mut self, l: &Listener) {
        let r = l.right.normalize_or_zero();
        self.listener = Listener { pos: l.pos, right: if r == Vec3::ZERO { Vec3::X } else { r } };
    }

    /// Music state + intensity, ambience levels and the listener, from the game's frame.
    pub fn apply(&mut self, state: &AudioState, listener: &Listener) {
        let l = match state.listener {
            Some((pos, facing)) => {
                let y = facing.to_radians();
                Listener { pos, right: Vec3::new(y.cos(), 0.0, y.sin()) }
            }
            None => *listener,
        };
        self.set_listener(&l);
        let sr = self.sample_rate;
        match &state.music {
            Some(m) => self.music.set(&m.song, m.intensity, sr),
            None => self.music.stop(sr),
        }
        self.amb.set_levels(&state.ambience);
    }

    pub fn set_bus(&mut self, bus: Bus, vol: f32) {
        self.bus_vol[bus.index()] = vol.clamp(0.0, 2.0);
    }
    pub fn bus(&self, bus: Bus) -> f32 {
        self.bus_vol[bus.index()]
    }

    pub fn stinger(&mut self, name: &str) -> bool {
        let sr = self.sample_rate;
        self.music.stinger(name, sr)
    }
    pub fn music_skip(&mut self, delta: i32) {
        let sr = self.sample_rate;
        self.music.skip(delta, sr);
    }
    /// (song name, title) of what is playing.
    pub fn now_playing(&self) -> Option<(String, String)> {
        self.music.now_playing().map(|s| (s.name.clone(), s.title.clone()))
    }

    fn warn_once(&mut self, what: &str, name: &str) {
        let key = format!("{what}:{name}");
        if self.warned.insert(key) {
            log::warn!("audio: unknown {what} {name:?} (ignored)");
        }
    }

    // ------------------------------------------------------------ spawning

    /// Starts a voice in `pool`; steals the quietest when full. Returns its id.
    pub fn spawn(&mut self, s: &Spawn, pool: Pool) -> Option<u32> {
        let sr = self.sample_rate;
        let p = pool as usize;
        let cap = POOL_SIZES[p];
        let voices = &mut self.pools[p];
        let live = voices.iter().filter(|v| v.active && !v.dying).count();
        if live >= cap {
            let mut best: Option<(usize, (u8, f32))> = None;
            for (i, v) in voices.iter().enumerate() {
                if !v.active || v.dying {
                    continue;
                }
                let key = (if v.released() { 0 } else { 1 }, v.loudness());
                if best.is_none_or(|(_, b)| key.0 < b.0 || (key.0 == b.0 && key.1 < b.1)) {
                    best = Some((i, key));
                }
            }
            let (vi, (rel, loud)) = best?;
            if pool == Pool::Ambience && rel == 1 && loud > s.gain * s.layer.gain * 0.5 {
                return None;
            }
            voices[vi].kill(sr);
        }
        let slot = match voices.iter().position(|v| !v.active) {
            Some(i) => i,
            None => {
                // Every spare slot is fading out: reuse the quietest.
                let mut q = 0;
                let mut ql = f32::MAX;
                for (i, v) in voices.iter().enumerate() {
                    if v.dying && v.loudness() < ql {
                        ql = v.loudness();
                        q = i;
                    }
                }
                q
            }
        };
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1).max(1);
        voices[slot].start(s, id, sr, &mut self.rng);
        Some(id)
    }

    /// Spawns the layers of a template (repeats, delays and per-layer randomisation).
    fn spawn_layers(&mut self, layers: &[Layer], base: &Spawn, pool_override: Option<Pool>, variant: u8) {
        let sr = self.sample_rate;
        for l in layers {
            if l.variant != 0 && l.variant != variant {
                continue;
            }
            for r in 0..l.repeat.max(1) {
                let mut s = *base;
                s.layer = *l;
                let semis = self.rng.bi() * l.pitch_rand;
                if semis != 0.0 {
                    s.pitch *= 2f32.powf(semis / 12.0);
                }
                if l.gain_rand != 0.0 {
                    s.gain *= db(self.rng.bi() * l.gain_rand);
                }
                let d = l.delay + r as f32 * l.repeat_gap + self.rng.unit() * l.delay_rand;
                s.delay = base.delay + (d.max(0.0) * sr) as u32;
                s.pan = base.pan + l.pan + self.rng.bi() * l.spread;
                if let Some(b) = l.bus {
                    s.bus = b;
                }
                if let Some(rv) = l.reverb {
                    s.reverb = rv;
                }
                if let Some(e) = l.echo {
                    s.echo = e;
                }
                let pool = pool_override.unwrap_or_else(|| pool_for(s.bus, s.held));
                self.spawn(&s, pool);
            }
        }
    }

    fn cue_spawn(&mut self, cue: &Cue, pos: Option<Vec3>, gain: f32, pitch: f32, bus: Option<Bus>, group: Group, held: bool) {
        let variant = if cue.variants > 0 { 1 + self.rng.below(cue.variants as usize) as u8 } else { 0 };
        let semis = self.rng.bi() * cue.pitch_rand;
        let g = gain * cue.gain * db(self.rng.bi() * cue.gain_rand);
        let bus = bus.unwrap_or(cue.bus);
        let (rv, ec) = default_sends(bus);
        let mut s = Spawn::new(Layer::default());
        s.pitch = pitch.max(0.01) * 2f32.powf(semis / 12.0);
        s.gain = g;
        s.held = held;
        s.pos = if cue.is_positional() { pos } else { None };
        s.range = cue.range;
        s.bus = bus;
        s.group = group;
        s.reverb = cue.reverb.unwrap_or(rv);
        s.echo = cue.echo.unwrap_or(ec);
        let layers = cue.layer.clone();
        self.spawn_layers(&layers, &s, None, variant);
    }

    /// Plays a cue from the sound bank at `pos` (unknown names: logged once, ignored; a music
    /// stinger of that name plays if there is one).
    pub fn play_cue(&mut self, name: &str, pos: Vec3, gain: f32, pitch: f32, listener: &Listener) -> bool {
        self.set_listener(listener);
        let bank = self.bank.clone();
        let Some(cue) = bank.cue(name) else {
            if self.stinger(name) {
                return true;
            }
            self.warn_once("cue", name);
            return false;
        };
        if cue.cooldown > 0.0 {
            let now = self.time;
            let min = (cue.cooldown * self.sample_rate) as u64;
            if let Some(&t) = self.cue_last.get(name) {
                if now < t + min {
                    return true;
                }
            }
            self.cue_last.insert(name.to_string(), now);
        }
        self.cue_spawn(cue, Some(pos), gain, pitch, None, Group::None, false);
        if let Some(st) = &cue.stinger {
            if !self.stinger(st) {
                self.warn_once("stinger", st);
            }
        }
        true
    }

    /// Plays a simulation event: `Cue` through the bank; others through the built-in patches.
    pub fn play_event(&mut self, ev: &SimEvent, l: &Listener, sfx: f32) {
        if let SimEvent::Cue { cue, pos, gain, pitch } = ev {
            self.play_cue(cue, *pos, gain * sfx, *pitch, l);
            return;
        }
        let (pan, gain) = match crate::event_pos(ev) {
            Some(p) => {
                let d = p - l.pos;
                ((d.dot(l.right) / 12.0).clamp(-0.85, 0.85), 1.0 / (1.0 + d.length() / 14.0))
            }
            None => (0.0, 1.0),
        };
        for (patch, delay) in sounds_for(ev) {
            self.play(patch, pan, gain * sfx, delay);
        }
    }

    /// The original API: one patch, pan -1..1, delay in seconds.
    pub fn play(&mut self, p: Patch, pan: f32, gain: f32, delay: f32) {
        let mut s = Spawn::new(Layer::from(p));
        s.pan = pan.clamp(-1.0, 1.0);
        s.gain = gain;
        s.delay = (delay.max(0.0) * self.sample_rate) as u32;
        s.reverb = 0.08;
        self.spawn(&s, Pool::Sfx);
    }

    /// A UI sound (ui_move, ui_confirm, ...): the bank's cue, or a soft tick.
    pub fn ui(&mut self, name: &str) {
        let bank = self.bank.clone();
        match bank.cue(name) {
            Some(c) => self.cue_spawn(c, None, 1.0, 1.0, Some(Bus::Ui), Group::None, false),
            None => {
                self.warn_once("ui cue", name);
                let mut s = Spawn::new(Layer { wave: Wave::Triangle, freq: 1400.0, decay: 0.04, gain: 0.12, ..Layer::default() });
                s.bus = Bus::Ui;
                self.spawn(&s, Pool::Ui);
            }
        }
    }

    /// One syllable of a speaker's dialog voice.
    pub fn voice_blip(&mut self, speaker: &str) {
        let bank = self.bank.clone();
        let sp = match bank.speakers.get(speaker) {
            Some(s) => s.clone(),
            None => {
                self.warn_once("speaker", speaker);
                let h = hash_str(speaker);
                Speaker { pitch: 100.0 + (h % 90) as f32, ..Speaker::default() }
            }
        };
        if let Some(c) = &sp.cue {
            if let Some(cue) = bank.cue(c) {
                self.cue_spawn(cue, None, sp.gain, 1.0, Some(Bus::Voice), Group::None, false);
            }
            return;
        }
        let r = &mut self.rng;
        let vowels = if sp.vowels.is_empty() { vec![Formant::Ah] } else { sp.vowels.clone() };
        let a = vowels[r.below(vowels.len())];
        let b = if r.unit() < 0.35 { vowels[r.below(vowels.len())] } else { a };
        let len = 0.085 * sp.speed.max(0.2) * r.range(0.75, 1.25);
        let semis = r.bi() * sp.range;
        let rise = r.bi() * 1.2;
        let cons =
            [Consonant::None, Consonant::None, Consonant::T, Consonant::K, Consonant::P, Consonant::S, Consonant::H][r.below(7)];
        let rough = sp.rough.clamp(0.0, 1.0);
        let mut pc = crate::bank::PitchCurve { n: 3, ..Default::default() };
        pc.pts[0] = [0.0, semis + rise];
        pc.pts[1] = [len * 0.5, semis];
        pc.pts[2] = [len + 0.05, semis - 1.0 - rough];
        let layer = Layer {
            wave: Wave::Voice,
            freq: sp.pitch.max(40.0),
            pitch_curve: pc,
            unison: sp.unison.max(1),
            detune: if sp.unison > 1 { 30.0 } else { 0.0 },
            drift: 4.0 + rough * 20.0,
            noise: sp.breath + rough * 0.08,
            attack: 0.012,
            decay: 0.06,
            sustain: 0.85,
            hold: len,
            release: 0.05,
            formant: Some(a),
            formant_end: Some(b),
            formant_time: len,
            formant_shift: sp.shift,
            filter: Filter::Lp,
            cutoff: 5000.0,
            drive: rough * 0.4,
            lfo: [
                crate::dsp::Lfo {
                    to: crate::dsp::LfoTo::Pitch,
                    rate: 16.0,
                    depth: 0.1 + rough * 0.6,
                    shape: crate::dsp::LfoShape::Random,
                    delay: 0.0,
                    phase: None,
                },
                crate::dsp::Lfo::default(),
                crate::dsp::Lfo::default(),
            ],
            gain: 0.5 * sp.gain,
            ..Layer::default()
        };
        let mut s = Spawn::new(layer);
        s.bus = Bus::Voice;
        s.consonant = cons;
        s.reverb = 0.06;
        self.spawn(&s, Pool::Voice);
    }

    // ------------------------------------------------------------ held voices

    /// Starts a held (looping) cue under a handle chosen by the caller.
    pub fn start_loop(&mut self, handle: u32, cue: &str, pos: Option<Vec3>, gain: f32) {
        let bank = self.bank.clone();
        let Some(c) = bank.cue(cue) else {
            self.warn_once("cue", cue);
            return;
        };
        self.stop_loop(handle);
        self.handles.insert(handle, gain);
        self.cue_spawn(c, pos, 1.0, 1.0, None, Group::Handle(handle), true);
    }
    pub fn stop_loop(&mut self, handle: u32) {
        self.handles.remove(&handle);
        self.release_where(|g| g == Group::Handle(handle));
    }
    pub fn set_loop_gain(&mut self, handle: u32, gain: f32) {
        if let Some(g) = self.handles.get_mut(&handle) {
            *g = gain.max(0.0);
        }
    }
    pub fn set_loop_pos(&mut self, handle: u32, pos: Vec3) {
        for p in &mut self.pools {
            for v in p.iter_mut().filter(|v| v.active && v.group == Group::Handle(handle)) {
                v.pos = Some(pos);
            }
        }
    }
    fn release_where(&mut self, f: impl Fn(Group) -> bool) {
        for p in &mut self.pools {
            for v in p.iter_mut().filter(|v| v.active && f(v.group)) {
                v.release();
            }
        }
    }

    // ------------------------------------------------------------ rendering

    /// Voices playing (all pools).
    pub fn active(&self) -> usize {
        self.pools.iter().map(|p| p.iter().filter(|v| v.active).count()).sum()
    }

    pub fn stats(&self) -> Stats {
        let n = self.stats.frames.max(1) as f64;
        let mut s =
            Stats { seconds: self.stats.frames as f32 / self.sample_rate, limiter: self.stats.limiter, ..Default::default() };
        for i in 0..6 {
            s.rms[i] = (self.stats.sq[i] / n).sqrt() as f32;
            s.peak[i] = self.stats.peak[i];
        }
        for (i, p) in self.pools.iter().enumerate() {
            s.voices[i] = p.iter().filter(|v| v.active).count();
        }
        s
    }
    pub fn reset_stats(&mut self) {
        self.stats = StatAcc { limiter: 1.0, ..Default::default() };
    }

    /// Renders `seconds` of interleaved stereo.
    pub fn render_seconds(&mut self, seconds: f32) -> Vec<f32> {
        let mut out = vec![0.0; (seconds * self.sample_rate) as usize * 2];
        self.render(&mut out);
        out
    }

    /// Renders into an interleaved stereo buffer (overwrites).
    pub fn render(&mut self, out: &mut [f32]) {
        for chunk in out.chunks_mut(BLOCK * 2) {
            if chunk.len() % 2 == 1 {
                let n = chunk.len();
                chunk[n - 1] = 0.0;
                let n2 = n - 1;
                if n2 > 0 {
                    self.render_block(&mut chunk[..n2]);
                }
            } else {
                self.render_block(chunk);
            }
        }
    }

    fn render_block(&mut self, out: &mut [f32]) {
        let n = out.len() / 2;
        let sr = self.sample_rate;
        // Music: notes starting in this block.
        let mut notes = std::mem::take(&mut self.notes);
        notes.clear();
        self.music.process(n, sr, &mut notes);
        if let Some(bank) = self.music.bank.clone() {
            for no in &notes {
                let layers = bank.layers(no.inst);
                self.spawn_layers(layers, &no.spawn, Some(Pool::Music), 0);
            }
        }
        self.notes = notes;
        // Ambience.
        let mut acts = std::mem::take(&mut self.amb_actions);
        acts.clear();
        if let Some(ab) = self.amb.bank.clone() {
            let need = ab.layers.len() * 16;
            if self.amb_alive.len() != need {
                self.amb_alive = vec![0; need];
            }
            self.amb.process(n, sr, &self.amb_alive, &mut acts);
            for a in &acts {
                match *a {
                    AmbAction::Bed { layer, bed } => {
                        let l = ab.layers[layer].1.bed[bed];
                        let mut s = Spawn::new(l);
                        s.held = true;
                        s.bus = Bus::Ambience;
                        s.group = Group::Amb((layer * 16 + bed) as u16);
                        s.reverb = l.reverb.unwrap_or(0.06);
                        s.echo = l.echo.unwrap_or(0.0);
                        s.pan = l.pan;
                        // Beds start silent and follow the level.
                        if self.spawn(&s, Pool::Ambience).is_some() {
                            self.amb_alive[layer * 16 + bed] = 1;
                        }
                    }
                    AmbAction::Stop { layer } => {
                        let lo = (layer * 16) as u16;
                        self.release_where(|g| matches!(g, Group::Amb(k) if k >= lo && k < lo + 16));
                    }
                    AmbAction::Event { layer, event, gain, semis, pan } => {
                        let e = &ab.layers[layer].1.event[event];
                        let pitch = 2f32.powf(semis / 12.0);
                        if let Some(c) = &e.cue {
                            let bank = self.bank.clone();
                            match bank.cue(c) {
                                Some(cue) => {
                                    let mut cue = cue.clone();
                                    for l in &mut cue.layer {
                                        l.pan += pan;
                                    }
                                    self.cue_spawn(&cue, None, gain, pitch, Some(Bus::Ambience), Group::None, false);
                                }
                                None => self.warn_once("ambience cue", c),
                            }
                        } else {
                            let mut s = Spawn::new(Layer::default());
                            s.pitch = pitch;
                            s.gain = gain;
                            s.pan = pan;
                            s.bus = Bus::Ambience;
                            s.reverb = 0.12;
                            let layers = e.layer.clone();
                            self.spawn_layers(&layers, &s, Some(Pool::Sfx), 0);
                        }
                    }
                }
            }
        }
        self.amb_actions = acts;

        // Bus volumes ramp per block; dialog ducks the music a little.
        let prev_bus = self.bus_cur;
        for i in 0..6 {
            self.bus_cur[i] += (self.bus_vol[i] - self.bus_cur[i]) * 0.25;
        }
        let duck_target = if self.voice_energy > 1e-5 { 0.7 } else { 1.0 };
        let k = if duck_target < self.duck { 0.2 } else { 0.03 };
        self.duck += (duck_target - self.duck) * k;

        for b in &mut self.bufs {
            b[..n].fill(0.0);
        }
        self.amb_alive.fill(0);
        let listener = self.listener;
        let mut bufs = std::mem::take(&mut self.bufs);
        let (bus_bufs, rest) = bufs.split_at_mut(B_REV);
        let (sends, vbuf) = rest.split_at_mut(B_VOICE - B_REV);
        for p in 0..self.pools.len() {
            for vi in 0..self.pools[p].len() {
                if !self.pools[p][vi].active {
                    continue;
                }
                // Group gains.
                let group = self.pools[p][vi].group;
                let ext = match group {
                    Group::None => 1.0,
                    Group::Music { track, part } => match self.music.group_gain(track, part) {
                        Some(g) => g,
                        None => {
                            let v = &mut self.pools[p][vi];
                            if !v.dying {
                                v.kill(sr);
                            }
                            v.ext
                        }
                    },
                    Group::Amb(k) => match self.amb.bed_mod((k / 16) as usize) {
                        Some((g, pitch, cut)) => {
                            let v = &mut self.pools[p][vi];
                            v.ext_pitch = pitch;
                            v.ext_cut = cut;
                            if !v.released() {
                                if let Some(a) = self.amb_alive.get_mut(k as usize) {
                                    *a = a.saturating_add(1);
                                }
                            }
                            g
                        }
                        None => 0.0,
                    },
                    Group::Handle(h) => self.handles.get(&h).copied().unwrap_or(self.pools[p][vi].ext),
                };
                let v = &mut self.pools[p][vi];
                v.ext = ext;
                let mut span = 0.0;
                if let Some(pos) = v.pos {
                    let d = pos - listener.pos;
                    let dist = d.length();
                    span = if dist > 1e-3 { d.dot(listener.right) / dist * (dist / (dist + 1.5)) * 0.85 } else { 0.0 };
                    let r = v.range;
                    v.dist_gain = 1.0 / (1.0 + dist / r);
                    v.dist_a = OnePole::coef(20000.0 / (1.0 + 1.5 * dist / r), sr);
                }
                let (vl, vr) = vbuf.split_at_mut(1);
                let (vl, vr) = (&mut vl[0][..n], &mut vr[0][..n]);
                vl.fill(0.0);
                if v.l.stereo {
                    vr.fill(0.0);
                }
                let alive = v.render(vl, vr, sr);
                if !alive {
                    v.active = false;
                }
                if v.waiting() {
                    continue;
                }
                let bi = v.bus.index();
                let duck = if v.bus == Bus::Music { self.duck } else { 1.0 };
                let g1 = ext * self.bus_cur[bi] * v.dist_gain * duck;
                let pan = (v.pan + v.pan_mod + span).clamp(-1.0, 1.0);
                let a = (pan + 1.0) * FRAC_PI_4;
                let (tl, tr) = (a.cos() * g1, a.sin() * g1);
                // The first block of a voice starts at its gain (its envelope fades it in).
                let (gl0, gr0) = if v.ramped { (v.gl, v.gr) } else { (tl, tr) };
                v.ramped = true;
                v.gl = tl;
                v.gr = tr;
                let stereo = v.l.stereo;
                let rv = v.reverb * v.dist_gain.sqrt() / v.dist_gain.max(1e-3);
                let ec = v.echo;
                let slot = (bi.max(1) - 1) * 2;
                let (bl, br) = bus_bufs.split_at_mut(slot + 1);
                let (bl, br) = (&mut bl[slot][..n], &mut br[0][..n]);
                let inv = 1.0 / n as f32;
                let (dl, dr) = ((tl - gl0) * inv, (tr - gr0) * inv);
                let (s_rev, s_echo) = sends.split_at_mut(2);
                for i in 0..n {
                    let gl = gl0 + dl * i as f32;
                    let gr = gr0 + dr * i as f32;
                    let x = vl[i];
                    let y = if stereo { vr[i] } else { x };
                    let ol = x * gl;
                    let or = y * gr;
                    bl[i] += ol;
                    br[i] += or;
                    if rv > 0.0 {
                        s_rev[0][i] += ol * rv;
                        s_rev[1][i] += or * rv;
                    }
                    if ec > 0.0 {
                        s_echo[0][i] += ol * ec;
                        s_echo[1][i] += or * ec;
                    }
                }
            }
        }
        // Effects and master.
        let m0 = prev_bus[0] * self.master;
        let m1 = self.bus_cur[0] * self.master;
        let mut voice_sq = 0.0;
        for i in 0..n {
            let (rl, rr) = self.reverb.tick(sends[0][i], sends[1][i]);
            let (el, er) = self.echo.tick(sends[2][i], sends[3][i]);
            let mut l = rl + el;
            let mut r = rr + er;
            for b in 0..5 {
                let (x, y) = (bus_bufs[b * 2][i], bus_bufs[b * 2 + 1][i]);
                l += x;
                r += y;
                let bi = b + 1;
                self.stats.sq[bi] += ((x * x + y * y) * 0.5) as f64;
                let pk = x.abs().max(y.abs());
                if pk > self.stats.peak[bi] {
                    self.stats.peak[bi] = pk;
                }
                if bi == Bus::Voice.index() {
                    voice_sq += x * x;
                }
            }
            let m = m0 + (m1 - m0) * (i as f32 / n as f32);
            let (ol, or) = self.limiter.tick(l * m, r * m);
            out[i * 2] = ol;
            out[i * 2 + 1] = or;
            self.stats.sq[0] += ((ol * ol + or * or) * 0.5) as f64;
            let pk = ol.abs().max(or.abs());
            if pk > self.stats.peak[0] {
                self.stats.peak[0] = pk;
            }
        }
        self.voice_energy = voice_sq / n.max(1) as f32;
        self.stats.frames += n as u64;
        self.stats.limiter = self.stats.limiter.min(self.limiter.reduction_peak);
        self.limiter.reduction_peak = 1.0;
        self.time += n as u64;
        self.bufs = bufs;
    }
}

impl From<Patch> for Layer {
    fn from(p: Patch) -> Layer {
        let filtered = p.cutoff < 11000.0 || p.cutoff_end < 11000.0;
        Layer {
            wave: p.wave,
            freq: p.freq.max(1.0),
            freq_end: if p.freq_end != p.freq { Some(p.freq_end.max(1.0)) } else { None },
            glide: p.glide.max(1e-3),
            attack: p.attack,
            decay: p.decay,
            sustain: p.sustain,
            hold: p.hold,
            release: p.release,
            curve: crate::dsp::Curve::Lin,
            filter: if filtered { Filter::Lp } else { Filter::None },
            cutoff: p.cutoff,
            cutoff_end: if p.cutoff_end != p.cutoff { Some(p.cutoff_end) } else { None },
            q: Some(0.6),
            duty: p.duty,
            gain: p.volume,
            ..Layer::default()
        }
    }
}
