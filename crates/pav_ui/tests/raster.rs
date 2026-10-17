//! Rasterises every screen, the HUD and the overlays with sample data, checks they drew
//! something, and writes PNGs to `out/ui/` (composited over a painted stand-in for the live
//! scene) to look at: 1080p for all, 540p / 4K spot checks, and the phone overlay at 844×390.

mod common;

use std::time::Instant;

use pav_core::play::PlayFrame;
use pav_ui::frontend::Mode;
use pav_ui::glyph::{self, Glyph, GlyphFamily};
use pav_ui::{FrontEnd, HudEnv, Platform, Settings, UiInput, raster};

/// A sunset sea, standing in for the 3D scene behind the UI.
fn backdrop(w: u32, h: u32) -> Vec<u8> {
    let mut px = vec![0u8; (w * h * 4) as usize];
    let horizon = 0.56;
    for y in 0..h {
        let v = y as f32 / h as f32;
        for x in 0..w {
            let u = x as f32 / w as f32;
            let (r, g, b) = if v < horizon {
                let t = v / horizon;
                let sky_top = (40.0, 52.0, 92.0);
                let sky_low = (236.0, 166.0, 92.0);
                let k = t.powf(1.6);
                let sun = (-((u - 0.68).powi(2) * 30.0 + (v - horizon + 0.06).powi(2) * 120.0)).exp();
                (
                    sky_top.0 + (sky_low.0 - sky_top.0) * k + 60.0 * sun,
                    sky_top.1 + (sky_low.1 - sky_top.1) * k + 50.0 * sun,
                    sky_top.2 + (sky_low.2 - sky_top.2) * k + 20.0 * sun,
                )
            } else {
                let t = (v - horizon) / (1.0 - horizon);
                let glint =
                    (-((u - 0.68).powi(2) * 40.0)).exp() * (1.0 - t) * (0.5 + 0.5 * ((v * 900.0).sin() * (u * 300.0).sin()));
                (
                    20.0 + 30.0 * (1.0 - t) + 160.0 * glint,
                    52.0 + 30.0 * (1.0 - t) + 110.0 * glint,
                    66.0 + 20.0 * (1.0 - t) + 50.0 * glint,
                )
            };
            let i = ((y * w + x) * 4) as usize;
            px[i] = r.clamp(0.0, 255.0) as u8;
            px[i + 1] = g.clamp(0.0, 255.0) as u8;
            px[i + 2] = b.clamp(0.0, 255.0) as u8;
            px[i + 3] = 255;
        }
    }
    px
}

fn sample_frame() -> PlayFrame {
    PlayFrame { ui: pav_ui::sample_ui_model(), ..Default::default() }
}

/// Renders the front end in `mode` at `size` pixels; returns the UI layer (straight alpha).
fn render_mode(fe: &mut FrontEnd, size: [u32; 2], frame: Option<&PlayFrame>, family: GlyphFamily) -> Vec<u8> {
    let theme = common::theme();
    let bindings = common::bindings();
    let ppp = size[1] as f32 / 1080.0;
    let pts = egui::vec2(size[0] as f32 / ppp, size[1] as f32 / ppp);
    let settings = Settings::default();
    raster::render(
        size,
        ppp,
        |ctx| pav_ui::theme::install(ctx, &theme),
        |ctx| {
            let env = HudEnv {
                theme: &theme,
                bindings: &bindings,
                family,
                settings: &settings,
                proj: pav_ui::sample_projector(pts),
                time: 1.3,
            };
            fe.draw(ctx, frame, &env);
        },
    )
}

fn save(name: &str, size: [u32; 2], ui: &[u8], over_scene: bool) {
    let mut img = if over_scene { backdrop(size[0], size[1]) } else { vec![0u8; ui.len()] };
    if over_scene {
        raster::composite_over(&mut img, ui);
    } else {
        img.copy_from_slice(ui);
    }
    let path = common::out_dir().join(format!("{name}.png"));
    std::fs::write(&path, raster::encode_png(size[0], size[1], &img)).expect("write png");
}

fn front(mode: Mode, mobile: bool) -> FrontEnd {
    let mut fe = FrontEnd::new(common::info(), Settings::default(), Platform { mobile, kiosk: false, can_quit: true }, true);
    fe.show(mode);
    fe
}

#[test]
fn every_screen_rasterises() {
    let size = [1920, 1080];
    let frame = sample_frame();
    let t0 = Instant::now();
    for mode in Mode::ALL {
        let mobile = matches!(mode, Mode::MobileMenu | Mode::MobileAbout);
        let mut fe = front(mode, mobile);
        let f = if matches!(
            mode,
            Mode::Title | Mode::Menu | Mode::Chapters | Mode::Settings | Mode::Credits | Mode::Confirm | Mode::Boot
        ) {
            None
        } else {
            Some(&frame)
        };
        let ui = render_mode(&mut fe, size, f, GlyphFamily::PlayStation);
        let cov = raster::coverage(&ui);
        assert!(cov > 0.01, "{mode:?} drew almost nothing ({cov})");
        save(&format!("screen_{}_1080", mode.name()), size, &ui, true);
    }
    eprintln!("16 screens at 1080p in {:.2} s", t0.elapsed().as_secs_f32());
}

#[test]
fn one_frame_is_fast() {
    let mut fe = front(Mode::Settings, false);
    let t0 = Instant::now();
    let ui = render_mode(&mut fe, [1920, 1080], None, GlyphFamily::PlayStation);
    let s = t0.elapsed().as_secs_f32();
    assert!(raster::coverage(&ui) > 0.5);
    eprintln!("settings at 1080p: {s:.3} s");
    assert!(s < 3.0, "rasterising 1080p took {s} s");
}

#[test]
fn hud_and_overlays() {
    let size = [1920, 1080];
    let mut frame = sample_frame();
    let mut fe = front(Mode::Play, false);
    let ui = render_mode(&mut fe, size, Some(&frame), GlyphFamily::PlayStation);
    assert!(raster::coverage(&ui) > 0.02);
    save("hud_1080", size, &ui, true);
    let ui = render_mode(&mut fe, size, Some(&frame), GlyphFamily::Xbox);
    save("hud_xbox_1080", size, &ui, true);
    let ui = render_mode(&mut fe, size, Some(&frame), GlyphFamily::Keyboard);
    save("hud_keyboard_1080", size, &ui, true);
    frame.ui.banner = None;
    for (name, o) in pav_ui::sample_overlays() {
        frame.ui.overlay = Some(o);
        let ui = render_mode(&mut fe, size, Some(&frame), GlyphFamily::PlayStation);
        assert!(raster::coverage(&ui) > 0.02, "{name}");
        save(&format!("overlay_{name}_1080"), size, &ui, true);
    }
}

#[test]
fn spot_checks_at_other_sizes() {
    let frame = sample_frame();
    for (size, tag) in [([960u32, 540u32], "540"), ([3840, 2160], "4k"), ([1280, 720], "720")] {
        for mode in [Mode::Title, Mode::Menu, Mode::Settings, Mode::Play, Mode::Paused] {
            let mut fe = front(mode, false);
            let f = if matches!(mode, Mode::Play | Mode::Paused) { Some(&frame) } else { None };
            let ui = render_mode(&mut fe, size, f, GlyphFamily::PlayStation);
            assert!(raster::coverage(&ui) > 0.01, "{mode:?} at {tag}");
            save(&format!("screen_{}_{tag}", mode.name()), size, &ui, true);
        }
    }
}

#[test]
fn phone_overlay() {
    let size = [844, 390];
    let frame = sample_frame();
    for mode in [Mode::Attract, Mode::MobileMenu, Mode::MobileAbout] {
        let mut fe =
            FrontEnd::new(common::info(), Settings::default(), Platform { mobile: true, kiosk: false, can_quit: false }, false);
        let _ = fe.update(0.016, &UiInput::default(), None);
        fe.on_scene_loaded();
        fe.show(mode);
        let ui = render_mode(&mut fe, size, Some(&frame), GlyphFamily::PlayStation);
        assert!(raster::coverage(&ui) > 0.001, "{mode:?}");
        save(&format!("phone_{}", mode.name()), size, &ui, true);
        if mode == Mode::Attract {
            let c = fe.menu_button().expect("☰ shown");
            assert!(c.x > 2000.0 && c.y < 200.0, "☰ top right: {c:?}");
        }
    }
}

#[test]
fn glyph_sheet() {
    let theme = common::theme();
    let size = [1920u32, 1080u32];
    let ui = raster::render(
        size,
        1.0,
        |ctx| pav_ui::theme::install(ctx, &theme),
        |ctx| {
            let p = ctx.layer_painter(egui::LayerId::background());
            p.rect_filled(ctx.viewport_rect(), 0.0, theme.ink);
            let mut glyphs: Vec<Glyph> = pav_ui::PadButton::ALL.iter().map(|b| Glyph::Pad(*b)).collect();
            glyphs.extend([Glyph::LeftStick, Glyph::RightStick, Glyph::DPad, Glyph::DPadHorizontal]);
            let keys: Vec<Glyph> = ["Space", "KeyE", "Escape", "ShiftLeft", "ControlLeft", "Tab", "ArrowLeft", "Digit1", "Enter"]
                .iter()
                .map(|k| Glyph::key(k))
                .collect();
            let mice = [Glyph::MouseLeft, Glyph::MouseRight, Glyph::MouseMiddle, Glyph::MouseWheel, Glyph::MouseMove];
            for (row, family) in [GlyphFamily::PlayStation, GlyphFamily::Xbox].iter().enumerate() {
                for (i, g) in glyphs.iter().enumerate() {
                    let c = egui::pos2(80.0 + i as f32 * 85.0, 120.0 + row as f32 * 140.0);
                    glyph::paint(&p, c, 56.0, g, *family, &theme);
                }
            }
            let mut x = 80.0;
            for g in keys.iter().chain(mice.iter()) {
                let s = glyph::size(&p, g, 56.0, GlyphFamily::Keyboard, &theme);
                glyph::paint(&p, egui::pos2(x + s.x * 0.5, 420.0), 56.0, g, GlyphFamily::Keyboard, &theme);
                x += s.x + 24.0;
            }
            // Icons.
            let icons = [
                "hull",
                "shot",
                "chain",
                "sail",
                "keg",
                "kegs",
                "repair",
                "cargo",
                "coin",
                "timber",
                "pistol",
                "dart",
                "smoke",
                "bottle",
                "grog",
                "whistle",
                "map",
                "sheet",
                "spyglass",
                "chest",
                "perch",
                "perch_done",
                "tap",
                "fort",
                "tavern",
                "flotsam",
                "ship",
                "enemy_ship",
                "player",
                "objective",
                "waypoint",
                "skull",
                "rope",
                "swivel",
                "wind",
                "star",
                "mystery",
            ];
            for (i, name) in icons.iter().enumerate() {
                let c = egui::pos2(80.0 + (i % 19) as f32 * 95.0, 580.0 + (i / 19) as f32 * 120.0);
                p.circle_filled(c, 48.0, theme.parchment_dark);
                pav_ui::paint::icon(&p, name, c, 72.0, theme.parchment_light, &theme);
            }
            pav_ui::paint::keg_tap(&p, egui::pos2(1500.0, 900.0), 260.0, &theme);
            pav_ui::paint::medallion(&p, egui::pos2(1150.0, 900.0), 110.0, &theme);
        },
    );
    assert!(raster::coverage(&ui) > 0.9);
    save("glyphs", size, &ui, false);
}
