//! Assassin's Keg: Black Tap `[AssassinsKegBlackTap-W9]`, the game. Call [`register`] once at
//! startup (the app, the `pav` binary, tests); then `load scene=blacktap` works everywhere.

use std::borrow::Cow;

pub mod content;
pub mod game;

mod embedded {
    include!(concat!(env!("OUT_DIR"), "/blacktap_data.rs"));
}

/// A data file from `games/blacktap/` by relative path ("missions/r1.toml"): the file on
/// disk when running from the repository's root (edit, then `load`), else the copy built in.
pub fn data(path: &str) -> Option<Cow<'static, str>> {
    #[cfg(not(target_arch = "wasm32"))]
    if let Ok(text) = std::fs::read_to_string(format!("games/blacktap/{path}")) {
        return Some(text.into());
    }
    embedded::DATA.iter().find(|(k, _)| *k == path).map(|(_, v)| Cow::Borrowed(*v))
}

/// Every embedded data file's path.
pub fn data_files() -> impl Iterator<Item = &'static str> {
    embedded::DATA.iter().map(|(k, _)| *k)
}

#[derive(serde::Deserialize, Default)]
struct BindingsFile {
    #[serde(default)]
    action: Vec<ActionName>,
}

#[derive(serde::Deserialize)]
struct ActionName {
    name: String,
    bit: u32,
}

/// Registers the game with the engine and names its buttons for the tools (idempotent).
pub fn register() {
    pav_core::games::register(
        pav_core::games::entry::<game::BlackTap>(
            "Assassin's Keg: Black Tap — sail, swim, climb and strike across the Saltwind Isles.",
            game::build,
        )
        .with_front_end(front_end),
    );
    if let Some(text) = data("bindings.toml") {
        let file: BindingsFile = toml::from_str(&text).unwrap_or_default();
        let names: Vec<(&str, u32)> =
            file.action.iter().filter(|a| a.bit < 32).map(|a| (a.name.as_str(), 1u32 << a.bit)).collect();
        pav_core::input::buttons::register_names(&names);
    }
}

/// The front end's view of the game: title, scenes, chapters, the attract playlist, credits,
/// theme, bindings and the audio banks.
pub fn front_end() -> pav_core::play::FrontEndInfo {
    let c = content::load().unwrap_or_default();
    let text = |p: &str| data(p).map(|t| t.into_owned()).unwrap_or_default();
    let mut songs: Vec<(String, String)> = data_files()
        .filter(|p| p.starts_with("shanties/") && p.ends_with(".abc"))
        .map(|p| (p.trim_start_matches("shanties/").trim_end_matches(".abc").to_string(), text(p)))
        .collect();
    songs.sort();
    pav_core::play::FrontEndInfo {
        title: c.info.title.clone(),
        tagline: c.info.tagline.clone(),
        id: c.info.id.clone(),
        title_scene: c.info.title_scene.clone(),
        new_game_scene: c.info.new_game_scene.clone(),
        training_scene: c.info.training_scene.clone(),
        chapters: c.info.chapter.iter().map(|ch| (ch.label.clone(), ch.scene.clone())).collect(),
        demo: c.demo.clone(),
        credits: c.credits.iter().map(|b| (b.heading.clone(), b.body.clone())).collect(),
        theme: text("ui/theme.toml"),
        bindings: text("bindings.toml"),
        banks: ["sounds", "music", "ambience", "fx"].iter().map(|n| (n.to_string(), text(&format!("{n}.toml")))).collect(),
        songs,
        save_key: c.info.save_key.clone(),
    }
}
