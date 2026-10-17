//! The Drydock: the developer and training level, a cay with a station per system (PLAN.md
//! §9.19). Its rules data is `games/blacktap/drydock.toml` (markers, people, routes like
//! tab1.toml, plus the `[[station]]` table) and its geometry `games/blacktap/drydock_world.toml`.
//!
//! A station has a sign (`DrawItem::Text` on a board), a guide (the objective panel: its name, a
//! one-line lesson and its pads as a checklist) and pads: discs the player walks onto (or presses
//! Interact on) that switch something. Sea stations stand in the water; their pads are rings the
//! Black Tap sails through. Each pad names an effect (`kind:arg`); `trigger` maps it to one
//! function here. Everything else is the game's own: sailing, gunnery, boarding, the fights,
//! perception, the overlays and the camera contexts.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use glam::{Quat, Vec2, Vec3};
use pav_core::entity::EntityId;
use pav_core::frame::SimEvent;
use pav_core::input::InputFrame;
use pav_core::play::{
    AudioState, Awareness, DrawItem, DrawList, HudItem, MarkerKind, Objective, Overlay, Prompt, Results, Tile, UiModel,
    WorldMarker,
};
use pav_core::sim::Sim;
use pav_core::{Color, Look, Shape};
use pav_kit::combat::{Tool, Tools, Vitals};
use pav_kit::naval::{BatteryKind, CaptainKind, Faction};
use pav_kit::perception::{Heat, Mind, Senses};
use serde::{Deserialize, Serialize};

use crate::game::{BlackTap, Body, b};
use crate::world::World;

/// The people the Drydock places (`[[npc]]` groups in drydock.toml).
const GROUPS: [&str; 4] = ["ring", "range", "yard", "market"];
/// Groups of practice dummies: blind and deaf until the ring's Spar pad.
const DUMMIES: [&str; 2] = ["ring", "range"];
/// The fighters that step aside while a boarding action runs (its surrender counts officers).
const FIGHTERS: [&str; 3] = ["ring", "range", "yard"];
/// Camera contexts the bench may show (`camera.toml`).
const CONTEXTS: [&str; 11] =
    ["foot", "freerun", "climb", "crouch", "combat", "aim", "swim", "helm", "broadside", "spyglass", "title"];

const CREAM: &str = "#f6ecd2";
const GOLD: &str = "#e2b04a";
const TIMBER: &str = "#5a3f28";

// ---------------------------------------------------------------- the station table (data)

#[derive(Clone, Debug, Deserialize)]
struct PadDef {
    at: [f32; 3],
    effect: String,
    label: String,
    #[serde(default)]
    radius: f32,
}

#[derive(Clone, Debug, Deserialize)]
struct StationDef {
    key: String,
    name: String,
    kind: String,
    at: [f32; 3],
    #[serde(default)]
    facing: f32,
    #[serde(default)]
    afloat: bool,
    #[serde(default)]
    radius: f32,
    #[serde(default)]
    learn: String,
    #[serde(default)]
    pads: Vec<PadDef>,
}

#[derive(Debug, Default, Deserialize)]
struct DockFile {
    #[serde(default)]
    station: Vec<StationDef>,
}

/// A station as placed in the world.
#[derive(Clone, Debug)]
pub struct Station {
    pub key: String,
    pub name: String,
    /// sea, traversal, combat or presentation.
    pub kind: String,
    /// The sign's foot (on the ground, or at sea level for a sea station).
    pub sign: Vec3,
    /// The way the sign reads (degrees from north).
    pub facing: f32,
    /// A sea station: the sign stands in the water and the pads are rings for the ship.
    pub afloat: bool,
    /// The middle of the sign and its pads; the guide shows within `radius` of it.
    pub center: Vec3,
    pub radius: f32,
    pub learn: String,
    /// Indices into `Stations::pads`.
    pub pads: Vec<usize>,
}

/// A pad as placed.
#[derive(Clone, Debug)]
pub struct Pad {
    pub station: usize,
    pub at: Vec3,
    /// `kind:arg` (see `trigger`).
    pub effect: String,
    pub label: String,
    pub radius: f32,
    /// A ring on the water the ship sails through (else a disc the player stands on).
    pub afloat: bool,
}

/// The station and pad tables.
#[derive(Clone, Debug, Default)]
pub struct Stations {
    pub list: Vec<Station>,
    pub pads: Vec<Pad>,
}

impl Stations {
    pub fn by_key(&self, key: &str) -> Option<&Station> {
        self.list.iter().find(|s| s.key == key)
    }

    /// The pads with this effect.
    pub fn with_effect<'a>(&'a self, effect: &'a str) -> impl Iterator<Item = &'a Pad> + 'a {
        self.pads.iter().filter(move |p| p.effect == effect)
    }
}

/// Places the stations of a level file's text on its world (pads on the ground when `y` is 0).
pub fn parse(text: &str, w: &World) -> Stations {
    let file: DockFile = toml::from_str(text).unwrap_or_else(|e| {
        log::warn!("drydock.toml: {e}");
        DockFile::default()
    });
    let mut out = Stations::default();
    for (si, s) in file.station.iter().enumerate() {
        let place = |a: [f32; 3]| {
            let y = if s.afloat {
                0.0
            } else if a[1] != 0.0 {
                a[1]
            } else {
                w.ground(Vec2::new(a[0], a[2])).max(0.0)
            };
            Vec3::new(a[0], y, a[2])
        };
        let sign = place(s.at);
        let mut pads = Vec::new();
        for p in &s.pads {
            pads.push(out.pads.len());
            let radius = if p.radius > 0.0 {
                p.radius
            } else if s.afloat {
                9.0
            } else {
                0.95
            };
            out.pads.push(Pad {
                station: si,
                at: place(p.at),
                effect: p.effect.clone(),
                label: p.label.clone(),
                radius,
                afloat: s.afloat,
            });
        }
        let points: Vec<Vec3> = std::iter::once(sign).chain(pads.iter().map(|&i| out.pads[i].at)).collect();
        let center = points.iter().copied().sum::<Vec3>() / points.len() as f32;
        let reach = points.iter().map(|p| flat(*p - center).length()).fold(0.0, f32::max) + if s.afloat { 20.0 } else { 2.0 };
        out.list.push(Station {
            key: s.key.clone(),
            name: s.name.clone(),
            kind: s.kind.clone(),
            sign,
            facing: s.facing,
            afloat: s.afloat,
            center,
            radius: s.radius.max(reach),
            learn: s.learn.clone(),
            pads,
        });
    }
    out
}

static STATIONS: Mutex<Option<Arc<Stations>>> = Mutex::new(None);

/// The stations (placed at the scene's build; re-read from the level file when missing).
pub fn stations(g: &BlackTap) -> Arc<Stations> {
    if let Some(s) = STATIONS.lock().unwrap_or_else(|p| p.into_inner()).as_ref() {
        return s.clone();
    }
    reload(g)
}

fn reload(g: &BlackTap) -> Arc<Stations> {
    let text = crate::data(&g.level).unwrap_or_default();
    let s = Arc::new(parse(&text, &g.world()));
    *STATIONS.lock().unwrap_or_else(|p| p.into_inner()) = Some(s.clone());
    s
}

/// Table names in a data file (`[song.*]` in music.toml...), sorted, read once.
fn bank_keys(file: &str, table: &str) -> Arc<Vec<String>> {
    static CACHE: Mutex<BTreeMap<String, Arc<Vec<String>>>> = Mutex::new(BTreeMap::new());
    let key = format!("{file}#{table}");
    let mut cache = CACHE.lock().unwrap_or_else(|p| p.into_inner());
    if let Some(k) = cache.get(&key) {
        return k.clone();
    }
    let text = crate::data(file).unwrap_or_default();
    let doc: toml::Table = toml::from_str(&text).unwrap_or_default();
    let mut keys: Vec<String> =
        doc.get(table).and_then(|t| t.as_table()).map(|t| t.keys().cloned().collect()).unwrap_or_default();
    keys.sort();
    let keys = Arc::new(keys);
    cache.insert(key, keys.clone());
    keys
}

/// Every song in music.toml.
pub fn songs() -> Arc<Vec<String>> {
    bank_keys("music.toml", "song")
}

/// Every ambience layer in ambience.toml.
pub fn ambience_layers() -> Arc<Vec<String>> {
    bank_keys("ambience.toml", "layer")
}

/// Every cue in the sound bank.
pub fn sound_cues() -> Arc<Vec<String>> {
    bank_keys("sounds.toml", "cue")
}

// ---------------------------------------------------------------- state

/// The Drydock's own state (part of the game state: snapshots and rewind include it).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Dock {
    /// The station the player (or the ship, at the helm) is at, and since when.
    pub station: Option<String>,
    #[serde(default)]
    pub since: f32,
    /// The pad underfoot and the pad under the keel (their effects fire on stepping on).
    #[serde(default)]
    pub on_pad: Option<usize>,
    #[serde(default)]
    pub ship_pad: Option<usize>,
    /// How often each effect fired (the guide's checklist, tests, tools).
    #[serde(default)]
    pub fired: BTreeMap<String, u32>,
    /// The camera bench's context and the UI gallery's overlay, and the station that set them
    /// (leaving it ends them).
    #[serde(default)]
    pub camera: Option<String>,
    #[serde(default)]
    pub overlay: Option<String>,
    #[serde(default)]
    pub owner: Option<String>,
    /// The ring's slow-motion tells and the traversal stations' slow motion.
    #[serde(default)]
    pub slow_tells: bool,
    #[serde(default)]
    pub slowmo: bool,
    /// The stealth yard's vision cones are hidden.
    #[serde(default)]
    pub hide_cones: bool,
    /// The jukebox: the song, the soloed ambience layer (0 = the game's mix), the sound cue,
    /// the theatre's dialog line.
    #[serde(default)]
    pub song: Option<usize>,
    #[serde(default)]
    pub mix: usize,
    #[serde(default)]
    pub sound: Option<usize>,
    #[serde(default)]
    pub line: Option<usize>,
    /// Last Words is playing: the atmosphere and music to restore after it.
    #[serde(default)]
    pub theatre: Option<(String, String)>,
    /// Sea stations' ships and batteries.
    #[serde(default)]
    pub targets: Vec<u32>,
    #[serde(default)]
    pub dummy_battery: Option<u32>,
    #[serde(default)]
    pub duel: Option<u32>,
    #[serde(default)]
    pub mortar: Option<u32>,
    #[serde(default)]
    pub hulk: Option<u32>,
    #[serde(default)]
    pub hulk_wait: f32,
    /// The practice fighters stepped aside for a boarding action.
    #[serde(default)]
    pub hidden: bool,
}

impl Dock {
    /// Times an effect fired.
    pub fn fired(&self, effect: &str) -> u32 {
        self.fired.get(effect).copied().unwrap_or(0)
    }
}

fn flat(v: Vec3) -> Vec2 {
    Vec2::new(v.x, v.z)
}

fn hex(c: &str) -> Color {
    Color::hex(c)
}

/// Words from a data key ("dawn_after_storm" → "Dawn after storm").
fn pretty(key: &str) -> String {
    let s = key.replace('_', " ");
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

/// Senses that never notice anything (practice dummies).
fn blind() -> Senses {
    Senses { near: 0.0, far: 0.0, hearing: 0.0, ..Senses::default() }
}

// ---------------------------------------------------------------- setup and people

pub fn setup(g: &mut BlackTap, sim: &mut Sim) {
    reload(g);
    let w = g.world();
    g.set_atmosphere("afternoon", 0.0);
    g.music = "explore".into();
    g.body = Body::Foot;
    g.sea.timber = g.sea.timber.max(6);
    g.sea.iron = g.sea.iron.max(2);
    moor(g);
    spawn_deck(g, sim);
    // The player on the pier beside her.
    let start = w.marker("start").unwrap_or(Vec3::new(0.5, 2.1, 6.0));
    sim.state.spawn = start + Vec3::Y * 0.5;
    if let Some(pid) = sim.state.player {
        sim.set_position(pid, start);
        if let Some(ch) = sim.state.entities.get_mut(pid).and_then(|e| e.character.as_mut()) {
            ch.facing = 0.0;
            ch.scripted = None;
        }
    }
    fresh_targets(g);
    fresh_hulk(g);
    for group in GROUPS {
        spawn_group(g, sim, group);
    }
}

/// The Black Tap at her mooring west of the pier, bow to the lagoon's mouth, anchored.
fn moor(g: &mut BlackTap) {
    let at = g.world().marker_xz("r1_start");
    let me = g.sea.mine;
    if let Some(s) = g.sea.ship_mut(me) {
        s.pos = at;
        s.heading = 180.0;
        s.speed = 0.0;
        s.yaw_rate = 0.0;
        s.sail = 0;
        s.sail_shown = 0.0;
        s.anchored = true;
    }
}

/// The Black Tap's walkable deck (while she lies at her mooring with nobody at the helm).
fn spawn_deck(g: &mut BlackTap, sim: &mut Sim) {
    if g.land.deck.is_some() {
        return;
    }
    let Some(ship) = g.sea.my_ship().cloned() else { return };
    let class = g.content().classes.get(&ship.class).cloned().unwrap_or_default();
    g.land.deck = Some(crate::foot::spawn_ship_deck(sim, &ship, &class));
}

fn spawn_group(g: &mut BlackTap, sim: &mut Sim, group: &str) {
    crate::stealth::spawn_group(g, sim, group);
    if DUMMIES.contains(&group) {
        for n in g.stealth.npcs.values_mut().filter(|n| n.group == group) {
            n.senses = blind();
        }
    }
}

fn despawn_group(g: &mut BlackTap, sim: &mut Sim, group: &str) {
    let ids: Vec<EntityId> = g.stealth.npcs.iter().filter(|(_, n)| n.group == group).map(|(&id, _)| id).collect();
    for id in ids {
        g.stealth.npcs.remove(&id);
        g.stealth.bodies.remove(&id);
        g.land.fighters.remove(&id);
        g.land.tokens.give_back(id.0);
        sim.despawn(id);
    }
    g.flags.remove(&format!("spawned_{group}"));
}

fn respawn_group(g: &mut BlackTap, sim: &mut Sim, group: &str) {
    despawn_group(g, sim, group);
    spawn_group(g, sim, group);
}

/// The gunnery range's three dummy hulls (anchored, no captain) and the battery on Gun Rock.
fn fresh_targets(g: &mut BlackTap) {
    let c = g.content();
    let w = g.world();
    let old = std::mem::take(&mut g.dock.targets);
    g.sea.ships.retain(|s| !old.contains(&s.id));
    for (i, class) in ["gunboat", "schooner", "gunboat"].iter().enumerate() {
        let at = w.marker_xz(&format!("gun_target_{}", i + 1));
        let id = g.sea.add_ship(&c.classes, class, &format!("Target {}", i + 1), "target", at, 90.0, Faction::Temperance);
        if let Some(s) = g.sea.ship_mut(id) {
            s.anchored = true;
        }
        g.dock.targets.push(id);
    }
    match g.dock.dummy_battery.and_then(|id| g.sea.batteries.iter_mut().find(|b| b.id == id)) {
        Some(bt) => {
            bt.hp = bt.hp_max;
            bt.destroyed = false;
        }
        None => {
            // Range 0: a target that never fires back.
            let at = w.marker("gun_battery").unwrap_or(Vec3::new(-190.0, 4.5, 140.0));
            g.dock.dummy_battery = Some(g.sea.add_battery("gun_rock", BatteryKind::Guns, at, 4, 400.0, 0.0));
        }
    }
}

/// The boarding hulk: a disabled schooner tagged like the Restraint (so the game's boarding
/// rules take her), anchored at her mooring.
fn fresh_hulk(g: &mut BlackTap) {
    let c = g.content();
    let at = g.world().marker_xz("hulk");
    if let Some(old) = g.dock.hulk.take() {
        g.sea.ships.retain(|s| s.id != old);
    }
    let id = g.sea.add_ship(&c.classes, "schooner", "The Old Restraint", "restraint", at, 90.0, Faction::Temperance);
    if let Some(s) = g.sea.ship_mut(id) {
        s.disabled = true;
        s.hull = s.hull_max * 0.2;
        s.sail = 0;
        s.anchored = true;
    }
    g.dock.hulk = Some(id);
}

// ---------------------------------------------------------------- the tick

/// The nearest station whose guide area holds `p` (sea stations for the ship at the helm).
fn station_at(st: &Stations, p: Vec3, at_helm: bool) -> Option<usize> {
    st.list
        .iter()
        .enumerate()
        .filter(|(_, s)| s.afloat == at_helm)
        .map(|(i, s)| (i, flat(p - s.center).length(), s.radius))
        .filter(|(_, d, r)| d < r)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(i, _, _)| i)
}

/// The pad under `p` (feet on a disc, or the ship's position in a ring).
fn pad_at(st: &Stations, p: Vec3, afloat: bool) -> Option<usize> {
    st.pads
        .iter()
        .enumerate()
        .filter(|(_, q)| q.afloat == afloat && (afloat || (p.y - q.at.y).abs() < 1.5))
        .map(|(i, q)| (i, flat(p - q.at).length(), q.radius))
        .filter(|(_, d, r)| d < r)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(i, _, _)| i)
}

/// Every tick in the Drydock (after the player's own rules): stations, pads, switches.
pub fn pre(
    g: &mut BlackTap,
    sim: &mut Sim,
    input: &InputFrame,
    dt: f32,
    events: &mut Vec<SimEvent>,
) -> BTreeMap<EntityId, InputFrame> {
    let st = stations(g);
    // Nobody dies in the Drydock.
    if g.land.hero.vitals.dead {
        g.land.hero.vitals = Vitals::new(g.land.hero.vitals.max);
        g.toast("Patched up: nobody dies in the Drydock");
    }
    // A boarding action: the practice fighters step aside (its surrender counts officers).
    if g.land.boarding.is_some() && !g.dock.hidden {
        for group in FIGHTERS {
            despawn_group(g, sim, group);
        }
        g.dock.hidden = true;
    }
    // Where the player is: the station, then the pad.
    let at_helm = g.body == Body::Helm;
    let feet = BlackTap::player_feet(sim);
    let keel = g.sea.my_ship().map(|s| Vec3::new(s.pos.x, 0.0, s.pos.y));
    let here = if at_helm { keel } else { feet };
    let station = here.and_then(|p| station_at(&st, p, at_helm)).map(|i| st.list[i].key.clone());
    if station != g.dock.station {
        if g.dock.owner.is_some() && g.dock.owner != station {
            clear_station_fx(g);
        }
        g.dock.station = station;
        g.dock.since = g.time;
    }
    let busy = g.land.boarding.is_some() || g.stealth.leap.is_some() || g.stealth.survey.is_some();
    let on_foot = (!at_helm && !busy).then_some(feet).flatten().and_then(|p| pad_at(&st, p, false));
    if let Some(i) = on_foot
        && (g.dock.on_pad != Some(i) || input.just(b::INTERACT))
    {
        trigger(g, sim, &st.pads[i], events);
    }
    g.dock.on_pad = on_foot;
    let on_ship = (at_helm && !busy).then_some(keel).flatten().and_then(|p| pad_at(&st, p, true));
    if let Some(i) = on_ship
        && g.dock.ship_pad != Some(i)
    {
        trigger(g, sim, &st.pads[i], events);
    }
    g.dock.ship_pad = on_ship;
    switches(g, dt);
    BTreeMap::new()
}

/// What the pads switched on, every tick.
fn switches(g: &mut BlackTap, dt: f32) {
    // Slow motion for studying a climb (the whole world; off at the helm).
    if g.dock.slowmo {
        if g.body == Body::Helm {
            g.dock.slowmo = false;
        } else {
            g.land.slow = g.land.slow.max(dt * 2.0);
            g.land.slow_scale = 0.4;
        }
    }
    // Slow-motion tells: the ring's strikes wind up at a third of the speed.
    if g.dock.slow_tells && ring_winding_up(g) {
        g.land.slow = g.land.slow.max(dt * 2.0);
        g.land.slow_scale = 0.3;
    }
    // The mortar holds fire while nobody is at the helm (the Black Tap at her mooring).
    if let Some(id) = g.dock.mortar
        && g.body != Body::Helm
        && let Some(bt) = g.sea.batteries.iter_mut().find(|b| b.id == id)
    {
        bt.cooldown = bt.cooldown.max(1.5);
        bt.telegraph = 0.0;
    }
    // The Black Tap doesn't sink in practice.
    let me = g.sea.mine;
    if let Some(s) = g.sea.ship_mut(me) {
        s.hull = s.hull.max(s.hull_max * 0.15);
    }
    // The game names a silenced battery after Fort Abstinence's: Gun Rock's is a target.
    if g.sea.destroyed_order.last().map(|s| s.as_str()) == Some("gun_rock") {
        for t in g.toasts.iter_mut().filter(|t| t.text == "Mortar Tower silenced") {
            t.text = "Gun Rock silenced: sail through the ring for fresh targets".into();
        }
    }
}

/// A ring dummy is winding up a strike (the tell).
fn ring_winding_up(g: &BlackTap) -> bool {
    g.land
        .fighters
        .iter()
        .any(|(id, f)| f.swing.is_some_and(|s| s.t < s.windup) && g.stealth.npcs.get(id).is_some_and(|n| n.group == "ring"))
}

/// Leaving a station ends its camera context and overlay.
fn clear_station_fx(g: &mut BlackTap) {
    g.dock.camera = None;
    g.dock.overlay = None;
    g.dock.owner = None;
}

pub fn post(g: &mut BlackTap, sim: &mut Sim, dt: f32, _events: &mut Vec<SimEvent>) {
    // A boarding won and plundered: back to the helm, the prize scuttled, a fresh hulk later.
    if g.land.boarding.as_ref().is_some_and(|bd| bd.phase == crate::foot::BoardPhase::Plunder) && g.mission.flag("plunder_done") {
        crate::foot::end_boarding(g, sim);
        for f in ["plunder_done", "boarded", "boarding_won"] {
            g.mission.flags.remove(f);
        }
        g.dock.hulk = None;
        g.dock.hulk_wait = 10.0;
        g.toast("Prize taken: a fresh hulk is on her way");
    }
    if g.dock.hidden && g.land.boarding.is_none() {
        for group in FIGHTERS {
            respawn_group(g, sim, group);
        }
        g.dock.hidden = false;
    }
    // A fresh hulk a while after the last one went down.
    let boarding = g.land.boarding.is_some();
    let hulk_gone = g.dock.hulk.and_then(|id| g.sea.ship(id)).is_none_or(|s| !s.alive());
    if hulk_gone && !boarding {
        g.dock.hulk_wait -= dt;
        if g.dock.hulk_wait <= 0.0 {
            fresh_hulk(g);
        }
    }
    // Last Words over: the Drydock's weather and music come back.
    if let Some((atmos, music)) = g.dock.theatre.clone()
        && g.stealth.last_words.is_none()
    {
        g.set_atmosphere(&atmos, 2.0);
        g.music = music;
        g.mission.flags.remove("last_words_done");
        g.dock.theatre = None;
    }
    // The duel is over.
    if let Some(id) = g.dock.duel
        && g.sea.ship(id).is_none_or(|s| !s.alive())
    {
        g.dock.duel = None;
        if g.music == "sea_combat" {
            g.music = "explore".into();
        }
    }
}

// ---------------------------------------------------------------- pad effects

/// Effect kinds `trigger` knows (`kind:arg`).
const EFFECTS: [&str; 26] = [
    "ship", "map", "ui", "wind", "flotsam", "gunnery", "duel", "mortar", "hulk", "lift", "slowmo", "survey", "ring", "tools",
    "yard", "heat", "poster", "market", "theatre", "song", "mix", "sound", "atmos", "weather", "camera", "rumble",
];

/// Problems in the station table: unknown effects, lifts to missing markers, unknown
/// atmospheres, camera contexts or overlays (`pav level_check` covers Tab I; tests run this).
pub fn check(g: &BlackTap) -> Vec<String> {
    let st = stations(g);
    let w = g.world();
    let c = g.content();
    let mut problems = Vec::new();
    for p in &st.pads {
        let (kind, arg) = p.effect.split_once(':').unwrap_or((p.effect.as_str(), ""));
        let ok = match kind {
            "lift" => w.marker(arg).is_some(),
            "atmos" => c.atmospheres.contains_key(arg),
            "camera" => CONTEXTS.contains(&arg) && c.camera.0.contains_key(arg),
            "ui" => ["tiles", "wheel", "results", "map", "cinematic", "off"].contains(&arg),
            k => EFFECTS.contains(&k),
        };
        if !ok {
            problems.push(format!("{}: pad '{}' has a bad effect '{}'", st.list[p.station].key, p.label, p.effect));
        }
    }
    for m in ["r1_start", "start", "hulk", "duel_spawn", "gun_battery", "mortar_pos", "whistle", "ring_center", "map_table"] {
        if w.marker(m).is_none() {
            problems.push(format!("drydock.toml: marker '{m}' is missing"));
        }
    }
    problems
}

/// Fires a pad's effect.
fn trigger(g: &mut BlackTap, sim: &mut Sim, pad: &Pad, events: &mut Vec<SimEvent>) {
    let (kind, arg) = pad.effect.split_once(':').unwrap_or((pad.effect.as_str(), ""));
    events.push(SimEvent::cue("ui_confirm", pad.at + Vec3::Y));
    match kind {
        "ship" => match arg {
            "board" => fx_board(g, sim),
            "home" => fx_home(g, sim),
            _ => fx_refit(g),
        },
        "map" => fx_overlay(g, "map"),
        "ui" => fx_overlay(g, arg),
        "wind" => fx_wind(g, arg == "veer"),
        "flotsam" => fx_flotsam(g),
        "gunnery" => fx_gunnery(g),
        "duel" => fx_duel(g),
        "mortar" => fx_mortar(g),
        "hulk" => fx_hulk(g),
        "lift" => fx_lift(g, sim, arg),
        "slowmo" => fx_slowmo(g),
        "survey" => fx_survey_reset(g),
        "ring" => match arg {
            "spar" => fx_spar(g, sim),
            "stand_down" => fx_stand_down(g),
            "slow" => fx_slow_tells(g),
            _ => fx_fresh(g, sim, "ring"),
        },
        "tools" => match arg {
            "restock" => fx_restock(g),
            _ => fx_wake(g, sim),
        },
        "yard" => match arg {
            "cones" => fx_cones(g),
            _ => fx_yard_reset(g, sim),
        },
        "heat" => fx_heat(g, arg == "up"),
        "poster" => fx_repost(g),
        "market" => fx_fresh(g, sim, "market"),
        "theatre" => match arg {
            "last_words" => fx_last_words(g, sim),
            "line" => fx_line(g),
            _ => fx_banner(g),
        },
        "song" => fx_song(g, arg != "prev"),
        "mix" => fx_mix(g),
        "sound" => fx_sound(g, pad.at, events),
        "atmos" => fx_atmos(g, arg),
        "weather" => fx_squall(g),
        "camera" => fx_camera(g, arg),
        "rumble" => fx_rumble(g, arg),
        other => log::warn!("drydock: unknown pad effect '{other}'"),
    }
    *g.dock.fired.entry(pad.effect.clone()).or_default() += 1;
}

/// Pier: onto the Black Tap's deck by the wheel.
fn fx_board(g: &mut BlackTap, sim: &mut Sim) {
    if g.body != Body::Foot {
        return;
    }
    spawn_deck(g, sim);
    let Some(ship) = g.sea.my_ship().cloned() else { return };
    let class = g.content().classes.get(&ship.class).cloned().unwrap_or_default();
    let to = ship.world(Vec3::new(0.0, class.freeboard + 0.2, -class.length * 0.3));
    if let Some(pid) = sim.state.player {
        sim.set_position(pid, to);
    }
    g.toast("Aboard: hold Interact at the wheel to take the helm");
}

/// Pier: the Black Tap back at her mooring (wherever she was left).
fn fx_home(g: &mut BlackTap, sim: &mut Sim) {
    if g.body != Body::Foot {
        return;
    }
    if let Some(d) = g.land.deck.take() {
        sim.despawn(d);
    }
    moor(g);
    spawn_deck(g, sim);
    g.toast("The Black Tap is back at the pier");
}

/// Pier: hull, sails, kegs and stores full.
fn fx_refit(g: &mut BlackTap) {
    let c = g.content();
    let me = g.sea.mine;
    if let Some(s) = g.sea.ship_mut(me) {
        let kegs = c.classes.get(&s.class).map(|k| k.kegs).unwrap_or(4);
        s.hull = s.hull_max;
        s.sails = 1.0;
        s.fire = 0.0;
        s.torn = 0.0;
        s.guns.kegs_left = s.guns.kegs_left.max(kegs);
    }
    g.sea.timber = g.sea.timber.max(6);
    g.sea.iron = g.sea.iron.max(2);
    g.toast("Refitted: hull, sails, kegs and timber");
}

/// Map table and UI gallery: an overlay with sample data while the player stays at the station.
fn fx_overlay(g: &mut BlackTap, kind: &str) {
    if kind == "off" {
        g.dock.overlay = None;
        return;
    }
    g.dock.overlay = Some(kind.to_string());
    g.dock.owner = g.dock.station.clone();
}

/// Compass point a direction comes from.
fn compass(heading: f32) -> &'static str {
    const NAMES: [&str; 8] = ["north", "north-east", "east", "south-east", "south", "south-west", "west", "north-west"];
    NAMES[((heading.rem_euclid(360.0) + 22.5) / 45.0) as usize % 8]
}

/// Sailing lane: the wind swings 90° (veers clockwise or backs).
fn fx_wind(g: &mut BlackTap, veer: bool) {
    let w = g.sea.wind;
    g.sea.wind = if veer { Vec2::new(-w.y, w.x) } else { Vec2::new(w.y, -w.x) };
    let from = compass(pav_kit::naval::heading_of(-g.sea.wind));
    g.toast(&format!("The wind {} 90: now from the {from}", if veer { "veers" } else { "backs" }));
}

/// Sailing lane: six fresh crates down the lane.
fn fx_flotsam(g: &mut BlackTap) {
    let w = g.world();
    g.sea.floating.retain(|f| f.kind != crate::sea::FloatKind::Flotsam);
    for i in 1..=6 {
        let p = w.marker_xz(&format!("flotsam_{i}"));
        g.sea.add_float(p, crate::sea::FloatKind::Flotsam);
    }
    g.toast("Fresh flotsam down the lane");
}

/// Gunnery range: fresh dummy hulls and the battery rebuilt.
fn fx_gunnery(g: &mut BlackTap) {
    fresh_targets(g);
    g.toast("Fresh targets on the range");
}

/// Duel buoy: a hostile Temperance schooner comes for the Black Tap.
fn fx_duel(g: &mut BlackTap) {
    if g.dock.duel.and_then(|id| g.sea.ship(id)).is_some_and(|s| s.alive()) {
        g.toast("The schooner is still out there");
        return;
    }
    let c = g.content();
    let at = g.world().marker_xz("duel_spawn");
    let mine = g.sea.my_ship().map(|s| s.pos).unwrap_or_default();
    let id = g.sea.add_ship(
        &c.classes,
        "schooner",
        "TS Teetotal",
        "duel",
        at,
        pav_kit::naval::heading_of(mine - at),
        Faction::Temperance,
    );
    g.sea.add_captain(id, CaptainKind::Schooner, Vec::new(), None, true);
    g.dock.duel = Some(id);
    g.music = "sea_combat".into();
    g.toast("The TS Teetotal answers the challenge");
}

/// Mortar yard: the mortar on the rock is armed, or stood down.
fn fx_mortar(g: &mut BlackTap) {
    if let Some(id) = g.dock.mortar.take() {
        g.sea.batteries.retain(|b| b.id != id);
        g.sea.shells.clear();
        g.toast("The mortar stands down");
        return;
    }
    let at = g.world().marker("mortar_pos").unwrap_or(Vec3::new(118.0, 10.0, 28.0));
    g.dock.mortar = Some(g.sea.add_battery("mortar", BatteryKind::Mortar, at, 1, 600.0, 175.0));
    g.toast("Mortar armed: steer out of the red rings");
}

/// Boarding hulk: a fresh one at her mooring.
fn fx_hulk(g: &mut BlackTap) {
    if g.land.boarding.is_some() {
        return;
    }
    fresh_hulk(g);
    g.toast("A fresh hulk to board");
}

/// Traversal: straight up to a named spot (a roof, the wall's walk, the lantern roof).
fn fx_lift(g: &mut BlackTap, sim: &mut Sim, marker: &str) {
    let Some(to) = g.world().marker(marker) else {
        log::warn!("drydock: lift to unknown marker '{marker}'");
        return;
    };
    if let Some(pid) = sim.state.player {
        sim.set_position(pid, to + Vec3::Y * 0.3);
    }
}

/// Traversal: the world at 40% speed (off at the helm or on the pad again).
fn fx_slowmo(g: &mut BlackTap) {
    g.dock.slowmo = !g.dock.slowmo;
    g.toast(if g.dock.slowmo { "Slow motion on" } else { "Slow motion off" });
}

/// Haystack tower: the perches can be surveyed again.
fn fx_survey_reset(g: &mut BlackTap) {
    g.mission.flags.retain(|f| !f.starts_with("surveyed_") && !f.starts_with("on_"));
    g.stealth.revealed.clear();
    g.toast("The perches can be surveyed again");
}

/// Combat ring: the dummies see and fight.
fn fx_spar(g: &mut BlackTap, sim: &mut Sim) {
    let at = BlackTap::player_feet(sim).unwrap_or_default();
    for n in g.stealth.npcs.values_mut().filter(|n| n.group == "ring" && !n.out()) {
        n.senses = crate::looks::senses(&n.kind);
        n.mind.alert(at);
    }
    g.toast("Spar! Counter on the glint");
}

/// Combat ring: the dummies stop, forget and go back to their marks.
fn fx_stand_down(g: &mut BlackTap) {
    let ids: Vec<EntityId> = g.stealth.npcs.iter().filter(|(_, n)| n.group == "ring").map(|(&id, _)| id).collect();
    for id in ids {
        if let Some(n) = g.stealth.npcs.get_mut(&id) {
            n.mind = Mind::default();
            n.senses = blind();
            n.search = None;
        }
        if let Some(f) = g.land.fighters.get_mut(&id) {
            f.swing = None;
            f.target = None;
            f.idle = true;
        }
        g.land.tokens.give_back(id.0);
    }
    g.toast("The dummies stand down");
}

/// Combat ring: strikes wind up in slow motion.
fn fx_slow_tells(g: &mut BlackTap) {
    g.dock.slow_tells = !g.dock.slow_tells;
    g.toast(if g.dock.slow_tells { "Slow-motion tells on" } else { "Slow-motion tells off" });
}

/// Combat ring, market: the group as new.
fn fx_fresh(g: &mut BlackTap, sim: &mut Sim, group: &str) {
    respawn_group(g, sim, group);
    g.toast(if group == "market" { "A fresh crowd" } else { "Fresh dummies" });
}

/// Tool range: every tool, both pistols and the grog.
fn fx_restock(g: &mut BlackTap) {
    let hero = &mut g.land.hero;
    hero.tools.stock = Tools::full().stock;
    hero.pistols = [0.0; 2];
    hero.grog = hero.grog.max(3);
    g.toast("Tools restocked");
}

/// Tool range: the dummies wake (and the fallen get up).
fn fx_wake(g: &mut BlackTap, sim: &mut Sim) {
    if g.stealth.npcs.values().any(|n| n.group == "range" && n.dead) {
        respawn_group(g, sim, "range");
    } else {
        for n in g.stealth.npcs.values_mut().filter(|n| n.group == "range") {
            n.sleep = 0.0;
            n.drunk = 0.0;
            n.blind = 0.0;
            n.stun = 0.0;
            n.mind = Mind::default();
            n.senses = blind();
        }
    }
    g.toast("The dummies are awake");
}

/// Stealth yard: the debug vision cones on or off.
fn fx_cones(g: &mut BlackTap) {
    g.dock.hide_cones = !g.dock.hide_cones;
    g.toast(if g.dock.hide_cones { "Vision cones hidden" } else { "Vision cones shown" });
}

/// Stealth yard: fresh guards, no heat, no alarm.
fn fx_yard_reset(g: &mut BlackTap, sim: &mut Sim) {
    respawn_group(g, sim, "yard");
    g.stealth.heat = Heat::default();
    g.stealth.alarm = 0.0;
    g.toast("Fresh guards in the yard");
}

/// Market, poster wall: a crime seen (heat +1), or all forgotten.
fn fx_heat(g: &mut BlackTap, up: bool) {
    if up {
        g.stealth.heat.crime(1.0);
    } else {
        g.stealth.heat = Heat::default();
    }
    let level = g.stealth.heat.level();
    g.toast(&format!("Heat {level}"));
}

/// Poster wall: every poster back up.
fn fx_repost(g: &mut BlackTap) {
    g.stealth.posters_down.clear();
    g.toast("The posters are back up");
}

/// Theatre: Last Words (letterbox, white mist, slow time, three lines).
fn fx_last_words(g: &mut BlackTap, sim: &mut Sim) {
    if g.stealth.last_words.is_some() {
        return;
    }
    g.dock.theatre = Some((g.atmos_to.clone(), g.music.clone()));
    crate::stealth::act(g, sim, "last_words", None);
}

/// Theatre: the next line in dialog.toml (subtitles and voice).
fn fx_line(g: &mut BlackTap) {
    let c = g.content();
    let n = c.dialog.line.len();
    if n == 0 {
        return;
    }
    let i = g.dock.line.map(|i| (i + 1) % n).unwrap_or(0);
    g.dock.line = Some(i);
    if let Some(id) = c.dialog.line.keys().nth(i) {
        g.dialog.say(&c.dialog, id);
    }
}

/// Theatre: a Round-style banner.
fn fx_banner(g: &mut BlackTap) {
    g.banner("THE DRYDOCK", "A banner, as each Round opens", GOLD);
}

/// Jukebox: the next (or previous) song in music.toml.
fn fx_song(g: &mut BlackTap, next: bool) {
    let list = songs();
    let n = list.len();
    if n == 0 {
        return;
    }
    let i = match g.dock.song {
        None => {
            if next {
                0
            } else {
                n - 1
            }
        }
        Some(i) => {
            if next {
                (i + 1) % n
            } else {
                (i + n - 1) % n
            }
        }
    };
    g.dock.song = Some(i);
    g.music = list[i].clone();
    g.toast(&format!("Now playing: {}", pretty(&list[i])));
}

/// Jukebox: solo the next ambience layer (then back to the game's mix).
fn fx_mix(g: &mut BlackTap) {
    let layers = ambience_layers();
    g.dock.mix = (g.dock.mix + 1) % (layers.len() + 1);
    let text = match g.dock.mix.checked_sub(1).and_then(|i| layers.get(i)) {
        Some(l) => format!("Ambience: {} alone", pretty(l)),
        None => "Ambience: the game's mix".to_string(),
    };
    g.toast(&text);
}

/// Jukebox: the next cue in the sound bank, played at the pad.
fn fx_sound(g: &mut BlackTap, at: Vec3, events: &mut Vec<SimEvent>) {
    let cues = sound_cues();
    let n = cues.len();
    if n == 0 {
        return;
    }
    let i = g.dock.sound.map(|i| (i + 1) % n).unwrap_or(0);
    g.dock.sound = Some(i);
    events.push(SimEvent::cue(&cues[i], at + Vec3::Y));
    g.toast(&format!("Sound {}/{n}: {}", i + 1, cues[i]));
}

/// Weather pads: an atmosphere preset (sky, light, fog, sea state).
fn fx_atmos(g: &mut BlackTap, name: &str) {
    if !g.content().atmospheres.contains_key(name) {
        log::warn!("drydock: unknown atmosphere '{name}'");
        return;
    }
    g.set_atmosphere(name, 2.0);
    g.toast(&format!("Weather: {}", pretty(name)));
}

/// Weather pads: the storm with a gale and two waterspouts crossing the sailing lane.
fn fx_squall(g: &mut BlackTap) {
    g.set_atmosphere("storm", 2.0);
    g.sea.wind = Vec2::new(-11.0, -6.0);
    let lane = g.world().route("sailing_lane");
    for (k, p) in lane.iter().skip(1).step_by(2).take(2).enumerate() {
        let side = if k.is_multiple_of(2) { 1.0 } else { -1.0 };
        g.sea.spouts.push(crate::sea::Spout {
            pos: *p + Vec2::new(40.0 * side, 0.0),
            vel: Vec2::new(-2.5 * side, 0.5),
            radius: 12.0,
            life: 90.0,
        });
    }
    g.toast("Squall: a gale and waterspouts in the lane");
}

/// Camera bench: the camera context while standing at the bench.
fn fx_camera(g: &mut BlackTap, ctx: &str) {
    if !CONTEXTS.contains(&ctx) {
        log::warn!("drydock: unknown camera context '{ctx}'");
        return;
    }
    g.dock.camera = Some(ctx.to_string());
    g.dock.owner = g.dock.station.clone();
}

/// Rumble pads: screen shake and controller rumble.
fn fx_rumble(g: &mut BlackTap, kind: &str) {
    let (shake, rumble) = match kind {
        "tap" => (0.1, (0.15, 0.4, 60)),
        "broadside" => (0.3, (0.9, 0.4, 120)),
        "hull" => (0.45, (0.7, 0.6, 200)),
        _ => (0.7, (1.0, 0.9, 450)),
    };
    // The game keeps at most eight cues and nothing drains them yet: each pad shows its own.
    g.rumble.clear();
    g.shake(shake, rumble);
    g.toast(&format!("Rumble: {}", pretty(kind)));
}

// ---------------------------------------------------------------- presentation

/// The camera bench's context, if one is on: (context, target, ignored by the spring arm).
pub fn camera_context(g: &BlackTap, sim: &Sim) -> Option<(&'static str, Vec3, Option<EntityId>)> {
    let name = g.dock.camera.as_deref()?;
    if g.body != Body::Foot {
        return None;
    }
    let ctx = CONTEXTS.iter().copied().find(|c| *c == name)?;
    Some((ctx, BlackTap::player_feet(sim)?, sim.state.player))
}

/// The jukebox's ambience mixer: one layer alone.
pub fn audio(g: &BlackTap, a: &mut AudioState) {
    let layers = ambience_layers();
    if let Some(solo) = g.dock.mix.checked_sub(1).and_then(|i| layers.get(i)) {
        a.ambience = layers.iter().map(|l| (l.clone(), if l == solo { 1.0 } else { 0.0 })).collect();
    }
}

fn kind_color(kind: &str) -> &'static str {
    match kind {
        "sea" => "#1f4e66",
        "traversal" => "#2f5a2c",
        "combat" => "#6e2222",
        _ => "#5a3b24",
    }
}

fn pad_color(kind: &str) -> &'static str {
    match kind {
        "sea" => "#3fa7d6",
        "traversal" => "#7ccf5a",
        "combat" => "#e0584a",
        _ => "#e8a948",
    }
}

fn kind_label(kind: &str) -> &'static str {
    match kind {
        "sea" => "SEA",
        "traversal" => "TRAVERSAL",
        "combat" => "COMBAT AND STEALTH",
        _ => "PRESENTATION",
    }
}

/// A sign's text yaw (degrees, for `DrawItem::Text`), its board's rotation and front normal.
fn face(facing: f32) -> (f32, Quat, Vec3) {
    let r = facing.to_radians();
    let yaw = (facing - 180.0).rem_euclid(360.0);
    (yaw, Quat::from_rotation_y(-yaw.to_radians()), Vec3::new(r.sin(), 0.0, -r.cos()))
}

fn prim(out: &mut DrawList, shape: Shape, pos: Vec3, rot: Quat, color: &str, emissive: f32) {
    out.push(DrawItem::Prim { shape, pos, rot, color: hex(color), emissive, look: Look::Lit });
}

/// Text on both faces of a board (each reads the right way round from its side).
#[allow(clippy::too_many_arguments)]
fn text2(out: &mut DrawList, text: &str, center: Vec3, normal: Vec3, yaw: f32, size: f32, color: &str, gap: f32) {
    out.push(DrawItem::Text { pos: center + normal * gap, text: text.into(), size, color: hex(color), yaw });
    out.push(DrawItem::Text {
        pos: center - normal * gap,
        text: text.into(),
        size,
        color: hex(color),
        yaw: (yaw + 180.0).rem_euclid(360.0),
    });
}

/// A plank sign on two posts (land) or a big board on piles in the water (sea).
fn sign(out: &mut DrawList, s: &Station) {
    let (yaw, rot, n) = face(s.facing);
    let title = s.name.to_uppercase();
    let chars = title.chars().count() as f32;
    let right = rot * Vec3::X;
    let (size, width, h, mid, post_r, foot) = if s.afloat {
        (1.6, chars * 1.6 * 0.62 + 2.4, 3.4, 8.0, 0.32, -4.0)
    } else {
        (0.5, (chars * 0.5 * 0.62 + 1.0).max(2.6), 1.3, 2.3, 0.07, s.sign.y - 0.2)
    };
    let c = Vec3::new(s.sign.x, s.sign.y + mid, s.sign.z);
    prim(
        out,
        Shape::Box { half: Vec3::new(width * 0.5, h * 0.5, if s.afloat { 0.12 } else { 0.06 }) },
        c,
        rot,
        kind_color(&s.kind),
        0.0,
    );
    // A gold rim along the top.
    prim(
        out,
        Shape::Box { half: Vec3::new(width * 0.5 + 0.04, h * 0.04, if s.afloat { 0.16 } else { 0.08 }) },
        c + Vec3::Y * h * 0.5,
        rot,
        GOLD,
        0.2,
    );
    let top = c.y + h * 0.5;
    let half = (top - foot) * 0.5;
    for side in [-1.0, 1.0] {
        let p = c + right * side * (width * 0.5 - post_r * 3.0);
        prim(
            out,
            Shape::Cylinder { radius: post_r, half_height: half },
            Vec3::new(p.x, foot + half, p.z),
            Quat::IDENTITY,
            TIMBER,
            0.0,
        );
    }
    let gap = if s.afloat { 0.15 } else { 0.075 };
    text2(out, &title, c + Vec3::Y * h * 0.14, n, yaw, size, CREAM, gap);
    text2(out, kind_label(&s.kind), c - Vec3::Y * h * 0.3, n, yaw, size * 0.4, GOLD, gap);
}

/// A pad: a coloured disc with a ring and a labelled plaque (land), or a ring of buoys on the
/// water with a labelled pole (sea).
fn pad(out: &mut DrawList, p: &Pad, s: &Station, on: bool, tried: bool, ocean: &pav_core::ocean::Ocean) {
    let (yaw, rot, n) = face(s.facing);
    let color = pad_color(&s.kind);
    let rim = if on {
        "#ffffff"
    } else if tried {
        GOLD
    } else {
        "#d8d0c0"
    };
    if p.afloat {
        let y = ocean.height(flat(p.at)) + 0.15;
        let c = Vec3::new(p.at.x, y, p.at.z);
        out.push(DrawItem::Decal { center: c, radius: p.radius, ring: 0.92, color: hex(color), alpha: 1.0 });
        out.push(DrawItem::Decal { center: c + Vec3::Y * 0.02, radius: p.radius * 0.8, ring: 0.95, color: hex(rim), alpha: 1.0 });
        for k in 0..6 {
            let a = k as f32 / 6.0 * std::f32::consts::TAU;
            let q = p.at + Vec3::new(a.cos(), 0.0, a.sin()) * p.radius;
            let qy = ocean.height(flat(q));
            prim(
                out,
                Shape::Cylinder { radius: 0.35, half_height: 0.45 },
                Vec3::new(q.x, qy + 0.2, q.z),
                Quat::IDENTITY,
                "#ff8a2a",
                0.3,
            );
        }
        let post = p.at + rot * Vec3::X * (p.radius + 1.0);
        prim(
            out,
            Shape::Cylinder { radius: 0.15, half_height: 3.0 },
            Vec3::new(post.x, 1.0, post.z),
            Quat::IDENTITY,
            TIMBER,
            0.0,
        );
        let w = p.label.chars().count() as f32 * 0.6 + 1.0;
        let pc = Vec3::new(post.x, 4.6, post.z);
        prim(out, Shape::Box { half: Vec3::new(w * 0.5, 0.6, 0.06) }, pc, rot, "#2a2018", 0.0);
        text2(out, &p.label, pc, n, yaw, 0.95, CREAM, 0.08);
        return;
    }
    let c = p.at + Vec3::Y * 0.05;
    out.push(DrawItem::Decal { center: c, radius: p.radius, ring: 0.0, color: hex(color), alpha: if on { 1.0 } else { 0.7 } });
    out.push(DrawItem::Decal { center: c + Vec3::Y * 0.01, radius: p.radius + 0.12, ring: 0.84, color: hex(rim), alpha: 1.0 });
    let post = p.at - n * (p.radius + 0.25);
    prim(out, Shape::Cylinder { radius: 0.04, half_height: 0.75 }, post + Vec3::Y * 0.75, Quat::IDENTITY, TIMBER, 0.0);
    let w = p.label.chars().count() as f32 * 0.24 * 0.6 + 0.35;
    let pc = post + Vec3::Y * 1.62;
    prim(out, Shape::Box { half: Vec3::new(w * 0.5, 0.19, 0.025) }, pc, rot, "#2a2018", 0.0);
    text2(out, &p.label, pc, n, yaw, 0.24, CREAM, 0.035);
}

/// Signs, pads and the stations' props: lane buoys, ring ropes, the map table, the whistle
/// spot, vision cones and strike tells.
pub fn draw(g: &BlackTap, sim: &Sim, out: &mut DrawList) {
    let st = stations(g);
    let ocean = g.ocean();
    for s in &st.list {
        sign(out, s);
    }
    for (i, p) in st.pads.iter().enumerate() {
        let on = g.dock.on_pad == Some(i) || g.dock.ship_pad == Some(i);
        pad(out, p, &st.list[p.station], on, g.dock.fired.contains_key(&p.effect), &ocean);
    }
    let w = g.world();
    // The sailing lane: a red and a green buoy at each gate.
    for c in w.route("sailing_lane") {
        for (side, color) in [(-16.0, "#c0392b"), (16.0, "#2e8b57")] {
            let q = c + Vec2::new(side, 0.0);
            let y = ocean.height(q);
            prim(
                out,
                Shape::Cylinder { radius: 0.7, half_height: 1.0 },
                Vec3::new(q.x, y + 0.5, q.y),
                Quat::IDENTITY,
                color,
                0.15,
            );
            prim(out, Shape::Sphere { radius: 0.5 }, Vec3::new(q.x, y + 1.9, q.y), Quat::IDENTITY, color, 0.6);
        }
    }
    // The combat ring's ropes (open to the east, where the pads are).
    if let Some(rc) = w.marker("ring_center") {
        let r = 8.5;
        let post = |k: i32| {
            let a = (22.5 + k as f32 * 45.0).to_radians();
            rc + Vec3::new(a.cos() * r, 0.0, a.sin() * r)
        };
        for k in 0..8 {
            prim(out, Shape::Cylinder { radius: 0.1, half_height: 0.7 }, post(k) + Vec3::Y * 0.7, Quat::IDENTITY, TIMBER, 0.0);
            if k == 7 {
                continue;
            }
            for h in [0.6, 1.15] {
                out.push(DrawItem::Line {
                    a: post(k) + Vec3::Y * h,
                    b: post(k + 1) + Vec3::Y * h,
                    radius: 0.03,
                    color: hex("#b99b6b"),
                    emissive: 0.0,
                });
            }
        }
    }
    // The map table on the quay.
    if let Some(m) = w.marker("map_table") {
        prim(out, Shape::Box { half: Vec3::new(0.9, 0.05, 0.6) }, m + Vec3::Y * 0.92, Quat::IDENTITY, "#6b4a2f", 0.0);
        prim(out, Shape::Box { half: Vec3::new(0.75, 0.01, 0.48) }, m + Vec3::Y * 0.98, Quat::IDENTITY, "#e9dcb8", 0.1);
        for (x, z) in [(-0.8, -0.5), (0.8, -0.5), (-0.8, 0.5), (0.8, 0.5)] {
            prim(out, Shape::Box { half: Vec3::new(0.05, 0.45, 0.05) }, m + Vec3::new(x, 0.45, z), Quat::IDENTITY, TIMBER, 0.0);
        }
    }
    // The stealth yard's whistle spot.
    if let Some(m) = w.marker("whistle") {
        out.push(DrawItem::Decal { center: m + Vec3::Y * 0.05, radius: 1.1, ring: 0.75, color: hex("#9fd3ff"), alpha: 1.0 });
    }
    if !g.dock.hide_cones {
        cones(g, sim, out);
    }
    tells(g, sim, out);
}

/// The stealth yard guards' sight: the cone to `far`, an arc at `near`, coloured by awareness.
fn cones(g: &BlackTap, sim: &Sim, out: &mut DrawList) {
    for (&id, n) in &g.stealth.npcs {
        if n.group != "yard" || n.out() {
            continue;
        }
        let Some(feet) = sim.feet(id) else { continue };
        let facing = sim.state.entities.get(id).and_then(|e| e.character.as_ref()).map(|c| c.facing).unwrap_or(n.home_facing);
        let color = hex(match n.mind.state {
            Awareness::Unaware => "#f2ead0",
            Awareness::Suspicious | Awareness::Investigating => "#ffd24a",
            Awareness::Searching => "#ff9a2e",
            _ => "#e0281e",
        });
        let base = feet + Vec3::Y * 0.15;
        let half = (n.senses.fov * 0.5).to_radians();
        let dir = |a: f32| Vec3::new((facing + a).sin(), 0.0, (facing + a).cos());
        let line = |out: &mut DrawList, a: Vec3, b: Vec3| out.push(DrawItem::Line { a, b, radius: 0.04, color, emissive: 1.2 });
        line(out, base, base + dir(-half) * n.senses.far);
        line(out, base, base + dir(half) * n.senses.far);
        for r in [n.senses.near, n.senses.far] {
            let k = 12;
            for i in 0..k {
                let a0 = -half + 2.0 * half * i as f32 / k as f32;
                let a1 = -half + 2.0 * half * (i + 1) as f32 / k as f32;
                line(out, base + dir(a0) * r, base + dir(a1) * r);
            }
        }
    }
}

/// The ring's strike tells: a red ring that fills as the wind-up runs.
fn tells(g: &BlackTap, sim: &Sim, out: &mut DrawList) {
    for (id, f) in &g.land.fighters {
        if g.stealth.npcs.get(id).is_none_or(|n| n.group != "ring") {
            continue;
        }
        let (Some(s), Some(feet)) = (f.swing, sim.feet(*id)) else { continue };
        if s.t >= s.windup {
            continue;
        }
        let k = (s.t / s.windup.max(0.01)).clamp(0.0, 1.0);
        out.push(DrawItem::Decal { center: feet + Vec3::Y * 0.05, radius: 1.3, ring: 0.85, color: hex("#e0281e"), alpha: 1.0 });
        out.push(DrawItem::Decal {
            center: feet + Vec3::Y * 0.06,
            radius: 1.3 * k,
            ring: 0.0,
            color: hex("#ffcc33"),
            alpha: 0.6,
        });
    }
}

/// The overlay a UI gallery pad shows, with sample data.
fn sample_overlay(g: &BlackTap, sim: &Sim, kind: &str) -> Option<Overlay> {
    Some(match kind {
        "tiles" => {
            let (title, subtitle, tiles) = crate::foot::plunder_tiles(1);
            Overlay::Tiles { title, subtitle, tiles, focus: 1 }
        }
        "wheel" => Overlay::Wheel {
            slots: Tool::ALL
                .iter()
                .map(|t| Tile {
                    name: t.name().into(),
                    desc: "A sample of the tool wheel".into(),
                    cost: format!("×{}", g.land.hero.tools.count(*t).min(99)),
                    icon: format!("{t:?}").to_lowercase(),
                    disabled: g.land.hero.tools.count(*t) == 0,
                })
                .collect(),
            selected: 0,
        },
        "results" => Overlay::Results(Results {
            title: "TAB I — THE DRY RUN (SAMPLE)".into(),
            pour: 82.0,
            checklist: vec![
                ("Collect all three flotsam crates".into(), true),
                ("Disable the Restraint with chain shot".into(), true),
                ("Silence the mortar first".into(), false),
                ("Air-assassinate the courier".into(), true),
            ],
            stats: vec![
                ("Time".into(), "41:07".into()),
                ("Reales".into(), "1250".into()),
                ("Collectibles".into(), "14 / 26".into()),
                ("Assassinations".into(), "17".into()),
                ("Ships sunk".into(), "6".into()),
                ("Times detected".into(), "2".into()),
            ],
            tiles: ["Continue", "Replay a Round", "Title"]
                .iter()
                .zip(["sail", "repair", "keg"])
                .map(|(n, i)| Tile {
                    name: (*n).into(),
                    desc: String::new(),
                    cost: String::new(),
                    icon: i.into(),
                    disabled: false,
                })
                .collect(),
            focus: 0,
        }),
        "map" => Overlay::Map { cursor: BlackTap::player_feet(sim).map(flat).unwrap_or_default(), zoom: 2.0 },
        "cinematic" => Overlay::Cinematic { skip: 0.35 },
        _ => return None,
    })
}

/// The station guide, the pad underfoot, sea stations on the HUD, the gallery's overlay.
pub fn ui(g: &BlackTap, sim: &Sim, ui: &mut UiModel) {
    let st = stations(g);
    if let Some(s) = g.dock.station.as_deref().and_then(|k| st.by_key(k)) {
        ui.objective = Some(Objective {
            title: s.name.to_uppercase(),
            text: s.learn.clone(),
            optional: s.pads.iter().map(|&i| (st.pads[i].label.clone(), g.dock.fired.contains_key(&st.pads[i].effect))).collect(),
            age: g.time - g.dock.since,
        });
    }
    if let Some(p) = g.dock.on_pad.or(g.dock.ship_pad).and_then(|i| st.pads.get(i)) {
        ui.prompts.push(Prompt::new("interact", p.label.clone()));
    }
    if g.body == Body::Helm {
        for s in st.list.iter().filter(|s| s.afloat && g.dock.station.as_deref() != Some(s.key.as_str())) {
            ui.markers.push(WorldMarker {
                pos: s.sign + Vec3::Y * 12.0,
                kind: MarkerKind::Waypoint,
                label: s.name.clone(),
                bar: None,
                distance: true,
            });
        }
    }
    if g.dock.slowmo {
        ui.hud.push(HudItem::Counter { icon: "clock".into(), label: "Slow motion".into(), value: "on".into() });
    }
    if let Some(o) = g.dock.overlay.as_deref().and_then(|k| sample_overlay(g, sim, k)) {
        ui.overlay = Some(o);
    }
}
