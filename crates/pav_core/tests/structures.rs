//! Structures: every kind builds deterministically with climb flags and walkable roofs,
//! foundations reach the terrain, the chapel's perch is at 24 m, gates open and close, and a
//! character put on a roof stays there.
// `as f32` on rapier reals is a no-op with f32 physics but needed by the f64 switch.
#![allow(clippy::unnecessary_cast)]

use glam::{Vec2, Vec3};
use pav_core::island;
use pav_core::rapier::prelude::*;
use pav_core::statics::{Block, block_flags};
use pav_core::structures::{self, AreaKind, Kind, StructureDef, StructureOut};
use pav_core::{Color, InputFrame, Sim};

/// Rolling ground: a slope with bumps (foundations must reach down to it).
fn slope(p: Vec2) -> f32 {
    2.0 + p.x * 0.06 + p.y * 0.03 + (p.x * 0.31).sin() * 0.3
}

fn flat(_: Vec2) -> f32 {
    0.0
}

fn def(kind: Kind) -> StructureDef {
    let mut d = StructureDef::new(kind, [0.0, 0.0]);
    d.seed = 7;
    if matches!(kind, Kind::LaundryLine | Kind::Beam) {
        d.to = Some([6.0, 0.0]);
    }
    d
}

fn block_hash(o: &StructureOut) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in &o.blocks {
        for v in b.min.to_array().into_iter().chain(b.max.to_array()) {
            h = (h ^ v.to_bits() as u64).wrapping_mul(0x0100_0000_01b3);
        }
        h = (h ^ b.flags as u64).wrapping_mul(0x0100_0000_01b3);
    }
    for d in &o.decor {
        for v in d.pos.to_array() {
            h = (h ^ v.to_bits() as u64).wrapping_mul(0x0100_0000_01b3);
        }
    }
    h ^ o.markers.len() as u64
}

/// Kinds with walls you can free-climb.
fn climbable(kind: Kind) -> bool {
    !matches!(
        kind,
        Kind::Stall
            | Kind::Stocks
            | Kind::HayCart
            | Kind::Pier
            | Kind::LaundryLine
            | Kind::Beam
            | Kind::Palisade
            | Kind::WantedPoster
            | Kind::Paving
    )
}

/// Kinds with a walkable roof or top a raycast should land on.
fn has_roof(kind: Kind) -> bool {
    !matches!(
        kind,
        Kind::Stall | Kind::Stocks | Kind::Palisade | Kind::WantedPoster | Kind::LaundryLine | Kind::Hut | Kind::Chapel
    )
}

/// The world with one structure placed on flat ground, physics ready for queries.
fn placed(o: &StructureOut) -> (Sim, std::collections::BTreeMap<String, structures::GateRefs>) {
    let mut sim = Sim::empty(1);
    let st = &mut sim.state;
    st.statics.add(&mut st.physics, Block::new(Vec3::new(-60.0, -1.0, -60.0), Vec3::new(60.0, 0.0, 60.0), Color::hex("#888888")));
    let gates = o.place(&mut st.statics, &mut st.physics);
    sim.step(&InputFrame::default());
    (sim, gates)
}

/// What a ray straight down from high above `p` first hits: the block and the height.
fn hit_down(sim: &Sim, p: Vec2) -> Option<(Option<Block>, f32)> {
    let ray = Ray::new(Vec3::new(p.x, 60.0, p.y), Vec3::NEG_Y);
    let qp = sim.state.physics.query();
    let (h, toi) = qp.cast_ray(&ray, 120.0, true)?;
    let block = sim.state.statics.block_of(&sim.state.physics, h).map(|(_, b)| b.clone());
    Some((block, 60.0 - toi as f32))
}

#[test]
fn every_kind_builds_with_flags_and_walkable_roofs() {
    for kind in Kind::ALL {
        let a = structures::generate(&def(kind), &slope);
        let b = structures::generate(&def(kind), &slope);
        assert_eq!(block_hash(&a), block_hash(&b), "{kind:?} is deterministic");
        assert!(!a.blocks.is_empty() || !a.decor.is_empty(), "{kind:?} emits something");
        assert!(a.markers.contains_key(&format!("{}0.center", kind.name())), "{kind:?} has a centre marker");
        if climbable(kind) {
            assert!(a.blocks.iter().any(|b| b.has(block_flags::CLIMBABLE)), "{kind:?} has climbable walls");
        }
        if has_roof(kind) {
            assert!(a.blocks.iter().any(|b| b.has(block_flags::ROOF)), "{kind:?} has a walkable top");
            // A ray down onto the roof (on flat ground, at the structure's highest roof block)
            // lands on a ROOF block.
            let o = structures::generate(&def(kind), &flat);
            let top = o
                .blocks
                .iter()
                .filter(|b| b.has(block_flags::ROOF) && b.max.x - b.min.x > 0.25 && b.max.z - b.min.z > 0.25)
                .max_by(|x, y| x.max.y.total_cmp(&y.max.y))
                .unwrap_or_else(|| panic!("{kind:?}: a roof block"));
            let (sim, _) = placed(&o);
            let c = (top.min + top.max) * 0.5;
            let (hit, y) = hit_down(&sim, Vec2::new(c.x, c.z)).unwrap_or_else(|| panic!("{kind:?}: ray hits"));
            let hit = hit.unwrap_or_else(|| panic!("{kind:?}: the ray hits a block at {c}"));
            assert!(hit.has(block_flags::ROOF), "{kind:?}: the roof under {c} is walkable (hit {:?} at {y})", hit.min);
        }
    }
}

#[test]
fn foundations_touch_the_terrain() {
    for kind in [
        Kind::House,
        Kind::Tavern,
        Kind::Chapel,
        Kind::Warehouse,
        Kind::Hall,
        Kind::Barracks,
        Kind::Keep,
        Kind::FortWall,
        Kind::Bastion,
        Kind::Battery,
        Kind::MortarTower,
        Kind::Gate,
        Kind::Lighthouse,
        Kind::WatchtowerRuin,
        Kind::Hut,
        Kind::Quay,
        Kind::HarbourWall,
    ] {
        let d = def(kind);
        let o = structures::generate(&d, &slope);
        let fp = o.footprints.first().unwrap_or_else(|| panic!("{kind:?} has a footprint"));
        // The ground under the footprint, and the blocks standing on it.
        let mut low_ground = f32::MAX;
        let (n, size) = (12, fp.max - fp.min);
        for k in 0..=n {
            for i in 0..=n {
                let p = fp.min + size * Vec2::new(i as f32 / n as f32, k as f32 / n as f32);
                low_ground = low_ground.min(slope(p));
            }
        }
        let bottom = o.blocks.iter().map(|b| b.min.y).fold(f32::MAX, f32::min);
        assert!(bottom <= low_ground + 0.01, "{kind:?}: lowest block at {bottom}, ground down to {low_ground}");
        // Something solid stands on the lowest corner of the footprint (no gap under it).
        let corners = [fp.min, fp.max, Vec2::new(fp.min.x, fp.max.y), Vec2::new(fp.max.x, fp.min.y)];
        let low = corners.iter().copied().min_by(|a, b| slope(*a).total_cmp(&slope(*b))).unwrap();
        let inset = low + (Vec2::new((fp.min.x + fp.max.x) * 0.5, (fp.min.y + fp.max.y) * 0.5) - low).normalize() * 0.8;
        let g = slope(inset);
        let covered = o.blocks.iter().any(|b| {
            b.min.x <= inset.x
                && b.max.x >= inset.x
                && b.min.z <= inset.y
                && b.max.z >= inset.y
                && b.min.y <= g + 0.01
                && b.max.y > g
        });
        assert!(covered, "{kind:?}: a block reaches the ground at {inset} (ground {g})");
    }
}

#[test]
fn chapel_perch_is_at_24_m() {
    let o = structures::generate(&def(Kind::Chapel), &flat);
    let perch = o.areas.iter().find(|a| a.kind == AreaKind::Perch).expect("the chapel has a perch");
    let base = 0.0;
    assert!((perch.min.y + 0.3 - (base + structures::PLINTH + 24.0)).abs() < 0.6, "perch floor at {}", perch.min.y);
    let m = o.markers["chapel0.perch"];
    assert!((m.y - 24.25).abs() < 0.6, "perch marker at {m}");
    assert!(o.markers.contains_key("chapel0.bell"));
    // The lighthouse's perch is at 28 m.
    let l = structures::generate(&def(Kind::Lighthouse), &flat);
    assert!((l.markers["lighthouse0.perch"].y - 28.0).abs() < 0.3);
    assert!(l.areas.iter().any(|a| a.kind == AreaKind::Perch));
    // The hay cart is a soft landing; the battery marks its cannons.
    let h = structures::generate(&def(Kind::HayCart), &flat);
    assert!(h.areas.iter().any(|a| a.kind == AreaKind::Soft));
    let mut b = def(Kind::Battery);
    b.cannons = Some(3);
    let bat = structures::generate(&b, &flat);
    assert_eq!(bat.markers.keys().filter(|k| k.contains(".cannon")).count(), 3);
}

#[test]
fn gates_open_and_close() {
    let o = structures::generate(&def(Kind::Gate), &flat);
    assert_eq!(o.gates.len(), 1);
    let (mut sim, mut gates) = placed(&o);
    let gate = gates.get_mut("gate0").expect("the gate is placed by name");
    assert!(!gate.is_open, "gates start closed");
    let through = |sim: &Sim| {
        // A horizontal ray along the passage at waist height.
        let ray = Ray::new(Vec3::new(0.0, 1.0, 6.0), Vec3::NEG_Z);
        sim.state.physics.query().cast_ray(&ray, 12.0, true).is_none()
    };
    assert!(!through(&sim), "closed: the passage is blocked");
    let st = &mut sim.state;
    structures::set_gate(&mut st.statics, &mut st.physics, gate, true);
    sim.step(&InputFrame::default());
    assert!(through(&sim), "open: the passage is clear");
    let st = &mut sim.state;
    let gate = gates.get_mut("gate0").unwrap();
    structures::set_gate(&mut st.statics, &mut st.physics, gate, false);
    sim.step(&InputFrame::default());
    assert!(!through(&sim), "closed again");
    // Open leaves stand against the passage walls (alive only when open).
    assert!(gate.open.iter().all(|r| !sim.state.statics.get(*r).unwrap().alive));
}

#[test]
fn a_character_on_a_roof_stays_there() {
    let mut d = def(Kind::House);
    d.floors = Some(2);
    let o = structures::generate(&d, &flat);
    let roof = o.markers["house0.roof"];
    let (mut sim, _) = placed(&o);
    sim.state.spawn = roof + Vec3::Y * 0.05;
    sim.spawn_player();
    for _ in 0..120 {
        sim.step(&InputFrame::default());
    }
    let id = sim.state.player.unwrap();
    let feet = sim.feet(id).unwrap();
    assert!((feet.y - roof.y).abs() < 0.15, "still on the roof: feet {feet}, roof {roof}");
    assert!(sim.player().unwrap().character.as_ref().unwrap().grounded);
}

#[test]
fn the_yard_has_every_kind_and_lines_rest_on_roofs() {
    let def = island::yard_def();
    for kind in Kind::ALL {
        assert!(def.structure.iter().any(|s| s.kind == kind), "the yard shows a {kind:?}");
    }
    let mut sim = Sim::empty(1);
    let built = island::build_level(&mut sim, &def);
    // The laundry line rests on the roofs it joins (parapet height), not on the ground.
    let start = built.markers["line_a.start"];
    let ground = built.ground(Vec2::new(start.x, start.z));
    assert!(start.y > ground + 6.0, "laundry line at {}, ground {ground}", start.y);
    assert!(built.markers.contains_key("chapel.perch") && built.markers.contains_key("keep.flagpole"));
    assert!(built.areas.iter().any(|a| a.kind == AreaKind::Hide), "stalk grass hides");
    assert!(built.gates.contains_key("fort_gate"));
    // Trunks are climbable poles with colliders.
    let poles: usize = sim.state.statics.chunks.values().map(|c| c.poles.len()).sum();
    assert!(poles > 20, "{poles} trunk poles");
}
