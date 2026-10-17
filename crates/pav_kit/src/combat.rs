//! Fighting on foot: vitals (health, regeneration, death), melee with telegraphed enemy
//! attacks, attack tokens (at most N attackers at once), counters timed to the hit, dodges,
//! finishers, the assassination rules, pistols and tools (darts, smoke). Pure rules over plain
//! numbers: the game feeds positions and facings and plays the animations and sounds the
//! results name.

use std::collections::BTreeMap;

use glam::{Vec2, Vec3};
use serde::{Deserialize, Serialize};

/// What a fighter's counter does to it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CounterResult {
    /// A counter-kill (guards, musketeers, crew).
    #[default]
    Kill,
    /// Disarm and stagger (brutes, officers).
    Stagger,
}

/// An archetype's fighting numbers (`npcs.toml`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CombatProfile {
    pub hp: f32,
    pub damage: f32,
    /// Wind-up (the tell) before a strike lands (s).
    pub windup: f32,
    /// Seconds between this fighter's attacks.
    pub interval: f32,
    pub reach: f32,
    pub counter: CounterResult,
    /// Light attacks only chip a guard that blocks them (brutes need a heavy attack).
    pub blocks_light: bool,
    /// Shoots from range (musketeers) instead of closing in.
    pub ranged: bool,
    pub speed: f32,
}

impl Default for CombatProfile {
    fn default() -> Self {
        Self {
            hp: 60.0,
            damage: 12.0,
            windup: 0.5,
            interval: 2.4,
            reach: 2.0,
            counter: CounterResult::Kill,
            blocks_light: false,
            ranged: false,
            speed: 3.2,
        }
    }
}

/// Health and status effects (players and NPCs).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Vitals {
    pub hp: f32,
    pub max: f32,
    /// Seconds since the last damage (regeneration waits).
    pub since_hit: f32,
    pub dead: bool,
    pub death_t: f32,
    /// Seconds left of: a stagger, sleep (Nightcap), drunken rage (Grog), blindness (smoke).
    pub stagger: f32,
    pub asleep: f32,
    pub drunk: f32,
    pub blind: f32,
    /// Invulnerable (a dodge's i-frames).
    pub iframes: f32,
}

impl Vitals {
    pub fn new(hp: f32) -> Self {
        Self { hp, max: hp, since_hit: 99.0, ..Default::default() }
    }

    pub fn frac(&self) -> f32 {
        (self.hp / self.max.max(1.0)).clamp(0.0, 1.0)
    }

    /// Out of the fight for now (asleep, staggered, dead).
    pub fn helpless(&self) -> bool {
        self.dead || self.asleep > 0.0 || self.stagger > 0.0
    }

    /// Applies damage; true when it killed.
    pub fn hurt(&mut self, amount: f32) -> bool {
        if self.dead || self.iframes > 0.0 {
            return false;
        }
        self.hp -= amount;
        self.since_hit = 0.0;
        self.asleep = 0.0;
        if self.hp <= 0.0 {
            self.hp = 0.0;
            self.dead = true;
            return true;
        }
        false
    }

    /// Timers and regeneration (`regen` hp/s after `delay` s out of harm; 0 = none).
    pub fn tick(&mut self, dt: f32, regen: f32, delay: f32) {
        if self.dead {
            self.death_t += dt;
            return;
        }
        self.since_hit += dt;
        for t in [&mut self.stagger, &mut self.asleep, &mut self.drunk, &mut self.blind, &mut self.iframes] {
            *t = (*t - dt).max(0.0);
        }
        if regen > 0.0 && self.since_hit > delay {
            self.hp = (self.hp + regen * dt).min(self.max);
        }
    }
}

/// One enemy's attack in progress.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Swing {
    /// Seconds since the wind-up began; the strike lands at `windup`.
    pub t: f32,
    pub windup: f32,
    pub damage: f32,
    /// The hit resolves this long after the strike (the counter's late window).
    pub late: f32,
}

/// Counter timing: a press counts from `early` seconds before the strike to `late` after.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct CounterWindow {
    pub early: f32,
    pub late: f32,
}

impl Default for CounterWindow {
    fn default() -> Self {
        Self { early: 0.35, late: 0.10 }
    }
}

/// How an enemy strike ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StrikeOutcome {
    Countered,
    Dodged,
    Hit,
}

/// Resolves a strike landing now: the counter press time (seconds ago, if any), dodge
/// i-frames. Call when `swing.t >= swing.windup + swing.late`.
pub fn resolve_strike(counter_ago: Option<f32>, window: CounterWindow, swing: &Swing, iframes: bool) -> StrikeOutcome {
    if let Some(ago) = counter_ago {
        // The press happened at (now - ago); the strike landed at (now - late).
        let rel = swing.late - ago; // press time relative to the strike moment
        if rel >= -window.early && rel <= window.late {
            return StrikeOutcome::Countered;
        }
    }
    if iframes { StrikeOutcome::Dodged } else { StrikeOutcome::Hit }
}

/// Hands out attack tokens: at most `max` fighters attack at once; the rest circle.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Tokens {
    pub max: usize,
    pub holders: Vec<u32>,
}

impl Tokens {
    pub fn new(max: usize) -> Self {
        Self { max, holders: Vec::new() }
    }

    /// Tries to take a token (true if `id` holds one now).
    pub fn take(&mut self, id: u32) -> bool {
        if self.holders.contains(&id) {
            return true;
        }
        if self.holders.len() < self.max {
            self.holders.push(id);
            return true;
        }
        false
    }

    pub fn give_back(&mut self, id: u32) {
        self.holders.retain(|&h| h != id);
    }
}

/// The player's light-attack chain: damage per swing and the move each plays.
pub const LIGHT_CHAIN: [(f32, &str); 3] = [(18.0, "slash"), (18.0, "backslash"), (30.0, "thrust")];
pub const HEAVY: (f32, &str) = (40.0, "overhead");

/// Kinds of Tap Blade kills.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Assassination {
    Standing,
    Air,
    Hiding,
    Ledge,
    Double,
}

impl Assassination {
    pub fn name(self) -> &'static str {
        match self {
            Assassination::Standing => "standing",
            Assassination::Air => "air",
            Assassination::Hiding => "hiding",
            Assassination::Ledge => "ledge",
            Assassination::Double => "double",
        }
    }

    /// How far the kill is heard (m).
    pub fn noise(self) -> f32 {
        if self == Assassination::Air { 6.0 } else { 2.0 }
    }
}

/// What the player is doing, for the assassination rules.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Assassin {
    pub feet: Vec3,
    pub falling: bool,
    pub on_ledge: bool,
    pub hanging: bool,
    pub hidden: bool,
    pub double_unlocked: bool,
}

/// A possible target.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mark {
    pub id: u32,
    pub feet: Vec3,
    /// Facing (radians, 0 = +Z, as characters use).
    pub facing: f32,
    /// Alerted or fighting (no standing assassination from the front).
    pub alerted: bool,
    /// Not aware of the player at all.
    pub unaware: bool,
}

fn behind(target: &Mark, from: Vec3) -> bool {
    let f = Vec2::new(target.facing.sin(), target.facing.cos());
    let to = Vec2::new(from.x - target.feet.x, from.z - target.feet.z).normalize_or_zero();
    f.dot(to) < -0.2
}

/// Which assassination (if any) is possible on `t` (the first valid kind in priority order).
pub fn assassination(a: &Assassin, t: &Mark) -> Option<Assassination> {
    let d = t.feet - a.feet;
    let flat = Vec2::new(d.x, d.z).length();
    let above = a.feet.y - t.feet.y;
    if a.hanging && (0.5..=2.6).contains(&(t.feet.y - a.feet.y)) && flat <= 2.2 {
        return Some(Assassination::Ledge);
    }
    if (a.falling || a.on_ledge) && (1.5..=6.0).contains(&above) && flat <= 3.5 {
        return Some(Assassination::Air);
    }
    if a.hidden && flat <= 2.0 && above.abs() < 1.0 && !t.alerted {
        return Some(Assassination::Hiding);
    }
    if flat <= 1.6 && above.abs() < 1.0 && !t.alerted && (t.unaware || behind(t, a.feet)) {
        return Some(Assassination::Standing);
    }
    None
}

/// A double assassination when two valid targets stand within 1.2 m of each other.
pub fn double_assassination(a: &Assassin, t1: &Mark, t2: &Mark) -> bool {
    a.double_unlocked && t1.feet.distance(t2.feet) <= 1.2 && assassination(a, t1).is_some() && assassination(a, t2).is_some()
}

/// Pistol lock: the best target in a cone (degrees) within range, scored by angle and
/// distance; `visible` checks line of sight.
pub fn pistol_lock(
    eye: Vec3,
    aim_dir: Vec3,
    cone_deg: f32,
    range: f32,
    marks: &[(u32, Vec3)],
    visible: &dyn Fn(Vec3) -> bool,
) -> Option<u32> {
    let cone = cone_deg.to_radians().cos();
    marks
        .iter()
        .filter_map(|&(id, p)| {
            let d = p - eye;
            let dist = d.length();
            if dist > range || dist < 0.5 {
                return None;
            }
            let c = d.normalize().dot(aim_dir.normalize_or_zero());
            (c >= cone && visible(p)).then_some((id, c - dist / range * 0.3))
        })
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(id, _)| id)
}

/// The player's tools and their stock.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tool {
    Nightcap,
    Grog,
    Smoke,
    BungLine,
}

impl Tool {
    pub const ALL: [Tool; 4] = [Tool::Nightcap, Tool::Grog, Tool::Smoke, Tool::BungLine];

    pub fn name(self) -> &'static str {
        match self {
            Tool::Nightcap => "Nightcap Dart",
            Tool::Grog => "Grog Dart",
            Tool::Smoke => "Smoke Keg",
            Tool::BungLine => "Bung Line",
        }
    }
}

/// A smoke cloud on the ground.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Smoke {
    pub at: Vec3,
    pub radius: f32,
    pub left: f32,
}

/// The tool belt (counts per tool) and smoke clouds in the world.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Tools {
    pub stock: BTreeMap<Tool, u32>,
    pub selected: usize,
    pub smoke: Vec<Smoke>,
}

impl Tools {
    pub fn full() -> Self {
        let mut stock = BTreeMap::new();
        stock.insert(Tool::Nightcap, 4);
        stock.insert(Tool::Grog, 3);
        stock.insert(Tool::Smoke, 2);
        stock.insert(Tool::BungLine, 99);
        Self { stock, selected: 0, smoke: Vec::new() }
    }

    pub fn current(&self) -> Tool {
        Tool::ALL[self.selected % Tool::ALL.len()]
    }

    pub fn count(&self, t: Tool) -> u32 {
        self.stock.get(&t).copied().unwrap_or(0)
    }

    /// Uses one of `t`; false when there are none.
    pub fn spend(&mut self, t: Tool) -> bool {
        match self.stock.get_mut(&t) {
            Some(n) if *n > 0 => {
                *n -= 1;
                true
            }
            _ => false,
        }
    }

    pub fn in_smoke(&self, p: Vec3) -> bool {
        self.smoke.iter().any(|s| Vec2::new(p.x - s.at.x, p.z - s.at.z).length() <= s.radius)
    }

    pub fn tick(&mut self, dt: f32) {
        for s in &mut self.smoke {
            s.left -= dt;
        }
        self.smoke.retain(|s| s.left > 0.0);
    }
}

/// Fall damage from a landing speed (m/s): safe under 11 (14 with a roll), lethal at 22.
pub fn fall_damage(speed: f32, roll: bool, max_hp: f32) -> f32 {
    let safe = if roll { 14.0 } else { 11.0 };
    if speed <= safe {
        return 0.0;
    }
    ((speed - safe) / (22.0 - safe)).clamp(0.0, 1.0) * max_hp
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counter_window_in_ticks() {
        let w = CounterWindow::default();
        let s = Swing { t: 0.6, windup: 0.5, damage: 10.0, late: 0.1 };
        // Pressed 0.30 s before the strike → resolved 0.1 s after the strike: ago = 0.4.
        assert_eq!(resolve_strike(Some(0.40), w, &s, false), StrikeOutcome::Countered);
        // Pressed just after the strike (0.05 s): ago = 0.05.
        assert_eq!(resolve_strike(Some(0.05), w, &s, false), StrikeOutcome::Countered);
        // Too early: 0.5 s before the strike.
        assert_eq!(resolve_strike(Some(0.6), w, &s, false), StrikeOutcome::Hit);
        assert_eq!(resolve_strike(None, w, &s, true), StrikeOutcome::Dodged);
    }

    #[test]
    fn tokens_limit_attackers() {
        let mut t = Tokens::new(2);
        assert!(t.take(1) && t.take(2) && !t.take(3));
        t.give_back(1);
        assert!(t.take(3));
    }

    #[test]
    fn assassination_matrix() {
        let target = Mark { id: 1, feet: Vec3::ZERO, facing: 0.0, alerted: false, unaware: false };
        // Behind (target faces +Z, player at -Z).
        let a = Assassin { feet: Vec3::new(0.0, 0.0, -1.2), ..Default::default() };
        assert_eq!(assassination(&a, &target), Some(Assassination::Standing));
        // In front and aware: no.
        let a = Assassin { feet: Vec3::new(0.0, 0.0, 1.2), ..Default::default() };
        assert_eq!(assassination(&a, &target), None);
        // From above while falling.
        let a = Assassin { feet: Vec3::new(1.0, 3.0, 0.0), falling: true, ..Default::default() };
        assert_eq!(assassination(&a, &target), Some(Assassination::Air));
        // Too high.
        let a = Assassin { feet: Vec3::new(1.0, 7.0, 0.0), falling: true, ..Default::default() };
        assert_eq!(assassination(&a, &target), None);
        // From the grass.
        let a = Assassin { feet: Vec3::new(1.5, 0.0, 0.5), hidden: true, ..Default::default() };
        assert_eq!(assassination(&a, &target), Some(Assassination::Hiding));
        // Alerted: only air/ledge work.
        let alert = Mark { alerted: true, ..target };
        let a = Assassin { feet: Vec3::new(0.0, 0.0, -1.2), ..Default::default() };
        assert_eq!(assassination(&a, &alert), None);
        // Ledge.
        let a = Assassin { feet: Vec3::new(1.0, -1.5, 0.0), hanging: true, ..Default::default() };
        assert_eq!(assassination(&a, &target), Some(Assassination::Ledge));
        let t2 = Mark { id: 2, feet: Vec3::new(1.0, 0.0, 0.0), ..target };
        let a = Assassin { feet: Vec3::new(0.5, 0.0, -1.0), double_unlocked: true, ..Default::default() };
        assert!(double_assassination(&a, &target, &t2));
    }

    #[test]
    fn falls_hurt_past_the_safe_speed() {
        assert_eq!(fall_damage(10.0, false, 100.0), 0.0);
        assert_eq!(fall_damage(13.0, true, 100.0), 0.0);
        assert!(fall_damage(16.0, false, 100.0) > 0.0);
        assert_eq!(fall_damage(25.0, false, 100.0), 100.0);
    }

    #[test]
    fn pistol_picks_the_target_in_the_cone() {
        let marks = [(1, Vec3::new(0.0, 0.0, -10.0)), (2, Vec3::new(10.0, 0.0, 0.0)), (3, Vec3::new(0.0, 0.0, -40.0))];
        let got = pistol_lock(Vec3::ZERO, Vec3::NEG_Z, 25.0, 30.0, &marks, &|_| true);
        assert_eq!(got, Some(1));
        let got = pistol_lock(Vec3::ZERO, Vec3::NEG_Z, 25.0, 30.0, &marks, &|p| p.z > -5.0);
        assert_eq!(got, None);
    }
}
