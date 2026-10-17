//! Cannonballs, chain shot, swivel shots, mortar shells and pistol balls as a deterministic
//! list the game steps itself (the engine's projectiles stop only at fixed colliders and report
//! no impacts). Each tick a shot's segment is tested against hulls (oriented boxes), sails,
//! the ground and the sea; hits come back as [`Impact`]s.

use glam::{Vec2, Vec3};
use serde::{Deserialize, Serialize};

pub const GRAVITY: f32 = 9.81;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShotKind {
    Round,
    Chain,
    Swivel,
    Mortar,
    Pistol,
}

/// Who fired: a vessel or a battery by its id, or a character.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Owner {
    Vessel(u32),
    Battery(u32),
    Character(u32),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Shot {
    pub pos: Vec3,
    pub vel: Vec3,
    pub kind: ShotKind,
    pub owner: Owner,
    pub damage: f32,
    /// Blast radius on impact (mortars, kegs), 0 for a direct hit only.
    pub blast: f32,
    /// Seconds before it appears (a volley's stagger).
    pub delay: f32,
    pub life: f32,
}

/// Something a shot can hit: an oriented box on the XZ plane (yaw in degrees, 0 = north).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Target {
    pub id: u32,
    pub center: Vec3,
    pub yaw: f32,
    /// Half extents: (half beam, half height, half length).
    pub half: Vec3,
    /// A sail volume above the hull for chain shot: height of its bottom and top.
    pub sail: Option<(f32, f32)>,
    /// Shots from this owner pass through (your own ship).
    pub owner: Owner,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HitWhat {
    Hull(u32),
    Sail(u32),
    Water,
    Ground,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Impact {
    pub pos: Vec3,
    pub what: HitWhat,
    pub kind: ShotKind,
    pub owner: Owner,
    pub damage: f32,
    pub blast: f32,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Ballistics {
    pub shots: Vec<Shot>,
}

/// Whether the point is inside the box (local frame: x = right, z = forward along yaw).
fn inside(t: &Target, p: Vec3, pad: f32) -> bool {
    let d = p - t.center;
    let y = t.yaw.to_radians();
    let fwd = Vec2::new(y.sin(), -y.cos());
    let right = Vec2::new(y.cos(), y.sin());
    let lx = Vec2::new(d.x, d.z).dot(right);
    let lz = Vec2::new(d.x, d.z).dot(fwd);
    lx.abs() <= t.half.x + pad && lz.abs() <= t.half.z + pad && d.y.abs() <= t.half.y + pad
}

fn in_sail(t: &Target, p: Vec3) -> bool {
    let Some((lo, hi)) = t.sail else { return false };
    let d = p - t.center;
    let y = t.yaw.to_radians();
    let fwd = Vec2::new(y.sin(), -y.cos());
    let right = Vec2::new(y.cos(), y.sin());
    let lx = Vec2::new(d.x, d.z).dot(right);
    let lz = Vec2::new(d.x, d.z).dot(fwd);
    lx.abs() <= t.half.x * 2.2 && lz.abs() <= t.half.z * 0.8 && p.y >= lo && p.y <= hi
}

impl Ballistics {
    pub fn fire(&mut self, s: Shot) {
        if self.shots.len() < 512 {
            self.shots.push(s);
        }
    }

    /// Moves every shot and returns what they hit. `ground(x, z)` is the terrain height (or
    /// very low over open sea), `sea(x, z)` the water surface.
    pub fn step(&mut self, dt: f32, targets: &[Target], ground: &dyn Fn(Vec2) -> f32, sea: &dyn Fn(Vec2) -> f32) -> Vec<Impact> {
        let mut out = Vec::new();
        let mut keep = Vec::with_capacity(self.shots.len());
        for mut s in std::mem::take(&mut self.shots) {
            if s.delay > 0.0 {
                s.delay -= dt;
                keep.push(s);
                continue;
            }
            s.life -= dt;
            let from = s.pos;
            s.vel.y -= GRAVITY * dt;
            let to = from + s.vel * dt;
            // Sub-steps of ≤ 1 m so fast shots don't tunnel through hulls.
            let n = ((to - from).length() / 1.0).ceil().max(1.0) as usize;
            let mut hit = None;
            'walk: for k in 1..=n {
                let p = from.lerp(to, k as f32 / n as f32);
                for t in targets {
                    if t.owner == s.owner {
                        continue;
                    }
                    if s.kind == ShotKind::Chain && in_sail(t, p) {
                        hit = Some((p, HitWhat::Sail(t.id)));
                        break 'walk;
                    }
                    if inside(t, p, if s.kind == ShotKind::Pistol { 0.0 } else { 0.3 }) {
                        hit = Some((p, HitWhat::Hull(t.id)));
                        break 'walk;
                    }
                }
                let xz = Vec2::new(p.x, p.z);
                let g = ground(xz);
                let w = sea(xz);
                if p.y <= g && g >= w {
                    hit = Some((Vec3::new(p.x, g, p.z), HitWhat::Ground));
                    break;
                }
                if p.y <= w {
                    hit = Some((Vec3::new(p.x, w, p.z), HitWhat::Water));
                    break;
                }
            }
            match hit {
                Some((pos, what)) => {
                    out.push(Impact { pos, what, kind: s.kind, owner: s.owner, damage: s.damage, blast: s.blast })
                }
                None if s.life > 0.0 => {
                    s.pos = to;
                    keep.push(s);
                }
                None => {}
            }
        }
        self.shots = keep;
        out
    }
}

/// The elevation (radians) that lands a shot at `muzzle` speed `range` metres away on level
/// ground (the low arc), or None out of range.
pub fn elevation_for(range: f32, height_diff: f32, muzzle: f32) -> Option<f32> {
    // Solve tan θ for the trajectory through (range, height_diff): low root.
    let v2 = muzzle * muzzle;
    let g = GRAVITY;
    let disc = v2 * v2 - g * (g * range * range + 2.0 * height_diff * v2);
    if disc < 0.0 || range < 1e-3 {
        return None;
    }
    Some(((v2 - disc.sqrt()) / (g * range)).atan())
}

/// Where a shot fired from `from` with velocity `vel` comes down to `level` (seconds, point).
pub fn landing(from: Vec3, vel: Vec3, level: f32) -> (f32, Vec3) {
    // y(t) = y0 + vy t - g t²/2 = level
    let a = -0.5 * GRAVITY;
    let (b, c) = (vel.y, from.y - level);
    let disc = b * b - 4.0 * a * c;
    let t = if disc < 0.0 { 0.0 } else { (-b - disc.sqrt()) / (2.0 * a) };
    let t = t.max(0.0);
    (t, from + vel * t + Vec3::new(0.0, a * t * t, 0.0))
}

/// Points along an arc for an aim preview (every `step` seconds up to `max_t` or the water).
pub fn arc(from: Vec3, vel: Vec3, step: f32, max_t: f32, sea_level: f32) -> Vec<Vec3> {
    let mut pts = Vec::new();
    let mut t = 0.0;
    while t <= max_t {
        let p = from + vel * t + Vec3::new(0.0, -0.5 * GRAVITY * t * t, 0.0);
        pts.push(p);
        if p.y < sea_level && t > 0.0 {
            break;
        }
        t += step;
    }
    pts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_ball_aimed_with_elevation_for_lands_at_range() {
        let v = 70.0;
        let el = elevation_for(100.0, 0.0, v).unwrap();
        let vel = Vec3::new(0.0, el.sin() * v, -el.cos() * v);
        let (_, p) = landing(Vec3::ZERO, vel, 0.0);
        assert!((p.z + 100.0).abs() < 0.5, "landed at {p:?}");
        assert!(elevation_for(2000.0, 0.0, v).is_none());
    }

    #[test]
    fn shots_hit_hulls_sails_and_water() {
        let target = Target {
            id: 7,
            center: Vec3::new(0.0, 1.0, -100.0),
            yaw: 90.0,
            half: Vec3::new(3.0, 2.0, 12.0),
            sail: Some((4.0, 18.0)),
            owner: Owner::Vessel(7),
        };
        let mut b = Ballistics::default();
        let v = 70.0;
        let el = elevation_for(100.0, 1.0, v).unwrap();
        b.fire(Shot {
            pos: Vec3::new(0.0, 1.0, 0.0),
            vel: Vec3::new(0.0, el.sin() * v, -el.cos() * v),
            kind: ShotKind::Round,
            owner: Owner::Vessel(1),
            damage: 30.0,
            blast: 0.0,
            delay: 0.0,
            life: 10.0,
        });
        let mut hits = Vec::new();
        for _ in 0..300 {
            hits.extend(b.step(1.0 / 60.0, &[target], &|_| -20.0, &|_| 0.0));
        }
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].what, HitWhat::Hull(7));
        // Chain shot high through the rigging hits the sail.
        b.fire(Shot {
            pos: Vec3::new(0.0, 9.0, -60.0),
            vel: Vec3::new(0.0, 3.0, -80.0),
            kind: ShotKind::Chain,
            owner: Owner::Vessel(1),
            damage: 10.0,
            blast: 0.0,
            delay: 0.0,
            life: 5.0,
        });
        let mut hits = Vec::new();
        for _ in 0..120 {
            hits.extend(b.step(1.0 / 60.0, &[target], &|_| -20.0, &|_| 0.0));
        }
        assert_eq!(hits[0].what, HitWhat::Sail(7));
    }
}
