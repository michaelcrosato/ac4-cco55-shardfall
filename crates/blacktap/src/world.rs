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

fn tab1_geometry() -> String {
    "tab1_world.toml".into()
}

fn shelf() -> f32 {
    35.0
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct LevelFile {
    /// The generated geometry (islands, structures, plants) for this level.
    #[serde(default = "tab1_geometry")]
    pub geometry: String,
    #[serde(default)]
    pub marker: BTreeMap<String, [f32; 3]>,
    #[serde(default)]
    pub route: BTreeMap<String, Vec<[f32; 2]>>,
    #[serde(default)]
    pub shape: Vec<IslandShape>,
    /// Half-size of the play area (soft boundary at `soft`, hard at `hard`).
    #[serde(default)]
    pub bounds: Bounds,
    #[serde(default)]
    pub npc: Vec<NpcSpawn>,
    #[serde(default)]
    pub hide: Vec<Hide>,
    #[serde(default)]
    pub poster: Vec<[f32; 3]>,
    #[serde(default)]
    pub area: Vec<Area>,
    #[serde(default)]
    pub perch: Vec<Perch>,
    #[serde(default)]
    pub collectible: Vec<Collectible>,
    /// Bot hints: named waypoint lists (x, y, z) with an action at each point.
    #[serde(default)]
    pub bot_hint: Vec<BotHint>,
    /// Markers taken from the generated structures (`chapel_perch = "chapel.perch"`), with
    /// an optional offset: `"keep.flagpole+0,0,1.5"`.
    #[serde(default)]
    pub marker_from: BTreeMap<String, String>,
}

/// People placed by the mission actions (`spawn_town`, `spawn_fort_guards`...), by group.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct NpcSpawn {
    /// The action that spawns it: town, courier, stocks, fort, drye, crowd.
    pub group: String,
    /// Archetype in npcs.toml.
    pub kind: String,
    pub at: [f32; 3],
    /// Facing (degrees, 0 = north) while standing at its post.
    pub facing: f32,
    /// A route it walks (loops unless `once`), or none to keep its post.
    pub route: String,
    pub once: bool,
    /// Extra tags (courier, escort, bell_ringer, drye_guard, gossip...).
    pub tags: Vec<String>,
    /// How many to place around `at` (crowds).
    pub count: u32,
}

/// Concealment: stalk grass and crowds conceal; haystacks and hay carts hide completely (and
/// soften falls).
#[derive(Clone, Debug, Deserialize)]
pub struct Hide {
    pub kind: String,
    pub at: [f32; 3],
    pub radius: f32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Area {
    pub name: String,
    pub kind: String,
    pub center: [f32; 2],
    pub radii: [f32; 2],
}

/// A Crow's Perch: survey here (reveals the named map icons), leap to `leap_to`.
#[derive(Clone, Debug, Deserialize)]
pub struct Perch {
    pub name: String,
    #[serde(default)]
    pub at: [f32; 3],
    /// A marker to stand on instead of `at` (resolved at load).
    #[serde(default)]
    pub marker: String,
    #[serde(default)]
    pub leap_to: String,
    #[serde(default)]
    pub reveals: f32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Collectible {
    pub kind: String,
    pub name: String,
    pub at: [f32; 3],
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct BotHint {
    pub name: String,
    /// (x, y, z) points; `do` per point: walk, run, climb, swim, jump, crouch, wait, interact.
    pub points: Vec<[f32; 3]>,
    #[serde(rename = "do")]
    pub actions: Vec<String>,
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
    pub npcs: Vec<NpcSpawn>,
    pub hides: Vec<Hide>,
    pub posters: Vec<Vec3>,
    pub areas: Vec<Area>,
    pub perches: Vec<Perch>,
    pub collectibles: Vec<Collectible>,
    pub bot_hints: BTreeMap<String, BotHint>,
    /// Generated areas: stalk grass and hay (hiding), soft landings, perches.
    pub zones: Vec<pav_core::structures::Area>,
    /// The geometry file this world was built from.
    pub geometry: String,
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

    /// The named area (restricted zones) containing `p`.
    pub fn area_at(&self, p: Vec2) -> Option<&Area> {
        self.areas.iter().find(|a| {
            let d = (p - Vec2::from(a.center)) / Vec2::from(a.radii).max(Vec2::splat(0.1));
            d.length() < 1.0
        })
    }

    /// The hiding spot `p` is in (feet), if any.
    pub fn hide_at(&self, p: Vec3) -> Option<Hide> {
        if let Some(h) = self.hides.iter().find(|h| {
            let c = Vec3::from(h.at);
            Vec2::new(p.x - c.x, p.z - c.z).length() < h.radius && (p.y - c.y).abs() < 2.0
        }) {
            return Some(h.clone());
        }
        use pav_core::structures::AreaKind;
        let probe = p + Vec3::Y * 0.5;
        self.zones.iter().filter(|a| a.contains(probe)).find_map(|a| {
            let kind = match a.kind {
                AreaKind::Hide if a.name.contains("hay") => "hay",
                AreaKind::Hide => "grass",
                AreaKind::Soft if a.name.contains("hay") => "cart",
                AreaKind::Soft => "awning",
                AreaKind::Perch => return None,
            };
            Some(Hide { kind: kind.into(), at: a.center().to_array(), radius: (a.max - a.min).length() * 0.5 })
        })
    }

    pub fn perch(&self, name: &str) -> Option<&Perch> {
        self.perches.iter().find(|p| p.name == name)
    }

    pub fn hint(&self, name: &str) -> Option<&BotHint> {
        self.bot_hints.get(name)
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
    let level = crate::island::prebuilt_file(&f.geometry);
    let half = f.bounds.hard + 40.0;
    let seabed = Arc::new(if level.fields.is_empty() {
        bake_seabed(&f.shape, half)
    } else {
        let refs: Vec<&pav_core::island::Heightfield> = level.fields.iter().map(|x| x.as_ref()).collect();
        pav_core::island::seabed(&refs, Vec2::splat(-half), Vec2::splat(half), 4.0)
    });
    let mut markers: BTreeMap<String, Vec3> = f.marker.iter().map(|(k, v)| (k.clone(), Vec3::from(*v))).collect();
    for (k, v) in &level.markers {
        markers.entry(k.clone()).or_insert(*v);
    }
    for (alias, from) in &f.marker_from {
        let (name, off) = from.split_once('+').map(|(n, o)| (n, Some(o))).unwrap_or((from.as_str(), None));
        let Some(mut p) = level.markers.get(name).copied() else {
            log::warn!("tab1.toml: marker_from '{alias}' names unknown structure marker '{name}'");
            continue;
        };
        if let Some(o) = off {
            let v: Vec<f32> = o.split(',').filter_map(|x| x.trim().parse().ok()).collect();
            if v.len() == 3 {
                p += Vec3::new(v[0], v[1], v[2]);
            }
        }
        markers.insert(alias.clone(), p);
    }
    // Collectibles written at height 0 lie on the ground.
    let mut collectibles = f.collectible;
    for c in &mut collectibles {
        if c.at[1] == 0.0 && !level.fields.is_empty() {
            c.at[1] = level.ground(Vec2::new(c.at[0], c.at[2])).max(0.15);
        }
    }
    let mut perches = f.perch;
    for p in &mut perches {
        if let Some(m) = markers.get(&p.marker).filter(|_| !p.marker.is_empty()) {
            p.at = m.to_array();
        }
    }
    let terrain: Option<Arc<dyn Fn(Vec2) -> Option<f32> + Send + Sync>> = if level.fields.is_empty() {
        None
    } else {
        let lv = level.clone();
        Some(Arc::new(move |p: Vec2| Some(lv.ground(p))))
    };
    let w = Arc::new(World {
        markers,
        routes: f.route.iter().map(|(k, v)| (k.clone(), v.iter().map(|p| Vec2::from(*p)).collect())).collect(),
        shapes: f.shape,
        bounds: f.bounds,
        seabed,
        terrain,
        npcs: f.npc,
        hides: f.hide,
        posters: f.poster.iter().map(|p| Vec3::from(*p)).collect(),
        areas: f.area,
        perches,
        collectibles,
        bot_hints: f.bot_hint.into_iter().map(|h| (h.name.clone(), h)).collect(),
        zones: level.areas.clone(),
        geometry: f.geometry.clone(),
    });
    *CACHE.lock().unwrap_or_else(|p| p.into_inner()) = Some((key, w.clone()));
    Ok(w)
}
