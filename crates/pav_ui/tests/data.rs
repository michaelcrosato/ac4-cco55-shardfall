//! Settings, bindings, theme, glyph choice and projection.

mod common;

use glam::Vec3;
use pav_ui::glyph::{self, Glyph, GlyphFamily};
use pav_ui::settings::{Colorblind, Prompts, Quality, Row, RowValue, SubtitleSize, TABS};
use pav_ui::{Bindings, PadButton, Projector, Settings, Theme};

#[test]
fn settings_round_trip() {
    let mut s = Settings::default();
    s.audio.music = 0.3;
    s.display.quality = Quality::High;
    s.controls.prompts = Prompts::Xbox;
    s.accessibility.colorblind = Colorblind::Tritanopia;
    s.accessibility.subtitle_size = SubtitleSize::L;
    let (back, err) = Settings::load(&s.to_json());
    assert!(err.is_none());
    assert_eq!(back, s);
    assert!(Settings::default().accessibility.subtitles, "subtitles default on");
}

#[test]
fn corrupt_settings_give_defaults_and_an_error() {
    for text in ["{not json", "[1,2,3]", "{\"audio\": {\"master\": \"loud\"}}", ""] {
        let (s, err) = Settings::load(text);
        assert_eq!(s, Settings::default(), "{text}");
        assert!(err.is_some(), "{text}");
    }
    // Missing fields are fine.
    let (s, err) = Settings::load("{\"version\": 2, \"audio\": {\"music\": 0.25}}");
    assert!(err.is_none());
    assert_eq!(s.audio.music, 0.25);
    assert_eq!(s.audio.master, Settings::default().audio.master);
    // Out-of-range numbers are clamped.
    let (s, _) = Settings::load("{\"version\": 2, \"audio\": {\"master\": 7.0}, \"controls\": {\"sensitivity\": -3}}");
    assert_eq!(s.audio.master, 1.0);
    assert_eq!(s.controls.sensitivity, 0.2);
}

#[test]
fn version_1_settings_migrate() {
    let v1 = r#"{"master_volume": 0.4, "music_volume": 0.2, "audio": {"effects": 0.6},
                "accessibility": {"colorblind": true, "subtitles": false},
                "controls": {"prompt_style": "xbox", "invert_y": true}}"#;
    let (s, err) = Settings::load(v1);
    assert!(err.is_none(), "{err:?}");
    assert_eq!(s.version, pav_ui::settings::VERSION);
    assert_eq!(s.audio.master, 0.4);
    assert_eq!(s.audio.music, 0.2);
    assert_eq!(s.audio.sfx, 0.6);
    assert_eq!(s.accessibility.colorblind, Colorblind::Deuteranopia);
    assert!(!s.accessibility.subtitles);
    assert_eq!(s.controls.prompts, Prompts::Xbox);
    assert!(s.controls.invert_y);
}

#[test]
fn every_row_steps_and_resets() {
    for (tab, rows) in TABS {
        assert!(rows.len() <= 7, "{tab}: at most 7 rows");
        assert_eq!(rows.last(), Some(&Row::Defaults), "{tab}: Defaults last");
        for row in rows.iter() {
            assert!(!row.description().is_empty());
            let mut s = Settings::default();
            let before = row.value(&s);
            let ch = row.step(&mut s, 1);
            let ch2 = if ch == pav_ui::settings::Changed::No { row.step(&mut s, -1) } else { ch };
            match before {
                RowValue::Action => assert_eq!(ch2, pav_ui::settings::Changed::No),
                _ => {
                    assert_ne!(ch2, pav_ui::settings::Changed::No, "{row:?} changes");
                    assert!(row.reset(&mut s), "{row:?} resets");
                    assert_eq!(s, Settings::default(), "{row:?}");
                }
            }
        }
    }
}

#[test]
fn blacktap_bindings_match_the_plan() {
    let b = common::bindings();
    let expect = [
        ("jump", 0, PadButton::South, Some("Space")),
        ("crouch", 1, PadButton::LeftThumb, Some("ControlLeft")),
        ("interact", 2, PadButton::North, Some("KeyE")),
        ("fire", 3, PadButton::RightTrigger2, Some("KeyF")),
        ("aim", 4, PadButton::LeftTrigger2, None),
        ("sprint", 5, PadButton::RightTrigger, Some("ShiftLeft")),
        ("attack", 6, PadButton::West, None),
        ("drop", 13, PadButton::East, Some("KeyC")),
        ("heal", 14, PadButton::DPadUp, Some("KeyR")),
        ("gull", 15, PadButton::RightThumb, Some("KeyQ")),
        ("tools", 16, PadButton::LeftTrigger, Some("Tab")),
        ("whistle", 17, PadButton::DPadDown, Some("KeyG")),
        ("prev", 18, PadButton::DPadLeft, Some("Digit1")),
        ("next", 19, PadButton::DPadRight, Some("Digit2")),
        ("map", 20, PadButton::Touchpad, Some("KeyM")),
    ];
    assert_eq!(b.actions.len(), expect.len());
    for (name, bit, pad, key) in expect {
        let a = b.action(name).unwrap_or_else(|| panic!("{name}"));
        assert_eq!(a.bit, bit, "{name}");
        assert_eq!(b.bit(name), Some(bit));
        assert_eq!(a.pad.first(), Some(&pad), "{name}");
        assert_eq!(a.keys.first().map(String::as_str), key, "{name}");
        assert!(!a.label.is_empty());
    }
    assert!(b.action("tools").expect("tools").keys.contains(&"KeyZ".to_string()));
    assert!(b.action("map").expect("map").pad.contains(&PadButton::Select));
    assert!(b.action("prev").expect("prev").keys.contains(&"ArrowLeft".to_string()));
    assert_eq!(b.names().len(), 15);
    // Masks and the conditional mouse binding.
    assert_eq!(b.pad_mask(PadButton::South), 1);
    assert_eq!(b.key_mask("KeyZ"), 1 << 16);
    let aim = 1 << 4;
    assert_eq!(b.mouse_mask("Left", 0), 1 << 6, "left click attacks");
    assert_eq!(b.mouse_mask("Left", aim), 1 << 3, "while aiming it fires");
    assert_eq!(b.mouse_mask("Right", 0), aim);
}

#[test]
fn bindings_errors_name_the_problem() {
    let e = Bindings::from_toml("[[action]]\nname=\"a\"\nbit=40\n").unwrap_err();
    assert!(e.to_string().contains("bit 40"));
    let e = Bindings::from_toml("[[action]]\nname=\"a\"\nbit=1\npad=[\"Banana\"]\n").unwrap_err();
    assert!(e.to_string().contains("Banana"));
    let e = Bindings::from_toml("[[action]]\nname=\"a\"\nbit=1\n[[action]]\nname=\"b\"\nbit=1\n").unwrap_err();
    assert!(e.to_string().contains("twice"));
    assert_eq!(PadButton::from_name("L1"), Some(PadButton::LeftTrigger));
    assert_eq!(PadButton::from_name("RT"), Some(PadButton::RightTrigger2));
    for b in PadButton::ALL {
        assert_eq!(PadButton::from_name(b.name()), Some(b));
    }
}

#[test]
fn glyphs_follow_the_device() {
    let b = common::bindings();
    assert_eq!(glyph::glyph_for_action(&b, "jump", GlyphFamily::PlayStation), Glyph::Pad(PadButton::South));
    assert_eq!(glyph::glyph_for_action(&b, "jump", GlyphFamily::Keyboard), Glyph::key("Space"));
    assert_eq!(glyph::glyph_for_action(&b, "attack", GlyphFamily::Keyboard), Glyph::MouseLeft);
    assert_eq!(glyph::glyph_for_action(&b, "aim", GlyphFamily::Keyboard), Glyph::MouseRight);
    assert_eq!(
        glyph::glyph_for_action(&b, "map", GlyphFamily::Xbox),
        Glyph::Pad(PadButton::Select),
        "Xbox pads have no touchpad"
    );
    assert_eq!(glyph::glyph_for_action(&b, "map", GlyphFamily::PlayStation), Glyph::Pad(PadButton::Touchpad));
    assert_eq!(glyph::glyph_for_action(&b, "nope", GlyphFamily::Xbox), Glyph::key("?"));
    assert_eq!(glyph::family_from_pad_name("DualSense Wireless Controller"), GlyphFamily::PlayStation);
    assert_eq!(glyph::family_from_pad_name("Wireless Controller"), GlyphFamily::PlayStation);
    assert_eq!(glyph::family_from_pad_name("PS5 Controller"), GlyphFamily::PlayStation);
    assert_eq!(glyph::family_from_pad_name("Xbox Series X Controller"), GlyphFamily::Xbox);
    assert_eq!(glyph::family_from_pad_name("8BitDo Pro 2"), GlyphFamily::Xbox);
    assert_eq!(glyph::resolve_family(Prompts::Keyboard, GlyphFamily::Xbox), GlyphFamily::Keyboard);
    assert_eq!(glyph::resolve_family(Prompts::Auto, GlyphFamily::Xbox), GlyphFamily::Xbox);
    assert_eq!(glyph::key_label("KeyE"), "E");
    assert_eq!(glyph::key_label("ControlLeft"), "Ctrl");
    assert_eq!(glyph::key_label("Digit2"), "2");
}

#[test]
fn theme_file_parses_and_rejects_typos() {
    let t = common::theme();
    assert_eq!(t.parchment, egui::Color32::from_rgb(0xf1, 0xe3, 0xc6));
    assert_eq!(t.ink, egui::Color32::from_rgb(0x2b, 0x1d, 0x12));
    assert_eq!(t.amber, egui::Color32::from_rgb(0xc9, 0x82, 0x2b));
    assert_eq!(t.teal, egui::Color32::from_rgb(0x1f, 0x6f, 0x78));
    assert_eq!(t.gold, egui::Color32::from_rgb(0xe2, 0xb0, 0x4a));
    assert_eq!(t.red, egui::Color32::from_rgb(0xa8, 0x32, 0x2d));
    assert!(t.sizes.body >= 28.0 && t.sizes.label >= 24.0 && t.sizes.title >= 56.0);
    assert_eq!(Theme::from_toml("").expect("empty is defaults"), Theme::default());
    assert!(Theme::from_toml("[colors]\nparchmint = \"#ffffff\"\n").is_err());
    assert!(Theme::from_toml("[colors]\nink = \"#zz0000\"\n").is_err());
    assert_eq!(pav_ui::theme::hex("#a8322d80").map(|c| c.a()), Some(0x80));
}

#[test]
fn projector_projects_and_rejects_points_behind() {
    let size = egui::vec2(1920.0, 1080.0);
    let p = Projector::look_at(Vec3::new(0.0, 10.0, 20.0), Vec3::new(0.0, 10.0, 0.0), 60.0, size);
    let c = p.project(Vec3::new(0.0, 10.0, -30.0)).expect("in front");
    assert!((c.x - 960.0).abs() < 0.5 && (c.y - 540.0).abs() < 0.5, "{c:?}");
    let right = p.project(Vec3::new(5.0, 10.0, 0.0)).expect("in front");
    assert!(right.x > 960.0);
    let up = p.project(Vec3::new(0.0, 15.0, 0.0)).expect("in front");
    assert!(up.y < 540.0, "y down on screen");
    assert!(p.project(Vec3::new(0.0, 10.0, 40.0)).is_none(), "behind");
    let d = p.screen_direction(Vec3::new(-3.0, 10.0, 60.0)).expect("dir");
    assert!(d.y > 0.0, "behind → toward the bottom edge");
    let eye = p.eye().expect("perspective");
    assert!(eye.distance(Vec3::new(0.0, 10.0, 20.0)) < 1e-2, "{eye}");
    assert!((p.distance_hint(Vec3::new(0.0, 10.0, -30.0)).expect("d") - 50.0).abs() < 0.05);
}

#[test]
fn png_encoder_round_trips_the_header() {
    let px = vec![200u8; 3 * 2 * 4];
    let png = pav_ui::raster::encode_png(3, 2, &px);
    assert_eq!(&png[..8], &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]);
    assert_eq!(&png[12..16], b"IHDR");
    assert_eq!(&png[png.len() - 8..png.len() - 4], b"IEND");
    let mut base = vec![0u8, 0, 255, 255];
    pav_ui::raster::composite_over(&mut base, &[255, 0, 0, 128]);
    assert!(base[0] > 120 && base[2] < 135 && base[3] == 255, "{base:?}");
}
