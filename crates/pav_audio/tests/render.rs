//! Offline renders of the real game data: every song and state, the cues and the ambience
//! are audible, finite and under 0 dBFS; rendering is deterministic; the sequencer is on time.

mod common;

use std::sync::Arc;

use glam::Vec3;
use pav_audio::mixer::Cmd;
use pav_audio::render::music_state;
use pav_audio::{Bus, Listener, MusicBank, Renderer, render_ambience, render_cues, render_song, stats_of};
use pav_core::play::AudioState;

fn check(name: &str, s: &[f32], floor_db: f32) -> (f32, f32) {
    let (rms, peak) = stats_of(s);
    assert!(rms.is_finite() && peak.is_finite(), "{name}: NaN or infinity in the output");
    let rms_db = 20.0 * rms.max(1e-9).log10();
    assert!(rms_db > floor_db, "{name}: too quiet ({rms_db:.1} dB RMS)");
    assert!(peak < 1.0, "{name}: peak {peak} reaches 0 dBFS");
    (rms_db, 20.0 * peak.log10())
}

#[test]
fn every_song_renders_30s() {
    let music = common::music();
    let names: Vec<String> = music.songs.iter().map(|s| s.name.clone()).collect();
    std::thread::scope(|sc| {
        for chunk in names.chunks(names.len().div_ceil(3)) {
            let music = music.clone();
            sc.spawn(move || {
                for n in chunk {
                    let s = render_song(&music, n, 0.6, 30.0);
                    let (r, p) = check(n, &s, -40.0);
                    println!("song {n:<20} rms {r:6.1} dB  peak {p:6.1} dB");
                }
            });
        }
    });
}

#[test]
fn every_state_renders_30s_at_low_and_high_intensity() {
    let music = common::music();
    let names: Vec<String> = music.states.keys().cloned().collect();
    std::thread::scope(|sc| {
        for chunk in names.chunks(names.len().div_ceil(3)) {
            let music = music.clone();
            sc.spawn(move || {
                for n in chunk {
                    for i in [0.0, 1.0] {
                        let s = render_song(&music, n, i, 30.0);
                        let (r, p) = check(&format!("state {n} @ {i}"), &s, -40.0);
                        println!("state {n:<12} @ {i}  rms {r:6.1} dB  peak {p:6.1} dB");
                    }
                }
            });
        }
    });
}

#[test]
fn every_cue_is_audible() {
    let bank = common::bank();
    for name in bank.cues.keys() {
        let s = render_cues(&bank, &[(0.0, name.as_str())], 1.5);
        check(&format!("cue {name}"), &s, -60.0);
    }
}

#[test]
fn every_ambience_layer_is_audible() {
    let amb = common::ambience();
    let bank = common::bank();
    for (name, _) in &amb.layers {
        let s = render_ambience(&amb, &bank, &[(name.as_str(), 0.9)], 30.0);
        let (r, p) = check(&format!("ambience {name}"), &s, -75.0);
        println!("ambience {name:<10} rms {r:6.1} dB  peak {p:6.1} dB");
    }
    // A sea-voyage mix.
    let mix = [("sea", 0.8), ("wind", 0.5), ("creak", 0.6), ("sails", 0.5), ("gulls", 0.4)];
    let s = render_ambience(&amb, &bank, &mix, 20.0);
    check("voyage mix", &s, -40.0);
}

#[test]
fn rendering_is_deterministic() {
    let music = common::music();
    let a = render_song(&music, "sailing", 0.7, 8.0);
    let b = render_song(&music, "sailing", 0.7, 8.0);
    assert!(a == b, "two renders of the same state differ");
    let amb = common::ambience();
    let bank = common::bank();
    let lv = [("sea", 0.8), ("gulls", 1.0), ("creak", 1.0), ("rain", 0.6)];
    assert!(render_ambience(&amb, &bank, &lv, 10.0) == render_ambience(&amb, &bank, &lv, 10.0));
    let cues = [(0.0, "cannon_fire"), (0.3, "splash_big"), (0.5, "ship_creak"), (0.9, "sword_clash")];
    assert!(render_cues(&bank, &cues, 2.0) == render_cues(&bank, &cues, 2.0));
}

/// The first sample index (frame) whose level exceeds `thr`.
fn onset(s: &[f32], thr: f32) -> Option<usize> {
    s.chunks(2).position(|f| f[0].abs().max(f[1].abs()) > thr)
}

#[test]
fn sequencer_is_sample_accurate() {
    // One note on beat 4 at 120 bpm: 2 s = sample 96000 at 48 kHz.
    let abc = "X:1\nT:t\nM:4/4\nL:1/4\nQ:1/4=120\nK:C\nz4 | c4 |]\n";
    let toml = "[song.t]\nhumanize = 0\n[[song.t.part]]\ninst = \"clap\"\nrole = \"melody\"\n";
    let music = Arc::new(MusicBank::parse(toml, &[("t".into(), abc.into())]).unwrap());
    let song = music.song("t").unwrap();
    assert_eq!(song.events.len(), 1);
    assert!((song.events[0].start - 4.0).abs() < 1e-9);
    // Drums play once per note; clap starts with noise, so its first sample is non-zero.
    let s = render_song(&music, "t", 1.0, 3.0);
    let at = onset(&s, 1e-4).expect("the note plays");
    assert!((96000..96000 + 128).contains(&at), "note starts at sample {at}, expected 96000");
    // The same with an odd buffer size in the live path (Mixer::render on 300-frame chunks).
    let mut r = Renderer::new();
    r.mixer.load_music(music.clone());
    r.mixer.handle(Cmd::Apply(Box::new(music_state("t", 1.0)), Listener::default()));
    let mut out = vec![0.0f32; 48000 * 3 * 2];
    for c in out.chunks_mut(600) {
        r.mixer.render(c);
    }
    let at2 = onset(&out, 1e-4).unwrap();
    assert!((96000..96000 + 128).contains(&at2), "with 300-frame buffers: {at2}");
}

#[test]
fn music_states_crossfade_and_stems_follow_intensity() {
    let music = common::music();
    let mut r = Renderer::new();
    r.mixer.load_music(music.clone());
    r.at(0.0, Cmd::Apply(Box::new(music_state("stealth", 0.0)), Listener::default()));
    let quiet = r.render(10.0);
    r.at(10.0, Cmd::Apply(Box::new(music_state("stealth", 1.0)), Listener::default()));
    let tense = r.render(10.0);
    let (q, _) = stats_of(&quiet[48000 * 2 * 4..]);
    let (t, _) = stats_of(&tense[48000 * 2 * 4..]);
    assert!(t > q * 1.4, "stealth at full awareness ({t}) should be louder than calm ({q})");
    // Switching state: the old song fades, the new one starts; nothing goes silent or clips.
    r.at(20.0, Cmd::Apply(Box::new(music_state("detected", 1.0)), Listener::default()));
    let x = r.render(6.0);
    check("crossfade", &x, -40.0);
    assert_eq!(r.mixer.now_playing().map(|n| n.0), Some("detected".to_string()));
    // Music off: fades to silence.
    r.at(26.0, Cmd::Apply(Box::default(), Listener::default()));
    let off = r.render(8.0);
    let (tail, _) = stats_of(&off[48000 * 2 * 7..]);
    assert!(tail < 1e-3, "music should fade out, rms {tail}");
}

#[test]
fn positional_cues_pan_and_fade_with_distance() {
    let bank = common::bank();
    let render = |pos: Vec3| {
        let mut r = Renderer::new();
        r.mixer.load_bank(bank.clone());
        r.at(0.0, Cmd::Cue { name: "sword_clash".into(), pos, gain: 1.0, pitch: 1.0, listener: Listener::default() });
        r.render(0.5)
    };
    let side = |s: &[f32]| {
        let (mut l, mut r) = (0.0f64, 0.0f64);
        for f in s.chunks(2) {
            l += (f[0] * f[0]) as f64;
            r += (f[1] * f[1]) as f64;
        }
        (l.sqrt(), r.sqrt())
    };
    let (l, r) = side(&render(Vec3::new(8.0, 0.0, 0.0)));
    assert!(r > l * 2.0, "a sound to the right is louder on the right: {l} {r}");
    let near = stats_of(&render(Vec3::new(0.0, 0.0, -2.0))).0;
    let far = stats_of(&render(Vec3::new(0.0, 0.0, -80.0))).0;
    assert!(near > far * 3.0, "distance attenuates: near {near} far {far}");
}

#[test]
fn buses_mix_and_report() {
    let bank = common::bank();
    let music = common::music();
    let amb = common::ambience();
    let mut r = Renderer::new();
    r.mixer.load_bank(bank);
    r.mixer.load_music(music);
    r.mixer.load_ambience(amb);
    let st = AudioState {
        music: Some(pav_core::play::MusicCue { song: "sailing".into(), intensity: 0.6 }),
        ambience: vec![("sea".into(), 0.7), ("wind".into(), 0.4)],
        listener: None,
    };
    r.at(0.0, Cmd::Apply(Box::new(st), Listener::default()));
    for i in 0..10 {
        r.at(
            1.0 + i as f64 * 0.5,
            Cmd::Cue {
                name: "cannon_fire".into(),
                pos: Vec3::new(-10.0, 0.0, -30.0),
                gain: 1.0,
                pitch: 1.0,
                listener: Listener::default(),
            },
        );
    }
    r.at(2.0, Cmd::Blip("bung".into()));
    r.at(2.1, Cmd::Blip("bung".into()));
    r.at(3.0, Cmd::Ui("ui_confirm".into()));
    let s = r.render(8.0);
    check("full mix", &s, -35.0);
    let st = r.stats();
    for b in [Bus::Music, Bus::Sfx, Bus::Ambience, Bus::Voice, Bus::Ui] {
        assert!(st.rms(b) > 0.0, "{} bus is silent", b.name());
    }
    assert!(st.peak(Bus::Master) < 1.0);
    // Muting a bus silences it (after a few ms of ramp).
    r.mixer.set_bus(Bus::Music, 0.0);
    r.render(0.1);
    r.mixer.reset_stats();
    r.render(3.0);
    assert!(r.stats().rms(Bus::Music) < 1e-3);
}

#[test]
fn legacy_events_still_play() {
    let ev = pav_core::SimEvent::Explosion { pos: Vec3::ZERO, radius: 1.35 };
    let s = pav_audio::render_events(&[(0.0, ev)], 1.0, 48000.0, &Listener::default());
    check("legacy explosion", &s, -40.0);
    // A Cue event goes through the bank.
    let bank = common::bank();
    let mut r = Renderer::new();
    r.mixer.load_bank(bank);
    let ev = pav_core::SimEvent::cue("pistol_fire", Vec3::new(0.0, 0.0, -3.0));
    r.mixer.play_event(&ev, &Listener::default(), 1.0);
    let s = r.render(1.0);
    check("cue event", &s, -40.0);
    // Unknown cues are ignored.
    let ev = pav_core::SimEvent::cue("no_such_cue", Vec3::ZERO);
    r.mixer.play_event(&ev, &Listener::default(), 1.0);
    r.mixer.play_event(&ev, &Listener::default(), 1.0);
}

#[test]
fn voices_and_ui_sounds() {
    let bank = common::bank();
    for sp in ["bung", "pip", "cask", "drye", "sobers", "molasses", "guard", "crew", "crier", "ballast"] {
        assert!(bank.speakers.contains_key(sp), "speaker {sp} missing");
        let mut r = Renderer::new();
        r.mixer.load_bank(bank.clone());
        for i in 0..6 {
            r.at(i as f64 * 0.11, Cmd::Blip(sp.into()));
        }
        let s = r.render(1.2);
        check(&format!("voice {sp}"), &s, -50.0);
        assert!(r.stats().rms(Bus::Voice) > 0.0);
    }
    for ui in ["ui_move", "ui_confirm", "ui_back", "ui_toggle", "ui_slider", "ui_error", "ui_start"] {
        let mut r = Renderer::new();
        r.mixer.load_bank(bank.clone());
        r.at(0.0, Cmd::Ui(ui.into()));
        let s = r.render(1.0);
        check(ui, &s, -70.0);
        assert!(r.stats().rms(Bus::Ui) > 0.0, "{ui} is on the ui bus");
    }
}

#[test]
fn held_loops_start_and_stop() {
    let bank = common::bank();
    let mut r = Renderer::new();
    r.mixer.load_bank(bank);
    r.at(0.0, Cmd::StartLoop { handle: 7, cue: "fire_crackle".into(), pos: Some(Vec3::new(0.0, 0.0, -3.0)), gain: 1.0 });
    let a = r.render(4.0);
    let (on, _) = stats_of(&a[48000 * 2 * 2..]);
    assert!(on > 1e-3, "the loop holds after its decay time: {on}");
    r.at(4.0, Cmd::StopLoop(7));
    let b = r.render(4.0);
    let (off, _) = stats_of(&b[48000 * 2 * 3..]);
    assert!(off < 1e-4, "stopped: {off}");
}
