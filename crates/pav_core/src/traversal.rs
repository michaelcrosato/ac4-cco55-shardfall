//! Traversal for the character controller (`MovementParams::parkour`): the world probes behind
//! free-run vaults and leaps, wall, cliff and pole climbing, edge hangs, beams and climbing out
//! of the water. Everything here only reads the world; the state machine that uses it is in
//! `character.rs`.
//!
//! What climbs: upright cylinders and capsules up to `pole_radius` (poles, trunks, masts: all
//! the way round), terrain faces steeper than 55°, and block faces within 30° of vertical that
//! carry `block_flags::LADDER` (the climbable flag). With `climb_all` every static face within
//! 30° of vertical climbs. Characters and dynamic props never do.

use glam::{Quat, Vec3};
use rapier::prelude::*;

use crate::character::{HANG_DROP, Hang, MovementParams, Posture, RADIUS, Vault, WallClimb};
use crate::entity::EntityId;
use crate::physics::entity_from_tag;
use crate::sim::SimState;
use crate::statics::{BlockRef, block_flags};

/// Gap between a climbing capsule and the surface it holds (m).
const CLIMB_GAP: f32 = 0.06;
/// Probe heights above the feet while climbing: knee, waist, chest and hands.
const KNEE: f32 = 0.3;
const WAIST: f32 = 0.6;
const CHEST: f32 = 1.2;
const HANDS: f32 = 1.75;
/// Terrain steeper than this climbs (normal y below cos 55°).
const CLIFF_NY: f32 = 0.574;

/// Distance from a held surface to the climber's capsule axis.
pub(crate) fn hold() -> f32 {
    RADIUS + CLIMB_GAP
}

fn flat(v: Vec3) -> Vec3 {
    Vec3::new(v.x, 0.0, v.z)
}

/// A climbable surface found by a probe.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Grip {
    pub point: Vec3,
    /// Horizontal, toward the climber.
    pub normal: Vec3,
    /// A pole's axis (x, z; y = 0) and radius.
    pub pole: Option<(Vec3, f32)>,
}

/// Read-only world queries for one character this tick.
pub(crate) struct Probe<'a> {
    st: &'a SimState,
    qp: QueryPipeline<'a>,
    pub mp: &'a MovementParams,
}

impl<'a> Probe<'a> {
    pub fn new(st: &'a SimState, filter: QueryFilter<'a>, mp: &'a MovementParams) -> Self {
        Self { st, qp: st.physics.query_filtered(filter), mp }
    }

    /// First hit along a ray: distance, normal, collider.
    pub fn ray(&self, from: Vec3, dir: Vec3, len: f32) -> Option<(f32, Vec3, ColliderHandle)> {
        self.qp
            .cast_ray_and_get_normal(&Ray::new(from, dir), len as Real, true)
            .map(|(h, hit)| (hit.time_of_impact as f32, hit.normal, h))
    }

    /// Something to stand on within `depth` below `at`: its height and its normal's y.
    pub fn ground(&self, at: Vec3, depth: f32) -> Option<(f32, f32)> {
        self.ray(at, Vec3::NEG_Y, depth).map(|(t, n, _)| (at.y - t, n.y))
    }

    /// Support under a point at feet height: anything within `depth` below it.
    fn support(&self, p: Vec3, depth: f32) -> bool {
        self.ground(p + Vec3::Y * 0.25, 0.25 + depth).is_some()
    }

    /// Whether the surface hit (collider `h`, normal `n`) can be climbed: None = no,
    /// Some(None) = a face, Some(Some((axis, radius))) = a pole.
    fn climbable(&self, h: ColliderHandle, n: Vec3) -> Option<Option<(Vec3, f32)>> {
        let c = self.qp.colliders.get(h)?;
        if c.parent().and_then(|p| self.qp.bodies.get(p)).is_some_and(|b| b.is_dynamic()) {
            return None;
        }
        if entity_from_tag(c.user_data).and_then(|i| self.st.entities.get(EntityId(i))).is_some_and(|e| e.character.is_some()) {
            return None;
        }
        let shape = c.shape();
        let axis = c.rotation() * Vec3::Y;
        let pole = match (shape.as_cylinder(), shape.as_capsule()) {
            (Some(cy), _) if axis.y.abs() > 0.97 => Some(cy.radius as f32),
            (_, Some(cp)) => {
                let a = c.rotation() * (cp.segment.b - cp.segment.a);
                (a.normalize_or_zero().y.abs() > 0.97).then_some(cp.radius as f32)
            }
            _ => None,
        };
        if let Some(r) = pole.filter(|r| *r <= self.mp.pole_radius) {
            let t = c.translation();
            return Some(Some((Vec3::new(t.x, 0.0, t.z), r)));
        }
        if shape.as_trimesh().is_some() && c.parent().is_none() && c.user_data == 0 {
            // Terrain: cliffs.
            return (n.y < CLIFF_NY && n.y > -0.5).then_some(None);
        }
        if n.y.abs() > 0.5 {
            return None;
        }
        if self.mp.climb_all {
            return Some(None);
        }
        let flagged = BlockRef::from_tag(c.user_data)
            .and_then(|r| self.st.statics.get(r))
            .is_some_and(|b| b.has(block_flags::LADDER) || b.has(block_flags::CLIMBABLE));
        flagged.then_some(None)
    }

    /// A climbable surface along one ray.
    fn grip_ray(&self, from: Vec3, dir: Vec3, reach: f32) -> Option<(f32, Grip)> {
        let (t, n, c) = self.ray(from, dir, reach)?;
        if t < 1e-3 {
            return None; // started inside something
        }
        let point = from + dir * t;
        match self.climbable(c, n)? {
            Some((axis, r)) => {
                let normal = flat(from - axis).normalize_or(-dir);
                Some((t, Grip { point, normal, pole: Some((axis, r)) }))
            }
            None => {
                let normal = flat(n).normalize_or(-dir);
                (normal.dot(dir) < -0.5).then_some((t, Grip { point, normal, pole: None }))
            }
        }
    }

    /// A climbable surface ahead of the feet within `reach` of the capsule axis, tried at each
    /// height in turn (and beside the axis, for thin poles).
    pub fn grip(&self, feet: Vec3, dir: Vec3, reach: f32, heights: &[f32]) -> Option<Grip> {
        let side = Vec3::new(-dir.z, 0.0, dir.x);
        for h in heights {
            let mut best: Option<(f32, Grip)> = None;
            for off in [0.0f32, 0.18, -0.18] {
                if let Some((t, g)) = self.grip_ray(feet + Vec3::Y * *h + side * off, dir, reach) {
                    let score = t + off.abs();
                    if best.is_none_or(|(b, _)| score < b) {
                        best = Some((score, g));
                    }
                }
            }
            if let Some((_, g)) = best {
                return Some(g);
            }
        }
        None
    }

    /// The walkable top behind a face (`face` on it, normal `n`) between heights `lo` and `hi`,
    /// with head room over the edge and room to stand on it.
    pub fn top(&self, face: Vec3, n: Vec3, lo: f32, hi: f32) -> Option<f32> {
        let start = Vec3::new(face.x, hi + 0.05, face.z) - n * 0.15;
        let (t, up, _) = self.ray(start, Vec3::NEG_Y, (hi + 0.05 - lo).max(0.05))?;
        if t < 0.02 || up.y < 0.7 {
            return None; // the face goes on up, or slopes
        }
        let top = start.y - t;
        let reach = RADIUS + 0.45;
        let head = Vec3::new(face.x, top + 0.35, face.z) + n * reach;
        if self.qp.cast_ray(&Ray::new(head, -n), (reach + 0.35) as Real, true).is_some() {
            return None;
        }
        let stand = SharedShape::capsule_y(Posture::Stand.half_height(), RADIUS - 0.04);
        let pos = Vec3::new(face.x, top + Posture::Stand.height() * 0.5 + 0.03, face.z) - n * (RADIUS + 0.1);
        if self.qp.intersect_shape(Pose::from_translation(pos), stand.as_ref()).next().is_some() {
            return None;
        }
        Some(top)
    }

    /// Whether moving by `v` (m/s) for a tick leaves the feet over a drop deeper than `drop`.
    pub fn drop_ahead(&self, feet: Vec3, v: Vec3, dt: f32, drop: f32) -> bool {
        let len = v.length();
        if len < 1e-3 {
            return false;
        }
        let at = feet + v * dt + v / len * 0.08;
        !self.support(at, drop)
    }
}

/// A pull-up from `feet` onto a ledge at `top`: slower the further it lifts.
fn pull_time(mp: &MovementParams, feet_y: f32, top: f32) -> f32 {
    mp.ledge_climb_time * ((top - feet_y) / HANG_DROP).max(1.0)
}

/// Mantling onto (or catching) something ahead.
pub(crate) enum Catch {
    Mantle(Hang),
    Climb(WallClimb),
}

/// Parkour Up and mid-air catches: a walkable top within `mantle_height` to pull up onto, else
/// a climbable surface within `reach` of the capsule's surface.
pub(crate) fn catch(p: &Probe, feet: Vec3, dir: Vec3, reach: f32, mantle: bool) -> Option<Catch> {
    let g = p.grip(feet, dir, RADIUS + reach, &[CHEST, WAIST, HANDS])?;
    if mantle && g.pole.is_none() {
        if let Some(top) = p.top(g.point, g.normal, feet.y + 0.4, feet.y + p.mp.mantle_height) {
            let wall = Vec3::new(g.point.x, top, g.point.z);
            let time = pull_time(p.mp, feet.y, top);
            return Some(Catch::Mantle(Hang { normal: g.normal, wall, top, climb: 0.0, lower: 0.0, time }));
        }
    }
    Some(Catch::Climb(WallClimb::on(g, feet)))
}

/// Starting a wall climb from a hang (climbing down): the face below the hands.
pub(crate) fn climb_below(p: &Probe, feet: Vec3, n: Vec3) -> Option<WallClimb> {
    let g = p.grip(feet, -n, hold() + 0.4, &[WAIST, KNEE])?;
    g.pole.is_none().then(|| WallClimb::on(g, feet))
}

/// What a tick on the wall does.
pub(crate) enum ClimbOut {
    /// Keep climbing: this tick's move, and the sideways and vertical speeds (animation).
    Move { delta: Vec3, side: f32, up: f32 },
    /// Let go or lost the surface: fall with this velocity.
    Fall(Vec3),
    /// Jumped off backwards (ballistic).
    Eject(Vec3),
    /// Reached a walkable top: hang from it, or pull up (`climb >= 0`).
    Ledge(Hang),
}

/// The stick on a wall: toward it = up, away = down, along it = sideways.
pub(crate) struct ClimbInput {
    pub wish: Vec3,
    pub vertical: f32,
    pub jump: bool,
    pub dodge: bool,
}

/// One tick of wall, cliff or pole climbing.
pub(crate) fn climb(p: &Probe, wc: &mut WallClimb, feet: Vec3, i: &ClimbInput, dt: f32) -> ClimbOut {
    let mp = p.mp;
    let d = hold();
    wc.time += dt;
    if (feet - wc.at).length() > 0.5 {
        return ClimbOut::Fall(Vec3::ZERO); // moved away (a teleport)
    }
    if i.dodge {
        return ClimbOut::Fall(wc.normal * 1.2);
    }
    let n = wc.normal;
    if i.jump && i.wish.length() > 0.3 && i.wish.normalize().dot(n) > 0.5 {
        // Back-eject: up `eject_up` at the apex, `eject_out` away when back at this height.
        let vy = (2.0 * mp.gravity * mp.eject_up.max(0.05)).sqrt();
        let t = 2.0 * vy / mp.gravity.max(0.1);
        return ClimbOut::Eject(n * (mp.eject_out / t) + Vec3::Y * vy);
    }
    if i.jump && wc.leap <= 0.0 {
        wc.leap = mp.climb_leap_time.max(0.05);
    }
    let dead = |x: f32| if x.abs() < 0.25 { 0.0 } else { x };
    let tangent = Vec3::new(-n.z, 0.0, n.x);
    let up_in = dead((i.wish.dot(-n) + i.vertical).clamp(-1.0, 1.0));
    // Rounding an outer corner keeps the stick mapped to the face it started on.
    let side_in = dead(i.wish.dot(wc.corner.map(|c| c.tangent).unwrap_or(tangent)));
    let mut vy = if wc.leap > 0.0 {
        mp.climb_leap / mp.climb_leap_time.max(0.05)
    } else if up_in > 0.0 {
        up_in * mp.climb_up
    } else {
        up_in * mp.climb_down
    };
    wc.leap = (wc.leap - dt).max(0.0);
    let mut vs = side_in * mp.climb_side;

    // How far the surface is (a face just caught may still be a little way off).
    let chest = feet + Vec3::Y * CHEST;
    let mid = feet + Vec3::Y * 0.9;
    let held = if wc.pivot.is_some() {
        None
    } else {
        let found = [mid, chest, feet + Vec3::Y * KNEE, feet + Vec3::Y * HANDS]
            .into_iter()
            .find_map(|from| p.ray(from, -n, d + 1.0).filter(|(t, ..)| *t > 1e-3));
        if found.is_none() {
            return ClimbOut::Fall(n * 0.5); // nothing left to hold
        }
        found
    };
    let gap = held.map(|h| h.0).unwrap_or(d).max(d);

    // The top: the hands find nothing to hold.
    if vy > 0.0 && p.ray(feet + Vec3::Y * (HANDS + vy * dt), -n, gap + 0.25).is_none() {
        let face = p.ray(chest, -n, gap + 0.35).map(|(t, ..)| chest - n * t).unwrap_or(chest - n * gap);
        if wc.pivot.is_none() || wc.corner.is_some() {
            if let Some(top) = p.top(face, n, feet.y + 0.9, feet.y + HANDS + 0.45) {
                let wall = Vec3::new(face.x, top, face.z);
                let climb = if up_in > 0.0 { 0.0 } else { -1.0 };
                let time = pull_time(mp, feet.y, top);
                return ClimbOut::Ledge(Hang { normal: n, wall, top, climb, lower: 0.0, time });
            }
        }
        vy = 0.0; // nothing to stand on up there
        wc.leap = 0.0;
    }
    // The bottom: the surface ends under the feet (an overhang); the ground ends the climb.
    if vy < 0.0 && p.ray(feet + Vec3::Y * KNEE, -n, gap + 0.3).is_none() {
        vy = 0.0;
    }

    let r_feet = flat(feet);
    if let Some(pv) = wc.pivot {
        // Round a pole, or an outer corner's edge; one just caught is closed in on.
        let away = flat(r_feet - pv).length();
        let hold_r = wc.radius + d;
        let r = if away > hold_r { (away - 6.0 * dt).max(hold_r) } else { hold_r };
        let mut u = flat(r_feet - pv).normalize_or(n);
        match wc.corner.as_mut() {
            Some(c) => {
                let total = c.start.dot(c.to).clamp(-1.0, 1.0).acos();
                c.angle += side_in.signum() * c.dir * vs.abs() * dt / r.max(0.2);
                if c.angle >= total {
                    // Onto the new face, a little way along it.
                    let to = c.to;
                    let along = -c.from * 0.08;
                    wc.normal = to;
                    wc.pivot = None;
                    wc.corner = None;
                    let target = pv + to * d + along;
                    let delta = Vec3::new(target.x - feet.x, vy * dt, target.z - feet.z);
                    return ClimbOut::Move { delta, side: vs, up: vy };
                }
                if c.angle <= 0.0 {
                    wc.normal = c.from;
                    wc.pivot = None;
                    wc.corner = None;
                    return ClimbOut::Move { delta: Vec3::Y * vy * dt, side: vs, up: vy };
                }
                u = Quat::from_rotation_y(-c.dir * c.angle) * c.start;
            }
            None => {
                // A pole: is it still there?
                let reach = away - wc.radius + 0.3;
                if p.ray(feet + Vec3::Y * 0.9, -u, reach).is_none() && p.ray(feet + Vec3::Y * CHEST, -u, reach).is_none() {
                    return ClimbOut::Fall(u * 0.5);
                }
                u = Quat::from_rotation_y(-vs * dt / r.max(0.2)) * u;
            }
        }
        wc.normal = u;
        let target = pv + u * r;
        let delta = Vec3::new(target.x - feet.x, vy * dt, target.z - feet.z);
        return ClimbOut::Move { delta, side: vs, up: vy };
    }

    // A flat face: corners first.
    let reach = gap + 0.3;
    if vs != 0.0 {
        let ts = tangent * vs.signum();
        let step = vs.abs() * dt;
        if let Some((_, g)) =
            p.grip_ray(chest, ts, RADIUS + 0.12 + step).filter(|(_, g)| g.pole.is_none() && g.normal.dot(ts) < -0.7)
        {
            // Inner corner: a wall across the way. Turn onto it.
            wc.normal = g.normal;
            return ClimbOut::Move { delta: Vec3::Y * vy * dt, side: vs, up: vy };
        }
        let ahead = RADIUS * 0.6 + step;
        if p.ray(chest + ts * ahead, -n, reach).is_none() {
            // Outer corner: the face ends. Find its edge, then the face round it.
            let (mut lo, mut hi) = (0.0f32, ahead);
            for _ in 0..6 {
                let m = (lo + hi) * 0.5;
                if p.ray(chest + ts * m, -n, reach).is_some() {
                    lo = m;
                } else {
                    hi = m;
                }
            }
            let dist = p.ray(chest + ts * lo, -n, reach).map(|r| r.0).unwrap_or(gap);
            let edge = chest + ts * lo - n * dist;
            let round =
                p.grip_ray(edge + ts * 0.3 - n * 0.3, -ts, 0.45).filter(|(_, g)| g.pole.is_none() && g.normal.dot(ts) > 0.7);
            match round {
                Some((_, g)) => {
                    let pv = flat(edge);
                    let start = flat(r_feet - pv).normalize_or(n);
                    wc.pivot = Some(pv);
                    wc.radius = 0.0;
                    wc.corner =
                        Some(crate::character::Corner { from: n, to: g.normal, start, tangent, dir: vs.signum(), angle: 0.0 });
                    let target = pv + start * d;
                    let delta = Vec3::new(target.x - feet.x, vy * dt, target.z - feet.z);
                    return ClimbOut::Move { delta, side: vs, up: vy };
                }
                None => vs = 0.0, // nothing to climb round there
            }
        }
    }
    // Hold the surface: follow its distance and lean.
    let Some((t, sn, c)) = held else {
        return ClimbOut::Fall(n * 0.5);
    };
    if p.climbable(c, sn).is_some() {
        let nh = flat(sn).normalize_or(n);
        if nh.dot(n) > 0.7 {
            wc.normal = (n + (nh - n) * 0.3).normalize_or(n);
        }
    }
    let k = 1.0 - (-18.0 * dt).exp();
    let pull = (-wc.normal * (t - d) * k).clamp_length_max(6.0 * dt);
    let delta = tangent * vs * dt + Vec3::Y * vy * dt + pull;
    ClimbOut::Move { delta, side: vs, up: vy }
}

/// Free-run auto-vault: an obstacle ahead taller than a step and at most `vault_height` high,
/// with room to pass over it.
pub(crate) fn vault(p: &Probe, feet: Vec3, dir: Vec3, speed: f32) -> Option<Vault> {
    let mp = p.mp;
    let low = (mp.freerun_step.max(mp.step_height) + 0.08).min(mp.vault_height - 0.05);
    let from = feet + Vec3::Y * low;
    let (t, n, _) = p.ray(from, dir, RADIUS + 1.3)?;
    if t < 1e-3 || n.y.abs() > 0.3 || flat(n).normalize_or_zero().dot(dir) > -0.7 {
        return None;
    }
    let face = from + dir * t;
    let start = Vec3::new(face.x, feet.y + mp.vault_height + 0.1, face.z) + dir * 0.12;
    let (tt, up, _) = p.ray(start, Vec3::NEG_Y, mp.vault_height + 0.1 - low + 0.05)?;
    if tt < 0.02 || up.y < 0.7 {
        return None; // taller than a vault
    }
    let top = start.y - tt;
    // Room to pass over it, and nothing in the way at head height.
    let stand = SharedShape::capsule_y(Posture::Stand.half_height(), RADIUS - 0.04);
    let over = Vec3::new(face.x, top + 0.12 + Posture::Stand.height() * 0.5, face.z) + dir * 0.3;
    if p.qp.intersect_shape(Pose::from_translation(over), stand.as_ref()).next().is_some() {
        return None;
    }
    if p.ray(feet + Vec3::Y * (top - feet.y + 1.5), dir, t + 0.4).is_some() {
        return None;
    }
    // How deep it is: thin things are hopped over, deep ones landed on.
    let depth =
        (1..=16).map(|i| i as f32 * 0.1).find(|o| p.ground(Vec3::new(face.x, top + 0.2, face.z) + dir * *o, 0.35).is_none());
    let g = mp.gravity;
    let release = match depth {
        Some(dp) => g * ((dp + 2.0 * RADIUS + 0.1) / speed.max(1.0)) * 0.5 + 0.3,
        None => 1.0,
    };
    let rise = ((t - RADIUS - 0.03) / speed.max(1.0)).max(0.06);
    Some(Vault { dir, speed, from: feet.y, to: top + 0.08, t: 0.0, rise, release, at: feet })
}

/// One tick of a vault's hop: the move, and whether the hands let go.
pub(crate) fn vault_step(v: &mut Vault, feet: Vec3, dt: f32) -> (Vec3, bool) {
    v.t += dt;
    let k = (v.t / v.rise).min(1.0);
    let y = v.from + (v.to - v.from) * (1.0 - (1.0 - k) * (1.0 - k));
    (v.dir * v.speed * dt + Vec3::Y * (y - feet.y), k >= 1.0)
}

/// Parkour jump assist at an edge: a landing within `leap_reach` ahead, at most `leap_drop`
/// below and `leap_rise` above the feet, across a gap. Returns the launch velocity.
pub(crate) fn leap(p: &Probe, feet: Vec3, dir: Vec3, speed: f32) -> Option<Vec3> {
    let mp = p.mp;
    let above = mp.leap_rise + 0.2;
    let span = above + mp.leap_drop;
    let land_at = |o: f32| -> Option<f32> {
        let at = Vec3::new(feet.x, feet.y + above, feet.z) + dir * o;
        match p.ray(at, Vec3::NEG_Y, span) {
            Some((t, n, _)) if t > 0.02 && n.y > 0.7 => Some(at.y - t),
            _ => None,
        }
    };
    let mut gap = false;
    let mut o = 0.4f32;
    let mut hit = None;
    while o <= mp.leap_reach {
        let at = Vec3::new(feet.x, feet.y + above, feet.z) + dir * o;
        if p.ray(at, Vec3::NEG_Y, 0.01).is_some() {
            return None; // a wall taller than the reach in the way
        }
        match land_at(o) {
            Some(y) if gap => {
                hit = Some((o, y));
                break;
            }
            Some(_) => return None, // no gap: just run on (or off)
            None => gap = true,
        }
        o += 0.15;
    }
    let (o1, _) = hit?;
    let (o_land, y_land) = [o1 + 0.5, o1 + 0.25, o1].into_iter().find_map(|o| land_at(o).map(|y| (o, y)))?;
    if p.ray(feet + Vec3::Y * 1.0, dir, o_land).is_some() {
        return None; // something in the way
    }
    let v = speed.max(mp.run_speed).max(1.0);
    let t = o_land / v;
    let vy = ((y_land - feet.y) + 0.5 * mp.gravity * t * t) / t;
    Some(dir * v + Vec3::Y * vy)
}

/// Ledge safety at walking pace: the part of `vh` that keeps the feet off drops deeper than
/// `edge_drop` (sliding along an edge, or stopping at it).
pub(crate) fn safe_step(p: &Probe, feet: Vec3, vh: Vec3, dt: f32) -> Vec3 {
    let drop = p.mp.edge_drop;
    if !p.drop_ahead(feet, vh, dt, drop) {
        return vh;
    }
    let dir = vh.normalize_or_zero();
    let mut best = Vec3::ZERO;
    for a in [45f32, -45.0, 90.0, -90.0] {
        let d = Quat::from_rotation_y(a.to_radians()) * dir;
        let v = d * vh.dot(d);
        if v.length() > 0.2 && v.length() > best.length() && !p.drop_ahead(feet, v, dt, drop) {
            best = v;
        }
    }
    best
}

/// A beam under the feet narrower than `width`: its direction, the sideways axis and how far
/// the centreline is along it.
pub(crate) fn beam(p: &Probe, feet: Vec3, width: f32) -> Option<(Vec3, Vec3, f32)> {
    let depth = 0.45;
    if !p.support(feet, depth) {
        return None;
    }
    let r = width * 0.5 + 0.06;
    let dirs: [Vec3; 8] = std::array::from_fn(|i| {
        let a = i as f32 * std::f32::consts::FRAC_PI_4;
        Vec3::new(a.cos(), 0.0, a.sin())
    });
    let s: [bool; 8] = std::array::from_fn(|i| p.support(feet + dirs[i] * r, depth));
    // Supported along it (close in, both ways: not a floor's corner), not across it.
    let along = |i: usize| p.support(feet + dirs[i] * 0.12, depth) && p.support(feet + dirs[i + 4] * 0.12, depth);
    let i = (0..4).find(|&i| (s[i] || s[i + 4]) && !s[(i + 2) % 8] && !s[(i + 6) % 8] && along(i))?;
    let axis = dirs[i];
    let side = dirs[(i + 2) % 8];
    let edge = |dir: Vec3| -> f32 {
        let (mut lo, mut hi) = (0.0f32, r);
        for _ in 0..6 {
            let mid = (lo + hi) * 0.5;
            if p.support(feet + dir * mid, depth) {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        lo
    };
    let (a, b) = (edge(side), edge(-side));
    (a + b <= width).then_some((axis, side, (a - b) * 0.5))
}

/// Parkour Down: the edge ahead of the feet (within 1 m) over a drop deep enough to hang from
/// it; the hang, facing back at the face under the edge, lowered into from the top.
pub(crate) fn edge_hang(p: &Probe, feet: Vec3, dir: Vec3) -> Option<Hang> {
    let near = |o: f32| p.support(feet + dir * o, 0.35);
    if !near(0.0) {
        return None;
    }
    let mut o = 0.1f32;
    while o <= 1.0 && near(o) {
        o += 0.1;
    }
    if o > 1.0 {
        return None;
    }
    let (mut lo, mut hi) = (o - 0.1, o);
    for _ in 0..5 {
        let mid = (lo + hi) * 0.5;
        if near(mid) {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let edge = feet + dir * lo;
    if p.support(edge + dir * 0.3, HANG_DROP + 0.4) {
        return None; // too shallow to hang: step down instead
    }
    let top = p.ground(feet + dir * (lo - 0.05).max(0.0) + Vec3::Y * 0.3, 0.7)?.0;
    let below = Vec3::new(edge.x, top - 0.4, edge.z) + dir * 0.35;
    let (normal, wall) = match p.ray(below, -dir, 0.6) {
        Some((t, n, _)) if t > 1e-3 && flat(n).normalize_or_zero().dot(dir) > 0.7 => {
            let at = below - dir * t;
            (flat(n).normalize(), Vec3::new(at.x, top, at.z))
        }
        _ => (dir, Vec3::new(edge.x, top, edge.z)),
    };
    Some(Hang { normal, wall, top, climb: -1.0, lower: 1.0, time: 0.0 })
}

/// Climbing out of the water: a face ahead with a walkable top at most `swim_climb` above the
/// water surface.
pub(crate) fn swim_ledge(p: &Probe, feet: Vec3, surface: f32, dir: Vec3) -> Option<Hang> {
    let mp = p.mp;
    for h in [0.3f32, 0.9, -0.3, 1.4] {
        let from = Vec3::new(feet.x, surface + h, feet.z);
        let Some((t, n, _)) = p.ray(from, dir, RADIUS + 0.5) else { continue };
        let nh = flat(n).normalize_or_zero();
        if t < 1e-3 || nh.dot(dir) > -0.5 {
            continue;
        }
        let face = from + dir * t;
        if let Some(top) = p.top(face, nh, surface - 0.2, surface + mp.swim_climb) {
            let time = pull_time(mp, feet.y, top);
            return Some(Hang { normal: nh, wall: Vec3::new(face.x, top, face.z), top, climb: 0.0, lower: 0.0, time });
        }
    }
    None
}
