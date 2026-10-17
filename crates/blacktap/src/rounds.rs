//! The missions' vocabulary: every `game = "..."` action and condition the Round files use
//! (games/blacktap/missions/r*.toml), and what happens between Rounds (title, results, free
//! roam, the Drydock). `pav level_check scene=blacktap` checks that each name used in data is
//! handled here (`ACTS`, `CONDS`).

use glam::{Vec2, Vec3};
use pav_core::entity::EntityId;
use pav_core::frame::SimEvent;
use pav_core::input::InputFrame;
use pav_core::play::UiRequest;
use pav_core::sim::Sim;
use pav_kit::combat::Assassination;
use pav_kit::naval::{BatteryKind, CaptainKind, Faction, Helm};
use serde_json::json;

use crate::game::{BlackTap, Body, Mode, b};
use crate::sea::{HelmInput, SeaEvent};

/// Action names (exact, or a prefix ending in ':').
pub const ACTS: &[&str] = &[
    "place_ship:",
    "player_at_helm",
    "player_at:",
    "sail:",
    "teach:",
    "supplies",
    "wind:",
    "spawn_flotsam",
    "spawn_restraint_patrol",
    "spawn_gunboats",
    "restraint_engage",
    "plunder",
    "end_boarding",
    "start_fort",
    "spawn_town",
    "spawn_tavern",
    "spawn_courier",
    "spawn_stocks_guards",
    "spawn_fort_guards",
    "drye_balcony",
    "has_key",
    "last_words",
    "alarm",
    "ship_under_cliffs",
    "storm",
    "spawn_frigate",
    "spawn_waterspouts",
];

/// Condition names.
pub const CONDS: &[&str] = &[
    "sails_raised",
    "gunboats_sunk",
    "restraint_disabled",
    "restraint_disabled_by_chain",
    "fort_silent",
    "hull_above_half",
    "at_anchorage",
    "ashore",
    "at_helm",
    "escaped",
];

/// Whether the vocabulary handles a name used in data.
pub fn known_act(name: &str) -> bool {
    ACTS.iter().any(|a| if a.ends_with(':') { name.starts_with(a) } else { *a == name })
}

pub fn known_cond(name: &str) -> bool {
    CONDS.contains(&name)
}

fn place_ship(g: &mut BlackTap, at: Vec2, heading: f32) {
    let me = g.sea.mine;
    if let Some(s) = g.sea.ship_mut(me) {
        s.pos = at;
        s.heading = heading;
        s.speed = 0.0;
        s.sail = 0;
        s.anchored = false;
        s.yaw_rate = 0.0;
    }
}

/// A heading looking from `from` toward `to`.
fn heading(from: Vec2, to: Vec2) -> f32 {
    pav_kit::naval::heading_of(to - from)
}

/// Performs a mission action.
pub fn act(g: &mut BlackTap, sim: &mut Sim, name: &str) {
    let c = g.content();
    let w = g.world();
    let (head, arg) = name.split_once(':').map(|(a, b)| (a, Some(b))).unwrap_or((name, None));
    match head {
        "place_ship" => {
            let at = w.marker_xz(arg.unwrap_or("r1_start"));
            // Face the next place the Round goes.
            let toward = match arg {
                Some("r1_start") => w.marker_xz("flotsam_1"),
                Some("r2_start") => Vec2::new(0.0, 150.0),
                Some("r3_start") => w.marker_xz("battery_west"),
                Some("r4_start") => w.marker_xz("r4_anchorage"),
                _ => at + Vec2::new(1.0, -1.0),
            };
            place_ship(g, at, heading(at, toward));
        }
        "player_at_helm" => {
            g.body = Body::Helm;
        }
        "player_at" => {
            g.body = Body::Foot;
            let p = w.marker(arg.unwrap_or("r5_start")).unwrap_or_default();
            let y = w.ground(Vec2::new(p.x, p.z)).max(p.y) + 0.1;
            if let Some(pid) = sim.state.player {
                sim.set_position(pid, Vec3::new(p.x, y, p.z));
            }
            // The Black Tap waits at anchor off the beach.
            let anchorage = w.marker_xz("r4_anchorage");
            place_ship(g, anchorage, 300.0);
        }
        "sail" => {
            let lvl: u8 = arg.and_then(|a| a.parse().ok()).unwrap_or(1);
            let me = g.sea.mine;
            if let Some(s) = g.sea.ship_mut(me) {
                s.sail = lvl;
                s.sail_shown = lvl as f32;
                let cl = c.classes.get(&s.class).cloned().unwrap_or_default();
                s.speed = if lvl == 2 { cl.speed_full * 0.7 } else { cl.speed_half * 0.6 };
            }
        }
        "teach" => g.teach(arg.unwrap_or("")),
        "wind" => {
            // The wind blows FROM this heading (degrees from north).
            let from: f32 = arg.and_then(|a| a.parse().ok()).unwrap_or(0.0) + crate::game::seed_jitter(g.seed, 12.0);
            let r = from.to_radians();
            let speed = g.sea.wind.length().max(5.5);
            g.sea.wind = -Vec2::new(r.sin(), -r.cos()) * speed;
        }
        "supplies" => {
            // A Round played from its own start has what the Tab would have given by then.
            g.sea.timber = g.sea.timber.max(6);
            g.sea.iron = g.sea.iron.max(2);
        }
        "spawn_flotsam" => {
            // The six crates are placed with the world; Round 1 counts any three.
        }
        "spawn_restraint_patrol" => {
            if g.sea.by_tag("restraint").next().is_none() {
                let route = w.route("restraint_patrol");
                let at = route.first().copied().unwrap_or(Vec2::new(-150.0, 60.0));
                let id = g.sea.add_ship(&c.classes, "schooner", "TS Restraint", "restraint", at, 90.0, Faction::Temperance);
                g.sea.add_captain(id, CaptainKind::Patrol, route, None, false);
            }
        }
        "spawn_gunboats" => {
            let me = g.sea.my_ship().map(|s| (s.pos, s.forward())).unwrap_or((Vec2::ZERO, Vec2::Y));
            for (i, side) in [-1.0f32, 1.0].iter().enumerate() {
                let r = Vec2::new(-me.1.y, me.1.x);
                let at = me.0 + me.1 * 230.0 + r * side * 70.0;
                let id = g.sea.add_ship(
                    &c.classes,
                    "gunboat",
                    &format!("Gunboat {}", i + 1),
                    "gunboat",
                    at,
                    heading(at, me.0),
                    Faction::Temperance,
                );
                g.sea.add_captain(id, CaptainKind::Gunboat, Vec::new(), None, true);
            }
        }
        "restraint_engage" => {
            let refuge = w.marker_xz("battery_west");
            let mine = g.sea.my_ship().map(|s| (s.pos, s.forward())).unwrap_or((Vec2::ZERO, Vec2::Y));
            let found = g.sea.by_tag("restraint").next().map(|s| s.id);
            let id = match found {
                Some(id) => id,
                None => {
                    let at = mine.0 + mine.1 * 260.0;
                    g.sea.add_ship(&c.classes, "schooner", "TS Restraint", "restraint", at, 0.0, Faction::Temperance)
                }
            };
            // She comes about to meet us: within sight, closing.
            if let Some(s) = g.sea.ship_mut(id) {
                if s.pos.distance(mine.0) > 300.0 {
                    s.pos = mine.0 + mine.1 * 260.0 + Vec2::new(mine.1.y, -mine.1.x) * 60.0;
                }
                s.heading = heading(s.pos, mine.0);
            }
            g.sea.add_captain(id, CaptainKind::Schooner, Vec::new(), Some(refuge), true);
        }
        "plunder" => {}
        "end_boarding" => crate::foot::end_boarding(g, sim),
        "start_fort" => {
            if g.sea.batteries.is_empty() {
                let wb = w.marker("battery_west").unwrap_or_default();
                let nb = w.marker("battery_north").unwrap_or_default();
                let mt = w.marker("mortar_tower").unwrap_or_default();
                g.sea.add_battery("west", BatteryKind::Guns, wb, 4, 450.0, 180.0);
                g.sea.add_battery("north", BatteryKind::Guns, nb, 4, 450.0, 180.0);
                g.sea.add_battery("mortar", BatteryKind::Mortar, mt, 1, 500.0, 260.0);
            }
        }
        "storm" => {
            g.set_atmosphere("storm", 60.0);
            g.sea.wind = Vec2::new(-11.0, -6.0);
        }
        "spawn_frigate" => {
            let me = g.sea.my_ship().map(|s| s.pos).unwrap_or_default();
            let at = me + Vec2::new(180.0, 60.0);
            let id = g.sea.add_ship(&c.classes, "frigate", "TS Abstemious", "frigate", at, heading(at, me), Faction::Temperance);
            g.sea.add_captain(id, CaptainKind::Frigate, Vec::new(), None, true);
        }
        "spawn_waterspouts" => {
            let route = w.route("escape");
            for (i, p) in route.iter().enumerate().skip(1).take(3) {
                g.sea.spouts.push(crate::sea::Spout {
                    pos: *p + Vec2::new(40.0 * if i % 2 == 0 { 1.0 } else { -1.0 }, 30.0),
                    vel: Vec2::new(if i % 2 == 0 { -3.0 } else { 3.0 }, -1.5),
                    radius: 14.0,
                    life: 400.0,
                });
            }
        }
        "ship_under_cliffs" => {
            let at = w.marker_xz("under_cliffs");
            place_ship(g, at, 250.0);
            let me = g.sea.mine;
            if let Some(s) = g.sea.ship_mut(me) {
                s.anchored = true;
            }
        }
        // Land actions (towns, guards, the target) live with the land rules.
        other => crate::stealth::act(g, sim, other, arg),
    }
}

/// Evaluates a mission condition.
pub fn eval(g: &BlackTap, sim: &Sim, name: &str) -> bool {
    let w = g.world();
    match name {
        "sails_raised" => g.sea.my_ship().is_some_and(|s| s.sail >= 1),
        "gunboats_sunk" => g.sea.by_tag("gunboat").all(|s| !s.alive()) && g.sea.by_tag("gunboat").next().is_some(),
        "restraint_disabled" => g.sea.by_tag("restraint").any(|s| s.disabled),
        "restraint_disabled_by_chain" => g.flags.contains("restraint_disabled_by_chain"),
        "fort_silent" => !g.sea.batteries.is_empty() && g.sea.batteries.iter().all(|b| b.destroyed),
        "hull_above_half" => g.sea.my_ship().is_some_and(|s| s.frac() > 0.5),
        "at_anchorage" => {
            let a = w.marker_xz("r4_anchorage");
            g.sea.my_ship().is_some_and(|s| s.pos.distance(a) < 45.0)
        }
        "ashore" => {
            let beach = w.marker_xz("r4_beach");
            BlackTap::player_feet(sim)
                .is_some_and(|p| p.y > -0.2 && Vec2::new(p.x, p.z).distance(beach) < 40.0 && w.ground(Vec2::new(p.x, p.z)) > -0.6)
        }
        "at_helm" => g.body == Body::Helm,
        "escaped" => {
            let x = w.marker_xz("extraction");
            g.sea.my_ship().is_some_and(|s| s.pos.distance(x) < 70.0)
        }
        other => crate::stealth::eval(g, sim, other),
    }
}

/// Reacts to what happened at sea.
pub fn sea_events(g: &mut BlackTap, sim: &mut Sim, input: &InputFrame, evs: Vec<SeaEvent>, events: &mut Vec<SimEvent>) {
    let c = g.content();
    for e in evs {
        match e {
            SeaEvent::Flotsam { .. } => {
                g.sea.timber += 2;
                g.sea.iron += 1;
                g.stats.collectibles += 1;
                g.mission.add("flotsam", 1);
                g.dialog.say(&c.dialog, "r1_flotsam_got");
                g.toast("Flotsam: +2 timber, +1 iron");
                g.show("flotsam");
            }
            SeaEvent::Fired { ship, weapon } => {
                if ship == g.sea.mine {
                    g.shake(0.3, (0.9, 0.4, 120));
                    g.show(&weapon);
                }
            }
            SeaEvent::Sunk { ship, by_player } => {
                if by_player {
                    g.stats.ships_sunk += 1;
                    g.dialog.bark(&c.dialog, "crew_cheer", ship as u64);
                }
                g.show("sink");
            }
            SeaEvent::Disabled { ship, by } => {
                if by == "chain" {
                    g.flags.insert("restraint_disabled_by_chain".into());
                }
                let _ = ship;
                g.show("disabled");
            }
            SeaEvent::BatteryDown { name } => {
                g.show("battery_down");
                // Progress is kept: a checkpoint after every battery that falls.
                sim.request_checkpoint(pav_core::sim::CheckpointRequest::Save);
                if g.sea.destroyed_order.first().map(|s| s.as_str()) == Some("mortar") && g.sea.destroyed_order.len() == 1 {
                    g.mission.set_flag("mortar_first");
                } else if g.sea.destroyed_order.len() == 1 {
                    g.mission.set_flag("battery_first");
                }
                g.dialog.say(&c.dialog, "r3_battery_down");
                g.toast(&format!(
                    "{} silenced",
                    match name.as_str() {
                        "west" => "West Battery",
                        "north" => "North Battery",
                        _ => "Mortar Tower",
                    }
                ));
                g.show("battery");
            }
            SeaEvent::PlayerHit { damage } => {
                g.shake((damage / 80.0).min(0.6), (0.7, 0.3, 160));
                if g.mission.flag("escape_started") {
                    g.mission.set_flag("escape_hull_hit");
                }
            }
            SeaEvent::Tagged { .. } => {}
            SeaEvent::Telegraph { .. } => {
                g.dialog.bark(&c.dialog, "ballast_brace", 3);
                g.show("telegraph");
            }
            SeaEvent::Spout => g.show("waterspout"),
        }
    }
    // The player's ship sinks: Spilled.
    if g.body == Body::Helm && g.sea.my_ship().is_some_and(|s| s.hull <= 0.0) && g.mode == Mode::Tab {
        g.spill();
    }
    // Spyglass: hold L1, fire tags the ship at the centre of the view.
    if g.body == Body::Helm && input.down(b::TOOLS) && input.just(b::FIRE) {
        let hint = g.cam.peek(None);
        if let Some(id) = g.sea.spyglass_target(hint.eye, hint.target - hint.eye) {
            if g.sea.tagged.insert(id) {
                let tag = g.sea.ship(id).map(|s| s.tag.clone()).unwrap_or_default();
                if tag == "restraint" {
                    g.mission.set_flag("restraint_tagged");
                }
                events.push(SimEvent::cue("objective_new", g.sea.ship(id).map(|s| s.world(Vec3::Y * 5.0)).unwrap_or_default()));
                g.toast(&format!("Tagged: {}", g.sea.ship(id).map(|s| s.name.clone()).unwrap_or_default()));
                g.show("spyglass");
            }
        }
    }
    // Board: the Restraint disabled, close and slow.
    if g.body == Body::Helm && input.just(b::JUMP) {
        if let Some(prize) = boardable(g) {
            crate::foot::start_boarding(g, sim, prize, events);
            g.learned("board");
            g.show("board");
        }
    }
}

/// The ship the player could board now (disabled, within 25 m, slow).
pub fn boardable(g: &BlackTap) -> Option<u32> {
    let me = g.sea.my_ship()?;
    g.sea
        .ships
        .iter()
        .filter(|s| s.disabled && s.alive() && s.tag == "restraint")
        .find(|s| s.pos.distance(me.pos) < 30.0 && (me.speed - s.speed).abs() < 4.0)
        .map(|s| s.id)
}

/// Every tick after the rules (between-Round beats, free roam convoys).
pub fn post(g: &mut BlackTap, sim: &mut Sim, dt: f32, events: &mut Vec<SimEvent>) {
    let _ = dt;
    crate::stealth::post(g, sim, dt, events);
    if g.mode == Mode::FreeRoam {
        // A Temperance convoy every few minutes (the first half a minute in).
        let first = !g.flags.contains("convoy_seen") && g.time > 30.0;
        if g.sea.ships.iter().filter(|s| s.tag == "convoy" && s.alive()).count() == 0 && ((g.time % 180.0) < dt || first) {
            g.flags.insert("convoy_seen".into());
            let c = g.content();
            let w = g.world();
            let route = w.route("convoy");
            if let Some(&start) = route.first() {
                let id = g.sea.add_ship(&c.classes, "merchant", "Temperance convoy", "convoy", start, 90.0, Faction::Temperance);
                g.sea.add_captain(id, CaptainKind::Merchant, route.clone(), None, false);
                let esc = g.sea.add_ship(
                    &c.classes,
                    "gunboat",
                    "Convoy escort",
                    "convoy",
                    start + Vec2::new(-30.0, 20.0),
                    90.0,
                    Faction::Temperance,
                );
                g.sea.add_captain(esc, CaptainKind::Gunboat, Vec::new(), None, true);
                g.dialog.say(&c.dialog, "fr_convoy");
            }
        }
    }
}

/// The title backdrop: the Black Tap under sail off Isla Templanza at golden hour.
pub fn title_setup(g: &mut BlackTap, _sim: &mut Sim) {
    let w = g.world();
    let at = Vec2::new(40.0, 120.0);
    place_ship(g, at, 80.0);
    let me = g.sea.mine;
    if let Some(s) = g.sea.ship_mut(me) {
        s.sail = 2;
        s.sail_shown = 2.0;
        s.speed = 8.0;
    }
    g.music = "title".into();
    g.set_atmosphere("golden_hour", 0.0);
    let _ = w;
}

/// The title ship sails a slow loop.
pub fn title_helm(g: &BlackTap) -> HelmInput {
    let s = g.sea.my_ship();
    let pos = s.map(|s| s.pos).unwrap_or_default();
    let center = Vec2::new(0.0, 160.0);
    let tangent = {
        let d = (pos - center).normalize_or(Vec2::X);
        Vec2::new(-d.y, d.x)
    };
    let want = pav_kit::naval::heading_of(tangent + (center - pos) * 0.004);
    let rudder = s.map(|s| pav_kit::naval::rudder_for(s, want)).unwrap_or(0.0);
    HelmInput { helm: Helm { sail: 0, rudder, boost: false, anchor: false }, look_yaw: g.cam.yaw, ..Default::default() }
}

/// After the Tab: free roam with the fort flying the Tap flag.
pub fn free_roam_setup(g: &mut BlackTap, sim: &mut Sim) {
    g.mode = Mode::FreeRoam;
    g.set_atmosphere("dawn_after_storm", 0.0);
    g.music = "sailing".into();
    let w = g.world();
    let at = w.marker_xz("r4_anchorage") + Vec2::new(-80.0, 60.0);
    place_ship(g, at, 300.0);
    g.body = Body::Helm;
    g.flags.insert("fort_captured".into());
    crate::stealth::act(g, sim, "spawn_town", None);
    crate::stealth::act(g, sim, "spawn_tavern", None);
    let c = g.content();
    g.dialog.say(&c.dialog, "fr_tavern");
}

/// The Drydock: the developer level (stations by the cay).
pub fn drydock_setup(g: &mut BlackTap, sim: &mut Sim) {
    g.mode = Mode::Drydock;
    crate::drydock::setup(g, sim);
}

/// Jumps into a Round at a named step (its world state set up as if played to there).
pub fn start_at(g: &mut BlackTap, sim: &mut Sim, step: &str) {
    let c = g.content();
    let Some(def) = c.missions.get(g.round as usize - 1).cloned() else { return };
    let Some(i) = def.step.iter().position(|s| s.id == step) else { return };
    // The flags of the steps skipped.
    match (g.round, step) {
        (6, "escape") => {
            for f in ["in_fort", "drye_dead", "last_words_done", "flag_raised"] {
                g.mission.set_flag(f);
            }
            act(g, sim, "ship_under_cliffs");
            g.body = Body::Helm;
            if let Some(s) = g.sea.ship_mut(g.sea.mine) {
                s.anchored = false;
                s.sail = 2;
            }
            g.set_atmosphere("sunset", 0.0);
        }
        (2, "board" | "melee") => {
            for s in g.sea.ships.iter_mut().filter(|s| s.tag == "gunboat") {
                s.sunk = true;
                s.hull = 0.0;
            }
            act(g, sim, "restraint_engage");
            let mine = g.sea.my_ship().map(|s| (s.pos, s.heading, s.forward())).unwrap_or_default();
            let found = g.sea.by_tag("restraint").next().map(|s| s.id);
            if let Some(id) = found {
                if let Some(r) = g.sea.ship_mut(id) {
                    r.pos = mine.0 + Vec2::new(mine.2.y, -mine.2.x) * 16.0 + mine.2 * 4.0;
                    r.heading = mine.1;
                    r.speed = 0.0;
                    r.hull = r.hull_max * 0.2;
                    r.disabled = true;
                    r.sail = 0;
                }
            }
            if let Some(s) = g.sea.ship_mut(g.sea.mine) {
                s.speed = 1.0;
                s.sail = 0;
            }
            g.mission.set_flag("restraint_disabled");
        }
        (4, "chapel" | "survey" | "leap") => {
            let w = g.world();
            g.mission.set_flag("left_helm");
            g.body = Body::Foot;
            let at = if step == "chapel" { w.marker("r4_beach") } else { w.perch("chapel").map(|p| Vec3::from(p.at)) }
                .unwrap_or_default();
            if let Some(pid) = sim.state.player {
                sim.set_position(pid, at + Vec3::Y * 1.2);
            }
            if step == "leap" {
                g.mission.set_flag("surveyed_chapel");
            }
            let anchorage = w.marker_xz("r4_anchorage");
            place_ship(g, anchorage, 300.0);
            if let Some(s) = g.sea.ship_mut(g.sea.mine) {
                s.anchored = true;
            }
        }
        (5, "stocks") => {
            g.mission.set_flag("has_key");
        }
        (5, "key") => {
            // Straight into town (attract): by the plaza, the courier on his way.
            let w = g.world();
            if let (Some(pid), Some(at)) = (sim.state.player, w.marker("r5_close")) {
                sim.set_position(pid, at + Vec3::Y * 0.2);
            }
        }
        (2, "disable") => {
            for s in g.sea.ships.iter_mut().filter(|s| s.tag == "gunboat") {
                s.sunk = true;
                s.hull = 0.0;
            }
            act(g, sim, "restraint_engage");
        }
        (6, "dive") => {
            for f in ["in_fort", "drye_dead", "last_words_done", "flag_raised"] {
                g.mission.set_flag(f);
            }
            g.flags.insert("fort_captured".into());
            g.stealth.flag_raised = true;
            act(g, sim, "ship_under_cliffs");
            let w = g.world();
            if let (Some(pid), Some(at)) = (sim.state.player, w.marker("battlement")) {
                sim.set_position(pid, at + Vec3::Y * 0.3);
            }
            g.set_atmosphere("sunset", 0.0);
        }
        (6, "drye" | "last_words" | "flag") => {
            let w = g.world();
            g.mission.set_flag("in_fort");
            let at = w.marker(if step == "drye" { "cellar" } else { "flagpole" }).unwrap_or_default();
            if let Some(pid) = sim.state.player {
                sim.set_position(pid, at + Vec3::Y * 1.2);
            }
            if step != "drye" {
                g.mission.set_flag("drye_dead");
                g.mission.set_flag("last_words_done");
                for n in g.stealth.npcs.values_mut().filter(|n| n.kind == "drye") {
                    n.dead = true;
                }
            }
        }
        _ => {}
    }
    let (acts, _) = {
        let mut acts = Vec::new();
        g.mission.step = i;
        g.mission.step_time = 0.0;
        acts.extend(def.step[i].on_start.clone());
        (acts, ())
    };
    g.perform(sim, acts);
}

/// The helm is let go (Round 4): the ship anchors and the captain stands on the deck.
pub fn leave_helm(g: &mut BlackTap, sim: &mut Sim) {
    let c = g.content();
    let Some(ship) = g.sea.my_ship().cloned() else { return };
    let class = c.classes.get(&ship.class).cloned().unwrap_or_default();
    let me = g.sea.mine;
    if let Some(s) = g.sea.ship_mut(me) {
        s.sail = 0;
        s.anchored = true;
    }
    g.body = Body::Foot;
    g.land.deck = Some(crate::foot::spawn_ship_deck(sim, &ship, &class));
    let feet = ship.world(Vec3::new(0.0, class.freeboard + 0.1, -class.length * 0.3));
    if let Some(pid) = sim.state.player {
        sim.set_position(pid, feet);
    }
    g.mission.set_flag("left_helm");
    g.learned("leave_helm");
    g.show("leave_helm");
}

/// The Tab is done: the results screen (the overlay handles the choice).
pub fn tab_complete(g: &mut BlackTap, _sim: &mut Sim) {
    g.flags.insert("tab_complete".into());
    g.best_round = 6;
    g.save_for = 1.0;
    g.music = "results".into();
    g.set_atmosphere("dawn_after_storm", 8.0);
    g.results = Some(0);
}

/// The results overlay's choice (from the game's input).
pub fn results_input(g: &mut BlackTap, sim: &mut Sim, input: &InputFrame) -> Option<UiRequest> {
    let focus = g.results?;
    let f = g.cam.forward();
    let r = Vec2::new(-f.y, f.x);
    let x = input.move_dir.dot(r);
    let mut focus = focus;
    if x > 0.6 && g.stick_prev <= 0.6 {
        focus = (focus + 1) % 3;
    } else if x < -0.6 && g.stick_prev >= -0.6 {
        focus = (focus + 2) % 3;
    }
    g.stick_prev = x;
    g.results = Some(focus);
    if input.just(b::JUMP) || input.just(b::INTERACT) {
        g.results = None;
        return match focus {
            0 => {
                free_roam_setup(g, sim);
                None
            }
            1 => Some(UiRequest::Replay),
            _ => Some(UiRequest::Title),
        };
    }
    None
}

/// Pour % for the Tab: the main path is 70%, the optional objectives 30%.
pub fn pour(g: &BlackTap) -> f32 {
    let main = g.stats.rounds_done as f32 / 6.0 * 70.0;
    let opt = if g.stats.optional_total > 0 { g.stats.optional_done as f32 / g.stats.optional_total as f32 * 30.0 } else { 0.0 };
    (main + opt).clamp(0.0, 100.0)
}

/// Every collectible in the level: the islands' and the flotsam at sea.
pub fn collectibles_total(g: &BlackTap) -> usize {
    let w = g.world();
    w.collectibles.len() + w.markers.keys().filter(|k| k.starts_with("flotsam_")).count()
}

pub fn apply_upgrade(g: &mut BlackTap, up: &str) {
    let me = g.sea.mine;
    match up {
        "reinforced_hull" => {
            if let Some(s) = g.sea.ship_mut(me) {
                s.hull_max += 300.0;
                s.hull += 300.0;
            }
        }
        "heavy_shot" => g.sea.heavy_shot += 0.35,
        "swift_rigging" => g.sea.swift_rigging = true,
        "more_kegs" => {
            if let Some(s) = g.sea.ship_mut(me) {
                s.guns.kegs_left += 3;
            }
        }
        _ => {}
    }
    g.flags.insert(format!("upgrade_{up}"));
    g.toast(&format!("Upgrade: {}", up.replace('_', " ")));
}

/// A kill happened (fights, assassinations): mission bookkeeping.
pub fn on_kill(g: &mut BlackTap, sim: &mut Sim, id: EntityId, how: Option<Assassination>) {
    crate::stealth::on_kill(g, sim, id, how);
}

/// Adds one to a mission counter.
pub fn counted(g: &mut BlackTap, name: &str) {
    g.mission.add(name, 1);
}

/// Progress to save: Rounds done, best Pour, collectibles, upgrades.
pub fn progress(g: &BlackTap) -> serde_json::Value {
    json!({
        "version": 1,
        "rounds_done": g.stats.rounds_done.max(if g.flags.contains("tab_complete") { 6 } else { 0 }),
        "round": g.round,
        "pour": pour(g),
        "collectibles": g.stats.collectibles,
        "upgrades": g.flags.iter().filter(|f| f.starts_with("upgrade_")).collect::<Vec<_>>(),
        "reales": g.stats.reales,
        "best_round": g.best_round,
        // Where Continue takes the player (the app reads this key).
        "continue_scene": if g.best_round >= 6 { "blacktap/freeroam".to_string() } else { format!("blacktap/round/{}", g.best_round + 1) },
    })
}

pub fn restore_progress(g: &mut BlackTap, p: &serde_json::Value) {
    let best = p.get("best_round").and_then(|b| b.as_u64()).unwrap_or(0).min(6) as u8;
    g.best_round = g.best_round.max(best);
    if let Some(ups) = p.get("upgrades").and_then(|u| u.as_array()) {
        for u in ups.iter().filter_map(|u| u.as_str()) {
            if let Some(name) = u.strip_prefix("upgrade_") {
                if g.demo.is_none() && !g.flags.contains(u) {
                    apply_upgrade(g, name);
                    g.toasts.clear();
                }
            }
        }
    }
}
