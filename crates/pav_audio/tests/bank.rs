//! The game's data files parse, every cue the game needs exists, and every tune is well formed.

mod common;

use pav_audio::music::Role;
use pav_audio::{AmbienceBank, Bank, MusicBank, abc};

const CUES: &[&str] = &[
    "cannon_fire",
    "cannon_fire_heavy",
    "chain_fire",
    "swivel_fire",
    "mortar_launch",
    "mortar_whistle",
    "splash_big",
    "splash_small",
    "hull_hit",
    "splinters",
    "explosion",
    "keg_drop",
    "sail_rip",
    "ship_creak",
    "sail_flap",
    "ship_bell",
    "brace",
    "grapple",
    "ram_hit",
    "sink",
    "repair",
    "fire_crackle",
    "bow_spray",
    "sword_swing",
    "sword_clash",
    "sword_hit",
    "counter",
    "assassinate",
    "pistol_fire",
    "pistol_reload",
    "dart_throw",
    "smoke_keg",
    "whistle",
    "footstep_stone",
    "footstep_sand",
    "footstep_wood",
    "land_hard",
    "land_roll",
    "climb_grab",
    "leap_whoosh",
    "hay_land",
    "water_dive",
    "swim_stroke",
    "body_drop",
    "poster_tear",
    "pickpocket",
    "guard_alert",
    "guard_suspicious",
    "detection_stinger",
    "alarm_bell",
    "crowd_scream",
    "pickup_chest",
    "pickup_sheet",
    "pickup_bottle",
    "pickup_tap",
    "pickup_flotsam",
    "survey_swirl",
    "gull_sight_on",
    "gull_sight_off",
    "objective_new",
    "objective_done",
    "round_complete",
    "fort_captured",
    "spilled",
    "thunder",
    "lightning_crack",
    "rain_burst",
    "parrot_squawk",
    "ui_move",
    "ui_confirm",
    "ui_back",
    "ui_toggle",
    "ui_slider",
    "ui_error",
    "ui_start",
];

#[test]
fn sound_bank_parses_with_every_cue() {
    let bank = Bank::parse(common::SOUNDS).expect("sounds.toml");
    let missing: Vec<&&str> = CUES.iter().filter(|c| !bank.cues.contains_key(**c)).collect();
    assert!(missing.is_empty(), "missing cues: {missing:?}");
    for (name, cue) in &bank.cues {
        assert!(cue.length() > 0.0 && cue.length() < 12.0, "{name}: length {}", cue.length());
        assert!(!cue.about.is_empty(), "{name}: say what it is (about = ...)");
    }
    for ui in CUES.iter().filter(|c| c.starts_with("ui_")) {
        assert_eq!(bank.cues[*ui].bus, pav_audio::Bus::Ui, "{ui} belongs on the ui bus");
    }
    // Haptics for the big moments.
    for c in ["cannon_fire", "hull_hit", "sword_clash", "assassinate", "land_hard", "grapple", "thunder"] {
        assert!(bank.rumble(c).is_some(), "{c} should rumble");
    }
}

#[test]
fn bank_rejects_typos_and_bad_values() {
    let bad_field = "[cue.x]\n[[cue.x.layer]]\nwave = \"sine\"\nfreqq = 100\n";
    assert!(Bank::parse(bad_field).is_err(), "unknown fields are errors");
    let bad_wave = "[cue.x]\n[[cue.x.layer]]\nwave = \"kazoo\"\n";
    assert!(Bank::parse(bad_wave).is_err());
    let bad_time = "[cue.x]\n[[cue.x.layer]]\ndecay = -1\n";
    assert!(Bank::parse(bad_time).is_err());
    let no_layers = "[cue.x]\ngain = 1\n";
    assert!(Bank::parse(no_layers).is_err());
    let ok = "[cue.x]\n[[cue.x.layer]]\nwave = \"pink\"\nfilter = \"bp\"\ncutoff = 900\nlfo = [{ to = \"amp\", rate = 3, depth = 0.5, shape = \"random\" }]\n";
    let b = Bank::parse(ok).unwrap();
    assert_eq!(b.cues["x"].layer[0].lfo[0].rate, 3.0);
}

#[test]
fn ambience_parses_and_its_cues_exist() {
    let amb = AmbienceBank::parse(common::AMBIENCE).expect("ambience.toml");
    let bank = Bank::parse(common::SOUNDS).unwrap();
    for l in ["sea", "wind", "rain", "thunder", "creak", "sails", "gulls", "harbour", "jungle", "town", "fire"] {
        assert!(amb.index(l).is_some(), "ambience layer {l} missing");
    }
    for c in amb.cues() {
        assert!(bank.cues.contains_key(&c), "ambience uses cue {c}, not in sounds.toml");
    }
    for (name, l) in &amb.layers {
        for b in &l.bed {
            assert!(b.sustain > 0.0, "{name}: a bed needs sustain > 0 to hold");
        }
    }
}

#[test]
fn music_parses_with_every_state_and_song() {
    let songs = common::songs();
    let music = MusicBank::parse(common::MUSIC, &songs).expect("music.toml");
    for s in [
        "title",
        "sailing",
        "attract",
        "sea_combat",
        "fort_siege",
        "explore",
        "stealth",
        "combat",
        "storm",
        "last_words",
        "results",
    ] {
        assert!(music.states.contains_key(s), "music state {s} missing");
    }
    for s in ["detection", "objective", "fort_captured"] {
        assert!(music.stingers.contains_key(s), "stinger {s} missing");
    }
    let shanties =
        ["drunken_sailor", "leave_her_johnny", "spanish_ladies", "haul_away_joe", "blow_the_man_down", "south_australia"];
    for s in shanties {
        let a = music.song(s).unwrap_or_else(|| panic!("shanty {s} missing"));
        assert!(!a.song.chords.is_empty(), "{s} has chord symbols");
        assert!(a.parts.iter().any(|p| p.role == Role::Choir), "{s}: the crew sings it");
        assert!(a.seconds() > 30.0, "{s} plays for {:.0} s", a.seconds());
    }
    // The sailing rotation is the shanties.
    let sailing = &music.states["sailing"];
    assert!(sailing.songs.len() >= 6);
    // The title theme is original, stately and long enough.
    let t = music.song("the_black_tap").unwrap();
    assert!(t.song.bars.len() >= 16 && t.song.key.mode.is_minor());
}

#[test]
fn every_tune_is_well_formed() {
    for (name, text) in common::songs() {
        let s = abc::parse(&text).unwrap_or_else(|e| panic!("{name}.abc: {e:#}"));
        assert!(!s.notes.is_empty(), "{name}: no notes");
        let full = s.bar_beats();
        // Every bar is full except a pickup (first) and the last bar.
        let n = s.bars.len();
        for (i, b) in s.bars.iter().enumerate() {
            if i == 0 || i == n - 1 {
                assert!(b.len <= full + 1e-6, "{name}: bar {} is {} beats (meter has {full})", b.number, b.len);
            } else {
                assert!((b.len - full).abs() < 1e-6, "{name}: bar {} is {} beats, not {full}", b.number, b.len);
            }
        }
        // A pickup and a short last bar add up to one bar (so repeats stay on the beat).
        if s.bars.len() > 2 && s.bars[0].len < full - 1e-6 {
            let last = s.bars.last().unwrap().len;
            assert!((s.bars[0].len + last - full).abs() < 1e-6, "{name}: pickup {} + last bar {last} != {full}", s.bars[0].len);
        }
        // Singable, playable ranges (MIDI): melody between G2 and A6.
        let lo = s.notes.iter().map(|n| n.midi).min().unwrap();
        let hi = s.notes.iter().map(|n| n.midi).max().unwrap();
        assert!(lo >= 40 && hi <= 93, "{name}: range {lo}..{hi}");
        // Lyrics, where written, line up: no syllables left over in any w: line is checked
        // by the parser silently, so check the songs with lyrics have most notes sung.
        if s.lyrics.iter().any(|l| !l.is_empty()) {
            let sung = s.notes.iter().filter(|n| n.syllable.is_some()).count();
            assert!(sung * 10 >= s.notes.len() * 9, "{name}: only {sung} of {} notes have a syllable", s.notes.len());
        }
    }
}
