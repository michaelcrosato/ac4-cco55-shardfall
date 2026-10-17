//! The islands, Dryport and Fort Abstinence in the simulation: heightfield terrain with its
//! colliders, every structure and the vegetation, from `games/blacktap/tab1_world.toml`
//! (`pav_core::island`). Generation is pure, so it runs once per process and every scene only
//! places the cached result (terrain patches are shared `Arc`s).

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use glam::{Vec2, Vec3};
use pav_core::Color;
use pav_core::island::{Heightfield, LevelDef};
use pav_core::play::Mesh;
use pav_core::sim::Sim;
use pav_core::statics::RegionKey;
use pav_core::structures::{Area, StructureOut};
use pav_core::terrain::TerrainPatch;
use pav_core::vegetation::VegetationOut;

use crate::content::Content;
use crate::world::{IslandShape, World, shape_height};

/// Tab I generated once: terrain, structures, plants, areas and markers.
pub struct Prebuilt {
    pub fields: Vec<Arc<Heightfield>>,
    pub patches: Vec<Arc<TerrainPatch>>,
    pub structures: StructureOut,
    pub vegetation: VegetationOut,
    pub areas: Vec<Area>,
    pub markers: BTreeMap<String, Vec3>,
}

impl Prebuilt {
    /// Ground height (the highest island; the open sea floor elsewhere).
    pub fn ground(&self, p: Vec2) -> f32 {
        let refs: Vec<&Heightfield> = self.fields.iter().map(|f| f.as_ref()).collect();
        pav_core::island::height_at(&refs, p)
    }
}

fn generate(text: &str) -> anyhow::Result<Prebuilt> {
    let def = LevelDef::parse(text).map_err(|e| anyhow::anyhow!("tab1_world.toml: {e}"))?;
    let fields: Vec<Arc<Heightfield>> = def.island.iter().map(|d| Arc::new(pav_core::island::generate(d))).collect();
    let refs: Vec<&Heightfield> = fields.iter().map(|f| f.as_ref()).collect();
    let ground = |p: Vec2| pav_core::island::height_at(&refs, p);
    let patches = pav_core::island::terrain_patches(&refs).into_iter().map(Arc::new).collect();
    let mut all = StructureOut::default();
    for (i, s) in def.structure.iter().enumerate() {
        let built = pav_core::structures::generate_with(s, i, &ground, &all.blocks);
        all.merge(built);
    }
    let vegetation = pav_core::vegetation::scatter(&def.vegetation, &refs, &all.footprints);
    let mut areas = all.areas.clone();
    for f in &refs {
        for (gi, g) in f.def.grass.iter().enumerate() {
            let pts: Vec<Vec2> = g.iter().map(|p| Vec2::from(*p)).collect();
            let lo = pts.iter().copied().fold(Vec2::splat(f32::MAX), Vec2::min);
            let hi = pts.iter().copied().fold(Vec2::splat(f32::MIN), Vec2::max);
            let base = pts.iter().map(|p| f.height(*p)).fold(f32::MAX, f32::min);
            areas.push(Area {
                name: format!("{}.grass{gi}", f.name),
                kind: pav_core::structures::AreaKind::Hide,
                min: Vec3::new(lo.x, base - 0.5, lo.y),
                max: Vec3::new(hi.x, base + 3.0, hi.y),
                polygon: g.clone(),
            });
        }
    }
    let mut markers = all.markers.clone();
    for (name, p) in &def.markers {
        let y = if p[1] == 0.0 { ground(Vec2::new(p[0], p[2])).max(0.0) } else { p[1] };
        markers.insert(name.clone(), Vec3::new(p[0], y, p[2]));
    }
    Ok(Prebuilt { fields, patches, structures: all, vegetation, areas, markers })
}

/// The generated Tab I (built on first use).
pub fn prebuilt() -> Arc<Prebuilt> {
    static CACHE: Mutex<Option<Arc<Prebuilt>>> = Mutex::new(None);
    let mut c = CACHE.lock().unwrap_or_else(|p| p.into_inner());
    if let Some(p) = c.as_ref() {
        return p.clone();
    }
    let text = crate::data("tab1_world.toml").unwrap_or_default();
    let p = Arc::new(generate(&text).unwrap_or_else(|e| {
        log::error!("{e:#}");
        Prebuilt {
            fields: Vec::new(),
            patches: Vec::new(),
            structures: StructureOut::default(),
            vegetation: VegetationOut::default(),
            areas: Vec::new(),
            markers: BTreeMap::new(),
        }
    }));
    *c = Some(p.clone());
    p
}

/// Places Tab I in a fresh simulation.
pub fn build(sim: &mut Sim, world: &World, content: &Content) -> anyhow::Result<()> {
    let _ = (world, content);
    let p = prebuilt();
    let st = &mut sim.state;
    for patch in &p.patches {
        st.statics.set_terrain(&mut st.physics, RegionKey::Chunk(patch.cx, patch.cz), patch.clone());
    }
    p.structures.place(&mut st.statics, &mut st.physics);
    p.vegetation.place(&mut st.statics, &mut st.physics);
    Ok(())
}

/// Whether the scene has generated terrain (else the shapes are drawn).
pub fn has_terrain(_sim: &Sim) -> bool {
    !prebuilt().fields.is_empty()
}

/// A drawn mesh of an island shape (cached).
pub fn shape_mesh(s: &IslandShape, i: u64) -> Arc<Mesh> {
    static CACHE: Mutex<BTreeMap<u64, Arc<Mesh>>> = Mutex::new(BTreeMap::new());
    let mut cache = CACHE.lock().unwrap_or_else(|p| p.into_inner());
    if let Some(m) = cache.get(&i) {
        return m.clone();
    }
    let mut m = Mesh::new(0x15_1a_0000 + i);
    let c = Vec2::from(s.center);
    let r = Vec2::from(s.radii) + Vec2::splat(s.shelf * 0.5);
    let cell = 4.0;
    let (nx, nz) = ((r.x * 2.0 / cell) as usize + 1, (r.y * 2.0 / cell) as usize + 1);
    let shapes = std::slice::from_ref(s);
    let sand = Color::hex("#e8d3a2").0;
    let grass = Color::hex("#6aa84f").0;
    let jungle = Color::hex("#2f6d3a").0;
    let wet = Color::hex("#b89e72").0;
    let base = m.positions.len() as u32;
    for z in 0..nz {
        for x in 0..nx {
            let p = c - r + Vec2::new(x as f32, z as f32) * cell;
            let h = shape_height(shapes, p);
            let col = if h < 0.3 {
                wet
            } else if h < 2.0 {
                sand
            } else if h < 10.0 {
                grass
            } else {
                jungle
            };
            m.vertex(Vec3::new(p.x, h, p.y), Vec3::Y, col);
        }
    }
    for z in 0..nz - 1 {
        for x in 0..nx - 1 {
            let i0 = base + (z * nx + x) as u32;
            let (i1, i2, i3) = (i0 + 1, i0 + nx as u32 + 1, i0 + nx as u32);
            m.tri(i0, i2, i1);
            m.tri(i0, i3, i2);
        }
    }
    m.smooth_normals();
    let m = Arc::new(m);
    cache.insert(i, m.clone());
    m
}
