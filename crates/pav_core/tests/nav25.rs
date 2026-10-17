//! Nav 2.5D (`pav_core::nav25`): paths round walls, two layers under a roof, stairs up a fort
//! wall, a hill on an island and its cliff, the yard's docks to the fort gate, gates and links,
//! a flow field round a building, determinism, and the build's cost on a 400 × 350 m island.
// `as f32` on rapier reals is a no-op with f32 physics but needed by the f64 switch.
#![allow(clippy::unnecessary_cast)]

use glam::{Vec2, Vec3};
use pav_core::island::{self, IslandDef};
use pav_core::nav25::NavGrid25;
use pav_core::statics::{Block, RegionKey};
use pav_core::structures::{self, Kind, Side, StructureDef};
use pav_core::{Color, Sim};

fn add_block(sim: &mut Sim, min: Vec3, max: Vec3) {
    let st = &mut sim.state;
    st.statics.add(&mut st.physics, Block::new(min, max, Color::hex("#888888")));
}

/// A world with flat ground (top at y = 0) out to ±`half` m.
fn flat(half: f32) -> Sim {
    let mut sim = Sim::empty(1);
    add_block(&mut sim, Vec3::new(-half, -1.0, -half), Vec3::new(half, 0.0, half));
    sim
}

/// Length of the walk from `from` through `pts`.
fn walk_len(from: Vec3, pts: &[Vec3]) -> f32 {
    let mut at = from;
    let mut len = 0.0;
    for p in pts {
        len += (*p - at).length();
        at = *p;
    }
    len
}

/// Points every 0.2 m along the walk from the cell a path starts on (the one nearest `from`),
/// each with the walkable height under it nearest the previous one's: None where a leg leaves
/// the walkable cells or jumps more than a step.
fn trace(grid: &NavGrid25, from: Vec3, pts: &[Vec3]) -> Option<Vec<Vec3>> {
    let mut out = Vec::new();
    let mut at = grid.nearest_walkable(from, 2.0)?;
    let mut y = at.y;
    for p in pts {
        let d = *p - at;
        let n = (Vec2::new(d.x, d.z).length() / 0.2).ceil().max(1.0) as usize;
        for k in 1..=n {
            let q = at + d * (k as f32 / n as f32);
            let c = grid.cell_at(Vec3::new(q.x, y, q.z))?;
            if c.blocked || (c.pos.y - y).abs() > 0.65 {
                return None;
            }
            y = c.pos.y;
            out.push(Vec3::new(q.x, y, q.z));
        }
        at = *p;
    }
    Some(out)
}

#[test]
fn paths_go_round_a_wall_never_through() {
    let mut sim = flat(30.0);
    add_block(&mut sim, Vec3::new(-0.5, 0.0, -6.0), Vec3::new(0.5, 2.5, 6.0));
    let grid = NavGrid25::build(&sim, Vec2::splat(-15.0), Vec2::splat(15.0), 0.5, 10.0);
    assert!(!grid.walkable(Vec3::new(0.0, 0.0, 0.0)), "inside the wall");
    assert!(!grid.walkable(Vec3::new(0.8, 0.0, 0.0)), "within a radius of its face");
    assert!(grid.walkable(Vec3::new(1.3, 0.0, 0.0)), "clear of it");
    assert!(grid.walkable(Vec3::new(0.0, 0.0, 7.0)), "past its end");
    let (from, to) = (Vec3::new(-5.0, 0.0, 0.0), Vec3::new(5.0, 0.0, 0.0));
    let p = grid.path(from, to, 100_000).expect("a way round");
    assert!(p.iter().any(|q| q.z.abs() > 6.0), "goes past an end of the wall: {p:?}");
    let walk = trace(&grid, from, &p).expect("every leg stays on walkable cells");
    assert!(walk.iter().all(|q| q.x.abs() > 0.85 || q.z.abs() > 6.3), "never through the wall or its margin");
    assert!(walk.iter().all(|q| q.y.abs() < 0.01), "on the ground");
    let len = walk_len(from, &p);
    assert!(len < 17.5, "close to the shortest way round: {len:.2} m, {} legs", p.len());
    assert!(p.len() <= 4, "string-pulled to a few legs: {p:?}");
    assert!((p.last().unwrap().x - 5.0).abs() < 1e-3, "ends at the goal");
}

/// An 8 × 6 m house: 0.3 m walls 3.5 m high, a 1.4 m door in the south wall, a flat roof.
fn house(sim: &mut Sim) {
    let (h, t) = (3.5, 0.3);
    add_block(sim, Vec3::new(-4.0, 0.0, -3.0), Vec3::new(4.0, h, -3.0 + t));
    add_block(sim, Vec3::new(-4.0, 0.0, -3.0), Vec3::new(-4.0 + t, h, 3.0));
    add_block(sim, Vec3::new(4.0 - t, 0.0, -3.0), Vec3::new(4.0, h, 3.0));
    add_block(sim, Vec3::new(-4.0, 0.0, 3.0 - t), Vec3::new(-0.7, h, 3.0));
    add_block(sim, Vec3::new(0.7, 0.0, 3.0 - t), Vec3::new(4.0, h, 3.0));
    add_block(sim, Vec3::new(-4.2, h, -3.2), Vec3::new(4.2, h + 0.3, 3.2));
}

#[test]
fn a_roof_and_the_floor_under_it_are_two_layers() {
    let mut sim = flat(30.0);
    house(&mut sim);
    let grid = NavGrid25::build(&sim, Vec2::splat(-12.0), Vec2::splat(12.0), 0.5, 10.0);
    let inside = grid.layers_at(Vec3::new(1.1, 0.0, 0.1));
    assert_eq!(inside.len(), 2, "floor and roof: {inside:?}");
    assert!(inside[0].abs() < 0.01 && (inside[1] - 3.8).abs() < 0.01, "{inside:?}");
    assert_eq!(grid.layers_at(Vec3::new(7.0, 0.0, 0.0)).len(), 1, "outside: the ground only");
    let floor = grid.cell_at(Vec3::new(1.1, 0.2, 0.1)).unwrap();
    let roof = grid.cell_at(Vec3::new(1.1, 3.9, 0.1)).unwrap();
    assert!(floor.layer == 0 && roof.layer == 1 && (roof.pos.y - 3.8).abs() < 0.01);
    assert!(grid.cell_at(Vec3::new(1.1, 1.9, 0.1)).is_none(), "nothing within 1 m of mid-air");
    // Round the house on the ground: never up on the roof.
    let (from, to) = (Vec3::new(-8.0, 0.0, 0.0), Vec3::new(8.0, 0.0, 0.0));
    let p = grid.path(from, to, 100_000).expect("round the house");
    assert!(p.iter().all(|q| q.y.abs() < 0.01), "stays on the ground: {p:?}");
    let walk = trace(&grid, from, &p).expect("walkable legs");
    assert!(walk.iter().all(|q| q.x.abs() > 4.3 || q.z.abs() > 3.3), "round it, not through its walls");
    // In through the door.
    let p = grid.path(Vec3::new(0.0, 0.0, 7.0), Vec3::new(0.0, 0.0, -1.0), 100_000).expect("through the door");
    assert!(walk_len(Vec3::new(0.0, 0.0, 7.0), &p) < 8.5, "straight in: {p:?}");
    // The roof has no way up.
    assert!(grid.path(from, Vec3::new(0.0, 3.8, 0.0), 1_000_000).is_none(), "no stairs, no ladder");
    // Walking the roof stays on it.
    let p = grid.path(Vec3::new(-3.5, 3.8, -2.5), Vec3::new(3.5, 3.8, 2.5), 100_000).expect("across the roof");
    assert!(p.iter().all(|q| (q.y - 3.8).abs() < 0.01), "{p:?}");
}

#[test]
fn stairs_climb_to_the_wall_walk() {
    let mut def = StructureDef::new(Kind::FortWall, [0.0, 0.0]);
    def.name = "wall".into();
    def.size = Some([30.0, 3.0]);
    def.stairs = Some(Side::Right);
    let o = structures::generate(&def, &|_| 0.0);
    let mut sim = flat(60.0);
    {
        let st = &mut sim.state;
        o.place(&mut st.statics, &mut st.physics);
    }
    let walk = o.markers["wall.walk"];
    let bottom = o.markers["wall.stairs_bottom"];
    let grid = NavGrid25::build(&sim, Vec2::new(-25.0, -12.0), Vec2::new(25.0, 12.0), 0.5, 20.0);
    assert!(grid.walkable(walk), "the wall walk is walkable at {walk}");
    let from = Vec3::new(-6.0, 0.0, 6.0);
    let p = grid.path(from, walk, 400_000).expect("up the stairs");
    let end = *p.last().unwrap();
    assert!((end.y - walk.y).abs() < 0.1, "ends on the wall walk ({}): {p:?}", walk.y);
    let walked = trace(&grid, from, &p).expect("each leg follows the cells and steps");
    let mid = walked.iter().filter(|q| q.y > 1.0 && q.y < walk.y - 1.0).count();
    assert!(mid > 5, "climbs through the heights in between ({mid} samples)");
    assert!(walked.iter().any(|q| (q.x - bottom.x).abs() < 6.0 && q.y > 0.2 && q.y < 3.0), "takes the stairs");
    println!("wall walk {:.2} m: path {} legs, {:.1} m", walk.y, p.len(), walk_len(from, &p));
    // Without stairs there is no way up.
    def.stairs = Some(Side::None);
    let o = structures::generate(&def, &|_| 0.0);
    let mut sim = flat(60.0);
    {
        let st = &mut sim.state;
        o.place(&mut st.statics, &mut st.physics);
    }
    let grid = NavGrid25::build(&sim, Vec2::new(-25.0, -12.0), Vec2::new(25.0, 12.0), 0.5, 20.0);
    assert!(grid.walkable(walk));
    assert!(grid.path(from, walk, 1_000_000).is_none(), "no stairs: the walk is out of reach");
}

fn yard_island() -> IslandDef {
    island::yard_def().island.remove(0)
}

#[test]
fn a_gentle_hill_is_walkable_and_a_cliff_is_not() {
    let def = yard_island();
    let hf = island::generate(&def);
    let mut sim = Sim::empty(1);
    {
        let st = &mut sim.state;
        for p in island::terrain_patches(&[&hf]) {
            st.statics.set_terrain(&mut st.physics, RegionKey::Chunk(p.cx, p.cz), std::sync::Arc::new(p));
        }
    }
    let grid = NavGrid25::build(&sim, hf.origin, hf.max(), 0.5, 60.0);
    // From the east (no slope steeper than 30° on the way) up the western peak (21 m).
    let beach = Vec2::new(30.0, -26.0);
    let peak = Vec2::new(-34.0, -26.0);
    let from = Vec3::new(beach.x, hf.height(beach), beach.y);
    let to = Vec3::new(peak.x, hf.height(peak), peak.y);
    assert!(hf.slope(peak) < 20.0 && grid.walkable(to), "the top is gentle ({}°)", hf.slope(peak));
    let p = grid.path(from, to, 1_000_000).expect("up the hill");
    let walk = trace(&grid, from, &p).expect("walkable legs");
    for q in &walk {
        let g = hf.height(Vec2::new(q.x, q.z));
        assert!((q.y - g).abs() < 0.3, "on the terrain at {q}: ground {g}");
    }
    assert!((p.last().unwrap().y - to.y).abs() < 0.3);
    let len = walk_len(from, &p);
    let straight = (to - from).length();
    println!("hill: {straight:.1} m straight, rising {:.1} m; path {len:.1} m in {} legs", to.y - from.y, p.len());
    assert!(len < straight * 1.1, "nearly straight up a gentle hill: {len:.1} vs {straight:.1}");
    // The sea cliff: every steep sample is not walkable; gentle land mostly is.
    let cliff = &def.cliffs[0];
    let (a, b) = (Vec2::new(cliff.from[0], cliff.from[1]), Vec2::new(cliff.to[0], cliff.to[1]));
    let (mut steep, mut gentle, mut gentle_ok) = (0, 0, 0);
    for t in 0..80 {
        let along = a.lerp(b, 0.1 + 0.8 * t as f32 / 80.0);
        for s in -40..40 {
            let p = along + along.normalize() * s as f32 * 0.37;
            let y = hf.height(p);
            let q = Vec3::new(p.x, y, p.y);
            if hf.slope(p) > 50.0 {
                steep += 1;
                let c = grid.cell_at(q);
                assert!(c.is_none_or(|c| (c.pos.y - y).abs() > 0.3), "steep ground at {p} ({}°) is not walkable", hf.slope(p));
            } else if hf.slope(p) < 25.0 && y > 0.5 {
                gentle += 1;
                gentle_ok += grid.walkable(q) as i32;
            }
        }
    }
    println!("cliff: {steep} steep samples, {gentle_ok}/{gentle} gentle ones walkable");
    assert!(steep > 50, "sampled the cliff face ({steep})");
    assert!(gentle_ok * 10 >= gentle * 9, "gentle ground is walkable: {gentle_ok}/{gentle}");
}

/// The yard level (island, town, fort, docks, vegetation, sea) and its grid.
fn yard() -> (Sim, island::BuiltLevel, NavGrid25, f64) {
    let mut sim = Sim::empty(1);
    let built = island::build_level(&mut sim, &island::yard_def());
    let t = std::time::Instant::now();
    let grid = NavGrid25::build(&sim, Vec2::new(-120.0, -100.0), Vec2::new(120.0, 100.0), 0.5, 60.0);
    (sim, built, grid, t.elapsed().as_secs_f64() * 1000.0)
}

#[test]
fn the_yard_docks_reach_the_fort_gate_and_builds_repeat() {
    let (sim, built, grid, ms) = yard();
    println!(
        "yard grid {}x{}: {} nodes, {:.1} MB, built in {ms:.0} ms",
        grid.w,
        grid.h,
        grid.node_count(),
        grid.bytes() as f64 / 1e6
    );
    // Determinism: the same level built again gives the same grid.
    let (_, _, again, _) = yard();
    assert_eq!(grid.hash(), again.hash(), "building twice gives identical grids");
    // From the far end of the pier, along it, over the quay, through the town and up the road.
    let dock = built.markers["pier.end"];
    let gate = built.markers["fort_gate.outside"];
    assert!(grid.nearest_walkable(dock, 1.5).is_some_and(|q| (q.y - dock.y).abs() < 0.1), "the pier's deck at {dock}");
    let t = std::time::Instant::now();
    let p = grid.path(dock, gate, 2_000_000).expect("docks to the fort gate");
    let path_ms = t.elapsed().as_secs_f64() * 1000.0;
    let walk = trace(&grid, dock, &p).expect("walkable legs");
    let len = walk_len(dock, &p);
    let straight = Vec2::new(gate.x - dock.x, gate.z - dock.z).length();
    println!("docks {dock} to gate {gate}: straight {straight:.1} m, path {len:.1} m in {} legs ({path_ms:.2} ms)", p.len());
    assert!(len <= straight * 1.6, "path {len:.1} m vs straight {straight:.1} m");
    let end = *p.last().unwrap();
    assert!((end - gate).length() < 1.5, "ends at the gate: {end} vs {gate}");
    assert!(walk.iter().all(|q| q.y > -0.6), "stays out of the sea");
    // The mortar tower's two stacked ladders link its foot to its top.
    let ladders = grid.links().iter().filter(|l| l.kind == pav_core::nav25::LinkKind::Ladder).count();
    println!("{ladders} ladder links (both ways)");
    let mortar = built.markers["mortar_tower.mortar"];
    let foot = grid.nearest_walkable(Vec3::new(88.0, 15.0, -34.0), 3.0).expect("ground by the tower");
    assert!(grid.connected(foot, built.markers["mortar_tower.top"]));
    assert!(!grid.connected(foot, mortar), "the mortar's bed is a step too high to walk onto");
    let p = grid.path(foot, mortar, 200_000).expect("up the tower's ladders");
    let climb = std::iter::once(foot).chain(p.iter().copied()).collect::<Vec<_>>();
    assert!(climb.windows(2).any(|w| w[1].y - w[0].y > 8.0), "climbs on the ladder link: {climb:?}");
    assert!((climb.last().unwrap().y - mortar.y).abs() < 0.2, "ends on the platform beside the mortar");
    // Walkers that don't climb: no ladder links, and no way up (known at once).
    let cfg = pav_core::nav25::Nav25Config { ladders: false, ..Default::default() };
    let (min, max) = (Vec2::new(60.0, -50.0), Vec2::new(100.0, -15.0));
    let flat_feet = NavGrid25::build_with(&sim, min, max, &cfg, &|_, _| true);
    assert!(flat_feet.links().is_empty());
    assert!(!flat_feet.connected(foot, built.markers["mortar_tower.top"]));
    assert!(flat_feet.path(foot, mortar, 2_000_000).is_none());
}

/// A 10 × 10 m yard walled 3 m high with a 2 m gap in its south wall.
fn walled_yard(sim: &mut Sim) {
    let (h, t) = (3.0, 0.4);
    add_block(sim, Vec3::new(-5.0, 0.0, -5.0), Vec3::new(5.0, h, -5.0 + t));
    add_block(sim, Vec3::new(-5.0, 0.0, -5.0), Vec3::new(-5.0 + t, h, 5.0));
    add_block(sim, Vec3::new(5.0 - t, 0.0, -5.0), Vec3::new(5.0, h, 5.0));
    add_block(sim, Vec3::new(-5.0, 0.0, 5.0 - t), Vec3::new(-1.0, h, 5.0));
    add_block(sim, Vec3::new(1.0, 0.0, 5.0 - t), Vec3::new(5.0, h, 5.0));
}

#[test]
fn a_closed_gate_blocks_and_a_link_joins() {
    let mut sim = flat(30.0);
    walled_yard(&mut sim);
    let mut grid = NavGrid25::build(&sim, Vec2::splat(-12.0), Vec2::splat(12.0), 0.5, 10.0);
    let (out, inside) = (Vec3::new(0.0, 0.0, 10.0), Vec3::new(0.0, 0.0, 0.0));
    let p = grid.path(out, inside, 100_000).expect("in through the gap");
    assert!(walk_len(out, &p) < 10.5, "{p:?}");
    let before = grid.hash();
    let shared = grid.clone();
    // Close the gate: the gap's cells.
    let n = grid.set_blocked_area(Vec3::new(-1.1, -1.0, 4.4), Vec3::new(1.1, 1.0, 5.6), true);
    assert!(n >= 4, "closed {n} cells");
    assert!(!grid.walkable(Vec3::new(0.0, 0.0, 4.8)));
    assert!(grid.path(out, inside, 1_000_000).is_none(), "the gate is shut");
    assert!(shared.path(out, inside, 100_000).is_some(), "a clone made before is unchanged");
    // A link over the wall (a ladder the grid can't see) joins the two sides again.
    let (a, b) = (Vec3::new(3.0, 0.0, -7.0), Vec3::new(3.0, 0.0, -3.5));
    assert!(grid.add_link(a, b, 8.0).is_some());
    let p = grid.path(out, inside, 1_000_000).expect("over the wall by the link");
    let hop = p.windows(2).any(|w| w[0].z < -5.5 && w[1].z > -4.6);
    assert!(hop, "crosses the north wall on the link: {p:?}");
    // Open the gate: back to the short way.
    grid.set_blocked_area(Vec3::new(-1.1, -1.0, 4.4), Vec3::new(1.1, 1.0, 5.6), false);
    let p = grid.path(out, inside, 100_000).unwrap();
    assert!(walk_len(out, &p) < 10.5);
    assert_ne!(grid.hash(), before, "the link is still there");
    // Weighted: watched cells in the gap make the walk take the link instead.
    let watched = |q: Vec3| if q.x.abs() < 1.5 && (q.z - 5.0).abs() < 1.5 { 50.0 } else { 0.0 };
    let p = grid.path_weighted(out, inside, 1_000_000, &watched).expect("a weighted way");
    assert!(p.windows(2).any(|w| w[0].z < -5.5 && w[1].z > -4.6), "avoids the watched gap: {p:?}");
}

#[test]
fn a_pack_follows_the_flow_field_round_a_building() {
    let mut sim = flat(30.0);
    add_block(&mut sim, Vec3::new(-4.0, 0.0, -4.0), Vec3::new(4.0, 4.0, 4.0));
    let grid = NavGrid25::build(&sim, Vec2::splat(-14.0), Vec2::splat(14.0), 0.5, 10.0);
    let goal = Vec3::new(0.0, 0.0, 9.0);
    let field = grid.flow_field(goal, 60.0);
    assert!(field.reached() > 1000);
    assert!(field.dir(Vec3::new(0.0, 4.0, 0.0)).is_none(), "the roof can't reach the ground");
    let starts = [
        Vec3::new(0.0, 0.0, -9.0),
        Vec3::new(-9.0, 0.0, 0.0),
        Vec3::new(9.0, 0.0, 0.0),
        Vec3::new(-7.0, 0.0, -7.5),
        Vec3::new(7.5, 0.0, -8.0),
    ];
    for s in starts {
        let d0 = field.distance(s).expect("in reach");
        assert!(d0 >= (goal - s).length() * 0.98, "no shorter than straight");
        if s.x == 0.0 {
            assert!(d0 > (goal - s).length() + 2.0, "the way from the far side bends round the building");
        }
        let mut p = s;
        let mut steps = 0;
        while (p - goal).length() > 0.5 {
            let d = field.dir(p).unwrap_or_else(|| panic!("a way on from {p} (start {s})"));
            p += Vec3::new(d.x, 0.0, d.y) * 0.25;
            if let Some(c) = grid.cell_at(p) {
                p.y = c.pos.y;
            }
            assert!(p.x.abs() > 4.0 || p.z.abs() > 4.0, "into the building at {p}");
            steps += 1;
            assert!(steps < 400, "reaches the goal from {s} (stuck at {p})");
        }
        println!("from {s}: {steps} steps ({:.1} m), field distance {d0:.1} m", steps as f32 * 0.25);
    }
    // Beyond the reach there is no way.
    let near = grid.flow_field(goal, 5.0);
    assert!(near.dir(Vec3::new(0.0, 0.0, -9.0)).is_none());
}

#[test]
fn builds_are_deterministic_and_clones_share_arrays() {
    let mut sim = flat(30.0);
    house(&mut sim);
    walled_yard(&mut sim);
    add_block(&mut sim, Vec3::new(8.0, 0.0, 8.0), Vec3::new(9.0, 0.3, 9.0));
    let a = NavGrid25::build(&sim, Vec2::splat(-12.0), Vec2::splat(12.0), 0.5, 10.0);
    let b = NavGrid25::build(&sim, Vec2::splat(-12.0), Vec2::splat(12.0), 0.5, 10.0);
    assert_eq!(a.hash(), b.hash());
    let c = a.clone();
    assert!(std::sync::Arc::ptr_eq(&a.layers, &c.layers), "a clone shares the arrays");
    assert_eq!(NavGrid25::default().node_count(), 0);
}

/// The yard's level on an island scaled up to ~400 × 330 m (town, fort and docks as in the yard,
/// vegetation over the whole island): the cost of building the grid and of a 300 m path.
#[test]
fn measure_a_400_by_350_build() {
    let mut def = island::yard_def();
    let k = 1.95;
    for isl in &mut def.island {
        isl.outline.iter_mut().for_each(|p| *p = [p[0] * k, p[1] * k]);
        for pk in &mut isl.peaks {
            pk.at = [pk.at[0] * k, pk.at[1] * k];
            pk.radius *= k;
        }
    }
    def.sea = None;
    let mut sim = Sim::empty(1);
    let built = island::build_level(&mut sim, &def);
    let hf = &built.fields[0];
    let (min, max) = (Vec2::new(-200.0, -175.0), Vec2::new(200.0, 175.0));
    let t = std::time::Instant::now();
    let grid = NavGrid25::build(&sim, min, max, 0.5, 60.0);
    let build_ms = t.elapsed().as_secs_f64() * 1000.0;
    println!(
        "400x350 m at 0.5 m: island grid {}x{} m, {} colliders; nav {}x{} columns, {} nodes, {:.2} MB, {} ladder links; build {build_ms:.0} ms",
        hf.w as f32 * hf.cell,
        hf.h as f32 * hf.cell,
        sim.state.physics.colliders.len(),
        grid.w,
        grid.h,
        grid.node_count(),
        grid.bytes() as f64 / 1e6,
        grid.links().len()
    );
    // Paths across ~300 m of the island, west to east.
    let land = |x: f32, z: f32| {
        let p = Vec2::new(x, z);
        grid.nearest_walkable(Vec3::new(x, hf.height(p), z), 3.0)
    };
    let (mut total, mut n, mut longest) = (0.0, 0, 0.0f32);
    for z in [-60.0f32, -20.0, 20.0] {
        let (Some(a), Some(b)) = (land(-150.0, z), land(150.0, z)) else { continue };
        // The best of 5 (the machine may be busy with other work).
        let mut ms = f64::MAX;
        let mut p = None;
        for _ in 0..5 {
            let t = std::time::Instant::now();
            p = grid.path(a, b, 2_000_000);
            ms = ms.min(t.elapsed().as_secs_f64() * 1000.0);
        }
        if let Some(p) = p {
            longest = longest.max(walk_len(a, &p));
            total += ms;
            n += 1;
            println!("path {a} -> {b}: {:.1} m in {} legs, {ms:.2} ms", walk_len(a, &p), p.len());
        }
    }
    assert!(n > 0, "found paths across the island");
    println!("{n} paths across, longest {longest:.0} m, mean {:.2} ms", total / n as f64);
    assert!(grid.node_count() > 300_000);
    assert!(build_ms < 10_000.0, "builds in {build_ms:.0} ms");
}
