//! Islands: deterministic generation, the collider matching the drawn mesh, flat plateaus,
//! the seabed's shelf and deep water, climbable cliffs, and a measured 320 × 220 m island.
// `as f32` on rapier reals is a no-op with f32 physics but needed by the f64 switch.
#![allow(clippy::unnecessary_cast)]

use glam::{Vec2, Vec3};
use pav_core::island::{self, DEEP, Ground, Heightfield, IslandDef};
use pav_core::rapier::prelude::*;
use pav_core::statics::RegionKey;
use pav_core::terrain::TerrainPatch;
use pav_core::{InputFrame, Sim};

fn yard_island() -> IslandDef {
    island::yard_def().island.remove(0)
}

/// Height of a patch's drawn mesh at `p` (the patch's grid triangles, as drawn).
fn mesh_height(patch: &TerrainPatch, p: Vec2) -> f32 {
    let n = 32usize;
    let (x0, z0) = (patch.cx as f32 * 32.0, patch.cz as f32 * 32.0);
    let g = p - Vec2::new(x0, z0);
    let (i, k) = ((g.x.floor() as usize).min(n - 1), (g.y.floor() as usize).min(n - 1));
    let (fx, fz) = (g.x - i as f32, g.y - k as f32);
    let y = |i: usize, k: usize| patch.positions[k * (n + 1) + i][1];
    let (a, b, c, d) = (y(i, k), y(i + 1, k), y(i, k + 1), y(i + 1, k + 1));
    if fx + fz <= 1.0 { a + (b - a) * fx + (c - a) * fz } else { d + (c - d) * (1.0 - fx) + (b - d) * (1.0 - fz) }
}

#[test]
fn generation_is_deterministic() {
    let def = yard_island();
    let a = island::generate(&def);
    let b = island::generate(&def);
    assert_eq!(a.hash(), b.hash());
    assert!(a.w > 200 && a.h > 150, "grid {}x{}", a.w, a.h);
    // The whole level too: same blocks, same plants, same state.
    let mut s1 = Sim::new("yard", 1).unwrap();
    let mut s2 = Sim::new("yard", 1).unwrap();
    assert_eq!(s1.state_hash(), s2.state_hash());
    let v1 = island::build_level(&mut Sim::empty(1), &island::yard_def()).vegetation.hash();
    let v2 = island::build_level(&mut Sim::empty(1), &island::yard_def()).vegetation.hash();
    assert_eq!(v1, v2, "vegetation scatter is deterministic");
    s1.run(30, &InputFrame::default());
    s2.run(30, &InputFrame::default());
    assert_eq!(s1.state_hash(), s2.state_hash());
}

#[test]
fn collider_height_matches_mesh_and_queries() {
    let def = yard_island();
    let hf = island::generate(&def);
    let fields = [&hf];
    let patches = island::terrain_patches(&fields);
    assert!(!patches.is_empty());
    let mut sim = Sim::empty(1);
    let st = &mut sim.state;
    for p in &patches {
        st.statics.set_terrain(&mut st.physics, RegionKey::Chunk(p.cx, p.cz), std::sync::Arc::new(p.clone()));
    }
    sim.step(&InputFrame::default());
    let mut checked = 0;
    let mut worst = 0.0f32;
    for k in 0..40 {
        for i in 0..40 {
            // Off-grid points over the whole island (hash-free, deterministic).
            let p = hf.origin + Vec2::new(7.37 + i as f32 * 6.91, 5.13 + k as f32 * 5.27);
            if !hf.contains(p) {
                continue;
            }
            let Some(patch) = patches.iter().find(|t| t.cx == (p.x / 32.0).floor() as i32 && t.cz == (p.y / 32.0).floor() as i32)
            else {
                continue;
            };
            let mesh = mesh_height(patch, p);
            let query = hf.height(p);
            let ray = Ray::new(Vec3::new(p.x, 80.0, p.y), Vec3::NEG_Y);
            let qp = sim.state.physics.query();
            let (_, toi) = qp.cast_ray(&ray, 200.0, true).expect("the collider is under every patch");
            let collider = 80.0 - toi as f32;
            worst = worst.max((collider - mesh).abs()).max((query - mesh).abs());
            assert!((collider - mesh).abs() < 0.05, "collider {collider} vs mesh {mesh} at {p}");
            assert!((query - mesh).abs() < 0.05, "height() {query} vs mesh {mesh} at {p}");
            checked += 1;
        }
    }
    assert!(checked > 500, "checked {checked} points");
    println!("collider vs mesh: {checked} points, worst {worst:.4} m");
    // Queries: open sea is deep, the beach is sand and gentle, the sea cliff is climbable.
    assert_eq!(hf.height(Vec2::new(5000.0, 0.0)), DEEP);
    let cliff = &def.cliffs[0];
    let (a, b) = (Vec2::new(cliff.from[0], cliff.from[1]), Vec2::new(cliff.to[0], cliff.to[1]));
    let mut climbable = 0;
    for t in 0..60 {
        let along = a.lerp(b, 0.2 + 0.6 * t as f32 / 60.0);
        // Walk out to sea from the cliff line across the face.
        let out = along.normalize();
        for s in 0..60 {
            let p = along + out * s as f32 * 0.5;
            if hf.climbable(p) {
                climbable += 1;
                assert!(hf.slope(p) > 45.0, "climbable cells are steep");
            }
        }
    }
    assert!(climbable > 30, "the sea cliff has a climbable face ({climbable} samples)");
    let beach = Vec2::new(-20.0, 61.0);
    assert!(!hf.climbable(beach) && hf.slope(beach) < 15.0, "the beach is gentle ({}°)", hf.slope(beach));
    assert!(matches!(hf.ground(beach), Ground::Sand | Ground::WetSand), "{:?}", hf.ground(beach));
}

#[test]
fn flatten_areas_are_flat() {
    let def = yard_island();
    let hf = island::generate(&def);
    for f in &def.flatten {
        let pts: Vec<Vec2> = f.polygon.iter().map(|p| Vec2::new(p[0], p[1])).collect();
        let lo = pts.iter().copied().fold(Vec2::splat(f32::MAX), Vec2::min);
        let hi = pts.iter().copied().fold(Vec2::splat(f32::MIN), Vec2::max);
        let mut n = 0;
        for k in 0..30 {
            for i in 0..30 {
                let p = lo + (hi - lo) * Vec2::new((i as f32 + 0.5) / 30.0, (k as f32 + 0.5) / 30.0);
                // At least a cell inside the polygon.
                let inside = (0..8).all(|a| {
                    let d = Vec2::from_angle(a as f32 * std::f32::consts::FRAC_PI_4) * 1.1;
                    island_point_in(&pts, p + d)
                });
                if !inside {
                    continue;
                }
                let h = hf.height(p);
                assert!((h - f.height).abs() < 0.05, "plateau at {p} is {h}, not {}", f.height);
                n += 1;
            }
        }
        assert!(n > 100, "sampled {n} points of a plateau");
    }
}

fn island_point_in(poly: &[Vec2], p: Vec2) -> bool {
    let mut inside = false;
    let mut j = poly.len() - 1;
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[j]);
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            inside = !inside;
        }
        j = i;
    }
    inside
}

#[test]
fn seabed_is_shallow_on_the_shelf_and_deep_beyond() {
    let def = yard_island();
    let hf = island::generate(&def);
    let fields: [&Heightfield; 1] = [&hf];
    let bed = island::seabed(&fields, hf.origin - Vec2::splat(40.0), hf.max() + Vec2::splat(40.0), 2.0);
    // Points just offshore along the south bay (the shelf), and far out.
    let mut shelf = 0;
    for x in [-60.0f32, -40.0, -20.0, 0.0, 20.0] {
        // Find the waterline going south, then sample the shelf beyond it.
        let mut z = 40.0;
        while hf.height(Vec2::new(x, z)) > 0.0 && z < 140.0 {
            z += 0.5;
        }
        for off in [3.0f32, 8.0, 14.0] {
            let p = Vec2::new(x, z + off);
            let d = bed.height(p);
            assert!(d > -2.0 && d <= 0.05, "shelf at {p} is {d} m: shallower than 2 m");
            shelf += 1;
        }
    }
    assert_eq!(shelf, 15);
    let far = Vec2::new(0.0, hf.max().y + 30.0);
    assert_eq!(bed.height(far), DEEP, "outside the grid is open sea");
    let edge = Vec2::new(0.0, hf.max().y - 3.0);
    assert!((bed.height(edge) - DEEP).abs() < 0.3, "the grid's edge is deep water: {}", bed.height(edge));
}

#[test]
fn measure_a_320_by_220_island() {
    let def = IslandDef {
        name: "big".into(),
        outline: vec![
            [-160.0, -60.0],
            [-90.0, -110.0],
            [40.0, -112.0],
            [150.0, -80.0],
            [162.0, 10.0],
            [120.0, 100.0],
            [0.0, 110.0],
            [-120.0, 90.0],
            [-162.0, 20.0],
        ],
        peaks: vec![
            island::Peak { at: [-30.0, -20.0], height: 38.0, radius: 90.0 },
            island::Peak { at: [80.0, -40.0], height: 22.0, radius: 50.0 },
        ],
        cliffs: vec![island::Cliff { from: [100.0, -95.0], to: [160.0, -20.0], height: 16.0 }],
        flatten: vec![island::Flatten {
            polygon: vec![[-40.0, 40.0], [40.0, 40.0], [40.0, 90.0], [-40.0, 90.0]],
            height: 3.0,
            blend: 8.0,
            ground: Some(Ground::Dirt),
            color: None,
        }],
        roads: vec![island::Road { points: vec![[0.0, 40.0], [40.0, 0.0], [100.0, -40.0]], width: 4.0 }],
        seed: 9,
        ..yard_island()
    };
    let t = std::time::Instant::now();
    let hf = island::generate(&def);
    let gen_ms = t.elapsed().as_secs_f64() * 1000.0;
    let t = std::time::Instant::now();
    let patches = island::terrain_patches(&[&hf]);
    let mesh_ms = t.elapsed().as_secs_f64() * 1000.0;
    let mesh_bytes: usize = patches
        .iter()
        .map(|p| p.positions.len() * 36 + p.indices.len() * 4 + p.heightfield.as_ref().map_or(0, |g| g.heights.len() * 4))
        .sum();
    let t = std::time::Instant::now();
    let mut sim = Sim::empty(1);
    let st = &mut sim.state;
    for p in &patches {
        st.statics.set_terrain(&mut st.physics, RegionKey::Chunk(p.cx, p.cz), std::sync::Arc::new(p.clone()));
    }
    let collider_ms = t.elapsed().as_secs_f64() * 1000.0;
    // The same chunks as triangle meshes, for comparison.
    let t = std::time::Instant::now();
    let mut trimesh = 0usize;
    for p in &patches {
        let mut q = p.clone();
        q.heightfield = None;
        q.build_shape();
        trimesh += q.indices.len() / 3;
    }
    let trimesh_ms = t.elapsed().as_secs_f64() * 1000.0;
    println!(
        "320x220 island: grid {}x{} ({} samples), generate {gen_ms:.0} ms, heightfield {:.2} MB; {} chunks, mesh {mesh_ms:.0} ms, {:.1} MB; height-field colliders {collider_ms:.0} ms vs trimesh colliders {trimesh_ms:.0} ms ({trimesh} triangles)",
        hf.w,
        hf.h,
        hf.w * hf.h,
        hf.bytes() as f64 / 1e6,
        patches.len(),
        mesh_bytes as f64 / 1e6
    );
    assert!(hf.w >= 360 && hf.h >= 260);
    assert!(collider_ms < trimesh_ms, "height fields build faster than trimeshes");
    // The whole yard level: island, 40 structures, vegetation, sea.
    let t = std::time::Instant::now();
    let mut sim = Sim::empty(1);
    let built = island::build_level(&mut sim, &island::yard_def());
    let level_ms = t.elapsed().as_secs_f64() * 1000.0;
    let (blocks, decor, props, ladders) = built.structures.counts();
    println!(
        "yard build {level_ms:.0} ms: {blocks} blocks, {decor} decor, {props} props, {ladders} ladders from structures; plants {:?}; {} colliders",
        built.vegetation.counts,
        sim.state.physics.colliders.len()
    );
}
