//! Land missions (Rounds 4–6) and the systems they stand on: the town's and the fort's people
//! (brains per archetype: patrols, posts, the courier and his escorts, civilians and gossip
//! groups, Commodore Drye), their senses (`pav_kit::perception`: sight cones with line of
//! sight from raycasts, hearing, concealment in grass, hay and crowds), bodies, alarm bells and
//! reinforcements, heat and wanted posters, the player's tools (whistle, darts, Smoke Keg, Bung
//! Line, pistols), pickpocketing, the stocks, Last Words and the Tap flag.
//!
//! Guards are also `foot::Fighter`s: while unaware they are `idle` and walk their brain's way;
//! once alerted the melee rules (`foot.rs`) fight with them.

use std::collections::{BTreeMap, BTreeSet};

use glam::{Quat, Vec2, Vec3};
use pav_core::character::Posture;
use pav_core::entity::EntityId;
use pav_core::frame::SimEvent;
use pav_core::input::InputFrame;
use pav_core::play::{
    Awareness, AwarenessArc, CameraHint, DrawItem, DrawList, MarkerKind, Overlay, Prompt, Tile, UiModel, WorldMarker,
};
use pav_core::sim::Sim;
use pav_core::{Color, Shape};
use pav_kit::combat::{Assassination, Tool};
use pav_kit::perception::{self, Exposure, Heat, Mind, MindEvent, Noise, Senses, noise};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::foot::Team;
use crate::game::{BlackTap, Body, b};

const EYE: f32 = 1.6;
/// Sight checks per NPC every this many ticks (round robin).
const SIGHT_EVERY: u64 = 3;
/// People further than this from the player don't think (unless mission-critical).
const FREEZE: f32 = 120.0;

/// What a person does when nothing is wrong.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Post,
    Patrol,
    Wander,
    Gossip,
    Follow(EntityId),
    Stocks,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Npc {
    pub kind: String,
    pub group: String,
    pub tags: Vec<String>,
    pub role: Role,
    pub home: Vec3,
    pub home_facing: f32,
    pub route: Vec<Vec2>,
    pub leg: usize,
    pub once: bool,
    pub mind: Mind,
    pub senses: Senses,
    /// Nightcap dart (s asleep), Grog dart (s drunk), Smoke (s blinded), Bung Line (s stunned).
    pub sleep: f32,
    pub drunk: f32,
    pub blind: f32,
    pub stun: f32,
    pub search: Option<Vec3>,
    pub search_t: f32,
    /// Civilians: seconds left fleeing.
    pub flee: f32,
    pub shot_cd: f32,
    pub dead: bool,
    /// Fighters only: engaged in melee (not idle).
    pub fighter: bool,
}

impl Npc {
    pub fn has(&self, tag: &str) -> bool {
        self.tags.iter().any(|t| t == tag)
    }

    pub fn out(&self) -> bool {
        self.dead || self.sleep > 0.0
    }

    pub fn civilian(&self) -> bool {
        !self.fighter
    }
}

/// A body on the ground (guards who find one raise the alarm).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Corpse {
    pub at: Vec3,
    pub found: bool,
}

/// The perch survey and the Leap of Spirits in progress.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Leap {
    pub from: Vec3,
    pub to: Vec3,
    pub t: f32,
    pub len: f32,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Stealth {
    pub npcs: BTreeMap<EntityId, Npc>,
    pub heat: Heat,
    pub posters_down: BTreeSet<usize>,
    /// Noises made this tick (heard in the next sight pass).
    pub noises: Vec<Noise>,
    pub bodies: BTreeMap<EntityId, Corpse>,
    /// Seconds the bells keep ringing.
    pub alarm: f32,
    pub alarms: u32,
    pub reinforcements: u32,
    pub detections: u32,
    /// The player's concealment now (for the HUD, the bot and assassinations).
    pub hidden: bool,
    pub concealed: bool,
    pub restricted: bool,
    /// The tool wheel is open (L1 held) and the slot under the stick.
    pub wheel: Option<usize>,
    /// Hold timers for interactions (stocks, flag, pickpocket, posters).
    pub hold: f32,
    pub hold_what: String,
    /// Crow's Perch survey: (perch, seconds) while the camera pans.
    pub survey: Option<(String, f32)>,
    pub leap: Option<Leap>,
    pub revealed: BTreeSet<String>,
    pub collected: BTreeSet<String>,
    /// Last Words: seconds into the scene.
    pub last_words: Option<f32>,
    pub flag_raised: bool,
    pub ashore_at: Option<f32>,
    /// Quick-shot wind-up (s).
    pub quick: Option<f32>,
    /// Seconds since a crime was seen (civilians point you out at heat 2+).
    pub since_crime: f32,
    /// The fade over the tunnel cut (seconds left).
    #[serde(default)]
    pub tunnel_fade: f32,
}

fn yaw_to(from: Vec3, to: Vec3) -> f32 {
    (to.x - from.x).atan2(to.z - from.z)
}

fn flat(v: Vec3) -> Vec2 {
    Vec2::new(v.x, v.z)
}

fn fwd(facing: f32) -> Vec2 {
    Vec2::new(facing.sin(), facing.cos())
}

/// A deterministic 0..1 from a seed.
fn hash01(a: u64, b: u64) -> f32 {
    let mut h = a.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ b.wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
    h ^= h >> 29;
    h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h ^= h >> 32;
    (h % 10_000) as f32 / 10_000.0
}

fn heading_rad(deg: f32) -> f32 {
    // Degrees from north (−Z), clockwise → character facing (0 = +Z).
    let h = deg.to_radians();
    h.sin().atan2(-h.cos())
}

/// Ground under a point (terrain, else the shapes), at least sea level plus a bit.
fn ground_at(g: &BlackTap, sim: &Sim, p: Vec2, hint_y: f32) -> f32 {
    // A ray from above finds floors, roofs and terrain.
    let top = Vec3::new(p.x, hint_y + 3.0, p.y);
    if let Some(h) = sim.raycast(top, Vec3::NEG_Y, 40.0, None) {
        return h.point.y;
    }
    g.world().ground(p).max(0.2)
}

impl BlackTap {
    /// A noise the people around may hear.
    pub fn noise(&mut self, at: Vec3, radius: f32) {
        self.stealth.noises.push(Noise { at, radius });
    }
}

fn spawn(g: &mut BlackTap, sim: &mut Sim, s: &crate::world::NpcSpawn, offset: Vec2, idx: u32) -> EntityId {
    let w = g.world();
    let at = Vec3::from(s.at);
    let xz = flat(at) + offset;
    let y = ground_at(g, sim, xz, at.y.max(2.0));
    let feet = Vec3::new(xz.x, y + 0.05, xz.y);
    let facing = heading_rad(s.facing);
    let route = if s.route.is_empty() { Vec::new() } else { w.route(&s.route) };
    // The courier can be cut down (he has a fighter's body) but he runs rather than fights.
    let fighter = !matches!(s.kind.as_str(), "civilian" | "molasses");
    let id = if fighter {
        g.land.spawn_fighter(sim, &s.kind, Team::Enemy, feet, facing, crate::looks::look(&s.kind))
    } else {
        sim.spawn_npc(&s.kind, feet, facing, crate::looks::look(&s.kind), None, None)
    };
    if let Some(f) = g.land.fighters.get_mut(&id) {
        f.idle = true;
    }
    let role = if s.has_tag("escort") {
        Role::Follow(EntityId(0))
    } else if s.kind == "molasses" {
        Role::Stocks
    } else if s.has_tag("gossip") {
        Role::Gossip
    } else if !route.is_empty() {
        Role::Patrol
    } else if s.kind == "civilian" {
        Role::Wander
    } else {
        Role::Post
    };
    let leg = (idx as usize) % route.len().max(1);
    g.stealth.npcs.insert(
        id,
        Npc {
            kind: s.kind.clone(),
            group: s.group.clone(),
            tags: s.tags.clone(),
            role,
            home: feet,
            home_facing: facing,
            route,
            leg,
            once: s.once,
            mind: Mind::default(),
            senses: crate::looks::senses(&s.kind),
            sleep: 0.0,
            drunk: 0.0,
            blind: 0.0,
            stun: 0.0,
            search: None,
            search_t: 0.0,
            flee: 0.0,
            shot_cd: 2.0,
            dead: false,
            fighter,
        },
    );
    id
}

fn spawn_group(g: &mut BlackTap, sim: &mut Sim, group: &str) {
    if g.flags.contains(&format!("spawned_{group}")) {
        return;
    }
    g.flags.insert(format!("spawned_{group}"));
    let w = g.world();
    let list: Vec<_> = w.npcs.iter().filter(|n| n.group == group).cloned().collect();
    let mut lead = None;
    for s in list {
        let n = s.count.max(1);
        for i in 0..n {
            let a = i as f32 / n as f32 * std::f32::consts::TAU;
            let off = if n > 1 { Vec2::new(a.cos(), a.sin()) * 1.3 } else { Vec2::ZERO };
            let id = spawn(g, sim, &s, off, i);
            if s.has_tag("courier") || s.kind == "drye" {
                lead = Some(id);
            }
        }
    }
    // Escorts follow the courier.
    if let Some(l) = lead {
        for n in g.stealth.npcs.values_mut().filter(|n| n.group == group && n.has("escort")) {
            n.role = Role::Follow(l);
        }
    }
}

impl crate::world::NpcSpawn {
    pub fn has_tag(&self, t: &str) -> bool {
        self.tags.iter().any(|x| x == t)
    }
}

/// Land actions (`rounds::act` hands over the names it doesn't know).
pub fn act(g: &mut BlackTap, sim: &mut Sim, name: &str, _arg: Option<&str>) {
    match name {
        "spawn_town" => spawn_group(g, sim, "town"),
        "spawn_tavern" => spawn_group(g, sim, "tavern"),
        "spawn_courier" => spawn_group(g, sim, "courier"),
        "spawn_stocks_guards" => spawn_group(g, sim, "stocks"),
        "spawn_fort_guards" => spawn_group(g, sim, "fort"),
        "drye_balcony" => spawn_group(g, sim, "drye"),
        "alarm" => ring_alarm(g, sim),
        "has_key" => g.mission.set_flag("has_key"),
        "last_words" => {
            g.stealth.last_words = Some(0.0);
            g.music = "last_words".into();
            g.set_atmosphere("memory_mist", 1.5);
            g.dialog.say(&g.content().dialog, "r6_last_words_1");
        }
        other => log::warn!("blacktap: unknown mission action '{other}'"),
    }
}

/// Land conditions (the land Rounds use flags; nothing else yet).
pub fn eval(_g: &BlackTap, _sim: &Sim, name: &str) -> bool {
    log::warn!("blacktap: unknown mission condition '{name}'");
    false
}

fn ring_alarm(g: &mut BlackTap, sim: &mut Sim) {
    if g.stealth.alarm <= 0.0 {
        g.stealth.alarms += 1;
        let at = g.world().marker("bell_tower").unwrap_or_default();
        g.pending_events.push(SimEvent::cue("alarm_bell", at));
        g.toast("The bells! The fort is roused.");
    }
    g.stealth.alarm = 30.0;
    g.stealth.heat.crime(3.0);
    let target = BlackTap::player_feet(sim).unwrap_or_default();
    for n in g.stealth.npcs.values_mut().filter(|n| n.group == "fort" && !n.out()) {
        n.mind.meter = n.mind.meter.max(0.7);
        n.mind.noise = Some(target);
        if !n.mind.aware() {
            n.mind.state = Awareness::Searching;
            n.mind.t = 0.0;
            n.mind.last_known = Some(target);
        }
    }
    // Reinforcements from the keep (bounded).
    let w = g.world();
    let keep = w.marker("fort_keep").unwrap_or_default();
    while g.stealth.reinforcements < 4 {
        let i = g.stealth.reinforcements;
        g.stealth.reinforcements += 1;
        let s = crate::world::NpcSpawn {
            group: "fort".into(),
            kind: if i % 2 == 0 { "guard".into() } else { "musketeer".into() },
            at: [keep.x + i as f32 * 1.5 - 2.0, keep.y, keep.z + 6.0],
            ..Default::default()
        };
        let id = spawn(g, sim, &s, Vec2::ZERO, i);
        if let Some(n) = g.stealth.npcs.get_mut(&id) {
            n.mind.state = Awareness::Searching;
            n.mind.last_known = Some(target);
            n.mind.noise = Some(target);
        }
    }
}

/// A kill: bodies, witnesses (heat), the Rounds' flags.
pub fn on_kill(g: &mut BlackTap, sim: &mut Sim, id: EntityId, how: Option<Assassination>) {
    let at = sim.feet(id).unwrap_or_default();
    let Some(n) = g.stealth.npcs.get_mut(&id) else { return };
    n.dead = true;
    let (kind, group, courier) = (n.kind.clone(), n.group.clone(), n.has("courier"));
    g.stealth.bodies.insert(id, Corpse { at, found: false });
    g.noise(at, noise::BODY_DROP);
    if courier {
        if how == Some(Assassination::Air) {
            g.mission.set_flag("courier_air");
        }
        g.flags.insert("key_dropped".into());
    }
    if group == "stocks" {
        g.mission.set_flag("killed_at_stocks");
    }
    if group == "contract" {
        g.flags.insert("contract_done".into());
        g.stats.reales += 500;
        g.toast("Contract complete: 500 reales");
        g.pending_events.push(SimEvent::cue("pickup_chest", at));
    }
    if kind == "drye" {
        g.mission.set_flag("drye_dead");
        if matches!(how, Some(Assassination::Air | Assassination::Hiding)) {
            g.mission.set_flag("drye_air_or_hiding");
        }
    }
    // Witnesses within 20 m who can see it raise the heat (only in town and fort).
    let witnessed = g
        .stealth
        .npcs
        .iter()
        .any(|(&wid, w)| wid != id && !w.out() && w.blind <= 0.0 && sees_point(sim, wid, at + Vec3::Y, 20.0));
    if witnessed {
        g.stealth.heat.crime(if how.is_some() { 1.0 } else { 0.6 });
        g.stealth.since_crime = 0.0;
    }
}

/// Line of sight from an NPC's eye to a point within `range`.
fn sees_point(sim: &Sim, id: EntityId, p: Vec3, range: f32) -> bool {
    let Some(feet) = sim.feet(id) else { return false };
    let eye = feet + Vec3::Y * EYE;
    let d = p - eye;
    let dist = d.length();
    if dist > range || dist < 0.05 {
        return dist <= range;
    }
    let facing = sim.state.entities.get(id).and_then(|e| e.character.as_ref()).map(|c| c.facing).unwrap_or(0.0);
    if fwd(facing).dot(flat(d).normalize_or_zero()) < -0.2 {
        return false;
    }
    sim.raycast(eye, d, dist - 0.25, Some(id)).is_none()
}

/// The player's exposure to a looker now.
fn player_exposure(g: &BlackTap, sim: &Sim) -> (Vec3, Exposure, [f32; 3]) {
    let Some(pid) = sim.state.player else { return (Vec3::ZERO, Exposure::default(), [1.6, 1.0, 0.2]) };
    let feet = sim.feet(pid).unwrap_or_default();
    let ch = sim.state.entities.get(pid).and_then(|e| e.character.as_ref());
    let crouched = ch.is_some_and(|c| c.posture != Posture::Stand);
    let speed = ch.map(|c| flat(c.vel).length()).unwrap_or(0.0);
    let heights = if crouched { [1.0, 0.6, 0.2] } else { [1.6, 1.0, 0.2] };
    let e = Exposure {
        concealed: g.stealth.concealed,
        hidden: g.stealth.hidden,
        crouched,
        sprinting: speed > 5.5,
        restricted: g.stealth.restricted,
        heat: g.stealth.heat.level(),
        visible: true,
    };
    (feet, e, heights)
}

/// Where the player hides: hay hides, grass and crowds conceal (unless high-profile).
fn update_concealment(g: &mut BlackTap, sim: &Sim) {
    let Some(pid) = sim.state.player else { return };
    let feet = sim.feet(pid).unwrap_or_default();
    let ch = sim.state.entities.get(pid).and_then(|e| e.character.as_ref());
    let speed = ch.map(|c| flat(c.vel).length()).unwrap_or(0.0);
    let grounded = ch.is_some_and(|c| c.grounded || c.swimming);
    let high = speed > 5.5 || !grounded;
    let w = g.world();
    let spot = w.hide_at(feet).map(|h| h.kind.clone());
    g.stealth.hidden = matches!(spot.as_deref(), Some("hay" | "haystack" | "cart")) && grounded;
    let in_grass = spot.as_deref() == Some("grass") && !high;
    // A gossip group carries a concealing volume.
    let crowd = !high
        && g.stealth
            .npcs
            .iter()
            .filter(|(_, n)| n.role == Role::Gossip && !n.out())
            .filter_map(|(&id, _)| sim.feet(id))
            .filter(|p| flat(*p - feet).length() < 1.6)
            .count()
            >= 2;
    let diving = ch.is_some_and(|c| c.swimming && c.water_depth > 2.0 && c.vel.y < -0.1);
    g.stealth.concealed = in_grass || crowd || diving || g.stealth.hidden;
    g.stealth.restricted = w.area_at(flat(feet)).is_some_and(|a| a.kind == "restricted");
}

/// The sight pass: round-robin raycasts, minds, alerts spreading to allies.
fn perceive(g: &mut BlackTap, sim: &Sim, dt: f32, events: &mut Vec<SimEvent>) {
    let Some(pid) = sim.state.player else { return };
    let (pfeet, exposure, heights) = player_exposure(g, sim);
    let noises = std::mem::take(&mut g.stealth.noises);
    let tick = sim.state.tick;
    let ids: Vec<EntityId> = g.stealth.npcs.keys().copied().collect();
    let mut alerts: Vec<(Vec3, Vec3)> = Vec::new();
    let mut found_bodies = Vec::new();
    let in_smoke = |p: Vec3, g: &BlackTap| g.land.hero.tools.in_smoke(p);
    for id in ids {
        let Some(n) = g.stealth.npcs.get(&id) else { continue };
        if n.out() || !n.fighter && n.kind != "courier" && n.kind != "civilian" {
            continue;
        }
        let Some(feet) = sim.feet(id) else { continue };
        if flat(feet - pfeet).length() > FREEZE && !n.has("critical") {
            continue;
        }
        let facing = sim.state.entities.get(id).and_then(|e| e.character.as_ref()).map(|c| c.facing).unwrap_or(n.home_facing);
        let heard = noises.iter().find(|z| perception::hears(feet, z, &n.senses)).map(|z| z.at);
        // Sight every few ticks (staggered by id), hearing every tick.
        let check = (tick + id.0 as u64).is_multiple_of(SIGHT_EVERY);
        let mut n = n.clone();
        let blind = n.blind > 0.0 || in_smoke(feet, g) || n.drunk > 0.0 || n.stun > 0.0;
        if check {
            let eye = feet + Vec3::Y * EYE;
            let searching = n.mind.state == Awareness::Searching;
            let sight = perception::look(eye, facing, pfeet + Vec3::Y * heights[1], &n.senses, searching);
            let mut e = exposure;
            e.visible = (sight.central || sight.peripheral)
                && !blind
                && heights.iter().any(|&h| {
                    let t = pfeet + Vec3::Y * h;
                    let d = t - eye;
                    sim.raycast(eye, d, d.length() - 0.3, Some(id)).is_none()
                });
            let rate = if n.civilian() { 0.0 } else { perception::sight_rate(&n.senses, &sight, &e) };
            // Civilians at heat 2+ point the player out when close.
            if n.civilian() && e.visible && g.stealth.heat.level() >= 2 && sight.dist < 8.0 && n.flee <= 0.0 {
                n.flee = 6.0;
                g.stealth.noises.push(Noise { at: pfeet, radius: noise::SCREAM });
                events.push(SimEvent::cue("crowd_scream", feet + Vec3::Y));
            }
            let arrived = n.mind.noise.or(n.mind.last_known).is_some_and(|t| flat(t - feet).length() < 1.6);
            let before = n.mind.state;
            let ev = if n.civilian() { None } else { n.mind.update(rate, pfeet, heard, arrived, dt * SIGHT_EVERY as f32) };
            // Searchers poke bushes: a poke within 1 m finds a hidden player.
            if n.mind.state == Awareness::Searching
                && (g.stealth.concealed || g.stealth.hidden)
                && flat(pfeet - feet).length() < 1.0
            {
                n.mind.alert(pfeet);
            }
            match ev {
                Some(MindEvent::Suspicious) => events.push(SimEvent::cue("guard_suspicious", feet + Vec3::Y * 1.8)),
                Some(MindEvent::Alerted) => {
                    events.push(SimEvent::cue("guard_alert", feet + Vec3::Y * 1.8));
                    alerts.push((feet, pfeet));
                }
                Some(MindEvent::Investigating) => events.push(SimEvent::cue("guard_suspicious", feet + Vec3::Y * 1.8)),
                _ => {}
            }
            if before != n.mind.state && n.mind.aware() && !matches!(before, Awareness::Alerted | Awareness::Combat) {
                alerts.push((feet, pfeet));
            }
            // Bodies in view.
            if !n.civilian() && !n.mind.aware() {
                for (&bid, c) in &g.stealth.bodies {
                    if !c.found && flat(c.at - feet).length() < 14.0 {
                        let d = c.at + Vec3::Y * 0.3 - eye;
                        if fwd(facing).dot(flat(d).normalize_or_zero()) > 0.2
                            && sim.raycast(eye, d, d.length() - 0.4, Some(id)).is_none()
                        {
                            found_bodies.push((bid, id, c.at));
                        }
                    }
                }
            }
        } else if let Some(h) = heard {
            if !n.civilian() && n.mind.state == Awareness::Unaware {
                n.mind.update(0.0, pfeet, Some(h), false, dt);
            }
        }
        g.stealth.npcs.insert(id, n);
    }
    let _ = pid;
    // A body: the finder investigates it and the heat rises.
    for (bid, finder, at) in found_bodies {
        if let Some(c) = g.stealth.bodies.get_mut(&bid) {
            if !c.found {
                c.found = true;
                g.stealth.heat.crime(0.5);
                events.push(SimEvent::cue("guard_alert", at + Vec3::Y));
                if let Some(n) = g.stealth.npcs.get_mut(&finder) {
                    n.mind.noise = Some(at);
                    n.mind.state = Awareness::Investigating;
                    n.mind.t = 0.0;
                    n.mind.meter = n.mind.meter.max(0.5);
                }
            }
        }
    }
    // An alert spreads: shouts reach allies within 18 m.
    for (from, target) in alerts {
        g.stealth.detections += 1;
        g.mission.set_flag("detected");
        if !g.mission.flag("drye_dead") {
            g.mission.set_flag("detected_before_drye");
        }
        let ids: Vec<EntityId> = g.stealth.npcs.keys().copied().collect();
        for oid in ids {
            let Some(p) = sim.feet(oid) else { continue };
            if let Some(o) = g.stealth.npcs.get_mut(&oid) {
                if o.out() {
                    continue;
                }
                if o.civilian() {
                    if flat(p - from).length() < 15.0 {
                        o.flee = 8.0;
                    }
                } else if flat(p - from).length() < 18.0 {
                    o.mind.alert(target);
                }
            }
        }
        // The fort's bells, if the alarm is raised in the fort.
        if g.world().area_at(flat(from)).is_some_and(|a| a.kind == "restricted") && g.stealth.alarm <= 0.0 {
            g.flags.insert("alarm_pending".into());
        }
    }
}

/// Movement for one person who isn't fighting.
fn walk_to(feet: Vec3, to: Vec3, speed: f32) -> (Vec2, Option<f32>) {
    let d = flat(to - feet);
    let l = d.length();
    if l < 0.4 {
        return (Vec2::ZERO, None);
    }
    (d / l * speed.min(l), Some(d.x.atan2(d.y)))
}

/// Every person's brain for this tick → its input.
fn brains(g: &mut BlackTap, sim: &mut Sim, dt: f32, events: &mut Vec<SimEvent>) -> BTreeMap<EntityId, InputFrame> {
    let mut out = BTreeMap::new();
    let pfeet = BlackTap::player_feet(sim).unwrap_or_default();
    let ids: Vec<EntityId> = g.stealth.npcs.keys().copied().collect();
    let positions: BTreeMap<EntityId, Vec3> = ids.iter().filter_map(|&id| sim.feet(id).map(|p| (id, p))).collect();
    let time = g.time;
    for id in ids {
        let Some(mut n) = g.stealth.npcs.get(&id).cloned() else { continue };
        let Some(&feet) = positions.get(&id) else { continue };
        n.sleep = (n.sleep - dt).max(0.0);
        n.blind = (n.blind - dt).max(0.0);
        n.stun = (n.stun - dt).max(0.0);
        n.flee = (n.flee - dt).max(0.0);
        n.shot_cd = (n.shot_cd - dt).max(0.0);
        if let Some(f) = g.land.fighters.get(&id) {
            if f.vitals.dead {
                n.dead = true;
            }
        }
        if n.drunk > 0.0 {
            n.drunk -= dt;
            if n.drunk <= 0.0 {
                n.sleep = 45.0;
            }
        }
        // Down: asleep (slumped) or dead (the fighter rules topple the dead).
        if let Some(ch) = sim.state.entities.get_mut(id).and_then(|e| e.character.as_mut()) {
            if n.sleep > 0.0 && !n.dead {
                ch.anim.down = (ch.anim.down + dt * 2.0).min(0.85);
            } else if !n.dead && ch.anim.down > 0.0 {
                ch.anim.down = (ch.anim.down - dt * 1.5).max(0.0);
            }
        }
        let far = flat(feet - pfeet).length() > FREEZE && !n.has("critical");
        if n.out() || n.stun > 0.0 || far {
            g.stealth.npcs.insert(id, n);
            out.insert(id, InputFrame::default());
            continue;
        }
        let coward = n.kind == "courier";
        if coward && n.mind.aware() {
            n.flee = n.flee.max(1.0);
        }
        let engaged = n.fighter && !coward && n.mind.aware() && n.drunk <= 0.0 && n.blind <= 0.0;
        if let Some(f) = g.land.fighters.get_mut(&id) {
            f.idle = !engaged;
        }
        let mut inp = InputFrame::default();
        let mut face: Option<f32> = None;
        if n.drunk > 0.0 {
            // Staggers about and swings at whoever is near.
            let a = time * 1.7 + id.0 as f32;
            inp.move_dir = Vec2::new(a.sin(), a.cos()) * 0.25;
            if n.drunk % 1.5 < dt {
                for (&oid, &p) in &positions {
                    if oid != id && flat(p - feet).length() < 2.2 {
                        if let Some(f) = g.land.fighters.get_mut(&oid) {
                            if !f.vitals.dead && f.vitals.hurt(14.0) {
                                g.land.kills += 0;
                            }
                        }
                        events.push(SimEvent::cue("sword_clash", p + Vec3::Y));
                    }
                }
            }
        } else if engaged {
            // Musketeers keep their distance and shoot; the melee rules drive the rest.
            if n.kind == "musketeer" {
                let d = flat(pfeet - feet).length();
                face = Some(yaw_to(feet, pfeet));
                if d < 25.0 && d > 3.5 && n.shot_cd <= 0.0 && sees_point(sim, id, pfeet + Vec3::Y, 25.0) {
                    n.shot_cd = 4.0;
                    events.push(SimEvent::cue("pistol_fire", feet + Vec3::Y * 1.5));
                    g.stealth.noises.push(Noise { at: feet, radius: noise::PISTOL });
                    if g.land.hero.vitals.iframes <= 0.0 && hash01(id.0 as u64, sim.state.tick) < 0.4 {
                        let dmg = 10.0 * g.options.difficulty;
                        g.land.hero.vitals.hurt(dmg);
                        g.shake(0.3, (0.6, 0.4, 120));
                    }
                }
                if let Some(f) = g.land.fighters.get_mut(&id) {
                    f.idle = d > 3.5;
                }
            }
            // Drye runs for the bell tower when alarmed.
            if n.kind == "drye" && g.stealth.alarm <= 0.0 {
                if let Some(bell) = g.world().marker("bell_tower") {
                    let (m, f) = walk_to(feet, bell, 1.0);
                    inp.move_dir = m;
                    face = f;
                    if let Some(fi) = g.land.fighters.get_mut(&id) {
                        fi.idle = true;
                    }
                    if flat(bell - feet).length() < 2.5 {
                        g.flags.insert("alarm_pending".into());
                    }
                }
            }
        } else if (n.civilian() || coward) && n.flee > 0.0 {
            let away = flat(feet - pfeet).normalize_or(Vec2::X);
            inp.move_dir = away;
            face = Some(away.x.atan2(away.y));
        } else {
            match n.mind.state {
                Awareness::Suspicious => {
                    face = n.mind.last_known.map(|t| yaw_to(feet, t));
                }
                Awareness::Investigating => {
                    if let Some(t) = n.mind.noise.or(n.mind.last_known) {
                        let (m, f) = walk_to(feet, t, 0.32);
                        inp.move_dir = m;
                        face = f;
                    }
                }
                Awareness::Searching => {
                    n.search_t -= dt;
                    let centre = n.mind.last_known.or(n.mind.noise).unwrap_or(n.home);
                    if n.search.is_none() || n.search_t <= 0.0 {
                        let a = hash01(id.0 as u64, (time * 0.25) as u64) * std::f32::consts::TAU;
                        let r = 1.5 + hash01(id.0 as u64 + 7, (time * 0.25) as u64) * 5.0;
                        n.search = Some(centre + Vec3::new(a.cos() * r, 0.0, a.sin() * r));
                        n.search_t = 4.0;
                    }
                    if let Some(t) = n.search {
                        let (m, f) = walk_to(feet, t, 0.28);
                        inp.move_dir = m;
                        face = f.or(Some(n.home_facing + time.sin()));
                    }
                }
                _ => {
                    // The role.
                    match n.role.clone() {
                        Role::Patrol => {
                            if let Some(&p) = n.route.get(n.leg) {
                                let to = Vec3::new(p.x, feet.y, p.y);
                                let speed = if n.has("courier") || n.has("escort") { 0.22 } else { 0.24 };
                                let (m, f) = walk_to(feet, to, speed);
                                inp.move_dir = m;
                                face = f;
                                if flat(to - feet).length() < 1.2 {
                                    if n.leg + 1 < n.route.len() {
                                        n.leg += 1;
                                    } else if !n.once {
                                        n.leg = 0;
                                    } else {
                                        n.route.clear();
                                        n.role = Role::Post;
                                        n.home = feet;
                                    }
                                }
                            }
                        }
                        Role::Follow(lead) => {
                            if let Some(&lp) = positions.get(&lead) {
                                let side = if id.0 % 2 == 0 { 1.0 } else { -1.0 };
                                let lf = sim
                                    .state
                                    .entities
                                    .get(lead)
                                    .and_then(|e| e.character.as_ref())
                                    .map(|c| c.facing)
                                    .unwrap_or(0.0);
                                let back = fwd(lf);
                                let slot =
                                    lp - Vec3::new(back.x, 0.0, back.y) * 1.8 + Vec3::new(back.y, 0.0, -back.x) * side * 1.2;
                                let (m, f) = walk_to(feet, slot, 0.3);
                                inp.move_dir = m;
                                face = f.or(Some(lf));
                            } else {
                                n.role = Role::Post;
                            }
                        }
                        Role::Wander => {
                            let bucket = (time / 9.0) as u64;
                            let a = hash01(id.0 as u64, bucket) * std::f32::consts::TAU;
                            let r = hash01(id.0 as u64 + 3, bucket) * 6.0;
                            let to = n.home + Vec3::new(a.cos() * r, 0.0, a.sin() * r);
                            let (m, f) = walk_to(feet, to, 0.18);
                            inp.move_dir = m;
                            face = f;
                        }
                        Role::Gossip | Role::Post | Role::Stocks => {
                            let (m, f) = walk_to(feet, n.home, 0.25);
                            inp.move_dir = m;
                            face = f.or(Some(n.home_facing));
                            if n.role == Role::Stocks && g.mission.flag("molasses_freed") {
                                if let Some(t) = g.world().marker("tavern") {
                                    n.home = t;
                                    n.role = Role::Post;
                                }
                            }
                        }
                    }
                }
            }
        }
        if let Some(ch) = sim.state.entities.get_mut(id).and_then(|e| e.character.as_mut()) {
            ch.face = face;
        }
        g.stealth.npcs.insert(id, n);
        out.insert(id, inp);
    }
    out
}

/// What the player can do here now (the prompt and its hold time).
fn interaction(g: &BlackTap, sim: &Sim) -> Option<(&'static str, &'static str, f32)> {
    let p = BlackTap::player_feet(sim)?;
    let w = g.world();
    let near = |name: &str, r: f32| w.marker(name).is_some_and(|m| flat(m - p).length() < r && (m.y - p.y).abs() < 3.5);
    if g.mission.flag("drye_dead") && g.mission.flag("last_words_done") && !g.mission.flag("flag_raised") && near("flagpole", 4.0)
    {
        return Some(("flag", "Raise the Tap flag", 1.5));
    }
    if g.mode == crate::game::Mode::FreeRoam && !g.flags.contains("contract_taken") && near("tavern", 3.5) {
        return Some(("contract", "Mother Molasses: take the contract", 0.6));
    }
    if g.mission.flag("has_key") && !g.mission.flag("in_fort") && near("cave", 2.6) {
        return Some(("tunnel", "Take the smugglers' tunnel", 0.5));
    }
    if !g.mission.flag("molasses_freed") && g.flags.contains("spawned_stocks") && near("stocks", 3.0) {
        return Some(("stocks", "Free Mother Molasses", 1.0));
    }
    if !g.mission.flag("has_key") {
        if let Some((cid, _)) = g.stealth.npcs.iter().find(|(_, n)| n.has("courier")) {
            if let Some(cf) = sim.feet(*cid) {
                let n = &g.stealth.npcs[cid];
                let d = flat(cf - p).length();
                if n.dead && d < 2.2 {
                    return Some(("key", "Take the cellar key", 0.3));
                }
                let facing = sim.state.entities.get(*cid).and_then(|e| e.character.as_ref()).map(|c| c.facing).unwrap_or(0.0);
                let behind = fwd(facing).dot(flat(p - cf).normalize_or_zero()) < -0.3;
                if !n.out() && !n.mind.aware() && d < 1.8 && behind {
                    return Some(("pickpocket", "Pickpocket", 0.6));
                }
            }
        }
    }
    for (i, poster) in w.posters.iter().enumerate() {
        if !g.stealth.posters_down.contains(&i) && flat(*poster - p).length() < 2.2 && (poster.y - p.y).abs() < 3.0 {
            return Some(("poster", "Tear down the poster", 0.4));
        }
    }
    if g.body == Body::Foot && g.land.deck.is_some() {
        if let Some(s) = g.sea.my_ship() {
            let c = g.content();
            let cl = c.classes.get(&s.class).cloned().unwrap_or_default();
            let helm = s.world(Vec3::new(0.0, cl.freeboard, -cl.length * 0.36));
            if flat(helm - p).length() < 3.5 {
                return Some(("helm", "Take the helm", 0.3));
            }
        }
    }
    None
}

fn do_interaction(g: &mut BlackTap, sim: &mut Sim, what: &str, events: &mut Vec<SimEvent>) {
    let p = BlackTap::player_feet(sim).unwrap_or_default();
    match what {
        "flag" => {
            g.mission.set_flag("flag_raised");
            g.stealth.flag_raised = true;
            g.flags.insert("fort_captured".into());
            events.push(SimEvent::cue("fort_captured", p));
            g.show("raise_flag");
        }
        "stocks" => {
            // Watching guards see it happen.
            let watched: Vec<EntityId> = g
                .stealth
                .npcs
                .iter()
                .filter(|(_, n)| n.group == "stocks" && !n.out() && n.blind <= 0.0 && n.drunk <= 0.0)
                .map(|(&id, _)| id)
                .filter(|&id| sees_point(sim, id, p + Vec3::Y, 9.0))
                .collect();
            for id in &watched {
                if let Some(n) = g.stealth.npcs.get_mut(id) {
                    n.mind.alert(p);
                }
            }
            g.mission.set_flag("molasses_freed");
            events.push(SimEvent::cue("pickup_chest", p));
            g.show("free_molasses");
        }
        "key" | "pickpocket" => {
            g.mission.set_flag("has_key");
            events.push(SimEvent::cue("pickpocket", p));
            g.show(if what == "pickpocket" { "pickpocket" } else { "key" });
            if what == "pickpocket" {
                g.mission.set_flag("pickpocketed");
            }
        }
        "poster" => {
            let w = g.world();
            if let Some(i) = w.posters.iter().position(|q| flat(*q - p).length() < 2.2) {
                g.stealth.posters_down.insert(i);
                g.stealth.heat.poster();
                events.push(SimEvent::cue("poster_tear", p));
                g.show("poster");
            }
        }
        "contract" => {
            // A tax collector walks one of the town's routes, picked from the seed and the time.
            g.flags.insert("contract_taken".into());
            let routes = ["town_patrol_a", "town_patrol_b", "courier_route"];
            let pick = routes[((g.seed.wrapping_mul(31) + g.time as u64) % routes.len() as u64) as usize];
            let route = g.world().route(pick);
            let start = route.first().copied().unwrap_or(glam::Vec2::new(184.0, 18.0));
            let spawn_def = crate::world::NpcSpawn {
                group: "contract".into(),
                kind: "officer".into(),
                at: [start.x, 2.4, start.y],
                route: pick.into(),
                tags: vec!["collector".into(), "critical".into()],
                ..Default::default()
            };
            spawn(g, sim, &spawn_def, glam::Vec2::ZERO, 0);
            let c = g.content();
            g.dialog.say(&c.dialog, "fr_contract");
            g.toast("Contract: the tax collector walks Dryport");
            events.push(SimEvent::cue("objective_new", p));
        }
        "tunnel" => {
            // Through the smugglers' tunnel to the fort's cellar (a fade covers the cut).
            if let (Some(pid), Some(to)) = (sim.state.player, g.world().marker("cellar")) {
                sim.set_position(pid, to + Vec3::Y * 0.1);
            }
            g.stealth.tunnel_fade = 1.2;
            events.push(SimEvent::cue("climb_grab", p));
            g.show("tunnel");
        }
        "helm" => {
            if let Some(d) = g.land.deck.take() {
                sim.despawn(d);
            }
            g.body = Body::Helm;
            if let Some(s) = g.sea.ship_mut(g.sea.mine) {
                s.anchored = false;
            }
            events.push(SimEvent::cue("ship_bell", p));
            g.show("take_helm");
        }
        _ => {}
    }
}

/// The player on land: interactions, the whistle, tools and pistols; then everyone's brain.
pub fn pre(
    g: &mut BlackTap,
    sim: &mut Sim,
    input: &InputFrame,
    playing: bool,
    dt: f32,
    events: &mut Vec<SimEvent>,
) -> BTreeMap<EntityId, InputFrame> {
    update_concealment(g, sim);
    let feet = BlackTap::player_feet(sim).unwrap_or_default();
    if playing && g.body == Body::Foot && g.land.boarding.is_none() {
        // Interactions (hold △).
        match interaction(g, sim) {
            Some((what, _, need)) if input.down(b::INTERACT) => {
                if g.stealth.hold_what != what {
                    g.stealth.hold_what = what.to_string();
                    g.stealth.hold = 0.0;
                }
                g.stealth.hold += dt;
                if g.stealth.hold >= need {
                    g.stealth.hold = 0.0;
                    g.stealth.hold_what.clear();
                    do_interaction(g, sim, what, events);
                }
            }
            _ => {
                g.stealth.hold = 0.0;
            }
        }
        // Whistle (lure).
        if input.just(b::WHISTLE) {
            g.noise(feet, noise::WHISTLE);
            events.push(SimEvent::cue("whistle", feet + Vec3::Y * 1.6));
            g.learned("whistle");
            g.show("whistle");
        }
        // The tool wheel (hold L1): slow motion, the stick picks a slot.
        if input.down(b::TOOLS) {
            let f = g.cam.forward();
            let r = Vec2::new(-f.y, f.x);
            let axis = Vec2::new(input.move_dir.dot(r), input.move_dir.dot(f));
            let mut slot = g.stealth.wheel.unwrap_or(g.land.hero.tools.selected);
            if axis.length() > 0.5 {
                // Up = 0, right = 1, down = 2, left = 3.
                let a = axis.x.atan2(axis.y).to_degrees().rem_euclid(360.0);
                slot = (((a + 45.0) / 90.0) as usize) % 4;
            }
            g.stealth.wheel = Some(slot);
            g.land.slow = g.land.slow.max(dt * 2.0);
            g.land.slow_scale = 0.2;
        } else if let Some(slot) = g.stealth.wheel.take() {
            g.land.hero.tools.selected = slot;
            events.push(SimEvent::cue("ui_confirm", feet));
            g.show("tool_wheel");
        }
        // Aim + fire: the selected tool, else the pistols; fire alone: a quick-shot.
        if input.just(b::FIRE) && g.stealth.wheel.is_none() {
            let aiming = input.down(b::AIM) || input.triggers[0] > 0.3;
            use_tool(g, sim, aiming, events);
        }
        if let Some(t) = g.stealth.quick {
            let t = t - dt;
            if t <= 0.0 {
                g.stealth.quick = None;
                pistol(g, sim, false, events);
            } else {
                g.stealth.quick = Some(t);
            }
        }
        // Sprinting makes noise.
        let speed = sim
            .state
            .player
            .and_then(|id| sim.state.entities.get(id))
            .and_then(|e| e.character.as_ref())
            .map(|c| flat(c.vel).length())
            .unwrap_or(0.0);
        if speed > 6.0 && sim.state.tick.is_multiple_of(20) {
            g.noise(feet, noise::SPRINT_STEP);
        }
        // Coming aboard from the water (the hull ladder).
        let swimming = sim
            .state
            .player
            .and_then(|id| sim.state.entities.get(id))
            .and_then(|e| e.character.as_ref())
            .is_some_and(|c| c.swimming);
        if swimming && input.just(b::JUMP) {
            if let Some(s) = g.sea.my_ship().cloned() {
                if flat(Vec3::new(s.pos.x, 0.0, s.pos.y) - feet).length() < 9.0 {
                    let c = g.content();
                    let cl = c.classes.get(&s.class).cloned().unwrap_or_default();
                    if g.land.deck.is_none() {
                        g.land.deck = Some(crate::foot::spawn_ship_deck(sim, &s, &cl));
                    }
                    let to = s.world(Vec3::new(0.0, cl.freeboard + 0.2, -cl.length * 0.25));
                    if let Some(pid) = sim.state.player {
                        sim.set_position(pid, to);
                    }
                    events.push(SimEvent::cue("climb_grab", to));
                }
            }
        }
    }
    // Last Words: the world holds its breath.
    if g.stealth.last_words.is_some() {
        return g.stealth.npcs.keys().map(|&id| (id, InputFrame::default())).collect();
    }
    perceive(g, sim, dt, events);
    brains(g, sim, dt, events)
}

/// Darts, Smoke Keg and Bung Line, or the pistols.
fn use_tool(g: &mut BlackTap, sim: &mut Sim, aiming: bool, events: &mut Vec<SimEvent>) {
    let tool = g.land.hero.tools.current();
    if !aiming {
        // A quick-shot after a short wind-up.
        if g.stealth.quick.is_none() && g.land.hero.pistols.iter().any(|t| *t <= 0.0) {
            g.stealth.quick = Some(0.6);
        }
        return;
    }
    let feet = BlackTap::player_feet(sim).unwrap_or_default();
    let eye = feet + Vec3::Y * 1.6;
    let f = g.cam.forward();
    let aim = Vec3::new(f.x, 0.0, f.y);
    let target = aim_target(g, sim, eye, aim, 25.0);
    match tool {
        Tool::Nightcap | Tool::Grog => {
            let Some(id) = target else {
                pistol(g, sim, true, events);
                return;
            };
            if !g.land.hero.tools.spend(tool) {
                g.toast("No darts left");
                return;
            }
            let at = sim.feet(id).unwrap_or_default();
            if let Some(n) = g.stealth.npcs.get_mut(&id) {
                if tool == Tool::Nightcap {
                    n.sleep = 60.0;
                } else {
                    n.drunk = 12.0;
                }
            }
            events.push(SimEvent::cue("dart_throw", at + Vec3::Y * 1.4));
            g.show(if tool == Tool::Nightcap { "nightcap_dart" } else { "grog_dart" });
        }
        Tool::Smoke => {
            if !g.land.hero.tools.spend(tool) {
                g.toast("No Smoke Kegs left");
                return;
            }
            let at = target.and_then(|id| sim.feet(id)).unwrap_or(feet + aim * 6.0);
            g.land.hero.tools.smoke.push(pav_kit::combat::Smoke { at, radius: 4.0, left: 6.0 });
            for (&id, n) in g.stealth.npcs.iter_mut() {
                if sim.feet(id).is_some_and(|p| flat(p - at).length() < 4.0) {
                    n.blind = 6.0;
                    n.mind.meter = 0.0;
                    n.mind.state = Awareness::Unaware;
                }
            }
            g.noise(at, 10.0);
            events.push(SimEvent::cue("smoke_keg", at));
            g.show("smoke_keg");
        }
        Tool::BungLine => {
            let Some(id) = target else { return };
            let Some(at) = sim.feet(id) else { return };
            // Pulled toward the player, off ledges and down.
            let to = feet + flat(at - feet).normalize_or(Vec2::X).extend(0.0).xzy() * 1.4;
            sim.set_position(id, Vec3::new(to.x, feet.y.min(at.y) + 0.1, to.z));
            if let Some(n) = g.stealth.npcs.get_mut(&id) {
                n.stun = 2.0;
            }
            events.push(SimEvent::cue("grapple", at));
            g.show("bung_line");
        }
    }
}

trait Xzy {
    fn xzy(self) -> Vec3;
}

impl Xzy for Vec3 {
    /// (x, z, _) → (x, 0, z).
    fn xzy(self) -> Vec3 {
        Vec3::new(self.x, 0.0, self.y)
    }
}

/// The best person in a 25° cone ahead within `range`, in sight.
fn aim_target(g: &BlackTap, sim: &Sim, eye: Vec3, dir: Vec3, range: f32) -> Option<EntityId> {
    let cands: Vec<(u32, Vec3)> =
        g.stealth.npcs.iter().filter(|(_, n)| !n.dead).filter_map(|(&id, _)| sim.feet(id).map(|p| (id.0, p))).collect();
    let visible = |p: Vec3| {
        let d = p + Vec3::Y - eye;
        sim.raycast(eye, d, d.length() - 0.4, None).is_none()
    };
    pav_kit::combat::pistol_lock(eye, dir, 25.0, range, &cands, &visible).map(EntityId)
}

fn pistol(g: &mut BlackTap, sim: &mut Sim, aimed: bool, events: &mut Vec<SimEvent>) {
    let Some(slot) = g.land.hero.pistols.iter().position(|t| *t <= 0.0) else { return };
    let feet = BlackTap::player_feet(sim).unwrap_or_default();
    let eye = feet + Vec3::Y * 1.5;
    let f = g.cam.forward();
    let range = if aimed { 30.0 } else { 15.0 };
    let target = aim_target(g, sim, eye, Vec3::new(f.x, 0.0, f.y), range);
    g.land.hero.pistols[slot] = 3.0;
    events.push(SimEvent::cue("pistol_fire", eye));
    g.noise(feet, noise::PISTOL);
    g.show("pistol");
    if let Some(id) = target {
        let at = sim.feet(id).unwrap_or_default() + Vec3::Y * 1.2;
        events.push(SimEvent::Hit { pos: at, strength: 0.5, target: Some(id) });
        let killed = g.land.fighters.get_mut(&id).is_some_and(|fi| !fi.vitals.dead && fi.vitals.hurt(60.0));
        if killed {
            g.land.kills += 1;
            crate::rounds::on_kill(g, sim, id, None);
        } else if let Some(n) = g.stealth.npcs.get_mut(&id) {
            n.mind.alert(feet);
        }
    }
}

/// After physics: heat, the bells, Last Words, flags from places, collectibles.
pub fn post(g: &mut BlackTap, sim: &mut Sim, dt: f32, events: &mut Vec<SimEvent>) {
    g.stealth.heat.tick(dt);
    g.stealth.since_crime += dt;
    g.stealth.tunnel_fade = (g.stealth.tunnel_fade - dt).max(0.0);
    g.stealth.alarm = (g.stealth.alarm - dt).max(0.0);
    if g.flags.remove("alarm_pending") {
        ring_alarm(g, sim);
    }
    let Some(feet) = BlackTap::player_feet(sim) else { return };
    let w = g.world();
    if g.stealth.restricted && w.area_at(flat(feet)).is_some_and(|a| a.name == "fort") {
        g.mission.set_flag("in_fort");
    }
    // Ashore (Round 4's fast-climb clock starts on the sand).
    if g.stealth.ashore_at.is_none() && g.mission.flag("left_helm") && w.ground(flat(feet)) > 0.0 && g.body == Body::Foot {
        g.stealth.ashore_at = Some(g.time);
    }
    // Last Words: a slow, white scene; then the flag.
    if let Some(t) = g.stealth.last_words {
        let t = t + dt;
        g.land.slow = 0.2;
        g.land.slow_scale = 0.5;
        let c = g.content();
        for (at, line) in [(3.5, "r6_last_words_2"), (7.5, "r6_last_words_3")] {
            if t >= at && t - dt < at {
                g.dialog.say(&c.dialog, line);
            }
        }
        if t >= 11.5 {
            g.stealth.last_words = None;
            g.mission.set_flag("last_words_done");
            g.set_atmosphere("sunset", 2.0);
            g.music = "stealth".into();
        } else {
            g.stealth.last_words = Some(t);
        }
    }
    // The cellar key from the courier's body.
    if g.flags.contains("key_dropped") && !g.mission.flag("has_key") {
        if let Some((cid, _)) = g.stealth.npcs.iter().find(|(_, n)| n.has("courier")) {
            if sim.feet(*cid).is_some_and(|c| flat(c - feet).length() < 1.6) {
                g.mission.set_flag("has_key");
                events.push(SimEvent::cue("pickpocket", feet));
            }
        }
    }
    // Collectibles on land (walk over them).
    let ship = g.sea.my_ship().filter(|_| g.body == Body::Helm).map(|s| s.pos);
    for c in &w.collectibles {
        let at = Vec3::from(c.at);
        // On foot: walk over it; at sea (floating ones): sail through it.
        let by_ship = at.y < 0.6 && ship.is_some_and(|sp| sp.distance(glam::Vec2::new(at.x, at.z)) < 12.0);
        if !g.stealth.collected.contains(&c.name) && (at.distance(feet) < 1.8 || by_ship) {
            g.stealth.collected.insert(c.name.clone());
            g.stats.collectibles += 1;
            let (cue, text, reales) = match c.kind.as_str() {
                "sheet" => ("pickup_sheet", "Shanty Sheet: a new song for the crew", 0),
                "bottle" => ("pickup_bottle", "Message in a Bottle", 0),
                "chest" => ("pickup_chest", "Treasure Chest: 250 reales", 250),
                "keg_tap" => ("pickup_tap", "Keg Tap: a Tap Blade token", 0),
                _ => ("pickup_bottle", "Found something", 0),
            };
            g.stats.reales += reales;
            if c.kind == "keg_tap" && g.stealth.collected.iter().filter(|n| n.starts_with("keg_tap")).count() >= 1 {
                g.flags.insert("double_assassination".into());
            }
            events.push(SimEvent::cue(cue, at));
            g.toast(text);
            g.show(&format!("collect_{}", c.kind));
        }
    }
    let _ = dt;
}

/// Who sees whom (`pav perception`).
pub fn perception_report(g: &BlackTap, sim: &Sim, filter: Option<&str>) -> serde_json::Value {
    let p = BlackTap::player_feet(sim).unwrap_or_default();
    let npcs: Vec<_> = g
        .stealth
        .npcs
        .iter()
        .filter(|(_, n)| filter.is_none_or(|f| n.kind.contains(f) || n.group.contains(f) || n.tags.iter().any(|t| t.contains(f))))
        .map(|(&id, n)| {
            let at = sim.feet(id).unwrap_or_default();
            json!({
                "id": id.0, "kind": n.kind, "group": n.group, "tags": n.tags,
                "state": n.mind.state, "meter": crate::game::round2(n.mind.meter),
                "distance": crate::game::round2(flat(at - p).length()),
                "sees_player": !n.out() && sees_point(sim, id, p + Vec3::Y, n.senses.far),
                "asleep": n.sleep > 0.0, "drunk": n.drunk > 0.0, "blind": n.blind > 0.0, "dead": n.dead,
                "pos": [crate::game::round2(at.x), crate::game::round2(at.y), crate::game::round2(at.z)],
            })
        })
        .collect();
    json!({
        "npcs": npcs,
        "heat": g.stealth.heat.level(),
        "heat_value": crate::game::round2(g.stealth.heat.value),
        "hidden": g.stealth.hidden,
        "concealed": g.stealth.concealed,
        "restricted": g.stealth.restricted,
        "alarm": crate::game::round2(g.stealth.alarm),
        "detections": g.stealth.detections,
        "bodies": g.stealth.bodies.len(),
        "posters_down": g.stealth.posters_down.len(),
    })
}

/// A scripted scene's camera (Last Words), if one plays.
pub fn cinematic_camera(g: &BlackTap, sim: &Sim) -> Option<CameraHint> {
    let t = g.stealth.last_words?;
    let drye = g.stealth.npcs.iter().find(|(_, n)| n.kind == "drye").and_then(|(&id, _)| sim.feet(id))?;
    let yaw = 30.0 + t * 6.0;
    Some(CameraHint::orbit(drye + Vec3::Y * 0.6, yaw, 18.0, 3.4 + t * 0.08, 40.0))
}

pub fn heat_level(g: &BlackTap) -> u8 {
    g.stealth.heat.level()
}

/// HUD on land: prompts, awareness arcs, the heat.
pub fn ui(g: &BlackTap, sim: &Sim, ui: &mut UiModel) {
    if g.body != Body::Foot {
        return;
    }
    if let Some((_, label, hold)) = interaction(g, sim) {
        ui.prompts.push(Prompt { action: "interact".into(), label: label.into(), hold: hold > 0.35 });
    }
    for (&id, n) in &g.stealth.npcs {
        if n.out() || n.civilian() {
            continue;
        }
        let Some(p) = sim.feet(id) else { continue };
        if n.mind.meter > 0.02 || n.mind.state != Awareness::Unaware {
            ui.awareness.push(AwarenessArc { pos: p + Vec3::Y * 2.2, fill: n.mind.meter, state: n.mind.state });
        }
    }
    if g.stealth.wheel.is_some() || g.land.hero.tools.selected != 0 || g.mode != crate::game::Mode::Title {
        let t = g.land.hero.tools.current();
        ui.hud.push(pav_core::play::HudItem::Tool { name: t.name().into(), count: g.land.hero.tools.count(t).min(99) });
    }
}

/// The tool wheel.
pub fn overlay(g: &BlackTap) -> Option<Overlay> {
    let selected = g.stealth.wheel?;
    let slots = Tool::ALL
        .iter()
        .map(|t| Tile {
            name: t.name().into(),
            desc: match t {
                Tool::Nightcap => "Puts a target to sleep for a minute. Silent.",
                Tool::Grog => "The target staggers and swings at his friends, then dozes off.",
                Tool::Smoke => "A cloud that blinds everyone inside for six seconds.",
                Tool::BungLine => "Pulls a target to you, or off a ledge.",
            }
            .into(),
            cost: format!("×{}", g.land.hero.tools.count(*t).min(99)),
            icon: format!("{t:?}").to_lowercase(),
            disabled: g.land.hero.tools.count(*t) == 0,
        })
        .collect();
    Some(Overlay::Wheel { slots, selected })
}

/// Black bars during Last Words.
pub fn letterbox(g: &BlackTap) -> f32 {
    if g.stealth.last_words.is_some() { 1.0 } else { 0.0 }
}

/// The white sea-mist of Last Words.
pub fn white(g: &BlackTap) -> f32 {
    g.stealth.last_words.map(|t| (t.min(1.0) * (11.5 - t).clamp(0.0, 1.0)) * 0.55).unwrap_or(0.0)
}

pub fn markers_for_tag(g: &BlackTap, sim: &Sim, tag: &str, ui: &mut UiModel) {
    for (&id, n) in &g.stealth.npcs {
        if n.has(tag) && !n.dead {
            if let Some(p) = sim.feet(id) {
                ui.markers.push(WorldMarker {
                    pos: p + Vec3::Y * 2.4,
                    kind: MarkerKind::Target,
                    label: String::new(),
                    bar: None,
                    distance: true,
                });
            }
        }
    }
}

pub fn markers_for_npc(g: &BlackTap, sim: &Sim, name: &str, ui: &mut UiModel) {
    for (&id, n) in &g.stealth.npcs {
        if (n.kind == name || n.has(name)) && !n.dead {
            if let Some(p) = sim.feet(id) {
                let label = match name {
                    "courier" => "The courier",
                    "drye" => "Commodore Drye",
                    _ => "",
                };
                ui.markers.push(WorldMarker {
                    pos: p + Vec3::Y * 2.4,
                    kind: MarkerKind::Target,
                    label: label.into(),
                    bar: None,
                    distance: true,
                });
            }
        }
    }
}

/// Predicted arcs while aiming the broadside.
pub fn draw_arcs(g: &BlackTap, out: &mut DrawList) {
    let Some(s) = g.sea.my_ship() else { return };
    let c = g.content();
    let Some(cl) = c.classes.get(&s.class) else { return };
    let Some(w) = g.sea.weapon else { return };
    let dir = match w {
        pav_kit::naval::Weapon::Broadside(pav_kit::naval::Side::Port) => -s.right(),
        pav_kit::naval::Weapon::Broadside(pav_kit::naval::Side::Starboard) => s.right(),
        pav_kit::naval::Weapon::Chain => s.forward(),
        _ => return,
    };
    let el: f32 = 0.06;
    let from = s.world(Vec3::new(0.0, cl.freeboard * 0.6, 0.0)) + Vec3::new(dir.x, 0.0, dir.y) * cl.beam * 0.5;
    let vel = Vec3::new(dir.x * el.cos(), el.sin(), dir.y * el.cos()) * cl.muzzle;
    let pts = pav_kit::ballistics::arc(from, vel, 0.12, 3.0, 0.0);
    for p in pts.windows(2) {
        out.push(DrawItem::Line { a: p[0], b: p[1], radius: 0.12, color: Color::hex("#e2b04a"), emissive: 1.5 });
    }
    if let Some(last) = pts.last() {
        out.push(DrawItem::Decal {
            center: Vec3::new(last.x, 0.1, last.z),
            radius: 5.0,
            ring: 0.85,
            color: Color::hex("#e2b04a"),
            alpha: 0.9,
        });
    }
}

/// Gull Sight colours: (enemy, target, ally, neutral) for the colour-blind setting.
pub fn gull_palette(mode: &str) -> [&'static str; 4] {
    match mode {
        "protanopia" | "deuteranopia" => ["#f2a33a", "#ffe066", "#4aa8ff", "#e8e8f0"],
        "tritanopia" => ["#ff4d6d", "#ffd2e0", "#3fd0c9", "#e8e8f0"],
        _ => ["#e0452f", "#ffcc33", "#4aa8ff", "#e8e8f0"],
    }
}

/// Land things to draw: Gull Sight highlights, smoke, posters, the Tap flag, hiding props,
/// collectibles' glints, the last-known ghost.
pub fn draw(g: &BlackTap, sim: &Sim, out: &mut DrawList) {
    let pal = gull_palette(&g.options.colorblind);
    if g.gull {
        for (&id, n) in &g.stealth.npcs {
            if n.dead {
                continue;
            }
            let c = if n.kind == "drye" || n.has("courier") {
                pal[1]
            } else if n.civilian() {
                pal[3]
            } else {
                pal[0]
            };
            out.push(DrawItem::Highlight { entity: id, color: Color::hex(c) });
        }
        for (&id, f) in &g.land.fighters {
            if f.team == Team::Ally && !f.vitals.dead {
                out.push(DrawItem::Highlight { entity: id, color: Color::hex(pal[2]) });
            }
        }
    }
    for s in &g.land.hero.tools.smoke {
        let k = (s.left / 6.0).clamp(0.0, 1.0);
        for i in 0..6 {
            let a = i as f32 * 1.05 + g.time * 0.3;
            let p = s.at + Vec3::new(a.cos() * s.radius * 0.5, 0.8 + (i % 3) as f32 * 0.6, a.sin() * s.radius * 0.5);
            out.push(DrawItem::Prim {
                shape: Shape::Sphere { radius: s.radius * 0.55 * (0.6 + 0.4 * k) },
                pos: p,
                rot: Quat::IDENTITY,
                color: Color::hex("#d8d4cc"),
                emissive: 0.1,
                look: pav_core::Look::Lit,
            });
        }
    }
    let w = g.world();
    for (i, p) in w.posters.iter().enumerate() {
        if !g.stealth.posters_down.contains(&i) {
            out.push(DrawItem::Prim {
                shape: Shape::Box { half: Vec3::new(0.35, 0.45, 0.02) },
                pos: *p + Vec3::Y * 1.6,
                rot: Quat::IDENTITY,
                color: Color::hex("#e8d9b0"),
                emissive: if g.gull { 0.8 } else { 0.0 },
                look: pav_core::Look::Lit,
            });
        }
    }
    for c in &w.collectibles {
        if g.stealth.collected.contains(&c.name) {
            continue;
        }
        let at = Vec3::from(c.at) + Vec3::Y * (0.5 + (g.time * 2.0).sin() * 0.08);
        let (shape, color) = match c.kind.as_str() {
            "chest" => (Shape::Box { half: Vec3::new(0.45, 0.3, 0.3) }, "#8a5a2a"),
            "keg_tap" => (Shape::Cylinder { radius: 0.25, half_height: 0.3 }, "#c9a24a"),
            "sheet" => (Shape::Box { half: Vec3::new(0.25, 0.02, 0.32) }, "#f2ead0"),
            _ => (Shape::Cylinder { radius: 0.09, half_height: 0.25 }, "#6fae8f"),
        };
        out.push(DrawItem::Prim {
            shape,
            pos: at,
            rot: Quat::from_rotation_y(g.time),
            color: Color::hex(color),
            emissive: 0.6,
            look: pav_core::Look::Lit,
        });
        out.push(DrawItem::Light { pos: at + Vec3::Y * 0.6, color: Color::hex("#ffd27a"), intensity: 1.2, radius: 3.0 });
    }
    if g.stealth.flag_raised || g.flags.contains("fort_captured") {
        if let Some(fp) = w.marker("flagpole") {
            let wave = (g.time * 3.0).sin() * 0.15;
            out.push(DrawItem::Prim {
                shape: Shape::Box { half: Vec3::new(1.1, 0.7, 0.03) },
                pos: fp + Vec3::new(1.2, 0.0, 0.0),
                rot: Quat::from_rotation_y(wave),
                color: Color::hex("#111111"),
                emissive: 0.0,
                look: pav_core::Look::Lit,
            });
            out.push(DrawItem::Prim {
                shape: Shape::Cylinder { radius: 0.32, half_height: 0.02 },
                pos: fp + Vec3::new(1.2, 0.0, 0.05),
                rot: Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
                color: Color::hex("#c9a24a"),
                emissive: 0.4,
                look: pav_core::Look::Lit,
            });
        }
    }
    // The last place a searching guard saw the player: a ghost.
    if let Some(at) = g.stealth.npcs.values().filter(|n| n.mind.state == Awareness::Searching).find_map(|n| n.mind.last_known) {
        out.push(DrawItem::Prim {
            shape: Shape::Capsule { radius: 0.3, half_height: 0.6 },
            pos: at + Vec3::Y * 0.9,
            rot: Quat::IDENTITY,
            color: Color::hex("#9fd3ff"),
            emissive: 0.6,
            look: pav_core::Look::Unlit,
        });
    }
    let _ = sim;
}

pub fn music_intensity(g: &BlackTap) -> f32 {
    let aware = g.stealth.npcs.values().filter(|n| !n.out() && n.mind.aware()).count();
    let wary = g.stealth.npcs.values().filter(|n| !n.out() && n.mind.state != Awareness::Unaware).count();
    if aware > 0 {
        1.0
    } else if wary > 0 {
        0.75
    } else {
        0.5
    }
}

pub fn town_level(g: &BlackTap, sim: &Sim) -> f32 {
    let Some(p) = BlackTap::player_feet(sim) else { return 0.0 };
    let t = g.world().marker("dryport").unwrap_or_default();
    (1.0 - flat(p - t).length() / 90.0).clamp(0.0, 1.0)
}

pub fn jungle_level(g: &BlackTap, sim: &Sim) -> f32 {
    let Some(p) = BlackTap::player_feet(sim) else { return 0.0 };
    let w = g.world();
    let on_island = w.ground(flat(p)) > 2.0;
    if on_island && town_level(g, sim) < 0.2 { 1.0 } else { 0.0 }
}

/// The bot on land (`bot.rs` has the skills).
pub fn bot(g: &BlackTap, sim: &Sim, skill: &str) -> InputFrame {
    crate::bot::land(g, sim, skill)
}
