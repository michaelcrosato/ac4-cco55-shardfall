//! Layered 2.5D navigation (PLAN §9.11 "Nav 2.5D"): a grid over the ground plane where each
//! column holds up to K walkable surfaces (terrain, streets, floors, roofs, wall walks, stairs),
//! so a building's roof and the street under it are separate layers of one column.
//!
//! **Building.** Every column is probed from `top` down through the fixed colliders of the
//! physics world (height-field terrain, static blocks, decor, baked props, fixed bodies; never
//! characters or anything dynamic): each hit facing up is a surface, and the probe goes on from
//! just under it, so lower layers are found too. A surface is walkable when
//! - its slope is gentle (normal.y ≥ cos `max_slope`, 40°),
//! - it is not inside a solid and has `height` (1.8 m) of head room,
//! - it is not deeper than `wade` under a water zone,
//! - a character capsule (`radius` 0.35 m) standing on it misses everything that reaches above
//!   knee height (`low`, 0.9 m over the feet): walls, pillars, crates stacked high, merlons.
//!   Lower things (steps, curbs, a cart's bed) don't grow by the radius, so stairs stay open;
//!   they only cut the edges that would cross them.
//!
//! Surfaces with no neighbour at all (a bollard's top, a chimney pot) are dropped. The colliders
//! are read straight from the collider set (each column tests only the colliders whose box covers
//! it), so the grid can be built right after a level is placed, before the physics has stepped,
//! and building it twice gives the same bytes.
//!
//! **Edges.** Neighbouring cells (8-connected; diagonals only where both orthogonal ways are open,
//! so paths never cut corners) join on layers whose heights differ by at most `step` (0.45 m),
//! or by up to two steps where the surface halfway is a step from both (stairs whose treads are
//! shorter than a cell). Explicit links join anything else: ladders (read from the static world)
//! and whatever game code adds with [`NavGrid25::add_link`]. Gates close cells with
//! [`NavGrid25::set_blocked_area`].
//!
//! **Queries.** A* paths (octile distance with height as the heuristic, weighted by `greed`;
//! string-pulled so walkers go straight where they can), reach-limited flow fields for packs,
//! nearest walkable points, a weighted variant of `path` for costs per cell (cells guards watch,
//! grass that hides), and [`NavGrid25::connected`], which answers "is there any way?" at once
//! (a guard asking how to reach a hero on a roof gets None without a search).
//!
//! The grid is a cache: the big arrays sit behind `Arc`, so cloning is cheap, and it implements
//! `Default`, so games keep it as a `#[serde(skip)]` field outside the snapshot and rebuild it at
//! level load.

use std::collections::{BTreeMap, BinaryHeap};
use std::sync::Arc;

use glam::{Quat, Vec2, Vec3};
use rapier::parry::query::intersection_test;
use rapier::parry::shape::{Capsule, Cuboid, ShapeType};
use rapier::prelude::*;

use crate::sim::Sim;
use crate::zones::ZoneKind;

/// The most layers a column can keep (the neighbour layer index is packed in 3 bits).
pub const MAX_LAYERS: usize = 8;

/// Neighbour directions in grid steps (x, z): E, W, S, N, SE, NE, SW, NW.
const DIRS: [(i32, i32); 8] = [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1)];
const OPP: [usize; 8] = [1, 0, 3, 2, 7, 6, 5, 4];
/// Columns per side of a collider bucket (the build's spatial index).
const BUCKET: usize = 16;
/// How far a straight walk may skip ahead while string-pulling (nodes).
const MAX_SKIP: usize = 64;

/// What counts as walkable, and how the grid is probed.
#[derive(Clone, Debug, PartialEq)]
pub struct Nav25Config {
    /// Cell size (m).
    pub cell: f32,
    /// Probes start here: nothing higher is walkable.
    pub top: f32,
    /// ... and stop here.
    pub bottom: f32,
    /// A walker's radius: tall obstacles grow by it.
    pub radius: f32,
    /// Head room a surface needs (m).
    pub height: f32,
    /// The highest step up or down between neighbouring cells (m).
    pub step: f32,
    /// The steepest walkable slope (degrees).
    pub max_slope: f32,
    /// Obstacles reaching higher than this above the feet grow by `radius`; lower ones (steps,
    /// curbs) only cut the edges that cross them.
    pub low: f32,
    /// Walkable surfaces kept per column (lowest first; at most [`MAX_LAYERS`]).
    pub layers: usize,
    /// The deepest water (m) a walker wades through; deeper surfaces aren't walkable.
    pub wade: f32,
    /// Link the foot and the top of every ladder in the static world.
    pub ladders: bool,
    /// A ladder link's cost per metre of its length (climbing is slow).
    pub ladder_cost: f32,
}

impl Default for Nav25Config {
    fn default() -> Self {
        Self {
            cell: 0.5,
            top: 60.0,
            bottom: -20.0,
            radius: 0.35,
            height: 1.8,
            step: 0.45,
            max_slope: 40.0,
            low: 0.9,
            layers: 4,
            wade: 0.5,
            ladders: true,
            ladder_cost: 3.0,
        }
    }
}

/// The grid's big arrays (shared between clones).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Layers {
    /// Column `c` (row-major: `z * w + x`) holds nodes `start[c]..start[c + 1]`, lowest first.
    pub start: Vec<u32>,
    /// Each node's surface height.
    pub y: Vec<f32>,
    /// Each node's column as `x | z << 16`.
    pub xz: Vec<u32>,
    /// Each node's edges: bit d (0..8) = joined toward `DIRS[d]`; bits `8 + 3d..` = which layer of
    /// that neighbour column.
    pub nbr: Vec<u32>,
    /// Each node's connected component over the grid's edges (links join components).
    pub comp: Vec<u32>,
}

/// Why a link exists.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkKind {
    /// Added by game code ([`NavGrid25::add_link`]).
    Custom,
    /// A ladder's foot and top.
    Ladder,
}

/// An explicit one-way edge between two nodes (two-way links are two of these).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Link25 {
    pub from: u32,
    pub to: u32,
    pub cost: f32,
    pub kind: LinkKind,
}

#[derive(Clone, Debug, Default, PartialEq)]
struct Links {
    list: Vec<Link25>,
    /// Node → indices into `list` of links leaving it / arriving at it.
    out: BTreeMap<u32, Vec<u32>>,
    inn: BTreeMap<u32, Vec<u32>>,
    /// Union-find over components: links join them.
    parent: Vec<u32>,
}

impl Links {
    fn find(&self, mut c: u32) -> u32 {
        while let Some(&p) = self.parent.get(c as usize) {
            if p == c {
                break;
            }
            c = p;
        }
        c
    }
}

/// One walkable surface of one column.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NavCell {
    pub x: usize,
    pub z: usize,
    /// Which layer of its column (0 = lowest).
    pub layer: usize,
    /// Node index (into [`Layers`]).
    pub node: usize,
    /// The cell's centre on the surface.
    pub pos: Vec3,
    /// Closed (a shut gate).
    pub blocked: bool,
}

/// The layered walkable grid. Cheap to clone; `Default` is an empty grid.
#[derive(Clone)]
pub struct NavGrid25 {
    /// The grid's corner on the ground plane (x, z).
    pub origin: Vec2,
    pub cell: f32,
    /// Columns along x and z.
    pub w: usize,
    pub h: usize,
    /// The step height edges were built with.
    pub step: f32,
    /// How much A* trusts its heuristic: 1 = always the shortest path; above 1 searches far
    /// fewer nodes for paths a little longer (1.25: a few percent, mostly removed by
    /// string-pulling).
    pub greed: f32,
    pub layers: Arc<Layers>,
    /// One bit per node: closed.
    blocked: Arc<Vec<u64>>,
    links: Arc<Links>,
}

/// The default [`NavGrid25::greed`].
pub const GREED: f32 = 1.25;

impl Default for NavGrid25 {
    fn default() -> Self {
        Self {
            origin: Vec2::ZERO,
            cell: 0.5,
            w: 0,
            h: 0,
            step: 0.45,
            greed: GREED,
            layers: Arc::new(Layers { start: vec![0], ..Default::default() }),
            blocked: Arc::default(),
            links: Arc::default(),
        }
    }
}

impl std::fmt::Debug for NavGrid25 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NavGrid25")
            .field("origin", &self.origin)
            .field("cell", &self.cell)
            .field("w", &self.w)
            .field("h", &self.h)
            .field("nodes", &self.layers.y.len())
            .field("links", &self.links.list.len())
            .finish()
    }
}

// ---------------------------------------------------------------------------------------------
// Building.

#[derive(Clone, Copy, PartialEq)]
enum ObsKind {
    /// A height field or terrain patch: one surface, never an obstacle.
    Terrain,
    /// An axis-aligned box: its bounds are exact.
    Box,
    /// Anything else (rotated boxes, cylinders, props): probed through its shape.
    Shape,
}

struct Obs<'a> {
    co: &'a Collider,
    lo: Vec3,
    hi: Vec3,
    kind: ObsKind,
}

/// What the build reads: the fixed colliders near the grid, bucketed by column.
struct Scene<'a> {
    cfg: &'a Nav25Config,
    origin: Vec2,
    cell: f32,
    w: usize,
    h: usize,
    bw: usize,
    obs: Vec<Obs<'a>>,
    buckets: Vec<Vec<u32>>,
    water: Vec<(Vec3, Vec3)>,
    min_ny: f32,
}

impl Scene<'_> {
    fn bucket(&self, p: Vec2) -> &[u32] {
        let x = (((p.x - self.origin.x) / self.cell).floor().max(0.0) as usize).min(self.w - 1) / BUCKET;
        let z = (((p.y - self.origin.y) / self.cell).floor().max(0.0) as usize).min(self.h - 1) / BUCKET;
        &self.buckets[z * self.bw + x]
    }

    /// Every boundary the vertical line at `c` crosses: candidate surfaces (height, normal y),
    /// all crossing heights, and solid spans (bottom, top).
    fn probe(&self, c: Vec2, cand: &mut Vec<(f32, f32)>, ceil: &mut Vec<f32>, solid: &mut Vec<(f32, f32)>) {
        let (cfg, tol) = (self.cfg, 0.05);
        cand.clear();
        ceil.clear();
        solid.clear();
        for &oi in self.bucket(c) {
            let o = &self.obs[oi as usize];
            if c.x < o.lo.x || c.x > o.hi.x || c.y < o.lo.z || c.y > o.hi.z {
                continue;
            }
            match o.kind {
                ObsKind::Box => {
                    solid.push((o.lo.y, o.hi.y));
                    ceil.push(o.lo.y);
                    ceil.push(o.hi.y);
                    cand.push((o.hi.y, 1.0));
                }
                ObsKind::Terrain => {
                    let y0 = cfg.top.min(o.hi.y + tol);
                    if let Some((y, ny)) = cast_down(o.co, c, y0, y0 - cfg.bottom, false) {
                        ceil.push(y);
                        if ny > 0.0 {
                            cand.push((y, ny));
                        }
                    }
                }
                ObsKind::Shape => {
                    // Down through the shape: entries (facing up) and exits (facing down).
                    let mut y0 = cfg.top.min(o.hi.y + tol);
                    let mut entry = (o.hi.y + tol > cfg.top).then_some(f32::INFINITY);
                    for _ in 0..8 {
                        let Some((y, ny)) = cast_down(o.co, c, y0, y0 - cfg.bottom.max(o.lo.y - tol), false) else {
                            break;
                        };
                        ceil.push(y);
                        if ny >= 0.0 {
                            if ny > 0.0 {
                                cand.push((y, ny));
                            }
                            entry = Some(y);
                        } else {
                            solid.push((y, entry.take().unwrap_or(f32::INFINITY)));
                        }
                        y0 = y - 0.002;
                    }
                }
            }
        }
    }

    /// The first surface going down from `y0` to `y1` at `p` (height, normal y).
    fn first_hit_down(&self, p: Vec2, y0: f32, y1: f32) -> Option<(f32, f32)> {
        let mut best: Option<(f32, f32)> = None;
        for &oi in self.bucket(p) {
            let o = &self.obs[oi as usize];
            if p.x < o.lo.x || p.x > o.hi.x || p.y < o.lo.z || p.y > o.hi.z || o.lo.y > y0 || o.hi.y < y1 {
                continue;
            }
            let hit = match o.kind {
                ObsKind::Box => Some((o.hi.y.min(y0), 1.0)),
                _ => cast_down(o.co, p, y0, y0 - y1, true),
            };
            if let Some(h) = hit {
                if h.0 >= y1 && best.is_none_or(|b| h.0 > b.0) {
                    best = Some(h);
                }
            }
        }
        best
    }

    /// Whether a walker standing at `c` on height `y` misses every obstacle reaching above `low`.
    fn tall_clear(&self, c: Vec2, y: f32) -> bool {
        let cfg = self.cfg;
        let (r, b0, b1) = (cfg.radius, y + cfg.low, y + cfg.height);
        for &oi in self.bucket(c) {
            let o = &self.obs[oi as usize];
            if o.kind == ObsKind::Terrain || o.hi.y <= b0 || o.lo.y >= b1 {
                continue;
            }
            if c.x < o.lo.x - r || c.x > o.hi.x + r || c.y < o.lo.z - r || c.y > o.hi.z + r {
                continue;
            }
            let hit = match o.kind {
                ObsKind::Box => {
                    let d = (Vec2::new(o.lo.x, o.lo.z) - c).max(c - Vec2::new(o.hi.x, o.hi.z)).max(Vec2::ZERO);
                    d.length() < r
                }
                _ => {
                    let half = ((b1 - b0) * 0.5 - r).max(0.01);
                    let body = Capsule::new_y(half as Real, r as Real);
                    let pose = Pose::from_translation(Vec3::new(c.x, (b0 + b1) * 0.5, c.y));
                    intersection_test(&pose, &body, o.co.position(), o.co.shape()).is_ok_and(|i| i.intersecting)
                }
            };
            if hit {
                return false;
            }
        }
        true
    }

    /// Whether anything short of `low` stands near enough to `c` that edges from it need checking.
    fn near_low(&self, c: Vec2, y: f32) -> bool {
        let cfg = self.cfg;
        let (m, b0, b1) = (self.cell * 1.5, y + cfg.step, y + 2.0 * cfg.step + cfg.low);
        self.bucket(c).iter().any(|&oi| {
            let o = &self.obs[oi as usize];
            o.kind != ObsKind::Terrain
                && o.hi.y > b0
                && o.lo.y < b1
                && c.x >= o.lo.x - m
                && c.x <= o.hi.x + m
                && c.y >= o.lo.z - m
                && c.y <= o.hi.z + m
        })
    }

    /// Whether the straight step from `a` to `b` misses everything between a step and `low`
    /// over the higher end (a low wall, a bench, a cart).
    fn low_clear(&self, a: Vec3, b: Vec3) -> bool {
        let cfg = self.cfg;
        let top = a.y.max(b.y);
        let (b0, b1) = (top + cfg.step, top + cfg.low);
        let (a2, b2) = (Vec2::new(a.x, a.z), Vec2::new(b.x, b.z));
        let (lo, hi) = (a2.min(b2), a2.max(b2));
        for &oi in self.bucket((a2 + b2) * 0.5) {
            let o = &self.obs[oi as usize];
            if o.kind == ObsKind::Terrain || o.hi.y <= b0 || o.lo.y >= b1 {
                continue;
            }
            if hi.x < o.lo.x || lo.x > o.hi.x || hi.y < o.lo.z || lo.y > o.hi.z {
                continue;
            }
            let hit = match o.kind {
                ObsKind::Box => segment_hits_rect(a2, b2, Vec2::new(o.lo.x, o.lo.z), Vec2::new(o.hi.x, o.hi.z)),
                _ => {
                    let d = b2 - a2;
                    let blade = Cuboid::new(Vec3::new(d.length() * 0.5, (b1 - b0) * 0.5, 0.02));
                    let mid = (a2 + b2) * 0.5;
                    let pose =
                        Pose::from_parts(Vec3::new(mid.x, (b0 + b1) * 0.5, mid.y), Quat::from_rotation_y((-d.y).atan2(d.x)));
                    intersection_test(&pose, &blade, o.co.position(), o.co.shape()).is_ok_and(|i| i.intersecting)
                }
            };
            if hit {
                return false;
            }
        }
        true
    }

    /// The step from `a` to `b` (heights more than a step apart): the surface halfway must be a
    /// step from both (a stair tread between them).
    fn mid_step(&self, a: Vec3, b: Vec3) -> bool {
        let step = self.cfg.step;
        let m = Vec2::new(a.x + b.x, a.z + b.z) * 0.5;
        let Some((y, ny)) = self.first_hit_down(m, a.y.max(b.y) + 0.1, a.y.min(b.y) - 0.1) else { return false };
        ny >= self.min_ny && (y - a.y).abs() <= step && (y - b.y).abs() <= step
    }

    /// Whether a surface at `y` (column centre `c`) lies more than `wade` under a water zone.
    fn too_deep(&self, c: Vec2, y: f32) -> bool {
        self.water.iter().any(|(lo, hi)| {
            c.x >= lo.x && c.x < hi.x && c.y >= lo.z && c.y < hi.z && y >= lo.y && y <= hi.y && hi.y - y > self.cfg.wade
        })
    }
}

/// A vertical ray down at `p` from `y0` over `len` against one collider: the height and normal y.
fn cast_down(co: &Collider, p: Vec2, y0: f32, len: f32, solid: bool) -> Option<(f32, f32)> {
    if len <= 0.0 {
        return None;
    }
    let ray = Ray::new(Vec3::new(p.x, y0, p.y), Vec3::NEG_Y);
    let hit = co.shape().cast_ray_and_get_normal(co.position(), &ray, len as Real, solid)?;
    Some((y0 - hit.time_of_impact as f32, hit.normal.y as f32))
}

/// Whether segment `a`–`b` crosses the rectangle `lo`–`hi` (2D, Liang–Barsky).
fn segment_hits_rect(a: Vec2, b: Vec2, lo: Vec2, hi: Vec2) -> bool {
    let d = b - a;
    let (mut t0, mut t1) = (0.0f32, 1.0f32);
    for (pa, pd, l, h) in [(a.x, d.x, lo.x, hi.x), (a.y, d.y, lo.y, hi.y)] {
        if pd.abs() < 1e-9 {
            if pa < l || pa > h {
                return false;
            }
        } else {
            let (mut ta, mut tb) = ((l - pa) / pd, (h - pa) / pd);
            if ta > tb {
                std::mem::swap(&mut ta, &mut tb);
            }
            t0 = t0.max(ta);
            t1 = t1.min(tb);
            if t0 > t1 {
                return false;
            }
        }
    }
    true
}

fn pack(x: usize, z: usize) -> u32 {
    x as u32 | (z as u32) << 16
}

fn unpack(v: u32) -> (usize, usize) {
    ((v & 0xFFFF) as usize, (v >> 16) as usize)
}

impl NavGrid25 {
    /// Builds the grid over `min..max` (x, z) with `cell`-sized cells, probing down from `top`,
    /// with the default walker ([`Nav25Config`]) and every fixed collider.
    pub fn build(sim: &Sim, min: Vec2, max: Vec2, cell: f32, top: f32) -> NavGrid25 {
        let cfg = Nav25Config { cell, top, ..Default::default() };
        Self::build_with(sim, min, max, &cfg, &|_, _| true)
    }

    /// Builds the grid with a custom walker; `keep` leaves out fixed colliders it returns false
    /// for (entities that come and go, so the grid is the same whenever it is built).
    pub fn build_with(
        sim: &Sim,
        min: Vec2,
        max: Vec2,
        cfg: &Nav25Config,
        keep: &dyn Fn(ColliderHandle, &Collider) -> bool,
    ) -> NavGrid25 {
        let cell = cfg.cell.max(0.05);
        let w = (((max.x - min.x) / cell).ceil().max(1.0) as usize).min(0xFFFF);
        let h = (((max.y - min.y) / cell).ceil().max(1.0) as usize).min(0xFFFF);
        let k_max = cfg.layers.clamp(1, MAX_LAYERS);
        let ph = &sim.state.physics;
        let statics = &sim.state.statics;

        // Terrain patches (height fields, or trimeshes when a patch has no grid), water.
        let mut terrain: Vec<(u32, u32)> =
            statics.chunks.values().filter_map(|c| c.terrain_collider).map(|h| h.into_raw_parts()).collect();
        terrain.sort_unstable();
        let water: Vec<(Vec3, Vec3)> = statics
            .chunks
            .values()
            .flat_map(|c| c.zones.iter())
            .filter(|z| z.kind == ZoneKind::Water)
            .map(|z| (z.min, z.max))
            .collect();

        // Fixed colliders near the grid (the collider set's order, so builds repeat exactly).
        let margin = cfg.radius + cell * 1.5;
        let (lo2, hi2) = (min - Vec2::splat(margin), max + Vec2::splat(margin));
        let mut obs = Vec::new();
        for (hdl, co) in ph.colliders.iter() {
            if !co.is_enabled() || co.is_sensor() {
                continue;
            }
            if let Some(b) = co.parent() {
                if !ph.bodies.get(b).is_some_and(|b| b.is_fixed()) {
                    continue;
                }
            }
            if !keep(hdl, co) {
                continue;
            }
            let ab = co.compute_aabb();
            let (lo, hi) = (ab.mins, ab.maxs);
            if hi.x < lo2.x || hi.z < lo2.y || lo.x > hi2.x || lo.z > hi2.y || hi.y < cfg.bottom || lo.y > cfg.top + cfg.height {
                continue;
            }
            let shape = co.shape();
            let kind = if shape.shape_type() == ShapeType::HeightField || terrain.binary_search(&hdl.into_raw_parts()).is_ok() {
                ObsKind::Terrain
            } else if let Some(c) = shape.as_cuboid() {
                let vol = 8.0 * (c.half_extents.x * c.half_extents.y * c.half_extents.z) as f32;
                let avol = (hi - lo).x * (hi - lo).y * (hi - lo).z;
                if (avol - vol).abs() <= 1e-3 * vol.max(1e-6) + 1e-7 { ObsKind::Box } else { ObsKind::Shape }
            } else {
                ObsKind::Shape
            };
            obs.push(Obs { co, lo, hi, kind });
        }

        // Buckets of BUCKET × BUCKET columns, each listing the colliders near it.
        let (bw, bh) = (w.div_ceil(BUCKET), h.div_ceil(BUCKET));
        let mut buckets: Vec<Vec<u32>> = vec![Vec::new(); bw * bh];
        let col_of = |v: f32, o: f32, n: usize| (((v - o) / cell).floor().max(0.0) as usize).min(n - 1);
        for (i, o) in obs.iter().enumerate() {
            let (x0, x1) = (col_of(o.lo.x - margin, min.x, w) / BUCKET, col_of(o.hi.x + margin, min.x, w) / BUCKET);
            let (z0, z1) = (col_of(o.lo.z - margin, min.y, h) / BUCKET, col_of(o.hi.z + margin, min.y, h) / BUCKET);
            for bz in z0..=z1 {
                for bx in x0..=x1 {
                    buckets[bz * bw + bx].push(i as u32);
                }
            }
        }
        let scene = Scene { cfg, origin: min, cell, w, h, bw, obs, buckets, water, min_ny: cfg.max_slope.to_radians().cos() };

        // Columns: probe, keep the walkable surfaces.
        let mut l = Layers { start: Vec::with_capacity(w * h + 1), ..Default::default() };
        l.start.push(0);
        let mut low_flag: Vec<bool> = Vec::new();
        let (mut cand, mut ceil, mut solid, mut kept) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        for z in 0..h {
            for x in 0..w {
                let c = min + Vec2::new(x as f32 + 0.5, z as f32 + 0.5) * cell;
                scene.probe(c, &mut cand, &mut ceil, &mut solid);
                cand.sort_by(|a: &(f32, f32), b| a.0.total_cmp(&b.0));
                kept.clear();
                for &(y, ny) in &cand {
                    if ny < scene.min_ny || y > cfg.top || y < cfg.bottom {
                        continue;
                    }
                    let probe = y + 0.05;
                    if solid.iter().any(|&(lo, hi): &(f32, f32)| lo < probe && probe < hi) {
                        continue;
                    }
                    if ceil.iter().any(|&q: &f32| q > probe && q < y + cfg.height) {
                        continue;
                    }
                    if scene.too_deep(c, y) {
                        continue;
                    }
                    // Two surfaces this close are one (a paving slab on the terrain).
                    if let Some(last) = kept.last_mut() {
                        if y - *last < 0.1 {
                            *last = y;
                            continue;
                        }
                    }
                    kept.push(y);
                }
                let mut n = 0;
                for &y in &kept {
                    if n == k_max {
                        break;
                    }
                    if scene.tall_clear(c, y) {
                        l.y.push(y);
                        l.xz.push(pack(x, z));
                        low_flag.push(scene.near_low(c, y));
                        n += 1;
                    }
                }
                l.start.push(l.y.len() as u32);
            }
        }
        l.nbr = vec![0; l.y.len()];

        // Edges.
        let pos = |l: &Layers, i: usize| {
            let (x, z) = unpack(l.xz[i]);
            Vec3::new(min.x + (x as f32 + 0.5) * cell, l.y[i], min.y + (z as f32 + 0.5) * cell)
        };
        let ncol = |l: &Layers, i: usize, d: usize| {
            let (x, z) = unpack(l.xz[i]);
            (z as i32 + DIRS[d].1) as usize * w + (x as i32 + DIRS[d].0) as usize
        };
        let set = |l: &mut Layers, i: usize, d: usize, j: usize| {
            let (x, z) = unpack(l.xz[j]);
            let lj = j - l.start[z * w + x] as usize;
            l.nbr[i] |= (1 << d) | ((lj as u32) << (8 + 3 * d));
        };
        let step_to = |l: &Layers, i: usize, d: usize| -> Option<usize> {
            let nb = l.nbr[i];
            (nb & (1 << d) != 0).then(|| l.start[ncol(l, i, d)] as usize + ((nb >> (8 + 3 * d)) & 7) as usize)
        };
        // Orthogonal (each pair once, both ways).
        for i in 0..l.y.len() {
            let (x, z) = unpack(l.xz[i]);
            for d in [0usize, 2] {
                if x + (DIRS[d].0 as usize) >= w || z + (DIRS[d].1 as usize) >= h {
                    continue;
                }
                let nc = ncol(&l, i, d);
                let y = l.y[i];
                let Some(j) = (l.start[nc] as usize..l.start[nc + 1] as usize)
                    .filter(|&j| (l.y[j] - y).abs() <= 2.0 * cfg.step)
                    .min_by(|&a, &b| (l.y[a] - y).abs().total_cmp(&(l.y[b] - y).abs()))
                else {
                    continue;
                };
                let (pa, pb) = (pos(&l, i), pos(&l, j));
                if (pb.y - pa.y).abs() > cfg.step && !scene.mid_step(pa, pb) {
                    continue;
                }
                if (low_flag[i] || low_flag[j]) && !scene.low_clear(pa, pb) {
                    continue;
                }
                set(&mut l, i, d, j);
                set(&mut l, j, OPP[d], i);
            }
        }
        // Diagonals: only where both orthogonal ways round reach the same node.
        for i in 0..l.y.len() {
            for (d, dx_dir) in [(4usize, 0usize), (6, 1)] {
                let (Some(b), Some(dn)) = (step_to(&l, i, dx_dir), step_to(&l, i, 2)) else { continue };
                let (Some(c1), Some(c2)) = (step_to(&l, b, 2), step_to(&l, dn, dx_dir)) else { continue };
                if c1 != c2 {
                    continue;
                }
                let (pa, pc) = (pos(&l, i), pos(&l, c1));
                if (pc.y - pa.y).abs() > 2.0 * cfg.step {
                    continue;
                }
                if (low_flag[i] || low_flag[c1]) && !scene.low_clear(pa, pc) {
                    continue;
                }
                set(&mut l, i, d, c1);
                set(&mut l, c1, OPP[d], i);
            }
        }

        // Drop surfaces with no neighbour (bollard tops, chimney pots): nothing walks there.
        let mut map = vec![u32::MAX; l.y.len()];
        let mut m = Layers::default();
        let mut per_col = vec![0u32; w * h];
        for (i, slot) in map.iter_mut().enumerate() {
            if l.nbr[i] & 0xFF != 0 {
                *slot = m.y.len() as u32;
                m.y.push(l.y[i]);
                m.xz.push(l.xz[i]);
                m.nbr.push(l.nbr[i]);
                let (x, z) = unpack(l.xz[i]);
                per_col[z * w + x] += 1;
            }
        }
        m.start = Vec::with_capacity(w * h + 1);
        m.start.push(0);
        let mut at = 0;
        for n in per_col {
            at += n;
            m.start.push(at);
        }
        for i in 0..m.y.len() {
            let mut nb = m.nbr[i] & 0xFF;
            for d in 0..8 {
                if nb & (1 << d) != 0 {
                    // The old neighbour, renumbered: its layer in the new column.
                    let old = l.start[ncol(&m, i, d)] as usize + ((m.nbr[i] >> (8 + 3 * d)) & 7) as usize;
                    let lj = map[old] - m.start[ncol(&m, i, d)];
                    nb |= lj << (8 + 3 * d);
                }
            }
            m.nbr[i] = nb;
        }
        drop(l);

        // Components over the edges (flood fill in node order).
        let mut comp = vec![u32::MAX; m.y.len()];
        let mut stack = Vec::new();
        let mut count = 0u32;
        for s in 0..m.y.len() {
            if comp[s] != u32::MAX {
                continue;
            }
            comp[s] = count;
            stack.push(s);
            while let Some(i) = stack.pop() {
                for d in 0..8 {
                    if let Some(j) = step_to(&m, i, d) {
                        if comp[j] == u32::MAX {
                            comp[j] = count;
                            stack.push(j);
                        }
                    }
                }
            }
            count += 1;
        }
        m.comp = comp;

        let n = m.y.len();
        let mut grid = NavGrid25 {
            origin: min,
            cell,
            w,
            h,
            step: cfg.step,
            greed: GREED,
            layers: Arc::new(m),
            blocked: Arc::new(vec![0; n.div_ceil(64)]),
            links: Arc::new(Links { parent: (0..count).collect(), ..Default::default() }),
        };

        if cfg.ladders {
            let mut ladders: Vec<crate::statics::Ladder> =
                statics.chunks.values().flat_map(|c| c.ladders.iter().cloned()).collect();
            ladders.retain(|l| l.max.x >= min.x && l.min.x <= max.x && l.max.z >= min.y && l.min.z <= max.y);
            for (i, l) in ladders.iter().enumerate() {
                let (c, d) = (l.center(), l.facing.dir());
                let foot = Vec3::new(c.x, l.min.y, c.z) - d * (cfg.radius + 0.25);
                let Some(a) = grid.nearest_node(foot, 1.0, 0.8) else { continue };
                // Up the ladder, and on up any ladder that carries on from its top (a tower's
                // climb in stages with no landing between): to the first top with a floor.
                let mut k = i;
                for _ in 0..8 {
                    let t = &ladders[k];
                    let (c, d) = (t.center(), t.facing.dir());
                    let top = Vec3::new(c.x, t.max.y, c.z) + d * (cfg.radius + 0.35);
                    if let Some(b) = grid.nearest_node(top, 1.0, 0.8) {
                        let (pa, pb) = (grid.pos(a), grid.pos(b));
                        if (pb.y - pa.y).abs() > cfg.step {
                            let cost = (pb - pa).length() * cfg.ladder_cost.max(1.0);
                            grid.push_link(a, b, cost, LinkKind::Ladder);
                            grid.push_link(b, a, cost, LinkKind::Ladder);
                        }
                        break;
                    }
                    let next = ladders.iter().position(|m| {
                        (m.min.y - t.max.y).abs() < 0.3
                            && m.min.y > t.min.y
                            && Vec2::new(m.center().x - c.x, m.center().z - c.z).length() < 1.0
                    });
                    let Some(next) = next else { break };
                    k = next;
                }
            }
        }
        grid
    }
}

// ---------------------------------------------------------------------------------------------
// Queries.

impl NavGrid25 {
    /// Number of walkable surfaces (nodes).
    pub fn node_count(&self) -> usize {
        self.layers.y.len()
    }

    /// Bytes held by the grid (arrays, blocked bits, links).
    pub fn bytes(&self) -> usize {
        let l = &self.layers;
        (l.start.len() + l.y.len() + l.xz.len() + l.nbr.len() + l.comp.len()) * 4
            + self.blocked.len() * 8
            + self.links.list.len() * (std::mem::size_of::<Link25>() + 8)
            + self.links.parent.len() * 4
    }

    /// A hash of everything in the grid (determinism checks).
    pub fn hash(&self) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        let mut eat = |v: u64| h = (h ^ v).wrapping_mul(0x0100_0000_01b3);
        eat(self.w as u64);
        eat(self.h as u64);
        eat(self.origin.x.to_bits() as u64 | (self.origin.y.to_bits() as u64) << 32);
        eat(self.cell.to_bits() as u64);
        let l = &self.layers;
        l.start.iter().for_each(|v| eat(*v as u64));
        l.y.iter().for_each(|v| eat(v.to_bits() as u64));
        l.xz.iter().for_each(|v| eat(*v as u64));
        l.nbr.iter().for_each(|v| eat(*v as u64));
        l.comp.iter().for_each(|v| eat(*v as u64));
        self.blocked.iter().for_each(|v| eat(*v));
        for k in &self.links.list {
            eat(k.from as u64 | (k.to as u64) << 32);
            eat(k.cost.to_bits() as u64 | (k.kind as u64) << 32);
        }
        h
    }

    /// The explicit links (ladders and added ones), one entry per direction.
    pub fn links(&self) -> &[Link25] {
        &self.links.list
    }

    fn column(&self, p: Vec3) -> Option<(usize, usize)> {
        let x = ((p.x - self.origin.x) / self.cell).floor();
        let z = ((p.z - self.origin.y) / self.cell).floor();
        (x >= 0.0 && z >= 0.0 && x < self.w as f32 && z < self.h as f32).then_some((x as usize, z as usize))
    }

    fn nodes_of(&self, col: usize) -> std::ops::Range<usize> {
        self.layers.start[col] as usize..self.layers.start[col + 1] as usize
    }

    /// A node's position: its cell's centre on its surface.
    fn pos(&self, i: usize) -> Vec3 {
        let (x, z) = unpack(self.layers.xz[i]);
        Vec3::new(self.origin.x + (x as f32 + 0.5) * self.cell, self.layers.y[i], self.origin.y + (z as f32 + 0.5) * self.cell)
    }

    fn is_blocked(&self, i: usize) -> bool {
        self.blocked.get(i / 64).is_some_and(|w| w & (1 << (i % 64)) != 0)
    }

    /// The node toward `d` from `i`, if they are joined.
    fn neighbour(&self, i: usize, d: usize) -> Option<usize> {
        let nb = self.layers.nbr[i];
        if nb & (1 << d) == 0 {
            return None;
        }
        let (x, z) = unpack(self.layers.xz[i]);
        let ncol = (z as i32 + DIRS[d].1) as usize * self.w + (x as i32 + DIRS[d].0) as usize;
        Some(self.layers.start[ncol] as usize + ((nb >> (8 + 3 * d)) & 7) as usize)
    }

    /// Every open node one move from `i` (`reverse`: one move to `i`), with its position and the
    /// move's length or link cost.
    fn for_each_edge(&self, i: usize, reverse: bool, mut f: impl FnMut(usize, Vec3, f32)) {
        let l = &*self.layers;
        let nb = l.nbr[i];
        let (x, z) = unpack(l.xz[i]);
        let p = self.pos(i);
        for (d, &(dx, dz)) in DIRS.iter().enumerate() {
            if nb & (1 << d) == 0 {
                continue;
            }
            let ncol = (z as i32 + dz) as usize * self.w + (x as i32 + dx) as usize;
            let j = l.start[ncol] as usize + ((nb >> (8 + 3 * d)) & 7) as usize;
            if self.is_blocked(j) {
                continue;
            }
            let q = Vec3::new(p.x + dx as f32 * self.cell, l.y[j], p.z + dz as f32 * self.cell);
            f(j, q, (q - p).length());
        }
        if self.links.list.is_empty() {
            return;
        }
        let map = if reverse { &self.links.inn } else { &self.links.out };
        if let Some(v) = map.get(&(i as u32)) {
            for &li in v {
                let k = &self.links.list[li as usize];
                let j = if reverse { k.from } else { k.to } as usize;
                if !self.is_blocked(j) {
                    f(j, self.pos(j), k.cost);
                }
            }
        }
    }

    /// The surface of the column under `p` nearest its feet height, within 1 m.
    pub fn cell_at(&self, p: Vec3) -> Option<NavCell> {
        let (x, z) = self.column(p)?;
        let col = z * self.w + x;
        let base = self.layers.start[col] as usize;
        let node = self
            .nodes_of(col)
            .filter(|&i| (self.layers.y[i] - p.y).abs() <= 1.0)
            .min_by(|&a, &b| (self.layers.y[a] - p.y).abs().total_cmp(&(self.layers.y[b] - p.y).abs()))?;
        Some(NavCell { x, z, layer: node - base, node, pos: self.pos(node), blocked: self.is_blocked(node) })
    }

    /// Heights of every walkable surface in the column under `p`, lowest first.
    pub fn layers_at(&self, p: Vec3) -> Vec<f32> {
        self.column(p).map(|(x, z)| self.nodes_of(z * self.w + x).map(|i| self.layers.y[i]).collect()).unwrap_or_default()
    }

    /// Whether a walker with its feet at `p` stands on an open walkable cell.
    pub fn walkable(&self, p: Vec3) -> bool {
        self.cell_at(p).is_some_and(|c| !c.blocked)
    }

    /// The open node under `p` within `vtol`, or else the nearest within `radius` (horizontal).
    fn nearest_node(&self, p: Vec3, radius: f32, vtol: f32) -> Option<usize> {
        self.nearest_node_where(p, radius, vtol, |_| true)
    }

    fn nearest_node_where(&self, p: Vec3, radius: f32, vtol: f32, ok: impl Fn(usize) -> bool) -> Option<usize> {
        let usable = |i: usize, dy: f32| dy.abs() <= vtol && !self.is_blocked(i) && ok(i);
        if let Some((x, z)) = self.column(p) {
            let own = self
                .nodes_of(z * self.w + x)
                .filter(|&i| usable(i, self.layers.y[i] - p.y))
                .min_by(|&a, &b| (self.layers.y[a] - p.y).abs().total_cmp(&(self.layers.y[b] - p.y).abs()));
            if own.is_some() {
                return own;
            }
        }
        let r = (radius / self.cell).ceil() as i32 + 1;
        let cx = ((p.x - self.origin.x) / self.cell).floor() as i32;
        let cz = ((p.z - self.origin.y) / self.cell).floor() as i32;
        let mut best: Option<(f32, usize)> = None;
        for z in (cz - r).max(0)..=(cz + r).min(self.h as i32 - 1) {
            for x in (cx - r).max(0)..=(cx + r).min(self.w as i32 - 1) {
                for i in self.nodes_of(z as usize * self.w + x as usize) {
                    let q = self.pos(i);
                    let (dh, dy) = (Vec2::new(q.x - p.x, q.z - p.z).length(), q.y - p.y);
                    if dh > radius || !usable(i, dy) {
                        continue;
                    }
                    let d = dh * dh + dy * dy;
                    if best.is_none_or(|b| d < b.0) {
                        best = Some((d, i));
                    }
                }
            }
        }
        best.map(|b| b.1)
    }

    /// The nearest open walkable point (a cell's centre on its surface) within `radius` of `p`
    /// (horizontally; up to `radius + 1` m above or below).
    pub fn nearest_walkable(&self, p: Vec3, radius: f32) -> Option<Vec3> {
        self.nearest_node(p, radius, radius + 1.0).map(|i| self.pos(i))
    }

    /// Whether any way at all (edges and links, gates ignored) joins the surfaces nearest `a`
    /// and `b`: answered at once, without a search.
    pub fn connected(&self, a: Vec3, b: Vec3) -> bool {
        match (self.nearest_node(a, 2.0, 1.5), self.nearest_node(b, 2.0, 1.5)) {
            (Some(i), Some(j)) => self.links.find(self.layers.comp[i]) == self.links.find(self.layers.comp[j]),
            _ => false,
        }
    }

    /// Opens or closes the cell under `p` (its surface nearest the feet). False if there is none.
    pub fn set_blocked(&mut self, p: Vec3, blocked: bool) -> bool {
        let Some(c) = self.cell_at(p) else { return false };
        self.set_node_blocked(c.node, blocked);
        true
    }

    /// Opens or closes every node whose cell centre lies in the box (a gate's passage); returns
    /// how many.
    pub fn set_blocked_area(&mut self, min: Vec3, max: Vec3, blocked: bool) -> usize {
        let clamp = |v: f32, o: f32, n: usize| ((v - o) / self.cell).floor().clamp(0.0, n as f32 - 1.0) as usize;
        if self.w == 0 || max.x < self.origin.x || max.z < self.origin.y {
            return 0;
        }
        let (x0, x1) = (clamp(min.x, self.origin.x, self.w), clamp(max.x, self.origin.x, self.w));
        let (z0, z1) = (clamp(min.z, self.origin.y, self.h), clamp(max.z, self.origin.y, self.h));
        let mut n = 0;
        for z in z0..=z1 {
            for x in x0..=x1 {
                for i in self.nodes_of(z * self.w + x) {
                    let q = self.pos(i);
                    if q.cmpge(min).all() && q.cmple(max).all() {
                        self.set_node_blocked(i, blocked);
                        n += 1;
                    }
                }
            }
        }
        n
    }

    fn set_node_blocked(&mut self, i: usize, blocked: bool) {
        if self.is_blocked(i) == blocked {
            return;
        }
        let bits = Arc::make_mut(&mut self.blocked);
        if blocked {
            bits[i / 64] |= 1 << (i % 64);
        } else {
            bits[i / 64] &= !(1 << (i % 64));
        }
    }

    fn push_link(&mut self, from: usize, to: usize, cost: f32, kind: LinkKind) -> usize {
        let (ca, cb) = (self.layers.comp[from], self.layers.comp[to]);
        let links = Arc::make_mut(&mut self.links);
        let id = links.list.len() as u32;
        links.list.push(Link25 { from: from as u32, to: to as u32, cost, kind });
        links.out.entry(from as u32).or_default().push(id);
        links.inn.entry(to as u32).or_default().push(id);
        let (ra, rb) = (links.find(ca), links.find(cb));
        if ra != rb {
            links.parent[ra.max(rb) as usize] = ra.min(rb);
        }
        id as usize
    }

    /// Joins the walkable surfaces nearest `from` and `to` (within 1.5 m) both ways at `cost`
    /// (a doorway the grid can't see, a jump, a rope): None if either end has no surface.
    /// Keep `cost` at least the straight distance, or paths may not be the shortest.
    pub fn add_link(&mut self, from: Vec3, to: Vec3, cost: f32) -> Option<usize> {
        let id = self.add_one_way_link(from, to, cost)?;
        self.add_one_way_link(to, from, cost)?;
        Some(id)
    }

    /// Like [`NavGrid25::add_link`], one way only (a drop off a ledge).
    pub fn add_one_way_link(&mut self, from: Vec3, to: Vec3, cost: f32) -> Option<usize> {
        let a = self.nearest_node(from, 1.5, 1.5)?;
        let b = self.nearest_node(to, 1.5, 1.5)?;
        (a != b).then(|| self.push_link(a, b, cost.max(0.0), LinkKind::Custom))
    }

    fn heuristic(&self, a: Vec3, b: Vec3) -> f32 {
        let (dx, dz) = ((a.x - b.x).abs(), (a.z - b.z).abs());
        let oct = dx.max(dz) + (std::f32::consts::SQRT_2 - 1.0) * dx.min(dz);
        (oct * oct + (a.y - b.y) * (a.y - b.y)).sqrt()
    }

    /// A* from `from` to `to` over at most `max_nodes` expanded nodes: waypoints after the start
    /// (with heights), string-pulled so straight stretches are one leg, ending at `to` (or, when
    /// `to` is off the grid or on something the start can't reach, such as a crate's top, the
    /// nearest cell within 2 m that it can). None if there is no way (known at once when no edge
    /// or link joins the two) or the search ran out of nodes.
    pub fn path(&self, from: Vec3, to: Vec3, max_nodes: usize) -> Option<Vec<Vec3>> {
        self.search(from, to, max_nodes, None)
    }

    /// [`NavGrid25::path`] with an extra cost per cell: entering the cell at `p` costs its length
    /// times `1 + extra(p)` (at least 0.1×). Cells guards watch get a high `extra`, grass a
    /// negative one. Straightening never crosses cells costlier than the stretch of path it
    /// replaces.
    pub fn path_weighted(&self, from: Vec3, to: Vec3, max_nodes: usize, extra: &dyn Fn(Vec3) -> f32) -> Option<Vec<Vec3>> {
        self.search(from, to, max_nodes, Some(extra))
    }

    fn search(&self, from: Vec3, to: Vec3, max_nodes: usize, extra: Option<&dyn Fn(Vec3) -> f32>) -> Option<Vec<Vec3>> {
        let s = self.nearest_node(from, 2.0, 1.5)?;
        // The goal: the cell nearest `to` that any way joins to the start (none: no search).
        let cs = self.links.find(self.layers.comp[s]);
        let g = self.nearest_node_where(to, 2.0, 1.5, |i| self.links.find(self.layers.comp[i]) == cs)?;
        let goal = self.pos(g);
        let greed = self.greed.max(1.0);
        let mut seen = Visited::new();
        let mut open = BinaryHeap::new();
        seen.set(s as u32, s as u32, 0.0);
        open.push(Item { f: self.heuristic(self.pos(s), goal) * greed, g: 0.0, node: s as u32 });
        let mut expanded = 0usize;
        let mut found = false;
        while let Some(Item { g: gi, node: ni, .. }) = open.pop() {
            let Some((par, ci)) = seen.get(ni) else { continue };
            if par & CLOSED != 0 || gi > ci {
                continue;
            }
            if ni as usize == g {
                found = true;
                break;
            }
            seen.set(ni, par | CLOSED, ci);
            expanded += 1;
            if expanded > max_nodes {
                return None;
            }
            self.for_each_edge(ni as usize, false, |j, q, len| {
                let c = match extra {
                    Some(f) => len * (1.0 + f(q)).max(0.1),
                    None => len,
                };
                let ng = gi + c;
                let (k, new) = seen.entry(j as u32);
                let (pj, cj) = seen.vals[k];
                if new || (pj & CLOSED == 0 && ng < cj) {
                    seen.vals[k] = (ni, ng);
                    open.push(Item { f: ng + self.heuristic(q, goal) * greed, g: ng, node: j as u32 });
                }
            });
        }
        if !found {
            return None;
        }
        let mut nodes = vec![g];
        let mut at = g as u32;
        while let Some((par, _)) = seen.get(at) {
            let par = par & !CLOSED;
            if par == at {
                break;
            }
            at = par;
            nodes.push(at as usize);
        }
        nodes.reverse();
        // The exact goal when it is on the goal's cell.
        let end = match self.cell_at(to) {
            Some(c) if c.node == g => Vec3::new(to.x, goal.y, to.z),
            _ => goal,
        };
        // String-pulling: from each anchor, the farthest node a straight walk reaches.
        let extras: Option<Vec<f32>> = extra.map(|f| nodes.iter().map(|&i| f(self.pos(i))).collect());
        let mut out = Vec::new();
        let mut k = 0;
        while k + 1 < nodes.len() {
            let mut far = k + 1;
            let mut cap = extras.as_ref().map_or(0.0, |e| e[k].max(e[k + 1]));
            while far + 1 < nodes.len() && far + 1 - k <= MAX_SKIP {
                let cap2 = extras.as_ref().map_or(0.0, |e| cap.max(e[far + 1]));
                if !self.straight(nodes[k], nodes[far + 1], extra.map(|f| (f, cap2))) {
                    break;
                }
                cap = cap2;
                far += 1;
            }
            out.push(self.pos(nodes[far]));
            k = far;
        }
        match out.last_mut() {
            Some(last) => *last = end,
            None => out.push(end),
        }
        Some(out)
    }

    /// Whether a straight walk from node `a` to node `b` follows the grid's edges cell by cell
    /// (no blocked cell; with weights, none costlier than `cap`).
    fn straight(&self, a: usize, b: usize, weights: Option<(&dyn Fn(Vec3) -> f32, f32)>) -> bool {
        let (pa, pb) = (self.pos(a), self.pos(b));
        let (xa, za) = unpack(self.layers.xz[a]);
        let (xb, zb) = unpack(self.layers.xz[b]);
        let (mut x, mut z) = (xa as i32, za as i32);
        let (xb, zb) = (xb as i32, zb as i32);
        let (dx, dz) = (pb.x - pa.x, pb.z - pa.z);
        let (sx, sz) = (dx.signum() as i32, dz.signum() as i32);
        // Grid traversal (Amanatides–Woo) from cell centre to cell centre.
        let t_dx = if dx != 0.0 { self.cell / dx.abs() } else { f32::INFINITY };
        let t_dz = if dz != 0.0 { self.cell / dz.abs() } else { f32::INFINITY };
        let (mut tx, mut tz) = (t_dx * 0.5, t_dz * 0.5);
        let mut cur = a;
        let mut guard = (xb - x).abs() + (zb - z).abs() + 2;
        while (x, z) != (xb, zb) {
            guard -= 1;
            if guard < 0 {
                return false;
            }
            let (mx, mz) = if (tx - tz).abs() < 1e-6 {
                tx += t_dx;
                tz += t_dz;
                (sx, sz)
            } else if tx < tz {
                tx += t_dx;
                (sx, 0)
            } else {
                tz += t_dz;
                (0, sz)
            };
            let Some(d) = DIRS.iter().position(|&v| v == (mx, mz)) else { return false };
            let Some(next) = self.neighbour(cur, d) else { return false };
            if self.is_blocked(next) {
                return false;
            }
            if let Some((f, cap)) = weights {
                if f(self.pos(next)) > cap + 1e-6 {
                    return false;
                }
            }
            cur = next;
            x += mx;
            z += mz;
        }
        cur == b
    }

    /// Every node's path distance to `goal` (reverse Dijkstra) out to `reach` metres: a whole
    /// pack walks downhill on it.
    pub fn flow_field(&self, goal: Vec3, reach: f32) -> FlowField {
        let n = self.layers.y.len();
        let mut dist = vec![f32::INFINITY; n];
        let g = self.nearest_node(goal, 2.0, 1.5);
        if let Some(g) = g {
            let mut open = BinaryHeap::new();
            dist[g] = 0.0;
            open.push(Item { f: 0.0, g: 0.0, node: g as u32 });
            while let Some(Item { g: d, node, .. }) = open.pop() {
                let i = node as usize;
                if d > dist[i] {
                    continue;
                }
                self.for_each_edge(i, true, |j, _, len| {
                    let nd = d + len;
                    if nd < dist[j] && nd <= reach {
                        dist[j] = nd;
                        open.push(Item { f: nd, g: nd, node: j as u32 });
                    }
                });
            }
        }
        let goal_pos = g.map_or(goal, |g| {
            let q = self.pos(g);
            match self.cell_at(goal) {
                Some(c) if c.node == g => Vec3::new(goal.x, q.y, goal.z),
                _ => q,
            }
        });
        FlowField { grid: self.clone(), goal: g, goal_pos, dist }
    }
}

/// Every node's path distance to one goal (within a reach): walk downhill to get there.
#[derive(Clone, Debug)]
pub struct FlowField {
    grid: NavGrid25,
    /// The goal's node, if the goal is on the grid.
    pub goal: Option<usize>,
    /// Where the field leads (the goal, on its surface).
    pub goal_pos: Vec3,
    dist: Vec<f32>,
}

impl FlowField {
    /// Which way to walk from `p` (x, z; None when `p` is out of reach or at the goal): toward
    /// the lowest node a few moves downhill, so the way bends smoothly round corners.
    pub fn dir(&self, p: Vec3) -> Option<Vec2> {
        let goal = self.goal?;
        let g = &self.grid;
        let mut at = g.nearest_node_where(p, 1.0, 1.5, |i| self.dist[i].is_finite())?;
        for _ in 0..3 {
            if at == goal {
                break;
            }
            let mut best = (self.dist[at], at);
            g.for_each_edge(at, false, |j, _, _| {
                if self.dist[j] < best.0 {
                    best = (self.dist[j], j);
                }
            });
            if best.1 == at {
                break;
            }
            at = best.1;
        }
        let t = if at == goal { self.goal_pos } else { g.pos(at) };
        let d = Vec2::new(t.x - p.x, t.z - p.z);
        (d.length() > 0.05).then(|| d.normalize())
    }

    /// Path length (m) from `p` to the goal, if within reach.
    pub fn distance(&self, p: Vec3) -> Option<f32> {
        let i = self.grid.nearest_node_where(p, 1.0, 1.5, |i| self.dist[i].is_finite())?;
        Some(self.dist[i])
    }

    /// How many nodes the field reaches.
    pub fn reached(&self) -> usize {
        self.dist.iter().filter(|d| d.is_finite()).count()
    }
}

/// A search's per-node state (parent, with [`CLOSED`] in its top bit, and cost so far) in a small
/// open-addressing table holding only the nodes the search reaches, so a short search costs
/// little however big the grid is. Only looked up, never iterated.
struct Visited {
    /// Node + 1 (0 = empty slot).
    keys: Vec<u32>,
    vals: Vec<(u32, f32)>,
    len: usize,
    bits: u32,
}

/// Set in a visited node's parent once it has been expanded.
const CLOSED: u32 = 1 << 31;

impl Visited {
    fn new() -> Self {
        Self::with_bits(12)
    }

    fn with_bits(bits: u32) -> Self {
        Self { keys: vec![0; 1 << bits], vals: vec![(0, 0.0); 1 << bits], len: 0, bits }
    }

    fn slot(&self, k: u32) -> usize {
        let mask = self.keys.len() - 1;
        let mut i = (k.wrapping_mul(0x9E37_79B9) >> (32 - self.bits)) as usize;
        while self.keys[i] != 0 && self.keys[i] != k + 1 {
            i = (i + 1) & mask;
        }
        i
    }

    fn get(&self, k: u32) -> Option<(u32, f32)> {
        let i = self.slot(k);
        (self.keys[i] != 0).then(|| self.vals[i])
    }

    /// The slot for `k`, made if new (then true).
    fn entry(&mut self, k: u32) -> (usize, bool) {
        if (self.len + 1) * 2 > self.keys.len() {
            let old = std::mem::replace(self, Self::with_bits(self.bits + 1));
            for (key, v) in old.keys.into_iter().zip(old.vals) {
                if key != 0 {
                    let (i, _) = self.entry(key - 1);
                    self.vals[i] = v;
                }
            }
        }
        let i = self.slot(k);
        let new = self.keys[i] == 0;
        if new {
            self.keys[i] = k + 1;
            self.len += 1;
        }
        (i, new)
    }

    fn set(&mut self, k: u32, parent: u32, cost: f32) {
        let (i, _) = self.entry(k);
        self.vals[i] = (parent, cost);
    }
}

/// An open-list entry: least `f` first, then the deepest (largest `g`), then the lowest node, so
/// the order never depends on anything else.
#[derive(PartialEq)]
struct Item {
    f: f32,
    g: f32,
    node: u32,
}

impl Eq for Item {}

impl Ord for Item {
    fn cmp(&self, o: &Self) -> std::cmp::Ordering {
        o.f.total_cmp(&self.f).then(self.g.total_cmp(&o.g)).then(o.node.cmp(&self.node))
    }
}

impl PartialOrd for Item {
    fn partial_cmp(&self, o: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(o))
    }
}
