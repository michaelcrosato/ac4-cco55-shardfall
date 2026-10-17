//! The bot on land: authored routes (`[[bot_hint]]` in tab1.toml: points with an action each)
//! for swimming ashore, climbing and infiltrating; stealth skills for the courier, the stocks
//! and Commodore Drye (darts, pickpocketing, assassinations from behind); melee when a fight
//! starts. Stateless like the sea bot: every decision comes from the game state.

use glam::{Vec2, Vec3};
use pav_core::input::InputFrame;
use pav_core::play::Awareness;
use pav_core::sim::Sim;
use pav_kit::camera::angle_diff;
use pav_kit::naval::heading_of;

use crate::game::{BlackTap, b};

fn flat(v: Vec3) -> Vec2 {
    Vec2::new(v.x, v.z)
}

fn tap(sim: &Sim, every: u64) -> bool {
    sim.state.tick.is_multiple_of(every.max(2))
}

fn press(i: &mut InputFrame, bit: u32) {
    i.pressed |= bit;
    i.held |= bit;
}

/// Turns the camera toward a heading (degrees from north), gently.
fn look_at(g: &BlackTap, yaw: f32, rate: f32) -> Vec2 {
    Vec2::new(angle_diff(yaw, g.cam.yaw).clamp(-rate, rate), (g.cam.pitch - 12.0).clamp(-1.0, 1.0))
}

/// Walks (or runs) toward a point, with the camera following the way.
fn go(g: &BlackTap, sim: &Sim, to: Vec3, run: bool) -> InputFrame {
    let mut i = InputFrame::default();
    let Some(p) = BlackTap::player_feet(sim) else { return i };
    let l = flat(to - p).length();
    // A wall or a house in the way: round it on the level's walking grid.
    let d = crate::paths::steer(sim, &g.world().geometry, p, to, 10.0) * l;
    if l > 0.3 {
        i.move_dir = d.normalize_or_zero() * if l < 1.5 { 0.5 } else { 1.0 };
        i.look = look_at(g, heading_of(d), 2.5);
    }
    if run && l > 3.0 {
        i.held |= b::SPRINT;
    }
    // A low wall right ahead (a sea wall, a cart): free-running vaults or climbs it.
    let ahead = Vec3::new(d.x, 0.0, d.y).normalize_or_zero();
    if l > 2.0
        && [0.25f32, 0.5].iter().any(|y| {
            sim.raycast(p + Vec3::Y * *y, ahead, 1.2, None).is_some_and(|h| h.normal.y.abs() < 0.5 && h.entity.is_none())
        })
        && sim.raycast(p + Vec3::Y * 2.2, ahead, 1.6, None).is_none()
    {
        i.held |= b::SPRINT;
    }
    // Stuck: hop.
    let speed = sim
        .state
        .player
        .and_then(|id| sim.state.entities.get(id))
        .and_then(|e| e.character.as_ref())
        .map(|c| flat(c.vel).length())
        .unwrap_or(0.0);
    // Stuck (a ledge that walking won't step off, a lip): run, which skips ledge safety, and hop.
    if l > 2.0 && speed < 0.25 {
        i.held |= b::SPRINT;
        if tap(sim, 75) {
            press(&mut i, b::JUMP);
        }
    }
    i
}

/// The direction into the nearest climbable wall within 1.6 m (rays every 30° at chest
/// height), preferring walls in the direction of `toward`.
fn wall_dir(sim: &Sim, feet: Vec3, toward: Vec2) -> Option<Vec2> {
    let from = feet + Vec3::Y * 1.0;
    let want = toward.normalize_or_zero();
    let mut best: Option<(f32, Vec2)> = None;
    for k in 0..12 {
        let a = k as f32 * std::f32::consts::TAU / 12.0;
        let d = Vec2::new(a.sin(), a.cos());
        let dir = Vec3::new(d.x, 0.0, d.y);
        if let Some(h) = sim.raycast(from, dir, 1.6, None) {
            // A wall that goes on above the head (not a cart or a crate).
            let tall = sim.raycast(feet + Vec3::Y * 3.2, dir, 2.2, None).is_some();
            if h.normal.y.abs() < 0.4 && tall {
                let dist = (h.point - from).length();
                let score = dist - 0.6 * d.dot(want);
                if best.is_none_or(|b| score < b.0) {
                    best = Some((score, -Vec2::new(h.normal.x, h.normal.z).normalize_or(d)));
                }
            }
        }
    }
    best.map(|b| b.1)
}

/// Follows a bot hint: the point after the nearest segment, and that point's action.
pub fn follow(g: &BlackTap, sim: &Sim, name: &str) -> Option<InputFrame> {
    let w = g.world();
    let h = w.hint(name)?;
    let p = BlackTap::player_feet(sim)?;
    if h.points.is_empty() {
        return None;
    }
    let pts: Vec<Vec3> = h.points.iter().map(|q| if q[1] == 0.0 { Vec3::new(q[0], p.y, q[2]) } else { Vec3::from(*q) }).collect();
    // The route segment the player is nearest to (3D, so climbs count height) → aim for its
    // end; before the route starts, its first point.
    let mut best = (pts[0].distance(p), 0usize);
    for k in 1..pts.len() {
        let (a, bpt) = (pts[k - 1], pts[k]);
        let ab = bpt - a;
        let t = ((p - a).dot(ab) / ab.length_squared().max(1e-3)).clamp(0.0, 1.0);
        let d = (a + ab * t).distance(p);
        if d < best.0 - 0.01 {
            best = (d, k);
        }
    }
    let mut k = best.1;
    // A point at height 0 is "on the ground, whatever its height" (roads over hills).
    let reached = |q: Vec3| flat(q - p).length() < 1.4 && (q.y - p.y).abs() < 2.5;
    while k + 1 < pts.len() && reached(pts[k]) {
        k += 1;
    }
    let target = pts[k];
    let action = h.actions.get(k).map(|s| s.as_str()).unwrap_or("walk");
    let ch = sim.state.player.and_then(|id| sim.state.entities.get(id)).and_then(|e| e.character.as_ref());
    let climbing = ch.is_some_and(|c| c.hang.is_some() || c.climbing.is_some() || c.climb.is_some());
    let mut i = go(g, sim, target, matches!(action, "run" | "climb" | "jump" | "dive"));
    // Stuck on a wall the route doesn't climb (a bank beside a road): let go.
    let still = ch.is_some_and(|c| c.vel.length() < 0.2);
    if action != "climb" && climbing && still && tap(sim, 40) {
        press(&mut i, if ch.is_some_and(|c| c.hang.is_some()) { b::CROUCH } else { b::DROP });
    }
    match action {
        "climb" => {
            // At the foot of the wall: free-run into it toward the top (the next point) and
            // Parkour Up; keep pushing at it while climbing.
            i.held |= b::SPRINT;
            let top = pts.get(k + 1).copied().unwrap_or(target);
            if flat(target - p).length() < 3.2 || climbing {
                // Into the nearest wall (probed with rays; ties go to the side facing the
                // top), else from the climb point toward the top.
                let toward = if flat(top - target).length() > 0.3 { flat(top - target) } else { flat(top - p) };
                let into = wall_dir(sim, p, toward).unwrap_or(toward).normalize_or_zero();
                // Up the wall, drifting sideways to line up with the top (the stick's
                // sideways part climbs sideways).
                let side = Vec2::new(-into.y, into.x);
                let lateral = (flat(top - p).dot(side) * 0.8).clamp(-1.0, 1.0);
                let d = into + side * lateral;
                if d.length() > 0.05 {
                    i.move_dir = d.normalize();
                    i.look = look_at(g, heading_of(d), 2.5);
                }
                if !climbing && tap(sim, 10) {
                    press(&mut i, b::JUMP);
                }
                // Stalled on the wall (a cornice, a gap in the holds): a climb leap.
                let still = ch.is_some_and(|c| c.vel.length() < 0.2);
                if climbing && still && tap(sim, 45) {
                    press(&mut i, b::JUMP);
                }
            }
        }
        "jump" | "dive" => {
            if flat(target - p).length() < 2.5 && tap(sim, 12) {
                press(&mut i, b::JUMP);
            }
        }
        "crouch" => i.held |= b::CROUCH,
        "aboard" => {
            if flat(target - p).length() < 8.0 && tap(sim, 15) {
                press(&mut i, b::JUMP);
            }
        }
        "interact" => {
            if flat(target - p).length() < 2.0 {
                i.held |= b::INTERACT;
            }
        }
        "wait" => {
            if flat(target - p).length() < 1.5 {
                i.move_dir = Vec2::ZERO;
            }
        }
        _ => {}
    }
    Some(i)
}

/// The nearest live person matching `pick`.
fn nearest(g: &BlackTap, sim: &Sim, pick: impl Fn(&crate::stealth::Npc) -> bool) -> Option<(pav_core::entity::EntityId, Vec3)> {
    let p = BlackTap::player_feet(sim)?;
    g.stealth
        .npcs
        .iter()
        .filter(|(_, n)| pick(n))
        .filter_map(|(&id, _)| sim.feet(id).map(|f| (id, f)))
        .min_by(|a, b| flat(a.1 - p).length().total_cmp(&flat(b.1 - p).length()))
}

/// Aim a dart at someone: turn to them, hold aim, fire when lined up.
/// A clear line from the eyes to someone's chest (darts need one).
fn in_sight(sim: &Sim, p: Vec3, at: Vec3) -> bool {
    let (from, to) = (p + Vec3::Y * 1.6, at + Vec3::Y * 1.2);
    let d = to - from;
    sim.raycast(from, d, d.length() - 0.5, None).is_none_or(|h| h.entity.is_some())
}

fn dart(g: &BlackTap, sim: &Sim, at: Vec3) -> InputFrame {
    let mut i = InputFrame::default();
    let Some(p) = BlackTap::player_feet(sim) else { return i };
    let d = flat(at - p);
    let yaw = heading_of(d);
    i.look = look_at(g, yaw, 6.0);
    i.held |= b::AIM;
    if angle_diff(yaw, g.cam.yaw).abs() < 6.0 && tap(sim, 10) {
        press(&mut i, b::FIRE);
    }
    i
}

/// Creep up behind someone and take them: pickpocket (hold △) or a Tap Blade kill (□).
fn creep(g: &BlackTap, sim: &Sim, id: pav_core::entity::EntityId, at: Vec3, kill: bool) -> InputFrame {
    let Some(p) = BlackTap::player_feet(sim) else { return InputFrame::default() };
    // Seen: no more creeping — run him down.
    if g.stealth.npcs.get(&id).is_some_and(|n| n.mind.aware()) {
        let mut i = go(g, sim, at, true);
        if flat(at - p).length() < 2.2 && tap(sim, 10) {
            press(&mut i, b::ATTACK);
        }
        return i;
    }
    let facing = sim.state.entities.get(id).and_then(|e| e.character.as_ref()).map(|c| c.facing).unwrap_or(0.0);
    let back = Vec2::new(facing.sin(), facing.cos());
    let behind = at - Vec3::new(back.x, 0.0, back.y) * 1.1;
    let d = flat(at - p).length();
    let mut i = go(g, sim, behind, false);
    if d < 6.0 {
        i.held |= b::CROUCH;
    }
    if d < 1.7 {
        if kill {
            if tap(sim, 8) {
                press(&mut i, b::ATTACK);
            }
        } else {
            i.held |= b::INTERACT;
            i.move_dir *= 0.2;
        }
    }
    i
}

/// Land skills by the mission step's `bot` name.
pub fn land(g: &BlackTap, sim: &Sim, skill: &str) -> InputFrame {
    // A fight comes first.
    if g.land.enemies_left() > 0 {
        return crate::bot::melee(g, sim);
    }
    let (head, arg) = skill.split_once(':').unwrap_or((skill, ""));
    let showcase = g.showcase.is_some();
    // The showcase pulses Gull Sight now and then.
    if showcase && sim.state.tick % 900 == 450 {
        let mut i = InputFrame::default();
        press(&mut i, b::GULL);
        return i;
    }
    match head {
        "swim_ashore" => {
            // Off the deck (free-running: no ledge safety), swim, walk up the beach.
            let beach = g.world().marker("r4_beach").unwrap_or_default();
            let mut i = go(g, sim, beach, true);
            let ch = sim.state.player.and_then(|id| sim.state.entities.get(id)).and_then(|e| e.character.as_ref());
            let swimming = ch.is_some_and(|c| c.swimming);
            if g.land.deck.is_some() && !swimming {
                i.held |= b::SPRINT;
                if tap(sim, 20) {
                    press(&mut i, b::JUMP);
                }
            }
            i
        }
        "climb_to" => follow(g, sim, &format!("r4_climb_{arg}")).unwrap_or_default(),
        "survey" => {
            let mut i = InputFrame::default();
            if g.stealth.survey.is_none() {
                i.held |= b::INTERACT;
            }
            i
        }
        "leap" => {
            let mut i = InputFrame::default();
            if g.stealth.survey.is_none() && g.stealth.leap.is_none() && tap(sim, 20) {
                press(&mut i, b::JUMP);
            }
            i
        }
        "get_key" => {
            if g.flags.contains("key_dropped") {
                if let Some((_, at)) = nearest(g, sim, |n| n.has("courier")) {
                    let mut i = go(g, sim, at, false);
                    i.held |= b::INTERACT;
                    return i;
                }
            }
            // Escorts first (darts), then the courier from behind.
            if let Some((_, at)) = nearest(g, sim, |n| n.has("escort") && !n.out()) {
                let p = BlackTap::player_feet(sim).unwrap_or_default();
                if flat(at - p).length() < 22.0
                    && g.land.hero.tools.count(pav_kit::combat::Tool::Nightcap) > 0
                    && in_sight(sim, p, at)
                {
                    return dart(g, sim, at);
                }
                let mut i = go(g, sim, at, false);
                i.held |= b::CROUCH;
                return i;
            }
            match nearest(g, sim, |n| n.has("courier") && !n.dead) {
                Some((id, at)) => creep(g, sim, id, at, showcase),
                None => InputFrame::default(),
            }
        }
        "free_molasses" => {
            let p = BlackTap::player_feet(sim).unwrap_or_default();
            let stocks = g.world().marker("stocks").unwrap_or_default();
            // Put each stocks guard to sleep, then open the stocks.
            if let Some((_, at)) = nearest(g, sim, |n| n.group == "stocks" && n.fighter && !n.out()) {
                if flat(at - p).length() < 22.0
                    && g.land.hero.tools.count(pav_kit::combat::Tool::Nightcap) > 0
                    && in_sight(sim, p, at)
                {
                    return dart(g, sim, at);
                }
                if let Some((id, at)) = nearest(g, sim, |n| n.group == "stocks" && n.fighter && !n.out()) {
                    return creep(g, sim, id, at, true);
                }
            }
            let mut i = go(g, sim, stocks, false);
            if flat(stocks - p).length() < 2.5 {
                i.held |= b::INTERACT;
                i.move_dir = Vec2::ZERO;
            }
            i
        }
        "infiltrate" => follow(g, sim, "r6_infiltrate").unwrap_or_default(),
        "assassinate" => {
            let p = BlackTap::player_feet(sim).unwrap_or_default();
            // The bodyguards sleep first.
            if let Some((_, at)) = nearest(g, sim, |n| n.group == "drye" && n.kind != "drye" && !n.out()) {
                if flat(at - p).length() < 24.0
                    && g.land.hero.tools.count(pav_kit::combat::Tool::Nightcap) > 0
                    && in_sight(sim, p, at)
                {
                    return dart(g, sim, at);
                }
            }
            match nearest(g, sim, |n| n.kind == arg && !n.dead) {
                Some((id, at)) => {
                    let alerted =
                        g.stealth.npcs.get(&id).is_some_and(|n| matches!(n.mind.state, Awareness::Alerted | Awareness::Combat));
                    if alerted {
                        let mut i = go(g, sim, at, true);
                        if flat(at - p).length() < 2.4 && tap(sim, 12) {
                            press(&mut i, b::ATTACK);
                        }
                        i
                    } else {
                        creep(g, sim, id, at, true)
                    }
                }
                None => follow(g, sim, "r6_drye").unwrap_or_default(),
            }
        }
        "raise_flag" => {
            if let Some(i) = follow(g, sim, "r6_flag") {
                let p = BlackTap::player_feet(sim).unwrap_or_default();
                let fp = g.world().marker("flagpole").unwrap_or_default();
                // On the roof: step onto the battlement's perch (the flag rises there and the
                // Leap of Spirits starts there), then raise it.
                if flat(fp - p).length() < 3.5 && (fp.y - p.y).abs() < 2.0 {
                    if flat(fp - p).length() > 1.0 {
                        return go(g, sim, fp, false);
                    }
                    let mut i = InputFrame::default();
                    i.held |= b::INTERACT;
                    return i;
                }
                return i;
            }
            let fp = g.world().marker("flagpole").unwrap_or_default();
            let mut i = go(g, sim, fp, false);
            i.held |= b::INTERACT;
            i
        }
        "escape_to_ship" => {
            let p = BlackTap::player_feet(sim).unwrap_or_default();
            // From the battlement: the Leap of Spirits into the sea.
            if crate::traverse::perch_here(g, sim).is_some_and(|q| !q.leap_to.is_empty()) && g.stealth.leap.is_none() {
                let mut i = InputFrame::default();
                if tap(sim, 20) {
                    press(&mut i, b::JUMP);
                }
                return i;
            }
            // On deck: take the helm.
            if g.land.deck.is_some() {
                if let Some(s) = g.sea.my_ship() {
                    let c = g.content();
                    let cl = c.classes.get(&s.class).cloned().unwrap_or_default();
                    let helm = s.world(Vec3::new(0.0, cl.freeboard, -cl.length * 0.36));
                    let mut i = go(g, sim, helm, false);
                    if flat(helm - p).length() < 3.0 {
                        i.held |= b::INTERACT;
                    }
                    return i;
                }
            }
            // In the water: swim to her and climb aboard.
            let swimming = sim
                .state
                .player
                .and_then(|id| sim.state.entities.get(id))
                .and_then(|e| e.character.as_ref())
                .is_some_and(|c| c.swimming);
            if swimming {
                if let Some(s) = g.sea.my_ship() {
                    let at = Vec3::new(s.pos.x, 0.0, s.pos.y);
                    let mut i = go(g, sim, at, true);
                    if flat(at - p).length() < 8.0 && tap(sim, 10) {
                        press(&mut i, b::JUMP);
                    }
                    return i;
                }
            }
            follow(g, sim, "r6_dive").unwrap_or_default()
        }
        "wait" => InputFrame::default(),
        _ => InputFrame::default(),
    }
}
