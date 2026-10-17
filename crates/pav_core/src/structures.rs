//! Structures: parametric, deterministic, game-agnostic generators for a colonial harbour town,
//! a stone fort and docks (`[[structure]]` in a level).
//!
//! Each generator emits axis-aligned blocks (quarter-turn yaws only, so collision and climbing
//! stay simple), ladders, decor, baked props, labels, gameplay areas (soft landings, perches,
//! hiding spots), gates that open and close, and named markers (doors, perches, cannon spots,
//! bells, flagpoles…) for game code. Blocks carry `block_flags::CLIMBABLE` (free-climbable
//! walls), `ROOF` (walkable tops), `POLE` and `BEAM`. Foundations fill down to the terrain
//! under the footprint (`ground`).
//!
//! ```toml
//! [[structure]]
//! kind = "house"      # see `Kind`
//! name = "casa_azul"  # marker prefix (default: kind + index)
//! at = [12, -30]      # footprint centre (x, z)
//! yaw = 90            # 0, 90, 180, 270: the front (+z locally) turned clockwise from south
//! size = [8, 6]       # width (local x) and depth (local z), m
//! floors = 2
//! style = "coral"     # ochre coral seafoam sky lime limestone timber
//! seed = 3
//! balcony = "front"   # front back left right none
//! awning = true
//! ```
//! Lines between roofs (`laundry_line`, `beam`) take `at` and `to` and find the roofs below
//! their ends among the structures listed before them.

use std::collections::BTreeMap;

use glam::{Quat, Vec2, Vec3};
use serde::{Deserialize, Serialize};

use crate::color::Color;
use crate::physics::PhysicsState;
use crate::rng::hash_f32;
use crate::shape::{Look, Shape, Sway};
use crate::statics::{Block, BlockRef, Decor, Facing, Ladder, Pole, RegionKey, StaticProp, StaticWorld, block_flags};
use crate::zones::{Label, LabelMode};

use block_flags::{BEAM, CLIMBABLE, POLE, ROOF};

/// Storey height (m).
pub const FLOOR: f32 = 3.2;
/// Parapet height above a flat roof (m).
pub const PARAPET: f32 = 0.6;
/// Height of the stone plinth a building stands on (m).
pub const PLINTH: f32 = 0.25;

const MAT_PLASTER: &str = "MAT/plaster";
const MAT_LIMESTONE: &str = "MAT/limestone";
const MAT_TERRACOTTA: &str = "MAT/terracotta";
const MAT_CANVAS: &str = "MAT/canvas";
const MAT_THATCH: &str = "MAT/thatch";
const MAT_ROPE: &str = "MAT/rope";
const MAT_PLANKS: &str = "MAT/oak_planks";
const MAT_STRIPES: &str = "MAT/awning_stripes";
const MAT_IRON: &str = "MAT/wrought_iron";
const MAT_SAND: &str = "MAT/sand";

/// Every structure kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    House,
    Chapel,
    Warehouse,
    Hall,
    Tavern,
    Stall,
    Stocks,
    HayCart,
    Pier,
    Quay,
    HarbourWall,
    FortWall,
    Bastion,
    Battery,
    MortarTower,
    Keep,
    Gate,
    Barracks,
    Lighthouse,
    WatchtowerRuin,
    Hut,
    LaundryLine,
    Beam,
    Palisade,
    CrateStack,
    WantedPoster,
    /// Cobbled paving (plazas, streets): a thin walkable slab on the ground.
    Paving,
}

impl Kind {
    pub const ALL: [Kind; 27] = [
        Kind::House,
        Kind::Chapel,
        Kind::Warehouse,
        Kind::Hall,
        Kind::Tavern,
        Kind::Stall,
        Kind::Stocks,
        Kind::HayCart,
        Kind::Pier,
        Kind::Quay,
        Kind::HarbourWall,
        Kind::FortWall,
        Kind::Bastion,
        Kind::Battery,
        Kind::MortarTower,
        Kind::Keep,
        Kind::Gate,
        Kind::Barracks,
        Kind::Lighthouse,
        Kind::WatchtowerRuin,
        Kind::Hut,
        Kind::LaundryLine,
        Kind::Beam,
        Kind::Palisade,
        Kind::CrateStack,
        Kind::WantedPoster,
        Kind::Paving,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Kind::House => "house",
            Kind::Chapel => "chapel",
            Kind::Warehouse => "warehouse",
            Kind::Hall => "hall",
            Kind::Tavern => "tavern",
            Kind::Stall => "stall",
            Kind::Stocks => "stocks",
            Kind::HayCart => "hay_cart",
            Kind::Pier => "pier",
            Kind::Quay => "quay",
            Kind::HarbourWall => "harbour_wall",
            Kind::FortWall => "fort_wall",
            Kind::Bastion => "bastion",
            Kind::Battery => "battery",
            Kind::MortarTower => "mortar_tower",
            Kind::Keep => "keep",
            Kind::Gate => "gate",
            Kind::Barracks => "barracks",
            Kind::Lighthouse => "lighthouse",
            Kind::WatchtowerRuin => "watchtower_ruin",
            Kind::Hut => "hut",
            Kind::LaundryLine => "laundry_line",
            Kind::Beam => "beam",
            Kind::Palisade => "palisade",
            Kind::CrateStack => "crate_stack",
            Kind::WantedPoster => "wanted_poster",
            Kind::Paving => "paving",
        }
    }

    /// Default footprint (width, depth) in metres.
    pub fn default_size(self) -> [f32; 2] {
        match self {
            Kind::House | Kind::Tavern => [8.0, 6.0],
            Kind::Chapel => [10.0, 18.0],
            Kind::Warehouse => [14.0, 10.0],
            Kind::Hall => [12.0, 10.0],
            Kind::Stall => [3.0, 2.0],
            Kind::Stocks => [2.4, 1.4],
            Kind::HayCart => [2.0, 3.2],
            Kind::Pier => [4.0, 24.0],
            Kind::Quay => [20.0, 6.0],
            Kind::HarbourWall => [30.0, 3.0],
            Kind::FortWall => [24.0, 3.0],
            Kind::Bastion => [12.0, 12.0],
            Kind::Battery => [14.0, 8.0],
            Kind::MortarTower => [8.0, 8.0],
            Kind::Keep => [16.0, 12.0],
            Kind::Gate => [10.0, 6.0],
            Kind::Barracks => [16.0, 8.0],
            Kind::Lighthouse => [6.0, 6.0],
            Kind::WatchtowerRuin => [5.0, 5.0],
            Kind::Hut => [4.0, 4.0],
            Kind::LaundryLine | Kind::Beam => [6.0, 0.3],
            Kind::Palisade => [12.0, 0.4],
            Kind::CrateStack => [2.4, 2.4],
            Kind::WantedPoster => [0.8, 0.2],
            Kind::Paving => [12.0, 12.0],
        }
    }
}

/// A side of a structure in its own frame (front = local +z).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    Front,
    Back,
    Left,
    Right,
    None,
}

/// One structure as written in a level (`[[structure]]`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StructureDef {
    pub kind: Kind,
    /// Marker prefix; default `<kind><index>`.
    #[serde(default)]
    pub name: String,
    /// Footprint centre (x, z), or a line's start.
    pub at: [f32; 2],
    /// 0, 90, 180 or 270 (other values snap to the nearest quarter turn).
    #[serde(default)]
    pub yaw: f32,
    /// Width (local x) and depth (local z), m.
    #[serde(default)]
    pub size: Option<[f32; 2]>,
    #[serde(default)]
    pub floors: Option<u32>,
    /// A palette entry (`STYLES`): ochre coral seafoam sky lime limestone timber.
    #[serde(default)]
    pub style: Option<String>,
    #[serde(default)]
    pub seed: u64,
    /// Floor level (y) instead of the highest ground under the footprint; for piers, quays
    /// and harbour walls the deck height.
    #[serde(default)]
    pub base: Option<f32>,
    /// Main height (walls, towers), m above the base.
    #[serde(default)]
    pub height: Option<f32>,
    /// A line's end (laundry_line, beam).
    #[serde(default)]
    pub to: Option<[f32; 2]>,
    #[serde(default)]
    pub balcony: Option<Side>,
    #[serde(default)]
    pub awning: Option<bool>,
    /// Gates: start open.
    #[serde(default)]
    pub open: bool,
    /// Text of a sign or label on the building.
    #[serde(default)]
    pub sign: Option<String>,
    /// Battery cannons (3–4).
    #[serde(default)]
    pub cannons: Option<u32>,
    /// Fort walls: inner stairs at this end (left, right; none).
    #[serde(default)]
    pub stairs: Option<Side>,
    /// Pitched (visual) roof instead of a flat walkable one, where the kind allows.
    #[serde(default)]
    pub pitched: Option<bool>,
}

impl StructureDef {
    pub fn new(kind: Kind, at: [f32; 2]) -> Self {
        Self {
            kind,
            name: String::new(),
            at,
            yaw: 0.0,
            size: None,
            floors: None,
            style: None,
            seed: 0,
            base: None,
            height: None,
            to: None,
            balcony: None,
            awning: None,
            open: false,
            sign: None,
            cannons: None,
            stairs: None,
            pitched: None,
        }
    }
}

/// A palette entry: wall, trim and accent (shutters, doors) colours.
#[derive(Clone, Copy, Debug)]
pub struct Style {
    pub name: &'static str,
    pub wall: &'static str,
    pub trim: &'static str,
    pub accent: &'static str,
    /// The wall material.
    pub mat: &'static str,
}

/// The styles (§9.4): plaster colours, stone and timber.
pub const STYLES: &[Style] = &[
    Style { name: "ochre", wall: "#d9a35b", trim: "#efe6d2", accent: "#2f6b8a", mat: MAT_PLASTER },
    Style { name: "coral", wall: "#e07a5f", trim: "#efe6d2", accent: "#2f7d6b", mat: MAT_PLASTER },
    Style { name: "seafoam", wall: "#8fc1a9", trim: "#efe6d2", accent: "#8a4a2f", mat: MAT_PLASTER },
    Style { name: "sky", wall: "#7fb7d6", trim: "#efe6d2", accent: "#c98a2b", mat: MAT_PLASTER },
    Style { name: "lime", wall: "#efe6d2", trim: "#d9c9a8", accent: "#3f6f3a", mat: MAT_PLASTER },
    Style { name: "limestone", wall: "#cbbd9b", trim: "#e2d6ba", accent: "#6b4a2f", mat: MAT_LIMESTONE },
    Style { name: "timber", wall: "#6b4a2f", trim: "#8a6a48", accent: "#b5543a", mat: MAT_PLANKS },
];

const PLASTERS: [&str; 5] = ["ochre", "coral", "seafoam", "sky", "lime"];
const TIMBER: &str = "#6b4a2f";
const TIMBER_DARK: &str = "#4a3320";
const TERRACOTTA: &str = "#b5543a";
const LIMESTONE: &str = "#cbbd9b";
const STONE_DARK: &str = "#a39578";
const IRON: &str = "#2e2c2a";
const ROPE: &str = "#b99b6b";
const GLASS: &str = "#2a3138";

pub fn style(name: &str) -> Option<Style> {
    STYLES.iter().copied().find(|s| s.name == name)
}

/// What kind of gameplay area.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AreaKind {
    /// Falls into it do no damage (hay carts, awnings, deep water).
    Soft,
    /// Hides a character inside it (stalk grass, hay).
    Hide,
    /// A high viewpoint (survey; leap from here).
    Perch,
}

/// A gameplay area: a box (and, for grass, the polygon inside it).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Area {
    pub name: String,
    pub kind: AreaKind,
    pub min: Vec3,
    pub max: Vec3,
    /// x, z outline when the area is not the whole box (stalk grass).
    #[serde(default)]
    pub polygon: Vec<[f32; 2]>,
}

impl Area {
    pub fn contains(&self, p: Vec3) -> bool {
        let inside_box = p.cmpge(self.min).all() && p.cmple(self.max).all();
        if !inside_box || self.polygon.len() < 3 {
            return inside_box;
        }
        let poly: Vec<Vec2> = self.polygon.iter().map(|q| Vec2::new(q[0], q[1])).collect();
        crate::island::point_in_polygon(Vec2::new(p.x, p.z), &poly)
    }
    pub fn center(&self) -> Vec3 {
        (self.min + self.max) * 0.5
    }
}

/// A gate as generated: indices into `StructureOut::blocks` of the leaves drawn when closed
/// and when open.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GateDef {
    pub name: String,
    pub closed: Vec<usize>,
    pub open: Vec<usize>,
    pub start_open: bool,
}

/// A gate placed in the world: toggle it with `set_gate`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct GateRefs {
    pub closed: Vec<BlockRef>,
    pub open: Vec<BlockRef>,
    pub is_open: bool,
}

/// Opens or closes a gate: the leaves of the other state go (no collider, not drawn) and this
/// state's come back.
pub fn set_gate(statics: &mut StaticWorld, physics: &mut PhysicsState, gate: &mut GateRefs, open: bool) {
    let (show, hide) = if open { (&gate.open, &gate.closed) } else { (&gate.closed, &gate.open) };
    for r in hide {
        statics.destroy(physics, *r);
    }
    for r in show {
        statics.restore(physics, *r);
    }
    gate.is_open = open;
}

/// A footprint on the ground (vegetation keeps clear of it).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Footprint {
    pub name: String,
    pub min: Vec2,
    pub max: Vec2,
}

impl Footprint {
    pub fn distance(&self, p: Vec2) -> f32 {
        (self.min - p).max(p - self.max).max(Vec2::ZERO).length()
    }
}

/// What generators emit.
#[derive(Clone, Debug, Default)]
pub struct StructureOut {
    pub blocks: Vec<Block>,
    pub ladders: Vec<Ladder>,
    pub decor: Vec<Decor>,
    pub props: Vec<StaticProp>,
    pub labels: Vec<Label>,
    pub areas: Vec<Area>,
    /// `<name>.<marker>` → world position.
    pub markers: BTreeMap<String, Vec3>,
    pub gates: Vec<GateDef>,
    pub poles: Vec<Pole>,
    pub footprints: Vec<Footprint>,
}

impl StructureOut {
    pub fn merge(&mut self, mut o: StructureOut) {
        let n = self.blocks.len();
        for g in &mut o.gates {
            g.closed.iter_mut().for_each(|i| *i += n);
            g.open.iter_mut().for_each(|i| *i += n);
        }
        self.blocks.append(&mut o.blocks);
        self.ladders.append(&mut o.ladders);
        self.decor.append(&mut o.decor);
        self.props.append(&mut o.props);
        self.labels.append(&mut o.labels);
        self.areas.append(&mut o.areas);
        self.markers.append(&mut o.markers);
        self.gates.append(&mut o.gates);
        self.poles.append(&mut o.poles);
        self.footprints.append(&mut o.footprints);
    }

    /// Adds everything to the static world; returns the gates by name.
    pub fn place(&self, statics: &mut StaticWorld, physics: &mut PhysicsState) -> BTreeMap<String, GateRefs> {
        let refs: Vec<BlockRef> = self.blocks.iter().map(|b| statics.add(physics, b.clone())).collect();
        for l in &self.ladders {
            statics.add_ladder(l.clone());
        }
        for d in &self.decor {
            statics.add_decor(physics, RegionKey::chunk_of(d.pos), d.clone());
        }
        for p in &self.props {
            if let Err(e) = statics.add_prop(physics, p.clone()) {
                log::warn!("structure prop '{}': {e}", p.asset);
            }
        }
        for l in &self.labels {
            statics.add_label_to(RegionKey::chunk_of(l.pos), l.clone());
        }
        for p in &self.poles {
            statics.add_pole(physics, p.clone());
        }
        let mut gates = BTreeMap::new();
        for g in &self.gates {
            let mut r = GateRefs {
                closed: g.closed.iter().map(|i| refs[*i]).collect(),
                open: g.open.iter().map(|i| refs[*i]).collect(),
                is_open: !g.start_open,
            };
            set_gate(statics, physics, &mut r, g.start_open);
            gates.insert(g.name.clone(), r);
        }
        gates
    }

    /// Counts for reports: blocks, decor, props, ladders.
    pub fn counts(&self) -> (usize, usize, usize, usize) {
        (self.blocks.len(), self.decor.len(), self.props.len(), self.ladders.len())
    }
}

// ---------------------------------------------------------------------------------------------
// The builder: a local frame (x across, y up, z toward the front) turned by quarter turns.

struct B<'a> {
    origin: Vec3,
    q: u8,
    name: String,
    seed: u64,
    out: StructureOut,
    ground: &'a dyn Fn(Vec2) -> f32,
    /// Blocks of earlier structures (lines between roofs look for roofs among them).
    existing: &'a [Block],
}

fn hex(s: &str) -> Color {
    Color::hex(s)
}

impl B<'_> {
    fn rot(&self, v: Vec3) -> Vec3 {
        let mut v = v;
        for _ in 0..self.q {
            v = Vec3::new(-v.z, v.y, v.x);
        }
        v
    }
    fn p(&self, local: Vec3) -> Vec3 {
        self.origin + self.rot(local)
    }
    fn quat(&self) -> Quat {
        Quat::from_rotation_y(-(self.q as f32) * std::f32::consts::FRAC_PI_2)
    }
    fn facing(&self, f: Facing) -> Facing {
        f.rotated(self.q)
    }
    fn r(&self, k: u32) -> f32 {
        hash_f32(self.seed, k as i32, 0x57, 11)
    }
    fn pick<'s>(&self, k: u32, list: &[&'s str]) -> &'s str {
        list[((self.r(k) * list.len() as f32) as usize).min(list.len() - 1)]
    }
    fn ground_at(&self, local: Vec3) -> f32 {
        let w = self.p(local);
        (self.ground)(Vec2::new(w.x, w.z))
    }

    /// An axis-aligned block from local corners.
    fn block(&mut self, a: Vec3, b: Vec3, color: &str, flags: u32, mat: Option<&str>) -> usize {
        let (wa, wb) = (self.p(a), self.p(b));
        let mut blk = Block::new(wa.min(wb), wa.max(wb), hex(color)).with_flags(flags);
        if let Some(m) = mat {
            blk = blk.with_material(m);
        }
        self.out.blocks.push(blk);
        self.out.blocks.len() - 1
    }
    /// A climbable wall mass with a walkable top.
    fn mass(&mut self, a: Vec3, b: Vec3, color: &str, mat: &str) -> usize {
        self.block(a, b, color, CLIMBABLE | ROOF, Some(mat))
    }
    fn decor(&mut self, shape: Shape, pos: Vec3, rot: Quat, color: &str, solid: bool, mat: Option<&str>) {
        let d = Decor {
            shape,
            pos: self.p(pos),
            rot: self.quat() * rot,
            color: hex(color),
            look: Look::Cel,
            emissive: 0.0,
            solid,
            collider: None,
            sway: Sway::None,
            material: mat.map(str::to_owned),
        };
        self.out.decor.push(d);
    }
    fn decor_box(&mut self, center: Vec3, size: Vec3, color: &str, mat: Option<&str>) {
        self.decor(Shape::Box { half: size * 0.5 }, center, Quat::IDENTITY, color, false, mat);
    }
    fn glow(&mut self, shape: Shape, pos: Vec3, color: &str, emissive: f32) {
        self.decor(shape, pos, Quat::IDENTITY, color, false, None);
        if let Some(d) = self.out.decor.last_mut() {
            d.emissive = emissive;
            d.look = Look::Unlit;
        }
    }
    fn sway_last(&mut self, sway: Sway) {
        if let Some(d) = self.out.decor.last_mut() {
            d.sway = sway;
        }
    }
    fn prop(&mut self, asset: &str, pos: Vec3, yaw_deg: f32, scale: f32, seed: u32, solid: bool) {
        let rot = self.quat() * Quat::from_rotation_y(-yaw_deg.to_radians());
        self.out.props.push(StaticProp::new(asset, self.p(pos), rot, scale, seed, solid));
    }
    fn marker(&mut self, name: &str, local: Vec3) {
        let p = self.p(local);
        self.out.markers.insert(format!("{}.{name}", self.name), p);
    }
    fn area(&mut self, name: &str, kind: AreaKind, a: Vec3, b: Vec3) {
        let (wa, wb) = (self.p(a), self.p(b));
        self.out.areas.push(Area {
            name: format!("{}.{name}", self.name),
            kind,
            min: wa.min(wb),
            max: wa.max(wb),
            polygon: Vec::new(),
        });
    }
    /// A ladder against a face: `facing` is the local direction from the climber to the wall.
    fn ladder(&mut self, a: Vec3, b: Vec3, facing: Facing) {
        let (wa, wb) = (self.p(a), self.p(b));
        self.out.ladders.push(Ladder { min: wa.min(wb), max: wa.max(wb), facing: self.facing(facing), color: hex("#7a5232") });
    }
    fn label(&mut self, text: &str, local: Vec3, size: f32, facing: Facing, color: &str) {
        self.out.labels.push(Label {
            text: text.to_owned(),
            pos: self.p(local),
            size,
            color: hex(color),
            mode: LabelMode::Wall,
            facing: self.facing(facing),
        });
    }
    fn footprint(&mut self, w: f32, d: f32) {
        let (a, b) = (self.p(Vec3::new(-w * 0.5, 0.0, -d * 0.5)), self.p(Vec3::new(w * 0.5, 0.0, d * 0.5)));
        self.out.footprints.push(Footprint {
            name: self.name.clone(),
            min: Vec2::new(a.x.min(b.x), a.z.min(b.z)),
            max: Vec2::new(a.x.max(b.x), a.z.max(b.z)),
        });
    }

    /// The lowest ground under a local footprint (x, z centred on `c`).
    fn lowest(&self, c: Vec2, w: f32, d: f32) -> f32 {
        let mut lo = f32::MAX;
        let (nx, nz) = ((w.ceil() as i32).max(1), (d.ceil() as i32).max(1));
        for k in 0..=nz {
            for i in 0..=nx {
                let l = Vec3::new(c.x - w * 0.5 + w * i as f32 / nx as f32, 0.0, c.y - d * 0.5 + d * k as f32 / nz as f32);
                lo = lo.min(self.ground_at(l));
            }
        }
        lo
    }

    /// A stone foundation under a local footprint, from below the lowest ground up to `top`
    /// (local y), so the building never floats on a slope.
    fn foundation(&mut self, c: Vec2, w: f32, d: f32, top: f32, color: &str) {
        let low = self.lowest(c, w, d) - self.origin.y - 0.6;
        if low < top {
            self.block(
                Vec3::new(c.x - w * 0.5, low, c.y - d * 0.5),
                Vec3::new(c.x + w * 0.5, top, c.y + d * 0.5),
                color,
                CLIMBABLE | ROOF,
                Some(MAT_LIMESTONE),
            );
        }
    }

    /// The top of the highest roof block among earlier structures under world point `p`.
    fn roof_below(&self, p: Vec2) -> Option<f32> {
        self.existing
            .iter()
            .filter(|b| {
                b.has(ROOF) && p.x >= b.min.x - 0.4 && p.x <= b.max.x + 0.4 && p.y >= b.min.z - 0.4 && p.y <= b.max.z + 0.4
            })
            .map(|b| b.max.y)
            .fold(None, |m: Option<f32>, y| Some(m.map_or(y, |m| m.max(y))))
    }
}

// ---------------------------------------------------------------------------------------------
// Shared pieces.

/// A flat roof on a mass `[-w/2, y0] .. [w/2, top]` (local), with a terracotta cap, a parapet
/// and corner spouts. Returns the parapet top.
fn flat_roof(b: &mut B, cx: f32, cz: f32, w: f32, d: f32, top: f32, color: &str, cap: &str) -> f32 {
    let t = 0.25;
    let (x0, x1, z0, z1) = (cx - w * 0.5, cx + w * 0.5, cz - d * 0.5, cz + d * 0.5);
    b.block(Vec3::new(x0 + t, top, z0 + t), Vec3::new(x1 - t, top + 0.08, z1 - t), cap, ROOF, Some(MAT_PLASTER));
    let py = top + PARAPET;
    b.block(Vec3::new(x0, top, z1 - t), Vec3::new(x1, py, z1), color, CLIMBABLE | ROOF, Some(MAT_PLASTER));
    b.block(Vec3::new(x0, top, z0), Vec3::new(x1, py, z0 + t), color, CLIMBABLE | ROOF, Some(MAT_PLASTER));
    b.block(Vec3::new(x0, top, z0 + t), Vec3::new(x0 + t, py, z1 - t), color, CLIMBABLE | ROOF, Some(MAT_PLASTER));
    b.block(Vec3::new(x1 - t, top, z0 + t), Vec3::new(x1, py, z1 - t), color, CLIMBABLE | ROOF, Some(MAT_PLASTER));
    py
}

/// Crenellated stone parapet along a local edge from `a` to `b` (on the x or z axis) at height
/// `y`, merlons on the outside.
fn crenellate(b: &mut B, a: Vec3, e: Vec3, y: f32, thick: f32, outward: Vec3) {
    let dir = (e - a).normalize_or(Vec3::X).abs();
    let len = (e - a).length();
    // The parapet lies inside the edge line: from it, `thick` against `outward`.
    let inward = -outward * thick;
    let lo = a.min(e).min(a.min(e) + inward);
    let hi = a.max(e).max(a.max(e) + inward);
    b.block(lo + Vec3::Y * y, hi + Vec3::Y * (y + 0.6), LIMESTONE, CLIMBABLE | ROOF, Some(MAT_LIMESTONE));
    // Merlons on it, 1.2 m wide with 0.8 m embrasures.
    let n = ((len - 0.4) / 2.0).floor().max(1.0) as i32;
    let step = len / n as f32;
    for i in 0..n {
        let c = a.min(e) + dir * ((i as f32 + 0.5) * step);
        let half = dir * 0.6;
        let (p, q) = ((c - half).min(c - half + inward), (c + half).max(c + half + inward));
        b.block(Vec3::new(p.x, y + 0.6, p.z), Vec3::new(q.x, y + 1.5, q.z), LIMESTONE, CLIMBABLE | ROOF, Some(MAT_LIMESTONE));
    }
}

/// A window on a face: dark opening, two open shutters, a sill. `face` is the local outward
/// normal (±x or ±z); `c` the centre on the wall surface.
fn window(b: &mut B, c: Vec3, face: Vec3, w: f32, h: f32, shutter: &str, trim: &str, shutters: bool) {
    let along = Vec3::new(face.z.abs(), 0.0, face.x.abs());
    let size = along * w + Vec3::Y * h + face.abs() * 0.12;
    b.decor_box(c + face * 0.02, size, GLASS, None);
    // Sill (a handhold).
    b.decor_box(
        c + face * 0.08 - Vec3::Y * (h * 0.5 + 0.05),
        along * (w + 0.3) + Vec3::Y * 0.1 + face.abs() * 0.22,
        trim,
        Some(MAT_LIMESTONE),
    );
    if shutters {
        for s in [-1.0f32, 1.0] {
            let sc = c + along * s * (w * 0.5 + w * 0.27) + face * 0.06;
            b.decor_box(sc, along * (w * 0.5) + Vec3::Y * (h + 0.05) + face.abs() * 0.05, shutter, Some(MAT_PLANKS));
        }
    }
}

/// A door on a face at local ground level `y`.
fn door(b: &mut B, c: Vec3, face: Vec3, w: f32, h: f32, color: &str, trim: &str) {
    let along = Vec3::new(face.z.abs(), 0.0, face.x.abs());
    b.decor_box(c + face * 0.03 + Vec3::Y * (h * 0.5), along * w + Vec3::Y * h + face.abs() * 0.1, color, Some(MAT_PLANKS));
    b.decor_box(
        c + face * 0.07 + Vec3::Y * (h + 0.12),
        along * (w + 0.4) + Vec3::Y * 0.24 + face.abs() * 0.14,
        trim,
        Some(MAT_LIMESTONE),
    );
}

/// Steps up from local `a` (bottom front edge centre) toward `dir` (a unit local axis), rising
/// `rise` in steps of ≤ 0.3 m, `width` wide; each step is a solid block down to `floor`.
fn stairs(b: &mut B, a: Vec3, dir: Vec3, rise: f32, width: f32, floor: f32, color: &str, mat: &str) -> Vec3 {
    let n = (rise / 0.3).ceil().max(1.0) as i32;
    let (h, run) = (rise / n as f32, 0.34);
    let across = Vec3::new(dir.z.abs(), 0.0, dir.x.abs()) * (width * 0.5);
    for i in 0..n {
        let s0 = a + dir * (run * i as f32);
        let s1 = a + dir * (run * (i + 1) as f32);
        let top = a.y + h * (i + 1) as f32;
        let (p, q) = (s0 - across, s1 + across);
        b.block(Vec3::new(p.x, floor, p.z), Vec3::new(q.x, top, q.z), color, ROOF, Some(mat));
    }
    a + dir * (run * n as f32) + Vec3::Y * rise
}

/// A cannon on its carriage at local `p` (on the floor), barrel pointing along local `dir`.
fn cannon(b: &mut B, p: Vec3, dir: Vec3) {
    let yaw = dir.x.atan2(dir.z);
    let r = Quat::from_rotation_y(yaw);
    b.decor(Shape::Box { half: Vec3::new(0.45, 0.25, 0.75) }, p + Vec3::Y * 0.35, r, TIMBER, true, Some(MAT_PLANKS));
    for s in [-1.0f32, 1.0] {
        let wheel = p + r * Vec3::new(s * 0.52, 0.3, 0.3);
        b.decor(
            Shape::Cylinder { half_height: 0.06, radius: 0.3 },
            wheel,
            r * Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
            TIMBER_DARK,
            false,
            None,
        );
    }
    let barrel = r * Quat::from_rotation_x(std::f32::consts::FRAC_PI_2 - 0.08);
    b.decor(
        Shape::Cylinder { half_height: 0.95, radius: 0.17 },
        p + Vec3::Y * 0.72 + r * Vec3::new(0.0, 0.0, 0.35),
        barrel,
        IRON,
        false,
        Some(MAT_IRON),
    );
}

fn yaw_quarters(yaw: f32) -> u8 {
    ((yaw / 90.0).round() as i32).rem_euclid(4) as u8
}

// ---------------------------------------------------------------------------------------------
// Entry points.

/// Generates one structure. `ground` gives the terrain height at a world x, z.
pub fn generate(def: &StructureDef, ground: &dyn Fn(Vec2) -> f32) -> StructureOut {
    generate_with(def, 0, ground, &[])
}

/// Generates a list in order: lines between roofs find the roofs of the structures before them.
pub fn generate_all(defs: &[StructureDef], ground: &dyn Fn(Vec2) -> f32) -> StructureOut {
    let mut all = StructureOut::default();
    for (i, d) in defs.iter().enumerate() {
        let o = generate_with(d, i, ground, &all.blocks);
        all.merge(o);
    }
    all
}

/// Generates one structure; `existing` are blocks already in the level.
pub fn generate_with(def: &StructureDef, index: usize, ground: &dyn Fn(Vec2) -> f32, existing: &[Block]) -> StructureOut {
    let name = if def.name.is_empty() { format!("{}{index}", def.kind.name()) } else { def.name.clone() };
    let size = def.size.unwrap_or(def.kind.default_size());
    let (w, d) = (size[0].max(0.5), size[1].max(0.2));
    let q = yaw_quarters(def.yaw);
    let at = Vec2::new(def.at[0], def.at[1]);
    let seed = def.seed ^ (def.kind as u64).wrapping_mul(0x9E37_79B9) ^ 0x5eed;
    let mut b = B { origin: Vec3::new(at.x, 0.0, at.y), q, name, seed, out: StructureOut::default(), ground, existing };
    // Floor level: the highest ground under the footprint (or as written).
    let water = matches!(def.kind, Kind::Pier | Kind::Quay | Kind::HarbourWall);
    let lines = matches!(def.kind, Kind::LaundryLine | Kind::Beam);
    if !water && !lines {
        let mut hi = f32::MIN;
        let (nx, nz) = ((w.ceil() as i32).max(1), (d.ceil() as i32).max(1));
        for k in 0..=nz {
            for i in 0..=nx {
                hi =
                    hi.max(b.ground_at(Vec3::new(-w * 0.5 + w * i as f32 / nx as f32, 0.0, -d * 0.5 + d * k as f32 / nz as f32)));
            }
        }
        b.origin.y = def.base.unwrap_or(hi);
        b.footprint(w, d);
    } else if water {
        b.origin.y = 0.0;
        b.footprint(w, d);
    }
    match def.kind {
        Kind::House => house(&mut b, def, w, d, false),
        Kind::Tavern => house(&mut b, def, w, d, true),
        Kind::Barracks => barracks(&mut b, def, w, d),
        Kind::Chapel => chapel(&mut b, def, w, d),
        Kind::Warehouse => warehouse(&mut b, def, w, d),
        Kind::Hall => hall(&mut b, def, w, d),
        Kind::Stall => stall(&mut b, def, w, d),
        Kind::Stocks => stocks(&mut b, w, d),
        Kind::HayCart => hay_cart(&mut b, w, d),
        Kind::Pier => pier(&mut b, def, w, d),
        Kind::Quay => quay(&mut b, def, w, d),
        Kind::HarbourWall => harbour_wall(&mut b, def, w, d),
        Kind::FortWall => fort_wall(&mut b, def, w, d),
        Kind::Bastion => bastion(&mut b, def, w, d),
        Kind::Battery => battery(&mut b, def, w, d),
        Kind::MortarTower => mortar_tower(&mut b, def, w, d),
        Kind::Keep => keep(&mut b, def, w, d),
        Kind::Gate => gate(&mut b, def, w, d),
        Kind::Lighthouse => lighthouse(&mut b, def, w, d),
        Kind::WatchtowerRuin => ruin(&mut b, def, w, d),
        Kind::Hut => hut(&mut b, def, w, d),
        Kind::LaundryLine => line(&mut b, def, true),
        Kind::Beam => line(&mut b, def, false),
        Kind::Palisade => palisade(&mut b, def, w, d),
        Kind::CrateStack => crate_stack(&mut b, w, d),
        Kind::WantedPoster => poster(&mut b, def),
        Kind::Paving => paving(&mut b, def, w, d),
    }
    b.marker("center", Vec3::ZERO);
    b.out
}

fn style_of(b: &B, def: &StructureDef, default: &str) -> Style {
    let fallback = if default.is_empty() { b.pick(1, &PLASTERS) } else { default };
    def.style.as_deref().and_then(style).or_else(|| style(fallback)).unwrap_or(STYLES[0])
}

// ---------------------------------------------------------------------------------------------
// Town.

/// A town house (or the tavern): plastered mass, flat roof with parapet, string courses,
/// windows with shutters, a door, balcony, awning, beam outriggers, spouts.
fn house(b: &mut B, def: &StructureDef, w: f32, d: f32, tavern: bool) {
    let st = style_of(b, def, "");
    let floors = def.floors.unwrap_or(if tavern { 2 } else { 1 + (b.r(2) * 2.0) as u32 }).clamp(1, 3);
    let h = floors as f32 * FLOOR;
    let p = PLINTH;
    b.foundation(Vec2::ZERO, w + 0.2, d + 0.2, p, STONE_DARK);
    b.mass(Vec3::new(-w * 0.5, p, -d * 0.5), Vec3::new(w * 0.5, p + h, d * 0.5), st.wall, st.mat);
    let top = p + h;
    let roof_tone = b.pick(3, &["#cf9f78", "#d9b38c", "#c88e66", "#e0c4a0"]);
    let py = flat_roof(b, 0.0, 0.0, w, d, top, st.wall, roof_tone);
    b.marker("roof", Vec3::new(0.0, top + 0.08, 0.0));
    // String courses at the floor lines (front and back), handholds.
    for f in 1..floors {
        let y = p + f as f32 * FLOOR;
        for z in [d * 0.5, -d * 0.5] {
            let s = z.signum();
            b.block(
                Vec3::new(-w * 0.5 - 0.06, y - 0.07, z - s * 0.02),
                Vec3::new(w * 0.5 + 0.06, y + 0.07, z + s * 0.12),
                st.trim,
                CLIMBABLE | ROOF,
                Some(MAT_LIMESTONE),
            );
        }
    }
    // Door, windows.
    let shutter = st.accent;
    let door_x = if w > 6.0 { (b.r(4) - 0.5) * (w - 4.0) * 0.5 } else { 0.0 };
    door(b, Vec3::new(door_x, p, d * 0.5), Vec3::Z, 1.1, 2.2, TIMBER, st.trim);
    b.marker("door", Vec3::new(door_x, p, d * 0.5 + 0.8));
    let n = (((w - 1.2) / 2.6).floor() as i32).max(1);
    let boarded = tavern;
    let side = def.balcony.unwrap_or(if floors >= 2 && b.r(5) < 0.7 { Side::Front } else { Side::None });
    let side = if floors >= 2 { side } else { Side::None };
    for f in 0..floors {
        let y = p + f as f32 * FLOOR + 1.75;
        for i in 0..n {
            let x = -w * 0.5 + (i as f32 + 0.5) * (w / n as f32);
            if f == 0 && (x - door_x).abs() < 1.5 {
                continue;
            }
            // The balcony's own door takes this spot.
            if f == 1 && side == Side::Front && (x - door_x * 0.5).abs() < 1.4 {
                continue;
            }
            window(b, Vec3::new(x, y, d * 0.5), Vec3::Z, 0.85, 1.3, shutter, st.trim, !boarded || f > 0);
            if boarded && f == 0 {
                for k in [-0.3f32, 0.25] {
                    b.decor(
                        Shape::Box { half: Vec3::new(0.55, 0.07, 0.03) },
                        Vec3::new(x, y + k, d * 0.5 + 0.12),
                        Quat::from_rotation_z(0.25 * k.signum()),
                        "#8a6a48",
                        false,
                        Some(MAT_PLANKS),
                    );
                }
            }
        }
        // Back: one window per floor above the ground.
        if f > 0 {
            window(
                b,
                Vec3::new((b.r(10 + f) - 0.5) * (w - 2.0) * 0.6, y, -d * 0.5),
                Vec3::NEG_Z,
                0.8,
                1.2,
                shutter,
                st.trim,
                true,
            );
        }
    }
    // Balcony.
    if side != Side::None {
        let (face, half_len, centre) = match side {
            Side::Front => (Vec3::Z, d * 0.5, Vec3::new(door_x * 0.5, 0.0, d * 0.5)),
            Side::Back => (Vec3::NEG_Z, d * 0.5, Vec3::new(0.0, 0.0, -d * 0.5)),
            Side::Left => (Vec3::NEG_X, w * 0.5, Vec3::new(-w * 0.5, 0.0, 0.0)),
            _ => (Vec3::X, w * 0.5, Vec3::new(w * 0.5, 0.0, 0.0)),
        };
        let _ = half_len;
        let along = Vec3::new(face.z.abs(), 0.0, face.x.abs());
        let y = p + FLOOR;
        let (len, depth) = ((w.min(d * 2.0) * 0.5).clamp(2.2, 3.6), 1.0);
        let c = centre + face * (depth * 0.5);
        let half = along * (len * 0.5) + face.abs() * (depth * 0.5);
        b.block(c - half + Vec3::Y * (y - 0.15), c + half + Vec3::Y * y, TIMBER, ROOF, Some(MAT_PLANKS));
        // Railing: a top rail all round the open sides, balusters under it.
        let rail_y = y + 0.95;
        let front = c + face * (depth * 0.5 - 0.03);
        b.block(
            front - along * (len * 0.5) + Vec3::Y * (rail_y - 0.06) - face.abs() * 0.03,
            front + along * (len * 0.5) + Vec3::Y * rail_y + face.abs() * 0.03,
            IRON,
            BEAM,
            None,
        );
        for s in [-1.0f32, 1.0] {
            let sc = c + along * (s * (len * 0.5 - 0.03));
            b.block(
                sc - face.abs() * (depth * 0.5) - along * 0.03 + Vec3::Y * (rail_y - 0.06),
                sc + face.abs() * (depth * 0.5) + along * 0.03 + Vec3::Y * rail_y,
                IRON,
                BEAM,
                None,
            );
        }
        let k = ((len / 0.45) as i32).max(3);
        for i in 0..=k {
            let bc = front - along * (len * 0.5) + along * (len * i as f32 / k as f32) + Vec3::Y * (y + 0.45);
            b.decor_box(bc, Vec3::new(0.04, 0.9, 0.04), IRON, None);
        }
        // Brackets and a door onto it.
        for s in [-1.0f32, 1.0] {
            b.decor_box(
                centre + face * 0.35 + along * (s * len * 0.38) + Vec3::Y * (y - 0.35),
                along * 0.14 + Vec3::Y * 0.4 + face.abs() * 0.7,
                TIMBER_DARK,
                None,
            );
        }
        door(b, centre + Vec3::Y * y, face, 1.0, 2.1, st.accent, st.trim);
        b.marker("balcony", c + Vec3::Y * y);
    }
    // Awning over the door.
    if def.awning.unwrap_or(b.r(6) < 0.6 || tavern) {
        let (aw, ad) = (2.8, 1.5);
        let y = p + 2.75;
        let tilt: f32 = 0.32;
        let c = Vec3::new(door_x, y - ad * 0.5 * tilt.sin(), d * 0.5 + ad * 0.5 * tilt.cos() + 0.02);
        let striped = b.r(7) < 0.5;
        let color = b.pick(8, &["#c0392b", "#2f6b8a", "#e0b040", "#3f7f4f", "#efe6d2"]);
        b.decor(
            Shape::Box { half: Vec3::new(aw * 0.5, 0.03, ad * 0.5) },
            c,
            Quat::from_rotation_x(tilt),
            color,
            false,
            Some(if striped { MAT_STRIPES } else { MAT_CANVAS }),
        );
        b.sway_last(Sway::Leaves);
        b.decor_box(Vec3::new(door_x, y + 0.02, d * 0.5 + 0.05), Vec3::new(aw, 0.1, 0.1), TIMBER_DARK, None);
        b.area(
            "awning",
            AreaKind::Soft,
            Vec3::new(door_x - aw * 0.5, y - 0.8, d * 0.5),
            Vec3::new(door_x + aw * 0.5, y + 0.6, d * 0.5 + ad + 0.1),
        );
    }
    // Beam outriggers (vigas) under the parapet: perches and handholds.
    if floors >= 2 || b.r(9) < 0.5 {
        let k = ((w / 2.2) as i32).clamp(2, 5);
        for i in 0..k {
            let x = -w * 0.5 + (i as f32 + 0.5) * (w / k as f32);
            b.block(
                Vec3::new(x - 0.09, top - 0.42, d * 0.5),
                Vec3::new(x + 0.09, top - 0.24, d * 0.5 + 0.55),
                TIMBER_DARK,
                BEAM | CLIMBABLE,
                Some(MAT_PLANKS),
            );
        }
    }
    // Spouts through the parapet.
    for s in [-1.0f32, 1.0] {
        b.decor(
            Shape::Box { half: Vec3::new(0.08, 0.07, 0.3) },
            Vec3::new(s * (w * 0.5 - 0.6), top + 0.12, d * 0.5 + 0.25),
            Quat::from_rotation_x(-0.15),
            TERRACOTTA,
            false,
            None,
        );
    }
    // A roof hatch.
    b.decor_box(
        Vec3::new((b.r(12) - 0.5) * (w - 2.0) * 0.5, top + 0.14, -(d * 0.25)),
        Vec3::new(0.9, 0.12, 0.9),
        TIMBER,
        Some(MAT_PLANKS),
    );
    let _ = py;
    if tavern {
        // A hanging sign on a bracket and barrels by the door.
        let sx = door_x + 1.6;
        b.decor_box(Vec3::new(sx, p + 3.0, d * 0.5 + 0.45), Vec3::new(0.08, 0.08, 0.9), IRON, None);
        b.decor_box(Vec3::new(sx, p + 2.55, d * 0.5 + 0.7), Vec3::new(0.06, 0.65, 0.85), "#7a5232", Some(MAT_PLANKS));
        b.sway_last(Sway::Leaves);
        b.prop("LIB/core/barrel", Vec3::new(door_x - 1.6, p, d * 0.5 + 0.6), 10.0, 1.0, 0, true);
        b.prop("LIB/core/barrel", Vec3::new(door_x - 2.4, p, d * 0.5 + 0.7), 40.0, 1.0, 1, true);
        let sign = def.sign.as_deref().unwrap_or("TAVERN");
        b.label(sign, Vec3::new(door_x, p + 3.55, d * 0.5 + 0.16), 0.32, Facing::South, "#3a2414");
    } else if let Some(s) = def.sign.as_deref() {
        b.label(s, Vec3::new(door_x, p + 3.0, d * 0.5 + 0.16), 0.3, Facing::South, "#3a2414");
    }
}

/// Barracks: a long lime-washed block of two floors, barred windows, a row of doors.
fn barracks(b: &mut B, def: &StructureDef, w: f32, d: f32) {
    let st = style_of(b, def, "lime");
    let floors = def.floors.unwrap_or(2).clamp(1, 3);
    let h = floors as f32 * FLOOR;
    let p = PLINTH;
    b.foundation(Vec2::ZERO, w + 0.2, d + 0.2, p, STONE_DARK);
    b.mass(Vec3::new(-w * 0.5, p, -d * 0.5), Vec3::new(w * 0.5, p + h, d * 0.5), st.wall, st.mat);
    let top = p + h;
    flat_roof(b, 0.0, 0.0, w, d, top, st.wall, "#c9b58f");
    b.marker("roof", Vec3::new(0.0, top + 0.08, 0.0));
    // A stone base course.
    b.block(
        Vec3::new(-w * 0.5 - 0.05, p, d * 0.5 - 0.1),
        Vec3::new(w * 0.5 + 0.05, p + 0.9, d * 0.5 + 0.06),
        LIMESTONE,
        CLIMBABLE,
        Some(MAT_LIMESTONE),
    );
    let n = ((w / 3.0) as i32).max(2);
    for f in 0..floors {
        for i in 0..n {
            let x = -w * 0.5 + (i as f32 + 0.5) * (w / n as f32);
            if f == 0 && i % 2 == 0 {
                door(b, Vec3::new(x, p, d * 0.5), Vec3::Z, 1.0, 2.1, "#2c3e66", st.trim);
                if i == 0 {
                    b.marker("door", Vec3::new(x, p, d * 0.5 + 0.8));
                }
            } else {
                window(b, Vec3::new(x, p + f as f32 * FLOOR + 1.8, d * 0.5), Vec3::Z, 0.7, 1.0, "#2c3e66", st.trim, false);
                // Bars.
                b.decor_box(Vec3::new(x, p + f as f32 * FLOOR + 1.8, d * 0.5 + 0.1), Vec3::new(0.05, 1.0, 0.05), IRON, None);
            }
        }
    }
    if let Some(s) = def.sign.as_deref() {
        b.label(s, Vec3::new(0.0, top - 0.5, d * 0.5 + 0.03), 0.4, Facing::South, "#2c3e66");
    }
}

/// The chapel: a nave with a pitched terracotta roof, a front gable with a rose window and a
/// 24 m bell tower whose top is a perch.
fn chapel(b: &mut B, def: &StructureDef, w: f32, d: f32) {
    let st = style_of(b, def, "lime");
    let p = PLINTH;
    let tower = 5.0f32.min(w * 0.5);
    let nave_w = w;
    let nave_h = def.height.unwrap_or(9.0);
    b.foundation(Vec2::ZERO, w + 0.4, d + 0.4, p, STONE_DARK);
    // Nave.
    b.mass(Vec3::new(-nave_w * 0.5, p, -d * 0.5), Vec3::new(nave_w * 0.5, p + nave_h, d * 0.5 - 0.6), st.wall, st.mat);
    // Pitched roof (visual and solid, not a walkable roof).
    let pitch = 0.55f32;
    let half_w = nave_w * 0.5 + 0.3;
    let slab = half_w / pitch.cos();
    let rise = half_w * pitch.tan();
    for s in [-1.0f32, 1.0] {
        let c = Vec3::new(s * half_w * 0.5, p + nave_h + rise * 0.5 + 0.1, -0.3);
        b.decor(
            Shape::Box { half: Vec3::new(slab * 0.5, 0.12, d * 0.5) },
            c,
            Quat::from_rotation_z(-s * pitch),
            TERRACOTTA,
            true,
            Some(MAT_TERRACOTTA),
        );
    }
    // Gable infill under the roof (front and back): stepped blocks.
    // Back gable: steps that stay under the roof's slope.
    let steps = 6;
    for i in 0..steps {
        let f0 = i as f32 / steps as f32;
        let f1 = (i + 1) as f32 / steps as f32;
        let hw = half_w * (1.0 - f1) - 0.05;
        if hw <= 0.1 {
            break;
        }
        b.block(
            Vec3::new(-hw, p + nave_h + rise * f0, -d * 0.5),
            Vec3::new(hw, p + nave_h + rise * f1 - 0.02, -d * 0.5 + 0.6),
            st.wall,
            CLIMBABLE,
            Some(st.mat),
        );
    }
    // Front facade: a tall espadaña-like front wall with a stepped top.
    let fz = d * 0.5;
    let face_h = nave_h + rise + 1.2;
    b.mass(Vec3::new(-nave_w * 0.5 + tower, p, fz - 0.6), Vec3::new(nave_w * 0.5, p + face_h - 1.2, fz), st.wall, st.mat);
    b.mass(
        Vec3::new(-1.6 + tower * 0.5, p + face_h - 1.2, fz - 0.6),
        Vec3::new(1.6 + tower * 0.5, p + face_h, fz),
        st.trim,
        st.mat,
    );
    // Door (arched) and rose window.
    let dx = tower * 0.5;
    door(b, Vec3::new(dx, p, fz), Vec3::Z, 1.8, 3.2, TIMBER, st.trim);
    b.decor(
        Shape::Cylinder { half_height: 0.08, radius: 0.9 },
        Vec3::new(dx, p + 3.2, fz + 0.03),
        Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
        TIMBER,
        false,
        Some(MAT_PLANKS),
    );
    b.decor(
        Shape::Cylinder { half_height: 0.08, radius: 0.85 },
        Vec3::new(dx, p + nave_h - 1.0, fz + 0.03),
        Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
        GLASS,
        false,
        None,
    );
    b.decor(
        Shape::Cylinder { half_height: 0.06, radius: 1.05 },
        Vec3::new(dx, p + nave_h - 1.0, fz + 0.01),
        Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
        st.trim,
        false,
        Some(MAT_LIMESTONE),
    );
    b.marker("door", Vec3::new(dx, p, fz + 1.0));
    // Side windows (tall, narrow).
    for i in 0..3 {
        let z = -d * 0.5 + (i as f32 + 0.75) * (d / 3.4);
        for x in [-nave_w * 0.5, nave_w * 0.5] {
            window(b, Vec3::new(x, p + 4.5, z), Vec3::new(x.signum(), 0.0, 0.0), 0.7, 2.2, st.accent, st.trim, false);
        }
    }
    // Bell tower at the front left corner: 24 m to the perch.
    let total = 24.0;
    let (tx0, tx1, tz0, tz1) = (-nave_w * 0.5 - 0.3, -nave_w * 0.5 + tower, fz - tower - 0.3, fz + 0.3);
    let shaft = total - 4.4;
    b.mass(Vec3::new(tx0, p, tz0), Vec3::new(tx1, p + shaft, tz1), st.wall, st.mat);
    // Cornice bands on the tower (handholds every 5 m).
    for y in [6.0f32, 12.0, shaft] {
        b.block(
            Vec3::new(tx0 - 0.12, p + y - 0.15, tz0 - 0.12),
            Vec3::new(tx1 + 0.12, p + y + 0.1, tz1 + 0.12),
            st.trim,
            CLIMBABLE | ROOF,
            Some(MAT_LIMESTONE),
        );
    }
    // Belfry: four corner piers, the bell in the middle, a roof slab on top.
    let pier = 0.7;
    let by = p + shaft + 0.1;
    for (x, z) in [(tx0, tz0), (tx1 - pier, tz0), (tx0, tz1 - pier), (tx1 - pier, tz1 - pier)] {
        b.mass(Vec3::new(x, by, z), Vec3::new(x + pier, by + 3.3, z + pier), st.wall, st.mat);
    }
    let cx = (tx0 + tx1) * 0.5;
    let cz = (tz0 + tz1) * 0.5;
    b.decor(
        Shape::Cylinder { half_height: 0.45, radius: 0.5 },
        Vec3::new(cx, by + 1.9, cz),
        Quat::IDENTITY,
        "#b8862e",
        false,
        Some("MAT/pb/gold"),
    );
    b.decor(Shape::Sphere { radius: 0.38 }, Vec3::new(cx, by + 2.3, cz), Quat::IDENTITY, "#b8862e", false, Some("MAT/pb/gold"));
    b.decor_box(Vec3::new(cx, by + 2.95, cz), Vec3::new(tower, 0.14, 0.14), TIMBER_DARK, None);
    b.marker("bell", Vec3::new(cx, by + 1.9, cz));
    let ry = p + total - 0.4;
    b.mass(Vec3::new(tx0 - 0.2, by + 3.3, tz0 - 0.2), Vec3::new(tx1 + 0.2, p + total, tz1 + 0.2), st.trim, MAT_LIMESTONE);
    let _ = ry;
    // A cross and a perch beam over the hay-cart side (front).
    b.decor_box(Vec3::new(cx, p + total + 1.0, cz - 0.6), Vec3::new(0.14, 2.0, 0.14), IRON, None);
    b.decor_box(Vec3::new(cx, p + total + 1.4, cz - 0.6), Vec3::new(0.9, 0.14, 0.14), IRON, None);
    b.block(
        Vec3::new(cx - 0.15, p + total - 0.25, tz1 + 0.2),
        Vec3::new(cx + 0.15, p + total - 0.05, tz1 + 1.8),
        TIMBER_DARK,
        BEAM | ROOF,
        Some(MAT_PLANKS),
    );
    b.area(
        "perch",
        AreaKind::Perch,
        Vec3::new(tx0 - 0.2, p + total - 0.3, tz0 - 0.2),
        Vec3::new(tx1 + 0.2, p + total + 2.0, tz1 + 1.8),
    );
    b.marker("perch", Vec3::new(cx, p + total, tz1 + 1.4));
    b.marker("tower_top", Vec3::new(cx, p + total, cz));
    if let Some(s) = def.sign.as_deref() {
        b.label(s, Vec3::new(dx, p + 4.6, fz + 0.05), 0.34, Facing::South, "#5a4a3a");
    }
}

/// A bonded warehouse: a big stone-and-plaster box, double doors, a hoist beam with a rope,
/// crates outside, a flat walkable roof.
fn warehouse(b: &mut B, def: &StructureDef, w: f32, d: f32) {
    let st = style_of(b, def, "ochre");
    let h = def.height.unwrap_or(7.0);
    let p = PLINTH;
    b.foundation(Vec2::ZERO, w + 0.2, d + 0.2, p, STONE_DARK);
    b.mass(Vec3::new(-w * 0.5, p, -d * 0.5), Vec3::new(w * 0.5, p + h, d * 0.5), st.wall, st.mat);
    // Stone quoins at the corners.
    for x in [-w * 0.5 - 0.05, w * 0.5 - 0.55] {
        b.block(
            Vec3::new(x, p, d * 0.5 - 0.55),
            Vec3::new(x + 0.6, p + h, d * 0.5 + 0.05),
            LIMESTONE,
            CLIMBABLE,
            Some(MAT_LIMESTONE),
        );
    }
    let top = p + h;
    flat_roof(b, 0.0, 0.0, w, d, top, st.wall, "#c98a5e");
    b.marker("roof", Vec3::new(0.0, top + 0.08, 0.0));
    door(b, Vec3::new(0.0, p, d * 0.5), Vec3::Z, 3.2, 3.4, TIMBER, st.trim);
    b.decor_box(Vec3::new(0.0, p + 1.7, d * 0.5 + 0.1), Vec3::new(0.08, 3.4, 0.04), TIMBER_DARK, None);
    b.marker("door", Vec3::new(0.0, p, d * 0.5 + 1.0));
    // Loft door and hoist.
    door(b, Vec3::new(0.0, p + h - 3.0, d * 0.5), Vec3::Z, 1.4, 1.9, TIMBER, st.trim);
    b.block(
        Vec3::new(-0.12, top - 0.6, d * 0.5),
        Vec3::new(0.12, top - 0.36, d * 0.5 + 1.6),
        TIMBER_DARK,
        BEAM | POLE,
        Some(MAT_PLANKS),
    );
    b.decor(
        Shape::Cylinder { half_height: 1.6, radius: 0.03 },
        Vec3::new(0.0, top - 2.1, d * 0.5 + 1.45),
        Quat::IDENTITY,
        ROPE,
        false,
        Some(MAT_ROPE),
    );
    b.decor_box(Vec3::new(0.0, top - 3.85, d * 0.5 + 1.45), Vec3::new(0.7, 0.5, 0.5), "#8a6a48", Some(MAT_PLANKS));
    b.marker("hoist", Vec3::new(0.0, top - 0.36, d * 0.5 + 1.4));
    // High windows with bars.
    for x in [-w * 0.32, w * 0.32] {
        window(b, Vec3::new(x, p + h - 2.0, d * 0.5), Vec3::Z, 1.0, 0.8, st.accent, st.trim, false);
    }
    // Crates outside.
    for (i, x) in [-w * 0.5 + 1.2, w * 0.5 - 1.4].iter().enumerate() {
        let n = 1 + (b.r(20 + i as u32) * 2.5) as i32;
        for k in 0..n {
            let s = 1.0 - 0.1 * k as f32;
            let y = p + k as f32 * 1.0;
            b.block(
                Vec3::new(x - 0.5 * s, y, d * 0.5 + 0.5),
                Vec3::new(x + 0.5 * s, y + 1.0, d * 0.5 + 0.5 + s),
                "#9a7048",
                CLIMBABLE | ROOF,
                Some(MAT_PLANKS),
            );
        }
    }
    if let Some(s) = def.sign.as_deref() {
        b.label(s, Vec3::new(0.0, p + 4.0, d * 0.5 + 0.03), 0.42, Facing::South, "#3a2414");
    }
}

/// A civic hall: two tall floors, a portico of four columns with a walkable roof, front steps.
fn hall(b: &mut B, def: &StructureDef, w: f32, d: f32) {
    let st = style_of(b, def, "lime");
    let floors = def.floors.unwrap_or(2).clamp(1, 3);
    let h = floors as f32 * (FLOOR + 0.6);
    let p = PLINTH + 0.6;
    let porch = 2.6;
    let body_d = d - porch;
    let bz = -d * 0.5 + body_d * 0.5;
    b.foundation(Vec2::new(0.0, 0.0), w + 0.4, d + 0.2, p, STONE_DARK);
    b.mass(Vec3::new(-w * 0.5, p, -d * 0.5), Vec3::new(w * 0.5, p + h, bz + body_d * 0.5), st.wall, st.mat);
    let top = p + h;
    flat_roof(b, 0.0, bz, w, body_d, top, st.wall, "#c9b58f");
    b.marker("roof", Vec3::new(0.0, top + 0.08, bz));
    // Pediment block over the centre.
    b.mass(
        Vec3::new(-w * 0.2, top + PARAPET, bz + body_d * 0.5 - 0.4),
        Vec3::new(w * 0.2, top + PARAPET + 1.0, bz + body_d * 0.5),
        st.trim,
        st.mat,
    );
    // Portico: columns, entablature, a walkable roof with a low balustrade.
    let fz = bz + body_d * 0.5;
    let ph = FLOOR + 1.2;
    for i in 0..4 {
        let x = -w * 0.5 + 1.0 + i as f32 * ((w - 2.0) / 3.0);
        b.decor(
            Shape::Cylinder { half_height: ph * 0.5, radius: 0.32 },
            Vec3::new(x, p + ph * 0.5, fz + porch - 0.5),
            Quat::IDENTITY,
            st.trim,
            true,
            Some(MAT_LIMESTONE),
        );
    }
    b.block(
        Vec3::new(-w * 0.5, p + ph, fz),
        Vec3::new(w * 0.5, p + ph + 0.45, fz + porch),
        st.trim,
        ROOF | CLIMBABLE,
        Some(MAT_LIMESTONE),
    );
    b.block(
        Vec3::new(-w * 0.5, p + ph + 0.45, fz + porch - 0.2),
        Vec3::new(w * 0.5, p + ph + 1.0, fz + porch),
        st.wall,
        ROOF | CLIMBABLE,
        Some(st.mat),
    );
    b.marker("portico_roof", Vec3::new(0.0, p + ph + 0.45, fz + porch * 0.5));
    // Front steps from the ground to the porch floor.
    stairs(
        b,
        Vec3::new(0.0, PLINTH, d * 0.5 + 0.1 + 2.0 * 0.34),
        Vec3::NEG_Z,
        0.6,
        w * 0.6,
        b.lowest(Vec2::new(0.0, d * 0.5 + 0.6), w * 0.6, 1.0) - b.origin.y - 0.3,
        LIMESTONE,
        MAT_LIMESTONE,
    );
    door(b, Vec3::new(0.0, p, fz), Vec3::Z, 1.8, 3.0, TIMBER, st.trim);
    b.marker("door", Vec3::new(0.0, p, fz + 1.0));
    let n = ((w / 3.0) as i32).max(2);
    for f in 0..floors {
        for i in 0..n {
            let x = -w * 0.5 + (i as f32 + 0.5) * (w / n as f32);
            if f == 0 && x.abs() < 1.5 {
                continue;
            }
            window(b, Vec3::new(x, p + f as f32 * (FLOOR + 0.6) + 2.0, fz), Vec3::Z, 0.9, 1.7, "#2c3e66", st.trim, true);
        }
    }
    if let Some(s) = def.sign.as_deref() {
        b.label(s, Vec3::new(0.0, p + ph + 0.72, fz + porch + 0.02), 0.36, Facing::South, "#2c3e66");
    }
}

/// A market stall: four posts, a canvas canopy (soft landing), a counter with goods.
fn stall(b: &mut B, def: &StructureDef, w: f32, d: f32) {
    let h = 2.3;
    for (x, z) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
        let post_h = if z < 0.0 { h + 0.35 } else { h };
        b.block(
            Vec3::new(x * (w * 0.5 - 0.05) - 0.05, 0.0, z * (d * 0.5 - 0.05) - 0.05),
            Vec3::new(x * (w * 0.5 - 0.05) + 0.05, post_h, z * (d * 0.5 - 0.05) + 0.05),
            TIMBER,
            POLE,
            Some(MAT_PLANKS),
        );
    }
    let tilt = (0.35f32 / d).atan();
    let color =
        def.style.as_deref().and_then(style).map_or_else(|| b.pick(3, &["#c0392b", "#2f6b8a", "#e0b040", "#3f7f4f"]), |s| s.wall);
    b.decor(
        Shape::Box { half: Vec3::new(w * 0.5 + 0.2, 0.03, d * 0.5 + 0.3) },
        Vec3::new(0.0, h + 0.2, 0.0),
        Quat::from_rotation_x(tilt),
        color,
        false,
        Some(if b.r(4) < 0.6 { MAT_STRIPES } else { MAT_CANVAS }),
    );
    b.sway_last(Sway::Leaves);
    b.area(
        "canopy",
        AreaKind::Soft,
        Vec3::new(-w * 0.5 - 0.2, h - 0.4, -d * 0.5 - 0.3),
        Vec3::new(w * 0.5 + 0.2, h + 1.0, d * 0.5 + 0.3),
    );
    // Counter and goods.
    b.block(
        Vec3::new(-w * 0.5 + 0.1, 0.0, d * 0.5 - 0.7),
        Vec3::new(w * 0.5 - 0.1, 0.95, d * 0.5 - 0.1),
        "#8a6a48",
        ROOF | CLIMBABLE,
        Some(MAT_PLANKS),
    );
    let goods = ["#e0b040", "#c0392b", "#6aa84f", "#e07a2f", "#8a4a8a"];
    for i in 0..4 {
        let x = -w * 0.5 + 0.5 + i as f32 * ((w - 1.0) / 3.0);
        let c = goods[((b.r(10 + i) * 5.0) as usize).min(4)];
        b.decor_box(Vec3::new(x, 1.1, d * 0.5 - 0.4), Vec3::new(0.5, 0.3, 0.45), "#a07850", Some(MAT_PLANKS));
        b.decor(Shape::Sphere { radius: 0.2 }, Vec3::new(x, 1.3, d * 0.5 - 0.4), Quat::IDENTITY, c, false, None);
    }
    b.marker("counter", Vec3::new(0.0, 0.0, d * 0.5 + 0.6));
}

/// The stocks: a low platform, two posts and a board with three holes.
fn stocks(b: &mut B, w: f32, d: f32) {
    b.block(Vec3::new(-w * 0.5, -0.3, -d * 0.5), Vec3::new(w * 0.5, 0.3, d * 0.5), TIMBER, ROOF, Some(MAT_PLANKS));
    for x in [-w * 0.42, w * 0.42] {
        b.block(Vec3::new(x - 0.08, 0.3, -0.08), Vec3::new(x + 0.08, 1.6, 0.08), TIMBER_DARK, POLE, Some(MAT_PLANKS));
    }
    b.block(Vec3::new(-w * 0.45, 1.0, -0.07), Vec3::new(w * 0.45, 1.35, 0.07), "#7a5232", 0, Some(MAT_PLANKS));
    for x in [-0.45f32, 0.0, 0.45] {
        b.decor(
            Shape::Cylinder { half_height: 0.08, radius: if x == 0.0 { 0.12 } else { 0.07 } },
            Vec3::new(x, 1.17, 0.0),
            Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
            "#1e1a16",
            false,
            None,
        );
    }
    b.marker("stocks", Vec3::new(0.0, 0.3, 0.3));
}

/// A hay cart: the softest landing in town.
fn hay_cart(b: &mut B, w: f32, d: f32) {
    b.block(Vec3::new(-w * 0.5, 0.55, -d * 0.5), Vec3::new(w * 0.5, 0.75, d * 0.5), TIMBER, ROOF, Some(MAT_PLANKS));
    for x in [-w * 0.5, w * 0.5 - 0.08] {
        b.decor_box(Vec3::new(x + 0.04, 1.05, 0.0), Vec3::new(0.08, 0.6, d), "#7a5232", Some(MAT_PLANKS));
    }
    for (x, z) in [(-1.0, -0.6), (1.0, -0.6), (-1.0, 0.6), (1.0, 0.6)] {
        b.decor(
            Shape::Cylinder { half_height: 0.06, radius: 0.55 },
            Vec3::new(x * (w * 0.5 + 0.08), 0.55, z * d * 0.5),
            Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
            TIMBER_DARK,
            false,
            Some(MAT_PLANKS),
        );
    }
    b.decor(
        Shape::RoundedBox { half: Vec3::new(w * 0.5 - 0.05, 0.55, d * 0.5 - 0.05), radius: 0.45 },
        Vec3::new(0.0, 1.25, 0.0),
        Quat::IDENTITY,
        "#d9b65a",
        false,
        Some(MAT_THATCH),
    );
    b.decor(
        Shape::RoundedBox { half: Vec3::new(w * 0.4, 0.35, d * 0.38), radius: 0.32 },
        Vec3::new(0.05, 1.75, -0.1),
        Quat::from_rotation_y(0.2),
        "#e2c46a",
        false,
        Some(MAT_THATCH),
    );
    // Shafts.
    b.decor(
        Shape::Box { half: Vec3::new(0.05, 0.05, 0.9) },
        Vec3::new(-0.4, 0.5, d * 0.5 + 0.85),
        Quat::from_rotation_x(0.25),
        TIMBER,
        false,
        None,
    );
    b.decor(
        Shape::Box { half: Vec3::new(0.05, 0.05, 0.9) },
        Vec3::new(0.4, 0.5, d * 0.5 + 0.85),
        Quat::from_rotation_x(0.25),
        TIMBER,
        false,
        None,
    );
    b.area("hay", AreaKind::Soft, Vec3::new(-w * 0.5, 0.7, -d * 0.5), Vec3::new(w * 0.5, 3.0, d * 0.5));
    b.area("hay_hide", AreaKind::Hide, Vec3::new(-w * 0.5, 0.7, -d * 0.5), Vec3::new(w * 0.5, 2.2, d * 0.5));
    b.marker("landing", Vec3::new(0.0, 1.9, 0.0));
}

// ---------------------------------------------------------------------------------------------
// Docks.

/// A timber pier: planks on posts, bollards, ladders down to the water at the far end.
/// `at` is the centre; the far (seaward) end is local −z; `base` the deck top (default 1.4).
fn pier(b: &mut B, def: &StructureDef, w: f32, d: f32) {
    let deck = def.base.unwrap_or(1.4);
    b.block(Vec3::new(-w * 0.5, deck - 0.25, -d * 0.5), Vec3::new(w * 0.5, deck, d * 0.5), "#9a7048", ROOF, Some(MAT_PLANKS));
    // Stringers under the deck.
    for x in [-w * 0.5 + 0.2, w * 0.5 - 0.2] {
        b.decor_box(Vec3::new(x, deck - 0.4, 0.0), Vec3::new(0.25, 0.3, d), TIMBER_DARK, None);
    }
    // Posts every 3 m down to the seabed.
    let n = ((d / 3.0) as i32).max(1);
    for i in 0..=n {
        let z = -d * 0.5 + i as f32 * (d / n as f32);
        for x in [-w * 0.5 + 0.15, w * 0.5 - 0.15] {
            let g = b.ground_at(Vec3::new(x, 0.0, z)).min(deck - 0.6) - 0.5;
            let hh = (deck - 0.25 - g) * 0.5;
            b.decor(
                Shape::Cylinder { half_height: hh, radius: 0.16 },
                Vec3::new(x, g + hh, z),
                Quat::IDENTITY,
                "#5a3f28",
                true,
                Some(MAT_PLANKS),
            );
        }
    }
    // Bollards along the edges.
    for i in 0..3 {
        let z = -d * 0.5 + 1.0 + i as f32 * ((d - 2.0) / 2.0).max(0.0);
        for x in [-w * 0.5 + 0.35, w * 0.5 - 0.35] {
            b.decor(
                Shape::Cylinder { half_height: 0.3, radius: 0.18 },
                Vec3::new(x, deck + 0.3, z),
                Quat::IDENTITY,
                IRON,
                true,
                None,
            );
        }
    }
    // Ladders down to the water on both sides at the far end.
    let lz = -d * 0.5 + 1.2;
    let bottom = (b.ground_at(Vec3::new(0.0, 0.0, lz)) + 0.3).max(-2.5);
    b.ladder(Vec3::new(-w * 0.5 - 0.3, bottom, lz - 0.3), Vec3::new(-w * 0.5, deck, lz + 0.3), Facing::East);
    b.ladder(Vec3::new(w * 0.5, bottom, lz - 0.3), Vec3::new(w * 0.5 + 0.3, deck, lz + 0.3), Facing::West);
    b.marker("ladder_top_left", Vec3::new(-w * 0.5 + 0.4, deck, lz));
    b.marker("ladder_top_right", Vec3::new(w * 0.5 - 0.4, deck, lz));
    b.marker("end", Vec3::new(0.0, deck, -d * 0.5 + 0.5));
    b.marker("mooring", Vec3::new(w * 0.5 + 3.0, 0.0, -d * 0.25));
    // A few crates and a lamp post.
    b.prop("LIB/core/crate_stack", Vec3::new(-w * 0.5 + 0.8, deck, d * 0.3), 15.0, 0.8, (b.r(30) * 4.0) as u32, true);
    b.prop("LIB/core/lamp_post", Vec3::new(w * 0.5 - 0.4, deck, -d * 0.5 + 0.4), 0.0, 1.0, 0, true);
}

/// A stone quay along the waterfront: top at `base` (default 1.6), filled to the seabed,
/// bollards on the edge and stairs down into the water at the left end (seaward = local −z).
fn quay(b: &mut B, def: &StructureDef, w: f32, d: f32) {
    let top = def.base.unwrap_or(1.6);
    let low = b.lowest(Vec2::ZERO, w, d) - 0.5;
    b.block(
        Vec3::new(-w * 0.5, low, -d * 0.5),
        Vec3::new(w * 0.5, top, d * 0.5),
        LIMESTONE,
        CLIMBABLE | ROOF,
        Some(MAT_LIMESTONE),
    );
    // Coping stones on the sea edge.
    b.block(
        Vec3::new(-w * 0.5, top, -d * 0.5),
        Vec3::new(w * 0.5, top + 0.18, -d * 0.5 + 0.5),
        "#d8ccb0",
        ROOF | CLIMBABLE,
        Some(MAT_LIMESTONE),
    );
    let n = ((w / 6.0) as i32).max(1);
    for i in 0..n {
        let x = -w * 0.5 + (i as f32 + 0.5) * (w / n as f32);
        b.decor(
            Shape::Cylinder { half_height: 0.3, radius: 0.2 },
            Vec3::new(x, top + 0.48, -d * 0.5 + 0.8),
            Quat::IDENTITY,
            IRON,
            true,
            None,
        );
        b.decor(
            Shape::Cylinder { half_height: 0.24, radius: 0.24 },
            Vec3::new(x, top + 0.52, -d * 0.5 + 0.8),
            Quat::IDENTITY,
            IRON,
            false,
            None,
        );
        b.marker(&format!("bollard{i}"), Vec3::new(x, top, -d * 0.5 + 0.8));
    }
    // Stairs down to the water along the seaward face at the left end.
    let sw = 1.6;
    let steps = ((top + 0.9) / 0.3).ceil() as i32;
    for i in 0..steps {
        let y = top - 0.3 * (i + 1) as f32;
        let x0 = -w * 0.5 + 0.3 + i as f32 * 0.34;
        b.block(
            Vec3::new(x0, low, -d * 0.5 - sw),
            Vec3::new(x0 + 0.34, y.max(-1.2), -d * 0.5),
            "#bfb291",
            ROOF,
            Some(MAT_LIMESTONE),
        );
    }
    b.marker("stairs", Vec3::new(-w * 0.5 + 0.5, top, -d * 0.5 - sw * 0.5));
    // Mooring rings and a stack of goods.
    b.prop("LIB/core/barrel", Vec3::new(w * 0.3, top, d * 0.2), 0.0, 1.0, 2, true);
    b.prop("LIB/core/sack", Vec3::new(w * 0.3 + 0.9, top, d * 0.25), 30.0, 1.0, 0, false);
}

/// A harbour wall (breakwater): stone from the seabed to `base` (default 2.4), a parapet on the
/// sea side (local −z), a beacon at the far end.
fn harbour_wall(b: &mut B, def: &StructureDef, w: f32, d: f32) {
    let top = def.base.unwrap_or(2.4);
    let low = b.lowest(Vec2::ZERO, w, d) - 0.5;
    b.block(
        Vec3::new(-w * 0.5, low, -d * 0.5),
        Vec3::new(w * 0.5, top, d * 0.5),
        STONE_DARK,
        CLIMBABLE | ROOF,
        Some(MAT_LIMESTONE),
    );
    b.block(
        Vec3::new(-w * 0.5, top, -d * 0.5),
        Vec3::new(w * 0.5, top + 0.8, -d * 0.5 + 0.6),
        LIMESTONE,
        CLIMBABLE | ROOF,
        Some(MAT_LIMESTONE),
    );
    // Rubble at the foot on the sea side.
    for i in 0..((w / 4.0) as i32) {
        let x = -w * 0.5 + 2.0 + i as f32 * 4.0;
        let s = 0.6 + b.r(40 + i as u32) * 0.6;
        b.decor(
            Shape::RoundedBox { half: Vec3::new(s * 1.3, s, s), radius: s * 0.4 },
            Vec3::new(x, b.ground_at(Vec3::new(x, 0.0, -d * 0.5 - 1.0)).max(-3.0) + s * 0.4, -d * 0.5 - 0.9),
            Quat::from_rotation_y(b.r(60 + i as u32) * 3.0),
            "#7d766a",
            false,
            None,
        );
    }
    // Beacon at the right end.
    b.block(
        Vec3::new(w * 0.5 - 1.2, top, -0.6),
        Vec3::new(w * 0.5 - 0.2, top + 1.8, 0.4),
        LIMESTONE,
        CLIMBABLE | ROOF,
        Some(MAT_LIMESTONE),
    );
    b.glow(Shape::Sphere { radius: 0.35 }, Vec3::new(w * 0.5 - 0.7, top + 2.2, -0.1), "#ffcf6a", 2.5);
    b.marker("beacon", Vec3::new(w * 0.5 - 0.7, top + 1.8, -0.1));
    b.marker("walk", Vec3::new(0.0, top, d * 0.25));
}

// ---------------------------------------------------------------------------------------------
// Fort.

/// A curtain wall: 3 m thick (`size` = [length, thickness]), `height` (default 8) with a battered
/// base, a walkway on top, crenellations on the outside (local −z) and inner stairs.
fn fort_wall(b: &mut B, def: &StructureDef, w: f32, d: f32) {
    let h = def.height.unwrap_or(8.0);
    let low = b.lowest(Vec2::ZERO, w, d + 1.0) - b.origin.y - 0.6;
    b.mass(Vec3::new(-w * 0.5, low, -d * 0.5), Vec3::new(w * 0.5, h, d * 0.5), LIMESTONE, MAT_LIMESTONE);
    // Battered base (talus) on the outside and a cordon (string course) under the parapet.
    b.mass(Vec3::new(-w * 0.5, low, -d * 0.5 - 0.6), Vec3::new(w * 0.5, 2.0, -d * 0.5), STONE_DARK, MAT_LIMESTONE);
    b.block(
        Vec3::new(-w * 0.5, h - 0.35, -d * 0.5 - 0.15),
        Vec3::new(w * 0.5, h - 0.1, -d * 0.5),
        "#ddd0b2",
        CLIMBABLE | ROOF,
        Some(MAT_LIMESTONE),
    );
    crenellate(b, Vec3::new(-w * 0.5, 0.0, -d * 0.5), Vec3::new(w * 0.5, 0.0, -d * 0.5), h, 0.6, Vec3::NEG_Z);
    b.marker("walk", Vec3::new(0.0, h, 0.4));
    let side = def.stairs.unwrap_or(Side::Right);
    if side == Side::Left || side == Side::Right {
        let s = if side == Side::Right { 1.0 } else { -1.0 };
        // Inner stairs against the wall's inner face, rising toward the end.
        let start = Vec3::new(s * (w * 0.5 - 1.0 - (h / 0.3).ceil() * 0.34), 0.0, d * 0.5 + 0.8);
        let floor = b.lowest(Vec2::new(start.x + s * h * 0.6, d * 0.5 + 0.8), h * 1.2, 1.6) - b.origin.y - 0.4;
        stairs(b, start, Vec3::new(s, 0.0, 0.0), h, 1.6, floor, STONE_DARK, MAT_LIMESTONE);
        b.marker("stairs_bottom", start);
        b.marker("stairs_top", Vec3::new(s * (w * 0.5 - 1.0), h, d * 0.5 - 0.5));
    }
}

/// A bastion: a square stone platform `height` high (default 8), crenellated on its three outer
/// sides (local −z, ±x), a garita (sentry box) on the salient corner, two cannon spots.
fn bastion(b: &mut B, def: &StructureDef, w: f32, d: f32) {
    let h = def.height.unwrap_or(8.0);
    let low = b.lowest(Vec2::ZERO, w + 1.0, d + 1.0) - b.origin.y - 0.6;
    b.mass(Vec3::new(-w * 0.5, low, -d * 0.5), Vec3::new(w * 0.5, h, d * 0.5), LIMESTONE, MAT_LIMESTONE);
    b.mass(
        Vec3::new(-w * 0.5 - 0.6, low, -d * 0.5 - 0.6),
        Vec3::new(w * 0.5 + 0.6, 2.2, d * 0.5 - 1.0),
        STONE_DARK,
        MAT_LIMESTONE,
    );
    b.block(
        Vec3::new(-w * 0.5 + 0.6, h, -d * 0.5 + 0.6),
        Vec3::new(w * 0.5 - 0.6, h + 0.06, d * 0.5),
        "#bba98a",
        ROOF,
        Some(MAT_LIMESTONE),
    );
    crenellate(b, Vec3::new(-w * 0.5, 0.0, -d * 0.5), Vec3::new(w * 0.5, 0.0, -d * 0.5), h, 0.6, Vec3::NEG_Z);
    crenellate(b, Vec3::new(-w * 0.5, 0.0, -d * 0.5 + 0.6), Vec3::new(-w * 0.5, 0.0, d * 0.5), h, 0.6, Vec3::NEG_X);
    crenellate(b, Vec3::new(w * 0.5, 0.0, -d * 0.5 + 0.6), Vec3::new(w * 0.5, 0.0, d * 0.5), h, 0.6, Vec3::X);
    // Garita on the salient corner.
    let g = Vec3::new(-w * 0.5 - 0.2, h, -d * 0.5 - 0.2);
    b.decor(
        Shape::Cylinder { half_height: 1.2, radius: 0.85 },
        g + Vec3::Y * 1.25,
        Quat::IDENTITY,
        "#ddd0b2",
        true,
        Some(MAT_LIMESTONE),
    );
    b.decor(Shape::Sphere { radius: 0.88 }, g + Vec3::Y * 2.45, Quat::IDENTITY, "#ddd0b2", false, Some(MAT_LIMESTONE));
    b.decor(
        Shape::Cylinder { half_height: 0.6, radius: 0.6 },
        g + Vec3::new(-0.2, 0.0, -0.2) - Vec3::Y * 0.4,
        Quat::IDENTITY,
        "#c8b892",
        false,
        Some(MAT_LIMESTONE),
    );
    b.decor_box(g + Vec3::new(0.55, 1.4, 0.55), Vec3::new(0.12, 0.8, 0.5), GLASS, None);
    b.marker("garita", g + Vec3::Y * 0.1);
    for (i, x) in [-w * 0.2, w * 0.2].iter().enumerate() {
        cannon(b, Vec3::new(*x, h, -d * 0.5 + 1.6), Vec3::NEG_Z);
        b.marker(&format!("cannon{}", i + 1), Vec3::new(*x, h, -d * 0.5 + 1.6));
    }
    b.marker("top", Vec3::new(0.0, h, 0.0));
}

/// A gun battery: a raised platform with a thick front parapet cut by embrasures, 3–4 cannons
/// (and their spots as markers), powder kegs, steps up at the back.
fn battery(b: &mut B, def: &StructureDef, w: f32, d: f32) {
    let h = def.height.unwrap_or(2.5);
    let n = def.cannons.unwrap_or(4).clamp(3, 4);
    let low = b.lowest(Vec2::ZERO, w, d) - b.origin.y - 0.6;
    b.mass(Vec3::new(-w * 0.5, low, -d * 0.5), Vec3::new(w * 0.5, h, d * 0.5), LIMESTONE, MAT_LIMESTONE);
    b.block(
        Vec3::new(-w * 0.5 + 0.1, h, -d * 0.5 + 1.6),
        Vec3::new(w * 0.5 - 0.1, h + 0.06, d * 0.5 - 0.1),
        "#9a7a52",
        ROOF,
        Some("MAT/packed_earth"),
    );
    // Front parapet with embrasures between merlons.
    let gap = 1.2;
    let merlons = n + 1;
    let mw = (w - gap * n as f32) / merlons as f32;
    for i in 0..merlons {
        let x0 = -w * 0.5 + i as f32 * (mw + gap);
        b.block(
            Vec3::new(x0, h, -d * 0.5),
            Vec3::new(x0 + mw, h + 1.3, -d * 0.5 + 1.6),
            "#d6c8a8",
            CLIMBABLE | ROOF,
            Some(MAT_LIMESTONE),
        );
    }
    for i in 0..n {
        let x = -w * 0.5 + mw + gap * 0.5 + i as f32 * (mw + gap);
        b.block(
            Vec3::new(x - gap * 0.5, h, -d * 0.5),
            Vec3::new(x + gap * 0.5, h + 0.55, -d * 0.5 + 1.6),
            "#cbbd9b",
            CLIMBABLE | ROOF,
            Some(MAT_LIMESTONE),
        );
        cannon(b, Vec3::new(x, h + 0.06, -d * 0.5 + 2.6), Vec3::NEG_Z);
        b.marker(&format!("cannon{}", i + 1), Vec3::new(x, h + 0.06, -d * 0.5 + 2.6));
    }
    // Sandbags and powder kegs at the back.
    for i in 0..3 {
        b.decor(
            Shape::RoundedBox { half: Vec3::new(0.45, 0.18, 0.28), radius: 0.12 },
            Vec3::new(w * 0.5 - 1.2 - i as f32 * 0.8, h + 0.24, d * 0.5 - 0.6),
            Quat::from_rotation_y(i as f32 * 0.3),
            "#cdb98a",
            false,
            Some(MAT_SAND),
        );
    }
    b.prop("LIB/core/barrel", Vec3::new(-w * 0.5 + 1.0, h + 0.06, d * 0.5 - 0.8), 0.0, 1.0, 3, true);
    b.prop("LIB/core/cask", Vec3::new(-w * 0.5 + 2.0, h + 0.06, d * 0.5 - 0.7), 90.0, 1.0, 0, true);
    if h > 0.4 {
        let floor = b.lowest(Vec2::new(w * 0.25, d * 0.5 + 1.0), 2.0, 2.0) - b.origin.y - 0.3;
        stairs(
            b,
            Vec3::new(w * 0.25, 0.0, d * 0.5 + (h / 0.3).ceil() * 0.34),
            Vec3::NEG_Z,
            h,
            1.6,
            floor,
            STONE_DARK,
            MAT_LIMESTONE,
        );
    }
    b.marker("platform", Vec3::new(0.0, h, 0.0));
}

/// A mortar tower: a square stone tower (default 12 m), crenellated top, a mortar on it.
fn mortar_tower(b: &mut B, def: &StructureDef, w: f32, d: f32) {
    let h = def.height.unwrap_or(12.0);
    let low = b.lowest(Vec2::ZERO, w, d) - b.origin.y - 0.6;
    b.mass(
        Vec3::new(-w * 0.5 - 0.5, low, -d * 0.5 - 0.5),
        Vec3::new(w * 0.5 + 0.5, 2.5, d * 0.5 + 0.5),
        STONE_DARK,
        MAT_LIMESTONE,
    );
    b.mass(Vec3::new(-w * 0.5, 2.5, -d * 0.5), Vec3::new(w * 0.5, h, d * 0.5), LIMESTONE, MAT_LIMESTONE);
    b.block(
        Vec3::new(-w * 0.5 - 0.15, h - 0.4, -d * 0.5 - 0.15),
        Vec3::new(w * 0.5 + 0.15, h, d * 0.5 + 0.15),
        "#ddd0b2",
        CLIMBABLE | ROOF,
        Some(MAT_LIMESTONE),
    );
    for (a, e, out) in [
        (Vec3::new(-w * 0.5, 0.0, -d * 0.5), Vec3::new(w * 0.5, 0.0, -d * 0.5), Vec3::NEG_Z),
        (Vec3::new(-w * 0.5, 0.0, d * 0.5), Vec3::new(w * 0.5, 0.0, d * 0.5), Vec3::Z),
        (Vec3::new(-w * 0.5, 0.0, -d * 0.5 + 0.6), Vec3::new(-w * 0.5, 0.0, d * 0.5 - 0.6), Vec3::NEG_X),
        (Vec3::new(w * 0.5, 0.0, -d * 0.5 + 0.6), Vec3::new(w * 0.5, 0.0, d * 0.5 - 0.6), Vec3::X),
    ] {
        crenellate(b, a, e, h, 0.6, out);
    }
    // Arrow slits.
    for y in [4.5f32, 8.0] {
        b.decor_box(Vec3::new(0.0, y, d * 0.5 + 0.03), Vec3::new(0.25, 1.1, 0.1), GLASS, None);
        b.decor_box(Vec3::new(w * 0.5 + 0.03, y, 0.0), Vec3::new(0.1, 1.1, 0.25), GLASS, None);
    }
    // The mortar: a squat iron tube tilted up on a timber bed.
    b.decor(
        Shape::Box { half: Vec3::new(0.7, 0.25, 0.7) },
        Vec3::new(0.0, h + 0.25, 0.0),
        Quat::IDENTITY,
        TIMBER,
        true,
        Some(MAT_PLANKS),
    );
    b.decor(
        Shape::Cylinder { half_height: 0.5, radius: 0.42 },
        Vec3::new(0.0, h + 0.85, -0.1),
        Quat::from_rotation_x(-0.6),
        IRON,
        false,
        Some(MAT_IRON),
    );
    b.decor(Shape::Sphere { radius: 0.5 }, Vec3::new(0.0, h + 0.6, 0.15), Quat::IDENTITY, IRON, false, Some(MAT_IRON));
    b.marker("mortar", Vec3::new(0.0, h, 0.0));
    b.marker("top", Vec3::new(1.5, h, 1.5));
    // Ladders up the back: onto the base, then up the tower face.
    b.ladder(Vec3::new(-0.4, low.max(-0.5), -d * 0.5 - 0.8), Vec3::new(0.4, 2.5, -d * 0.5 - 0.5), Facing::South);
    b.ladder(Vec3::new(-0.4, 2.5, -d * 0.5 - 0.3), Vec3::new(0.4, h, -d * 0.5), Facing::South);
    b.marker("ladder_top", Vec3::new(0.0, h, -d * 0.5 + 0.5));
}

/// The keep: three floors of stone, the Commodore's balcony, a bell tower on a corner and a
/// flagpole on the roof.
fn keep(b: &mut B, def: &StructureDef, w: f32, d: f32) {
    let st = style_of(b, def, "limestone");
    let floors = def.floors.unwrap_or(3).clamp(2, 4);
    let h = floors as f32 * (FLOOR + 0.4);
    let p = PLINTH;
    b.foundation(Vec2::ZERO, w + 0.4, d + 0.4, p, STONE_DARK);
    b.mass(Vec3::new(-w * 0.5, p, -d * 0.5), Vec3::new(w * 0.5, p + h, d * 0.5), st.wall, st.mat);
    let top = p + h;
    // Crenellated roof.
    b.block(
        Vec3::new(-w * 0.5 + 0.6, top, -d * 0.5 + 0.6),
        Vec3::new(w * 0.5 - 0.6, top + 0.06, d * 0.5 - 0.6),
        "#bba98a",
        ROOF,
        Some(MAT_LIMESTONE),
    );
    for (a, e, out) in [
        (Vec3::new(-w * 0.5, 0.0, -d * 0.5), Vec3::new(w * 0.5, 0.0, -d * 0.5), Vec3::NEG_Z),
        (Vec3::new(-w * 0.5, 0.0, d * 0.5), Vec3::new(w * 0.5, 0.0, d * 0.5), Vec3::Z),
        (Vec3::new(-w * 0.5, 0.0, -d * 0.5 + 0.6), Vec3::new(-w * 0.5, 0.0, d * 0.5 - 0.6), Vec3::NEG_X),
        (Vec3::new(w * 0.5, 0.0, -d * 0.5 + 0.6), Vec3::new(w * 0.5, 0.0, d * 0.5 - 0.6), Vec3::X),
    ] {
        crenellate(b, a, e, top, 0.6, out);
    }
    b.marker("roof", Vec3::new(0.0, top + 0.06, 0.0));
    // Floor bands.
    for f in 1..floors {
        let y = p + f as f32 * (FLOOR + 0.4);
        b.block(
            Vec3::new(-w * 0.5 - 0.1, y - 0.1, d * 0.5 - 0.1),
            Vec3::new(w * 0.5 + 0.1, y + 0.1, d * 0.5 + 0.12),
            "#ddd0b2",
            CLIMBABLE | ROOF,
            Some(MAT_LIMESTONE),
        );
    }
    door(b, Vec3::new(0.0, p, d * 0.5), Vec3::Z, 2.0, 3.0, TIMBER, "#ddd0b2");
    b.marker("door", Vec3::new(0.0, p, d * 0.5 + 1.0));
    let n = ((w / 3.2) as i32).max(2);
    for f in 0..floors {
        for i in 0..n {
            let x = -w * 0.5 + (i as f32 + 0.5) * (w / n as f32);
            if f == 0 && x.abs() < 1.6 {
                continue;
            }
            window(b, Vec3::new(x, p + f as f32 * (FLOOR + 0.4) + 1.9, d * 0.5), Vec3::Z, 0.9, 1.5, "#2c3e66", "#ddd0b2", f > 0);
        }
    }
    // The Commodore's balcony on the top floor.
    let by = p + (floors - 1) as f32 * (FLOOR + 0.4);
    let (len, depth) = (4.4, 1.4);
    b.block(
        Vec3::new(-len * 0.5, by - 0.2, d * 0.5),
        Vec3::new(len * 0.5, by, d * 0.5 + depth),
        "#ddd0b2",
        ROOF,
        Some(MAT_LIMESTONE),
    );
    b.block(
        Vec3::new(-len * 0.5, by, d * 0.5 + depth - 0.2),
        Vec3::new(len * 0.5, by + 1.0, d * 0.5 + depth),
        "#ddd0b2",
        ROOF | CLIMBABLE,
        Some(MAT_LIMESTONE),
    );
    for s in [-1.0f32, 1.0] {
        b.block(
            Vec3::new(s * len * 0.5 - 0.1, by, d * 0.5),
            Vec3::new(s * len * 0.5 + 0.1, by + 1.0, d * 0.5 + depth),
            "#ddd0b2",
            ROOF | CLIMBABLE,
            Some(MAT_LIMESTONE),
        );
        b.decor_box(
            Vec3::new(s * len * 0.35, by - 0.6, d * 0.5 + 0.5),
            Vec3::new(0.25, 0.8, 0.9),
            STONE_DARK,
            Some(MAT_LIMESTONE),
        );
    }
    door(b, Vec3::new(0.0, by, d * 0.5), Vec3::Z, 1.4, 2.4, "#2c3e66", "#ddd0b2");
    b.marker("balcony", Vec3::new(0.0, by, d * 0.5 + depth * 0.5));
    // Bell tower on the back right corner.
    let t = 4.0;
    let th = top + 7.0;
    let (x0, z0) = (w * 0.5 - t, -d * 0.5);
    b.mass(Vec3::new(x0, top, z0), Vec3::new(x0 + t, th - 3.0, z0 + t), st.wall, st.mat);
    for (x, z) in [(x0, z0), (x0 + t - 0.6, z0), (x0, z0 + t - 0.6), (x0 + t - 0.6, z0 + t - 0.6)] {
        b.mass(Vec3::new(x, th - 3.0, z), Vec3::new(x + 0.6, th - 0.4, z + 0.6), st.wall, st.mat);
    }
    b.mass(Vec3::new(x0 - 0.2, th - 0.4, z0 - 0.2), Vec3::new(x0 + t + 0.2, th, z0 + t + 0.2), "#ddd0b2", MAT_LIMESTONE);
    let bell = Vec3::new(x0 + t * 0.5, th - 1.8, z0 + t * 0.5);
    b.decor(Shape::Cylinder { half_height: 0.4, radius: 0.45 }, bell, Quat::IDENTITY, "#b8862e", false, Some("MAT/pb/gold"));
    b.decor(Shape::Sphere { radius: 0.33 }, bell + Vec3::Y * 0.35, Quat::IDENTITY, "#b8862e", false, Some("MAT/pb/gold"));
    b.marker("bell", bell);
    b.marker("bell_tower_top", Vec3::new(x0 + t * 0.5, th, z0 + t * 0.5));
    b.area(
        "bell_perch",
        AreaKind::Perch,
        Vec3::new(x0 - 0.2, th - 0.1, z0 - 0.2),
        Vec3::new(x0 + t + 0.2, th + 2.0, z0 + t + 0.2),
    );
    // Flagpole with the flag.
    let fp = Vec3::new(-w * 0.25, top + 0.06, 0.0);
    b.block(fp + Vec3::new(-0.08, 0.0, -0.08), fp + Vec3::new(0.08, 9.0, 0.08), "#e8e8ee", POLE, None);
    b.decor_box(fp + Vec3::new(0.0, 0.25, 0.0), Vec3::new(0.7, 0.5, 0.7), STONE_DARK, Some(MAT_LIMESTONE));
    b.decor(
        Shape::Box { half: Vec3::new(1.1, 0.7, 0.02) },
        fp + Vec3::new(1.2, 8.1, 0.0),
        Quat::IDENTITY,
        "#2c3e66",
        false,
        Some(MAT_CANVAS),
    );
    b.sway_last(Sway::Leaves);
    b.decor(
        Shape::Box { half: Vec3::new(0.5, 0.3, 0.025) },
        fp + Vec3::new(1.2, 8.1, 0.0),
        Quat::IDENTITY,
        "#e8e8ee",
        false,
        None,
    );
    b.sway_last(Sway::Leaves);
    b.marker("flagpole", fp);
    b.marker("flag", fp + Vec3::Y * 9.0);
}

/// A gatehouse: two piers flanking a 4 m passage, a lintel and a crenellated walkway on top,
/// and two timber leaves that close the passage or stand open against its walls.
fn gate(b: &mut B, def: &StructureDef, w: f32, d: f32) {
    let h = def.height.unwrap_or(9.0);
    let gap = 4.0f32.min(w - 2.0);
    let pw = (w - gap) * 0.5;
    let low = b.lowest(Vec2::ZERO, w, d) - b.origin.y - 0.6;
    for s in [-1.0f32, 1.0] {
        let (x0, x1) = if s < 0.0 { (-w * 0.5, -gap * 0.5) } else { (gap * 0.5, w * 0.5) };
        b.mass(Vec3::new(x0, low, -d * 0.5), Vec3::new(x1, h, d * 0.5), LIMESTONE, MAT_LIMESTONE);
        // Pilasters.
        b.block(
            Vec3::new(s * (gap * 0.5 + 0.1) - 0.35, 0.0, d * 0.5),
            Vec3::new(s * (gap * 0.5 + 0.1) + 0.35, 5.6, d * 0.5 + 0.25),
            "#ddd0b2",
            CLIMBABLE,
            Some(MAT_LIMESTONE),
        );
        let _ = pw;
    }
    let arch = 4.8;
    b.mass(Vec3::new(-gap * 0.5, arch, -d * 0.5), Vec3::new(gap * 0.5, h, d * 0.5), LIMESTONE, MAT_LIMESTONE);
    b.block(
        Vec3::new(-gap * 0.5 - 0.6, arch - 0.1, d * 0.5),
        Vec3::new(gap * 0.5 + 0.6, arch + 0.6, d * 0.5 + 0.3),
        "#ddd0b2",
        CLIMBABLE | ROOF,
        Some(MAT_LIMESTONE),
    );
    // Coat of arms over the passage.
    b.decor_box(Vec3::new(0.0, arch + 1.6, d * 0.5 + 0.08), Vec3::new(1.4, 1.6, 0.16), "#e8e8ee", Some(MAT_LIMESTONE));
    b.decor_box(Vec3::new(0.0, arch + 1.6, d * 0.5 + 0.18), Vec3::new(0.8, 1.0, 0.06), "#2c3e66", None);
    crenellate(b, Vec3::new(-w * 0.5, 0.0, d * 0.5), Vec3::new(w * 0.5, 0.0, d * 0.5), h, 0.6, Vec3::Z);
    crenellate(b, Vec3::new(-w * 0.5, 0.0, -d * 0.5), Vec3::new(w * 0.5, 0.0, -d * 0.5), h, 0.6, Vec3::NEG_Z);
    b.marker("top", Vec3::new(0.0, h, 0.0));
    // The leaves: closed across the passage at its middle, open folded against its walls.
    let leaf = gap * 0.5;
    let lh = arch - 0.05;
    let wood = "#5a3f28";
    let c1 = b.block(Vec3::new(-gap * 0.5, 0.0, -0.15), Vec3::new(0.0, lh, 0.15), wood, CLIMBABLE, Some(MAT_PLANKS));
    let c2 = b.block(Vec3::new(0.0, 0.0, -0.15), Vec3::new(gap * 0.5, lh, 0.15), wood, CLIMBABLE, Some(MAT_PLANKS));
    // Open: swung inward (local −z) against the passage walls.
    let o1 = b.block(
        Vec3::new(-gap * 0.5, 0.0, 0.15 - leaf),
        Vec3::new(-gap * 0.5 + 0.3, lh, 0.15),
        wood,
        CLIMBABLE,
        Some(MAT_PLANKS),
    );
    let o2 =
        b.block(Vec3::new(gap * 0.5 - 0.3, 0.0, 0.15 - leaf), Vec3::new(gap * 0.5, lh, 0.15), wood, CLIMBABLE, Some(MAT_PLANKS));
    let name = b.name.clone();
    b.out.gates.push(GateDef { name, closed: vec![c1, c2], open: vec![o1, o2], start_open: def.open });
    b.marker("outside", Vec3::new(0.0, 0.0, d * 0.5 + 2.0));
    b.marker("inside", Vec3::new(0.0, 0.0, -d * 0.5 - 2.0));
    // Torches either side.
    for s in [-1.0f32, 1.0] {
        b.glow(Shape::Sphere { radius: 0.14 }, Vec3::new(s * (gap * 0.5 + 0.9), 3.4, d * 0.5 + 0.45), "#ffb347", 3.0);
        b.decor_box(Vec3::new(s * (gap * 0.5 + 0.9), 3.1, d * 0.5 + 0.3), Vec3::new(0.08, 0.5, 0.3), IRON, None);
    }
}

/// A 28 m lighthouse: a tapering striped shaft, a gallery with a railing, the lantern and a
/// perch on its roof.
fn lighthouse(b: &mut B, def: &StructureDef, w: f32, _d: f32) {
    let total = def.height.unwrap_or(28.0);
    let low = b.lowest(Vec2::ZERO, w + 1.0, w + 1.0) - b.origin.y - 0.6;
    let white = "#efe6d2";
    let red = "#a8322d";
    b.mass(
        Vec3::new(-w * 0.5 - 0.5, low, -w * 0.5 - 0.5),
        Vec3::new(w * 0.5 + 0.5, 1.2, w * 0.5 + 0.5),
        STONE_DARK,
        MAT_LIMESTONE,
    );
    // Shaft in stacked bands that narrow upward.
    let gallery = total - 4.0;
    let bands = 6;
    for i in 0..bands {
        let f0 = i as f32 / bands as f32;
        let f1 = (i + 1) as f32 / bands as f32;
        let half = w * 0.5 * (1.0 - 0.32 * f0);
        let y0 = 1.2 + (gallery - 1.2) * f0;
        let y1 = 1.2 + (gallery - 1.2) * f1;
        let color = if i % 2 == 0 { white } else { red };
        b.mass(Vec3::new(-half, y0, -half), Vec3::new(half, y1, half), color, MAT_PLASTER);
    }
    // Gallery and railing.
    let g = w * 0.5 * 0.85 + 0.6;
    b.block(Vec3::new(-g, gallery, -g), Vec3::new(g, gallery + 0.3, g), STONE_DARK, ROOF | CLIMBABLE, Some(MAT_LIMESTONE));
    for (a, e) in [
        (Vec3::new(-g, 0.0, -g), Vec3::new(g, 0.0, -g + 0.06)),
        (Vec3::new(-g, 0.0, g - 0.06), Vec3::new(g, 0.0, g)),
        (Vec3::new(-g, 0.0, -g), Vec3::new(-g + 0.06, 0.0, g)),
        (Vec3::new(g - 0.06, 0.0, -g), Vec3::new(g, 0.0, g)),
    ] {
        b.block(a + Vec3::Y * (gallery + 1.2), e + Vec3::Y * (gallery + 1.3), IRON, BEAM, None);
    }
    b.marker("gallery", Vec3::new(0.0, gallery + 0.3, g - 0.6));
    // Lantern room: glowing glass, a dark roof whose top is the perch.
    let l = 1.25;
    b.glow(Shape::Box { half: Vec3::new(l, 1.1, l) }, Vec3::new(0.0, gallery + 1.4, 0.0), "#ffe7a0", 2.2);
    b.block(Vec3::new(-l - 0.05, gallery + 0.3, -l - 0.05), Vec3::new(l + 0.05, gallery + 0.35, l + 0.05), IRON, 0, None);
    b.block(
        Vec3::new(-l - 0.25, gallery + 2.5, -l - 0.25),
        Vec3::new(l + 0.25, total, l + 0.25),
        "#2e3b3a",
        ROOF | CLIMBABLE,
        None,
    );
    b.decor(
        Shape::Sphere { radius: 0.3 },
        Vec3::new(0.0, total + 0.25, 0.0),
        Quat::IDENTITY,
        "#c9a24a",
        false,
        Some("MAT/pb/gold"),
    );
    b.area("perch", AreaKind::Perch, Vec3::new(-l - 0.25, total - 0.2, -l - 0.25), Vec3::new(l + 0.25, total + 2.0, l + 0.25));
    b.marker("perch", Vec3::new(0.0, total, l - 0.2));
    b.marker("light", Vec3::new(0.0, gallery + 1.4, 0.0));
    door(b, Vec3::new(0.0, 1.2, w * 0.5), Vec3::Z, 1.0, 2.0, TIMBER, white);
    b.marker("door", Vec3::new(0.0, 1.2, w * 0.5 + 0.8));
    // Two small windows up the shaft.
    for (i, y) in [8.0f32, 16.0].iter().enumerate() {
        let half = w * 0.5 * (1.0 - 0.32 * (y - 1.2) / (gallery - 1.2));
        let f = if i % 2 == 0 { Vec3::Z } else { Vec3::NEG_Z };
        b.decor_box(Vec3::new(0.0, *y, f.z * half) + f * 0.03, Vec3::new(0.5, 0.8, 0.1), GLASS, None);
    }
}

/// A ruined watchtower: four broken walls of different heights, a fallen floor, rubble.
fn ruin(b: &mut B, def: &StructureDef, w: f32, d: f32) {
    let h = def.height.unwrap_or(9.0);
    let t = 0.7;
    let low = b.lowest(Vec2::ZERO, w, d) - b.origin.y - 0.5;
    let stone = "#b3a888";
    let heights = [h, h * (0.55 + b.r(1) * 0.2), h * (0.3 + b.r(2) * 0.2), h * (0.75 + b.r(3) * 0.2)];
    // Back, left, front (with a doorway), right.
    b.mass(Vec3::new(-w * 0.5, low, -d * 0.5), Vec3::new(w * 0.5, heights[0], -d * 0.5 + t), stone, MAT_LIMESTONE);
    b.mass(Vec3::new(-w * 0.5, low, -d * 0.5 + t), Vec3::new(-w * 0.5 + t, heights[1], d * 0.5), stone, MAT_LIMESTONE);
    b.mass(Vec3::new(w * 0.5 - t, low, -d * 0.5 + t), Vec3::new(w * 0.5, heights[3], d * 0.5), stone, MAT_LIMESTONE);
    b.mass(Vec3::new(-w * 0.5 + t, low, d * 0.5 - t), Vec3::new(-0.6, heights[2], d * 0.5), stone, MAT_LIMESTONE);
    b.mass(Vec3::new(0.6, low, d * 0.5 - t), Vec3::new(w * 0.5 - t, heights[2] * 0.8, d * 0.5), stone, MAT_LIMESTONE);
    // Broken top steps on the tall wall.
    b.mass(Vec3::new(-w * 0.5, heights[0], -d * 0.5), Vec3::new(-w * 0.1, heights[0] + 1.1, -d * 0.5 + t), stone, MAT_LIMESTONE);
    // A half floor high up (a timber platform hanging on).
    b.block(
        Vec3::new(-w * 0.5 + t, h * 0.55, -d * 0.5 + t),
        Vec3::new(0.2, h * 0.55 + 0.18, -0.2),
        "#6b4a2f",
        ROOF,
        Some(MAT_PLANKS),
    );
    b.marker("top", Vec3::new(-w * 0.3, heights[0] + 1.1, -d * 0.5 + t * 0.5));
    b.marker("cache", Vec3::new(0.6, 0.0, 0.4));
    b.area(
        "perch",
        AreaKind::Perch,
        Vec3::new(-w * 0.5, heights[0] + 1.0, -d * 0.5),
        Vec3::new(-w * 0.1, heights[0] + 3.0, -d * 0.5 + t),
    );
    for i in 0..5 {
        let a = b.r(10 + i) * std::f32::consts::TAU;
        let r = w * 0.6 + b.r(20 + i) * 1.5;
        let s = 0.35 + b.r(30 + i) * 0.45;
        let pos = Vec3::new(a.cos() * r, 0.0, a.sin() * r);
        let g = b.ground_at(pos) - b.origin.y;
        b.decor(
            Shape::RoundedBox { half: Vec3::new(s * 1.3, s * 0.8, s), radius: s * 0.3 },
            pos + Vec3::Y * (g + s * 0.5),
            Quat::from_rotation_y(a * 3.0),
            stone,
            true,
            Some(MAT_LIMESTONE),
        );
    }
}

/// A fisherman's hut: timber walls and a thatched pitched roof.
fn hut(b: &mut B, def: &StructureDef, w: f32, d: f32) {
    let h = def.height.unwrap_or(2.4);
    b.foundation(Vec2::ZERO, w + 0.2, d + 0.2, 0.3, "#7a5232");
    b.mass(Vec3::new(-w * 0.5, 0.3, -d * 0.5), Vec3::new(w * 0.5, 0.3 + h, d * 0.5), "#7a5a3a", MAT_PLANKS);
    let pitch = 0.62f32;
    let half_w = w * 0.5 + 0.45;
    let slab = half_w / pitch.cos();
    let rise = half_w * pitch.tan();
    for s in [-1.0f32, 1.0] {
        let c = Vec3::new(s * half_w * 0.5, 0.3 + h + rise * 0.5 - 0.1, 0.0);
        b.decor(
            Shape::Box { half: Vec3::new(slab * 0.5, 0.16, d * 0.5 + 0.35) },
            c,
            Quat::from_rotation_z(-s * pitch),
            "#c8a456",
            true,
            Some(MAT_THATCH),
        );
    }
    for z in [d * 0.5, -d * 0.5 - 0.0] {
        let steps = 3;
        for i in 0..steps {
            let f = i as f32 / steps as f32;
            let hw = w * 0.5 * (1.0 - f);
            let z0 = if z > 0.0 { z - 0.15 } else { z };
            b.block(
                Vec3::new(-hw, 0.3 + h + rise * f * 0.8, z0),
                Vec3::new(hw, 0.3 + h + rise * (f + 0.34) * 0.8, z0 + 0.15),
                "#6b4a2f",
                0,
                Some(MAT_PLANKS),
            );
        }
    }
    door(b, Vec3::new(0.0, 0.3, d * 0.5), Vec3::Z, 0.9, 1.9, "#4a3320", "#6b4a2f");
    b.marker("door", Vec3::new(0.0, 0.3, d * 0.5 + 0.8));
    b.prop("LIB/core/barrel", Vec3::new(w * 0.5 + 0.5, 0.3, d * 0.3), 0.0, 1.0, 4, true);
}

// ---------------------------------------------------------------------------------------------
// Small things.

/// A laundry line (rope with cloth; a walkable tightrope) or a plank beam between two roofs.
/// Runs from `at` to `to` along the main axis; its height is the lower roof's parapet top
/// (roofs among the structures before it), or `base`, or 6 m.
fn line(b: &mut B, def: &StructureDef, laundry: bool) {
    let a = Vec2::new(def.at[0], def.at[1]);
    let e = def.to.map(|t| Vec2::new(t[0], t[1])).unwrap_or(a + Vec2::new(6.0, 0.0));
    let along_x = (e.x - a.x).abs() >= (e.y - a.y).abs();
    let e = if along_x { Vec2::new(e.x, a.y) } else { Vec2::new(a.x, e.y) };
    let ya = b.roof_below(a);
    let ye = b.roof_below(e);
    // Resting on the lower end's parapet (or roof) top.
    let y = def.base.unwrap_or_else(|| match (ya, ye) {
        (Some(p), Some(q)) => p.min(q) + 0.02,
        (Some(p), None) | (None, Some(p)) => p + 0.02,
        _ => (b.ground)(a) + 6.0,
    });
    let _ = laundry;
    b.origin = Vec3::new(0.0, 0.0, 0.0);
    b.q = 0;
    let (lo, hi) = (a.min(e), a.max(e));
    let len = (hi - lo).length();
    let mid = (lo + hi) * 0.5;
    b.out.footprints.clear();
    if laundry {
        let half = if along_x { Vec3::new(len * 0.5, 0.04, 0.04) } else { Vec3::new(0.04, 0.04, len * 0.5) };
        let c = Vec3::new(mid.x, y - 0.04, mid.y);
        b.block(c - half, c + half, ROPE, BEAM, Some(MAT_ROPE));
        // Cloth hanging from it: shirts, sheets, in bright colours.
        let colors = ["#efe6d2", "#c0392b", "#2f6b8a", "#e0b040", "#8fc1a9", "#e07a5f", "#7fb7d6"];
        let n = ((len - 1.0) / 0.9) as i32;
        for i in 0..n {
            if b.r(50 + i as u32) < 0.25 {
                continue;
            }
            let t = (i as f32 + 0.5) / n as f32;
            let p = lo.lerp(hi, 0.06 + t * 0.88);
            let (cw, ch) = (0.45 + b.r(60 + i as u32) * 0.4, 0.5 + b.r(70 + i as u32) * 0.5);
            let size = if along_x { Vec3::new(cw, ch, 0.02) } else { Vec3::new(0.02, ch, cw) };
            let color = colors[((b.r(80 + i as u32) * colors.len() as f32) as usize).min(colors.len() - 1)];
            b.decor(
                Shape::Box { half: size * 0.5 },
                Vec3::new(p.x, y - 0.09 - ch * 0.5, p.y),
                Quat::IDENTITY,
                color,
                false,
                Some(MAT_CANVAS),
            );
            b.sway_last(Sway::Leaves);
            // A peg.
            b.decor_box(Vec3::new(p.x, y - 0.06, p.y), Vec3::splat(0.05), "#8a6a48", None);
        }
    } else {
        let half = if along_x { Vec3::new(len * 0.5, 0.06, 0.16) } else { Vec3::new(0.16, 0.06, len * 0.5) };
        let c = Vec3::new(mid.x, y + 0.06, mid.y);
        b.block(c - half, c + half, "#8a6a48", BEAM | ROOF, Some(MAT_PLANKS));
    }
    b.out.markers.insert(format!("{}.start", b.name), Vec3::new(a.x, y, a.y));
    b.out.markers.insert(format!("{}.end", b.name), Vec3::new(e.x, y, e.y));
    b.origin = Vec3::new(mid.x, y, mid.y);
}

/// A palisade of sharpened stakes along local x (`size` = [length, thickness]), `height` 3 m.
fn palisade(b: &mut B, def: &StructureDef, w: f32, _d: f32) {
    let h = def.height.unwrap_or(3.0);
    let n = ((w / 0.4) as i32).max(2);
    for i in 0..n {
        let x = -w * 0.5 + (i as f32 + 0.5) * (w / n as f32);
        let g = b.ground_at(Vec3::new(x, 0.0, 0.0)) - b.origin.y;
        let sh = h * (0.9 + b.r(i as u32) * 0.2);
        let hh = (sh - g + 0.4) * 0.5;
        b.decor(
            Shape::Cylinder { half_height: hh, radius: 0.18 },
            Vec3::new(x, g - 0.4 + hh, 0.0),
            Quat::from_rotation_z((b.r(100 + i as u32) - 0.5) * 0.06),
            "#7a5a3a",
            true,
            Some(MAT_PLANKS),
        );
    }
    b.decor_box(Vec3::new(0.0, h * 0.7, 0.2), Vec3::new(w, 0.14, 0.1), "#5a3f28", None);
    b.decor_box(Vec3::new(0.0, h * 0.25, 0.2), Vec3::new(w, 0.14, 0.1), "#5a3f28", None);
}

/// Crates stacked into steps (a way up onto a roof).
fn crate_stack(b: &mut B, w: f32, d: f32) {
    let s = 1.0;
    let nx = ((w / s) as i32).max(1);
    let nz = ((d / s) as i32).max(1);
    let colors = ["#9a7048", "#8a6a48", "#a07850"];
    let mut k = 0;
    for j in 0..nz {
        for i in 0..nx {
            let levels = 1 + ((nx - 1 - i) + (nz - 1 - j)).min(2) as i32 - (b.r(k) * 1.4) as i32;
            for l in 0..levels.max(1) {
                let x = -w * 0.5 + i as f32 * s + 0.03;
                let z = -d * 0.5 + j as f32 * s + 0.03;
                let c = colors[((b.r(100 + k) * 3.0) as usize).min(2)];
                b.block(
                    Vec3::new(x, l as f32 * s, z),
                    Vec3::new(x + s - 0.06, (l + 1) as f32 * s - 0.02, z + s - 0.06),
                    c,
                    CLIMBABLE | ROOF,
                    Some(MAT_PLANKS),
                );
                k += 1;
            }
        }
    }
    b.marker("top", Vec3::new(-w * 0.5 + 0.5, 3.0, -d * 0.5 + 0.5));
}

/// A wanted poster on a post (or, with `base`, on a wall at that height facing the front).
fn poster(b: &mut B, def: &StructureDef) {
    let on_wall = def.base.is_some();
    let y = if on_wall { def.base.unwrap_or(1.6) - b.origin.y } else { 1.6 };
    if !on_wall {
        b.block(Vec3::new(-0.07, 0.0, -0.12), Vec3::new(0.07, 2.1, 0.02), TIMBER, POLE, Some(MAT_PLANKS));
    }
    b.decor_box(Vec3::new(0.0, y, 0.05), Vec3::new(0.6, 0.8, 0.02), "#e9dcb8", None);
    b.decor_box(Vec3::new(0.0, y + 0.05, 0.065), Vec3::new(0.32, 0.36, 0.01), "#6b5a45", None);
    b.label(def.sign.as_deref().unwrap_or("WANTED"), Vec3::new(0.0, y + 0.3, 0.08), 0.1, Facing::South, "#3a2414");
    b.marker("poster", Vec3::new(0.0, y, 0.6));
}

/// Cobbled paving: a slab 6 cm proud of the highest ground under it, filled down to the lowest.
fn paving(b: &mut B, def: &StructureDef, w: f32, d: f32) {
    let low = b.lowest(Vec2::ZERO, w, d) - b.origin.y - 0.3;
    let color = def.style.as_deref().and_then(style).map_or("#c9b99a", |s| s.wall);
    b.block(Vec3::new(-w * 0.5, low, -d * 0.5), Vec3::new(w * 0.5, 0.06, d * 0.5), color, ROOF, Some("MAT/cobblestone"));
    b.out.footprints.clear();
}
