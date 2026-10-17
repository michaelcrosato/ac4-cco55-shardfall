//! Everything at sea: the Black Tap at the helm, AI ships, Fort Abstinence's batteries, shot,
//! flotsam, powder kegs and waterspouts. Built on `pav_kit::naval` and `ballistics`; this file
//! is the glue (who shoots whom, what a hit does, what the crew shouts).

use std::collections::BTreeSet;

use glam::{Vec2, Vec3};
use pav_core::frame::SimEvent;
use pav_core::ocean::Ocean;
use pav_kit::ballistics::{self, Ballistics, HitWhat, Impact, Owner, ShotKind, Target};
use pav_kit::naval::{
    Battery, BatteryKind, Captain, CaptainKind, Classes, Faction, Helm, HullClass, MarkedShell, NavalEvent, Side, Vessel, Weapon,
    captain_orders, heading_of, turn_to, weapon_for_bearing,
};
use serde::{Deserialize, Serialize};

use crate::world::World;

/// Things floating on the sea.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Floating {
    pub id: u32,
    pub pos: Vec2,
    pub kind: FloatKind,
    pub alive: bool,
    /// Seconds since it was dropped (kegs arm after a moment).
    pub age: f32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FloatKind {
    /// Crates: timber and iron (Round 1, collectibles).
    Flotsam,
    /// A powder keg dropped astern (Keg Drop).
    Keg { owner: u32 },
}

/// A waterspout crossing the storm strait.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Spout {
    pub pos: Vec2,
    pub vel: Vec2,
    pub radius: f32,
    pub life: f32,
}

/// Where a ship went down and when (the sea clock): its wreckage floats there a while (drawn
/// by `vfx.rs`; nothing collides with it).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Wreck {
    pub id: u32,
    pub pos: Vec2,
    pub at: f32,
}

/// What the player's helm does this tick (from the buttons, see `game.rs`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HelmInput {
    pub helm: Helm,
    /// Camera yaw (degrees): looking to a side picks the weapon.
    pub look_yaw: f32,
    pub aiming: bool,
    pub fire: bool,
    pub swivel: bool,
    pub brace: bool,
    pub repair: bool,
    pub spyglass: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Sea {
    pub wind: Vec2,
    pub ships: Vec<Vessel>,
    /// The player's ship's id.
    pub mine: u32,
    pub batteries: Vec<Battery>,
    pub shells: Vec<MarkedShell>,
    pub shots: Ballistics,
    pub floating: Vec<Floating>,
    pub spouts: Vec<Spout>,
    pub next_id: u32,
    /// Ships tagged with the spyglass.
    pub tagged: BTreeSet<u32>,
    /// The weapon the camera chooses now, and whether the aim arcs show.
    pub weapon: Option<Weapon>,
    pub aiming: bool,
    /// Ship the swivel would hit (a weak point in front).
    pub swivel_target: Option<u32>,
    /// Timber and iron in the hold, reales from plunder.
    pub timber: u32,
    pub iron: u32,
    pub repair_cd: f32,
    /// Upgrades bought: heavy shot multiplies ball damage.
    pub heavy_shot: f32,
    /// Ships sunk by the player (results).
    pub sunk_by_player: u32,
    /// Damage taken by the player's ship since this was reset (optional objectives).
    pub hull_hits: f32,
    /// Battery destroyed order (optional: mortar first).
    pub destroyed_order: Vec<String>,
    pub clock: f32,
    /// Seconds added to the batteries' first volley (from the world seed).
    #[serde(default)]
    pub jitter: f32,
    /// Ships gone down in the last minute or so (their wreckage is drawn).
    #[serde(default)]
    pub wrecks: Vec<Wreck>,
}

/// Things that happened at sea this tick, for the rules and the bot.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SeaEvent {
    Fired { ship: u32, weapon: String },
    Flotsam { id: u32 },
    Sunk { ship: u32, by_player: bool },
    Disabled { ship: u32, by: String },
    BatteryDown { name: String },
    PlayerHit { damage: f32 },
    Tagged { ship: u32 },
    Telegraph { ship: u32 },
    Spout,
}

/// Where a weapon's guns are on a ship: the gun ports down the firing side (as the brace glow
/// draws them), the bow chasers, else amidships.
pub fn muzzles(v: &Vessel, c: &HullClass, w: Weapon) -> Vec<Vec3> {
    match w {
        Weapon::Broadside(side) => {
            let s = if side == Side::Starboard { 1.0 } else { -1.0 };
            let n = c.guns_per_side.max(1);
            (0..n)
                .map(|i| {
                    let along = (i as f32 / (n.max(2) - 1) as f32 - 0.5) * c.length * 0.56;
                    v.world(Vec3::new(s * (c.beam * 0.5 + 0.5), c.freeboard * 0.6, along))
                })
                .collect()
        }
        Weapon::Chain => vec![v.world(Vec3::new(0.0, c.freeboard, c.length * 0.5))],
        _ => vec![v.world(Vec3::Y * c.freeboard)],
    }
}

/// A volley's cues: the report and flash once, from the middle gun, and a puff of smoke from
/// every muzzle (`muzzle_smoke` is a look only: fx.toml).
fn volley(v: &Vessel, c: &HullClass, w: Weapon, cue: &str, gain: f32, events: &mut Vec<SimEvent>) {
    let guns = muzzles(v, c, w);
    let mid = guns[guns.len() / 2];
    events.push(SimEvent::Cue { cue: cue.into(), pos: mid, gain, pitch: 1.0 });
    for g in guns {
        events.push(SimEvent::cue("muzzle_smoke", g));
    }
}

impl Sea {
    pub fn ship(&self, id: u32) -> Option<&Vessel> {
        self.ships.iter().find(|s| s.id == id)
    }

    pub fn ship_mut(&mut self, id: u32) -> Option<&mut Vessel> {
        self.ships.iter_mut().find(|s| s.id == id)
    }

    pub fn my_ship(&self) -> Option<&Vessel> {
        self.ship(self.mine)
    }

    pub fn by_tag(&self, tag: &str) -> impl Iterator<Item = &Vessel> {
        self.ships.iter().filter(move |s| s.tag == tag)
    }

    /// Adds a ship; returns its id.
    pub fn add_ship(
        &mut self,
        classes: &Classes,
        class: &str,
        name: &str,
        tag: &str,
        pos: Vec2,
        heading: f32,
        faction: Faction,
    ) -> u32 {
        self.next_id += 1;
        let id = self.next_id;
        let c = classes.get(class).cloned().unwrap_or_default();
        let mut v = Vessel::new(id, &c, class, name, pos, heading);
        v.tag = tag.to_string();
        v.faction = faction;
        self.ships.push(v);
        id
    }

    pub fn add_captain(&mut self, id: u32, kind: CaptainKind, route: Vec<Vec2>, refuge: Option<Vec2>, hostile: bool) {
        if let Some(v) = self.ship_mut(id) {
            let mut c = Captain::new(kind);
            c.route = route;
            c.refuge = refuge;
            c.hostile = hostile;
            v.captain = Some(c);
            v.sail = if kind == CaptainKind::Patrol { 1 } else { 2 };
        }
    }

    pub fn add_battery(&mut self, name: &str, kind: BatteryKind, pos: Vec3, guns: u32, hp: f32, range: f32) -> u32 {
        self.next_id += 1;
        let id = self.next_id;
        self.batteries.push(Battery {
            id,
            name: name.to_string(),
            kind,
            pos,
            guns,
            hp,
            hp_max: hp,
            range,
            reload: if kind == BatteryKind::Mortar { 7.0 } else { 8.0 },
            cooldown: 3.0 + id as f32 % 3.0 + self.jitter,
            telegraph: 0.0,
            destroyed: false,
            half: if kind == BatteryKind::Mortar { Vec3::new(5.0, 9.0, 5.0) } else { Vec3::new(9.0, 4.0, 6.0) },
            damage: if kind == BatteryKind::Mortar { 22.0 } else { 10.0 },
            spread: 3.0,
        });
        id
    }

    pub fn add_float(&mut self, pos: Vec2, kind: FloatKind) -> u32 {
        self.next_id += 1;
        let id = self.next_id;
        self.floating.push(Floating { id, pos, kind, alive: true, age: 0.0 });
        id
    }

    /// Hit boxes of every ship afloat.
    fn targets(&self, classes: &Classes) -> Vec<Target> {
        let mut out: Vec<Target> = self
            .ships
            .iter()
            .filter(|s| !s.sunk)
            .map(|s| {
                let c = classes.get(&s.class).cloned().unwrap_or_default();
                Target {
                    id: s.id,
                    center: s.world(Vec3::new(0.0, c.freeboard * 0.3, 0.0)),
                    yaw: s.heading,
                    half: Vec3::new(c.beam * 0.5, c.freeboard * 0.5 + c.draft * 0.3, c.length * 0.5),
                    sail: Some((s.heave + c.freeboard + 3.0, s.heave + c.freeboard + c.mast_height)),
                    owner: Owner::Vessel(s.id),
                }
            })
            .collect();
        for b in self.batteries.iter().filter(|b| !b.destroyed) {
            out.push(Target { id: b.id, center: b.pos, yaw: 0.0, half: b.half, sail: None, owner: Owner::Battery(b.id) });
        }
        out
    }

    /// One tick. `helm` drives the player's ship when the player is at its helm.
    #[allow(clippy::too_many_arguments)]
    pub fn step(
        &mut self,
        classes: &Classes,
        world: &World,
        ocean: Option<&Ocean>,
        helm: Option<HelmInput>,
        difficulty: f32,
        dt: f32,
        events: &mut Vec<SimEvent>,
    ) -> Vec<SeaEvent> {
        let mut out = Vec::new();
        self.clock += dt;
        self.repair_cd = (self.repair_cd - dt).max(0.0);
        let seabed = |p: Vec2| world.ground(p);
        let wind = self.wind;
        let me = self.mine;
        let my_pose = self.my_ship().map(|s| (s.pos, s.heading, s.speed));

        // ---- the player's helm
        if let (Some(h), Some(i)) = (helm, self.ships.iter().position(|s| s.id == me)) {
            let c = classes.get(&self.ships[i].class).cloned().unwrap_or_default();
            let rel = turn_to(self.ships[i].heading, h.look_yaw);
            let weapon = weapon_for_bearing(rel);
            self.weapon = Some(weapon);
            self.aiming = h.aiming;
            if h.brace && self.ships[i].brace_cd <= 0.0 {
                self.ships[i].brace = 1.2;
                self.ships[i].brace_cd = 4.0;
                events.push(SimEvent::cue("brace", self.ships[i].world(Vec3::Y * 3.0)));
            }
            if h.repair && self.repair_cd <= 0.0 && self.timber > 0 && self.ships[i].hull < self.ships[i].hull_max {
                self.timber -= 1;
                self.repair_cd = 20.0;
                let v = &mut self.ships[i];
                v.hull = (v.hull + 200.0).min(v.hull_max);
                v.fire = 0.0;
                events.push(SimEvent::cue("repair", v.world(Vec3::Y * 3.0)));
            }
            // The swivel's mark: the nearest enemy ahead within 90 m.
            let pos = self.ships[i].pos;
            let fwd = self.ships[i].forward();
            self.swivel_target = self
                .ships
                .iter()
                .filter(|s| s.id != me && s.alive() && s.faction != Faction::Brotherhood)
                .filter(|s| {
                    let d = s.pos - pos;
                    d.length() < 90.0 && fwd.dot(d.normalize_or_zero()) > 0.75
                })
                .min_by(|a, b| a.pos.distance(pos).total_cmp(&b.pos.distance(pos)))
                .map(|s| s.id);
            if h.fire && !h.spyglass {
                let (el, aim_yaw) = self.aim_assist(classes, i, weapon);
                let heavy = 1.0 + self.heavy_shot;
                let fired = self.ships[i].fire(&c, weapon, el, aim_yaw, &mut self.shots, heavy);
                if fired {
                    let v = &self.ships[i];
                    let at = v.world(Vec3::Y * c.freeboard);
                    match weapon {
                        Weapon::Kegs => {
                            let astern = v.pos - v.forward() * (c.length * 0.6);
                            let id = self.add_float(astern, FloatKind::Keg { owner: me });
                            let _ = id;
                            events.push(SimEvent::cue("keg_drop", at));
                        }
                        Weapon::Chain => volley(v, &c, weapon, "chain_fire", 1.0, events),
                        _ => volley(v, &c, weapon, "cannon_fire", 1.2, events),
                    }
                    out.push(SeaEvent::Fired { ship: me, weapon: weapon.name().into() });
                }
            }
            if h.swivel && self.swivel_target.is_some() {
                let target = self.swivel_target.and_then(|t| self.ship(t)).map(|t| t.world(Vec3::Y * 2.0));
                let v = &self.ships[i];
                if let Some(tp) = target {
                    let from = v.world(Vec3::new(0.0, c.freeboard + 1.2, c.length * 0.3));
                    let d = tp - from;
                    let yaw = heading_of(Vec2::new(d.x, d.z));
                    let el = ballistics::elevation_for(Vec2::new(d.x, d.z).length(), d.y, 110.0).unwrap_or(0.0);
                    if self.ships[i].fire(&c, Weapon::Swivel, el, Some(yaw), &mut self.shots, 1.0) {
                        events.push(SimEvent::cue("swivel_fire", from));
                        out.push(SeaEvent::Fired { ship: me, weapon: "swivel".into() });
                    }
                }
            }
            let ev = self.ships[i].sail_step(&c, h.helm, wind, ocean, &seabed, dt);
            self.naval_events(ev, &mut out, events);
        }

        // ---- AI ships
        for i in 0..self.ships.len() {
            if self.ships[i].id == me && helm.is_some() {
                continue;
            }
            let c = classes.get(&self.ships[i].class).cloned().unwrap_or_default();
            let target = my_pose.filter(|_| self.ships[i].faction == Faction::Temperance);
            let orders = if self.ships[i].id == me {
                // The player's ship without its captain at the helm: drift to anchor.
                pav_kit::naval::Orders { helm: Helm { anchor: true, ..Default::default() }, ..Default::default() }
            } else {
                captain_orders(&mut self.ships[i], &c, target, wind, &seabed, dt)
            };
            if let Some(side) = orders.telegraph {
                let _ = side;
                out.push(SeaEvent::Telegraph { ship: self.ships[i].id });
                events.push(SimEvent::cue("ship_bell", self.ships[i].world(Vec3::Y * 3.0)));
            }
            if let Some((w, el)) = orders.fire {
                let dmg = 0.6 + 0.4 * difficulty;
                if self.ships[i].fire(&c, w, el, None, &mut self.shots, dmg) {
                    volley(&self.ships[i], &c, w, if w == Weapon::Chain { "chain_fire" } else { "cannon_fire" }, 1.0, events);
                }
            }
            let ev = self.ships[i].sail_step(&c, orders.helm, wind, ocean, &seabed, dt);
            self.naval_events(ev, &mut out, events);
        }

        // ---- ships bump and ram
        self.collisions(classes, events);

        // ---- the fort
        let target = self.my_ship().filter(|s| s.alive()).map(|s| {
            let v = s.forward() * s.speed;
            (s.world(Vec3::Y), Vec3::new(v.x, 0.0, v.y))
        });
        for b in &mut self.batteries {
            let primed = b.telegraph > 0.0;
            let (mark, started) = b.step(target, &mut self.shots, dt);
            // On top of the emplacement (its stand-in box reaches half its height above `pos`).
            let top = b.pos + Vec3::Y * (b.half.y * 0.5 + 0.6);
            if started && b.kind == BatteryKind::Guns {
                events.push(SimEvent::cue("ship_bell", b.pos));
                out.push(SeaEvent::Telegraph { ship: 0 });
            }
            if primed && b.telegraph <= 0.0 && b.kind == BatteryKind::Guns {
                // The volley: the report once, smoke from each gun along the parapet.
                events.push(SimEvent::cue("cannon_fire", top));
                for i in 0..b.guns {
                    let along = (i as f32 / b.guns.max(2).saturating_sub(1) as f32 - 0.5) * b.half.x * 1.4;
                    events.push(SimEvent::cue("muzzle_smoke", top + Vec3::new(along, 0.0, 0.0)));
                }
            }
            if let Some(m) = mark {
                out.push(SeaEvent::Telegraph { ship: 0 });
                events.push(SimEvent::cue("mortar_launch", top));
                events.push(SimEvent::cue("mortar_whistle", m.at));
                self.shells.push(m);
            }
        }
        for s in &mut self.shells {
            s.left -= dt;
        }
        self.shells.retain(|s| s.left > -0.3);

        // ---- shot
        let targets = self.targets(classes);
        let sea_h = |p: Vec2| ocean.map(|o| o.height(p)).unwrap_or(0.0);
        let impacts = self.shots.step(dt, &targets, &|p| world.ground(p), &sea_h);
        for imp in impacts {
            self.impact(classes, &imp, events, &mut out);
        }

        // ---- floating things
        let my = self.my_ship().map(|s| (s.pos, s.forward(), classes.get(&s.class).map(|c| c.length).unwrap_or(30.0)));
        let mut kegs_blown = Vec::new();
        for f in &mut self.floating {
            if !f.alive {
                continue;
            }
            f.age += dt;
            // Drift a little with the wind.
            f.pos += wind * 0.02 * dt;
            match f.kind {
                FloatKind::Flotsam => {
                    if let Some((p, fwd, len)) = my {
                        // Along the hull: the closest point on the keel line.
                        let d = f.pos - p;
                        let along = d.dot(fwd).clamp(-len * 0.5, len * 0.5);
                        if (d - fwd * along).length() < 9.0 {
                            f.alive = false;
                            out.push(SeaEvent::Flotsam { id: f.id });
                            events.push(SimEvent::cue("pickup_flotsam", Vec3::new(f.pos.x, 0.5, f.pos.y)));
                        }
                    }
                }
                FloatKind::Keg { owner } => {
                    if f.age > 1.5 {
                        let near = self.ships.iter().any(|s| s.id != owner && s.alive() && s.pos.distance(f.pos) < 10.0);
                        if near || f.age > 60.0 {
                            f.alive = false;
                            kegs_blown.push((f.pos, owner));
                        }
                    }
                }
            }
        }
        for (p, owner) in kegs_blown {
            self.blast(classes, Vec3::new(p.x, 0.5, p.y), 14.0, 160.0, Owner::Vessel(owner), events, &mut out);
        }
        self.floating.retain(|f| f.alive);

        // ---- waterspouts
        let near_me = self.my_ship().map(|v| v.pos);
        for sp in &mut self.spouts {
            sp.pos += sp.vel * dt;
            sp.life -= dt;
            // Passing close by (the showcase counts it; the crew shouts).
            if let Some(p) = near_me {
                let d = p.distance(sp.pos);
                if d < sp.radius + 40.0 && d + sp.vel.length() * dt >= sp.radius + 40.0 {
                    out.push(SeaEvent::Spout);
                }
            }
        }
        self.spouts.retain(|s| s.life > 0.0);
        let clock = self.clock;
        self.wrecks.retain(|w| clock - w.at < 80.0);
        let spouts = self.spouts.clone();
        for v in &mut self.ships {
            for sp in &spouts {
                let d = v.pos.distance(sp.pos);
                if d < sp.radius && v.alive() {
                    // Thrown about and battered.
                    let push = (v.pos - sp.pos).normalize_or(Vec2::X) * 6.0 * dt;
                    v.pos += push;
                    v.heading = (v.heading + 40.0 * dt).rem_euclid(360.0);
                    let dmg = v.damage(40.0 * dt, "spout");
                    if v.id == me {
                        self.hull_hits += dmg;
                        out.push(SeaEvent::PlayerHit { damage: dmg });
                    }
                }
            }
        }

        // ---- the soft edge of the world turns ships back
        for v in &mut self.ships {
            let r = v.pos.length();
            if r > world.bounds.soft {
                let home = heading_of(-v.pos);
                v.heading = (v.heading + turn_to(v.heading, home).clamp(-30.0 * dt, 30.0 * dt)).rem_euclid(360.0);
                if r > world.bounds.hard {
                    v.pos = v.pos.normalize() * world.bounds.hard;
                }
            }
        }
        out
    }

    fn naval_events(&mut self, ev: Vec<NavalEvent>, out: &mut Vec<SeaEvent>, events: &mut Vec<SimEvent>) {
        for e in ev {
            match e {
                NavalEvent::Sunk(id) => {
                    // Gone under (the sinking was reported when it began).
                    if let Some(s) = self.ship(id) {
                        events.push(SimEvent::cue("sink", s.world(Vec3::ZERO)));
                        let wreck = Wreck { id, pos: s.pos, at: self.clock };
                        self.wrecks.push(wreck);
                    }
                }
                NavalEvent::Grounded { id, damage } => {
                    if let Some(s) = self.ship(id) {
                        events.push(SimEvent::cue("hull_hit", s.world(Vec3::Y)));
                    }
                    if id == self.mine {
                        self.hull_hits += damage;
                        out.push(SeaEvent::PlayerHit { damage });
                    }
                }
                NavalEvent::Boost(id) => {
                    if let Some(s) = self.ship(id) {
                        events.push(SimEvent::cue("sail_flap", s.world(Vec3::Y * 8.0)));
                    }
                }
                _ => {}
            }
        }
    }

    /// Elevation and yaw for a weapon: aim assist toward the nearest enemy in its arc.
    fn aim_assist(&self, classes: &Classes, i: usize, w: Weapon) -> (f32, Option<f32>) {
        let v = &self.ships[i];
        let c = classes.get(&v.class).cloned().unwrap_or_default();
        let side = match w {
            Weapon::Broadside(Side::Port) => Some(-v.right()),
            Weapon::Broadside(Side::Starboard) => Some(v.right()),
            Weapon::Chain => Some(v.forward()),
            _ => None,
        };
        let Some(dir) = side else { return (0.03, None) };
        let mut best: Option<(f32, Vec3)> = None;
        // Ships are led: where they will be when the ball arrives (the ball keeps our way).
        let mine = v.forward() * v.speed;
        for s in self.ships.iter().filter(|s| s.id != v.id && s.alive() && s.faction != v.faction) {
            let d = s.pos - v.pos;
            let dist = d.length();
            if dist < 260.0 && dir.dot(d / dist.max(1e-3)) > 0.7 && best.is_none_or(|b| dist < b.0) {
                let flight = dist / c.muzzle.max(1.0);
                let lead = (s.forward() * s.speed - mine) * flight;
                best = Some((dist, s.world(Vec3::Y * 1.0) + Vec3::new(lead.x, 0.0, lead.y)));
            }
        }
        for b in self.batteries.iter().filter(|b| !b.destroyed) {
            let d = Vec2::new(b.pos.x, b.pos.z) - v.pos;
            let dist = d.length();
            if dist < 300.0 && dir.dot(d / dist.max(1e-3)) > 0.6 && best.is_none_or(|x| dist < x.0) {
                best = Some((dist, b.pos));
            }
        }
        match best {
            Some((_, at)) => {
                let from_y = c.freeboard * 0.6 + v.heave;
                let flat = Vec2::new(at.x, at.z) - v.pos;
                let el = ballistics::elevation_for(flat.length(), at.y - from_y, c.muzzle).unwrap_or(0.12);
                (el, Some(heading_of(flat)))
            }
            None => (0.04, None),
        }
    }

    fn collisions(&mut self, classes: &Classes, events: &mut Vec<SimEvent>) {
        let n = self.ships.len();
        for a in 0..n {
            for b in (a + 1)..n {
                let (sa, sb) = (&self.ships[a], &self.ships[b]);
                if sa.sunk || sb.sunk {
                    continue;
                }
                let ca = classes.get(&sa.class).cloned().unwrap_or_default();
                let cb = classes.get(&sb.class).cloned().unwrap_or_default();
                // Three circles along each hull.
                let circles = |s: &Vessel, c: &HullClass| {
                    let f = s.forward();
                    [-0.33f32, 0.0, 0.33].map(|k| s.pos + f * c.length * k)
                };
                let (pa, pb) = (circles(sa, &ca), circles(sb, &cb));
                let (ra, rb) = (ca.beam * 0.6, cb.beam * 0.6);
                let mut push = Vec2::ZERO;
                for p in pa {
                    for q in pb {
                        let d = p - q;
                        let pen = ra + rb - d.length();
                        if pen > 0.0 {
                            push += d.normalize_or(Vec2::X) * pen;
                        }
                    }
                }
                if push != Vec2::ZERO {
                    let rel = (sa.forward() * sa.speed - sb.forward() * sb.speed).length();
                    let ram = if rel > 4.0 { (rel - 4.0) * 30.0 } else { 0.0 };
                    let (ida, idb) = (sa.id, sb.id);
                    let boosted_a = sa.boost > 0.0;
                    self.ships[a].pos += push * 0.5;
                    self.ships[b].pos -= push * 0.5;
                    self.ships[a].speed *= 0.6;
                    self.ships[b].speed *= 0.6;
                    if ram > 0.0 {
                        let at = self.ships[a].world(Vec3::Y);
                        events.push(SimEvent::cue("ram_hit", at));
                        // A boosted ram hurts the other ship far more.
                        let (da, db) = if boosted_a { (ram * 0.3, ram * 1.5) } else { (ram, ram) };
                        self.ships[a].damage(da, "ram");
                        self.ships[b].damage(db, if ida == self.mine { "player" } else { "ram" });
                        if ida == self.mine {
                            self.hull_hits += da;
                        }
                        if idb == self.mine {
                            self.hull_hits += db;
                        }
                    }
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn blast(
        &mut self,
        classes: &Classes,
        at: Vec3,
        radius: f32,
        damage: f32,
        owner: Owner,
        events: &mut Vec<SimEvent>,
        out: &mut Vec<SeaEvent>,
    ) {
        events.push(SimEvent::cue("explosion", at));
        events.push(SimEvent::Explosion { pos: at, radius: radius * 0.4 });
        let me = self.mine;
        for i in 0..self.ships.len() {
            let s = &self.ships[i];
            if !s.alive() || Owner::Vessel(s.id) == owner {
                continue;
            }
            let c = classes.get(&s.class).cloned().unwrap_or_default();
            let d = (Vec2::new(at.x, at.z) - s.pos).length() - c.beam * 0.5;
            if d < radius {
                let k = 1.0 - (d.max(0.0) / radius);
                let kind = if owner == Owner::Vessel(me) { "player" } else { "blast" };
                let dmg = self.ships[i].damage(damage * k, kind);
                if self.ships[i].id == me {
                    self.hull_hits += dmg;
                    out.push(SeaEvent::PlayerHit { damage: dmg });
                }
                self.after_damage(i, kind, out);
            }
        }
    }

    fn after_damage(&mut self, i: usize, by: &str, out: &mut Vec<SeaEvent>) {
        let me = self.mine;
        let s = &mut self.ships[i];
        if s.id == me {
            return;
        }
        let can_disable = s.tag == "restraint";
        // A disabled prize stays afloat to be boarded.
        if can_disable && s.disabled {
            s.hull = s.hull.max(s.hull_max * 0.05);
        }
        if let Some(e) = s.update_state(can_disable) {
            match e {
                NavalEvent::Disabled(id) => out.push(SeaEvent::Disabled { ship: id, by: by.to_string() }),
                NavalEvent::Sinking(id) => {
                    if by == "player" || by == "chain" {
                        self.sunk_by_player += 1;
                    }
                    out.push(SeaEvent::Sunk { ship: id, by_player: by == "player" || by == "chain" });
                }
                _ => {}
            }
        }
    }

    fn impact(&mut self, classes: &Classes, imp: &Impact, events: &mut Vec<SimEvent>, out: &mut Vec<SeaEvent>) {
        let me = self.mine;
        let from_player = imp.owner == Owner::Vessel(me);
        match imp.what {
            HitWhat::Water => {
                events.push(SimEvent::cue(if imp.kind == ShotKind::Mortar { "splash_big" } else { "splash_small" }, imp.pos));
                events.push(SimEvent::Splash { pos: imp.pos });
                if imp.blast > 0.0 {
                    self.blast(classes, imp.pos, imp.blast, imp.damage, imp.owner, events, out);
                }
                // A shot into a powder keg sets it off.
                if let Some(f) = self.floating.iter_mut().find(|f| {
                    f.alive && matches!(f.kind, FloatKind::Keg { .. }) && f.pos.distance(Vec2::new(imp.pos.x, imp.pos.z)) < 3.0
                }) {
                    f.alive = false;
                    let p = f.pos;
                    self.blast(classes, Vec3::new(p.x, 0.5, p.y), 14.0, 160.0, imp.owner, events, out);
                }
            }
            HitWhat::Ground => {
                events.push(SimEvent::cue("explosion", imp.pos));
                if imp.blast > 0.0 {
                    self.blast(classes, imp.pos, imp.blast, imp.damage, imp.owner, events, out);
                }
            }
            HitWhat::Hull(id) | HitWhat::Sail(id) => {
                let sail = matches!(imp.what, HitWhat::Sail(_));
                if let Some(bi) = self.batteries.iter().position(|b| b.id == id) {
                    let b = &mut self.batteries[bi];
                    if !b.destroyed {
                        b.hp -= imp.damage;
                        events.push(SimEvent::cue("splinters", imp.pos));
                        if b.hp <= 0.0 {
                            b.hp = 0.0;
                            b.destroyed = true;
                            let name = b.name.clone();
                            self.destroyed_order.push(name.clone());
                            events.push(SimEvent::cue("explosion", b.pos));
                            events.push(SimEvent::Explosion { pos: b.pos, radius: 6.0 });
                            out.push(SeaEvent::BatteryDown { name });
                        }
                    }
                    return;
                }
                let Some(i) = self.ships.iter().position(|s| s.id == id) else { return };
                if imp.blast > 0.0 {
                    self.blast(classes, imp.pos, imp.blast, imp.damage, imp.owner, events, out);
                    return;
                }
                let kind = match (from_player, imp.kind) {
                    (true, ShotKind::Chain) => "chain",
                    (true, _) => "player",
                    (false, _) => "enemy",
                };
                if sail {
                    let s = &mut self.ships[i];
                    s.sails = (s.sails - 0.25).max(0.1);
                    s.torn = 20.0;
                    *s.took.entry("chain".into()).or_default() += 1.0;
                    events.push(SimEvent::cue("sail_rip", imp.pos));
                } else {
                    // Swivel shots on a weak point (the powder store) do triple damage.
                    let mult = if imp.kind == ShotKind::Swivel { 3.0 } else { 1.0 };
                    let dmg = self.ships[i].damage(imp.damage * mult, kind);
                    events.push(SimEvent::cue("hull_hit", imp.pos));
                    events.push(SimEvent::cue("splinters", imp.pos));
                    if id == me {
                        self.hull_hits += dmg;
                        out.push(SeaEvent::PlayerHit { damage: dmg });
                    }
                }
                self.after_damage(i, kind, out);
            }
        }
    }

    /// The bearing (degrees, relative to the bow) of the player's broadside camera for a side.
    pub fn look_for_side(&self, side: Side) -> f32 {
        let h = self.my_ship().map(|s| s.heading).unwrap_or(0.0);
        (h + if side == Side::Starboard { 90.0 } else { -90.0 }).rem_euclid(360.0)
    }

    /// Spyglass: the ship nearest the centre of the view (within 6°) and 600 m.
    pub fn spyglass_target(&self, eye: Vec3, fwd: Vec3) -> Option<u32> {
        self.ships
            .iter()
            .filter(|s| s.id != self.mine && !s.sunk)
            .filter_map(|s| {
                let d = s.world(Vec3::Y * 4.0) - eye;
                let dist = d.length();
                let ang = d.normalize().dot(fwd.normalize()).acos().to_degrees();
                (dist < 600.0 && ang < 6.0).then_some((s.id, ang))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(id, _)| id)
    }
}
