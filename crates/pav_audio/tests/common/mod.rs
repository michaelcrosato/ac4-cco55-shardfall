//! Loads the game's real audio data for the tests.

#![allow(dead_code)]

use std::sync::Arc;

use pav_audio::{AmbienceBank, Bank, MusicBank};

pub const SOUNDS: &str = include_str!("../../../../games/blacktap/sounds.toml");
pub const MUSIC: &str = include_str!("../../../../games/blacktap/music.toml");
pub const AMBIENCE: &str = include_str!("../../../../games/blacktap/ambience.toml");

/// Every `.abc` in games/blacktap/shanties, as (file stem, text).
pub fn songs() -> Vec<(String, String)> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../games/blacktap/shanties");
    let mut v: Vec<(String, String)> = std::fs::read_dir(&dir)
        .expect("games/blacktap/shanties")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "abc"))
        .map(|p| (p.file_stem().unwrap().to_string_lossy().to_string(), std::fs::read_to_string(&p).unwrap()))
        .collect();
    v.sort();
    v
}

pub fn bank() -> Arc<Bank> {
    Arc::new(Bank::parse(SOUNDS).expect("sounds.toml"))
}

pub fn music() -> Arc<MusicBank> {
    Arc::new(MusicBank::parse(MUSIC, &songs()).expect("music.toml"))
}

pub fn ambience() -> Arc<AmbienceBank> {
    Arc::new(AmbienceBank::parse(AMBIENCE).expect("ambience.toml"))
}

/// (RMS, peak) of a section of interleaved stereo, in seconds.
pub fn window(s: &[f32], from: f32, to: f32) -> (f32, f32) {
    let a = ((from * 48000.0) as usize * 2).min(s.len());
    let b = ((to * 48000.0) as usize * 2).min(s.len());
    pav_audio::stats_of(&s[a..b])
}
