//! The game's tool commands (`Game::command`): `pav ship`, `mission`, `perception`, `ocean`,
//! `level_check` and `tour`. They read and nudge the game for agents and tests; nothing here
//! runs during normal play.

use std::collections::BTreeSet;

use glam::{Vec2, Vec3};
use pav_core::sim::Sim;
use pav_kit::mission::Act;
use serde_json::{Value, json};

use crate::game::{BlackTap, Body, b, round2};

type Reply = Option<Result<Value, String>>;

pub fn command(g: &mut BlackTap, sim: &mut Sim, name: &str, args: &Value) -> Reply {
    Some(match name {
        "ship" => ship(g, args),
        "mission" => mission(g, sim, args),
        "perception" => Ok(crate::stealth::perception_report(g, sim, args.get("name").and_then(|v| v.as_str()))),
        "ocean" => ocean(g, args),
        "level_check" => Ok(level_check(g)),
        "tour" => tour(g, sim, args),
        "fighters" => {
            let me = sim.state.player.and_then(|id| sim.feet(id)).unwrap_or_default();
            Ok(json!({
                "hero": { "hp": round2(g.land.hero.vitals.hp), "attack": g.land.hero.attack.as_ref().map(|a| (a.0.0, round2(a.1), round2(a.2), a.3.clone())) },
                "fighters": g.land.fighters.iter().filter_map(|(id, f)| {
                    let p = sim.feet(*id)?;
                    Some(json!({
                        "id": id.0, "kind": f.kind, "team": f.team, "hp": round2(f.vitals.hp), "dead": f.vitals.dead,
                        "idle": f.idle, "target": f.target.map(|t| t.0), "swing": f.swing.is_some(), "cooldown": round2(f.cooldown),
                        "stagger": round2(f.vitals.stagger), "pos": [round2(p.x), round2(p.y), round2(p.z)],
                        "distance": round2(p.distance(me)),
                    }))
                }).collect::<Vec<_>>(),
            }))
        }
        "markers" => {
            let w = g.world();
            let f = arg_str(args, "name").unwrap_or("");
            Ok(json!(
                w.markers
                    .iter()
                    .filter(|(k, _)| k.contains(f))
                    .map(|(k, v)| (k.clone(), [round2(v.x), round2(v.y), round2(v.z)]))
                    .collect::<std::collections::BTreeMap<_, _>>()
            ))
        }
        _ => return None,
    })
}

fn arg_str<'a>(a: &'a Value, k: &str) -> Option<&'a str> {
    a.get(k).and_then(|v| v.as_str())
}

fn arg_f32(a: &Value, k: &str) -> Option<f32> {
    a.get(k).and_then(|v| v.as_f64().or_else(|| v.as_str().and_then(|s| s.parse().ok()))).map(|v| v as f32)
}

fn ship(g: &mut BlackTap, a: &Value) -> Result<Value, String> {
    let me = g.sea.mine;
    match arg_str(a, "do") {
        None => {}
        Some("sail") => {
            let lvl = arg_f32(a, "level").unwrap_or(1.0).clamp(0.0, 3.0) as u8;
            let s = g.sea.ship_mut(me).ok_or("no ship")?;
            s.sail = lvl;
        }
        Some("steer") => {
            let deg = arg_f32(a, "deg").ok_or("steer needs deg=")?;
            let s = g.sea.ship_mut(me).ok_or("no ship")?;
            s.heading = deg.rem_euclid(360.0);
        }
        Some("fire") => {
            let side = match arg_str(a, "side").unwrap_or("port") {
                "port" => pav_kit::naval::Side::Port,
                "starboard" => pav_kit::naval::Side::Starboard,
                other => return Err(format!("side is port or starboard (not '{other}')")),
            };
            g.cam.yaw = g.sea.look_for_side(side);
            g.tool_press |= b::FIRE;
        }
        Some("place") => {
            let (x, z) = (arg_f32(a, "x").ok_or("place needs x=")?, arg_f32(a, "z").ok_or("place needs z=")?);
            let s = g.sea.ship_mut(me).ok_or("no ship")?;
            s.pos = glam::Vec2::new(x, z);
            if let Some(deg) = arg_f32(a, "deg") {
                s.heading = deg.rem_euclid(360.0);
            }
            s.speed = 0.0;
        }
        Some("board") => g.tool_press |= b::INTERACT,
        Some("brace") => g.tool_press |= b::INTERACT,
        Some("repair") => g.tool_press |= b::HEAL,
        // Effects checks: set the nearest other ship alight, or send her down.
        Some(what @ ("ignite" | "sink")) => {
            let at = g.sea.my_ship().map(|s| s.pos).ok_or("no ship")?;
            let id = g
                .sea
                .ships
                .iter()
                .filter(|o| o.id != me && o.alive())
                .min_by(|a, b| a.pos.distance(at).total_cmp(&b.pos.distance(at)))
                .map(|o| o.id)
                .ok_or("no other ship afloat")?;
            let o = g.sea.ship_mut(id).ok_or("no ship")?;
            if what == "ignite" {
                o.hull = o.hull.min(o.hull_max * 0.45);
                o.fire = 12.0;
            } else {
                o.hull = 0.0;
                o.sinking = 0.001;
            }
        }
        Some(other) => {
            return Err(format!("do is sail, steer, place, fire, board, brace, repair, ignite or sink (not '{other}')"));
        }
    }
    let s = g.sea.my_ship().ok_or("no ship")?;
    Ok(json!({
        "name": s.name, "class": s.class, "pos": [round2(s.pos.x), round2(s.pos.y)], "heading": round2(s.heading),
        "speed": round2(s.speed), "sail": s.sail, "hull": round2(s.hull), "hull_max": round2(s.hull_max),
        "sails": round2(s.sails), "crew": s.crew, "anchored": s.anchored, "timber": g.sea.timber, "iron": g.sea.iron,
        "weapon": g.sea.weapon.map(|w| format!("{w:?}")), "at_helm": g.body == Body::Helm, "pending_press": g.tool_press,
        "others": g.sea.ships.iter().filter(|o| o.id != me && !o.sunk).map(|o| json!({
            "id": o.id, "tag": o.tag, "class": o.class, "hull": round2(o.hull), "disabled": o.disabled,
            "distance": round2(o.pos.distance(s.pos)),
        })).collect::<Vec<_>>(),
    }))
}

fn mission(g: &mut BlackTap, sim: &mut Sim, a: &Value) -> Result<Value, String> {
    if let Some(to) = arg_str(a, "goto") {
        let rest = to.strip_prefix("round/").ok_or("goto=round/N[@step]")?;
        let (n, step) = rest.split_once('@').map(|(n, s)| (n, Some(s))).unwrap_or((rest, None));
        let n: u8 = n.parse().map_err(|_| format!("bad round '{n}'"))?;
        if !(1..=6).contains(&n) {
            return Err("rounds are 1..6".into());
        }
        g.outcome = pav_core::games::Outcome::Playing;
        g.between = None;
        g.start_round(sim, n, true, step);
    }
    if let Some(step) = arg_str(a, "complete") {
        let c = g.content();
        let def = c.missions.get(g.round as usize - 1).ok_or("no mission")?;
        let cur = g.mission.current(def).map(|s| s.id.clone()).unwrap_or_default();
        if cur != step && step != "current" {
            return Err(format!("the current step is '{cur}', not '{step}'"));
        }
        g.mission.forced = true;
    }
    let c = g.content();
    let def = c.missions.get(g.round as usize - 1);
    Ok(json!({
        "round": g.round,
        "title": def.map(|d| d.title.clone()),
        "steps": def.map(|d| d.step.iter().map(|s| s.id.clone()).collect::<Vec<_>>()),
        "step": def.and_then(|d| g.mission.current(d)).map(|s| s.id.clone()),
        "objective": def.and_then(|d| g.mission.current(d)).map(|s| s.objective.clone()),
        "step_time": round2(g.mission.step_time),
        "complete": g.mission.complete,
        "failed": g.mission.failed,
        "flags": g.mission.flags,
        "counters": g.mission.counters,
        "optional": def.map(|d| g.mission.checklist(d)),
        "rounds_done": g.stats.rounds_done,
        "outcome": g.outcome,
    }))
}

fn ocean(g: &mut BlackTap, a: &Value) -> Result<Value, String> {
    if let Some(p) = arg_str(a, "preset") {
        let c = g.content();
        let name = match p {
            "flat" | "calm" => "golden_hour",
            "breezy" => "afternoon",
            "rough" => "dawn_after_storm",
            "storm" => "storm",
            other if c.atmospheres.contains_key(other) => other,
            other => return Err(format!("unknown sea preset or atmosphere '{other}'")),
        };
        g.set_atmosphere(name, 0.01);
    }
    if let Some(deg) = arg_f32(a, "wind") {
        let speed = g.sea.wind.length().max(4.0);
        let r = deg.to_radians();
        g.sea.wind = Vec2::new(r.sin(), -r.cos()) * speed;
    }
    let o = g.ocean();
    // In a storm: seconds until the next lightning strike and where it lands from the player
    // (with the camera as it is now), to step there and see it.
    let rain = crate::present::atmosphere(g).rain;
    let next = (1..60 * 12).map(|k| k as f32 / 60.0).find_map(|dt| {
        crate::vfx::lightning(g.seed, rain, glam::Vec3::ZERO, g.cam.yaw, g.time + dt)
            .filter(|s| s.age < 1.0 / 60.0)
            .map(|s| json!({ "in": round2(dt), "offset": [round2(s.pos.x), round2(s.pos.z)] }))
    });
    Ok(json!({
        "atmosphere": g.atmos_to, "wind": [round2(g.sea.wind.x), round2(g.sea.wind.y)], "max_height": round2(o.sea.max_height()),
        "foam": round2(o.sea.foam), "next_lightning": next,
    }))
}

/// Every reference in the game's data that names something else: markers, routes, dialog
/// lines, atmospheres, songs, game conditions and actions, demo scenes, camera contexts.
pub fn level_check(g: &BlackTap) -> Value {
    let c = g.content();
    let w = g.world();
    let mut problems: Vec<String> = Vec::new();
    let mut checked = 0usize;
    let lines: BTreeSet<String> = c.dialog.line.keys().cloned().collect();
    for m in &c.missions {
        for s in &m.step {
            for mk in &s.markers {
                checked += 1;
                let (kind, name) = mk.split_once(':').unwrap_or((mk.as_str(), ""));
                let ok = match kind {
                    "marker" => w.marker(name).is_some(),
                    "ship" | "tag" => !name.is_empty(),
                    "npc" => w.npcs.iter().any(|n| n.kind == name || n.tags.iter().any(|t| t == name)),
                    "battery" => ["west", "north", "mortar"].contains(&name),
                    "flotsam" => true,
                    _ => false,
                };
                if !ok {
                    problems.push(format!("{}: step {} shows unknown marker '{mk}'", m.id, s.id));
                }
            }
        }
        for l in m.lines() {
            checked += 1;
            if !lines.contains(&l) {
                problems.push(format!("{}: unknown dialog line '{l}'", m.id));
            }
        }
        let (conds, acts) = m.game_names();
        for cnd in conds {
            checked += 1;
            if !crate::rounds::known_cond(&cnd) {
                problems.push(format!("{}: unknown game condition '{cnd}'", m.id));
            }
        }
        for act in acts {
            checked += 1;
            if !crate::rounds::known_act(&act) {
                problems.push(format!("{}: unknown game action '{act}'", m.id));
            }
            if let Some(("place_ship" | "player_at", arg)) = act.split_once(':') {
                if w.marker(arg).is_none() {
                    problems.push(format!("{}: action '{act}' names an unknown marker", m.id));
                }
            }
        }
        let mut all_acts: Vec<&Act> = m.setup.iter().chain(&m.on_start).chain(&m.on_complete).collect();
        for s in &m.step {
            all_acts.extend(s.on_start.iter().chain(&s.on_done));
        }
        for a in all_acts {
            if let Act::Atmosphere(n) = a {
                checked += 1;
                if !c.atmospheres.contains_key(n) {
                    problems.push(format!("{}: unknown atmosphere '{n}'", m.id));
                }
            }
        }
    }
    for d in &c.demo {
        checked += 1;
        if pav_core::Sim::new(&d.scene, d.seeds.first().copied().unwrap_or(1)).is_err() {
            problems.push(format!("demo '{}': scene '{}' does not build", d.name, d.scene));
        }
    }
    // The geometry file built (a parse error is only logged at load, and the level would have
    // no islands or buildings at all).
    let pre = crate::island::prebuilt_file(&w.geometry);
    checked += 1;
    if pre.fields.is_empty() && pre.structures.blocks.is_empty() {
        problems.push(format!("geometry '{}' built nothing (a parse error? see the log)", w.geometry));
    }
    // Nothing built across a road (a fence or a hut that blocks the way).
    for f in &pre.fields {
        for r in &f.def.roads {
            let pts: Vec<glam::Vec2> = r.points.iter().map(|p| glam::Vec2::from(*p)).collect();
            // Gates close a road on purpose.
            for fp in pre.structures.footprints.iter().filter(|fp| !fp.name.contains("gate")) {
                checked += 1;
                let on = pts.windows(2).any(|s| {
                    (0..=20).any(|k| {
                        let q = s[0].lerp(s[1], k as f32 / 20.0);
                        fp.distance(q) < r.width * 0.5 - 0.5
                    })
                });
                if on {
                    problems.push(format!("structure '{}' stands on a road of {}", fp.name, f.name));
                }
            }
        }
    }
    let kinds = crate::looks::names();
    for n in &w.npcs {
        checked += 1;
        if !kinds.contains(&n.kind) {
            problems.push(format!("npc in group '{}' has unknown archetype '{}'", n.group, n.kind));
        }
        if !n.route.is_empty() && w.route(&n.route).is_empty() {
            problems.push(format!("npc '{}' walks unknown route '{}'", n.kind, n.route));
        }
    }
    for p in &w.perches {
        checked += 1;
        if !p.leap_to.is_empty() && w.marker(&p.leap_to).is_none() {
            problems.push(format!("perch '{}' leaps to unknown marker '{}'", p.name, p.leap_to));
        }
    }
    for (name, route) in &w.routes {
        checked += 1;
        if route.len() < 2 {
            problems.push(format!("route '{name}' has fewer than two points"));
        }
    }
    for ctx in ["foot", "freerun", "climb", "crouch", "combat", "aim", "swim", "helm", "broadside", "spyglass", "title"] {
        checked += 1;
        if !c.camera.0.contains_key(ctx) {
            problems.push(format!("camera context '{ctx}' is missing from camera.toml"));
        }
    }
    json!({ "checked": checked, "problems": problems, "ok": problems.is_empty() })
}

/// The canonical vistas (PLAN §9.17 tour sheet, §9.18 viewpoints): a list, one shot set up
/// (`shot=N`), or the shot made ready after its ticks (`shot=N frame=true`: its atmosphere
/// settled, whatever a Round's script did meanwhile, and the camera framed on what moves).
/// The first six are the performance viewpoints (`pav tour stats=true`). Shots without
/// `eye`/`look` use the game's own camera.
fn tour(g: &mut BlackTap, _sim: &mut Sim, a: &Value) -> Result<Value, String> {
    let w = g.world();
    let m = |n: &str| w.marker(n).unwrap_or_default();
    let town = m("town_square");
    let stocks = m("stocks");
    let shots = vec![
        json!({ "name": "Helm at sea", "scene": "blacktap/round/2", "atmosphere": "golden_hour", "ticks": 240 }),
        json!({ "name": "Dryport street", "scene": "blacktap/round/5", "atmosphere": "afternoon", "ticks": 240,
                "eye": [town.x + 24.0, 6.5, town.z + 6.0], "look": [town.x, 3.0, town.z] }),
        json!({ "name": "Rooftops of Dryport", "scene": "blacktap/round/5", "atmosphere": "golden_hour", "ticks": 240,
                "eye": [town.x + 66.0, 14.0, town.z - 10.0], "look": [town.x, 2.0, town.z] }),
        json!({ "name": "Fort Abstinence courtyard", "scene": "blacktap/round/6", "atmosphere": "afternoon", "ticks": 240,
                "eye": [338.0, 19.5, -178.0], "look": [348.0, 17.0, -214.0] }),
        json!({ "name": "The jungle", "scene": "blacktap/round/6", "atmosphere": "afternoon", "ticks": 240,
                "eye": [206.0, 33.0, -52.0], "look": [236.0, 31.0, -92.0] }),
        json!({ "name": "The storm", "scene": "blacktap/round/6@escape", "atmosphere": "storm", "ticks": 240 }),
        json!({ "name": "Ships of the line", "scene": "blacktap/round/2", "atmosphere": "afternoon", "ticks": 120,
                "frame": "ships" }),
        json!({ "name": "Cask, guards and townsfolk", "scene": "blacktap/round/5", "atmosphere": "afternoon", "ticks": 240,
                "eye": [stocks.x + 7.0, stocks.y + 1.8, stocks.z + 7.0], "look": [stocks.x - 1.0, stocks.y, stocks.z - 2.0],
                "fov": 50.0 }),
        json!({ "name": "Isla Templanza from the sea", "scene": "blacktap/freeroam", "atmosphere": "golden_hour",
                "eye": [town.x - 260.0, 40.0, town.z + 160.0], "look": [town.x, 20.0, town.z] }),
    ];
    if let Some(i) = a.get("shot").and_then(|v| v.as_u64()) {
        let s = shots.get(i as usize).ok_or("no such shot")?;
        if a.get("frame").and_then(|v| v.as_bool()) == Some(true) {
            if let Some(at) = s["atmosphere"].as_str() {
                g.atmos_from = at.to_string();
                g.atmos_to = at.to_string();
                g.atmos_t = 1.0;
            }
            return Ok(match s["frame"].as_str() {
                Some("ships") => frame_ships(g),
                _ => json!({}),
            });
        }
        if let Some(at) = s["atmosphere"].as_str() {
            g.set_atmosphere(at, 0.01);
        }
        return Ok(json!({ "atmosphere": s["atmosphere"] }));
    }
    Ok(json!({ "shots": shots }))
}

/// A camera on the nearest other ship: abeam and a little aft of it, 45 m off, on the side
/// away from ours.
fn frame_ships(g: &BlackTap) -> Value {
    let Some(me) = g.sea.my_ship() else { return json!({}) };
    let Some(them) = g
        .sea
        .ships
        .iter()
        .filter(|s| s.id != me.id && s.alive())
        .min_by(|a, b| a.pos.distance(me.pos).total_cmp(&b.pos.distance(me.pos)))
    else {
        return json!({});
    };
    let side = if them.right().dot(me.pos - them.pos) > 0.0 { -1.0 } else { 1.0 };
    let eye = them.pos + them.right() * (45.0 * side) - them.forward() * 14.0;
    let look = them.world(Vec3::new(0.0, 7.0, 0.0));
    json!({ "eye": [eye.x, 9.0, eye.y], "look": [look.x, look.y, look.z] })
}
