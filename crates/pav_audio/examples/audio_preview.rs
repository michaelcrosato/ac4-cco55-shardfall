//! Renders the game's music, cues and ambience to `out/audio/` (WAV + spectrogram PNG) and
//! prints loudness per bus, so the sound can be checked without ears.
//!
//! ```sh
//! cargo run --release -p pav_audio --example audio_preview -- [songs|states|cues|ambience|all] [seconds] [filter]
//! ```

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use pav_audio::instruments::Inst;
use pav_audio::mixer::Cmd;
use pav_audio::render::music_state;
use pav_audio::{AmbienceBank, Bank, Bus, Listener, MusicBank, Renderer, png_rgba, spectrogram, stats_of, write_wav};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn songs() -> Vec<(String, String)> {
    let dir = root().join("games/blacktap/shanties");
    let mut v: Vec<(String, String)> = std::fs::read_dir(&dir)
        .expect("shanties dir")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "abc"))
        .map(|p| (p.file_stem().unwrap().to_string_lossy().to_string(), std::fs::read_to_string(&p).unwrap()))
        .collect();
    v.sort();
    v
}

fn save(name: &str, samples: &[f32], secs: f32) {
    let out = root().join("out/audio");
    std::fs::create_dir_all(&out).unwrap();
    write_wav(&out.join(format!("{name}.wav")), samples, 48000).unwrap();
    let px: f32 = std::env::var("PX_PER_S").ok().and_then(|s| s.parse().ok()).unwrap_or(40.0);
    let w = ((secs * px) as usize).clamp(400, 2400);
    let h = 360;
    let img = spectrogram(samples, 48000.0, w, h);
    std::fs::write(out.join(format!("{name}.png")), png_rgba(w, h, &img)).unwrap();
}

fn report(name: &str, r: &Renderer, samples: &[f32], took: f32, secs: f32) {
    let s = r.stats();
    let (rms, peak) = stats_of(samples);
    let db = |x: f32| 20.0 * x.max(1e-9).log10();
    println!(
        "{name:<22} master rms {:>6.1} dB peak {:>6.1} dB | music {:>6.1} sfx {:>6.1} amb {:>6.1} voice {:>6.1} ui {:>6.1} | limiter {:.2} | {:.1}x realtime",
        db(rms),
        db(peak),
        s.rms_db(Bus::Music),
        s.rms_db(Bus::Sfx),
        s.rms_db(Bus::Ambience),
        s.rms_db(Bus::Voice),
        s.rms_db(Bus::Ui),
        s.limiter,
        secs / took.max(1e-6)
    );
    if s.stolen.iter().any(|&x| x > 0) || s.voices_peak[1] > 16 {
        println!("    voices peak {:?} stolen {:?}", s.voices_peak, s.stolen);
    }
}

/// Clicks: single-sample jumps far above the local derivative energy. Returns (time s, ratio).
fn clicks(s: &[f32]) -> Vec<(f32, f32)> {
    let n = s.len() / 2;
    let d: Vec<f32> = (1..n).map(|i| (s[i * 2] - s[(i - 1) * 2]).abs().max((s[i * 2 + 1] - s[(i - 1) * 2 + 1]).abs())).collect();
    let w = 240; // 5 ms each side
    let mut out = Vec::new();
    let mut acc: f64 = d[..w.min(d.len())].iter().map(|x| (x * x) as f64).sum();
    for i in 0..d.len() {
        if i + w < d.len() {
            acc += (d[i + w] * d[i + w]) as f64;
        }
        if i > w {
            acc -= (d[i - w - 1] * d[i - w - 1]) as f64;
        }
        let cnt = (i + w).min(d.len() - 1) - i.saturating_sub(w) + 1;
        let rms = (acc.max(0.0) / cnt as f64).sqrt() as f32;
        if d[i] > 0.02 && d[i] > rms * 9.0 {
            if out.last().is_none_or(|&(t, _): &(f32, f32)| (i as f32 / 48000.0) - t > 0.05) {
                out.push((i as f32 / 48000.0, d[i] / rms.max(1e-9)));
            }
        }
    }
    out
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let what = args.get(1).map(String::as_str).unwrap_or("all");
    let secs: f32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(30.0);
    let filter = args.get(3).cloned().unwrap_or_default();
    let data = |f: &str| std::fs::read_to_string(root().join("games/blacktap").join(f)).unwrap_or_default();
    let music = Arc::new(MusicBank::parse(&data("music.toml"), &songs()).expect("music"));
    let bank = Arc::new(Bank::parse(&data("sounds.toml")).expect("sounds"));
    let amb = Arc::new(AmbienceBank::parse(&data("ambience.toml")).expect("ambience"));
    let intensity: f32 = std::env::var("INTENSITY").ok().and_then(|s| s.parse().ok()).unwrap_or(0.6);
    let run = |name: &str, song: &str| {
        if !filter.is_empty() && !name.contains(&filter) {
            return;
        }
        let mut r = Renderer::new();
        r.mixer.load_music(music.clone());
        r.at(0.0, Cmd::Apply(Box::new(music_state(song, intensity)), Listener::default()));
        let t = Instant::now();
        let s = r.render(secs);
        report(name, &r, &s, t.elapsed().as_secs_f32(), secs);
        let c = clicks(&s);
        if !c.is_empty() {
            println!("    {} possible clicks: {:?}", c.len(), &c[..c.len().min(8)]);
        }
        save(name, &s, secs);
    };
    if what == "arr" {
        // The arrangement as text: per bar, the chord and what each part plays.
        let name = if filter.is_empty() { "drunken_sailor" } else { filter.as_str() };
        let a = music.song(name).expect("song");
        let names = ["C", "C#", "D", "Eb", "E", "F", "F#", "G", "Ab", "A", "Bb", "B"];
        let nn = |m: f32| format!("{}{}", names[(m.round() as i32).rem_euclid(12) as usize], (m.round() as i32) / 12 - 1);
        println!("{} ({}): {:.0} qpm, {} bars, {:.1} s", a.name, a.title, a.qpm, a.song.bars.len(), a.seconds());
        for (i, p) in a.parts.iter().enumerate() {
            println!("  part {i}: {} {:?} stem {}", p.inst.name(), p.role, p.stem);
        }
        for b in a.song.bars.iter().take(secs as usize) {
            let chord = a.song.chord_at(b.start + 1e-6);
            println!("bar {:>2} @{:>5.1} {:?}", b.number, b.start, chord.map(|c| (names[c.root as usize], c.quality)));
            for e in a.events.iter().filter(|e| e.start >= b.start - 0.05 && e.start < b.start + b.len - 0.05) {
                let notes: Vec<String> = e.notes[..e.n as usize].iter().map(|&m| nn(m)).collect();
                println!(
                    "    {:>5.2} {:<9} {:<14} len {:.2} vel {:.2} {:?}->{:?} {:?}",
                    e.start - b.start,
                    e.inst.name(),
                    notes.join(" "),
                    e.len,
                    e.vel,
                    e.vowel.0,
                    e.vowel.1,
                    e.cons
                );
            }
        }
        return;
    }
    if what == "parts" {
        // Loudness of each part alone in context (the same 30 s), to balance the mix.
        let name = if filter.is_empty() { "drunken_sailor" } else { filter.as_str() };
        let a = music.song(name).expect("song");
        for p in 0..a.parts.len() {
            let mut solo = (**a).clone();
            solo.events.retain(|e| e.part as usize == p);
            let mut m = (*music).clone();
            let idx = m.index[name];
            m.songs[idx] = Arc::new(solo);
            let mut r = Renderer::new();
            r.mixer.load_music(Arc::new(m));
            r.at(0.0, Cmd::Apply(Box::new(music_state(name, 1.0)), Listener::default()));
            let s = r.render(secs);
            let (rms, peak) = stats_of(&s);
            let db = |x: f32| 20.0 * x.max(1e-9).log10();
            println!(
                "part {p} {:<10} {:?}: rms {:>6.1} dB peak {:>6.1} dB",
                a.parts[p].inst.name(),
                a.parts[p].role,
                db(rms),
                db(peak)
            );
            save(&format!("part_{name}_{p}_{}", a.parts[p].inst.name()), &s, secs);
        }
        return;
    }
    if what == "scenes" {
        // Whole scenes: music + ambience + effects + dialog, as the game would mix them.
        use glam::Vec3;
        type Scene<'a> = (&'a str, &'a str, f32, Vec<(&'a str, f32)>, Vec<(f64, &'a str, Vec3)>);
        let p = |x: f32, z: f32| Vec3::new(x, 0.0, z);
        let mut scenes: Vec<Scene> = vec![
            (
                "voyage",
                "sailing",
                0.5,
                vec![("sea", 0.8), ("wind", 0.5), ("creak", 0.6), ("sails", 0.5), ("gulls", 0.4)],
                vec![(4.0, "ship_bell", p(0.0, -6.0)), (9.0, "bow_spray", p(0.0, -8.0))],
            ),
            ("naval_battle", "sea_combat", 0.9, vec![("sea", 0.9), ("wind", 0.6), ("creak", 0.5)], vec![]),
            ("town_stealth", "stealth", 0.4, vec![("town", 0.7), ("harbour", 0.5), ("gulls", 0.3)], vec![]),
            ("storm", "storm", 0.8, vec![("sea", 1.0), ("wind", 1.0), ("rain", 0.9), ("thunder", 0.8), ("creak", 0.8)], vec![]),
        ];
        // The battle: broadsides both ways.
        for i in 0..12 {
            let t = 2.0 + i as f64 * 1.3;
            scenes[1].4.push((t, if i % 3 == 0 { "cannon_fire_heavy" } else { "cannon_fire" }, p(-12.0, -20.0)));
            scenes[1].4.push((t + 0.6, "splash_big", p(30.0, -60.0)));
            if i % 2 == 1 {
                scenes[1].4.push((t + 0.4, "hull_hit", p(4.0, -3.0)));
                scenes[1].4.push((t + 0.45, "splinters", p(4.0, -3.0)));
            }
        }
        for i in 0..20 {
            scenes[2].4.push((1.0 + i as f64 * 0.42, "footstep_stone", p(0.0, -1.0)));
        }
        scenes[2].4.push((6.0, "guard_suspicious", p(6.0, -10.0)));
        scenes[2].4.push((12.0, "assassinate", p(0.0, -1.5)));
        scenes[2].4.push((12.5, "body_drop", p(0.0, -1.5)));
        for (name, song, intensity, amb_levels, cues) in scenes {
            let mut r = Renderer::new();
            r.mixer.load_bank(bank.clone());
            r.mixer.load_music(music.clone());
            r.mixer.load_ambience(amb.clone());
            let st = pav_core::play::AudioState {
                music: Some(pav_core::play::MusicCue { song: song.into(), intensity }),
                ambience: amb_levels.iter().map(|(n, l)| (n.to_string(), *l)).collect(),
                listener: None,
            };
            r.at(0.0, Cmd::Apply(Box::new(st), Listener::default()));
            for (t, c, pos) in cues {
                r.at(t, Cmd::Cue { name: c.into(), pos, gain: 1.0, pitch: 1.0, listener: Listener::default() });
            }
            for i in 0..16 {
                r.at(3.0 + i as f64 * 0.1, Cmd::Blip(if name == "storm" { "pip".into() } else { "bung".into() }));
            }
            let t = Instant::now();
            let s = r.render(secs.min(20.0));
            report(&format!("scene_{name}"), &r, &s, t.elapsed().as_secs_f32(), secs.min(20.0));
            let st = r.stats();
            println!(
                "    peaks dB: master {:.1} music {:.1} sfx {:.1} amb {:.1} voice {:.1}",
                st.peak_db(Bus::Master),
                st.peak_db(Bus::Music),
                st.peak_db(Bus::Sfx),
                st.peak_db(Bus::Ambience),
                st.peak_db(Bus::Voice)
            );
            save(&format!("scene_{name}"), &s, secs.min(20.0));
        }
        return;
    }
    if what == "voices" {
        // Each speaker says a "line" (14 blips), one after another.
        let mut r = Renderer::new();
        r.mixer.load_bank(bank.clone());
        let mut t = 0.0;
        for sp in bank.speakers.keys() {
            println!("{t:>6.2}s {sp}");
            for i in 0..14 {
                r.at(t + i as f64 * 0.1, Cmd::Blip(sp.clone()));
            }
            t += 2.0;
        }
        let s = r.render(t as f32);
        report("voices", &r, &s, 1.0, t as f32);
        save("voices", &s, t as f32);
        return;
    }
    if what == "insts" {
        // Each instrument alone on a phrase (drums on their pattern), to compare loudness and tone.
        let abc =
            "X:1\nT:lab\nM:4/4\nL:1/8\nQ:1/4=100\nK:C\n\"C\"C2 E2 G2 c2 | \"G\"B4 G4 | \"F\"A2 F2 \"G\"D2 G2 | \"C\"c8 |]\n";
        for inst in Inst::ALL {
            if !filter.is_empty() && !inst.name().contains(&filter) {
                continue;
            }
            let role = if inst.pitched() { "melody" } else { "drums" };
            let octave = match inst {
                Inst::Bass | Inst::Cello | Inst::Drone | Inst::Pulse | Inst::Timpani => -1,
                Inst::Whistle => 1,
                _ => 0,
            };
            let toml = format!(
                "[song.lab]\nabc = \"lab\"\nhumanize = 0\n[[song.lab.part]]\ninst = \"{}\"\nrole = \"{role}\"\noctave = {octave}\n",
                inst.name()
            );
            let m = Arc::new(MusicBank::parse(&toml, &[("lab".to_string(), abc.to_string())]).unwrap());
            let mut r = Renderer::new();
            r.mixer.load_music(m);
            r.at(0.0, Cmd::Apply(Box::new(music_state("lab", 1.0)), Listener::default()));
            let s = r.render(10.0);
            let (rms, peak) = stats_of(&s);
            let db = |x: f32| 20.0 * x.max(1e-9).log10();
            let c = pav_audio::analysis::spectral_centroid(&s, 48000.0);
            println!("{:<10} rms {:>6.1} dB  peak {:>6.1} dB  centroid {:>6.0} Hz", inst.name(), db(rms), db(peak), c);
            save(&format!("inst_{}", inst.name()), &s, 10.0);
        }
        return;
    }
    if what == "songs" || what == "all" {
        for s in &music.songs {
            run(&format!("song_{}", s.name), &s.name);
        }
    }
    if what == "states" || what == "all" {
        for s in music.states.keys() {
            run(&format!("state_{s}"), s);
        }
    }
    if what == "cues" || what == "all" {
        let names: Vec<&String> = bank.cues.keys().filter(|n| n.contains(&filter)).collect();
        let mut r = Renderer::new();
        r.mixer.load_bank(bank.clone());
        let mut t = 0.0;
        for n in &names {
            let c = bank.cue(n).unwrap();
            r.at(
                t,
                Cmd::Cue {
                    name: n.to_string(),
                    pos: glam::Vec3::new(0.0, 0.0, -3.0),
                    gain: 1.0,
                    pitch: 1.0,
                    listener: Listener::default(),
                },
            );
            println!("{t:>7.2}s {n}");
            t += (c.length() as f64).clamp(0.4, 4.0) + 0.3;
        }
        let total = t as f32 + 1.0;
        let tm = Instant::now();
        let s = r.render(total);
        report("cues", &r, &s, tm.elapsed().as_secs_f32(), total);
        save("cues", &s, total);
    }
    if what == "ambience" || what == "all" {
        for (name, _) in &amb.layers {
            if !filter.is_empty() && !name.contains(&filter) {
                continue;
            }
            let mut r = Renderer::new();
            r.mixer.load_bank(bank.clone());
            r.mixer.load_ambience(amb.clone());
            let st = pav_core::play::AudioState { ambience: vec![(name.clone(), 0.8)], ..Default::default() };
            r.at(0.0, Cmd::Apply(Box::new(st), Listener::default()));
            let t = Instant::now();
            let s = r.render(secs.min(20.0));
            report(&format!("amb_{name}"), &r, &s, t.elapsed().as_secs_f32(), secs.min(20.0));
            save(&format!("amb_{name}"), &s, secs.min(20.0));
        }
    }
}
