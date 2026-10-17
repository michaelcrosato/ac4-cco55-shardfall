//! The islands, town and fort in the simulation (terrain, colliders, structures). Until the
//! generators are in, the islands exist for the rules (seabed, coasts) and are drawn from
//! their shapes.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use glam::{Vec2, Vec3};
use pav_core::Color;
use pav_core::play::Mesh;
use pav_core::sim::Sim;

use crate::content::Content;
use crate::world::{IslandShape, World, shape_height};

pub fn build(sim: &mut Sim, world: &World, content: &Content) -> anyhow::Result<()> {
    let _ = (sim, world, content);
    Ok(())
}

/// Whether the scene has generated terrain (else the shapes are drawn).
pub fn has_terrain(_sim: &Sim) -> bool {
    false
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
