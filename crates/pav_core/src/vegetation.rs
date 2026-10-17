//! Vegetation: deterministic scatter rules (data) over island heightfields, explicit
//! placements, and stalk grass from the islands' grass polygons.
//!
//! Plants are static decor: baked library props (`StaticProp`, no entity) whose leaf parts
//! sway in the wind, grass tufts drawn by the grass renderer, and, for palms and trees, a
//! climbable trunk `Pole` with a cylinder collider. Rules keep clear of roads, structure
//! footprints, plateaus (unless allowed) and stalk grass.
//!
//! ```toml
//! [vegetation]
//! seed = 3
//! [[vegetation.rule]]
//! asset = "LIB/core/palm"          # any embedded prop, `NAME:preset` for a family preset
//! ground = ["sand", "grass"]       # allowed ground (`island::Ground` names)
//! height = [0.6, 8.0]              # allowed height band (m)
//! slope = 20                       # steepest ground (degrees)
//! spacing = 7                      # one try per cell of this size (jittered grid)
//! chance = 0.4                     # of a plant per cell
//! patchy = 0.3                     # 0 even … 1 only in noise patches
//! scale = [0.85, 1.2]
//! seeds = 4                        # variants (prop seeds 0..n)
//! pole = { radius = 0.17, height = 6.5, lean = 1.4 }   # climbable trunk (along the lean)
//! [[vegetation.place]]
//! asset = "LIB/core/palm:tall"
//! at = [10, 4]
//! yaw = 40
//! ```

use std::collections::BTreeMap;

use glam::{Quat, Vec2, Vec3};
use serde::{Deserialize, Serialize};

use crate::color::Color;
use crate::island::{Ground, Heightfield, fbm, height_at, point_in_polygon, polyline_dist, signed_distance, v2};
use crate::physics::PhysicsState;
use crate::rng::hash_f32;
use crate::shape::{Look, Shape, Sway};
use crate::statics::{Decor, Pole, RegionKey, StaticProp, StaticWorld};
use crate::structures::Footprint;

fn one() -> f32 {
    1.0
}
fn d_spacing() -> f32 {
    6.0
}
fn d_chance() -> f32 {
    0.4
}
fn d_scale() -> [f32; 2] {
    [0.9, 1.1]
}
fn d_slope() -> f32 {
    30.0
}
fn d_height() -> [f32; 2] {
    [0.3, 60.0]
}
fn d_seeds() -> u32 {
    1
}
fn d_road() -> f32 {
    1.5
}

/// A climbable trunk added with each plant of a rule: a pole from the base, `height` up (at
/// scale 1), leaning `lean` metres toward the plant's local +x.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrunkDef {
    pub radius: f32,
    pub height: f32,
    #[serde(default)]
    pub lean: f32,
}

/// Undergrowth drawn by the grass renderer: one instance per clump (cheap; no collider).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TuftDef {
    /// Height range (m).
    pub height: [f32; 2],
    /// Width (m).
    pub width: f32,
    pub color: String,
    #[serde(default)]
    pub color2: Option<String>,
}

/// A scatter rule: a prop (`asset`) or a grass-renderer clump (`tuft`) per hit.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScatterRule {
    #[serde(default)]
    pub asset: String,
    #[serde(default)]
    pub tuft: Option<TuftDef>,
    /// Allowed ground; empty: any land.
    #[serde(default)]
    pub ground: Vec<Ground>,
    #[serde(default = "d_height")]
    pub height: [f32; 2],
    /// Steepest slope (degrees).
    #[serde(default = "d_slope")]
    pub slope: f32,
    #[serde(default = "d_spacing")]
    pub spacing: f32,
    #[serde(default = "d_chance")]
    pub chance: f32,
    /// 0: even cover; toward 1: plants only where a noise field is high (clumps).
    #[serde(default)]
    pub patchy: f32,
    #[serde(default = "d_scale")]
    pub scale: [f32; 2],
    /// Prop seeds (variants) 0..n.
    #[serde(default = "d_seeds")]
    pub seeds: u32,
    /// Allowed on flattened plateaus (town squares).
    #[serde(default)]
    pub plateaus: bool,
    /// Allowed in stalk grass.
    #[serde(default)]
    pub in_grass: bool,
    /// Clearance from road edges and structure footprints (m).
    #[serde(default = "d_road")]
    pub clear: f32,
    /// Collides (the prop's box round its solid parts).
    #[serde(default)]
    pub solid: bool,
    #[serde(default)]
    pub pole: Option<TrunkDef>,
    /// Tilt the plant to the ground's slope (rocks, driftwood).
    #[serde(default)]
    pub align: bool,
}

/// One plant placed by hand.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlantDef {
    pub asset: String,
    pub at: [f32; 2],
    #[serde(default)]
    pub yaw: f32,
    #[serde(default = "one")]
    pub scale: f32,
    #[serde(default)]
    pub seed: u32,
    #[serde(default)]
    pub pole: Option<TrunkDef>,
    /// Ground height override (on a roof, a plinth); default: the terrain.
    #[serde(default)]
    pub y: Option<f32>,
}

/// How stalk grass is drawn.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct GrassStyle {
    /// Tuft spacing (jittered grid, m).
    pub spacing: f32,
    /// Tuft height range (m).
    pub height: [f32; 2],
    /// Tuft width (m).
    pub width: f32,
    pub color: String,
    pub color2: String,
}

impl Default for GrassStyle {
    fn default() -> Self {
        Self { spacing: 0.85, height: [1.0, 1.3], width: 1.0, color: "#a3b04a".into(), color2: "#7f9a3c".into() }
    }
}

/// The `[vegetation]` table of a level.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct VegetationDef {
    pub seed: u64,
    pub rule: Vec<ScatterRule>,
    pub place: Vec<PlantDef>,
    pub grass: GrassStyle,
    /// Scatter only inside this box (x, z min and max); default: the islands.
    pub bounds: Option<[[f32; 2]; 2]>,
}

/// What the scatter produced.
#[derive(Clone, Debug, Default)]
pub struct VegetationOut {
    pub props: Vec<StaticProp>,
    pub poles: Vec<Pole>,
    /// Stalk-grass tufts (sway: grass) and other single-shape decor.
    pub decor: Vec<Decor>,
    /// Plants per asset (reports).
    pub counts: BTreeMap<String, usize>,
}

impl VegetationOut {
    pub fn place(&self, statics: &mut StaticWorld, physics: &mut PhysicsState) {
        for p in &self.props {
            if let Err(e) = statics.add_prop(physics, p.clone()) {
                log::warn!("vegetation '{}': {e}", p.asset);
            }
        }
        for p in &self.poles {
            statics.add_pole(physics, p.clone());
        }
        for d in &self.decor {
            statics.add_decor(physics, RegionKey::chunk_of(d.pos), d.clone());
        }
    }

    /// A stable hash (determinism tests).
    pub fn hash(&self) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        let mut mix = |x: u64| h = (h ^ x).wrapping_mul(0x0100_0000_01b3);
        for p in &self.props {
            for c in p.asset.bytes() {
                mix(c as u64);
            }
            for v in p.pos.to_array().into_iter().chain(p.rot.to_array()).chain([p.scale]) {
                mix(v.to_bits() as u64);
            }
            mix(p.seed as u64);
        }
        for d in &self.decor {
            for v in d.pos.to_array() {
                mix(v.to_bits() as u64);
            }
        }
        h
    }
}

/// What keeps plants away.
struct Clearance<'a> {
    fields: &'a [&'a Heightfield],
    footprints: &'a [Footprint],
    roads: Vec<(Vec<Vec2>, f32)>,
    flats: Vec<Vec<Vec2>>,
    grass: Vec<Vec<Vec2>>,
}

impl Clearance<'_> {
    fn ok(&self, p: Vec2, rule_clear: f32, plateaus: bool, in_grass: bool) -> bool {
        if self.footprints.iter().any(|f| f.distance(p) < rule_clear) {
            return false;
        }
        if self.roads.iter().any(|(line, half)| polyline_dist(p, line).0 < half + rule_clear) {
            return false;
        }
        if !plateaus && self.flats.iter().any(|f| signed_distance(p, f) > -2.0) {
            return false;
        }
        if !in_grass && self.grass.iter().any(|g| point_in_polygon(p, g)) {
            return false;
        }
        true
    }
}

fn trunk_pole(base: Vec3, rot: Quat, scale: f32, t: &TrunkDef) -> Pole {
    let top = base + rot * (Vec3::new(t.lean, t.height, 0.0) * scale);
    Pole::new(base, top, t.radius * scale)
}

/// Scatters the rules over the islands, adds the hand placements and the stalk grass.
/// Deterministic: a jittered grid per rule, hashed from the vegetation seed.
pub fn scatter(def: &VegetationDef, fields: &[&Heightfield], footprints: &[Footprint]) -> VegetationOut {
    let mut out = VegetationOut::default();
    let pts = |v: &[[f32; 2]]| v.iter().copied().map(v2).collect::<Vec<Vec2>>();
    let clear = Clearance {
        fields,
        footprints,
        roads: fields.iter().flat_map(|f| f.def.roads.iter().map(|r| (pts(&r.points), r.width * 0.5))).collect(),
        flats: fields.iter().flat_map(|f| f.def.flatten.iter().map(|fl| pts(&fl.polygon))).collect(),
        grass: fields.iter().flat_map(|f| f.def.grass.iter().map(|g| pts(g))).collect(),
    };
    let (lo, hi) = match def.bounds {
        Some([a, b]) => (v2(a).min(v2(b)), v2(a).max(v2(b))),
        None => (
            fields.iter().map(|f| f.origin).fold(Vec2::splat(f32::MAX), Vec2::min),
            fields.iter().map(|f| f.max()).fold(Vec2::splat(f32::MIN), Vec2::max),
        ),
    };
    let seed = def.seed ^ 0x7e9e_7a71;
    // Earlier rules claim space: later plants keep their spacing from them.
    let mut taken: Vec<(Vec2, f32)> = Vec::new();
    if lo.x < hi.x {
        for (ri, rule) in def.rule.iter().enumerate() {
            let rs = seed.wrapping_add((ri as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15));
            let s = rule.spacing.max(0.5);
            let (nx, nz) = (((hi.x - lo.x) / s).ceil() as i32, ((hi.y - lo.y) / s).ceil() as i32);
            for k in 0..nz {
                for i in 0..nx {
                    let roll = hash_f32(rs, i, k, 1);
                    if roll >= rule.chance {
                        continue;
                    }
                    let p = lo
                        + Vec2::new(
                            (i as f32 + 0.15 + 0.7 * hash_f32(rs, i, k, 2)) * s,
                            (k as f32 + 0.15 + 0.7 * hash_f32(rs, i, k, 3)) * s,
                        );
                    if rule.patchy > 0.0 {
                        let n = fbm(rs ^ 0xc1, p.x / 30.0, p.y / 30.0, 3);
                        if n < rule.patchy * 0.6 {
                            continue;
                        }
                    }
                    let h = height_at(fields, p);
                    if h < rule.height[0] || h > rule.height[1] {
                        continue;
                    }
                    let Some(f) = fields.iter().find(|f| f.contains(p)) else { continue };
                    if !rule.ground.is_empty() && !rule.ground.contains(&f.ground(p)) {
                        continue;
                    }
                    if f.slope(p) > rule.slope {
                        continue;
                    }
                    if !clear.ok(p, rule.clear, rule.plateaus, rule.in_grass) {
                        continue;
                    }
                    let gap = s * 0.45;
                    if taken.iter().any(|(q, r)| q.distance(p) < gap.max(*r)) {
                        continue;
                    }
                    let yaw = hash_f32(rs, i, k, 4) * std::f32::consts::TAU;
                    let scale = rule.scale[0] + (rule.scale[1] - rule.scale[0]) * hash_f32(rs, i, k, 5);
                    let variant = (hash_f32(rs, i, k, 6) * rule.seeds.max(1) as f32) as u32 % rule.seeds.max(1);
                    let mut rot = Quat::from_rotation_y(yaw);
                    if rule.align {
                        let n = f.normal(p);
                        rot = Quat::from_rotation_arc(Vec3::Y, n) * rot;
                    }
                    // Sink a little so nothing floats on a slope.
                    let base = Vec3::new(p.x, h - 0.05 - f.slope(p).to_radians().tan() * 0.25 * scale, p.y);
                    if let Some(t) = &rule.tuft {
                        let th = (t.height[0] + (t.height[1] - t.height[0]) * hash_f32(rs, i, k, 7)) * scale;
                        let c1 = Color::hex(&t.color);
                        let c2 = t.color2.as_deref().map_or(c1, Color::hex);
                        out.decor.push(tuft(base, t.width * scale, th, yaw, c1.lerp(c2, hash_f32(rs, i, k, 8))));
                        *out.counts.entry("tufts".into()).or_default() += 1;
                        taken.push((p, gap * 0.4));
                        continue;
                    }
                    out.props.push(StaticProp::new(&rule.asset, base, rot, scale, variant, rule.solid));
                    if let Some(t) = &rule.pole {
                        out.poles.push(trunk_pole(base, rot, scale, t));
                    }
                    *out.counts.entry(rule.asset.clone()).or_default() += 1;
                    taken.push((p, gap * 0.6));
                }
            }
        }
    }
    for pl in &def.place {
        let p = v2(pl.at);
        let h = pl.y.unwrap_or_else(|| height_at(fields, p));
        let rot = Quat::from_rotation_y(-pl.yaw.to_radians());
        let base = Vec3::new(p.x, h - 0.03, p.y);
        out.props.push(StaticProp::new(&pl.asset, base, rot, pl.scale, pl.seed, false));
        if let Some(t) = &pl.pole {
            out.poles.push(trunk_pole(base, rot, pl.scale, t));
        }
        *out.counts.entry(pl.asset.clone()).or_default() += 1;
    }
    stalk_grass(def, &clear, &mut out);
    out
}

/// A clump drawn by the grass renderer (it bends in the wind and parts round the player).
fn tuft(base: Vec3, width: f32, height: f32, yaw: f32, color: Color) -> Decor {
    Decor {
        shape: Shape::Box { half: Vec3::new(width * 0.5, height * 0.5, width * 0.5) },
        pos: base + Vec3::Y * (height * 0.5 - 0.06),
        rot: Quat::from_rotation_y(yaw),
        color,
        look: Look::Cel,
        emissive: 0.0,
        solid: false,
        collider: None,
        sway: Sway::Grass,
        material: None,
    }
}

/// Dense tall tufts filling the islands' grass polygons (hiding places).
fn stalk_grass(def: &VegetationDef, clear: &Clearance, out: &mut VegetationOut) {
    let g = &def.grass;
    let s = g.spacing.max(0.3);
    let (c1, c2) = (Color::hex(&g.color), Color::hex(&g.color2));
    let mut n = 0usize;
    for (pi, poly) in clear.grass.iter().enumerate() {
        let lo = poly.iter().copied().fold(Vec2::splat(f32::MAX), Vec2::min);
        let hi = poly.iter().copied().fold(Vec2::splat(f32::MIN), Vec2::max);
        let gs = def.seed ^ 0x6a55 ^ (pi as u64).wrapping_mul(0x51ed);
        let (nx, nz) = (((hi.x - lo.x) / s).ceil() as i32, ((hi.y - lo.y) / s).ceil() as i32);
        for k in 0..=nz {
            for i in 0..=nx {
                let p =
                    lo + Vec2::new((i as f32 + 0.5 * hash_f32(gs, i, k, 1)) * s, (k as f32 + 0.5 * hash_f32(gs, i, k, 2)) * s);
                if !point_in_polygon(p, poly) {
                    continue;
                }
                // Thin out toward the edge so patches don't end in a hard line.
                let edge = signed_distance(p, poly);
                if edge < 0.8 && hash_f32(gs, i, k, 3) > edge / 0.8 * 0.7 + 0.3 {
                    continue;
                }
                let h = height_at(clear.fields, p);
                if h < 0.2 {
                    continue;
                }
                let th = g.height[0] + (g.height[1] - g.height[0]) * hash_f32(gs, i, k, 4);
                let wd = g.width * (0.85 + 0.3 * hash_f32(gs, i, k, 5));
                let color = c1.lerp(c2, hash_f32(gs, i, k, 6)).scale(0.92 + 0.16 * hash_f32(gs, i, k, 7));
                out.decor.push(tuft(Vec3::new(p.x, h - 0.02, p.y), wd, th, hash_f32(gs, i, k, 8) * std::f32::consts::TAU, color));
                n += 1;
            }
        }
    }
    if n > 0 {
        *out.counts.entry("stalk_grass".into()).or_default() += n;
    }
}
