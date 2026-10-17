//! Traversal (`MovementParams::parkour`, PLAN §9.8) on tiny levels built in code: free-run
//! vaults and leaps, ledge safety, Parkour Up and Down, wall, corner and pole climbing,
//! back-ejects, beams, fall landings, and swimming in waves. 60 Hz, scripted input.

use glam::{Quat, Vec2, Vec3};
use pav_core::character::{Character, TraversalState};
use pav_core::input::buttons::{DODGE, JUMP, SPRINT};
use pav_core::ocean::{Ocean, SeaPreset, SeaState};
use pav_core::statics::{Block, Decor, RegionKey};
use pav_core::{Color, InputFrame, Shape, Sim, SimEvent};

/// An empty world with parkour on (everything vertical climbs), and a floor at y = 0 if asked.
fn world(floor: bool) -> Sim {
    let mut sim = Sim::empty(1);
    let m = &mut sim.config.movement;
    m.parkour = true;
    m.climb_all = true;
    m.ledge_grab = true;
    if floor {
        block(&mut sim, Vec3::new(-60.0, -1.0, -60.0), Vec3::new(60.0, 0.0, 60.0));
    }
    sim
}

fn block(sim: &mut Sim, min: Vec3, max: Vec3) {
    let st = &mut sim.state;
    st.statics.add(&mut st.physics, Block::new(min, max, Color::hex("#9a9184")));
}

fn spawn(sim: &mut Sim, feet: Vec3) {
    let id = sim.spawn_character("player", feet);
    sim.state.player = Some(id);
    idle(sim, 10);
}

fn ch(sim: &Sim) -> &Character {
    sim.player().unwrap().character.as_ref().unwrap()
}

fn feet(sim: &Sim) -> Vec3 {
    sim.feet(sim.state.player.unwrap()).unwrap()
}

/// One tick: the sea's clock runs as a game's would.
fn step(sim: &mut Sim, mv: Vec2, held: u32, pressed: u32) {
    let dt = sim.dt();
    if let Some(o) = &mut sim.state.ocean {
        o.time += dt;
    }
    sim.step(&InputFrame { move_dir: mv, held, pressed, ..Default::default() });
}

fn idle(sim: &mut Sim, ticks: u32) {
    for _ in 0..ticks {
        step(sim, Vec2::ZERO, 0, 0);
    }
}

/// Free-run + Parkour Up (SPRINT and JUMP held, JUMP pressed on the first tick) along `mv`.
fn parkour_up(sim: &mut Sim, mv: Vec2, ticks: u32, until: impl Fn(&Character) -> bool) {
    for i in 0..ticks {
        step(sim, mv, SPRINT | JUMP, if i == 0 { JUMP } else { 0 });
        if until(ch(sim)) {
            return;
        }
    }
}

/// The 6 m wall scenario: run at the wall with Parkour Up, hold forward to the top, mantle.
fn climb_six_metres() -> (Sim, bool) {
    let mut sim = world(true);
    // Face at z = 0 looking north (-Z); a roof 6 m deep on top.
    block(&mut sim, Vec3::new(-5.0, 0.0, 0.0), Vec3::new(5.0, 6.0, 6.0));
    spawn(&mut sim, Vec3::new(0.0, 0.0, -4.0));
    let mut climbed = false;
    for i in 0..600 {
        step(&mut sim, Vec2::new(0.0, 1.0), SPRINT | JUMP, if i == 0 { JUMP } else { 0 });
        climbed |= ch(&sim).climb.is_some();
        if ch(&sim).grounded && feet(&sim).y > 5.0 {
            break;
        }
    }
    idle(&mut sim, 30);
    (sim, climbed)
}

#[test]
fn climbs_a_six_metre_wall_and_mantles_onto_the_roof() {
    let (sim, climbed) = climb_six_metres();
    let f = feet(&sim);
    assert!(climbed, "climbed the wall");
    assert!((f.y - 6.0).abs() < 0.2, "feet on the roof: {f}");
    assert!(f.z > 0.0 && f.z < 6.0, "standing on top: {f}");
    assert!(ch(&sim).grounded && ch(&sim).climb.is_none() && ch(&sim).hang.is_none());
    assert_eq!(ch(&sim).traversal_state(), TraversalState::Ground);
}

#[test]
fn wall_climb_is_deterministic() {
    let (a, _) = climb_six_metres();
    let (b, _) = climb_six_metres();
    let (fa, fb) = (feet(&a), feet(&b));
    assert_eq!(fa.to_array().map(f32::to_bits), fb.to_array().map(f32::to_bits), "{fa} vs {fb}");
    assert_eq!(a.state_hash(), b.state_hash());
}

#[test]
fn walking_into_a_wall_never_climbs() {
    let mut sim = world(true);
    block(&mut sim, Vec3::new(-5.0, 0.0, 0.0), Vec3::new(5.0, 6.0, 6.0));
    spawn(&mut sim, Vec3::new(0.0, 0.0, -3.0));
    for _ in 0..180 {
        step(&mut sim, Vec2::new(0.0, 1.0), 0, 0);
        let c = ch(&sim);
        assert!(c.climb.is_none() && c.hang.is_none(), "no climb at walking pace");
    }
    let f = feet(&sim);
    assert!(f.y < 0.1 && ch(&sim).grounded, "still on the ground: {f}");
    assert!(f.z > -0.5, "pressed up against the wall: {f}");
}

/// Climbs onto the face ahead with Parkour Up, then up for `up` ticks.
fn get_on_wall(sim: &mut Sim, toward: Vec2, up: u32) {
    parkour_up(sim, toward, 180, |c| c.climb.is_some());
    assert!(ch(sim).climb.is_some(), "on the wall");
    for _ in 0..up {
        step(sim, toward, 0, 0);
    }
    assert!(ch(sim).climb.is_some(), "still on the wall");
}

#[test]
fn climbing_sideways_wraps_round_an_outer_corner() {
    let mut sim = world(true);
    // A tower; climb its south face (normal +Z) westward round onto its west face (-X).
    block(&mut sim, Vec3::new(0.0, 0.0, 0.0), Vec3::new(4.0, 8.0, 4.0));
    spawn(&mut sim, Vec3::new(2.0, 0.0, 6.5));
    get_on_wall(&mut sim, Vec2::new(0.0, -1.0), 30);
    let n0 = ch(&sim).climb.unwrap().normal;
    assert!(n0.dot(Vec3::Z) > 0.95, "on the south face: {n0}");
    let mut wrapped = false;
    for _ in 0..240 {
        step(&mut sim, Vec2::new(-1.0, 0.0), 0, 0);
        let wc = ch(&sim).climb.expect("still climbing");
        if wc.corner.is_none() && wc.normal.dot(Vec3::NEG_X) > 0.95 {
            wrapped = true;
            break;
        }
    }
    let f = feet(&sim);
    assert!(wrapped, "rounded the corner onto the west face: {:?} at {f}", ch(&sim).climb);
    assert!(f.x < 0.0 && f.z > 0.0 && f.z < 4.0, "beside the west face: {f}");
    assert!(f.y > 1.0, "still up the wall: {f}");
    // And it keeps holding the new face.
    idle(&mut sim, 30);
    assert!(ch(&sim).climb.is_some_and(|w| w.normal.dot(Vec3::NEG_X) > 0.95));
}

#[test]
fn climbing_sideways_turns_into_an_inner_corner() {
    let mut sim = world(true);
    // Two walls meeting: A's south face (normal +Z) runs west into B's east face (normal +X).
    block(&mut sim, Vec3::new(0.0, 0.0, 0.0), Vec3::new(8.0, 8.0, 2.0));
    block(&mut sim, Vec3::new(0.0, 0.0, 2.0), Vec3::new(2.0, 8.0, 8.0));
    spawn(&mut sim, Vec3::new(5.0, 0.0, 4.5));
    get_on_wall(&mut sim, Vec2::new(0.0, -1.0), 30);
    let mut turned = false;
    for _ in 0..240 {
        step(&mut sim, Vec2::new(-1.0, 0.0), 0, 0);
        if ch(&sim).climb.expect("still climbing").normal.dot(Vec3::X) > 0.95 {
            turned = true;
            break;
        }
    }
    let f = feet(&sim);
    assert!(turned, "turned onto the east face: {:?} at {f}", ch(&sim).climb);
    assert!(f.x > 2.0 && f.x < 3.0 && f.z > 2.0, "in the corner: {f}");
    // Climb along the new face (its sideways axis is now north-south).
    for _ in 0..40 {
        step(&mut sim, Vec2::new(0.0, 1.0), 0, 0);
    }
    let g = feet(&sim);
    assert!(ch(&sim).climb.is_some() && g.z > f.z + 0.5, "moved along the east face: {f} -> {g}");
}

#[test]
fn back_eject_catches_the_opposite_wall() {
    let mut sim = world(true);
    // Wall A's face at z = 0 (normal -Z); wall B's face 2.5 m behind at z = -2.5 (normal +Z).
    block(&mut sim, Vec3::new(-5.0, 0.0, 0.0), Vec3::new(5.0, 8.0, 1.0));
    block(&mut sim, Vec3::new(-5.0, 0.0, -3.5), Vec3::new(5.0, 8.0, -2.5));
    spawn(&mut sim, Vec3::new(0.0, 0.0, -1.5));
    get_on_wall(&mut sim, Vec2::new(0.0, 1.0), 40);
    let a = ch(&sim).climb.unwrap().normal;
    assert!(a.dot(Vec3::NEG_Z) > 0.95, "on wall A: {a}");
    let y0 = feet(&sim).y;
    // JUMP with the stick pointing away from the wall.
    step(&mut sim, Vec2::new(0.0, -1.0), JUMP, JUMP);
    assert!(ch(&sim).climb.is_none() && ch(&sim).vel.z < -3.0, "ejected: {}", ch(&sim).vel);
    let mut caught = None;
    for _ in 0..90 {
        step(&mut sim, Vec2::new(0.0, -1.0), 0, 0);
        if let Some(w) = ch(&sim).climb {
            caught = Some((w.normal, feet(&sim)));
            break;
        }
    }
    let (n, f) = caught.expect("caught a wall in flight");
    assert!(n.dot(Vec3::Z) > 0.95, "caught wall B: {n}");
    assert!(f.z < -1.5 && f.y > y0 - 0.5, "across the gap, still high: {f} (from {y0})");
}

#[test]
fn free_run_vaults_a_crate_without_stopping() {
    let mut sim = world(true);
    // A 1 m crate, 1 m deep, across the way.
    block(&mut sim, Vec3::new(-2.0, 0.0, 3.0), Vec3::new(2.0, 1.0, 4.0));
    spawn(&mut sim, Vec3::new(0.0, 0.0, -3.0));
    let mut vaulted = false;
    let mut slowest = f32::MAX;
    for i in 0..120 {
        step(&mut sim, Vec2::new(0.0, 1.0), SPRINT, 0);
        let c = ch(&sim);
        assert!(c.climb.is_none() && c.hang.is_none(), "no climbing or hanging");
        vaulted |= c.traversal_state() == TraversalState::Vault;
        if i > 10 {
            slowest = slowest.min(Vec2::new(c.vel.x, c.vel.z).length());
        }
    }
    let f = feet(&sim);
    assert!(vaulted, "vaulted");
    assert!(f.z > 6.0 && f.y < 0.1, "ran on past the crate: {f}");
    assert!(slowest > 3.0, "never stopped: slowest {slowest:.2} m/s");
}

#[test]
fn ledge_safety_stops_at_a_roof_edge_when_walking() {
    let mut sim = world(true);
    block(&mut sim, Vec3::new(-5.0, 0.0, -5.0), Vec3::new(5.0, 4.0, 5.0));
    spawn(&mut sim, Vec3::new(0.0, 4.0, 0.0));
    for _ in 0..180 {
        step(&mut sim, Vec2::new(1.0, 0.0), 0, 0);
        let f = feet(&sim);
        assert!(f.y > 3.9 && f.x < 5.0, "stays on the roof: {f}");
    }
    let f = feet(&sim);
    assert!(f.x > 4.4 && ch(&sim).grounded, "walked up to the edge: {f}");
    // Diagonally into the edge: slides along it instead.
    let x0 = feet(&sim).z;
    for _ in 0..30 {
        step(&mut sim, Vec2::new(1.0, 1.0).normalize(), 0, 0);
    }
    let g = feet(&sim);
    assert!(g.y > 3.9 && g.x < 5.0 && g.z > x0 + 1.0, "slid along the edge: {g}");
}

#[test]
fn free_run_jump_assist_crosses_a_gap() {
    let mut sim = world(true);
    // Two roofs of equal height, 2.5 m apart.
    block(&mut sim, Vec3::new(-10.0, 0.0, -3.0), Vec3::new(0.0, 4.0, 3.0));
    block(&mut sim, Vec3::new(2.5, 0.0, -3.0), Vec3::new(40.0, 4.0, 3.0));
    spawn(&mut sim, Vec3::new(-8.0, 4.0, 0.0));
    let mut leapt = false;
    for _ in 0..240 {
        step(&mut sim, Vec2::new(1.0, 0.0), SPRINT, 0);
        leapt |= ch(&sim).leap;
        if ch(&sim).grounded && feet(&sim).x > 3.0 {
            break;
        }
    }
    idle(&mut sim, 20);
    let f = feet(&sim);
    assert!(leapt, "an assisted leap");
    assert!(f.x > 2.5 && (f.y - 4.0).abs() < 0.15, "landed on the far roof: {f}");
}

#[test]
fn falls_land_with_their_speed_and_rolls() {
    for (held, roll) in [(0, false), (SPRINT, true)] {
        let mut sim = world(true);
        spawn(&mut sim, Vec3::new(0.0, 12.0, 0.0));
        sim.drain_events();
        let mut land = None;
        for _ in 0..180 {
            step(&mut sim, Vec2::ZERO, held, 0);
            for e in sim.drain_events() {
                if let SimEvent::Land { speed, roll, .. } = e {
                    land.get_or_insert((speed, roll));
                }
            }
        }
        let (speed, r) = land.expect("landed");
        assert!(speed > 14.0, "a 12 m fall lands hard: {speed}");
        assert_eq!(r, roll, "roll flag with SPRINT = {}", held != 0);
    }
}

#[test]
fn climbs_up_and_round_a_pole() {
    let mut sim = world(true);
    let pos = Vec3::new(0.0, 4.0, 0.0);
    let pole = Decor {
        shape: Shape::Cylinder { half_height: 4.0, radius: 0.25 },
        pos,
        rot: Quat::IDENTITY,
        color: Color::hex("#6b4f35"),
        look: Default::default(),
        emissive: 0.0,
        solid: true,
        collider: None,
        sway: Default::default(),
        material: None,
    };
    {
        let st = &mut sim.state;
        st.statics.add_decor(&mut st.physics, RegionKey::chunk_of(pos), pole);
    }
    // Climbable without `climb_all`: poles always are.
    sim.config.movement.climb_all = false;
    spawn(&mut sim, Vec3::new(0.0, 0.0, 2.5));
    parkour_up(&mut sim, Vec2::new(0.0, -1.0), 120, |c| c.climb.is_some());
    let wc = ch(&sim).climb.expect("on the pole");
    assert!(wc.pivot.is_some(), "held as a pole");
    let y0 = feet(&sim).y;
    // Up 3 m: the stick toward the pole.
    for _ in 0..200 {
        let n = ch(&sim).climb.unwrap().normal;
        step(&mut sim, Vec2::new(-n.x, -n.z), 0, 0);
        if feet(&sim).y > y0 + 3.0 {
            break;
        }
    }
    assert!(feet(&sim).y > y0 + 3.0, "climbed 3 m: {} from {y0}", feet(&sim).y);
    // Round it: the stick along the pole's side (a camera following the climber).
    let mut turned = 0.0f32;
    let mut last = Vec2::new(feet(&sim).x, feet(&sim).z).to_angle();
    for _ in 0..180 {
        let n = ch(&sim).climb.expect("still on the pole").normal;
        step(&mut sim, Vec2::new(-n.z, n.x), 0, 0);
        let f = feet(&sim);
        let r = Vec2::new(f.x, f.z).length();
        assert!((r - 0.63).abs() < 0.1, "keeps its hold on the pole: r = {r}");
        let a = Vec2::new(f.x, f.z).to_angle();
        let mut d = a - last;
        if d > std::f32::consts::PI {
            d -= std::f32::consts::TAU;
        } else if d < -std::f32::consts::PI {
            d += std::f32::consts::TAU;
        }
        turned += d;
        last = a;
    }
    assert!(turned.abs() > std::f32::consts::TAU, "went all the way round: {turned:.2} rad");
    assert!(feet(&sim).y > y0 + 2.8, "kept its height");
}

#[test]
fn walks_a_beam_on_its_centreline() {
    let mut sim = world(true);
    // A 0.3 m beam 3 m up between two platforms.
    block(&mut sim, Vec3::new(-3.0, 0.0, -6.0), Vec3::new(3.0, 3.0, -4.0));
    block(&mut sim, Vec3::new(-0.15, 2.7, -4.0), Vec3::new(0.15, 3.0, 4.0));
    block(&mut sim, Vec3::new(-3.0, 0.0, 4.0), Vec3::new(3.0, 3.0, 6.0));
    spawn(&mut sim, Vec3::new(0.0, 3.0, -4.4));
    let mut on = 0;
    let mut widest = 0.0f32;
    for _ in 0..240 {
        // Pushing a little sideways: the balance keeps it on the centreline.
        step(&mut sim, Vec2::new(0.25, 1.0).normalize(), 0, 0);
        let f = feet(&sim);
        assert!(f.y > 2.9, "never fell off: {f}");
        let c = ch(&sim);
        if c.on_beam {
            on += 1;
            if on > 15 {
                widest = widest.max(f.x.abs());
            }
            assert!(Vec2::new(c.vel.x, c.vel.z).length() <= 2.5 + 1e-3, "slow on the beam: {}", c.vel);
            assert_eq!(c.traversal_state(), TraversalState::Beam);
        }
    }
    assert!(widest < 0.1, "kept to the centreline: {widest:.3} m off");
    let f = feet(&sim);
    assert!(on > 60, "balanced on the beam for a while ({on} ticks)");
    assert!(f.z > 4.0, "crossed it: {f}");
}

/// The sea at a point now.
fn sea_at(sim: &Sim, p: Vec3) -> f32 {
    let o = sim.state.ocean.unwrap();
    o.sea.height(Vec2::new(p.x, p.z), o.time)
}

fn sea(sim: &mut Sim, preset: SeaPreset) {
    sim.state.ocean = Some(Ocean { sea: SeaState::preset(preset, 30.0), time: 0.0 });
}

#[test]
fn swimmer_rides_the_waves() {
    let mut sim = world(false);
    block(&mut sim, Vec3::new(-80.0, -31.0, -80.0), Vec3::new(80.0, -30.0, 80.0)); // seabed
    sea(&mut sim, SeaPreset::Breezy);
    spawn(&mut sim, Vec3::new(0.0, -1.6, 0.0));
    idle(&mut sim, 50);
    let mut worst = 0.0f32;
    for i in 0..600 {
        // Swim a lazy circle, sprinting half the time.
        let a = i as f32 * 0.01;
        step(&mut sim, Vec2::new(a.cos(), a.sin()), if i % 240 < 120 { SPRINT } else { 0 }, 0);
        let c = ch(&sim);
        assert!(c.swimming, "swimming (depth {})", c.water_depth);
        let f = feet(&sim);
        worst = worst.max((f.y + 1.3 - sea_at(&sim, f)).abs());
    }
    assert!(worst <= 0.15, "follows the surface: worst {worst:.3} m");
}

#[test]
fn swimmer_dives_and_resurfaces() {
    let mut sim = world(false);
    block(&mut sim, Vec3::new(-80.0, -31.0, -80.0), Vec3::new(80.0, -30.0, 80.0));
    sea(&mut sim, SeaPreset::Calm);
    spawn(&mut sim, Vec3::new(0.0, -1.6, 0.0));
    idle(&mut sim, 50);
    step(&mut sim, Vec2::ZERO, DODGE, DODGE);
    idle(&mut sim, 60);
    let f = feet(&sim);
    let head = f.y + ch(&sim).height();
    assert_eq!(ch(&sim).traversal_state(), TraversalState::Dive);
    assert!(head < sea_at(&sim, f) - 1.0, "head under the surface: {head} vs {}", sea_at(&sim, f));
    idle(&mut sim, 6 * 60);
    let f = feet(&sim);
    assert_eq!(ch(&sim).traversal_state(), TraversalState::Swim, "back up after the dive");
    assert!((f.y + 1.3 - sea_at(&sim, f)).abs() < 0.15, "floating again: {f}");
}

#[test]
fn swimmer_climbs_out_onto_a_pier() {
    let mut sim = world(false);
    block(&mut sim, Vec3::new(-80.0, -31.0, -80.0), Vec3::new(80.0, -30.0, 80.0));
    // A pier whose deck is 1.2 m above the mean sea.
    block(&mut sim, Vec3::new(6.0, -4.0, -10.0), Vec3::new(20.0, 1.2, 10.0));
    sea(&mut sim, SeaPreset::Breezy);
    spawn(&mut sim, Vec3::new(0.0, -1.6, 0.0));
    idle(&mut sim, 30);
    let mut out = false;
    for _ in 0..600 {
        step(&mut sim, Vec2::new(1.0, 0.0), 0, 0);
        if ch(&sim).grounded && !ch(&sim).swimming && feet(&sim).x > 6.0 {
            out = true;
            break;
        }
    }
    idle(&mut sim, 10);
    let f = feet(&sim);
    assert!(out, "climbed out: {f}");
    assert!((f.y - 1.2).abs() < 0.15 && f.x > 6.0, "standing on the pier: {f}");
}

#[test]
fn parkour_down_hangs_from_the_edge_and_climbs_down() {
    let mut sim = world(true);
    block(&mut sim, Vec3::new(-5.0, 0.0, -5.0), Vec3::new(5.0, 6.0, 5.0));
    spawn(&mut sim, Vec3::new(0.0, 6.0, 4.2));
    // DODGE toward the edge (south): lower into a hang facing back at the wall.
    step(&mut sim, Vec2::new(0.0, 1.0), DODGE, DODGE);
    assert!(ch(&sim).hang.is_some(), "dropped to a hang");
    idle(&mut sim, 40);
    let h = ch(&sim).hang.expect("hanging");
    let f = feet(&sim);
    assert_eq!(ch(&sim).traversal_state(), TraversalState::Hang);
    assert!(h.normal.dot(Vec3::Z) > 0.95 && (f.y - (6.0 - 1.62)).abs() < 0.15 && f.z > 5.0, "hanging below the edge: {f}");
    // Stick away from the wall = down: climb down the face to the ground.
    let mut climbed = false;
    for _ in 0..240 {
        step(&mut sim, Vec2::new(0.0, 1.0), 0, 0);
        climbed |= ch(&sim).climb.is_some();
        if ch(&sim).grounded {
            break;
        }
    }
    let f = feet(&sim);
    assert!(climbed, "climbed down");
    assert!(ch(&sim).grounded && f.y < 0.1, "down on the ground: {f}");
}
