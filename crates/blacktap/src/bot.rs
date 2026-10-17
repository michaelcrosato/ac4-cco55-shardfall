//! The bot: the test player and the attract-mode player. Layers: the director reads the
//! Round's current step (its `bot` skill name in the mission data) → a skill controller that
//! emits an `InputFrame` like a player's (stick in the camera's frame, `look` for the camera,
//! button presses). The `showcase` personality (attract mode) adds flourishes.

use glam::{Vec2, Vec3};
use pav_core::input::InputFrame;
use pav_core::sim::Sim;
use pav_kit::camera::angle_diff;
use pav_kit::naval::{Faction, Side, Weapon, heading_of, turn_to, weapon_for_bearing};

use crate::game::{BlackTap, Body, Mode, b};

/// A stick axis (x right, y forward) as the move direction the app would send for this camera.
fn stick(g: &BlackTap, axis: Vec2) -> Vec2 {
    g.cam.relative(axis.clamp_length_max(1.0))
}

/// Turns the camera toward a yaw (and pitch), as a player's right stick would.
fn look_toward(g: &BlackTap, yaw: f32, pitch: Option<f32>) -> Vec2 {
    let dy = angle_diff(yaw, g.cam.yaw).clamp(-7.0, 7.0);
    let dp = pitch.map(|p| (g.cam.pitch - p).clamp(-3.0, 3.0)).unwrap_or(0.0);
    Vec2::new(dy, dp)
}

/// A rudder (-1..1) toward a heading.
fn steer(g: &BlackTap, want: f32) -> f32 {
    let h = g.sea.my_ship().map(|s| s.heading).unwrap_or(0.0);
    (turn_to(h, want) / 20.0).clamp(-1.0, 1.0)
}

/// Presses `bit` on alternate ticks (a held-down button never "just" presses again).
fn tap(sim: &Sim, every: u64) -> bool {
    sim.state.tick % every.max(2) == 0
}

pub fn input(g: &BlackTap, sim: &Sim) -> InputFrame {
    let mut i = InputFrame::default();
    if g.mode == Mode::Title || g.spilled.is_some() {
        return i;
    }
    if g.results.is_some() {
        if tap(sim, 30) {
            i.pressed |= b::JUMP;
            i.held |= b::JUMP;
        }
        return i;
    }
    if let Some(bd) = &g.land.boarding {
        return match bd.phase {
            crate::foot::BoardPhase::Swing => i,
            crate::foot::BoardPhase::Plunder => {
                // Repair the hull, then Heavy Shot.
                let want = if bd.stage == 0 { 0 } else { 1 };
                if bd.focus != want && tap(sim, 12) {
                    i.move_dir = stick(g, Vec2::new(1.0, 0.0));
                } else if bd.focus == want && tap(sim, 20) {
                    i.pressed |= b::JUMP;
                    i.held |= b::JUMP;
                }
                i
            }
            crate::foot::BoardPhase::Fight => melee(g, sim),
        };
    }
    let c = g.content();
    let skill = c
        .missions
        .get(g.round as usize - 1)
        .and_then(|d| g.mission.current(d))
        .map(|s| s.bot.clone())
        .unwrap_or_default();
    match g.body {
        Body::Helm => sea(g, sim, &skill),
        Body::Foot => crate::stealth::bot(g, sim, &skill),
    }
}

/// At the helm.
fn sea(g: &BlackTap, sim: &Sim, skill: &str) -> InputFrame {
    let mut i = InputFrame::default();
    let Some(me) = g.sea.my_ship() else { return i };
    let w = g.world();
    let (head, arg) = skill.split_once(':').unwrap_or((skill, ""));
    let mut axis = Vec2::ZERO;
    let mut want_sail = 2u8;
    let mut want: Option<f32> = None;
    // Repair when hurt.
    if me.frac() < 0.4 && g.sea.timber > 0 && g.sea.repair_cd <= 0.0 && tap(sim, 10) {
        i.pressed |= b::HEAL;
        i.held |= b::HEAL;
    }
    // Brace when a volley is coming.
    let threat = g.sea.ships.iter().any(|s| s.telegraph > 0.0 && s.telegraph < 1.2 && s.pos.distance(me.pos) < 170.0)
        || g.sea.batteries.iter().any(|b| b.telegraph > 0.0 && b.telegraph < 0.8);
    if threat && me.brace_cd <= 0.0 {
        i.pressed |= b::INTERACT;
        i.held |= b::INTERACT;
    }
    match head {
        "raise_sails" => {
            want = Some(heading_of(w.marker_xz("flotsam_1") - me.pos));
        }
        "collect_flotsam" => {
            let near = g
                .sea
                .floating
                .iter()
                .filter(|f| f.kind == crate::sea::FloatKind::Flotsam)
                .min_by(|a, b| a.pos.distance(me.pos).total_cmp(&b.pos.distance(me.pos)));
            if let Some(f) = near {
                want = Some(heading_of(f.pos - me.pos));
                if f.pos.distance(me.pos) < 60.0 {
                    want_sail = 1;
                }
            }
        }
        "tag_restraint" => {
            if let Some(r) = g.sea.by_tag("restraint").next() {
                let d = r.pos - me.pos;
                let bearing = heading_of(d);
                want = Some(bearing);
                if d.length() < 520.0 {
                    i.held |= b::TOOLS;
                    // Aim the glass at her.
                    let eye_h: f32 = 4.5;
                    let pitch = (eye_h - 4.0).atan2(d.length()).to_degrees();
                    i.look = look_toward(g, bearing, Some(pitch.clamp(-5.0, 5.0)));
                    if angle_diff(bearing, g.cam.yaw).abs() < 3.0 && tap(sim, 8) {
                        i.pressed |= b::FIRE;
                        i.held |= b::FIRE;
                    }
                }
            }
        }
        "fight_ships" => {
            let target = g
                .sea
                .ships
                .iter()
                .filter(|s| s.alive() && s.faction == Faction::Temperance && !s.disabled)
                .filter(|s| s.captain.as_ref().is_some_and(|c| c.hostile))
                .min_by(|a, b| a.pos.distance(me.pos).total_cmp(&b.pos.distance(me.pos)));
            if let Some(t) = target {
                let (h, look, fire) = duel(g, sim, me, t.pos, t.forward() * t.speed, t.tag == "gunboat");
                want = Some(h);
                i.look = look;
                if fire {
                    i.pressed |= b::FIRE;
                    i.held |= b::FIRE;
                }
            }
        }
        "board" => {
            if let Some(r) = g.sea.by_tag("restraint").find(|s| s.alive()) {
                let d = r.pos - me.pos;
                want = Some(heading_of(d));
                want_sail = if d.length() > 24.0 { 1 } else { 0 };
                if crate::rounds::boardable(g).is_some() && tap(sim, 6) {
                    i.pressed |= b::JUMP;
                    i.held |= b::JUMP;
                }
                if d.length() < 20.0 {
                    want = Some(me.heading);
                }
            }
        }
        "bombard" => {
            let target = g.sea.batteries.iter().filter(|b| !b.destroyed).min_by(|a, b| {
                let da = Vec2::new(a.pos.x, a.pos.z).distance(me.pos);
                let db = Vec2::new(b.pos.x, b.pos.z).distance(me.pos);
                // Mortar first (the Full Pour), then the nearest.
                let ka = if a.name == "mortar" { 0.0 } else { 1.0 };
                let kb = if b.name == "mortar" { 0.0 } else { 1.0 };
                (ka, da).partial_cmp(&(kb, db)).unwrap()
            });
            if let Some(t) = target {
                let tp = Vec2::new(t.pos.x, t.pos.z);
                let (h, look, fire) = duel_at(g, sim, me, tp, (175.0, 215.0));
                want = Some(h);
                i.look = look;
                if fire {
                    i.pressed |= b::FIRE;
                    i.held |= b::FIRE;
                }
                want_sail = 1;
            }
        }
        "sail_to" => {
            let goal = w.marker_xz(arg);
            let route = match arg {
                "r4_anchorage" => w.route("r4_approach"),
                "extraction" => w.route("escape"),
                _ => Vec::new(),
            };
            // The first waypoint still ahead of us along the route.
            let next = route
                .iter()
                .enumerate()
                .filter(|(k, p)| {
                    let to_goal = goal.distance(**p);
                    to_goal < goal.distance(me.pos) - 10.0 && (*k + 1 == route.len() || p.distance(me.pos) > 40.0)
                })
                .map(|(_, p)| *p)
                .next()
                .unwrap_or(goal);
            want = Some(heading_of(next - me.pos));
            let d = goal.distance(me.pos);
            want_sail = if d < 50.0 && arg == "r4_anchorage" { 0 } else if d < 140.0 && arg == "r4_anchorage" { 1 } else { 2 };
            // Steer round waterspouts.
            for sp in &g.sea.spouts {
                let ahead = me.pos + me.forward() * 40.0;
                if ahead.distance(sp.pos) < sp.radius + 25.0 {
                    let away = heading_of(me.pos - sp.pos);
                    want = Some((want.unwrap_or(me.heading) * 0.3 + away * 0.7).rem_euclid(360.0));
                }
            }
            if arg == "extraction" {
                i.pressed |= if tap(sim, 240) { b::SPRINT } else { 0 };
            }
        }
        "leave_helm" => {
            want_sail = 0;
            if me.speed.abs() <= 2.4 {
                i.held |= b::INTERACT;
            }
            want = Some(me.heading);
        }
        _ => {}
    }
    // Dodge mortar rings: steer away and boost.
    for sh in &g.sea.shells {
        let future = me.pos + me.forward() * me.speed * sh.left;
        let at = Vec2::new(sh.at.x, sh.at.z);
        if future.distance(at) < sh.radius + 6.0 || me.pos.distance(at) < sh.radius + 4.0 {
            let away = heading_of(me.pos - at);
            want = Some(away);
            want_sail = 2;
            if tap(sim, 30) {
                i.pressed |= b::SPRINT;
            }
        }
    }
    // Keep off the shallows: look ahead along the heading.
    if let Some(h) = want {
        let safe = pav_kit::naval::safe_heading(me.pos, h, (me.speed * 5.0).max(45.0), 3.5, &|p| w.ground(p));
        axis.x = steer(g, safe);
    }
    // Sails: step toward the wanted level on alternate ticks (edges).
    if me.sail < want_sail && tap(sim, 2) {
        axis.y = 1.0;
    } else if me.sail > want_sail && tap(sim, 2) {
        axis.y = -1.0;
    }
    i.move_dir = stick(g, axis);
    i
}

/// A broadside duel against something at `tp` moving at `tv`: the heading to hold, the
/// camera turn (look to the side that will fire) and whether to fire now.
fn duel(g: &BlackTap, sim: &Sim, me: &pav_kit::naval::Vessel, tp: Vec2, _tv: Vec2, small: bool) -> (f32, Vec2, bool) {
    // Keep the target on the beam at 70–140 m (closer for small boats).
    duel_at(g, sim, me, tp, if small { (45.0, 100.0) } else { (75.0, 140.0) })
}

fn duel_at(g: &BlackTap, sim: &Sim, me: &pav_kit::naval::Vessel, tp: Vec2, range: (f32, f32)) -> (f32, Vec2, bool) {
    let to = tp - me.pos;
    let dist = to.length();
    let bearing = heading_of(to);
    let rel = turn_to(me.heading, bearing);
    let (lo, hi) = range;
    let side = if rel >= 0.0 { 1.0 } else { -1.0 };
    let lean = if dist < lo { -25.0 } else if dist > hi { 35.0 } else { 0.0 };
    let heading = if dist > hi * 1.8 { bearing } else { (bearing - side * (90.0 - lean)).rem_euclid(360.0) };
    // Look straight at it: the weapon follows the look.
    let look = look_toward(g, bearing, Some(16.0));
    let w = weapon_for_bearing(turn_to(me.heading, g.cam.yaw));
    let c = g.content();
    let cl = c.classes.get(&me.class).cloned().unwrap_or_default();
    let aligned = angle_diff(bearing, g.cam.yaw).abs() < 12.0;
    let ready = match w {
        Weapon::Broadside(Side::Port) => me.guns.port <= 0.0,
        Weapon::Broadside(Side::Starboard) => me.guns.starboard <= 0.0,
        Weapon::Chain => me.guns.chain <= 0.0,
        _ => false,
    };
    let in_arc = match w {
        Weapon::Broadside(_) => (rel.abs() - 90.0).abs() < 38.0,
        Weapon::Chain => rel.abs() < 30.0 && dist < 110.0,
        _ => false,
    };
    let fire = aligned && ready && in_arc && dist < 230.0 && tap(sim, 4);
    let _ = cl;
    (heading, look, fire)
}

/// The boarding melee and land fights: counter on the tell, else attack the nearest.
pub fn melee(g: &BlackTap, sim: &Sim) -> InputFrame {
    let mut i = InputFrame::default();
    let Some(pid) = sim.state.player else { return i };
    let Some(me) = sim.feet(pid) else { return i };
    // Counter: a strike at the player lands within 0.25 s.
    let incoming = g.land.fighters.values().any(|f| {
        f.target == Some(pid) && f.swing.is_some_and(|s| s.windup - s.t < 0.22 && s.windup - s.t > -0.05)
    });
    if incoming && g.land.hero.counter_ago.is_none_or(|a| a > 0.4) {
        i.pressed |= b::INTERACT;
        i.held |= b::INTERACT;
        return i;
    }
    let target = g
        .land
        .fighters
        .iter()
        .filter(|(_, f)| f.team == crate::foot::Team::Enemy && !f.vitals.dead && !f.surrendered)
        .filter_map(|(&id, _)| sim.feet(id).map(|p| (id, p)))
        .min_by(|a, b| a.1.distance(me).total_cmp(&b.1.distance(me)));
    if let Some((_, tp)) = target {
        let d = Vec2::new(tp.x - me.x, tp.z - me.z);
        if d.length() > 1.8 {
            i.move_dir = d.normalize() * 0.8;
        }
        if d.length() < 2.6 && g.land.hero.attack.is_none() && tap(sim, 20) {
            i.pressed |= b::ATTACK;
            i.held |= b::ATTACK;
        }
        let yaw = heading_of(d);
        i.look = look_toward(g, yaw, None) * 0.5;
    }
    let _ = Vec3::ZERO;
    i
}
