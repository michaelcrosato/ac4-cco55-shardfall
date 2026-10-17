//! The band: instrument presets built from [`Layer`]s. Pitched instruments are templates at
//! `freq = 1` (the sequencer multiplies by the note's Hz); drums use absolute frequencies.
//! Loudness is roughly matched, so a part's `gain = 1` sits well in the mix.

use crate::bank::{Filter, Layer, Wave};
use crate::dsp::{Formant, Lfo, LfoShape, LfoTo};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Inst {
    /// Right-hand melody: three detuned reeds and a bellows swell.
    Accordion,
    /// Left-hand chord buttons (short, darker).
    Squeeze,
    Fiddle,
    Cello,
    /// Fiddle bowed in tremolo (storms).
    Tremolo,
    Whistle,
    /// Karplus-Strong guitar.
    Guitar,
    Bass,
    Stomp,
    Bodhran,
    Clap,
    /// The crew: formant-filtered saws, several singers per voice.
    Choir,
    /// The shantyman (Old Bung) singing the calls.
    Solo,
    Horn,
    Drone,
    Pulse,
    Tick,
    Bell,
    Timpani,
    Snare,
    /// A crew shout ("Hey!").
    Shout,
    /// A soft hummed pad.
    Hum,
}

impl Inst {
    pub const ALL: [Inst; 22] = [
        Inst::Accordion,
        Inst::Squeeze,
        Inst::Fiddle,
        Inst::Cello,
        Inst::Tremolo,
        Inst::Whistle,
        Inst::Guitar,
        Inst::Bass,
        Inst::Stomp,
        Inst::Bodhran,
        Inst::Clap,
        Inst::Choir,
        Inst::Solo,
        Inst::Horn,
        Inst::Drone,
        Inst::Pulse,
        Inst::Tick,
        Inst::Bell,
        Inst::Timpani,
        Inst::Snare,
        Inst::Shout,
        Inst::Hum,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Inst::Accordion => "accordion",
            Inst::Squeeze => "squeeze",
            Inst::Fiddle => "fiddle",
            Inst::Cello => "cello",
            Inst::Tremolo => "tremolo",
            Inst::Whistle => "whistle",
            Inst::Guitar => "guitar",
            Inst::Bass => "bass",
            Inst::Stomp => "stomp",
            Inst::Bodhran => "bodhran",
            Inst::Clap => "clap",
            Inst::Choir => "choir",
            Inst::Solo => "solo",
            Inst::Horn => "horn",
            Inst::Drone => "drone",
            Inst::Pulse => "pulse",
            Inst::Tick => "tick",
            Inst::Bell => "bell",
            Inst::Timpani => "timpani",
            Inst::Snare => "snare",
            Inst::Shout => "shout",
            Inst::Hum => "hum",
        }
    }

    pub fn parse(s: &str) -> Option<Inst> {
        Inst::ALL.into_iter().find(|i| i.name() == s)
    }

    /// Plays at the note's pitch (otherwise fixed-pitch percussion).
    pub fn pitched(self) -> bool {
        !matches!(self, Inst::Stomp | Inst::Bodhran | Inst::Clap | Inst::Tick | Inst::Snare | Inst::Shout)
    }

    /// Rings on after the note (not cut at the note's end).
    pub fn rings(self) -> bool {
        matches!(self, Inst::Bell | Inst::Timpani)
    }

    /// Sung (takes vowels and consonants from the lyrics).
    pub fn sung(self) -> bool {
        matches!(self, Inst::Choir | Inst::Solo | Inst::Shout | Inst::Hum)
    }

    /// Default reverb and echo sends.
    pub fn sends(self) -> (f32, f32) {
        match self {
            Inst::Choir | Inst::Hum => (0.42, 0.0),
            Inst::Solo => (0.32, 0.06),
            Inst::Fiddle | Inst::Cello | Inst::Tremolo => (0.3, 0.0),
            Inst::Whistle => (0.34, 0.1),
            Inst::Horn => (0.36, 0.05),
            Inst::Guitar => (0.26, 0.05),
            Inst::Bell => (0.45, 0.15),
            Inst::Drone => (0.5, 0.0),
            Inst::Pulse | Inst::Tick => (0.3, 0.18),
            Inst::Timpani => (0.4, 0.0),
            Inst::Stomp | Inst::Bodhran => (0.16, 0.0),
            Inst::Clap | Inst::Snare | Inst::Shout => (0.25, 0.0),
            Inst::Accordion | Inst::Squeeze => (0.2, 0.0),
            Inst::Bass => (0.05, 0.0),
        }
    }

    pub fn layers(self) -> Vec<Layer> {
        let note = Layer { freq: 1.0, ..Layer::default() };
        let vib =
            |rate: f32, depth: f32, delay: f32| Lfo { to: LfoTo::Pitch, rate, depth, shape: LfoShape::Sine, delay, phase: None };
        let wander =
            |rate: f32, depth: f32| Lfo { to: LfoTo::Pitch, rate, depth, shape: LfoShape::Random, delay: 0.0, phase: None };
        let none = Lfo::default();
        match self {
            Inst::Accordion => vec![Layer {
                wave: Wave::Saw,
                unison: 3,
                detune: 17.0,
                drift: 1.5,
                wave2: Some(Wave::Square),
                ratio2: 1.0,
                gain2: 0.5,
                duty: 0.42,
                attack: 0.022,
                decay: 0.35,
                sustain: 0.82,
                release: 0.09,
                filter: Filter::Lp,
                cutoff: 2300.0,
                key_track: 0.4,
                res: 0.3,
                env_cutoff: 0.35,
                lfo: [
                    Lfo { to: LfoTo::Amp, rate: 0.42, depth: 0.16, shape: LfoShape::Sine, delay: 0.0, phase: None },
                    none,
                    none,
                ],
                gain: 0.25,
                ..note
            }],
            Inst::Squeeze => vec![Layer {
                wave: Wave::Saw,
                unison: 2,
                detune: 12.0,
                wave2: Some(Wave::Square),
                ratio2: 1.0,
                gain2: 0.45,
                duty: 0.38,
                attack: 0.01,
                decay: 0.16,
                sustain: 0.55,
                release: 0.07,
                filter: Filter::Lp,
                cutoff: 1500.0,
                key_track: 0.3,
                res: 0.2,
                env_cutoff: 0.6,
                lfo: [
                    Lfo { to: LfoTo::Amp, rate: 0.42, depth: 0.12, shape: LfoShape::Sine, delay: 0.0, phase: None },
                    none,
                    none,
                ],
                gain: 0.16,
                ..note
            }],
            Inst::Fiddle | Inst::Tremolo => {
                let tremolo = self == Inst::Tremolo;
                vec![Layer {
                    wave: Wave::Saw,
                    unison: 2,
                    detune: 5.0,
                    drift: 3.0,
                    noise: 0.04,
                    attack: if tremolo { 0.02 } else { 0.07 },
                    decay: 0.25,
                    sustain: 0.85,
                    release: 0.14,
                    pitch_curve: crate::bank::PitchCurve {
                        n: 2,
                        pts: [[0.0, -0.3], [0.07, 0.0], [0.0; 2], [0.0; 2], [0.0; 2], [0.0; 2]],
                    },
                    formant: Some(Formant::Violin),
                    formant_dry: 0.22,
                    filter: Filter::Lp,
                    cutoff: 6500.0,
                    res: 0.1,
                    lfo: [
                        vib(5.6, 0.16, 0.38),
                        if tremolo {
                            Lfo { to: LfoTo::Amp, rate: 11.0, depth: 0.75, shape: LfoShape::Triangle, delay: 0.0, phase: None }
                        } else {
                            Lfo { to: LfoTo::Amp, rate: 1.3, depth: 0.08, shape: LfoShape::Random, delay: 0.0, phase: None }
                        },
                        wander(7.0, 0.04),
                    ],
                    gain: 0.34,
                    ..note
                }]
            }
            Inst::Cello => vec![Layer {
                wave: Wave::Saw,
                unison: 2,
                detune: 6.0,
                drift: 2.5,
                noise: 0.03,
                attack: 0.1,
                decay: 0.3,
                sustain: 0.85,
                release: 0.22,
                formant: Some(Formant::Cello),
                formant_dry: 0.3,
                filter: Filter::Lp,
                cutoff: 3800.0,
                lfo: [vib(5.0, 0.14, 0.45), wander(5.0, 0.03), none],
                gain: 0.36,
                ..note
            }],
            Inst::Whistle => vec![
                Layer {
                    wave: Wave::Triangle,
                    wave2: Some(Wave::Sine),
                    ratio2: 2.0,
                    gain2: 0.1,
                    noise: 0.05,
                    attack: 0.03,
                    decay: 0.2,
                    sustain: 0.88,
                    release: 0.07,
                    filter: Filter::Lp,
                    cutoff: 3200.0,
                    key_track: 0.6,
                    lfo: [vib(5.3, 0.13, 0.42), wander(4.0, 0.03), none],
                    gain: 0.26,
                    ..note
                },
                // The chiff of breath at the start of each note.
                Layer {
                    wave: Wave::Noise,
                    attack: 0.003,
                    decay: 0.05,
                    filter: Filter::Bp,
                    cutoff: 523.0,
                    key_track: 1.0,
                    q: Some(2.5),
                    gain: 0.08,
                    ..note
                },
            ],
            Inst::Guitar => vec![Layer {
                wave: Wave::Pluck,
                pluck_decay: 2.4,
                pluck_bright: 0.45,
                attack: 0.0,
                decay: 3.0,
                sustain: 1.0,
                release: 0.12,
                filter: Filter::Lp,
                cutoff: 3600.0,
                res: 0.15,
                gain: 1.0,
                ..note
            }],
            Inst::Bass => vec![Layer {
                wave: Wave::Triangle,
                wave2: Some(Wave::Saw),
                ratio2: 1.0,
                gain2: 0.28,
                attack: 0.006,
                decay: 0.4,
                sustain: 0.5,
                release: 0.09,
                filter: Filter::Lp,
                cutoff: 520.0,
                key_track: 0.5,
                env_cutoff: 1.1,
                res: 0.15,
                gain: 0.5,
                ..note
            }],
            Inst::Stomp => vec![
                Layer {
                    wave: Wave::Sine,
                    freq: 96.0,
                    freq_end: Some(46.0),
                    glide: 0.07,
                    decay: 0.24,
                    drive: 0.35,
                    gain: 0.62,
                    ..note
                },
                Layer {
                    wave: Wave::Brown,
                    filter: Filter::Lp,
                    cutoff: 1100.0,
                    cutoff_end: Some(220.0),
                    decay: 0.08,
                    gain: 0.55,
                    ..note
                },
                Layer { wave: Wave::Noise, filter: Filter::Bp, cutoff: 1900.0, q: Some(1.1), decay: 0.018, gain: 0.07, ..note },
            ],
            Inst::Bodhran => vec![
                Layer {
                    wave: Wave::Sine,
                    freq: 128.0,
                    freq_end: Some(80.0),
                    glide: 0.06,
                    decay: 0.3,
                    drive: 0.15,
                    gain: 0.62,
                    ..note
                },
                Layer {
                    wave: Wave::Noise,
                    filter: Filter::Bp,
                    cutoff: 520.0,
                    cutoff_end: Some(300.0),
                    q: Some(1.3),
                    decay: 0.05,
                    gain: 0.34,
                    ..note
                },
            ],
            Inst::Clap => vec![
                Layer {
                    wave: Wave::Noise,
                    filter: Filter::Bp,
                    cutoff: 1250.0,
                    q: Some(0.9),
                    decay: 0.075,
                    repeat: 4,
                    repeat_gap: 0.007,
                    delay_rand: 0.012,
                    gain_rand: 3.0,
                    gain: 0.75,
                    ..note
                },
                Layer {
                    wave: Wave::Noise,
                    filter: Filter::Bp,
                    cutoff: 2600.0,
                    q: Some(1.2),
                    decay: 0.12,
                    delay: 0.01,
                    gain: 0.3,
                    ..note
                },
            ],
            Inst::Choir => vec![Layer {
                wave: Wave::Voice,
                unison: 3,
                detune: 20.0,
                drift: 7.0,
                noise: 0.025,
                attack: 0.075,
                decay: 0.35,
                sustain: 0.9,
                release: 0.2,
                pitch_curve: crate::bank::PitchCurve {
                    n: 2,
                    pts: [[0.0, -0.5], [0.09, 0.0], [0.0; 2], [0.0; 2], [0.0; 2], [0.0; 2]],
                },
                formant_shift: 0.97,
                formant_time: 0.0,
                filter: Filter::Lp,
                cutoff: 4200.0,
                lfo: [vib(4.8, 0.1, 0.35), wander(3.0, 0.08), none],
                gain: 0.42,
                ..note
            }],
            Inst::Solo => vec![Layer {
                wave: Wave::Voice,
                unison: 1,
                drift: 4.0,
                noise: 0.05,
                attack: 0.045,
                decay: 0.3,
                sustain: 0.9,
                release: 0.16,
                pitch_curve: crate::bank::PitchCurve {
                    n: 2,
                    pts: [[0.0, -0.9], [0.08, 0.0], [0.0; 2], [0.0; 2], [0.0; 2], [0.0; 2]],
                },
                formant_shift: 0.93,
                filter: Filter::Lp,
                cutoff: 5000.0,
                drive: 0.15,
                lfo: [vib(5.3, 0.26, 0.28), wander(9.0, 0.07), none],
                gain: 0.62,
                ..note
            }],
            Inst::Horn => vec![Layer {
                wave: Wave::Saw,
                unison: 2,
                detune: 7.0,
                drift: 2.0,
                attack: 0.05,
                decay: 0.3,
                sustain: 0.8,
                release: 0.16,
                pitch_curve: crate::bank::PitchCurve {
                    n: 2,
                    pts: [[0.0, -0.35], [0.06, 0.0], [0.0; 2], [0.0; 2], [0.0; 2], [0.0; 2]],
                },
                filter: Filter::Lp,
                cutoff: 700.0,
                env_cutoff: 1.8,
                key_track: 0.7,
                res: 0.15,
                formant: Some(Formant::Horn),
                formant_dry: 0.5,
                lfo: [vib(5.0, 0.07, 0.5), none, none],
                gain: 0.3,
                ..note
            }],
            Inst::Drone => vec![Layer {
                wave: Wave::Saw,
                unison: 3,
                detune: 11.0,
                drift: 3.0,
                wave2: Some(Wave::Square),
                ratio2: 0.5,
                gain2: 0.3,
                attack: 1.6,
                decay: 1.0,
                sustain: 1.0,
                release: 2.5,
                filter: Filter::Lp,
                cutoff: 520.0,
                res: 0.35,
                lfo: [
                    Lfo { to: LfoTo::Cutoff, rate: 0.06, depth: 0.9, shape: LfoShape::Sine, delay: 0.0, phase: None },
                    Lfo { to: LfoTo::Amp, rate: 0.13, depth: 0.2, shape: LfoShape::Random, delay: 0.0, phase: None },
                    none,
                ],
                gain: 0.26,
                ..note
            }],
            Inst::Pulse => vec![Layer {
                wave: Wave::Triangle,
                wave2: Some(Wave::Sine),
                ratio2: 0.5,
                gain2: 0.6,
                attack: 0.004,
                decay: 0.22,
                filter: Filter::Lp,
                cutoff: 420.0,
                env_cutoff: 1.0,
                gain: 0.55,
                ..note
            }],
            Inst::Tick => vec![
                Layer { wave: Wave::Noise, filter: Filter::Bp, cutoff: 3300.0, q: Some(5.0), decay: 0.018, gain: 1.1, ..note },
                Layer { wave: Wave::Sine, freq: 1750.0, decay: 0.012, gain: 0.12, ..note },
            ],
            Inst::Bell => vec![Layer {
                wave: Wave::Sine,
                fm_ratio: 3.5,
                fm_index: 2.4,
                fm_index_end: Some(0.15),
                fm_time: 1.2,
                attack: 0.001,
                decay: 2.6,
                gain: 0.22,
                ..note
            }],
            Inst::Timpani => vec![
                Layer {
                    wave: Wave::Sine,
                    freq_end: Some(0.985),
                    freq: 1.02,
                    glide: 0.12,
                    decay: 1.4,
                    drive: 0.12,
                    gain: 0.55,
                    ..note
                },
                Layer { wave: Wave::Sine, freq: 1.5, decay: 0.5, gain: 0.16, ..note },
                Layer {
                    wave: Wave::Brown,
                    filter: Filter::Lp,
                    cutoff: 500.0,
                    cutoff_end: Some(150.0),
                    decay: 0.12,
                    gain: 0.3,
                    ..note
                },
            ],
            Inst::Snare => vec![
                Layer { wave: Wave::Noise, filter: Filter::Bp, cutoff: 2700.0, q: Some(0.7), decay: 0.16, gain: 0.42, ..note },
                Layer { wave: Wave::Triangle, freq: 200.0, freq_end: Some(165.0), decay: 0.08, gain: 0.3, ..note },
            ],
            Inst::Shout => vec![Layer {
                wave: Wave::Voice,
                freq: 175.0,
                unison: 4,
                detune: 60.0,
                drift: 20.0,
                noise: 0.12,
                attack: 0.012,
                decay: 0.2,
                sustain: 0.0,
                hold: 0.04,
                release: 0.12,
                pitch_curve: crate::bank::PitchCurve {
                    n: 2,
                    pts: [[0.0, 1.0], [0.25, -3.0], [0.0; 2], [0.0; 2], [0.0; 2], [0.0; 2]],
                },
                formant: Some(Formant::Eh),
                formant_end: Some(Formant::Ee),
                formant_time: 0.15,
                drive: 0.3,
                gain: 1.3,
                ..note
            }],
            Inst::Hum => vec![Layer {
                wave: Wave::Voice,
                unison: 3,
                detune: 14.0,
                drift: 5.0,
                noise: 0.01,
                attack: 0.35,
                decay: 0.5,
                sustain: 0.9,
                release: 0.6,
                formant: Some(Formant::Mm),
                formant_dry: 0.05,
                filter: Filter::Lp,
                cutoff: 2000.0,
                lfo: [vib(4.5, 0.06, 0.6), wander(2.0, 0.05), none],
                gain: 0.5,
                ..note
            }],
        }
    }
}
