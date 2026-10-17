//! Islands: heightmap terrain generated from data (`[[island]]` in a level), deterministic.
//!
//! `generate(&IslandDef) -> Heightfield` samples a 1 m grid over the island's outline plus its
//! shelf: the signed distance to the outline gives a coastal profile (seabed −12 m → shallow
//! shelf → beach → inland rise), then peaks, inland noise, sea cliffs, flattened plateaus (town,
//! fort) and carved roads. The heightfield answers cheap queries (`height`, `normal`, `slope`,
//! `climbable`, `ground`), becomes 32 m terrain chunks that render as `TerrainPatch`es and
//! collide as rapier height fields (`terrain_patches`), and gives the sea its depth (`seabed`).
//!
//! `build_level` assembles a whole level: islands, `structures`, `vegetation`, a sea, and
//! returns what game code needs (markers, gameplay areas, gates, heightfields).
//!
//! ```toml
//! [[island]]
//! name = "templanza"
//! seed = 7
//! outline = [[-100, -60], [90, -70], [110, 40], [-80, 70]]   # x, z (m), either winding
//! beach = 10            # beach width inland from the waterline (m)
//! shelf = 24            # shallow ring outside the coast, under 2 m deep (m)
//! rise = 6              # inland rise above the beach before peaks (m)
//! peaks = [{ at = [20, -10], height = 30, radius = 60 }]
//! cliffs = [{ from = [60, -68], to = [108, 10], height = 14 }]   # sea cliffs along the coast
//! noise = { scale = 40, amp = 3, seed = 2 }
//! flatten = [{ polygon = [[-40, 10], [0, 10], [0, 40], [-40, 40]], height = 3, blend = 8, ground = "dirt" }]
//! roads = [{ points = [[-20, 25], [30, 0], [70, -40]], width = 4 }]
//! grass = [[[10, -20], [25, -20], [25, -8], [10, -8]]]      # stalk grass (hiding) polygons
//! ```

use std::collections::BTreeMap;
use std::sync::Arc;

use glam::{Vec2, Vec3};
use serde::{Deserialize, Serialize};

use crate::color::Color;
use crate::ocean::Seabed;
use crate::rng::hash_f32;
use crate::sim::Sim;
use crate::statics::{CHUNK_SIZE, RegionKey};
use crate::structures::{Area, AreaKind, StructureDef, StructureOut};
use crate::terrain::{HeightGrid, TerrainPatch};
use crate::vegetation::{VegetationDef, VegetationOut};
use crate::zones::{Zone, ZoneKind};

/// Height of the open-sea floor (m).
pub const DEEP: f32 = -12.0;
/// Depth of the shelf's outer edge (m): everything on the shelf is shallower.
pub const SHELF_DEPTH: f32 = -1.8;

fn d_beach() -> f32 {
    10.0
}
fn d_shelf() -> f32 {
    24.0
}
fn d_rise() -> f32 {
    6.0
}
fn d_blend() -> f32 {
    8.0
}
fn d_width() -> f32 {
    4.0
}
fn d_cell() -> f32 {
    1.0
}

/// One island, as written in a level (`[[island]]`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IslandDef {
    #[serde(default)]
    pub name: String,
    /// The coastline polygon (x, z in metres), either winding.
    pub outline: Vec<[f32; 2]>,
    #[serde(default)]
    pub peaks: Vec<Peak>,
    /// Beach width inland from the waterline (m).
    #[serde(default = "d_beach")]
    pub beach: f32,
    /// Width of the shallow underwater ring outside the coast (m); deeper than 2 m beyond.
    #[serde(default = "d_shelf")]
    pub shelf: f32,
    /// Inland rise above the beach before peaks and noise (m).
    #[serde(default = "d_rise")]
    pub rise: f32,
    /// Sea cliffs along the coast.
    #[serde(default)]
    pub cliffs: Vec<Cliff>,
    #[serde(default)]
    pub noise: NoiseDef,
    /// Plateaus (town, fort): flat at `height` inside, blended out over `blend`.
    #[serde(default)]
    pub flatten: Vec<Flatten>,
    #[serde(default)]
    pub roads: Vec<Road>,
    /// Stalk-grass polygons: gameplay concealment, drawn as dense 1.0–1.3 m tufts.
    #[serde(default)]
    pub grass: Vec<Vec<[f32; 2]>>,
    #[serde(default)]
    pub seed: u64,
    /// Grid spacing (m); 1 m keeps the collider, mesh and queries identical.
    #[serde(default = "d_cell")]
    pub cell: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Peak {
    pub at: [f32; 2],
    pub height: f32,
    pub radius: f32,
}

/// A sea cliff along the coast between two points near the waterline.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cliff {
    pub from: [f32; 2],
    pub to: [f32; 2],
    /// Height of the cliff top above the sea (m).
    pub height: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct NoiseDef {
    /// Feature size (m).
    pub scale: f32,
    /// Height range (m).
    pub amp: f32,
    pub seed: u64,
}

impl Default for NoiseDef {
    fn default() -> Self {
        Self { scale: 40.0, amp: 2.5, seed: 0 }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Flatten {
    pub polygon: Vec<[f32; 2]>,
    pub height: f32,
    /// Width of the blend back to the natural ground outside the polygon (m).
    #[serde(default = "d_blend")]
    pub blend: f32,
    /// The ground drawn on the plateau (default: whatever the slope and height give).
    #[serde(default)]
    pub ground: Option<Ground>,
    /// Its colour (sRGB hex) instead of the ground's palette colour.
    #[serde(default)]
    pub color: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Road {
    pub points: Vec<[f32; 2]>,
    #[serde(default = "d_width")]
    pub width: f32,
}

/// What the ground is made of (drawing, footsteps, vegetation rules).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum Ground {
    #[default]
    Sand,
    WetSand,
    Grass,
    Jungle,
    Rock,
    Dirt,
    Cliff,
}

impl Ground {
    pub const ALL: [Ground; 7] =
        [Ground::Sand, Ground::WetSand, Ground::Grass, Ground::Jungle, Ground::Rock, Ground::Dirt, Ground::Cliff];

    pub fn name(self) -> &'static str {
        match self {
            Ground::Sand => "sand",
            Ground::WetSand => "wet_sand",
            Ground::Grass => "grass",
            Ground::Jungle => "jungle",
            Ground::Rock => "rock",
            Ground::Dirt => "dirt",
            Ground::Cliff => "cliff",
        }
    }

    pub fn from_name(n: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|g| g.name() == n)
    }

    /// The palette colour (sRGB hex, §9.17).
    pub fn hex(self) -> &'static str {
        match self {
            Ground::Sand => "#e8d3a2",
            Ground::WetSand => "#b89e72",
            Ground::Grass => "#6aa84f",
            Ground::Jungle => "#2f6d3a",
            Ground::Rock => "#8c8577",
            Ground::Dirt => "#9a7a52",
            Ground::Cliff => "#7d766a",
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Noise and 2D geometry helpers (shared with `vegetation` and `structures`).

fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Value noise in [0, 1].
pub(crate) fn noise(seed: u64, x: f32, z: f32) -> f32 {
    let (x0, z0) = (x.floor(), z.floor());
    let (fx, fz) = (smooth(x - x0), smooth(z - z0));
    let (ix, iz) = (x0 as i32, z0 as i32);
    let h = |dx: i32, dz: i32| hash_f32(seed, ix + dx, iz + dz, 0x15);
    let a = h(0, 0) + (h(1, 0) - h(0, 0)) * fx;
    let b = h(0, 1) + (h(1, 1) - h(0, 1)) * fx;
    a + (b - a) * fz
}

/// Fractal value noise in [0, 1].
pub(crate) fn fbm(seed: u64, x: f32, z: f32, octaves: u32) -> f32 {
    let (mut sum, mut amp, mut freq, mut norm) = (0.0, 1.0, 1.0, 0.0);
    for o in 0..octaves {
        sum += noise(seed.wrapping_add(o as u64 * 131), x * freq + o as f32 * 17.3, z * freq - o as f32 * 9.1) * amp;
        norm += amp;
        amp *= 0.5;
        freq *= 2.03;
    }
    sum / norm
}

pub(crate) fn v2(p: [f32; 2]) -> Vec2 {
    Vec2::new(p[0], p[1])
}

/// Distance from `p` to the segment `a`–`b`, and the segment parameter (unclamped).
pub(crate) fn seg_dist(p: Vec2, a: Vec2, b: Vec2) -> (f32, f32) {
    let ab = b - a;
    let t = (p - a).dot(ab) / ab.length_squared().max(1e-9);
    (p.distance(a + ab * t.clamp(0.0, 1.0)), t)
}

pub(crate) fn point_in_polygon(p: Vec2, poly: &[Vec2]) -> bool {
    let mut inside = false;
    let n = poly.len();
    if n < 3 {
        return false;
    }
    let mut j = n - 1;
    for i in 0..n {
        let (a, b) = (poly[i], poly[j]);
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// Signed distance to a polygon's boundary: positive inside.
pub(crate) fn signed_distance(p: Vec2, poly: &[Vec2]) -> f32 {
    let n = poly.len();
    if n < 2 {
        return f32::NEG_INFINITY;
    }
    let mut d = f32::INFINITY;
    for i in 0..n {
        d = d.min(seg_dist(p, poly[i], poly[(i + 1) % n]).0);
    }
    if point_in_polygon(p, poly) { d } else { -d }
}

/// Distance from `p` to a polyline, the index of the nearest segment and the parameter on it.
pub(crate) fn polyline_dist(p: Vec2, line: &[Vec2]) -> (f32, usize, f32) {
    let mut best = (f32::INFINITY, 0, 0.0);
    for i in 0..line.len().saturating_sub(1) {
        let (d, t) = seg_dist(p, line[i], line[i + 1]);
        if d < best.0 {
            best = (d, i, t.clamp(0.0, 1.0));
        }
    }
    if line.len() == 1 {
        best = (p.distance(line[0]), 0, 0.0);
    }
    best
}

fn poly(points: &[[f32; 2]]) -> Vec<Vec2> {
    points.iter().copied().map(v2).collect()
}

// ---------------------------------------------------------------------------------------------
// The heightfield.

/// A cell flag: a climbable cliff face (slope over 55° and more than 1.5 m of relief).
pub const CELL_CLIMB: u8 = 1;
/// A cell flag: on a road.
pub const CELL_ROAD: u8 = 2;
/// A cell flag: inside a flattened plateau.
pub const CELL_FLAT: u8 = 4;

/// A generated island: heights and ground on a regular grid, plus the def it came from.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Heightfield {
    pub name: String,
    /// World x, z of sample (0, 0); samples sit at whole multiples of `cell`.
    pub origin: Vec2,
    pub cell: f32,
    /// Samples along x (`w`) and z (`h`).
    pub w: usize,
    pub h: usize,
    /// Ground height per sample, row-major (z rows).
    pub heights: Vec<f32>,
    /// Ground per sample (the dominant one).
    pub ground: Vec<Ground>,
    /// Drawn colour per sample: blended grounds, packed as sqrt-encoded RGB8.
    pub colors: Vec<u32>,
    /// `CELL_*` flags per cell ((w-1) × (h-1), row-major).
    pub flags: Vec<u8>,
    /// The definition (roads, plateaus and grass for queries and vegetation).
    pub def: IslandDef,
}

impl Heightfield {
    /// World x, z of the grid's far corner.
    pub fn max(&self) -> Vec2 {
        self.origin + Vec2::new((self.w - 1) as f32, (self.h - 1) as f32) * self.cell
    }

    pub fn contains(&self, p: Vec2) -> bool {
        let m = self.max();
        p.x >= self.origin.x && p.y >= self.origin.y && p.x <= m.x && p.y <= m.y
    }

    /// Sample height at grid indices (clamped).
    pub fn at(&self, i: i32, k: i32) -> f32 {
        let i = i.clamp(0, self.w as i32 - 1) as usize;
        let k = k.clamp(0, self.h as i32 - 1) as usize;
        self.heights[k * self.w + i]
    }

    fn locate(&self, p: Vec2) -> Option<(usize, usize, f32, f32)> {
        let g = (p - self.origin) / self.cell;
        if !(g.x >= 0.0 && g.y >= 0.0 && g.x <= (self.w - 1) as f32 && g.y <= (self.h - 1) as f32) {
            return None;
        }
        let i = (g.x.floor() as usize).min(self.w - 2);
        let k = (g.y.floor() as usize).min(self.h - 2);
        Some((i, k, g.x - i as f32, g.y - k as f32))
    }

    /// Ground height at `p` (x, z): the collider's surface exactly (same triangles), `DEEP`
    /// outside the grid. A few multiplies: cheap enough for every hull sample every tick.
    pub fn height(&self, p: Vec2) -> f32 {
        let Some((i, k, fx, fz)) = self.locate(p) else { return DEEP };
        let w = self.w;
        let a = self.heights[k * w + i];
        let b = self.heights[k * w + i + 1];
        let c = self.heights[(k + 1) * w + i];
        let d = self.heights[(k + 1) * w + i + 1];
        // Split along (x0, z1)–(x1, z0), as rapier's height field does.
        if fx + fz <= 1.0 { a + (b - a) * fx + (c - a) * fz } else { d + (c - d) * (1.0 - fx) + (b - d) * (1.0 - fz) }
    }

    /// Upward surface normal of the triangle under `p`.
    pub fn normal(&self, p: Vec2) -> Vec3 {
        let Some((i, k, fx, fz)) = self.locate(p) else { return Vec3::Y };
        let w = self.w;
        let s = self.cell;
        let a = self.heights[k * w + i];
        let b = self.heights[k * w + i + 1];
        let c = self.heights[(k + 1) * w + i];
        let d = self.heights[(k + 1) * w + i + 1];
        let (dx, dz) = if fx + fz <= 1.0 { (b - a, c - a) } else { (d - c, d - b) };
        Vec3::new(-dx / s, 1.0, -dz / s).normalize()
    }

    /// Slope at `p` in degrees (0 = flat).
    pub fn slope(&self, p: Vec2) -> f32 {
        self.normal(p).y.clamp(-1.0, 1.0).acos().to_degrees()
    }

    fn cell_flags(&self, p: Vec2) -> u8 {
        self.locate(p).map_or(0, |(i, k, _, _)| self.flags[k * (self.w - 1) + i])
    }

    /// A climbable cliff face: slope over 55° and more than 1.5 m above the local base.
    pub fn climbable(&self, p: Vec2) -> bool {
        self.cell_flags(p) & CELL_CLIMB != 0
    }

    /// On a carved road.
    pub fn on_road(&self, p: Vec2) -> bool {
        self.cell_flags(p) & CELL_ROAD != 0
    }

    /// The ground at `p` (nearest sample); `Sand` (the sea floor) outside.
    pub fn ground(&self, p: Vec2) -> Ground {
        let g = ((p - self.origin) / self.cell).round();
        if g.x < 0.0 || g.y < 0.0 || g.x >= self.w as f32 || g.y >= self.h as f32 {
            return Ground::Sand;
        }
        self.ground[g.y as usize * self.w + g.x as usize]
    }

    /// The surface material at `p` (same as `ground`: sand, wet sand, grass, jungle, rock,
    /// dirt/road, cliff).
    pub fn material(&self, p: Vec2) -> Ground {
        self.ground(p)
    }

    /// The drawn ground colour at `p` (nearest sample, linear RGB).
    pub fn color(&self, p: Vec2) -> Color {
        let g = ((p - self.origin) / self.cell).round();
        if g.x < 0.0 || g.y < 0.0 || g.x >= self.w as f32 || g.y >= self.h as f32 {
            return Color::hex(Ground::Sand.hex());
        }
        unpack(self.colors[g.y as usize * self.w + g.x as usize])
    }

    /// Inside a stalk-grass polygon (concealment).
    pub fn in_grass(&self, p: Vec2) -> bool {
        self.def.grass.iter().any(|g| point_in_polygon(p, &poly(g)))
    }

    /// A stable hash of the generated data (determinism tests).
    pub fn hash(&self) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        let mut mix = |x: u64| h = (h ^ x).wrapping_mul(0x0100_0000_01b3);
        mix(self.w as u64);
        mix(self.h as u64);
        mix(self.origin.x.to_bits() as u64);
        mix(self.origin.y.to_bits() as u64);
        for v in &self.heights {
            mix(v.to_bits() as u64);
        }
        for g in &self.ground {
            mix(*g as u64);
        }
        for f in &self.flags {
            mix(*f as u64);
        }
        for c in &self.colors {
            mix(*c as u64);
        }
        h
    }

    /// Bytes held (heights, ground, flags).
    pub fn bytes(&self) -> usize {
        self.heights.len() * 4 + self.ground.len() + self.colors.len() * 4 + self.flags.len()
    }
}

/// Linear colour → sqrt-encoded RGB8 (keeps precision in the darks).
fn pack(c: Color) -> u32 {
    let q = |v: f32| (v.clamp(0.0, 1.0).sqrt() * 255.0).round() as u32;
    q(c.0[0]) << 16 | q(c.0[1]) << 8 | q(c.0[2])
}

fn unpack(v: u32) -> Color {
    let f = |x: u32| {
        let s = (x & 0xff) as f32 / 255.0;
        s * s
    };
    Color([f(v >> 16), f(v >> 8), f(v)])
}

/// Ground height over several islands: the highest; `DEEP` away from all of them.
pub fn height_at(fields: &[&Heightfield], p: Vec2) -> f32 {
    fields.iter().filter(|f| f.contains(p)).map(|f| f.height(p)).fold(DEEP, f32::max)
}

/// Closed contour lines where an island's ground crosses `level` (marching squares on every
/// `stride`-th sample, simplified to within `tolerance` m): coastlines at 0, hills above. The
/// grid's border counts as below the level, so every line closes (first point = last).
pub fn contours(field: &Heightfield, level: f32, stride: usize, tolerance: f32) -> Vec<Vec<Vec2>> {
    let s = stride.max(1);
    let nx = ((field.w - 1) / s + 1) as i32;
    let nz = ((field.h - 1) / s + 1) as i32;
    let value = |i: i32, k: i32| -> f32 {
        if i < 0 || k < 0 || i >= nx || k >= nz { -1.0 } else { field.at(i * s as i32, k * s as i32) - level }
    };
    let pos = |i: i32, k: i32| field.origin + Vec2::new(i as f32, k as f32) * (s as f32 * field.cell);
    // Edge keys: (i, k, 0) runs (i,k)→(i+1,k); (i, k, 1) runs (i,k)→(i,k+1).
    let crossing = |e: (i32, i32, u8)| -> Vec2 {
        let (i, k, dir) = e;
        let (j, l) = if dir == 0 { (i + 1, k) } else { (i, k + 1) };
        let (a, b) = (value(i, k), value(j, l));
        let t = if (a - b).abs() > 1e-6 { (a / (a - b)).clamp(0.0, 1.0) } else { 0.5 };
        pos(i, k).lerp(pos(j, l), t)
    };
    let mut links: BTreeMap<(i32, i32, u8), Vec<(i32, i32, u8)>> = BTreeMap::new();
    let mut link = |a: (i32, i32, u8), b: (i32, i32, u8)| {
        links.entry(a).or_default().push(b);
        links.entry(b).or_default().push(a);
    };
    for k in -1..nz {
        for i in -1..nx {
            let (v0, v1, v2, v3) = (value(i, k), value(i + 1, k), value(i + 1, k + 1), value(i, k + 1));
            let case = (v0 > 0.0) as u8 | ((v1 > 0.0) as u8) << 1 | ((v2 > 0.0) as u8) << 2 | ((v3 > 0.0) as u8) << 3;
            let (bottom, right, top, left) = ((i, k, 0), (i + 1, k, 1), (i, k + 1, 0), (i, k, 1));
            let high_centre = v0 + v1 + v2 + v3 > 0.0;
            match case {
                0 | 15 => {}
                1 | 14 => link(left, bottom),
                2 | 13 => link(bottom, right),
                3 | 12 => link(left, right),
                4 | 11 => link(right, top),
                6 | 9 => link(bottom, top),
                7 | 8 => link(left, top),
                // Saddles: the centre's side decides which corners join.
                5 if high_centre => {
                    link(left, top);
                    link(bottom, right);
                }
                10 if !high_centre => {
                    link(left, top);
                    link(bottom, right);
                }
                _ => {
                    link(left, bottom);
                    link(right, top);
                }
            }
        }
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut out = Vec::new();
    for &start in links.keys() {
        if !seen.insert(start) {
            continue;
        }
        let mut ring = vec![crossing(start)];
        let mut cur = start;
        while let Some(&next) = links[&cur].iter().find(|e| !seen.contains(*e)) {
            seen.insert(next);
            ring.push(crossing(next));
            cur = next;
        }
        if ring.len() < 4 {
            continue;
        }
        ring.push(ring[0]);
        out.push(simplify(&ring, tolerance));
    }
    out
}

/// Douglas–Peucker on a polyline (keeps both ends).
fn simplify(pts: &[Vec2], tolerance: f32) -> Vec<Vec2> {
    if pts.len() < 3 || tolerance <= 0.0 {
        return pts.to_vec();
    }
    let mut keep = vec![false; pts.len()];
    keep[0] = true;
    keep[pts.len() - 1] = true;
    let mut stack = vec![(0, pts.len() - 1)];
    while let Some((a, b)) = stack.pop() {
        let (pa, pb) = (pts[a], pts[b]);
        let d = pb - pa;
        let len = d.length();
        let mut far = (0.0, a);
        for (i, p) in pts.iter().enumerate().take(b).skip(a + 1) {
            let dist = if len < 1e-6 { p.distance(pa) } else { (d.perp_dot(*p - pa) / len).abs() };
            if dist > far.0 {
                far = (dist, i);
            }
        }
        // A closed ring's two ends coincide: always split it once.
        if far.0 > tolerance || (len < 1e-6 && far.1 > a) {
            keep[far.1] = true;
            stack.push((a, far.1));
            stack.push((far.1, b));
        }
    }
    pts.iter().zip(keep).filter(|(_, k)| *k).map(|(p, _)| *p).collect()
}

/// The island whose ground is highest at `p`.
fn field_at<'a>(fields: &[&'a Heightfield], p: Vec2) -> Option<&'a Heightfield> {
    fields.iter().filter(|f| f.contains(p)).max_by(|a, b| a.height(p).total_cmp(&b.height(p))).copied()
}

/// Seabed depth over an area (for the ocean's shallows, grounding and swimming): the islands'
/// ground on a grid of `cell` over `min`..`max`, `DEEP` elsewhere.
pub fn seabed(fields: &[&Heightfield], min: Vec2, max: Vec2, cell: f32) -> Seabed {
    let cell = cell.max(0.1);
    let width = (((max.x - min.x) / cell).ceil() as usize).max(2);
    let depth = (((max.y - min.y) / cell).ceil() as usize).max(2);
    let mut heights = Vec::with_capacity(width * depth);
    for k in 0..depth {
        for i in 0..width {
            let p = min + Vec2::new((i as f32 + 0.5) * cell, (k as f32 + 0.5) * cell);
            heights.push(height_at(fields, p));
        }
    }
    Seabed { origin: min, cell, width, depth, heights, deep: DEEP }
}

// ---------------------------------------------------------------------------------------------
// Generation.

/// A road's centre line with its height every metre along it, smoothed.
struct RoadPath {
    pts: Vec<Vec2>,
    /// Distance along the road at each point.
    along: Vec<f32>,
    /// Height every metre along the road.
    heights: Vec<f32>,
    half: f32,
    lo: Vec2,
    hi: Vec2,
}

impl RoadPath {
    const SHOULDER: f32 = 3.0;
    /// Metres over which a road leaves a plateau's height.
    const RAMP: f32 = 24.0;

    /// `ground` gives the height under a point and whether it is pinned (inside a plateau,
    /// which stays flat): pinned samples keep their height while the rest is smoothed, so a
    /// road ramps out of a plateau from its edge instead of stepping up beside it.
    fn new(r: &Road, ground: &dyn Fn(Vec2) -> (f32, bool)) -> Self {
        let pts = poly(&r.points);
        let mut along = vec![0.0];
        for w in pts.windows(2) {
            along.push(along.last().copied().unwrap_or(0.0) + w[0].distance(w[1]));
        }
        let total = along.last().copied().unwrap_or(0.0);
        let n = total.ceil() as usize + 1;
        let point_at = |s: f32| -> Vec2 {
            let i = along.partition_point(|a| *a <= s).clamp(1, pts.len().max(2) - 1);
            if pts.len() < 2 {
                return pts.first().copied().unwrap_or(Vec2::ZERO);
            }
            let (a, b) = (along[i - 1], along[i]);
            pts[i - 1].lerp(pts[i], ((s - a) / (b - a).max(1e-6)).clamp(0.0, 1.0))
        };
        let raw: Vec<(f32, bool)> = (0..n).map(|k| ground(point_at(k as f32))).map(|(h, pin)| (h.max(0.4), pin)).collect();
        // Smooth along the road (box filter, three passes ≈ a gaussian of ±8 m).
        let mut heights: Vec<f32> = raw.iter().map(|r| r.0).collect();
        for _ in 0..3 {
            let prev = heights.clone();
            for (i, h) in heights.iter_mut().enumerate() {
                let (lo, hi) = (i.saturating_sub(8), (i + 8).min(prev.len() - 1));
                *h = prev[lo..=hi].iter().sum::<f32>() / (hi - lo + 1) as f32;
            }
        }
        // Pinned stretches keep their height; the road ramps out of them over `RAMP` metres.
        let pins: Vec<usize> = (0..n).filter(|&i| raw[i].1).collect();
        if !pins.is_empty() {
            for (i, h) in heights.iter_mut().enumerate() {
                let j = pins[pins.partition_point(|&j| j < i).min(pins.len() - 1)];
                let k = pins[pins.partition_point(|&j| j < i).saturating_sub(1)];
                let near = if i.abs_diff(j) < i.abs_diff(k) { j } else { k };
                let t = smooth(i.abs_diff(near) as f32 / Self::RAMP);
                *h = raw[near].0 + (*h - raw[near].0) * t;
            }
        }
        let m = Vec2::splat(r.width * 0.5 + Self::SHOULDER + 1.0);
        let lo = pts.iter().copied().fold(Vec2::splat(f32::MAX), Vec2::min) - m;
        let hi = pts.iter().copied().fold(Vec2::splat(f32::MIN), Vec2::max) + m;
        Self { pts, along, heights, half: r.width * 0.5, lo, hi }
    }

    /// (distance from the centre line, the road's height there); None far from the road.
    fn query(&self, p: Vec2) -> Option<(f32, f32)> {
        if p.x < self.lo.x || p.y < self.lo.y || p.x > self.hi.x || p.y > self.hi.y {
            return None;
        }
        let (d, i, t) = polyline_dist(p, &self.pts);
        let seg = self.along.get(i + 1).copied().unwrap_or(0.0) - self.along.get(i).copied().unwrap_or(0.0);
        let s = self.along.get(i).copied().unwrap_or(0.0) + seg * t;
        let k = (s.floor() as usize).min(self.heights.len().saturating_sub(1));
        let f = s - k as f32;
        let h0 = self.heights.get(k).copied().unwrap_or(0.0);
        let h1 = self.heights.get(k + 1).copied().unwrap_or(h0);
        Some((d, h0 + (h1 - h0) * f))
    }
}

struct Gen<'a> {
    def: &'a IslandDef,
    outline: Vec<Vec2>,
    flats: Vec<(Vec<Vec2>, &'a Flatten)>,
    seed: u64,
}

impl Gen<'_> {
    /// Signed distance to the coast (positive inland), with a little wobble.
    fn coast(&self, p: Vec2) -> f32 {
        let d = signed_distance(p, &self.outline);
        d + (fbm(self.seed ^ 0xc0a5, p.x / 22.0, p.y / 22.0, 3) - 0.5) * 6.0
    }

    /// Natural height before plateaus and roads (`d`: the coast distance at `p`).
    fn natural(&self, p: Vec2, d: f32) -> f32 {
        let def = self.def;
        let mut h = if d >= 0.0 {
            let beach = def.beach.max(1.0);
            1.6 * smooth(d / beach) + def.rise * (1.0 - (-(d - beach).max(0.0) / 28.0).exp())
        } else {
            let s = -d;
            let shelf = def.shelf.max(1.0);
            if s <= shelf {
                SHELF_DEPTH * smooth(s / shelf)
            } else {
                SHELF_DEPTH + (DEEP - SHELF_DEPTH) * smooth((s - shelf) / 16.0)
            }
        };
        // Inland: peaks and rolling noise; the beach stays smooth.
        let inland = smooth((d - def.beach * 0.5) / (def.beach * 1.5).max(1.0));
        if inland > 0.0 {
            let peak = def
                .peaks
                .iter()
                .map(|pk| {
                    let x = p.distance(v2(pk.at)) / pk.radius.max(1.0);
                    if x < 1.0 { pk.height * 0.5 * (1.0 + (x * std::f32::consts::PI).cos()) } else { 0.0 }
                })
                .fold(0.0f32, f32::max);
            let n = &def.noise;
            let scale = n.scale.max(1.0);
            let rolling = (fbm(self.seed ^ n.seed.wrapping_mul(0x9e37) ^ 0x51, p.x / scale, p.y / scale, 4) - 0.5) * 2.0 * n.amp;
            h += inland * (peak + rolling);
        } else if d < 0.0 {
            // Ripples on the seabed.
            h += (fbm(self.seed ^ 0x5eab, p.x / 9.0, p.y / 9.0, 2) - 0.5) * 0.4 * smooth(-d / 6.0);
        }
        // Sea cliffs: a steep face at the waterline, a plateau on top, deep water below.
        for c in &def.cliffs {
            let (a, b) = (v2(c.from), v2(c.to));
            let len = a.distance(b).max(1.0);
            let (dseg, t) = seg_dist(p, a, b);
            let beyond = (-t * len).max((t - 1.0) * len).max(0.0);
            let weight = (1.0 - smooth(beyond / 12.0)) * (1.0 - smooth((dseg - 24.0) / 20.0));
            if weight <= 0.0 {
                continue;
            }
            let cliffed = if d >= 0.0 { h.max(c.height * smooth(d / 3.0)) } else { h.min(-6.0 * smooth(-d / 5.0)) };
            h += (cliffed - h) * weight;
        }
        h
    }
}

/// Generates an island (deterministic: the same def gives the same heightfield).
pub fn generate(def: &IslandDef) -> Heightfield {
    let cell = if def.cell > 0.0 { def.cell } else { 1.0 };
    let outline = poly(&def.outline);
    let mut lo = outline.iter().copied().fold(Vec2::splat(f32::MAX), Vec2::min);
    let mut hi = outline.iter().copied().fold(Vec2::splat(f32::MIN), Vec2::max);
    if outline.is_empty() {
        lo = Vec2::ZERO;
        hi = Vec2::ZERO;
    }
    let margin = def.shelf.max(0.0) + 22.0;
    let origin = ((lo - Vec2::splat(margin)) / cell).floor() * cell;
    let far = ((hi + Vec2::splat(margin)) / cell).ceil() * cell;
    let w = (((far.x - origin.x) / cell).round() as usize + 1).max(2);
    let h = (((far.y - origin.y) / cell).round() as usize + 1).max(2);
    let seed = def.seed ^ 0x15_1a_d0;
    let g = Gen { def, outline, flats: def.flatten.iter().map(|f| (poly(&f.polygon), f)).collect(), seed };
    let pos = |i: usize, k: usize| origin + Vec2::new(i as f32, k as f32) * cell;

    // 1. Natural ground.
    let mut heights = vec![0.0f32; w * h];
    let mut coast = vec![0.0f32; w * h];
    for k in 0..h {
        for i in 0..w {
            let p = pos(i, k);
            let d = g.coast(p);
            heights[k * w + i] = g.natural(p, d).max(DEEP);
            coast[k * w + i] = d;
        }
    }
    // 2. Plateaus, blended out.
    let flat_w = |p: Vec2, f: &(Vec<Vec2>, &Flatten)| -> f32 {
        let sd = signed_distance(p, &f.0);
        if sd >= 0.0 { 1.0 } else { smooth(1.0 + sd / f.1.blend.max(0.01)) }
    };
    let mut flat_mix: Vec<(f32, Option<&Flatten>)> = vec![(0.0, None); w * h];
    for k in 0..h {
        for i in 0..w {
            let p = pos(i, k);
            for f in &g.flats {
                let wgt = flat_w(p, f);
                if wgt > 0.0 {
                    let v = &mut heights[k * w + i];
                    *v += (f.1.height - *v) * wgt;
                    if wgt > flat_mix[k * w + i].0 {
                        flat_mix[k * w + i] = (wgt, Some(f.1));
                    }
                }
            }
        }
    }
    // 3. Roads: a smoothed centre-line profile, carved in with shoulders.
    let sample = |hs: &[f32], p: Vec2| -> f32 {
        let g = ((p - origin) / cell).clamp(Vec2::ZERO, Vec2::new((w - 1) as f32, (h - 1) as f32));
        let (i, k) = (g.x.round() as usize, g.y.round() as usize);
        hs[k * w + i]
    };
    let pinned = |p: Vec2| -> (f32, bool) {
        match g.flats.iter().find(|f| signed_distance(p, &f.0) >= 0.0) {
            Some(f) => (f.1.height, true),
            None => (sample(&heights, p), false),
        }
    };
    let roads: Vec<RoadPath> = def.roads.iter().map(|r| RoadPath::new(r, &pinned)).collect();
    let mut flags = vec![0u8; (w - 1) * (h - 1)];
    let mut road_cell = vec![false; w * h];
    // Distance from the nearest road's edge (negative on it).
    let mut road_d = vec![f32::INFINITY; w * h];
    for k in 0..h {
        for i in 0..w {
            let p = pos(i, k);
            for r in &roads {
                let Some((d, rh)) = r.query(p) else { continue };
                road_d[k * w + i] = road_d[k * w + i].min(d - r.half);
                let shoulder = RoadPath::SHOULDER;
                if d < r.half + shoulder {
                    let wgt = 1.0 - smooth((d - r.half) / shoulder);
                    let v = &mut heights[k * w + i];
                    *v += (rh - *v) * wgt;
                    if d < r.half + 0.3 {
                        road_cell[k * w + i] = true;
                    }
                }
            }
        }
    }
    // 4. Plateau interiors stay exactly flat (roads included).
    let mut flat_cell = vec![None::<&Flatten>; w * h];
    for k in 0..h {
        for i in 0..w {
            let p = pos(i, k);
            for f in &g.flats {
                let sd = signed_distance(p, &f.0);
                if sd >= 0.0 {
                    heights[k * w + i] = f.1.height;
                }
                if sd >= -0.5 {
                    flat_cell[k * w + i] = Some(f.1);
                }
            }
        }
    }

    // 5. Cell flags: climbable cliffs (slope > 55°, relief > 1.5 m within 2 m), roads, plateaus.
    let hs = &heights;
    let at = |i: i32, k: i32| hs[(k.clamp(0, h as i32 - 1) as usize) * w + i.clamp(0, w as i32 - 1) as usize];
    let tan55 = 55f32.to_radians().tan();
    for k in 0..h - 1 {
        for i in 0..w - 1 {
            let (a, b, c, d) = (hs[k * w + i], hs[k * w + i + 1], hs[(k + 1) * w + i], hs[(k + 1) * w + i + 1]);
            let g1 = Vec2::new(b - a, c - a).length() / cell;
            let g2 = Vec2::new(d - c, d - b).length() / cell;
            let mut f = 0u8;
            if g1.max(g2) > tan55 {
                let (mut mn, mut mx) = (f32::MAX, f32::MIN);
                for dk in -2..=3 {
                    for di in -2..=3 {
                        let v = at(i as i32 + di, k as i32 + dk);
                        mn = mn.min(v);
                        mx = mx.max(v);
                    }
                }
                if mx - mn > 1.5 && mx > 0.5 {
                    f |= CELL_CLIMB;
                }
            }
            if road_cell[k * w + i] && road_cell[k * w + i + 1] && road_cell[(k + 1) * w + i] && road_cell[(k + 1) * w + i + 1] {
                f |= CELL_ROAD;
            }
            if flat_cell[k * w + i].is_some() {
                f |= CELL_FLAT;
            }
            flags[k * (w - 1) + i] = f;
        }
    }

    // 6. Ground per sample (the dominant one) and its colour, blended so edges stay soft.
    let mut ground = vec![Ground::Sand; w * h];
    let mut colors = vec![0u32; w * h];
    let col = |g: Ground| Color::hex(g.hex());
    for k in 0..h {
        for i in 0..w {
            let p = pos(i, k);
            let idx = k * w + i;
            let ht = hs[idx];
            let gx = (at(i as i32 + 1, k as i32) - at(i as i32 - 1, k as i32)) / (2.0 * cell);
            let gz = (at(i as i32, k as i32 + 1) - at(i as i32, k as i32 - 1)) / (2.0 * cell);
            let slope = Vec2::new(gx, gz).length().atan().to_degrees();
            let d = coast[idx];
            let wob = (fbm(seed ^ 0x77, p.x / 7.0, p.y / 7.0, 2) - 0.5) * 2.0;
            let inland = smooth((d - def.beach - 8.0) / 20.0);
            let jn = fbm(seed ^ 0x1a6e, p.x / 45.0, p.y / 45.0, 3) * inland;
            let jungle_t = smooth((jn - 0.40 + wob * 0.03) / 0.07 + 0.5);
            let sand_t = (1.0 - smooth((d - def.beach - wob * 2.5) / 4.0 + 0.5)) * (1.0 - smooth(ht - 3.0));
            let sand_t = if ht < -0.2 { 1.0 } else { sand_t };
            let wet_t = (1.0 - smooth((ht - 0.3 - wob * 0.15) / 0.5 + 0.5)) * (1.0 - smooth((-ht - 1.0) / 0.8));
            let rock_t = smooth((slope - 36.0 - wob * 4.0) / 10.0 + 0.5);
            let cliff_t = smooth((slope - 50.0) / 8.0 + 0.5);
            let road_t = 1.0 - smooth(road_d[idx] / 1.6 + 0.5);
            let (flat_t, flat) = flat_mix[idx];
            let flat_t = if flat.and_then(|f| f.ground).is_some() { flat_t } else { 0.0 };
            let mut c = col(Ground::Grass).lerp(col(Ground::Jungle), jungle_t);
            c = c.lerp(col(Ground::Sand), sand_t);
            c = c.lerp(col(Ground::WetSand), wet_t);
            if let Some(f) = flat.filter(|_| flat_t > 0.0) {
                let fc = f.color.as_deref().map(Color::hex).or(f.ground.map(col)).unwrap_or(c);
                c = c.lerp(fc, flat_t);
            }
            c = c.lerp(col(Ground::Dirt), road_t);
            // Rock and cliff faces with strata.
            let strata = 0.86 + 0.14 * (ht * 1.9 + noise(seed ^ 0x5a, p.x / 9.0, p.y / 9.0) * 4.0).sin();
            c = c.lerp(col(Ground::Rock).scale(strata), rock_t);
            c = c.lerp(col(Ground::Cliff).scale(strata), cliff_t);
            colors[idx] = pack(c);
            ground[idx] = if cliff_t > 0.5 {
                Ground::Cliff
            } else if rock_t > 0.5 {
                Ground::Rock
            } else if road_t > 0.5 {
                Ground::Dirt
            } else if flat_t > 0.5 {
                flat.and_then(|f| f.ground).unwrap_or(Ground::Grass)
            } else if ht < -0.3 {
                Ground::Sand
            } else if wet_t > 0.5 {
                Ground::WetSand
            } else if sand_t > 0.5 {
                Ground::Sand
            } else if jungle_t > 0.5 {
                Ground::Jungle
            } else {
                Ground::Grass
            };
        }
    }
    Heightfield { name: def.name.clone(), origin, cell, w, h, heights, ground, colors, flags, def: def.clone() }
}

// ---------------------------------------------------------------------------------------------
// Terrain chunks: render mesh + height-field collider per 32 m chunk.

/// Chunks wholly below this are open seabed and get no patch.
const SKIP_BELOW: f32 = DEEP + 0.05;

fn ground_color(base: Color, p: Vec2, h: f32, slope: f32) -> Color {
    // Two scales of mottling, dry and lush patches, slope darkening, a darker sea floor.
    let n = noise(0xa11, p.x / 2.3, p.y / 2.3) * 0.5 + noise(0xa12, p.x / 9.0, p.y / 9.0) * 0.5;
    let mut c = base.scale(0.9 + 0.2 * n);
    let patch = noise(0xd27, p.x / 15.0, p.y / 15.0);
    let green = base.0[1] > base.0[0] * 1.15;
    if green {
        c = c.lerp(Color::hex("#9aa85a"), (patch - 0.62).max(0.0) * 1.3);
        c = c.lerp(Color::hex("#2a5a30"), (0.3 - patch).max(0.0) * 1.0);
    }
    let dark = 1.0 - 0.22 * ((slope - 20.0) / 40.0).clamp(0.0, 1.0);
    c = c.scale(dark);
    if h < 0.0 {
        c = c.lerp(Color::hex("#4f8a86"), (-h / 9.0).clamp(0.0, 0.75));
    }
    c
}

/// Terrain patches for these islands, one per 32 m chunk they touch (chunks that are all open
/// seabed are skipped). Each draws a vertex-coloured mesh with skirts and collides as a
/// height field sampled where the mesh's vertices are, so both are the same surface.
pub fn terrain_patches(fields: &[&Heightfield]) -> Vec<TerrainPatch> {
    if fields.is_empty() {
        return Vec::new();
    }
    let lo = fields.iter().map(|f| f.origin).fold(Vec2::splat(f32::MAX), Vec2::min);
    let hi = fields.iter().map(|f| f.max()).fold(Vec2::splat(f32::MIN), Vec2::max);
    let n = CHUNK_SIZE as i32;
    let (cx0, cz0) = ((lo.x / CHUNK_SIZE).floor() as i32, (lo.y / CHUNK_SIZE).floor() as i32);
    let (cx1, cz1) = (((hi.x - 0.01) / CHUNK_SIZE).floor() as i32, ((hi.y - 0.01) / CHUNK_SIZE).floor() as i32);
    let mut out = Vec::new();
    for cz in cz0..=cz1 {
        for cx in cx0..=cx1 {
            if let Some(p) = chunk_patch(fields, cx, cz, n) {
                out.push(p);
            }
        }
    }
    out
}

fn chunk_patch(fields: &[&Heightfield], cx: i32, cz: i32, n: i32) -> Option<TerrainPatch> {
    let (x0, z0) = (cx * n, cz * n);
    let s = (n + 1) as usize;
    let ht = |x: i32, z: i32| height_at(fields, Vec2::new(x as f32, z as f32));
    let mut heights = Vec::with_capacity(s * s);
    for k in 0..=n {
        for i in 0..=n {
            heights.push(ht(x0 + i, z0 + k));
        }
    }
    let top = heights.iter().copied().fold(f32::MIN, f32::max);
    if top < SKIP_BELOW {
        return None;
    }
    let mut m = TerrainPatch::default();
    (m.cx, m.cz) = (cx, cz);
    let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    for k in 0..=n {
        for i in 0..=n {
            let (x, z) = (x0 + i, z0 + k);
            let y = heights[k as usize * s + i as usize];
            let gx = (ht(x + 1, z) - ht(x - 1, z)) * 0.5;
            let gz = (ht(x, z + 1) - ht(x, z - 1)) * 0.5;
            let normal = Vec3::new(-gx, 1.0, -gz).normalize();
            let p = Vec2::new(x as f32, z as f32);
            let base = field_at(fields, p).map_or(Color::hex(Ground::Sand.hex()), |f| f.color(p));
            let slope = normal.y.clamp(-1.0, 1.0).acos().to_degrees();
            let col = ground_color(base, p, y, slope);
            m.positions.push([x as f32, y, z as f32]);
            m.normals.push(normal.to_array());
            m.colors.push(col.0);
            lo = lo.min(Vec3::new(x as f32, y, z as f32));
            hi = hi.max(Vec3::new(x as f32, y, z as f32));
        }
    }
    let idx = |i: i32, k: i32| (k * (n + 1) + i) as u32;
    for k in 0..n {
        for i in 0..n {
            // (x0,z0) (x0,z1) (x1,z0) | (x0,z1) (x1,z1) (x1,z0): rapier's split.
            let (a, b, c, d) = (idx(i, k), idx(i + 1, k), idx(i, k + 1), idx(i + 1, k + 1));
            m.indices.extend_from_slice(&[a, c, b, c, d, b]);
        }
    }
    // Skirts: each border hangs a strip 3 m down, so neighbours never show a crack.
    let skirt = 3.0;
    let borders: [(Vec<(i32, i32)>, [f32; 3]); 4] = [
        ((0..=n).map(|i| (i, 0)).collect(), [0.0, 0.0, -1.0]),
        ((0..=n).map(|i| (i, n)).collect(), [0.0, 0.0, 1.0]),
        ((0..=n).map(|k| (0, k)).collect(), [-1.0, 0.0, 0.0]),
        ((0..=n).map(|k| (n, k)).collect(), [1.0, 0.0, 0.0]),
    ];
    for (edge, normal) in borders {
        let first = m.positions.len() as u32;
        for &(i, k) in &edge {
            let v = idx(i, k) as usize;
            let p = m.positions[v];
            let c = Color(m.colors[v]).scale(0.7);
            m.positions.push(p);
            m.normals.push(normal);
            m.colors.push(c.0);
            m.positions.push([p[0], p[1] - skirt, p[2]]);
            m.normals.push(normal);
            m.colors.push(c.0);
        }
        for e in 0..edge.len() as u32 - 1 {
            let (a, b) = (first + e * 2, first + e * 2 + 2);
            m.indices.extend_from_slice(&[a, a + 1, b + 1, a, b + 1, b]);
        }
        lo.y = lo.y.min(lo.y - skirt);
    }
    m.min = lo;
    m.max = hi;
    m.heightfield = Some(HeightGrid { origin: Vec2::new(x0 as f32, z0 as f32), cell: 1.0, nx: s, nz: s, heights });
    m.build_shape();
    Some(m)
}

// ---------------------------------------------------------------------------------------------
// Levels: islands + structures + vegetation + sea.

/// The sea of a level (until the ocean renderer exists: a flat water zone that swims).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct SeaDef {
    /// The water's extent (x, z min and max); default: the islands' grids.
    pub min: Option<[f32; 2]>,
    pub max: Option<[f32; 2]>,
    pub color: String,
    /// Add a swimmable water zone (y −14 … 0).
    pub zone: bool,
}

impl Default for SeaDef {
    fn default() -> Self {
        Self { min: None, max: None, color: "#2f8fa6".into(), zone: true }
    }
}

/// A level's world data: `[[island]]`, `[[structure]]`, `[vegetation]` and `[sea]`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LevelDef {
    pub island: Vec<IslandDef>,
    pub structure: Vec<StructureDef>,
    pub vegetation: VegetationDef,
    pub sea: Option<SeaDef>,
    /// Named points (spawn etc.), x, y, z; y is lifted onto the ground when 0 is written.
    pub markers: BTreeMap<String, [f32; 3]>,
}

impl LevelDef {
    pub fn parse(text: &str) -> Result<Self, String> {
        toml::from_str(text).map_err(|e| e.to_string())
    }
}

/// What a built level hands to game code.
#[derive(Clone, Debug, Default)]
pub struct BuiltLevel {
    pub fields: Vec<Arc<Heightfield>>,
    /// Every structure's output (blocks were added to the world; the rest is here for game
    /// code: markers, areas, gates).
    pub structures: StructureOut,
    pub vegetation: VegetationOut,
    /// Gameplay areas: the structures' (soft landings, perches, hiding spots) and the
    /// islands' stalk grass (hiding).
    pub areas: Vec<Area>,
    /// Name → world position: structure markers (`chapel.perch`, `house3.door`) and the level's.
    pub markers: BTreeMap<String, Vec3>,
    /// Gates by name, ready for `structures::set_gate`.
    pub gates: BTreeMap<String, crate::structures::GateRefs>,
}

impl BuiltLevel {
    pub fn field_refs(&self) -> Vec<&Heightfield> {
        self.fields.iter().map(|f| f.as_ref()).collect()
    }
    /// Ground height over every island of the level.
    pub fn ground(&self, p: Vec2) -> f32 {
        height_at(&self.field_refs(), p)
    }
}

/// Builds a level into the sim's static world: terrain chunks (mesh + height-field collider),
/// structures, vegetation and the sea. Deterministic.
pub fn build_level(sim: &mut Sim, def: &LevelDef) -> BuiltLevel {
    let mut out = BuiltLevel { fields: def.island.iter().map(|d| Arc::new(generate(d))).collect(), ..Default::default() };
    let arcs = out.fields.clone();
    let refs: Vec<&Heightfield> = arcs.iter().map(|f| f.as_ref()).collect();
    let ground = |p: Vec2| height_at(&refs, p);

    let st = &mut sim.state;
    for patch in terrain_patches(&refs) {
        st.statics.set_terrain(&mut st.physics, RegionKey::Chunk(patch.cx, patch.cz), Arc::new(patch));
    }

    // Structures.
    let mut all = StructureOut::default();
    for (i, s) in def.structure.iter().enumerate() {
        let built = crate::structures::generate_with(s, i, &ground, &all.blocks);
        all.merge(built);
    }
    let gates = all.place(&mut st.statics, &mut st.physics);
    out.gates = gates;

    // Vegetation.
    let veg = crate::vegetation::scatter(&def.vegetation, &refs, &all.footprints);
    veg.place(&mut st.statics, &mut st.physics);

    // Sea.
    if let Some(sea) = &def.sea {
        let lo = refs.iter().map(|f| f.origin).fold(Vec2::splat(f32::MAX), Vec2::min);
        let hi = refs.iter().map(|f| f.max()).fold(Vec2::splat(f32::MIN), Vec2::max);
        let min = sea.min.map(v2).unwrap_or(lo);
        let max = sea.max.map(v2).unwrap_or(hi);
        if sea.zone && min.x < max.x && min.y < max.y {
            let zone = Zone {
                min: Vec3::new(min.x, DEEP - 2.0, min.y),
                max: Vec3::new(max.x, 0.0, max.y),
                kind: ZoneKind::Water,
                course: String::new(),
                index: 0,
                params: Default::default(),
                camera: None,
                label: String::new(),
                label_size: None,
                color: Some(Color::hex(&sea.color)),
                facing: None,
                speed: 0.0,
                signal: String::new(),
                note: String::new(),
            };
            st.statics.add_zone_to(RegionKey::Hub, zone);
        }
    }

    // Areas and markers.
    out.areas = all.areas.clone();
    for f in &refs {
        for (gi, g) in f.def.grass.iter().enumerate() {
            let pts = poly(g);
            let lo = pts.iter().copied().fold(Vec2::splat(f32::MAX), Vec2::min);
            let hi = pts.iter().copied().fold(Vec2::splat(f32::MIN), Vec2::max);
            let base = pts.iter().map(|p| f.height(*p)).fold(f32::MAX, f32::min);
            out.areas.push(Area {
                name: format!("{}.grass{gi}", if f.name.is_empty() { "island" } else { &f.name }),
                kind: AreaKind::Hide,
                min: Vec3::new(lo.x, base - 0.5, lo.y),
                max: Vec3::new(hi.x, base + 3.0, hi.y),
                polygon: g.clone(),
            });
        }
    }
    out.markers = all.markers.clone();
    for (name, p) in &def.markers {
        let y = if p[1] == 0.0 { ground(Vec2::new(p[0], p[2])).max(0.0) } else { p[1] };
        out.markers.insert(name.clone(), Vec3::new(p[0], y, p[2]));
    }
    out.structures = all;
    out.vegetation = veg;
    out
}

// ---------------------------------------------------------------------------------------------
// The yard: a scene for looking at all of this.

/// The yard level: one ~200 m island with every structure kind (`crates/pav_core/src/yard.toml`).
pub const YARD: &str = include_str!("yard.toml");

pub fn yard_def() -> LevelDef {
    LevelDef::parse(YARD).unwrap_or_else(|e| panic!("yard.toml: {e}"))
}

/// Builds the `yard` scene: the island, its town, fort and docks, the player on the street.
pub fn build_yard(sim: &mut Sim) -> BuiltLevel {
    let def = yard_def();
    let built = build_level(sim, &def);
    let spawn = built.markers.get("spawn").copied().unwrap_or(Vec3::new(0.0, 5.0, 0.0));
    sim.state.spawn = spawn + Vec3::Y * 0.05;
    sim.spawn_player();
    sim.state.focus = sim.state.spawn;
    built
}
