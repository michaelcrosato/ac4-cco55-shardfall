//! The Drydock (PLAN §9.19): the scene builds with every station's sign and pads, each pad does
//! what it says (on foot, or under the keel at sea), boarding the hulk works with the practice
//! dummies, and a run is exact.

mod common;

use blacktap::drydock::{Pad, stations};
use blacktap::game::{BlackTap, Body};
use glam::{Vec2, Vec3};
use pav_core::play::{DrawItem, Overlay, PlayFrame};
use pav_core::{InputFrame, Sim};

const SCENE: &str = "blacktap/drydock";

fn game(sim: &Sim) -> &BlackTap {
    sim.play().and_then(|g| g.get::<BlackTap>()).expect("the Black Tap game")
}

fn game_mut(sim: &mut Sim) -> &mut BlackTap {
    sim.state.play.as_mut().and_then(|g| g.get_mut::<BlackTap>()).expect("the Black Tap game")
}

fn idle(sim: &mut Sim, ticks: u32) {
    for _ in 0..ticks {
        sim.step(&InputFrame::default());
    }
}

fn present(sim: &mut Sim) -> std::sync::Arc<PlayFrame> {
    sim.frame().play.expect("a play frame")
}

fn pad(sim: &Sim, effect: &str) -> Pad {
    stations(game(sim)).with_effect(effect).next().cloned().unwrap_or_else(|| panic!("no pad '{effect}'"))
}

/// Walks the player onto a land pad (a teleport, then a few ticks).
fn stand_on(sim: &mut Sim, effect: &str) {
    let p = pad(sim, effect);
    assert!(!p.afloat, "{effect} is a sea pad");
    let pid = sim.state.player.expect("player");
    sim.set_position(pid, p.at + Vec3::Y * 0.05);
    idle(sim, 3);
}

/// Back to the start on the pier (off every pad).
fn step_off(sim: &mut Sim) {
    let pid = sim.state.player.expect("player");
    sim.set_position(pid, Vec3::new(0.5, 2.1, 6.0));
    idle(sim, 3);
}

/// The Black Tap (at the helm) sails into a sea pad's ring.
fn sail_into(sim: &mut Sim, effect: &str) {
    let p = pad(sim, effect);
    assert!(p.afloat, "{effect} is a land pad");
    let g = game_mut(sim);
    g.body = Body::Helm;
    let me = g.sea.mine;
    let s = g.sea.ship_mut(me).expect("the Black Tap");
    s.pos = Vec2::new(p.at.x, p.at.z);
    s.speed = 0.0;
    idle(sim, 3);
}

/// Out of every ring.
fn sail_off(sim: &mut Sim) {
    let g = game_mut(sim);
    let me = g.sea.mine;
    if let Some(s) = g.sea.ship_mut(me) {
        s.pos = Vec2::new(0.0, 60.0);
    }
    idle(sim, 2);
}

#[test]
fn scene_builds_with_signs_pads_and_people() {
    let mut sim = common::sim(SCENE, 1);
    idle(&mut sim, 5);
    let g = game(&sim);
    assert_eq!(g.body, Body::Foot, "the player starts on foot");
    let feet = BlackTap::player_feet(&sim).expect("player");
    assert!(feet.distance(Vec3::new(0.5, 2.0, 6.0)) < 1.0 && feet.y > 1.5, "on the pier: {feet}");
    let ship = g.sea.my_ship().expect("the Black Tap");
    assert!(ship.anchored && ship.pos.distance(Vec2::new(-6.8, 24.0)) < 1.0, "moored at the pier");
    assert!(g.land.deck.is_some(), "her deck is walkable");
    let st = stations(g);
    assert!(st.list.len() >= 21, "every station: {}", st.list.len());
    let problems = blacktap::drydock::check(g);
    assert!(problems.is_empty(), "{problems:#?}");
    for key in [
        "sailing_lane",
        "gunnery_range",
        "duel_buoy",
        "mortar_yard",
        "boarding_hulk",
        "pier",
        "climbing_wall",
        "haystack_tower",
        "rope_run",
        "combat_ring",
        "tool_range",
        "stealth_yard",
        "market",
        "poster_wall",
        "map_table",
        "theatre",
        "jukebox",
        "weather",
        "ui_gallery",
        "camera_bench",
        "rumble",
    ] {
        assert!(st.by_key(key).is_some(), "station {key}");
    }
    // People: the ring's four archetypes, the range's dummies, the yard's guards, the crowd.
    let count = |group: &str| g.stealth.npcs.values().filter(|n| n.group == group).count();
    assert_eq!(count("ring"), 4);
    assert_eq!(count("range"), 3);
    assert_eq!(count("yard"), 2);
    assert_eq!(count("market"), 8);
    let mut kinds: Vec<&str> = g.stealth.npcs.values().filter(|n| n.group == "ring").map(|n| n.kind.as_str()).collect();
    kinds.sort();
    assert_eq!(kinds, ["brute", "guard", "musketeer", "officer"]);
    // Sea stations' ships: three targets and the hulk (disabled, boardable by the game's rules).
    assert_eq!(g.sea.by_tag("target").filter(|s| s.alive()).count(), 3);
    assert!(g.sea.by_tag("restraint").any(|s| s.disabled && s.alive()), "the boarding hulk");
    // Every sign's name and at least one of its pads is drawn.
    let frame = present(&mut sim);
    for s in &st.list {
        let title = s.name.to_uppercase();
        assert!(frame.draw.items.iter().any(|i| matches!(i, DrawItem::Text { text, .. } if *text == title)), "sign {title}");
        let drawn = s.pads.iter().any(|&i| {
            let p = &st.pads[i];
            frame.draw.items.iter().any(|d| match d {
                DrawItem::Decal { center, .. } => Vec2::new(center.x - p.at.x, center.z - p.at.z).length() < 0.3,
                _ => false,
            })
        });
        assert!(drawn, "a pad of {}", s.name);
    }
    // The guide for the station the player stands at.
    let obj = frame.ui.objective.clone().expect("the station guide");
    assert_eq!(obj.title, "PIER");
    assert!(!obj.text.is_empty() && !obj.optional.is_empty());
}

#[test]
fn every_terrain_chunk_has_its_own_render_mesh() {
    // The view keys terrain meshes by a hash that collides for some chunks (their land then
    // draws as open sea); the Drydock gives each chunk a fresh key.
    let sim = common::sim(SCENE, 1);
    let mut keys = std::collections::BTreeMap::new();
    for (region, chunk) in &sim.state.statics.chunks {
        if let Some(t) = &chunk.terrain {
            let key = pav_core::terrain::render_key(t.cx, t.cz, chunk.version);
            if let Some(other) = keys.insert(key, *region) {
                panic!("{other:?} and {region:?} share a terrain render key");
            }
        }
    }
    assert!(keys.len() > 40, "terrain chunks: {}", keys.len());
}

#[test]
fn every_land_pad_fires() {
    let mut sim = common::sim(SCENE, 1);
    idle(&mut sim, 3);
    let st = stations(game(&sim));
    for p in st.pads.iter().filter(|p| !p.afloat) {
        let before = game(&sim).dock.fired(&p.effect);
        stand_on(&mut sim, &p.effect);
        assert!(game(&sim).dock.fired(&p.effect) > before, "pad {} ({}) did not fire", p.label, p.effect);
        step_off(&mut sim);
    }
}

#[test]
fn presentation_pads() {
    let mut sim = common::sim(SCENE, 1);
    idle(&mut sim, 3);
    // Weather: the atmosphere changes.
    stand_on(&mut sim, "atmos:storm");
    assert_eq!(game(&sim).atmos_to, "storm");
    stand_on(&mut sim, "weather:squall");
    assert!(!game(&sim).sea.spouts.is_empty(), "squall: waterspouts");
    // Camera bench: the context while on the bench, the game's own after.
    stand_on(&mut sim, "camera:helm");
    idle(&mut sim, 2);
    assert_eq!(game(&sim).cam.context, "helm");
    stand_on(&mut sim, "camera:spyglass");
    assert_eq!(game(&sim).cam.context, "spyglass");
    step_off(&mut sim);
    assert_ne!(game(&sim).cam.context, "spyglass", "off the bench");
    // UI gallery and the map table: overlays with sample data.
    stand_on(&mut sim, "ui:tiles");
    assert!(matches!(present(&mut sim).ui.overlay, Some(Overlay::Tiles { .. })));
    stand_on(&mut sim, "ui:results");
    assert!(matches!(present(&mut sim).ui.overlay, Some(Overlay::Results(_))));
    stand_on(&mut sim, "ui:wheel");
    assert!(matches!(present(&mut sim).ui.overlay, Some(Overlay::Wheel { .. })));
    stand_on(&mut sim, "ui:cinematic");
    assert!(matches!(present(&mut sim).ui.overlay, Some(Overlay::Cinematic { .. })));
    step_off(&mut sim);
    assert!(present(&mut sim).ui.overlay.is_none(), "leaving the gallery closes it");
    stand_on(&mut sim, "map:open");
    assert!(matches!(present(&mut sim).ui.overlay, Some(Overlay::Map { .. })));
    // Jukebox: songs from music.toml, the ambience mixer, the sound bank.
    stand_on(&mut sim, "song:next");
    let songs = blacktap::drydock::songs();
    assert!(songs.len() >= 10);
    assert_eq!(game(&sim).music, songs[0]);
    stand_on(&mut sim, "mix:next");
    let audio = present(&mut sim).audio.clone();
    let loud: Vec<_> = audio.ambience.iter().filter(|(_, v)| *v > 0.0).collect();
    assert_eq!(loud.len(), 1, "one ambience layer alone: {:?}", audio.ambience);
    // Theatre: Last Words' letterbox and white mist, then the Drydock comes back.
    stand_on(&mut sim, "theatre:last_words");
    assert!(game(&sim).stealth.last_words.is_some());
    idle(&mut sim, 60);
    let ui = present(&mut sim).ui.clone();
    assert_eq!(ui.screen.letterbox, 1.0);
    assert!(ui.screen.white > 0.0);
    idle(&mut sim, 60 * 26);
    assert!(game(&sim).stealth.last_words.is_none());
    assert_eq!(game(&sim).dock.theatre, None, "the theatre's weather and music restored");
    // Rumble: shake and a controller cue.
    stand_on(&mut sim, "rumble:blast");
    let fx = present(&mut sim).effects.clone();
    assert!(fx.shake > 0.3 && !fx.rumble.is_empty());
}

#[test]
fn traversal_and_combat_pads() {
    let mut sim = common::sim(SCENE, 1);
    idle(&mut sim, 3);
    // Lifts: up the haystack tower, where the perch offers the Leap of Spirits.
    stand_on(&mut sim, "lift:tower_perch");
    let feet = BlackTap::player_feet(&sim).expect("player");
    assert!(feet.y > 15.0, "on the lantern roof: {feet}");
    let prompts: Vec<String> = present(&mut sim).ui.prompts.iter().map(|p| p.label.clone()).collect();
    assert!(prompts.iter().any(|p| p == "Leap of Spirits"), "{prompts:?}");
    // Pier: aboard by the wheel, where the game offers the helm.
    stand_on(&mut sim, "ship:board");
    let prompts: Vec<String> = present(&mut sim).ui.prompts.iter().map(|p| p.label.clone()).collect();
    assert!(prompts.iter().any(|p| p == "Take the helm"), "{prompts:?}");
    step_off(&mut sim);
    // Combat ring: fresh dummies are new people; Spar wakes them.
    let ring =
        |sim: &Sim| -> Vec<_> { game(sim).stealth.npcs.iter().filter(|(_, n)| n.group == "ring").map(|(&id, _)| id).collect() };
    let before = ring(&sim);
    stand_on(&mut sim, "ring:fresh");
    let after = ring(&sim);
    assert_eq!(after.len(), 4);
    assert!(after.iter().all(|id| !before.contains(id)), "respawned");
    stand_on(&mut sim, "ring:spar");
    assert!(game(&sim).stealth.npcs.values().filter(|n| n.group == "ring").all(|n| n.mind.aware()));
    stand_on(&mut sim, "ring:stand_down");
    assert!(game(&sim).stealth.npcs.values().filter(|n| n.group == "ring").all(|n| !n.mind.aware()));
    stand_on(&mut sim, "ring:slow");
    assert!(game(&sim).dock.slow_tells);
    // Tool range: restocked.
    for v in game_mut(&mut sim).land.hero.tools.stock.values_mut() {
        *v = 0;
    }
    stand_on(&mut sim, "tools:restock");
    assert!(game(&sim).land.hero.tools.stock.values().all(|&n| n > 0));
    // Heat and posters.
    stand_on(&mut sim, "heat:up");
    assert!(game(&sim).stealth.heat.level() >= 1);
    stand_on(&mut sim, "heat:clear");
    assert_eq!(game(&sim).stealth.heat.level(), 0);
    // Stealth yard: the cones are drawn, and hidden by their pad.
    let lines = |sim: &mut Sim| present(sim).draw.items.iter().filter(|i| matches!(i, DrawItem::Line { .. })).count();
    let with = lines(&mut sim);
    stand_on(&mut sim, "yard:cones");
    assert!(game(&sim).dock.hide_cones);
    let without = lines(&mut sim);
    assert!(without + 20 < with, "vision cones hidden ({with} lines before, {without} after)");
}

#[test]
fn sea_pads() {
    let mut sim = common::sim(SCENE, 1);
    idle(&mut sim, 3);
    // Duel buoy: a hostile schooner.
    sail_into(&mut sim, "duel:spawn");
    let duel = game(&sim).sea.by_tag("duel").find(|s| s.alive()).expect("the duel schooner");
    assert!(duel.captain.as_ref().is_some_and(|c| c.hostile && c.kind == pav_kit::naval::CaptainKind::Schooner));
    sail_off(&mut sim);
    // Mortar yard: the mortar arms, then stands down.
    sail_into(&mut sim, "mortar:toggle");
    assert!(game(&sim).sea.batteries.iter().any(|b| b.kind == pav_kit::naval::BatteryKind::Mortar));
    sail_off(&mut sim);
    sail_into(&mut sim, "mortar:toggle");
    assert!(!game(&sim).sea.batteries.iter().any(|b| b.kind == pav_kit::naval::BatteryKind::Mortar));
    sail_off(&mut sim);
    // Sailing lane: the wind swings 90°.
    let w0 = game(&sim).sea.wind;
    sail_into(&mut sim, "wind:veer");
    let w1 = game(&sim).sea.wind;
    assert!(w0.dot(w1).abs() < 1e-3 && (w0.length() - w1.length()).abs() < 1e-3, "{w0} -> {w1}");
    sail_off(&mut sim);
    // Gunnery range: fresh targets (new ships).
    let old: Vec<u32> = game(&sim).sea.by_tag("target").map(|s| s.id).collect();
    sail_into(&mut sim, "gunnery:reset");
    let new: Vec<u32> = game(&sim).sea.by_tag("target").map(|s| s.id).collect();
    assert_eq!(new.len(), 3);
    assert!(new.iter().all(|id| !old.contains(id)));
    sail_off(&mut sim);
    // Boarding hulk: a fresh one.
    let old = game(&sim).dock.hulk;
    sail_into(&mut sim, "hulk:fresh");
    assert_ne!(game(&sim).dock.hulk, old);
}

#[test]
fn boarding_the_hulk_with_dummies_about() {
    let mut sim = common::sim(SCENE, 1);
    idle(&mut sim, 3);
    // Alongside the hulk at the helm, then Board (jump).
    let hulk = game(&sim).dock.hulk.and_then(|id| game(&sim).sea.ship(id)).map(|s| s.pos).expect("hulk");
    {
        let g = game_mut(&mut sim);
        g.body = Body::Helm;
        let me = g.sea.mine;
        let s = g.sea.ship_mut(me).expect("ship");
        s.pos = hulk + Vec2::new(0.0, 14.0);
        s.heading = 90.0;
        s.speed = 0.0;
        s.anchored = false;
    }
    idle(&mut sim, 2);
    let jump = InputFrame { held: blacktap::game::b::JUMP, pressed: blacktap::game::b::JUMP, ..Default::default() };
    sim.step(&jump);
    assert!(game(&sim).land.boarding.is_some(), "boarding");
    assert!(game(&sim).stealth.npcs.values().all(|n| n.group == "market"), "the practice fighters stepped aside");
    idle(&mut sim, 60 * 3);
    let phase = game(&sim).land.boarding.as_ref().map(|b| b.phase);
    assert_eq!(phase, Some(blacktap::foot::BoardPhase::Fight));
    // Won and plundered: back at the helm, the dummies back, a fresh hulk on her way.
    {
        let g = game_mut(&mut sim);
        if let Some(b) = &mut g.land.boarding {
            b.phase = blacktap::foot::BoardPhase::Plunder;
        }
        g.mission.set_flag("plunder_done");
    }
    idle(&mut sim, 2);
    let g = game(&sim);
    assert!(g.land.boarding.is_none() && g.body == Body::Helm);
    assert_eq!(g.stealth.npcs.values().filter(|n| n.group == "ring").count(), 4);
    assert_eq!(g.stealth.npcs.values().filter(|n| n.group == "yard").count(), 2);
    idle(&mut sim, 60 * 11);
    assert!(game(&sim).dock.hulk.and_then(|id| game(&sim).sea.ship(id)).is_some_and(|s| s.alive() && s.disabled));
}

#[test]
fn a_run_is_exact() {
    // Walk north off the pier, across the quay and onto the climbing wall's lift.
    let run = || {
        let mut sim = common::sim(SCENE, 4);
        let walk = InputFrame { move_dir: Vec2::new(0.0, -1.0), ..Default::default() };
        for t in 0..600 {
            let mut i = walk;
            if t % 97 == 0 {
                i.held |= blacktap::game::b::INTERACT;
                i.pressed |= blacktap::game::b::INTERACT;
            }
            sim.step(&i);
        }
        let fired = game(&sim).dock.fired.clone();
        (sim.state_hash(), fired)
    };
    let (a, fired) = run();
    let (b, _) = run();
    assert_eq!(a, b, "same inputs, same state");
    assert!(!fired.is_empty(), "the walk crossed a pad: {fired:?}");
}
