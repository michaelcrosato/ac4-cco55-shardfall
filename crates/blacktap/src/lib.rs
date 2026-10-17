//! Assassin's Keg: Black Tap `[AssassinsKegBlackTap-W9]`, the game. Call [`register`] once at
//! startup (the app, the `pav` binary, tests); then `load scene=blacktap` works everywhere.

use std::borrow::Cow;

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

/// Registers the game with the engine (idempotent).
pub fn register() {}
