//! The Tab I world as data the rules use: named markers, routes, the seabed (shallows, beaches
//! and the deep), ground height for shots and swimmers, and where the islands are. Built once
//! per level text and shared (`Arc`) by every scene built from it.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use glam::{Vec2, Vec3};
use pav_core::ocean::Seabed;
use serde::Deserialize;

/// An island as the rules see it before the terrain generator: an ellipse of land with a
/// shelf of shallows around it (the generator's heightfield replaces this when present).
#[derive(Clone, Debug, Deserialize)]
pub struct IslandShape {
    pub name: String,
    pub center: [f32; 2],
    /// Half-sizes (x, z) of the dry land, metres.
    pub radii: [f32; 2],
    /// Peak height (m).
    pub height: f32,
    /// Width of the shallow shelf around it (m).
    #[serde(default = "shelf")]
    pub shelf: f32,
}

fn shelf() -> f32 {
    35.0
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct LevelFile {
    #[serde(default)]
    pub marker: BTreeMap<String, [f32; 3]>,
    #[serde(default)]
    pub route: BTreeMap<String, Vec<[f32; 2]>>,
    #[serde(default)]
    pub shape: Vec<IslandShape>,
    /// Half-size of the play area (soft boundary at `soft`, hard at `hard`).
    #[serde(default)]
    pub bounds: Bounds,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Bounds {
    pub soft: f32,
    pub hard: f32,
}

impl Default for Bounds {
    fn default() -> Self {
        Self { soft: 560.0, hard: 600.0 }
    }
}

/// The world's static data.
#[derive(Default)]
pub struct World {
    pub markers: BTreeMap<String, Vec3>,
    pub routes: BTreeMap<String, Vec<Vec2>>,
    pub shapes: Vec<IslandShape>,
    pub bounds: Bounds,
    /// Ground height over the whole play area (land above 0, seabed below).
    pub seabed: Arc<Seabed>,
    /// Terrain from the island generator, when the level has it: (x, z) → height.
    pub terrain: Option<Arc<dyn Fn(Vec2) -> Option<f32> + Send + Sync>>,
}

impl World {
    pub fn marker(&self, name: &str) -> Option<Vec3> {
        self.markers.get(name).copied()
    }

    pub fn marker_xz(&self, name: &str) -> Vec2 {
        self.marker(name).map(|p| Vec2::new(p.x, p.z)).unwrap_or(Vec2::ZERO)
    }

    pub fn route(&self, name: &str) -> Vec<Vec2> {
        self.routes.get(name).cloned().unwrap_or_default()
    }

    /// Ground or seabed height at a point (negative under water).
    pub fn ground(&self, p: Vec2) -> f32 {
        if let Some(t) = &self.terrain {
            if let Some(h) = t(p) {
                return h;
            }
        }
        self.seabed.height(p)
    }

    /// The island whose land contains `p`, if any.
    pub fn island_at(&self, p: Vec2) -> Option<&IslandShape> {
        self.shapes.iter().find(|s| ellipse(s, p) < 1.0)
    }
}

/// Normalised ellipse distance (1 = on the coast).
fn ellipse(s: &IslandShape, p: Vec2) -> f32 {
    let d = (p - Vec2::from(s.center)) / Vec2::from(s.radii).max(Vec2::splat(1.0));
    d.length()
}

/// Ground height of the shape model: a beach rising to a rounded peak, a shelf of shallows
/// (−0.5 … −4 m) and the deep (−12 m beyond).
pub fn shape_height(shapes: &[IslandShape], p: Vec2) -> f32 {
    let mut best = -12.0f32;
    for s in shapes {
        let e = ellipse(s, p);
        let r = Vec2::from(s.radii).min_element().max(1.0);
        // Distance outside the coast in metres (approximately).
        let out = (e - 1.0) * r;
        let h = if out <= 0.0 {
            let inland = (-out / r).clamp(0.0, 1.0);
            0.6 + s.height * inland.powf(1.5)
        } else if out < s.shelf {
            let t = out / s.shelf;
            -0.4 - 3.6 * t * t - 1.0 * t
        } else {
            -5.0 - (out - s.shelf).min(40.0) / 40.0 * 7.0
        };
        best = best.max(h);
    }
    best
}

/// Builds the seabed grid for the play area from the shapes (4 m cells).
fn bake_seabed(shapes: &[IslandShape], half: f32) -> Seabed {
    let cell = 4.0;
    let n = ((half * 2.0) / cell).ceil() as usize + 1;
    let origin = Vec2::splat(-half);
    let mut heights = Vec::with_capacity(n * n);
    for z in 0..n {
        for x in 0..n {
            let p = origin + Vec2::new(x as f32 + 0.5, z as f32 + 0.5) * cell;
            heights.push(shape_height(shapes, p));
        }
    }
    Seabed { origin, cell, width: n, depth: n, heights, deep: -12.0 }
}

static CACHE: Mutex<Option<(u64, Arc<World>)>> = Mutex::new(None);

fn hash(text: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in text.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

/// The world for a level file's text (parsed and baked once per distinct text).
pub fn load(text: &str) -> anyhow::Result<Arc<World>> {
    let key = hash(text);
    if let Some((k, w)) = CACHE.lock().unwrap_or_else(|p| p.into_inner()).as_ref() {
        if *k == key {
            return Ok(w.clone());
        }
    }
    let f: LevelFile = toml::from_str(text)?;
    let seabed = Arc::new(bake_seabed(&f.shape, f.bounds.hard + 40.0));
    let w = Arc::new(World {
        markers: f.marker.iter().map(|(k, v)| (k.clone(), Vec3::from(*v))).collect(),
        routes: f.route.iter().map(|(k, v)| (k.clone(), v.iter().map(|p| Vec2::from(*p)).collect())).collect(),
        shapes: f.shape,
        bounds: f.bounds,
        seabed,
        terrain: None,
    });
    *CACHE.lock().unwrap_or_else(|p| p.into_inner()) = Some((key, w.clone()));
    Ok(w)
}
