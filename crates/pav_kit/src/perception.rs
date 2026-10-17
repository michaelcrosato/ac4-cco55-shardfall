//! Guards' senses and minds: sight cones (near/far ranges, horizontal and vertical field of
//! view, peripheral vision), concealment, hearing, an awareness meter per guard and its states
//! (Unaware → Suspicious → Alerted/Combat, then Investigating → Searching → Returning), and
//! heat (notoriety) with wanted posters. Line of sight comes from the game (raycasts).

use glam::{Vec2, Vec3};
use pav_core::play::Awareness;
use serde::{Deserialize, Serialize};

/// An archetype's senses (data).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Senses {
    /// Within `near` the meter fills fast; out to `far` slowly.
    pub near: f32,
    pub far: f32,
    /// Horizontal field of view (degrees, full angle) and the vertical limits (up, down).
    pub fov: f32,
    pub up: f32,
    pub down: f32,
    /// Peripheral field (full angle) and its rate factor.
    pub peripheral: f32,
    pub peripheral_rate: f32,
    /// Meter per second at near and at far range.
    pub rate_near: f32,
    pub rate_far: f32,
    /// Hearing multiplier on noise radii.
    pub hearing: f32,
}

impl Default for Senses {
    fn default() -> Self {
        Self {
            near: 8.0,
            far: 28.0,
            fov: 110.0,
            up: 35.0,
            down: 50.0,
            peripheral: 160.0,
            peripheral_rate: 0.3,
            rate_near: 1.6,
            rate_far: 0.35,
            hearing: 1.0,
        }
    }
}

/// How the target looks from a guard's eye (geometry only; the game adds line of sight).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Sighting {
    pub dist: f32,
    /// In the main cone, in the peripheral band, or out of sight.
    pub central: bool,
    pub peripheral: bool,
    /// Above the vertical limit (a player high on a roof).
    pub too_high: bool,
}

/// The cone test. `facing` in radians (0 = +Z, as characters use).
pub fn look(eye: Vec3, facing: f32, target: Vec3, s: &Senses, searching: bool) -> Sighting {
    let d = target - eye;
    let flat = Vec2::new(d.x, d.z);
    let dist = d.length();
    let f = Vec2::new(facing.sin(), facing.cos());
    let ang = f.angle_to(flat.normalize_or(f)).abs().to_degrees();
    let elev = d.y.atan2(flat.length().max(1e-3)).to_degrees();
    // Searching guards look up too.
    let up = if searching { 70.0 } else { s.up };
    let too_high = elev > up || elev < -s.down;
    Sighting {
        dist,
        central: ang <= s.fov * 0.5 && !too_high && dist <= s.far,
        peripheral: ang <= s.peripheral * 0.5 && !too_high && dist <= s.near * 1.5,
        too_high,
    }
}

/// Things that change how fast a guard notices.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Exposure {
    /// The target is in stalk grass, a crowd or a hay cart (and not high-profile).
    pub concealed: bool,
    /// Fully hidden (haystack, hay cart): never seen.
    pub hidden: bool,
    pub crouched: bool,
    pub sprinting: bool,
    /// In a restricted area (the fort): twice as fast, instant at near range.
    pub restricted: bool,
    /// Heat level 0..3.
    pub heat: u8,
    /// Line of sight to the target's head, chest or feet.
    pub visible: bool,
}

/// Meter per second this guard gains on the target now (0 = not seen).
pub fn sight_rate(s: &Senses, sight: &Sighting, e: &Exposure) -> f32 {
    if !e.visible || e.hidden || (!sight.central && !sight.peripheral) {
        return 0.0;
    }
    if e.concealed && sight.dist > 1.5 {
        return 0.0;
    }
    let range = if e.crouched { 0.6 } else { 1.0 };
    let (near, far) = (s.near * range, s.far * range);
    if sight.dist > far {
        return 0.0;
    }
    let mut rate = if sight.dist <= near {
        s.rate_near
    } else {
        s.rate_far + (s.rate_near - s.rate_far) * (1.0 - (sight.dist - near) / (far - near).max(0.1))
    };
    if !sight.central {
        rate *= s.peripheral_rate;
    }
    if e.sprinting {
        rate *= 1.4;
    }
    if e.restricted {
        rate *= 2.0;
        if sight.dist <= near && sight.central {
            rate = 50.0;
        }
    }
    rate *= 1.0 + 0.4 * e.heat as f32;
    if e.heat >= 2 && sight.dist <= near && sight.central {
        rate = rate.max(10.0);
    }
    rate
}

/// A guard's state of mind about the player.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Mind {
    pub meter: f32,
    pub state: Awareness,
    /// Seconds in the current state.
    pub t: f32,
    pub last_known: Option<Vec3>,
    /// Seconds since the target was last seen.
    pub unseen: f32,
    /// A noise to check.
    pub noise: Option<Vec3>,
    /// Extra alertness after a search (seconds left).
    pub wary: f32,
}

impl Default for Mind {
    fn default() -> Self {
        Self { meter: 0.0, state: Awareness::Unaware, t: 0.0, last_known: None, unseen: 99.0, noise: None, wary: 0.0 }
    }
}

/// A change of state worth a bark, a sound or a showcase note.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MindEvent {
    Suspicious,
    Alerted,
    LostTarget,
    Investigating,
    GaveUp,
}

impl Mind {
    pub fn aware(&self) -> bool {
        matches!(self.state, Awareness::Alerted | Awareness::Combat)
    }

    fn go(&mut self, s: Awareness) {
        self.state = s;
        self.t = 0.0;
    }

    /// Alerts this guard straight away (a shout from an ally, an alarm bell, a hit).
    pub fn alert(&mut self, at: Vec3) -> Option<MindEvent> {
        self.meter = 1.0;
        self.last_known = Some(at);
        self.unseen = 0.0;
        if !self.aware() {
            self.go(Awareness::Alerted);
            return Some(MindEvent::Alerted);
        }
        None
    }

    /// One tick: `rate` from [`sight_rate`] (0 = not seen), the target's position when seen,
    /// a heard noise. `arrived` tells the mind its body reached where it was going
    /// (investigating a noise).
    pub fn update(&mut self, rate: f32, target: Vec3, heard: Option<Vec3>, arrived: bool, dt: f32) -> Option<MindEvent> {
        self.t += dt;
        self.wary = (self.wary - dt).max(0.0);
        let seen = rate > 0.0;
        if seen {
            self.unseen = 0.0;
            self.last_known = Some(target);
            let wary = if self.wary > 0.0 { 1.5 } else { 1.0 };
            self.meter = (self.meter + rate * wary * dt).min(1.0);
        } else {
            self.unseen += dt;
        }
        match self.state {
            Awareness::Unaware | Awareness::Suspicious | Awareness::Investigating | Awareness::Searching => {
                if self.meter >= 1.0 {
                    self.go(Awareness::Combat);
                    return Some(MindEvent::Alerted);
                }
            }
            _ => {}
        }
        match self.state {
            Awareness::Unaware => {
                if !seen {
                    self.meter = (self.meter - 0.25 * dt).max(0.0);
                }
                if self.meter > 0.3 {
                    self.go(Awareness::Suspicious);
                    return Some(MindEvent::Suspicious);
                }
                if let Some(n) = heard {
                    self.noise = Some(n);
                    self.go(Awareness::Investigating);
                    return Some(MindEvent::Investigating);
                }
            }
            Awareness::Suspicious => {
                if !seen && self.unseen > 2.5 {
                    self.meter = (self.meter - 0.3 * dt).max(0.0);
                    if self.meter < 0.15 {
                        // Lost interest: check where it was.
                        self.noise = self.last_known;
                        self.go(Awareness::Investigating);
                        return Some(MindEvent::Investigating);
                    }
                }
            }
            Awareness::Alerted | Awareness::Combat => {
                if self.state == Awareness::Alerted && self.t > 0.8 {
                    self.go(Awareness::Combat);
                }
                if self.unseen > 6.0 {
                    self.meter = 0.6;
                    self.go(Awareness::Searching);
                    return Some(MindEvent::LostTarget);
                }
            }
            Awareness::Investigating => {
                if let Some(n) = heard {
                    self.noise = Some(n);
                }
                if arrived || self.t > 15.0 {
                    self.go(Awareness::Searching);
                }
            }
            Awareness::Searching => {
                if !seen {
                    self.meter = (self.meter - 0.02 * dt).max(0.0);
                }
                if self.t > 20.0 {
                    self.meter = 0.0;
                    self.wary = 60.0;
                    self.noise = None;
                    self.go(Awareness::Unaware);
                    return Some(MindEvent::GaveUp);
                }
            }
        }
        None
    }
}

/// Noise radii (m) of things that make noise.
pub mod noise {
    pub const SPRINT_STEP: f32 = 5.0;
    pub const HARD_LANDING: f32 = 8.0;
    pub const PISTOL: f32 = 35.0;
    pub const CANNON: f32 = 150.0;
    pub const WHISTLE: f32 = 14.0;
    pub const BODY_DROP: f32 = 6.0;
    pub const CLASH: f32 = 20.0;
    pub const EXPLOSION: f32 = 60.0;
    pub const SCREAM: f32 = 18.0;
}

/// A noise in the world this tick.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Noise {
    pub at: Vec3,
    pub radius: f32,
}

/// Whether a guard at `ear` hears the noise.
pub fn hears(ear: Vec3, n: &Noise, s: &Senses) -> bool {
    ear.distance(n.at) <= n.radius * s.hearing
}

/// Notoriety in town and fort, 0..3.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Heat {
    /// 0.0 ..= 3.0; the level is its floor.
    pub value: f32,
    /// Seconds since the last crime was seen.
    pub calm: f32,
}

impl Heat {
    pub fn level(&self) -> u8 {
        self.value.floor().clamp(0.0, 3.0) as u8
    }

    /// A crime someone saw (a kill 1.0, a fight 0.5, trespass 0.25...).
    pub fn crime(&mut self, severity: f32) {
        self.value = (self.value + severity).min(3.0);
        self.calm = 0.0;
    }

    /// Tearing down a wanted poster lowers heat by half a level.
    pub fn poster(&mut self) {
        self.value = (self.value - 0.5).max(0.0);
    }

    /// Bribing the crier: one level.
    pub fn bribe(&mut self) {
        self.value = (self.value - 1.0).max(0.0);
    }

    pub fn tick(&mut self, dt: f32) {
        self.calm += dt;
        // Heat 1 cools off by itself after a quiet minute.
        if self.level() <= 1 && self.calm > 60.0 && self.value > 0.0 {
            self.value = (self.value - dt * 0.1).max(0.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seen(dist: f32, e: Exposure) -> f32 {
        let s = Senses::default();
        let sight = look(Vec3::new(0.0, 1.7, 0.0), 0.0, Vec3::new(0.0, 1.2, dist), &s, false);
        sight_rate(&s, &sight, &e)
    }

    #[test]
    fn open_ground_grass_and_walls() {
        let open = Exposure { visible: true, ..Default::default() };
        assert!(seen(15.0, open) > 0.0, "seen in the open at 15 m");
        assert_eq!(seen(15.0, Exposure { visible: false, ..open }), 0.0, "a wall blocks");
        let grass = Exposure { concealed: true, crouched: true, ..open };
        assert_eq!(seen(5.0, grass), 0.0, "hidden crouched in grass at 5 m");
        assert!(seen(1.0, grass) > 0.0, "but seen at 1 m");
        // A rooftop 6 m up and 6 m away is above the vertical limit.
        let s = Senses::default();
        let sight = look(Vec3::new(0.0, 1.7, 0.0), 0.0, Vec3::new(0.0, 7.5, 6.0), &s, false);
        assert!(sight.too_high && sight_rate(&s, &sight, &open) == 0.0);
        let sight = look(Vec3::new(0.0, 1.7, 0.0), 0.0, Vec3::new(0.0, 7.5, 6.0), &s, true);
        assert!(!sight.too_high, "searching guards look up");
        // Behind the guard.
        assert_eq!(seen(-5.0, open), 0.0);
    }

    #[test]
    fn states_progress_in_order() {
        let mut m = Mind::default();
        let mut events = Vec::new();
        let t = Vec3::new(0.0, 0.0, 10.0);
        for _ in 0..60 {
            events.extend(m.update(0.5, t, None, false, 1.0 / 60.0));
        }
        assert_eq!(m.state, Awareness::Suspicious);
        for _ in 0..120 {
            events.extend(m.update(0.5, t, None, false, 1.0 / 60.0));
        }
        assert!(m.aware());
        for _ in 0..(7 * 60) {
            events.extend(m.update(0.0, t, None, false, 1.0 / 60.0));
        }
        assert_eq!(m.state, Awareness::Searching);
        for _ in 0..(21 * 60) {
            events.extend(m.update(0.0, t, None, false, 1.0 / 60.0));
        }
        assert_eq!(m.state, Awareness::Unaware);
        assert_eq!(events, vec![MindEvent::Suspicious, MindEvent::Alerted, MindEvent::LostTarget, MindEvent::GaveUp]);
        // A whistle lures.
        let mut m = Mind::default();
        assert_eq!(m.update(0.0, t, Some(Vec3::ONE), false, 0.1), Some(MindEvent::Investigating));
        assert_eq!(m.noise, Some(Vec3::ONE));
    }

    #[test]
    fn heat_rises_and_falls() {
        let mut h = Heat::default();
        h.crime(1.0);
        h.crime(1.0);
        assert_eq!(h.level(), 2);
        h.poster();
        h.poster();
        assert_eq!(h.level(), 1);
        for _ in 0..(200 * 60) {
            h.tick(1.0 / 60.0);
        }
        assert_eq!(h.level(), 0);
    }
}
