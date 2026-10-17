//! The effects the game draws (PLAN §9.17, `vfx.rs`): drawing never changes the simulation,
//! the continuous effects show where they should, lightning keeps a schedule, every visual-only
//! cue the game sends has a look in `fx.toml`, and rumble lives for one tick.

mod common;

use blacktap::game::{BlackTap, Body};
use pav_core::play::{DrawItem, PlayFrame};
use pav_core::{InputFrame, Shape, Sim};

fn game_mut(sim: &mut Sim) -> &mut BlackTap {
    sim.state.play.as_mut().and_then(|g| g.get_mut::<BlackTap>()).expect("the Black Tap game")
}

fn present(sim: &mut Sim) -> std::sync::Arc<PlayFrame> {
    sim.frame().play.expect("a play frame")
}

fn bot(sim: &mut Sim, ticks: u32) {
    for _ in 0..ticks {
        let i = sim.bot_input();
        sim.step(&i);
    }
}

fn emitters<'a>(p: &'a PlayFrame, preset: &'a str) -> impl Iterator<Item = &'a DrawItem> + 'a {
    p.draw.items.iter().filter(move |i| matches!(i, DrawItem::Emitter { def, .. } if def.preset == preset))
}

#[test]
fn drawing_never_changes_the_simulation() {
    for scene in ["blacktap/round/2", "blacktap/round/6"] {
        let mut sim = common::sim(scene, 2);
        bot(&mut sim, 240);
        let before = sim.state_hash();
        for _ in 0..3 {
            let _ = present(&mut sim);
        }
        assert_eq!(sim.state_hash(), before, "{scene}: presenting a frame changed the state");
    }
}

#[test]
fn a_burning_ship_draws_flames_smoke_and_haze() {
    let mut sim = common::sim("blacktap/round/2", 1);
    bot(&mut sim, 30);
    // The Black Tap herself, under way and always by the camera.
    let g = game_mut(&mut sim);
    let id = g.sea.mine;
    g.sea.ship_mut(id).expect("ship").fire = 8.0;
    sim.step(&InputFrame::default());
    let p = present(&mut sim);
    assert!(emitters(&p, "fire").count() >= 1, "flames");
    assert!(emitters(&p, "smoke").count() >= 1, "smoke");
    assert!(p.draw.items.iter().any(|i| matches!(i, DrawItem::Distortion { .. })), "heat haze");
    // Under way: the bow throws spray.
    assert!(emitters(&p, "fountain").count() >= 1, "bow spray");
}

#[test]
fn gulls_wheel_over_the_coast_and_the_ship() {
    let mut sim = common::sim("blacktap/round/1", 1);
    bot(&mut sim, 60);
    let p = present(&mut sim);
    let gulls = p.draw.items.iter().filter(|i| matches!(i, DrawItem::Prim { shape: Shape::Capsule { .. }, .. })).count();
    assert!(gulls >= 3, "gulls in view: {gulls}");
    assert!(gulls <= 40, "at most 40 gulls: {gulls}");
}

#[test]
fn dusk_lights_the_torches_and_wakes_the_fireflies() {
    let mut sim = common::sim("blacktap/round/6", 1);
    bot(&mut sim, 30);
    let lights = |p: &PlayFrame| p.draw.items.iter().filter(|i| matches!(i, DrawItem::Light { .. })).count();
    let g = game_mut(&mut sim);
    g.set_atmosphere("afternoon", 0.0);
    sim.step(&InputFrame::default());
    let day = present(&mut sim);
    let g = game_mut(&mut sim);
    g.set_atmosphere("dusk", 0.0);
    sim.step(&InputFrame::default());
    let dusk = present(&mut sim);
    assert!(lights(&dusk) >= lights(&day) + 4, "torch lights at dusk: {} by day, {} at dusk", lights(&day), lights(&dusk));
    assert!(emitters(&dusk, "fire").count() >= 4, "torch flames");
    assert_eq!(emitters(&day, "fireflies").count(), 0, "no fireflies by day");
}

#[test]
fn lightning_strikes_on_a_schedule_in_heavy_rain_only() {
    let around = glam::Vec3::ZERO;
    let strikes = |rain: f32| {
        let mut n = 0;
        let mut was = false;
        for k in 0..60 * 120 {
            let on = blacktap::vfx::lightning(7, rain, around, 0.0, k as f32 / 60.0).is_some();
            if on && !was {
                n += 1;
            }
            was = on;
        }
        n
    };
    let storm = strikes(0.85);
    assert!((10..=22).contains(&storm), "strikes in two minutes of storm: {storm}");
    assert_eq!(strikes(0.2), 0, "no lightning in light rain");
    let s = (0..60 * 60)
        .find_map(|k| blacktap::vfx::lightning(7, 0.85, around, 0.0, k as f32 / 60.0))
        .expect("a strike within a minute");
    let d = glam::Vec2::new(s.pos.x, s.pos.z).length();
    assert!((100.0..=350.0).contains(&d), "strikes land out to sea: {d} m");
}

#[test]
fn gull_sight_pulses_a_ring() {
    let mut sim = common::sim("blacktap/round/5", 1);
    bot(&mut sim, 30);
    let g = game_mut(&mut sim);
    assert_eq!(g.body, Body::Foot);
    g.gull = true;
    let p = present(&mut sim);
    assert!(p.draw.items.iter().any(|i| matches!(i, DrawItem::Decal { ring, .. } if *ring > 0.9)), "the pulse ring");
}

#[test]
fn visual_cues_have_a_look() {
    let text = blacktap::data("fx.toml").expect("fx.toml");
    let fx: toml::Value = toml::from_str(&text).expect("fx.toml parses");
    let cues = fx.get("cue").and_then(|c| c.as_table()).expect("[cue.*]");
    for name in [
        "wake_foam",
        "footfall_sand",
        "footfall_dust",
        "dart_trail",
        "survey_swirl",
        "water_dive",
        "hay_land",
        "fire_crackle",
        "lightning_crack",
        "splinters",
        "cannon_fire",
        "mortar_launch",
        "splash_big",
        "explosion",
        "sink",
        "smoke_keg",
        "pistol_fire",
        "sword_clash",
        "gull_sight_on",
    ] {
        assert!(cues.contains_key(name), "fx.toml has no [cue.{name}]");
    }
}

#[test]
fn rumble_lives_for_one_tick() {
    let mut sim = common::sim("blacktap/round/2", 1);
    bot(&mut sim, 30);
    let g = game_mut(&mut sim);
    g.shake(0.3, (0.9, 0.4, 120));
    assert!(!present(&mut sim).effects.rumble.is_empty(), "the frame carries the cue");
    sim.step(&InputFrame::default());
    let p = present(&mut sim);
    assert!(p.effects.rumble.is_empty(), "gone the next tick");
}
