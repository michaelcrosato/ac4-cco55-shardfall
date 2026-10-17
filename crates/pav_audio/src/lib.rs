//! Synthesized sound, music and ambience. Every sound is built from oscillators, noise,
//! envelopes, filters and formants (no audio files): a data-driven sound bank
//! (`sounds.toml`, [`Bank`]), a music sequencer that plays ABC-notation songs with a band and a
//! singing crew (`music.toml` + `*.abc`, [`MusicBank`]), ambience layers (`ambience.toml`,
//! [`AmbienceBank`]), dialog voice blips and UI sounds, mixed on buses with reverb, echo and a
//! limiter. The same [`Mixer`] runs in the audio callback ([`AudioOut`], native and browser)
//! and offline ([`render_song`], [`render_cues`], [`Renderer`]) for tests and tools. The game
//! runs fine without a sound device.
//!
//! The original API stays: [`sounds_for`] patches for the engine's built-in events,
//! [`AudioOut::play_event`], [`render_events`] and [`write_wav`].

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};

use glam::Vec3;
use pav_core::SimEvent;
use pav_core::play::AudioState;

pub mod abc;
pub mod ambience;
pub mod analysis;
pub mod bank;
pub mod dsp;
pub mod instruments;
pub mod mixer;
pub mod music;
pub mod render;
pub mod voice;

pub use ambience::AmbienceBank;
pub use analysis::{png_rgba, spectrogram, stats_of};
pub use bank::{Bank, Bus, Cue, Layer, Rumble, Speaker, Wave};
pub use mixer::{Cmd, Mixer, Stats};
pub use music::MusicBank;
pub use render::{Renderer, SAMPLE_RATE, render_ambience, render_cues, render_song};

/// One synthesized sound.
#[derive(Clone, Copy, Debug)]
pub struct Patch {
    pub wave: Wave,
    /// Start / end frequency (Hz) and the glide time between them (s, exponential).
    pub freq: f32,
    pub freq_end: f32,
    pub glide: f32,
    pub attack: f32,
    pub decay: f32,
    pub sustain: f32,
    pub hold: f32,
    pub release: f32,
    pub volume: f32,
    /// One-pole low-pass cutoff (Hz) at start and end of the sound.
    pub cutoff: f32,
    pub cutoff_end: f32,
    /// Square-wave duty cycle.
    pub duty: f32,
}

impl Patch {
    pub const fn new(wave: Wave, freq: f32) -> Self {
        Self {
            wave,
            freq,
            freq_end: freq,
            glide: 0.1,
            attack: 0.002,
            decay: 0.1,
            sustain: 0.0,
            hold: 0.0,
            release: 0.05,
            volume: 0.3,
            cutoff: 12000.0,
            cutoff_end: 12000.0,
            duty: 0.5,
        }
    }
    pub fn length(&self) -> f32 {
        self.attack + self.decay + self.hold + self.release
    }
}
/// Where the ears are: position and the camera's right vector (for panning).
#[derive(Clone, Copy, Debug)]
pub struct Listener {
    pub pos: Vec3,
    pub right: Vec3,
}

impl Default for Listener {
    fn default() -> Self {
        Self { pos: Vec3::ZERO, right: Vec3::X }
    }
}

/// The sound bank: which patches play for a simulation event.
pub fn sounds_for(ev: &SimEvent) -> Vec<(Patch, f32)> {
    use Wave::*;
    match ev {
        SimEvent::Jump { .. } => vec![(
            Patch {
                freq_end: 640.0,
                glide: 0.11,
                decay: 0.12,
                volume: 0.18,
                cutoff: 3500.0,
                cutoff_end: 2500.0,
                duty: 0.3,
                ..Patch::new(Square, 280.0)
            },
            0.0,
        )],
        SimEvent::Land { speed, .. } => {
            let v = (speed / 14.0).clamp(0.15, 1.0);
            vec![
                (Patch { decay: 0.12, volume: 0.35 * v, cutoff: 1100.0, cutoff_end: 200.0, ..Patch::new(Noise, 1.0) }, 0.0),
                (Patch { freq_end: 45.0, glide: 0.1, decay: 0.14, volume: 0.45 * v, ..Patch::new(Sine, 110.0) }, 0.0),
            ]
        }
        SimEvent::Step { .. } => {
            vec![(Patch { decay: 0.045, volume: 0.07, cutoff: 1800.0, cutoff_end: 500.0, ..Patch::new(Noise, 1.0) }, 0.0)]
        }
        SimEvent::Throw { .. } => vec![(
            Patch { attack: 0.03, decay: 0.16, volume: 0.1, cutoff: 900.0, cutoff_end: 4500.0, ..Patch::new(Noise, 1.0) },
            0.0,
        )],
        SimEvent::Explosion { radius, .. } => {
            let v = (radius / 1.35).clamp(0.5, 2.0);
            vec![
                (
                    Patch {
                        attack: 0.004,
                        decay: 0.25,
                        sustain: 0.35,
                        hold: 0.1,
                        release: 0.6,
                        volume: 0.6 * v,
                        cutoff: 4000.0,
                        cutoff_end: 120.0,
                        ..Patch::new(Noise, 1.0)
                    },
                    0.0,
                ),
                (Patch { freq_end: 28.0, glide: 0.5, decay: 0.7, volume: 0.7 * v, ..Patch::new(Sine, 85.0) }, 0.0),
            ]
        }
        SimEvent::EnterRoom { .. } => vec![
            (Patch { decay: 0.25, volume: 0.12, ..Patch::new(Triangle, 659.3) }, 0.0),
            (Patch { decay: 0.35, volume: 0.12, ..Patch::new(Triangle, 987.8) }, 0.09),
        ],
        SimEvent::Hit { strength, .. } => {
            let v = (strength / 8.0).clamp(0.4, 1.2);
            vec![
                (Patch { decay: 0.09, volume: 0.3 * v, cutoff: 2500.0, cutoff_end: 300.0, ..Patch::new(Noise, 1.0) }, 0.0),
                (
                    Patch { freq_end: 90.0, glide: 0.12, decay: 0.16, volume: 0.3 * v, duty: 0.4, ..Patch::new(Square, 220.0) },
                    0.0,
                ),
            ]
        }
        // Shardfall.
        SimEvent::Swing { heavy, .. } => {
            let h = if *heavy { 1.4 } else { 1.0 };
            vec![(
                Patch {
                    attack: 0.02,
                    decay: 0.12 * h,
                    volume: 0.16 * h,
                    cutoff: 1800.0,
                    cutoff_end: 5200.0,
                    ..Patch::new(Noise, 1.0)
                },
                0.0,
            )]
        }
        SimEvent::Strike { power, element, crit, .. } => {
            let v = (power.sqrt() / 4.0).clamp(0.35, 1.2) * if *crit { 1.3 } else { 1.0 };
            let tone = [150.0, 120.0, 520.0, 900.0, 260.0][(*element as usize).min(4)];
            let mut out = vec![
                (Patch { decay: 0.07, volume: 0.24 * v, cutoff: 3200.0, cutoff_end: 400.0, ..Patch::new(Noise, 1.0) }, 0.0),
                (
                    Patch {
                        freq_end: tone * 0.5,
                        glide: 0.08,
                        decay: 0.1,
                        volume: 0.2 * v,
                        duty: 0.35,
                        ..Patch::new(Square, tone)
                    },
                    0.0,
                ),
            ];
            if *crit {
                out.push((Patch { decay: 0.18, volume: 0.12, ..Patch::new(Triangle, 1760.0) }, 0.01));
            }
            out
        }
        SimEvent::Slain { size, .. } => {
            let k = size.clamp(0.3, 1.5);
            vec![
                (
                    Patch {
                        freq_end: 60.0,
                        glide: 0.3,
                        decay: 0.3,
                        volume: 0.22 * k,
                        duty: 0.5,
                        ..Patch::new(Square, 180.0 / k)
                    },
                    0.0,
                ),
                (Patch { decay: 0.25, volume: 0.15, cutoff: 1500.0, cutoff_end: 200.0, ..Patch::new(Noise, 1.0) }, 0.02),
            ]
        }
        SimEvent::Slam { radius, .. } => {
            let k = (radius / 2.5).clamp(0.6, 1.5);
            vec![
                (Patch { freq_end: 40.0, glide: 0.35, decay: 0.4, volume: 0.45 * k, ..Patch::new(Sine, 110.0) }, 0.0),
                (Patch { decay: 0.3, volume: 0.3 * k, cutoff: 2200.0, cutoff_end: 150.0, ..Patch::new(Noise, 1.0) }, 0.0),
            ]
        }
        SimEvent::Blast { element, .. } => {
            let cut = [2500.0, 3500.0, 6000.0, 8000.0, 2000.0][(*element as usize).min(4)];
            vec![
                (
                    Patch { attack: 0.003, decay: 0.3, volume: 0.35, cutoff: cut, cutoff_end: 200.0, ..Patch::new(Noise, 1.0) },
                    0.0,
                ),
                (Patch { freq_end: 50.0, glide: 0.25, decay: 0.3, volume: 0.25, ..Patch::new(Sine, 140.0) }, 0.0),
            ]
        }
        SimEvent::Spell { element, .. } => {
            let f = [300.0, 260.0, 900.0, 1400.0, 420.0][(*element as usize).min(4)];
            vec![(
                Patch {
                    freq_end: f * 1.8,
                    glide: 0.15,
                    attack: 0.02,
                    decay: 0.18,
                    volume: 0.12,
                    duty: 0.25,
                    ..Patch::new(Saw, f)
                },
                0.0,
            )]
        }
        SimEvent::Coin { .. } => vec![
            (Patch { decay: 0.08, volume: 0.1, ..Patch::new(Triangle, 1980.0) }, 0.0),
            (Patch { decay: 0.12, volume: 0.09, ..Patch::new(Triangle, 2640.0) }, 0.05),
        ],
        // Drops chime by rarity: rares ring, uniques ring a chord.
        SimEvent::Loot { rarity, .. } => match rarity {
            0 => vec![(Patch { decay: 0.05, volume: 0.05, cutoff: 2500.0, ..Patch::new(Triangle, 880.0) }, 0.0)],
            1 => vec![(Patch { decay: 0.12, volume: 0.08, ..Patch::new(Triangle, 1320.0) }, 0.0)],
            2 => vec![
                (Patch { decay: 0.25, volume: 0.12, ..Patch::new(Triangle, 1568.0) }, 0.0),
                (Patch { decay: 0.35, volume: 0.1, ..Patch::new(Sine, 2093.0) }, 0.08),
            ],
            _ => vec![
                (Patch { decay: 0.5, volume: 0.14, ..Patch::new(Triangle, 784.0) }, 0.0),
                (Patch { decay: 0.5, volume: 0.13, ..Patch::new(Triangle, 988.0) }, 0.06),
                (Patch { decay: 0.6, volume: 0.13, ..Patch::new(Triangle, 1175.0) }, 0.12),
                (Patch { decay: 0.9, volume: 0.12, ..Patch::new(Sine, 1568.0) }, 0.2),
            ],
        },
        SimEvent::Pickup { .. } => {
            vec![(Patch { freq_end: 1400.0, glide: 0.06, decay: 0.07, volume: 0.08, ..Patch::new(Triangle, 700.0) }, 0.0)]
        }
        SimEvent::Travel { .. } => vec![
            (Patch { freq_end: 1200.0, glide: 0.5, decay: 0.6, volume: 0.12, ..Patch::new(Sine, 200.0) }, 0.0),
            (Patch { freq_end: 2400.0, glide: 0.4, decay: 0.5, volume: 0.06, ..Patch::new(Triangle, 600.0) }, 0.1),
        ],
        SimEvent::Potion { .. } => {
            vec![(Patch { freq_end: 700.0, glide: 0.3, decay: 0.3, volume: 0.14, ..Patch::new(Sine, 300.0) }, 0.0)]
        }
        SimEvent::Block { .. } => {
            vec![(Patch { decay: 0.2, volume: 0.2, cutoff: 6000.0, cutoff_end: 3000.0, ..Patch::new(Square, 1200.0) }, 0.0)]
        }
        SimEvent::LevelUp { .. } => vec![
            (Patch { decay: 0.2, volume: 0.16, ..Patch::new(Triangle, 523.0) }, 0.0),
            (Patch { decay: 0.2, volume: 0.16, ..Patch::new(Triangle, 659.0) }, 0.1),
            (Patch { decay: 0.2, volume: 0.16, ..Patch::new(Triangle, 784.0) }, 0.2),
            (Patch { decay: 0.5, volume: 0.18, ..Patch::new(Triangle, 1047.0) }, 0.3),
        ],
        SimEvent::Respawn { .. } => vec![
            (Patch { freq_end: 880.0, glide: 0.18, decay: 0.2, volume: 0.12, ..Patch::new(Sine, 220.0) }, 0.0),
            (Patch { decay: 0.15, volume: 0.08, ..Patch::new(Triangle, 1320.0) }, 0.16),
        ],
        SimEvent::CourseStart { .. } => vec![(Patch { decay: 0.12, volume: 0.14, ..Patch::new(Square, 880.0) }, 0.0)],
        SimEvent::Gate { ok, .. } => {
            if *ok {
                vec![(Patch { decay: 0.1, volume: 0.12, ..Patch::new(Triangle, 1046.5) }, 0.0)]
            } else {
                vec![(
                    Patch { freq_end: 160.0, glide: 0.2, decay: 0.25, volume: 0.14, duty: 0.3, ..Patch::new(Square, 330.0) },
                    0.0,
                )]
            }
        }
        SimEvent::CourseFinish { new_best, .. } => {
            let mut v = vec![
                (Patch { decay: 0.18, volume: 0.13, ..Patch::new(Triangle, 523.3) }, 0.0),
                (Patch { decay: 0.18, volume: 0.13, ..Patch::new(Triangle, 659.3) }, 0.1),
                (Patch { decay: 0.4, volume: 0.14, ..Patch::new(Triangle, 784.0) }, 0.2),
            ];
            if *new_best {
                v.push((Patch { decay: 0.5, volume: 0.13, ..Patch::new(Triangle, 1046.5) }, 0.34));
            }
            v
        }
        SimEvent::Checkpoint { .. } => vec![
            (Patch { decay: 0.12, volume: 0.1, ..Patch::new(Sine, 1174.7) }, 0.0),
            (Patch { decay: 0.2, volume: 0.1, ..Patch::new(Sine, 1568.0) }, 0.07),
        ],
        SimEvent::Splash { .. } => vec![(
            Patch { attack: 0.01, decay: 0.35, volume: 0.25, cutoff: 3000.0, cutoff_end: 400.0, ..Patch::new(Noise, 1.0) },
            0.0,
        )],
        SimEvent::Pad { .. } => {
            vec![(Patch { freq_end: 1200.0, glide: 0.08, decay: 0.12, volume: 0.1, ..Patch::new(Sine, 600.0) }, 0.0)]
        }
        SimEvent::Grab { .. } => {
            vec![(Patch { decay: 0.05, volume: 0.12, cutoff: 1500.0, cutoff_end: 600.0, ..Patch::new(Noise, 1.0) }, 0.0)]
        }
        SimEvent::Break { .. } => vec![
            (Patch { decay: 0.18, volume: 0.3, cutoff: 6000.0, cutoff_end: 900.0, ..Patch::new(Noise, 1.0) }, 0.0),
            (Patch { freq_end: 70.0, glide: 0.15, decay: 0.2, volume: 0.25, ..Patch::new(Sine, 160.0) }, 0.0),
        ],
        SimEvent::Crack { .. } => {
            vec![(Patch { decay: 0.04, volume: 0.12, cutoff: 5000.0, cutoff_end: 2000.0, ..Patch::new(Noise, 1.0) }, 0.0)]
        }
        SimEvent::Bounce { .. } => {
            vec![(Patch { freq_end: 900.0, glide: 0.18, decay: 0.22, volume: 0.16, ..Patch::new(Sine, 180.0) }, 0.0)]
        }
        SimEvent::Roll { .. } => vec![(
            Patch { attack: 0.02, decay: 0.2, volume: 0.1, cutoff: 600.0, cutoff_end: 2400.0, ..Patch::new(Noise, 1.0) },
            0.0,
        )],
        SimEvent::Spotted { .. } => vec![
            (Patch { decay: 0.18, volume: 0.16, ..Patch::new(Square, 990.0) }, 0.0),
            (Patch { decay: 0.25, volume: 0.16, ..Patch::new(Square, 660.0) }, 0.16),
        ],
        SimEvent::Shot { .. } => {
            vec![(Patch { freq_end: 420.0, glide: 0.06, decay: 0.06, volume: 0.05, ..Patch::new(Square, 1500.0) }, 0.0)]
        }
        SimEvent::Damage { .. } => {
            vec![(Patch { decay: 0.03, volume: 0.08, cutoff: 3500.0, cutoff_end: 1200.0, ..Patch::new(Noise, 1.0) }, 0.0)]
        }
        SimEvent::Destroyed { size, .. } => {
            let v = size.clamp(0.4, 2.0);
            vec![
                (
                    Patch {
                        attack: 0.003,
                        decay: 0.3,
                        volume: 0.35 * v,
                        cutoff: 5000.0,
                        cutoff_end: 200.0,
                        ..Patch::new(Noise, 1.0)
                    },
                    0.0,
                ),
                (Patch { freq_end: 40.0, glide: 0.35, decay: 0.4, volume: 0.4 * v, ..Patch::new(Sine, 140.0) }, 0.0),
            ]
        }
        _ => Vec::new(),
    }
}

/// Position of an event (None = not spatial).
pub fn event_pos(ev: &SimEvent) -> Option<Vec3> {
    match ev {
        SimEvent::Jump { pos }
        | SimEvent::Land { pos, .. }
        | SimEvent::Step { pos }
        | SimEvent::Throw { pos }
        | SimEvent::Explosion { pos, .. }
        | SimEvent::Impact { pos, .. }
        | SimEvent::Hit { pos, .. }
        | SimEvent::Splash { pos }
        | SimEvent::Grab { pos }
        | SimEvent::Roll { pos }
        | SimEvent::Break { pos }
        | SimEvent::Crack { pos }
        | SimEvent::Bounce { pos }
        | SimEvent::Shot { pos }
        | SimEvent::Damage { pos }
        | SimEvent::Destroyed { pos, .. }
        | SimEvent::Swing { pos, .. }
        | SimEvent::Strike { pos, .. }
        | SimEvent::Slain { pos, .. }
        | SimEvent::Slam { pos, .. }
        | SimEvent::Blast { pos, .. }
        | SimEvent::Spell { pos, .. }
        | SimEvent::Coin { pos }
        | SimEvent::Loot { pos, .. }
        | SimEvent::Pickup { pos, .. }
        | SimEvent::Block { pos } => Some(*pos),
        _ => None,
    }
}

/// Plays an event's sounds with distance attenuation and panning.
pub fn play_event(mixer: &mut Mixer, ev: &SimEvent, l: &Listener, sfx: f32) {
    mixer.play_event(ev, l, sfx);
}

/// Renders timed events offline: (time in seconds, event). Returns interleaved stereo.
pub fn render_events(events: &[(f32, SimEvent)], duration: f32, sample_rate: f32, l: &Listener) -> Vec<f32> {
    let mut mixer = Mixer::new(sample_rate);
    let total = (duration * sample_rate) as usize;
    let mut out = vec![0.0f32; total * 2];
    let block = 256;
    let mut i = 0;
    let mut ev = events.iter().peekable();
    while i < total {
        let t = i as f32 / sample_rate;
        while let Some((et, e)) = ev.peek() {
            if *et > t {
                break;
            }
            mixer.play_event(e, l, 1.0);
            ev.next();
        }
        let n = block.min(total - i);
        mixer.render(&mut out[i * 2..(i + n) * 2]);
        i += n;
    }
    out
}

/// Interleaved stereo f32 samples as a 16-bit PCM WAV file's bytes.
pub fn wav_bytes(samples: &[f32], sample_rate: u32) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut b = Vec::with_capacity(44 + data_len as usize);
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data_len).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes()); // PCM
    b.extend_from_slice(&2u16.to_le_bytes()); // stereo
    b.extend_from_slice(&sample_rate.to_le_bytes());
    b.extend_from_slice(&(sample_rate * 4).to_le_bytes());
    b.extend_from_slice(&4u16.to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        b.extend_from_slice(&((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
    }
    b
}

/// Writes interleaved stereo f32 samples as a 16-bit PCM WAV file.
pub fn write_wav(path: &std::path::Path, samples: &[f32], sample_rate: u32) -> std::io::Result<()> {
    if let Some(d) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(d)?;
    }
    std::fs::write(path, wav_bytes(samples, sample_rate))
}

/// A held cue started with [`AudioOut::start_loop`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LoopHandle(pub u32);

#[derive(Clone, Debug, Default)]
struct Shared {
    stats: Stats,
    now_playing: Option<(String, String)>,
}

/// Live output through the default sound device (cpal: WASAPI/ALSA/..., Web Audio in the
/// browser). Every method only queues a message for the audio callback, so they are cheap and
/// never block.
pub struct AudioOut {
    _stream: cpal::Stream,
    tx: Sender<Cmd>,
    pub sample_rate: u32,
    pub device: String,
    shared: Arc<Mutex<Shared>>,
    bank: Mutex<Arc<Bank>>,
    next_handle: AtomicU32,
}

impl AudioOut {
    pub fn start() -> anyhow::Result<Self> {
        use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
        let host = cpal::default_host();
        let device = host.default_output_device().ok_or_else(|| anyhow::anyhow!("no sound output device"))?;
        let name = device.description().map(|d| d.to_string()).unwrap_or_else(|_| "default".into());
        let supported = device.default_output_config()?;
        let format = supported.sample_format();
        let config: cpal::StreamConfig = supported.config();
        let rate = config.sample_rate as u32;
        let channels = config.channels as usize;
        let (tx, rx) = channel::<Cmd>();
        let shared = Arc::new(Mutex::new(Shared::default()));
        let stream = match format {
            cpal::SampleFormat::F32 => build::<f32>(&device, config, rx, channels, shared.clone())?,
            cpal::SampleFormat::I16 => build::<i16>(&device, config, rx, channels, shared.clone())?,
            cpal::SampleFormat::U16 => build::<u16>(&device, config, rx, channels, shared.clone())?,
            other => anyhow::bail!("unsupported sample format {other:?}"),
        };
        stream.play()?;
        Ok(Self {
            _stream: stream,
            tx,
            sample_rate: rate,
            device: name,
            shared,
            bank: Mutex::new(Arc::new(Bank::default())),
            next_handle: AtomicU32::new(1),
        })
    }

    fn send(&self, c: Cmd) {
        let _ = self.tx.send(c);
    }

    /// Plays a simulation event: `SimEvent::Cue` through the sound bank, the engine's built-in
    /// events through [`sounds_for`]. `sfx` scales the gain.
    pub fn play_event(&self, ev: &SimEvent, l: &Listener, sfx: f32) {
        match ev {
            SimEvent::Cue { .. } => self.send(Cmd::Event(Box::new(ev.clone()), *l, sfx)),
            _ => {
                let (pan, gain) = match event_pos(ev) {
                    Some(p) => {
                        let d = p - l.pos;
                        ((d.dot(l.right) / 12.0).clamp(-0.85, 0.85), 1.0 / (1.0 + d.length() / 14.0))
                    }
                    None => (0.0, 1.0),
                };
                for (patch, delay) in sounds_for(ev) {
                    self.send(Cmd::Legacy(patch, pan, gain * sfx, delay));
                }
            }
        }
    }

    pub fn set_master(&self, v: f32) {
        self.set_bus(Bus::Master, v);
    }

    /// Starts output again. Browsers keep sound off until the page gets a click or key press,
    /// so the browser build calls this on the first input.
    pub fn resume(&self) {
        use cpal::traits::StreamTrait;
        let _ = self._stream.play();
    }

    /// Loads (replaces) the sound bank (`sounds.toml` text).
    pub fn load_bank(&self, text: &str) -> anyhow::Result<()> {
        let b = Arc::new(Bank::parse(text)?);
        if let Ok(mut m) = self.bank.lock() {
            *m = b.clone();
        }
        self.send(Cmd::Bank(b));
        Ok(())
    }

    /// Loads the music: `music.toml` text and the songs as (name, ABC text).
    pub fn load_music(&self, music_toml: &str, songs: &[(String, String)]) -> anyhow::Result<()> {
        self.send(Cmd::Music(Arc::new(MusicBank::parse(music_toml, songs)?)));
        Ok(())
    }

    /// Loads the ambience layers (`ambience.toml` text).
    pub fn load_ambience(&self, text: &str) -> anyhow::Result<()> {
        self.send(Cmd::Ambience(Arc::new(AmbienceBank::parse(text)?)));
        Ok(())
    }

    /// The game's audio state for this frame: music state or song + intensity (crossfades on
    /// change), ambience levels, and the listener (the state's, else `listener`).
    pub fn apply(&self, state: &AudioState, listener: &Listener) {
        self.send(Cmd::Apply(Box::new(state.clone()), *listener));
    }

    /// Plays a bank cue at `pos` (gain and pitch multiply). Unknown names are logged once.
    pub fn play_cue(&self, name: &str, pos: Vec3, gain: f32, pitch: f32, listener: &Listener) {
        self.send(Cmd::Cue { name: name.to_string(), pos, gain, pitch, listener: *listener });
    }

    /// One syllable of a dialog voice (bung, pip, cask, drye, sobers, molasses, guard, crew,
    /// crier, ballast).
    pub fn voice_blip(&self, speaker: &str) {
        self.send(Cmd::Blip(speaker.to_string()));
    }

    /// A UI sound: ui_move, ui_confirm, ui_back, ui_toggle, ui_slider, ui_error, ui_start.
    pub fn ui(&self, name: &str) {
        self.send(Cmd::Ui(name.to_string()));
    }

    pub fn set_bus(&self, bus: Bus, vol: f32) {
        self.send(Cmd::Bus(bus, vol));
    }

    /// Starts a cue held until [`AudioOut::stop_loop`] (a burning deck, a creaking hull).
    pub fn start_loop(&self, cue: &str, pos: Option<Vec3>, gain: f32) -> LoopHandle {
        let h = self.next_handle.fetch_add(1, Ordering::Relaxed);
        self.send(Cmd::StartLoop { handle: h, cue: cue.to_string(), pos, gain });
        LoopHandle(h)
    }
    pub fn stop_loop(&self, h: LoopHandle) {
        self.send(Cmd::StopLoop(h.0));
    }
    pub fn set_loop_gain(&self, h: LoopHandle, gain: f32) {
        self.send(Cmd::LoopGain(h.0, gain));
    }
    pub fn set_loop_pos(&self, h: LoopHandle, pos: Vec3) {
        self.send(Cmd::LoopPos(h.0, pos));
    }

    /// A music stinger on top of the music (`music.toml` [stinger.*] or any song, once).
    pub fn stinger(&self, name: &str) {
        self.send(Cmd::Stinger(name.to_string()));
    }
    /// Next (+1) / previous (-1) song of the current music state's playlist.
    pub fn music_skip(&self, delta: i32) {
        self.send(Cmd::Skip(delta));
    }

    /// Bus loudness over the last half second or so.
    pub fn stats(&self) -> Stats {
        self.shared.lock().map(|s| s.stats).unwrap_or_default()
    }
    /// (song name, title) playing now.
    pub fn now_playing(&self) -> Option<(String, String)> {
        self.shared.lock().ok().and_then(|s| s.now_playing.clone())
    }
    /// The rumble a cue asks for (from the loaded bank).
    pub fn rumble(&self, cue: &str) -> Option<Rumble> {
        self.bank.lock().ok().and_then(|b| b.rumble(cue))
    }
}

fn build<T>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    rx: Receiver<Cmd>,
    channels: usize,
    shared: Arc<Mutex<Shared>>,
) -> anyhow::Result<cpal::Stream>
where
    T: cpal::SizedSample + cpal::FromSample<f32>,
{
    use cpal::traits::DeviceTrait;
    let mut mixer = Mixer::new(config.sample_rate as f32);
    let mut buf: Vec<f32> = Vec::new();
    let mut playing: Option<String> = None;
    let stream = device.build_output_stream::<T, _, _>(
        config,
        move |data: &mut [T], _| {
            while let Ok(c) = rx.try_recv() {
                mixer.handle(c);
            }
            let frames = data.len() / channels.max(1);
            buf.resize(frames * 2, 0.0);
            mixer.render(&mut buf);
            for (i, frame) in data.chunks_mut(channels.max(1)).enumerate() {
                for (c, s) in frame.iter_mut().enumerate() {
                    *s = T::from_sample(buf[i * 2 + c.min(1)]);
                }
            }
            let st = mixer.stats();
            if st.seconds >= 0.5 {
                let np = mixer.now_playing();
                if let Ok(mut sh) = shared.try_lock() {
                    sh.stats = st;
                    if np.as_ref().map(|n| &n.0) != playing.as_ref() {
                        playing = np.as_ref().map(|n| n.0.clone());
                        sh.now_playing = np;
                    }
                }
                mixer.reset_stats();
            }
        },
        |e| log::warn!("audio stream error: {e}"),
        None,
    )?;
    Ok(stream)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explosion_is_loud_then_silent() {
        let ev = SimEvent::Explosion { pos: Vec3::ZERO, radius: 1.35 };
        let out = render_events(&[(0.0, ev)], 2.0, 22050.0, &Listener::default());
        let peak_early = out[..22050].iter().fold(0.0f32, |m, s| m.max(s.abs()));
        let peak_late = out[out.len() - 2000..].iter().fold(0.0f32, |m, s| m.max(s.abs()));
        assert!(peak_early > 0.1, "audible: {peak_early}");
        assert!(peak_late < 1e-3, "decays: {peak_late}");
    }
}
