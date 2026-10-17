//! DSP sanity: tuning (oscillators and the plucked string), formants where they belong, noise
//! colours of similar loudness, stable filters and reverb, envelopes on time.

use pav_audio::bank::{Filter, Layer, Wave};
use pav_audio::dsp::{Formant, Noise, Reverb};
use pav_audio::mixer::Pool;
use pav_audio::voice::Spawn;
use pav_audio::{Bus, Mixer, stats_of};

const SR: f32 = 48000.0;

/// Renders one layer (dry: no reverb or echo) for `secs`, mono (left channel).
fn render(l: Layer, pitch: f32, secs: f32, vowel: Option<(Formant, Formant)>) -> Vec<f32> {
    let mut m = Mixer::new(SR);
    let mut s = Spawn::new(l);
    s.pitch = pitch;
    s.vowel = vowel;
    s.held = true;
    s.bus = Bus::Sfx;
    m.spawn(&s, Pool::Sfx);
    let out = m.render_seconds(secs);
    let st = m.stats();
    assert_eq!(st.bad_voices, 0, "the voice went non-finite");
    assert!(st.peak(Bus::Sfx) < 8.0, "runaway level {}", st.peak(Bus::Sfx));
    out.chunks(2).map(|f| f[0]).collect()
}

/// Fundamental by autocorrelation (Hz), searching 50..1200 Hz.
fn pitch_of(x: &[f32]) -> f32 {
    let x = &x[x.len() / 4..];
    let n = x.len().min(16384);
    let x = &x[..n];
    let (lo, hi) = ((SR / 1200.0) as usize, (SR / 50.0) as usize);
    let ac = |lag: usize| -> f64 { (0..n - lag).map(|i| (x[i] * x[i + lag]) as f64).sum::<f64>() / (n - lag) as f64 };
    let vals: Vec<f64> = (lo..=hi + 1).map(ac).collect();
    let best = vals.iter().copied().fold(f64::MIN, f64::max);
    // The first lag that comes within 90% of the best peak (avoids octave errors).
    let mut k = 1;
    while k + 1 < vals.len() && !(vals[k] >= vals[k - 1] && vals[k] >= vals[k + 1] && vals[k] > 0.9 * best) {
        k += 1;
    }
    let (a, b, c) = (vals[k - 1], vals[k], vals[k + 1]);
    let off = 0.5 * (a - c) / (a - 2.0 * b + c);
    SR / ((lo + k) as f64 + off) as f32
}

fn cents(a: f32, b: f32) -> f32 {
    1200.0 * (a / b).log2()
}

#[test]
fn oscillators_are_in_tune() {
    for (wave, f) in [(Wave::Sine, 220.0), (Wave::Saw, 110.0), (Wave::Square, 330.0), (Wave::Triangle, 440.0)] {
        let l = Layer { wave, freq: f, attack: 0.001, sustain: 1.0, gain: 0.5, ..Layer::default() };
        let p = pitch_of(&render(l, 1.0, 0.6, None));
        assert!(cents(p, f).abs() < 5.0, "{wave:?} at {f}: {p} Hz");
    }
}

#[test]
fn plucked_strings_are_in_tune() {
    for f in [82.4, 146.8, 220.0, 392.0, 659.3] {
        let l =
            Layer { wave: Wave::Pluck, freq: 1.0, pluck_decay: 3.0, attack: 0.0, sustain: 1.0, gain: 0.5, ..Layer::default() };
        let p = pitch_of(&render(l, f, 0.6, None));
        assert!(cents(p, f).abs() < 8.0, "pluck at {f}: {p} Hz ({:.1} cents)", cents(p, f));
    }
}

/// Energy-weighted mean frequency within a band.
fn band_energy(x: &[f32], lo: f32, hi: f32) -> f64 {
    // Goertzel at 25 Hz steps.
    let n = x.len().min(32768);
    let x = &x[x.len() - n..];
    let mut e = 0.0;
    let mut f = lo;
    while f <= hi {
        let w = 2.0 * std::f64::consts::PI * f as f64 / SR as f64;
        let c = 2.0 * w.cos();
        let (mut s1, mut s2) = (0.0f64, 0.0f64);
        for &v in x {
            let s0 = v as f64 + c * s1 - s2;
            s2 = s1;
            s1 = s0;
        }
        e += s1 * s1 + s2 * s2 - c * s1 * s2;
        f += 25.0;
    }
    e
}

#[test]
fn vowels_put_energy_where_their_formants_are() {
    // A low voice on "ah" has more energy around F1 (650 Hz) than "ee" does; "ee" has more
    // up at F2 (1900-2300 Hz).
    let voice = Layer { wave: Wave::Voice, freq: 1.0, attack: 0.01, sustain: 1.0, gain: 0.5, ..Layer::default() };
    let ah = render(voice, 110.0, 0.8, Some((Formant::Ah, Formant::Ah)));
    let ee = render(voice, 110.0, 0.8, Some((Formant::Ee, Formant::Ee)));
    let (ah_f1, ee_f1) = (band_energy(&ah, 550.0, 750.0), band_energy(&ee, 550.0, 750.0));
    let (ah_f2, ee_f2) = (band_energy(&ah, 1800.0, 2300.0), band_energy(&ee, 1800.0, 2300.0));
    assert!(ah_f1 > ee_f1 * 3.0, "ah F1 {ah_f1:e} vs ee {ee_f1:e}");
    assert!(ee_f2 / ee_f1 > ah_f2 / ah_f1 * 3.0, "ee is brighter at F2");
}

#[test]
fn noise_colours_are_similarly_loud_and_coloured() {
    let mut n = Noise::new(12345);
    let rms = |f: &mut dyn FnMut() -> f32| {
        let v: Vec<f32> = (0..96000).map(|_| f()).collect();
        stats_of(&v).0
    };
    let w = rms(&mut || n.white());
    let p = rms(&mut || n.pink());
    let b = rms(&mut || n.brown());
    for (name, r) in [("white", w), ("pink", p), ("brown", b)] {
        assert!(r > 0.12 && r < 0.6, "{name} noise rms {r}");
    }
    // Brown is darker than white: compare energy above 4 kHz.
    let layer = |wave| Layer { wave, attack: 0.001, sustain: 1.0, gain: 0.5, ..Layer::default() };
    let white = render(layer(Wave::Noise), 1.0, 0.8, None);
    let brown = render(layer(Wave::Brown), 1.0, 0.8, None);
    let hi = |x: &[f32]| band_energy(x, 4000.0, 6000.0) / band_energy(x, 100.0, 300.0);
    assert!(hi(&white) > hi(&brown) * 100.0);
}

#[test]
fn resonant_filters_stay_stable_under_sweeps() {
    for filter in [Filter::Lp, Filter::Bp, Filter::Hp, Filter::Notch] {
        let l = Layer {
            wave: Wave::Saw,
            freq: 55.0,
            filter,
            cutoff: 20.0,
            cutoff_end: Some(22000.0),
            cutoff_time: 0.5,
            q: Some(30.0),
            attack: 0.001,
            sustain: 1.0,
            gain: 0.3,
            lfo: [
                pav_audio::dsp::Lfo { to: pav_audio::dsp::LfoTo::Cutoff, rate: 40.0, depth: 4.0, ..Default::default() },
                Default::default(),
                Default::default(),
            ],
            ..Layer::default()
        };
        let x = render(l, 1.0, 1.0, None);
        let (r, p) = stats_of(&x);
        assert!(r.is_finite() && p < 1.0, "{filter:?}: rms {r} peak {p}");
    }
}

#[test]
fn reverb_tail_decays() {
    let mut rv = Reverb::new(SR);
    let mut tail = Vec::new();
    for i in 0..(SR as usize * 6) {
        let x = if i < 480 { 1.0 } else { 0.0 };
        let (l, r) = rv.tick(x, x);
        tail.push(l.abs().max(r.abs()));
    }
    let early = tail[(SR * 0.1) as usize..(SR * 0.5) as usize].iter().fold(0.0f32, |a, &b| a.max(b));
    let late = tail[(SR * 5.0) as usize..].iter().fold(0.0f32, |a, &b| a.max(b));
    assert!(early > 1e-3, "the reverb answers: {early}");
    assert!(late < early * 1e-3, "and dies away: {late} vs {early}");
}

#[test]
fn one_shot_envelopes_end_on_time() {
    // attack + decay + release, exponential: silent shortly after.
    let l = Layer { wave: Wave::Sine, freq: 300.0, attack: 0.01, decay: 0.3, release: 0.1, gain: 0.5, ..Layer::default() };
    let mut m = Mixer::new(SR);
    let s = Spawn::new(l);
    m.spawn(&s, Pool::Sfx);
    let out = m.render_seconds(0.2);
    assert!(stats_of(&out).1 > 0.05);
    m.render_seconds(0.5);
    assert_eq!(m.active(), 0, "the voice has finished");
    assert!((l.length() - (0.01 + 0.3 + 0.126)).abs() < 1e-4);
}
