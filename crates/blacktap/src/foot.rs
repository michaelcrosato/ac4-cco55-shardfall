//! On land and on decks: the player on foot (walking, swimming, fighting), NPC fighters with
//! attack tokens and telegraphed strikes, counters, dodges, assassinations, and boarding
//! actions (swing across, a deck arena, surrender, plunder). Traversal and stealth build on
//! this in `traverse.rs` and `stealth.rs`.

use std::collections::BTreeMap;

use glam::{Quat, Vec2, Vec3};
use pav_core::entity::{BodyKind, EntityId, Spawn};
use pav_core::frame::SimEvent;
use pav_core::input::{InputFrame, buttons};
use pav_core::moves::MoveId;
use pav_core::play::{Overlay, Tile};
use pav_core::puppet::{BodyPlan, PuppetDef};
use pav_core::sim::Sim;
use pav_core::{Color, Shape, Visual};
use pav_kit::combat::{self, Assassin, CounterWindow, Mark, StrikeOutcome, Swing, Tokens, Vitals};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::content::Content;
use crate::game::{BlackTap, Body, b};
use crate::world::World;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Team {
    Ally,
    Enemy,
}

/// An NPC who fights (boarders, guards in a brawl).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Fighter {
    pub team: Team,
    pub kind: String,
    pub vitals: Vitals,
    pub profile: combat::CombatProfile,
    pub swing: Option<Swing>,
    pub cooldown: f32,
    pub target: Option<EntityId>,
    /// Which way it circles while waiting for a token.
    pub circle: f32,
    pub surrendered: bool,
    /// Lunging toward its target to attack (a few tenths of a second of movement).
    pub step: f32,
    /// Not fighting: its brain (`stealth.rs`) walks it about until it is alerted.
    #[serde(default)]
    pub idle: bool,
}

/// A boarding action in progress.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Boarding {
    pub prize: u32,
    pub phase: BoardPhase,
    pub t: f32,
    /// Kinematic deck colliders (player's ship, prize).
    pub decks: Vec<EntityId>,
    pub won: bool,
    /// Plunder overlay focus and stage (0 = repair/cargo, 1 = upgrade).
    pub focus: usize,
    pub stage: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BoardPhase {
    /// Ships drawing alongside, the swing across.
    Swing,
    Fight,
    Plunder,
}

/// The player's own fighting state.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Hero {
    pub vitals: Vitals,
    /// Seconds since △ was pressed (counters), and since the last light attack.
    pub counter_ago: Option<f32>,
    pub since_attack: f32,
    pub combo: usize,
    /// An attack swinging: (target, seconds until it lands, damage, move).
    pub attack: Option<(EntityId, f32, f32, String)>,
    /// Pistols: seconds until each is loaded again.
    pub pistols: [f32; 2],
    pub grog: u32,
    pub tools: combat::Tools,
}

impl Default for Hero {
    fn default() -> Self {
        Self {
            vitals: Vitals::new(100.0),
            counter_ago: None,
            since_attack: 9.0,
            combo: 0,
            attack: None,
            pistols: [0.0; 2],
            grog: 3,
            tools: combat::Tools::full(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Land {
    pub hero: Hero,
    pub fighters: BTreeMap<EntityId, Fighter>,
    pub tokens: Tokens,
    pub boarding: Option<Boarding>,
    /// Slow motion (finishers, Last Words): seconds left and scale.
    pub slow: f32,
    pub slow_scale: f32,
    /// Counter-kills in the current fight (optional objectives).
    pub counters: u32,
    pub kills: u32,
    /// Seconds since the player was last swimming / on a ship deck.
    pub swim_time: f32,
    /// The Black Tap's deck collider while the player walks on it.
    pub deck: Option<EntityId>,
    /// Free-run held this tick (a landing then rolls).
    #[serde(default)]
    pub sprinting: bool,
}

impl Land {
    pub fn boarding_active(&self) -> bool {
        self.boarding.is_some()
    }

    pub fn time_scale(&self) -> f32 {
        if self.slow > 0.0 { self.slow_scale.clamp(0.1, 1.0) } else { 1.0 }
    }

    /// The player character (spawned once per scene; the helm seats it).
    pub fn spawn_player(&mut self, sim: &mut Sim, world: &World) {
        let p = world.marker("r1_start").unwrap_or(Vec3::ZERO);
        sim.state.spawn = p + Vec3::Y * 4.0;
        let id = sim.spawn_player();
        if let Some(ch) = sim.state.entities.get_mut(id).and_then(|e| e.character.as_mut()) {
            ch.puppet = Some(std::sync::Arc::new(crate::looks::captain()));
        }
        self.tokens = Tokens::new(2);
    }

    /// The enemies still standing in the current fight.
    pub fn enemies_left(&self) -> usize {
        self.fighters.values().filter(|f| f.team == Team::Enemy && !f.vitals.dead && !f.surrendered && !f.idle).count()
    }

    pub(crate) fn spawn_fighter(
        &mut self,
        sim: &mut Sim,
        kind: &str,
        team: Team,
        feet: Vec3,
        facing: f32,
        look: PuppetDef,
    ) -> EntityId {
        let profile = crate::looks::profile(kind);
        let id = sim.spawn_npc(kind, feet, facing, look, None, None);
        self.fighters.insert(
            id,
            Fighter {
                team,
                kind: kind.to_string(),
                vitals: Vitals::new(profile.hp),
                profile,
                swing: None,
                cooldown: 1.0 + (id.0 % 7) as f32 * 0.3,
                target: None,
                circle: if id.0.is_multiple_of(2) { 1.0 } else { -1.0 },
                surrendered: false,
                step: 0.0,
                idle: false,
            },
        );
        id
    }

    pub fn status(&self) -> serde_json::Value {
        json!({
            "hp": self.hero.vitals.hp,
            "fighters": self.fighters.len(),
            "enemies_left": self.enemies_left(),
            "boarding": self.boarding.as_ref().map(|b| json!({"phase": b.phase, "won": b.won})),
            "counters": self.counters,
            "kills": self.kills,
        })
    }
}

/// The static world for every scene (islands, town, fort — Phase 2 generators).
pub fn build_world(sim: &mut Sim, world: &World, content: &Content) -> anyhow::Result<()> {
    crate::island::build(sim, world, content)?;
    crate::paths::build(sim, &world.geometry);
    Ok(())
}

/// A kinematic deck slab that walkers can stand on (no visual: the ship mesh is drawn).
fn spawn_deck(sim: &mut Sim, at: Vec3, heading: f32, half: Vec3) -> EntityId {
    let h = heading.to_radians();
    let mut s = Spawn::new("~deck", at).visual(Visual::new(Shape::Box { half }, Color::hex("#a07b52"))).body(BodyKind::Kinematic);
    s.rot = Quat::from_rotation_y(std::f32::consts::PI - h);
    let id = sim.spawn(s);
    if let Some(e) = sim.state.entities.get_mut(id) {
        e.visual = None;
    }
    id
}

/// The Black Tap's walkable deck (at anchor).
pub fn spawn_ship_deck(sim: &mut Sim, ship: &pav_kit::naval::Vessel, class: &pav_kit::naval::HullClass) -> EntityId {
    let at = ship.world(Vec3::new(0.0, class.freeboard - 0.3, 0.0));
    spawn_deck(sim, at, ship.heading, Vec3::new(class.beam * 0.42, 0.3, class.length * 0.42))
}

fn flat(v: Vec3) -> Vec2 {
    Vec2::new(v.x, v.z)
}

fn facing_to(from: Vec3, to: Vec3) -> f32 {
    let d = to - from;
    d.x.atan2(d.z)
}

/// Starts a boarding action against a disabled ship (called when the player presses Board).
pub fn start_boarding(g: &mut BlackTap, _sim: &mut Sim, prize: u32, events: &mut Vec<SimEvent>) {
    if g.land.boarding.is_some() {
        return;
    }
    let me = g.sea.mine;
    let c = g.content();
    let (Some(mine), Some(target)) = (g.sea.ship(me).cloned(), g.sea.ship(prize).cloned()) else { return };
    let mc = c.classes.get(&mine.class).cloned().unwrap_or_default();
    let tc = c.classes.get(&target.class).cloned().unwrap_or_default();
    // Draw the prize alongside: parallel, beam to beam with a 4 m gap.
    let side = if (target.pos - mine.pos).dot(mine.right()) >= 0.0 { 1.0 } else { -1.0 };
    let alongside = mine.pos + mine.right() * side * (mc.beam * 0.5 + tc.beam * 0.5 + 4.0);
    if let Some(t) = g.sea.ship_mut(prize) {
        t.pos = alongside;
        t.heading = mine.heading;
        t.speed = 0.0;
        t.captain = None;
    }
    if let Some(m) = g.sea.ship_mut(me) {
        m.speed = 0.0;
        m.sail = 0;
    }
    g.land.boarding =
        Some(Boarding { prize, phase: BoardPhase::Swing, t: 0.0, decks: Vec::new(), won: false, focus: 0, stage: 0 });
    events.push(SimEvent::cue("grapple", mine.world(Vec3::Y * 4.0)));
    g.shake(0.3, (0.6, 0.3, 250));
    g.show("boarding");
    let _ = tc;
}

fn deck_top(g: &BlackTap, ship: u32) -> Option<(Vec3, f32, Vec3)> {
    let s = g.sea.ship(ship)?;
    let c = g.content().classes.get(&s.class).cloned().unwrap_or_default();
    Some((s.world(Vec3::new(0.0, c.freeboard - 0.3, 0.0)), s.heading, Vec3::new(c.beam * 0.42, 0.3, c.length * 0.4)))
}

/// Begins the deck fight: colliders, the player on the prize's deck, boarders and crew.
fn begin_fight(g: &mut BlackTap, sim: &mut Sim) {
    let me = g.sea.mine;
    let Some(bd) = g.land.boarding.clone() else { return };
    let mut decks = Vec::new();
    for id in [me, bd.prize] {
        if let Some((at, heading, half)) = deck_top(g, id) {
            decks.push(spawn_deck(sim, at, heading, half));
        }
    }
    let Some((prize_deck, heading, half)) = deck_top(g, bd.prize) else { return };
    let h = heading.to_radians();
    let fwd = Vec3::new(h.sin(), 0.0, -h.cos());
    let right = Vec3::new(-fwd.z, 0.0, fwd.x);
    let top = prize_deck.y + 0.3;
    let at =
        |along: f32, across: f32| Vec3::new(prize_deck.x, top, prize_deck.z) + fwd * along * half.z + right * across * half.x;
    // The player lands amidships.
    g.body = Body::Foot;
    if let Some(pid) = sim.state.player {
        sim.set_position(pid, at(0.0, 0.0));
    }
    // Six crew, an officer and Captain Sobers; four of our crew swing across too.
    let enemies = [
        ("crew", 0.6, -0.5),
        ("crew", 0.6, 0.5),
        ("crew", -0.6, -0.5),
        ("crew", -0.6, 0.5),
        ("crew", 0.3, 0.6),
        ("crew", -0.3, -0.6),
        ("officer", 0.8, 0.0),
        ("sobers", -0.8, 0.0),
    ];
    let player_at = at(0.0, 0.0);
    for (k, (kind, along, across)) in enemies.into_iter().enumerate() {
        let p = at(along, across);
        let look = crate::looks::look_variant(kind, k as u32);
        g.land.spawn_fighter(sim, kind, crate::foot::Team::Enemy, p, facing_to(p, player_at), look);
    }
    for (k, (along, across)) in [(0.2, -0.3), (-0.2, 0.3), (0.45, 0.0), (-0.45, 0.0)].into_iter().enumerate() {
        let p = at(along, across);
        let look = crate::looks::look_variant("ally", k as u32);
        g.land.spawn_fighter(sim, "ally", crate::foot::Team::Ally, p, facing_to(p, player_at), look);
    }
    if let Some(b) = &mut g.land.boarding {
        b.phase = BoardPhase::Fight;
        b.t = 0.0;
        b.decks = decks;
    }
    g.mission.set_flag("boarded");
    g.land.counters = 0;
}

/// Ends a boarding action: the player back at the helm, the prize scuttled.
pub fn end_boarding(g: &mut BlackTap, sim: &mut Sim) {
    let Some(bd) = g.land.boarding.take() else { return };
    for id in bd.decks {
        sim.despawn(id);
    }
    let ids: Vec<EntityId> = g.land.fighters.keys().copied().collect();
    for id in ids {
        sim.despawn(id);
    }
    g.land.fighters.clear();
    g.land.tokens = Tokens::new(2);
    if let Some(p) = g.sea.ship_mut(bd.prize) {
        p.hull = 0.0;
        p.sinking = 0.001;
    }
    g.body = Body::Helm;
}

/// The plunder overlay's tiles for its stage.
pub fn plunder_tiles(stage: u8) -> (String, String, Vec<Tile>) {
    if stage == 0 {
        (
            "PLUNDER".into(),
            "The Restraint's hold is yours".into(),
            vec![
                Tile {
                    name: "Repair the hull".into(),
                    desc: "Patch the Black Tap to full".into(),
                    cost: "".into(),
                    icon: "repair".into(),
                    disabled: false,
                },
                Tile {
                    name: "Take the cargo".into(),
                    desc: "Timber, iron and 300 reales".into(),
                    cost: "".into(),
                    icon: "cargo".into(),
                    disabled: false,
                },
            ],
        )
    } else {
        (
            "SHIPWRIGHT".into(),
            "Choose one upgrade".into(),
            vec![
                Tile {
                    name: "Reinforced Hull".into(),
                    desc: "+300 hull".into(),
                    cost: "free".into(),
                    icon: "hull".into(),
                    disabled: false,
                },
                Tile {
                    name: "Heavy Shot".into(),
                    desc: "+35% cannon damage".into(),
                    cost: "free".into(),
                    icon: "shot".into(),
                    disabled: false,
                },
                Tile {
                    name: "Swift Rigging".into(),
                    desc: "+12% speed".into(),
                    cost: "free".into(),
                    icon: "sail".into(),
                    disabled: false,
                },
                Tile {
                    name: "More Kegs".into(),
                    desc: "+3 powder kegs".into(),
                    cost: "free".into(),
                    icon: "keg".into(),
                    disabled: false,
                },
            ],
        )
    }
}

/// The overlay to draw while boarding (plunder tiles).
pub fn overlay(g: &BlackTap) -> Option<Overlay> {
    let bd = g.land.boarding.as_ref()?;
    if bd.phase != BoardPhase::Plunder {
        return None;
    }
    let (title, subtitle, tiles) = plunder_tiles(bd.stage);
    Some(Overlay::Tiles { title, subtitle, tiles, focus: bd.focus })
}

fn apply_choice(g: &mut BlackTap, stage: u8, choice: usize) {
    let me = g.sea.mine;
    if stage == 0 {
        match choice {
            0 => {
                if let Some(s) = g.sea.ship_mut(me) {
                    s.hull = s.hull_max;
                    s.fire = 0.0;
                }
            }
            _ => {
                g.sea.timber += 4;
                g.sea.iron += 3;
                g.stats.reales += 300;
            }
        }
    } else {
        let up = ["reinforced_hull", "heavy_shot", "swift_rigging", "more_kegs"][choice.min(3)];
        crate::rounds::apply_upgrade(g, up);
    }
}

/// One tick on foot (before characters move): the player's moves and every fighter's.
pub fn pre(
    g: &mut BlackTap,
    sim: &mut Sim,
    input: &InputFrame,
    playing: bool,
    dt: f32,
    events: &mut Vec<SimEvent>,
) -> BTreeMap<EntityId, InputFrame> {
    let mut out = BTreeMap::new();
    let Some(pid) = sim.state.player else { return out };
    // Boarding: the swing, then the fight, then the plunder overlay.
    if let Some(bd) = g.land.boarding.clone() {
        match bd.phase {
            BoardPhase::Swing => {
                if let Some(b) = &mut g.land.boarding {
                    b.t += dt;
                }
                // Swing from the helm to the prize's deck on a rope.
                let from = g.sea.my_ship().map(|s| s.world(Vec3::Y * 4.0)).unwrap_or_default();
                let to = deck_top(g, bd.prize).map(|d| d.0 + Vec3::Y * 0.4).unwrap_or(from);
                let t = (bd.t / 2.0).clamp(0.0, 1.0);
                let p = from.lerp(to, t) + Vec3::Y * (t * std::f32::consts::PI).sin() * 3.5;
                if let Some(ch) = sim.state.entities.get_mut(pid).and_then(|e| e.character.as_mut()) {
                    ch.scripted = Some(pav_core::character::Scripted {
                        feet: p,
                        facing: facing_to(from, to),
                        vel: (to - from) / 2.0,
                        pose: pav_core::character::ScriptPose::Air,
                        climb_step: 0.0,
                    });
                }
                out.insert(pid, InputFrame::default());
                if bd.t >= 2.0 {
                    begin_fight(g, sim);
                }
                return out;
            }
            BoardPhase::Plunder => {
                out.insert(pid, InputFrame::default());
                let n = plunder_tiles(bd.stage).2.len();
                let f = g.cam.forward();
                let r = Vec2::new(-f.y, f.x);
                let x = input.move_dir.dot(r);
                let mut focus = bd.focus;
                if x > 0.6 && g.stick_prev <= 0.6 {
                    focus = (focus + 1) % n;
                } else if x < -0.6 && g.stick_prev >= -0.6 {
                    focus = (focus + n - 1) % n;
                }
                g.stick_prev = x;
                if let Some(b) = &mut g.land.boarding {
                    b.focus = focus;
                }
                if input.just(b::JUMP) || input.just(b::INTERACT) {
                    apply_choice(g, bd.stage, focus);
                    events.push(SimEvent::cue("ui_confirm", sim.feet(pid).unwrap_or_default()));
                    if bd.stage == 0 {
                        if let Some(b) = &mut g.land.boarding {
                            b.stage = 1;
                            b.focus = 0;
                        }
                    } else {
                        g.mission.set_flag("plunder_done");
                    }
                }
                return out;
            }
            BoardPhase::Fight => {}
        }
    }
    let feet = sim.feet(pid).unwrap_or_default();
    let mut me = InputFrame { move_dir: input.move_dir, ..Default::default() };
    if !playing || g.land.hero.vitals.dead {
        me.move_dir = Vec2::ZERO;
        out.insert(pid, me);
        return out;
    }
    // Free-run and jump pass through to the controller.
    if input.down(b::JUMP) {
        me.held |= buttons::JUMP;
    }
    if input.just(b::JUMP) {
        me.pressed |= buttons::JUMP;
    }
    if input.down(b::CROUCH) {
        me.held |= buttons::CROUCH;
    }
    // Free-run (R1) and Parkour Down / dive (◯) drive the traversal rules.
    g.land.sprinting = input.down(b::SPRINT);
    if g.land.sprinting {
        me.held |= buttons::SPRINT;
    }
    if input.down(b::DROP) {
        me.held |= buttons::DODGE;
    }
    if input.just(b::DROP) && g.land.enemies_left() == 0 {
        me.pressed |= buttons::DODGE;
    }
    // ---- fighting
    let hero = &mut g.land.hero;
    hero.since_attack += dt;
    if let Some(a) = &mut hero.counter_ago {
        *a += dt;
        if *a > 0.6 {
            hero.counter_ago = None;
        }
    }
    if input.just(b::INTERACT) {
        hero.counter_ago = Some(0.0);
    }
    // Dodge (◯ in a fight): a quick dash with i-frames.
    let in_fight = g.land.enemies_left() > 0;
    if in_fight && input.just(b::DROP) {
        g.land.hero.vitals.iframes = 0.3;
        let dir = if input.move_dir.length() > 0.2 { input.move_dir.normalize() } else { -g.cam.forward() };
        if let Some(ch) = sim.state.entities.get_mut(pid).and_then(|e| e.character.as_mut()) {
            ch.dash_vel = Vec3::new(dir.x, 0.0, dir.y) * 9.0;
            ch.dash_time = 0.25;
            ch.dash_roll = true;
        }
        events.push(SimEvent::cue("land_roll", feet));
    }
    // Attack (□): an assassination if one is possible, else the light chain.
    if input.just(b::ATTACK) && g.land.hero.attack.is_none() {
        let target = best_target(g, sim, feet);
        if let Some((tid, tfeet, kind)) = target {
            match kind {
                Some(k) => {
                    assassinate(g, sim, tid, k, events);
                    let _ = tfeet;
                }
                None => {
                    let hero = &mut g.land.hero;
                    hero.combo = if hero.since_attack < 0.9 { (hero.combo + 1) % 3 } else { 0 };
                    hero.since_attack = 0.0;
                    let (dmg, mv) = combat::LIGHT_CHAIN[hero.combo];
                    hero.attack = Some((tid, 0.22, dmg, mv.to_string()));
                    if let Some(ch) = sim.state.entities.get_mut(pid).and_then(|e| e.character.as_mut()) {
                        ch.face = Some(facing_to(feet, tfeet));
                    }
                    events.push(SimEvent::cue("sword_swing", feet + Vec3::Y));
                }
            }
        } else {
            events.push(SimEvent::cue("sword_swing", feet + Vec3::Y));
            g.land.hero.attack = Some((pid, 0.22, 0.0, "slash".into()));
        }
    }
    // The swing lands.
    if let Some((tid, left, dmg, mv)) = g.land.hero.attack.clone() {
        let left = left - dt;
        let progress = 1.0 - (left / 0.32).clamp(0.0, 1.0);
        if let Some(ch) = sim.state.entities.get_mut(pid).and_then(|e| e.character.as_mut()) {
            ch.anim.set_action(MoveId::of(&mv), progress, 0.7, if g.land.hero.combo.is_multiple_of(2) { 1.0 } else { -1.0 });
        }
        if left <= 0.0 {
            g.land.hero.attack = None;
            if let Some(ch) = sim.state.entities.get_mut(pid).and_then(|e| e.character.as_mut()) {
                ch.anim.set_action(MoveId::NONE, 0.0, 0.0, 1.0);
                ch.face = None;
            }
            if tid != pid && dmg > 0.0 {
                hit_fighter(g, sim, tid, dmg, events);
            }
        } else {
            g.land.hero.attack = Some((tid, left, dmg, mv));
        }
    }
    // Grog ration (D-pad ↑).
    if input.just(b::HEAL) && g.land.hero.grog > 0 && g.land.hero.vitals.hp < g.land.hero.vitals.max {
        let h = &mut g.land.hero;
        h.grog -= 1;
        h.vitals.hp = (h.vitals.hp + h.vitals.max * 0.5).min(h.vitals.max);
        events.push(SimEvent::cue("pickup_bottle", feet));
    }
    out.insert(pid, me);
    // ---- fighters
    out.extend(fighters(g, sim, dt, events));
    out
}

/// The fighter to strike: an assassination target first, else the nearest enemy in front.
fn best_target(g: &BlackTap, sim: &Sim, feet: Vec3) -> Option<(EntityId, Vec3, Option<combat::Assassination>)> {
    let falling = sim
        .state
        .player
        .and_then(|id| sim.state.entities.get(id))
        .and_then(|e| e.character.as_ref())
        .is_some_and(|c| !c.grounded && c.vel.y < -1.0);
    let ch = sim.state.player.and_then(|id| sim.state.entities.get(id)).and_then(|e| e.character.as_ref());
    let hanging = ch.is_some_and(|c| c.hang.is_some());
    let a = Assassin {
        feet,
        falling,
        on_ledge: ch.is_some_and(|c| c.grounded) && sim.raycast(feet + Vec3::Y * 0.3, Vec3::NEG_Y, 1.8, None).is_none(),
        hanging,
        hidden: g.stealth.hidden || g.stealth.concealed,
        double_unlocked: g.flags.contains("double_assassination"),
    };
    let mut best: Option<(f32, EntityId, Vec3, Option<combat::Assassination>)> = None;
    for (&id, f) in &g.land.fighters {
        if f.team != Team::Enemy || f.vitals.dead || f.surrendered {
            continue;
        }
        let Some(tf) = sim.feet(id) else { continue };
        let facing = sim.state.entities.get(id).and_then(|e| e.character.as_ref()).map(|c| c.facing).unwrap_or(0.0);
        let (alerted, unaware) = match g.stealth.npcs.get(&id) {
            Some(n) => (
                n.mind.aware() && n.blind <= 0.0 && n.sleep <= 0.0,
                matches!(n.mind.state, pav_core::play::Awareness::Unaware) || n.out() || n.blind > 0.0 || n.drunk > 0.0,
            ),
            None => (f.target.is_some(), f.target.is_none()),
        };
        let mark = Mark { id: id.0, feet: tf, facing, alerted, unaware };
        let kind = combat::assassination(&a, &mark);
        let d = flat(tf - feet).length();
        if d > 3.2 && kind.is_none() {
            continue;
        }
        let score = d - if kind.is_some() { 10.0 } else { 0.0 };
        if best.as_ref().is_none_or(|b| score < b.0) {
            best = Some((score, id, tf, kind));
        }
    }
    best.map(|b| (b.1, b.2, b.3))
}

/// A Tap Blade kill.
pub fn assassinate(g: &mut BlackTap, sim: &mut Sim, id: EntityId, kind: combat::Assassination, events: &mut Vec<SimEvent>) {
    let at = sim.feet(id).unwrap_or_default();
    if let Some(f) = g.land.fighters.get_mut(&id) {
        f.vitals.hurt(9999.0);
        f.swing = None;
    }
    g.land.tokens.give_back(id.0);
    g.land.kills += 1;
    g.stats.assassinations += 1;
    g.land.slow = 0.35;
    g.land.slow_scale = 0.35;
    events.push(SimEvent::cue("assassinate", at + Vec3::Y));
    g.shake(0.35, (0.8, 0.6, 140));
    g.show(&format!("assassinate_{}", kind.name()));
    if let Some(pid) = sim.state.player {
        if let Some(ch) = sim.state.entities.get_mut(pid).and_then(|e| e.character.as_mut()) {
            ch.anim.set_action(MoveId::of(if kind == combat::Assassination::Air { "plunge" } else { "thrust" }), 0.6, 0.6, 1.0);
        }
    }
    crate::rounds::on_kill(g, sim, id, Some(kind));
}

fn hit_fighter(g: &mut BlackTap, sim: &mut Sim, id: EntityId, dmg: f32, events: &mut Vec<SimEvent>) {
    let at = sim.feet(id).unwrap_or_default() + Vec3::Y * 1.1;
    let Some(f) = g.land.fighters.get_mut(&id) else { return };
    if f.vitals.dead {
        return;
    }
    // Brutes shrug off light attacks unless staggered.
    let dmg = if f.profile.blocks_light && f.vitals.stagger <= 0.0 { dmg * 0.25 } else { dmg };
    let killed = f.vitals.hurt(dmg);
    f.swing = None;
    f.cooldown = f.cooldown.max(0.6);
    events.push(SimEvent::cue(if dmg < 8.0 { "sword_clash" } else { "sword_hit" }, at));
    events.push(SimEvent::Hit { pos: at, strength: 0.4, target: Some(id) });
    g.shake(0.15, (0.2, 0.5, 70));
    if killed {
        g.land.tokens.give_back(id.0);
        g.land.kills += 1;
        g.land.slow = 0.14;
        g.land.slow_scale = 0.3;
        g.show("finisher");
        crate::rounds::on_kill(g, sim, id, None);
    }
}

/// NPC fighters: choose targets, close in, wait for a token, telegraph and strike.
fn fighters(g: &mut BlackTap, sim: &mut Sim, dt: f32, events: &mut Vec<SimEvent>) -> BTreeMap<EntityId, InputFrame> {
    let mut out = BTreeMap::new();
    if g.stealth.last_words.is_some() {
        for f in g.land.fighters.values_mut() {
            f.swing = None;
        }
        return out;
    }
    let Some(pid) = sim.state.player else { return out };
    let player_feet = sim.feet(pid).unwrap_or_default();
    let ids: Vec<EntityId> = g.land.fighters.keys().copied().collect();
    let positions: BTreeMap<EntityId, Vec3> = ids.iter().filter_map(|&id| sim.feet(id).map(|p| (id, p))).collect();
    let window = CounterWindow {
        early: 0.35
            * if g.options.difficulty < 0.8 {
                1.4
            } else if g.options.difficulty > 1.2 {
                0.8
            } else {
                1.0
            },
        late: 0.10,
    };
    for id in ids {
        let Some(mut f) = g.land.fighters.get(&id).cloned() else { continue };
        let Some(&me) = positions.get(&id) else { continue };
        f.vitals.tick(dt, 0.0, 0.0);
        f.cooldown = (f.cooldown - dt).max(0.0);
        let mut inp = InputFrame::default();
        if f.vitals.dead {
            if let Some(ch) = sim.state.entities.get_mut(id).and_then(|e| e.character.as_mut()) {
                ch.anim.set_action(MoveId::NONE, 0.0, 0.0, 1.0);
                ch.anim.down = (f.vitals.death_t / 0.9).min(1.0);
            }
            g.land.fighters.insert(id, f);
            out.insert(id, inp);
            continue;
        }
        if f.idle {
            // The brain in `stealth.rs` moves it.
            g.land.fighters.insert(id, f);
            continue;
        }
        if f.surrendered || f.vitals.helpless() {
            g.land.fighters.insert(id, f);
            out.insert(id, inp);
            continue;
        }
        // Targets: enemies go for the player (and allies near them), allies for enemies.
        let target = match f.team {
            Team::Enemy => {
                let ally = g
                    .land
                    .fighters
                    .iter()
                    .filter(|(_, o)| o.team == Team::Ally && !o.vitals.dead)
                    .filter_map(|(&oid, _)| positions.get(&oid).map(|p| (oid, *p)))
                    .filter(|(_, p)| flat(*p - me).length() < 2.5)
                    .min_by(|a, b| flat(a.1 - me).length().total_cmp(&flat(b.1 - me).length()));
                if flat(player_feet - me).length() < 7.0 || ally.is_none() { Some((pid, player_feet)) } else { ally }
            }
            Team::Ally => g
                .land
                .fighters
                .iter()
                .filter(|(_, o)| o.team == Team::Enemy && !o.vitals.dead && !o.surrendered)
                .filter_map(|(&oid, _)| positions.get(&oid).map(|p| (oid, *p)))
                .min_by(|a, b| flat(a.1 - me).length().total_cmp(&flat(b.1 - me).length())),
        };
        let Some((tid, tp)) = target else {
            g.land.fighters.insert(id, f);
            out.insert(id, inp);
            continue;
        };
        f.target = Some(tid);
        let to = flat(tp - me);
        let dist = to.length();
        let face = facing_to(me, tp);
        // A strike in progress: wind-up (the tell), then it lands.
        if let Some(mut s) = f.swing {
            s.t += dt;
            let progress = (s.t / (s.windup + s.late + 0.15)).clamp(0.0, 1.0);
            if let Some(ch) = sim.state.entities.get_mut(id).and_then(|e| e.character.as_mut()) {
                ch.anim.set_action(MoveId::of("slash"), progress, (s.windup / (s.windup + s.late + 0.15)).clamp(0.0, 1.0), 1.0);
                ch.face = Some(face);
            }
            if s.t >= s.windup + s.late {
                f.swing = None;
                f.cooldown = f.profile.interval;
                if f.team == Team::Enemy {
                    g.land.tokens.give_back(id.0);
                }
                if let Some(ch) = sim.state.entities.get_mut(id).and_then(|e| e.character.as_mut()) {
                    ch.anim.set_action(MoveId::NONE, 0.0, 0.0, 1.0);
                }
                if dist <= f.profile.reach + 0.6 {
                    if tid == pid {
                        let ago = g.land.hero.counter_ago;
                        match combat::resolve_strike(ago, window, &s, g.land.hero.vitals.iframes > 0.0) {
                            StrikeOutcome::Countered => {
                                g.land.hero.counter_ago = None;
                                g.land.counters += 1;
                                events.push(SimEvent::cue("counter", me + Vec3::Y));
                                g.show("counter");
                                match f.profile.counter {
                                    combat::CounterResult::Kill => {
                                        f.vitals.hurt(9999.0);
                                        g.land.kills += 1;
                                        g.land.slow = 0.15;
                                        g.land.slow_scale = 0.25;
                                        g.land.fighters.insert(id, f.clone());
                                        crate::rounds::on_kill(g, sim, id, None);
                                        if let Some(ch) = sim.state.entities.get_mut(pid).and_then(|e| e.character.as_mut()) {
                                            ch.anim.set_action(MoveId::of("thrust"), 0.6, 0.5, 1.0);
                                        }
                                        crate::rounds::counted(g, "board_counters");
                                    }
                                    combat::CounterResult::Stagger => {
                                        f.vitals.stagger = 1.6;
                                        f.vitals.hurt(f.profile.hp * 0.3);
                                    }
                                }
                            }
                            StrikeOutcome::Dodged => {}
                            StrikeOutcome::Hit => {
                                let dmg = s.damage * g.options.difficulty;
                                g.land.hero.vitals.hurt(dmg);
                                events.push(SimEvent::cue("sword_hit", tp + Vec3::Y));
                                events.push(SimEvent::Hit { pos: tp + Vec3::Y, strength: 0.6, target: Some(pid) });
                                g.shake(0.25, (0.5, 0.4, 120));
                            }
                        }
                    } else if let Some(o) = g.land.fighters.get_mut(&tid) {
                        // NPC against NPC: allies win in the end, but it takes a while.
                        let dmg = if f.team == Team::Ally { s.damage * 1.4 } else { s.damage * 0.5 };
                        if o.vitals.hurt(dmg) {
                            g.land.tokens.give_back(tid.0);
                        }
                        events.push(SimEvent::cue("sword_clash", tp + Vec3::Y));
                    }
                }
            } else {
                f.swing = Some(s);
            }
            g.land.fighters.insert(id, f);
            out.insert(id, inp);
            continue;
        }
        // Close in; wait for a token (enemies against the player); attack.
        let reach = f.profile.reach;
        let has_token = f.team == Team::Ally || tid != pid || g.land.tokens.take(id.0);
        if dist > reach * 0.9 {
            let mut dir = to / dist.max(1e-3);
            if !has_token && dist < 4.0 {
                // Circle and taunt at a distance.
                dir = Vec2::new(-dir.y, dir.x) * f.circle + dir * (dist - 3.5).clamp(-1.0, 1.0) * 0.5;
            }
            inp.move_dir = dir.clamp_length_max(1.0) * (f.profile.speed / 6.0).clamp(0.3, 1.0);
        } else if has_token && f.cooldown <= 0.0 {
            let windup = f.profile.windup * if g.options.difficulty < 0.8 { 1.25 } else { 1.0 };
            f.swing = Some(Swing { t: 0.0, windup, damage: f.profile.damage, late: 0.1 });
            events.push(SimEvent::cue("sword_swing", me + Vec3::Y));
        }
        if let Some(ch) = sim.state.entities.get_mut(id).and_then(|e| e.character.as_mut()) {
            ch.face = Some(face);
        }
        g.land.fighters.insert(id, f);
        out.insert(id, inp);
    }
    // Surrender: when the captain and officer are down and fewer than two crew stand.
    if g.land.boarding.as_ref().is_some_and(|b| b.phase == BoardPhase::Fight) {
        let leaders_down = g.land.fighters.values().filter(|f| f.kind == "sobers" || f.kind == "officer").all(|f| f.vitals.dead);
        let crew = g.land.fighters.values().filter(|f| f.kind == "crew" && !f.vitals.dead && !f.surrendered).count();
        if leaders_down && crew < 2 {
            for f in g.land.fighters.values_mut().filter(|f| f.team == Team::Enemy && !f.vitals.dead) {
                f.surrendered = true;
            }
            if let Some(b) = &mut g.land.boarding {
                b.phase = BoardPhase::Plunder;
                b.won = true;
                b.focus = 0;
                b.stage = 0;
            }
            g.mission.set_flag("boarding_won");
            events.push(SimEvent::cue("crowd_scream", player_feet));
            g.show("boarding_won");
        }
    }
    out
}

/// After physics: the player's health, death, falls.
pub fn post(g: &mut BlackTap, sim: &mut Sim, dt: f32, events: &mut [SimEvent]) {
    g.land.slow = (g.land.slow - dt).max(0.0);
    let in_fight = g.land.enemies_left() > 0;
    g.land.hero.vitals.tick(dt, if in_fight { 0.0 } else { 6.0 }, 4.0);
    g.land.hero.tools.tick(dt);
    for t in &mut g.land.hero.pistols {
        *t = (*t - dt).max(0.0);
    }
    if g.land.hero.vitals.dead && g.spilled.is_none() {
        g.spill();
        g.land.hero.vitals = Vitals::new(g.land.hero.vitals.max);
    }
    // Fall damage from landings.
    if let Some(pid) = sim.state.player {
        for e in events.iter() {
            if let SimEvent::Land { speed, roll, .. } = e {
                let rolled = *roll || g.land.sprinting;
                let feet = sim.feet(pid).unwrap_or_default();
                let soft =
                    g.world().hide_at(feet).is_some_and(|h| matches!(h.kind.as_str(), "hay" | "haystack" | "cart" | "awning"));
                let dmg = if soft { 0.0 } else { combat::fall_damage(*speed, rolled, g.land.hero.vitals.max) };
                if *speed > 9.0 && !soft {
                    g.pending_events.push(SimEvent::cue("land_hard", feet));
                    let k = ((*speed - 9.0) / 8.0).clamp(0.0, 1.0);
                    g.shake(0.15 + 0.25 * k, (0.5 + 0.5 * k, 0.3, 110 + (100.0 * k) as u32));
                }
                if dmg > 0.0 {
                    g.land.hero.vitals.hurt(dmg);
                }
            }
        }
        let swimming = sim.state.entities.get(pid).and_then(|e| e.character.as_ref()).is_some_and(|c| c.swimming);
        g.land.swim_time = if swimming { g.land.swim_time + dt } else { 0.0 };
    }
}

pub fn status(g: &BlackTap, _sim: &Sim) -> serde_json::Value {
    g.land.status()
}

/// A puppet for a body plan (kept here so `looks.rs` stays data-like).
pub fn biped() -> PuppetDef {
    PuppetDef::preset(BodyPlan::Biped)
}
