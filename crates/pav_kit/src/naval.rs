//! Sailing ships ("vessels"), their AI captains and shore batteries: an arcade sailing model
//! (speed from sail level and the angle to the wind, turn rate by speed, grounding on the
//! seabed, collisions between hulls), weapons readiness, damage states (smoke, fire, disabled,
//! sinking), brace, and the visual heave, pitch and roll from the sea. All deterministic and in
//! the game's state; classes are data (`ships.toml`).

use std::collections::BTreeMap;

use glam::{Vec2, Vec3};
use pav_core::ocean::Ocean;
use serde::{Deserialize, Serialize};

use crate::ballistics::{self, Owner, Shot, ShotKind};

/// A class of ship (data).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HullClass {
    pub name: String,
    pub length: f32,
    pub beam: f32,
    /// Below the waterline (m): shallower water grounds the ship.
    pub draft: f32,
    /// Deck height above the water (m).
    pub freeboard: f32,
    pub hp: f32,
    /// Top speed (m/s) at sail levels 1 (half) and 2 (full).
    pub speed_half: f32,
    pub speed_full: f32,
    /// m/s² speeding up and slowing down.
    pub accel: f32,
    pub brake: f32,
    /// Turn rate (°/s) at standstill, at mid speed and at full speed.
    pub turn_still: f32,
    pub turn_mid: f32,
    pub turn_full: f32,
    pub guns_per_side: u32,
    pub chasers: u32,
    pub swivels: u32,
    pub kegs: u32,
    pub crew: u32,
    /// Masts (1–3) and their height above the deck.
    pub masts: u32,
    pub mast_height: f32,
    /// Weapon numbers.
    pub muzzle: f32,
    pub ball_damage: f32,
    pub reload: f32,
}

impl Default for HullClass {
    fn default() -> Self {
        Self {
            name: "ship".into(),
            length: 24.0,
            beam: 7.0,
            draft: 2.6,
            freeboard: 2.2,
            hp: 700.0,
            speed_half: 7.0,
            speed_full: 11.0,
            accel: 1.2,
            brake: 2.0,
            turn_still: 4.0,
            turn_mid: 20.0,
            turn_full: 15.0,
            guns_per_side: 6,
            chasers: 0,
            swivels: 0,
            kegs: 0,
            crew: 10,
            masts: 2,
            mast_height: 16.0,
            muzzle: 70.0,
            ball_damage: 30.0,
            reload: 6.0,
        }
    }
}

pub type Classes = BTreeMap<String, HullClass>;

pub fn parse_classes(text: &str) -> anyhow::Result<Classes> {
    #[derive(Deserialize)]
    struct File {
        class: Classes,
    }
    Ok(toml::from_str::<File>(text)?.class)
}

/// Speed factor by the angle between the bow and where the wind comes from (degrees, 0 = into
/// the wind, 180 = running before it).
pub fn wind_factor(off_wind: f32) -> f32 {
    const T: [(f32, f32); 6] = [(0.0, 0.15), (30.0, 0.18), (45.0, 0.6), (90.0, 1.0), (135.0, 0.9), (180.0, 0.75)];
    let a = off_wind.abs().clamp(0.0, 180.0);
    for w in T.windows(2) {
        if a <= w[1].0 {
            let t = (a - w[0].0) / (w[1].0 - w[0].0);
            return w[0].1 + (w[1].1 - w[0].1) * t;
        }
    }
    0.75
}

/// Heading (degrees, 0 = north, clockwise) of a direction on the XZ plane.
pub fn heading_of(d: Vec2) -> f32 {
    d.x.atan2(-d.y).to_degrees().rem_euclid(360.0)
}

/// Unit direction of a heading.
pub fn dir_of(heading: f32) -> Vec2 {
    let y = heading.to_radians();
    Vec2::new(y.sin(), -y.cos())
}

/// Signed shortest turn from `from` to `to` (degrees).
pub fn turn_to(from: f32, to: f32) -> f32 {
    (to - from + 540.0).rem_euclid(360.0) - 180.0
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    #[default]
    Port,
    Starboard,
}

/// What the helm asks for this tick.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Helm {
    /// -1 lower the sails a level, +1 raise them (edges, not held).
    pub sail: i32,
    /// Rudder -1 (port) .. 1 (starboard).
    pub rudder: f32,
    pub boost: bool,
    /// Drop anchor: stop dead (furled).
    pub anchor: bool,
}

/// A weapon's readiness.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Guns {
    pub port: f32,
    pub starboard: f32,
    pub chain: f32,
    pub kegs: f32,
    pub swivel: f32,
    pub kegs_left: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Faction {
    #[default]
    Brotherhood,
    Temperance,
    Merchant,
}

/// One ship in play.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Vessel {
    pub id: u32,
    pub class: String,
    pub name: String,
    pub faction: Faction,
    /// Position (x, z) and heading (degrees, 0 = north).
    pub pos: Vec2,
    pub heading: f32,
    pub speed: f32,
    pub yaw_rate: f32,
    /// Visual: heave (m), pitch and roll (degrees, + = bow up / heel to starboard).
    pub heave: f32,
    pub pitch: f32,
    pub roll: f32,
    pub sail: u8,
    /// Smoothed sail (0..2) for drawing.
    pub sail_shown: f32,
    pub rudder: f32,
    pub hull: f32,
    pub hull_max: f32,
    /// Sails' condition 0..1 (chain shot tears them).
    pub sails: f32,
    pub crew: u32,
    /// Seconds of chain-shot slowdown left.
    pub torn: f32,
    pub boost: f32,
    pub boost_cd: f32,
    pub brace: f32,
    pub brace_cd: f32,
    pub guns: Guns,
    /// Seconds before this ship's next volley (enemies: gunports glow while > 0).
    pub telegraph: f32,
    pub telegraph_side: Side,
    pub fire: f32,
    pub sinking: f32,
    pub sunk: bool,
    pub disabled: bool,
    pub anchored: bool,
    pub grounded: f32,
    /// Damage taken from each weapon kind (optional objectives: "disabled with chain shot").
    pub took: BTreeMap<String, f32>,
    /// Free-form tag the game uses ("gunboat", "restraint").
    pub tag: String,
    pub captain: Option<Captain>,
}

impl Vessel {
    pub fn new(id: u32, class: &HullClass, class_key: &str, name: &str, pos: Vec2, heading: f32) -> Self {
        Self {
            id,
            class: class_key.to_string(),
            name: name.to_string(),
            faction: Faction::Temperance,
            pos,
            heading,
            speed: 0.0,
            yaw_rate: 0.0,
            heave: 0.0,
            pitch: 0.0,
            roll: 0.0,
            sail: 0,
            sail_shown: 0.0,
            rudder: 0.0,
            hull: class.hp,
            hull_max: class.hp,
            sails: 1.0,
            crew: class.crew,
            torn: 0.0,
            boost: 0.0,
            boost_cd: 0.0,
            brace: 0.0,
            brace_cd: 0.0,
            guns: Guns { kegs_left: class.kegs, ..Default::default() },
            telegraph: 0.0,
            telegraph_side: Side::Port,
            fire: 0.0,
            sinking: 0.0,
            sunk: false,
            disabled: false,
            anchored: false,
            grounded: 0.0,
            took: BTreeMap::new(),
            tag: String::new(),
            captain: None,
        }
    }

    pub fn forward(&self) -> Vec2 {
        dir_of(self.heading)
    }

    /// Unit vector to starboard (right).
    pub fn right(&self) -> Vec2 {
        let f = self.forward();
        Vec2::new(-f.y, f.x)
    }

    pub fn alive(&self) -> bool {
        !self.sunk && self.sinking <= 0.0
    }

    pub fn frac(&self) -> f32 {
        (self.hull / self.hull_max.max(1.0)).clamp(0.0, 1.0)
    }

    /// A point on the ship (x right, y up from the waterline, z forward) in the world, with
    /// heave (yaw only: decks stay level for walkers; pitch and roll are drawn).
    pub fn world(&self, local: Vec3) -> Vec3 {
        let f = self.forward();
        let r = self.right();
        let xz = self.pos + r * local.x + f * local.z;
        Vec3::new(xz.x, local.y + self.heave - self.sinking * 6.0, xz.y)
    }

    /// The world point with the drawn pitch and roll too (masts, sails, rigging).
    pub fn world_tilted(&self, local: Vec3) -> Vec3 {
        let p = self.pitch.to_radians();
        let r = self.roll.to_radians();
        let mut l = local;
        // Roll about the bow axis, then pitch about the beam axis.
        let (sr, cr) = r.sin_cos();
        l = Vec3::new(l.x * cr - l.y * sr, l.x * sr + l.y * cr, l.z);
        let (sp, cp) = p.sin_cos();
        l = Vec3::new(l.x, l.y * cp + l.z * sp, -l.y * sp + l.z * cp);
        self.world(l)
    }

    /// The angle off the wind (0 = into it) for a wind blowing toward `wind`.
    pub fn off_wind(&self, wind: Vec2) -> f32 {
        if wind.length() < 1e-3 {
            return 90.0;
        }
        let from = heading_of(-wind);
        turn_to(self.heading, from).abs()
    }

    /// Speed it would make at this sail level now.
    pub fn target_speed(&self, c: &HullClass, wind: Vec2) -> f32 {
        let top = match self.sail {
            0 => 0.0,
            1 => c.speed_half,
            _ => c.speed_full,
        };
        let torn = if self.torn > 0.0 { 0.5 } else { 1.0 };
        let wind_strength = (wind.length() / 6.0).clamp(0.6, 1.3);
        top * wind_factor(self.off_wind(wind))
            * (0.4 + 0.6 * self.sails)
            * torn
            * wind_strength
            * (1.0 + 0.25 * (self.boost > 0.0) as i32 as f32)
    }

    /// One tick of sailing. `seabed(x, z)` is the ground height (negative under water).
    pub fn sail_step(
        &mut self,
        c: &HullClass,
        helm: Helm,
        wind: Vec2,
        ocean: Option<&Ocean>,
        seabed: &dyn Fn(Vec2) -> f32,
        dt: f32,
    ) -> Vec<NavalEvent> {
        let mut ev = Vec::new();
        self.torn = (self.torn - dt).max(0.0);
        self.boost = (self.boost - dt).max(0.0);
        self.boost_cd = (self.boost_cd - dt).max(0.0);
        self.brace = (self.brace - dt).max(0.0);
        self.brace_cd = (self.brace_cd - dt).max(0.0);
        for t in [&mut self.guns.port, &mut self.guns.starboard, &mut self.guns.chain, &mut self.guns.kegs, &mut self.guns.swivel]
        {
            *t = (*t - dt).max(0.0);
        }
        if self.sinking > 0.0 {
            self.sinking = (self.sinking + dt / 8.0).min(1.0);
            self.speed *= 1.0 - dt;
            if self.sinking >= 1.0 && !self.sunk {
                self.sunk = true;
                ev.push(NavalEvent::Sunk(self.id));
            }
            self.pos += self.forward() * self.speed * dt;
            return ev;
        }
        if self.disabled {
            // Drifting: no sails, no helm.
            self.sail = 0;
        } else {
            if helm.sail != 0 {
                self.sail = (self.sail as i32 + helm.sail).clamp(0, 2) as u8;
                self.anchored = false;
            }
            if helm.boost && self.boost_cd <= 0.0 && self.sail > 0 {
                self.boost = 3.0;
                self.boost_cd = 10.0;
                ev.push(NavalEvent::Boost(self.id));
            }
            if helm.anchor {
                self.sail = 0;
                self.anchored = true;
            }
        }
        self.sail_shown += (self.sail as f32 - self.sail_shown) * (1.0 - (-2.0 * dt).exp());
        let target = self.target_speed(c, wind);
        let rate = if target > self.speed { c.accel } else { c.brake * if self.anchored { 3.0 } else { 1.0 } };
        self.speed += (target - self.speed).clamp(-rate * dt, rate * dt);
        // Turning: slow at a standstill, quickest at mid speed.
        let mid = c.speed_full * 0.5;
        let s = self.speed.abs();
        let turn = if s < mid {
            c.turn_still + (c.turn_mid - c.turn_still) * (s / mid.max(0.1))
        } else {
            c.turn_mid + (c.turn_full - c.turn_mid) * ((s - mid) / (c.speed_full - mid).max(0.1)).min(1.0)
        };
        let rudder = if self.disabled { 0.0 } else { helm.rudder.clamp(-1.0, 1.0) };
        self.rudder += (rudder - self.rudder) * (1.0 - (-6.0 * dt).exp());
        let want_rate = self.rudder * turn;
        self.yaw_rate += (want_rate - self.yaw_rate) * (1.0 - (-3.0 * dt).exp());
        self.heading = (self.heading + self.yaw_rate * dt).rem_euclid(360.0);
        let step = self.forward() * self.speed * dt;
        let next = self.pos + step;
        // Grounding: the bow and the middle must have water under the keel.
        let bow = next + self.forward() * c.length * 0.45;
        let depth_ok = |p: Vec2| seabed(p) < -c.draft;
        if depth_ok(bow) && depth_ok(next) {
            self.pos = next;
            self.grounded = 0.0;
        } else {
            if self.grounded <= 0.0 && self.speed > 4.0 {
                let dmg = (self.speed - 4.0) * 25.0;
                self.hull -= dmg;
                ev.push(NavalEvent::Grounded { id: self.id, damage: dmg });
            }
            self.grounded += dt;
            // Push back out along the way we came and stop.
            self.speed = -0.8;
            self.pos -= self.forward() * 0.6;
        }
        // Visual motion from the sea under the hull.
        if let Some(o) = ocean {
            let f = self.forward();
            let r = self.right();
            let h = |p: Vec2| o.height(p);
            let (hb, hs) = (h(self.pos + f * c.length * 0.4), h(self.pos - f * c.length * 0.4));
            let (hp, hst) = (h(self.pos - r * c.beam * 0.5), h(self.pos + r * c.beam * 0.5));
            let hc = h(self.pos);
            let k = 1.0 - (-5.0 * dt).exp();
            self.heave += ((hc + hb + hs) / 3.0 - self.heave) * k;
            let pitch = ((hb - hs) / (c.length * 0.8)).atan().to_degrees().clamp(-12.0, 12.0);
            let heel = -self.yaw_rate * self.speed * 0.04;
            let roll = (((hst - hp) / c.beam).atan().to_degrees() + heel).clamp(-14.0, 14.0);
            self.pitch += (pitch - self.pitch) * k;
            self.roll += (roll - self.roll) * k;
        }
        // Fire burns the hull until put out.
        if self.fire > 0.0 {
            self.fire -= dt;
            self.hull -= 6.0 * dt;
        }
        if self.hull <= 0.0 && self.sinking <= 0.0 {
            self.hull = 0.0;
        }
        ev
    }

    /// Damage from a hit: brace halves it; returns the damage done.
    pub fn damage(&mut self, amount: f32, kind: &str) -> f32 {
        if !self.alive() {
            return 0.0;
        }
        let d = if self.brace > 0.0 { amount * 0.5 } else { amount };
        self.hull = (self.hull - d).max(0.0);
        *self.took.entry(kind.to_string()).or_default() += d;
        if self.frac() < 0.5 && self.fire <= 0.0 && self.frac() > 0.0 {
            self.fire = 12.0;
        }
        d
    }

    /// Updates disabled and sinking for an enemy (25% disables, 0 sinks); the player's ship is
    /// handled by the game (0 = Spilled).
    pub fn update_state(&mut self, can_be_disabled: bool) -> Option<NavalEvent> {
        if self.sunk || self.sinking > 0.0 {
            return None;
        }
        if self.hull <= 0.0 {
            self.sinking = 0.001;
            return Some(NavalEvent::Sinking(self.id));
        }
        if can_be_disabled && !self.disabled && self.frac() < 0.25 {
            self.disabled = true;
            self.sail = 0;
            return Some(NavalEvent::Disabled(self.id));
        }
        None
    }

    /// Readiness 0..1 of a weapon (for the HUD).
    pub fn ready(&self, c: &HullClass, w: Weapon) -> f32 {
        let (left, full) = match w {
            Weapon::Broadside(Side::Port) => (self.guns.port, c.reload),
            Weapon::Broadside(Side::Starboard) => (self.guns.starboard, c.reload),
            Weapon::Chain => (self.guns.chain, c.reload * 1.2),
            Weapon::Kegs => (self.guns.kegs, 15.0),
            Weapon::Swivel => (self.guns.swivel, 2.0),
        };
        1.0 - (left / full.max(0.1)).clamp(0.0, 1.0)
    }

    /// Fires a weapon: queues its shots in `out` and starts its reload. `elevation` in radians
    /// (aim assist or the player's look), `aim_yaw` the horizontal aim (heading, degrees).
    pub fn fire(
        &mut self,
        c: &HullClass,
        w: Weapon,
        elevation: f32,
        aim_yaw: Option<f32>,
        out: &mut ballistics::Ballistics,
        heavy: f32,
    ) -> bool {
        if !self.alive() || self.disabled {
            return false;
        }
        let vel_ship = self.forward() * self.speed;
        let owner = Owner::Vessel(self.id);
        let deck = c.freeboard * 0.6;
        match w {
            Weapon::Broadside(side) => {
                let slot = match side {
                    Side::Port => &mut self.guns.port,
                    Side::Starboard => &mut self.guns.starboard,
                };
                if *slot > 0.0 {
                    return false;
                }
                *slot = c.reload;
                let out_dir = match side {
                    Side::Port => -self.right(),
                    Side::Starboard => self.right(),
                };
                let n = c.guns_per_side.max(1);
                for i in 0..n {
                    let along = (i as f32 / (n.max(2) - 1) as f32 - 0.5) * c.length * 0.6;
                    let local = Vec3::new(if side == Side::Port { -c.beam * 0.5 } else { c.beam * 0.5 }, deck, along);
                    let from = self.world(local);
                    // A little spread (±0.8°) from the gun index, deterministic.
                    let spread = ((i as f32 * 1.7).sin() * 0.8).to_radians();
                    let d = rotate(out_dir, spread);
                    let v = c.muzzle;
                    let dir3 = Vec3::new(d.x * elevation.cos(), elevation.sin(), d.y * elevation.cos());
                    out.fire(Shot {
                        pos: from,
                        vel: dir3 * v + Vec3::new(vel_ship.x, 0.0, vel_ship.y),
                        kind: ShotKind::Round,
                        owner,
                        damage: c.ball_damage * heavy,
                        blast: 0.0,
                        delay: i as f32 * 0.4 / n as f32,
                        life: 8.0,
                    });
                }
                true
            }
            Weapon::Chain => {
                if self.guns.chain > 0.0 {
                    return false;
                }
                self.guns.chain = c.reload * 1.2;
                let fwd = aim_yaw.map(dir_of).unwrap_or(self.forward());
                for i in 0..c.chasers.max(1) {
                    let off = if i % 2 == 0 { -0.8 } else { 0.8 };
                    let from = self.world(Vec3::new(off, deck + 0.8, c.length * 0.48));
                    let el = elevation.max(0.03);
                    let dir3 = Vec3::new(fwd.x * el.cos(), el.sin(), fwd.y * el.cos());
                    out.fire(Shot {
                        pos: from,
                        vel: dir3 * c.muzzle * 0.9 + Vec3::new(vel_ship.x, 0.0, vel_ship.y),
                        kind: ShotKind::Chain,
                        owner,
                        damage: c.ball_damage * 0.4,
                        blast: 0.0,
                        delay: i as f32 * 0.15,
                        life: 3.0,
                    });
                }
                true
            }
            Weapon::Swivel => {
                if self.guns.swivel > 0.0 {
                    return false;
                }
                self.guns.swivel = 2.0;
                let fwd = aim_yaw.map(dir_of).unwrap_or(self.forward());
                let from = self.world(Vec3::new(0.0, deck + 1.2, c.length * 0.3));
                let el = elevation;
                let dir3 = Vec3::new(fwd.x * el.cos(), el.sin(), fwd.y * el.cos());
                out.fire(Shot {
                    pos: from,
                    vel: dir3 * 110.0,
                    kind: ShotKind::Swivel,
                    owner,
                    damage: 45.0,
                    blast: 0.0,
                    delay: 0.0,
                    life: 2.0,
                });
                true
            }
            Weapon::Kegs => {
                if self.guns.kegs > 0.0 || self.guns.kegs_left == 0 {
                    return false;
                }
                self.guns.kegs = 15.0;
                self.guns.kegs_left -= 1;
                true
            }
        }
    }
}

fn rotate(v: Vec2, a: f32) -> Vec2 {
    let (s, c) = a.sin_cos();
    Vec2::new(v.x * c - v.y * s, v.x * s + v.y * c)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Weapon {
    Broadside(Side),
    Chain,
    Swivel,
    Kegs,
}

impl Weapon {
    pub fn name(self) -> &'static str {
        match self {
            Weapon::Broadside(Side::Port) => "port",
            Weapon::Broadside(Side::Starboard) => "starboard",
            Weapon::Chain => "chain",
            Weapon::Swivel => "swivel",
            Weapon::Kegs => "kegs",
        }
    }
}

/// Which weapon looking in a direction chooses (relative bearing in degrees, 0 = ahead,
/// + = to starboard): ±45° of the beam → that broadside, ahead → chain shot, astern → kegs.
pub fn weapon_for_bearing(rel: f32) -> Weapon {
    let r = turn_to(0.0, rel);
    if (45.0..=135.0).contains(&r) {
        Weapon::Broadside(Side::Starboard)
    } else if (-135.0..=-45.0).contains(&r) {
        Weapon::Broadside(Side::Port)
    } else if r.abs() < 45.0 {
        Weapon::Chain
    } else {
        Weapon::Kegs
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NavalEvent {
    Boost(u32),
    Grounded { id: u32, damage: f32 },
    Disabled(u32),
    Sinking(u32),
    Sunk(u32),
}

// ---------------------------------------------------------------- AI captains

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptainKind {
    /// Bow-on charges and circling; a bow gun at 60–120 m.
    Gunboat,
    /// Holds a broadside angle at 60–110 m; chain shot up close; flees under 30%.
    Schooner,
    /// Pursues and fires its chasers; steers round hazards.
    Frigate,
    /// Flees while its escorts fight.
    Merchant,
    /// Sails a route of waypoints and ignores the player (patrols, the title backdrop).
    Patrol,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Captain {
    pub kind: CaptainKind,
    /// Engaged (hostile) or not.
    pub hostile: bool,
    /// Route for patrols (looping) and a waypoint index.
    pub route: Vec<Vec2>,
    pub leg: usize,
    /// Where to run to when fleeing.
    pub refuge: Option<Vec2>,
    /// Seconds into its current manoeuvre (circling side switches).
    pub clock: f32,
    pub circle_side: f32,
}

impl Captain {
    pub fn new(kind: CaptainKind) -> Self {
        Self { kind, hostile: true, route: Vec::new(), leg: 0, refuge: None, clock: 0.0, circle_side: 1.0 }
    }
}

/// What an AI captain wants this tick.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Orders {
    pub helm: Helm,
    /// Fire this weapon now (the game checks readiness) and the elevation to use.
    pub fire: Option<(Weapon, f32)>,
    /// Start the volley telegraph (gunports glow) for this side.
    pub telegraph: Option<Side>,
}

/// A heading that keeps clear of shallows: tries `want`, then fans out either side until the
/// water `look` metres ahead is deeper than `draft` + 1.5 m.
pub fn safe_heading(pos: Vec2, want: f32, look: f32, draft: f32, seabed: &dyn Fn(Vec2) -> f32) -> f32 {
    let clear = |h: f32| (1..=4).all(|k| seabed(pos + dir_of(h) * look * k as f32 / 4.0) < -(draft + 1.5));
    if clear(want) {
        return want;
    }
    for step in 1..=12 {
        let d = step as f32 * 15.0;
        for h in [want + d, want - d] {
            if clear(h) {
                return h.rem_euclid(360.0);
            }
        }
    }
    want
}

/// Steers toward a heading: rudder from the turn needed.
pub fn rudder_for(v: &Vessel, want: f32) -> f32 {
    (turn_to(v.heading, want) / 25.0).clamp(-1.0, 1.0)
}

/// A heading toward `goal` that never points into the wind (tacks at ±50° off it).
pub fn wind_aware(from: Vec2, goal: Vec2, wind: Vec2, tack: f32) -> f32 {
    let want = heading_of(goal - from);
    if wind.length() < 0.1 {
        return want;
    }
    let into = heading_of(-wind);
    let off = turn_to(into, want);
    if off.abs() >= 45.0 {
        return want;
    }
    (into + 50.0 * if tack >= 0.0 { 1.0 } else { -1.0 }).rem_euclid(360.0)
}

/// Decides an AI ship's orders against a target ship (`target`: position, heading, speed).
pub fn captain_orders(
    v: &mut Vessel,
    c: &HullClass,
    target: Option<(Vec2, f32, f32)>,
    wind: Vec2,
    seabed: &dyn Fn(Vec2) -> f32,
    dt: f32,
) -> Orders {
    let mut o = Orders::default();
    let Some(cap) = v.captain.clone() else { return o };
    if !v.alive() || v.disabled {
        return o;
    }
    let mut cap = cap;
    cap.clock += dt;
    let look = (v.speed * 6.0).max(40.0);
    let mut want = v.heading;
    let mut sail = 2u8;
    match cap.kind {
        CaptainKind::Patrol => {
            if !cap.route.is_empty() {
                let goal = cap.route[cap.leg % cap.route.len()];
                if goal.distance(v.pos) < 35.0 {
                    cap.leg = (cap.leg + 1) % cap.route.len();
                }
                want = wind_aware(v.pos, goal, wind, (cap.leg % 2) as f32 * 2.0 - 1.0);
                sail = 1;
            }
        }
        CaptainKind::Merchant => {
            if let Some((tp, _, _)) = target.filter(|t| t.0.distance(v.pos) < 220.0) {
                want = heading_of(v.pos - tp);
            } else if !cap.route.is_empty() {
                let goal = cap.route[cap.leg % cap.route.len()];
                if goal.distance(v.pos) < 35.0 {
                    cap.leg = (cap.leg + 1) % cap.route.len();
                }
                want = heading_of(goal - v.pos);
                sail = 1;
            }
        }
        CaptainKind::Gunboat | CaptainKind::Schooner | CaptainKind::Frigate if cap.hostile => {
            let Some((tp, _, _)) = target else {
                v.captain = Some(cap);
                return o;
            };
            let to = tp - v.pos;
            let dist = to.length();
            let bearing = heading_of(to);
            match cap.kind {
                CaptainKind::Gunboat => {
                    if dist > 110.0 {
                        want = bearing;
                    } else {
                        // Circle the target, nose in toward it.
                        if cap.clock > 14.0 {
                            cap.clock = 0.0;
                            cap.circle_side = -cap.circle_side;
                        }
                        want = (bearing + 70.0 * cap.circle_side).rem_euclid(360.0);
                    }
                    let off = turn_to(v.heading, bearing).abs();
                    if (40.0..130.0).contains(&dist) && off < 20.0 && v.guns.chain <= 0.0 {
                        let el = ballistics::elevation_for(dist, 0.0, c.muzzle).unwrap_or(0.05);
                        o.fire = Some((Weapon::Chain, el));
                    }
                }
                CaptainKind::Schooner => {
                    if v.frac() < 0.3 {
                        if let Some(r) = cap.refuge {
                            want = heading_of(r - v.pos);
                        }
                    } else if dist > 110.0 {
                        want = bearing;
                    } else {
                        // Keep the target on the beam (the side it is already on), easing
                        // in or out to stay in 60–110 m.
                        let side = if turn_to(v.heading, bearing) >= 0.0 { 1.0 } else { -1.0 };
                        let lean = if dist < 60.0 {
                            -20.0
                        } else if dist > 95.0 {
                            20.0
                        } else {
                            0.0
                        };
                        want = (bearing - side * (90.0 - lean)).rem_euclid(360.0);
                        sail = 1;
                        let rel = turn_to(v.heading, bearing);
                        let s = if rel > 0.0 { Side::Starboard } else { Side::Port };
                        let aligned = (rel.abs() - 90.0).abs() < 22.0;
                        let loaded = match s {
                            Side::Port => v.guns.port <= 0.0,
                            Side::Starboard => v.guns.starboard <= 0.0,
                        };
                        if aligned && loaded {
                            if v.telegraph <= 0.0 {
                                v.telegraph = 1.5;
                                v.telegraph_side = s;
                                o.telegraph = Some(s);
                            }
                        }
                        if dist < 55.0 && turn_to(v.heading, bearing).abs() < 25.0 && v.guns.chain <= 0.0 {
                            o.fire = Some((Weapon::Chain, 0.03));
                        }
                    }
                }
                CaptainKind::Frigate => {
                    want = bearing;
                    let off = turn_to(v.heading, bearing).abs();
                    if dist < 160.0 && off < 15.0 && v.guns.chain <= 0.0 {
                        let el = ballistics::elevation_for(dist, 0.0, c.muzzle).unwrap_or(0.05);
                        o.fire = Some((Weapon::Chain, el));
                    }
                    // Broadside when the target is on the beam close by.
                    let rel = turn_to(v.heading, bearing);
                    if dist < 110.0 && (rel.abs() - 90.0).abs() < 20.0 && v.telegraph <= 0.0 {
                        let s = if rel > 0.0 { Side::Starboard } else { Side::Port };
                        v.telegraph = 1.5;
                        v.telegraph_side = s;
                        o.telegraph = Some(s);
                    }
                }
                _ => {}
            }
        }
        _ => {}
    }
    // A volley fires when its telegraph runs out.
    if v.telegraph > 0.0 {
        v.telegraph -= dt;
        if v.telegraph <= 0.0 {
            let dist = target.map(|t| t.0.distance(v.pos)).unwrap_or(80.0);
            let el = ballistics::elevation_for(dist, 0.0, c.muzzle).unwrap_or(0.05);
            o.fire = Some((Weapon::Broadside(v.telegraph_side), el));
        }
    }
    let want = safe_heading(v.pos, want, look, c.draft, seabed);
    o.helm = Helm { sail: (sail as i32 - v.sail as i32).signum(), rudder: rudder_for(v, want), boost: false, anchor: false };
    v.captain = Some(cap);
    o
}

// ---------------------------------------------------------------- forts

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BatteryKind {
    /// Telegraphed volleys of round shot.
    Guns,
    /// Shells on a high arc; the landing point is marked from launch.
    Mortar,
}

/// A shore emplacement.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Battery {
    pub id: u32,
    pub name: String,
    pub kind: BatteryKind,
    /// Gun position (world) and how many guns.
    pub pos: Vec3,
    pub guns: u32,
    pub hp: f32,
    pub hp_max: f32,
    pub range: f32,
    pub reload: f32,
    pub cooldown: f32,
    /// Seconds before the next volley (> 0 = telegraphing).
    pub telegraph: f32,
    pub destroyed: bool,
    /// Half extents of its hit box.
    pub half: Vec3,
}

/// A mortar shell's marked landing point.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MarkedShell {
    pub at: Vec3,
    pub radius: f32,
    /// Seconds until impact.
    pub left: f32,
}

impl Battery {
    /// One tick against a target ship (position, velocity): fires telegraphed volleys or
    /// mortar shells into `out`, returns a new landing mark for mortars.
    pub fn step(
        &mut self,
        target: Option<(Vec3, Vec3)>,
        out: &mut ballistics::Ballistics,
        dt: f32,
    ) -> (Option<MarkedShell>, bool) {
        if self.destroyed {
            return (None, false);
        }
        self.cooldown = (self.cooldown - dt).max(0.0);
        let Some((tp, tv)) = target else {
            self.telegraph = 0.0;
            return (None, false);
        };
        let dist = Vec2::new(tp.x - self.pos.x, tp.z - self.pos.z).length();
        if dist > self.range {
            self.telegraph = 0.0;
            return (None, false);
        }
        let mut started = false;
        if self.telegraph <= 0.0 && self.cooldown <= 0.0 {
            self.telegraph = if self.kind == BatteryKind::Mortar { 0.4 } else { 1.6 };
            started = true;
        }
        if self.telegraph > 0.0 {
            self.telegraph -= dt;
            if self.telegraph <= 0.0 {
                self.cooldown = self.reload;
                match self.kind {
                    BatteryKind::Guns => {
                        let flight = dist / 75.0;
                        let aim = tp + tv * flight;
                        let to = Vec2::new(aim.x - self.pos.x, aim.z - self.pos.z);
                        let d = to.normalize_or(Vec2::X);
                        let el = ballistics::elevation_for(to.length(), aim.y - self.pos.y, 75.0).unwrap_or(0.1);
                        for i in 0..self.guns {
                            let spread = ((i as f32 * 2.3).sin() * 1.2).to_radians();
                            let dd = rotate(d, spread);
                            let v = Vec3::new(dd.x * el.cos(), el.sin(), dd.y * el.cos()) * 75.0;
                            out.fire(Shot {
                                pos: self.pos + Vec3::new(0.0, 0.5, 0.0),
                                vel: v,
                                kind: ShotKind::Round,
                                owner: Owner::Battery(self.id),
                                damage: 26.0,
                                blast: 0.0,
                                delay: i as f32 * 0.25,
                                life: 8.0,
                            });
                        }
                    }
                    BatteryKind::Mortar => {
                        // A high lob that takes ~2.2 s, aimed where the ship will be (slightly
                        // short of perfect, so steering out of the ring works).
                        let t = 2.2;
                        let aim = tp + tv * t * 0.85;
                        let d = aim - self.pos;
                        let vel = Vec3::new(d.x / t, (d.y + 0.5 * ballistics::GRAVITY * t * t) / t, d.z / t);
                        out.fire(Shot {
                            pos: self.pos,
                            vel,
                            kind: ShotKind::Mortar,
                            owner: Owner::Battery(self.id),
                            damage: 60.0,
                            blast: 9.0,
                            delay: 0.0,
                            life: 4.0,
                        });
                        return (Some(MarkedShell { at: Vec3::new(aim.x, 0.0, aim.z), radius: 9.0, left: t }), started);
                    }
                }
            }
        }
        (None, started)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn beam_reach_is_fastest_and_irons_slowest() {
        assert!(wind_factor(90.0) > wind_factor(135.0));
        assert!(wind_factor(135.0) > wind_factor(180.0));
        assert!(wind_factor(10.0) < 0.2);
        let c = HullClass::default();
        let wind = Vec2::new(6.0, 0.0); // blowing east (from the west)
        let mut beam = Vessel::new(1, &c, "s", "a", Vec2::ZERO, 0.0); // heading north: beam reach
        let mut irons = Vessel::new(2, &c, "s", "b", Vec2::ZERO, 270.0); // heading west: into the wind
        for v in [&mut beam, &mut irons] {
            v.sail = 2;
            for _ in 0..1200 {
                v.sail_step(&c, Helm::default(), wind, None, &|_| -50.0, 1.0 / 60.0);
            }
        }
        assert!(beam.speed > c.speed_full * 0.9, "{}", beam.speed);
        assert!(irons.speed < c.speed_full * 0.3, "{}", irons.speed);
    }

    #[test]
    fn ships_ground_on_shallows() {
        let c = HullClass::default();
        let mut v = Vessel::new(1, &c, "s", "a", Vec2::ZERO, 0.0);
        v.sail = 2;
        // A beach 100 m north.
        let seabed = |p: Vec2| if p.y < -100.0 { 1.0 } else { -20.0 };
        for _ in 0..3000 {
            v.sail_step(&c, Helm::default(), Vec2::new(6.0, 0.0), None, &seabed, 1.0 / 60.0);
        }
        assert!(v.pos.y > -100.0 + c.length * 0.4 - 1.0, "ship went through the island: {:?}", v.pos);
        assert!(v.hull < c.hp, "grounding at speed hurts");
    }

    #[test]
    fn weapon_by_bearing() {
        assert_eq!(weapon_for_bearing(90.0), Weapon::Broadside(Side::Starboard));
        assert_eq!(weapon_for_bearing(-80.0), Weapon::Broadside(Side::Port));
        assert_eq!(weapon_for_bearing(10.0), Weapon::Chain);
        assert_eq!(weapon_for_bearing(175.0), Weapon::Kegs);
    }
}
