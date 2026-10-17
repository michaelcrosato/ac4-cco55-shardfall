#![allow(dead_code)]

use std::path::PathBuf;

use pav_ui::{Bindings, FrontEnd, Platform, Settings, Theme};

pub fn theme() -> Theme {
    Theme::from_toml(include_str!("../../../../games/blacktap/ui/theme.toml")).expect("theme.toml")
}

pub fn bindings() -> Bindings {
    Bindings::from_toml(include_str!("../../../../games/blacktap/bindings.toml")).expect("bindings.toml")
}

pub fn info() -> pav_core::play::FrontEndInfo {
    let mut i = pav_ui::sample_front_end_info();
    i.bindings = include_str!("../../../../games/blacktap/bindings.toml").to_string();
    i.theme = include_str!("../../../../games/blacktap/ui/theme.toml").to_string();
    i
}

pub fn desktop(has_progress: bool) -> FrontEnd {
    FrontEnd::new(info(), Settings::default(), Platform { mobile: false, kiosk: false, can_quit: true }, has_progress)
}

pub fn out_dir() -> PathBuf {
    let d = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../out/ui");
    std::fs::create_dir_all(&d).expect("out/ui");
    d
}
