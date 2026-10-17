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
    let w = ((secs * 40.0) as usize).clamp(400, 2400);
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
        save(name, &s, secs);
    };
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
