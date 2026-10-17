//! Effects the game draws itself every frame (PLAN §9.17), from its state and the clock:
//! - at sea: deck fires (flames, wind-drifted smoke, heat haze), sinking ships' bubbles and the
//!   wreckage they leave, bow spray, cannonball streaks, waterspouts' rotating columns;
//! - weather: rain splashing round the player, lightning bolts (`lightning` also drives the
//!   sky's flash and, in `game.rs`, the thunder cues and rumble);
//! - moments: the Leap of Spirits' trail, collectibles' sparkle, the Gull Sight pulse ring, the
//!   gulls that sweep past a survey;
//! - ambient life: gull flocks over the coasts, Gull Rock, the harbour and the ship, leaves
//!   falling from the jungle by the camera, fireflies over the grass at dusk, torches on the
//!   fort and by the town's doors at dusk.
//!
//! Everything here is a pure function of the game's state, its clock, the world seed, the
//! camera and the level's static geometry: drawing never changes the simulation. One-off
//! moments are `SimEvent::Cue`s instead (`games/blacktap/fx.toml`). Counts stay modest for the
//! browser: about 250 primitives and 30 emitters at most.

use std::collections::BTreeMap;
use std::f32::consts::TAU;
use std::sync::{Arc, Mutex};

use glam::{Mat3, Quat, Vec2, Vec3};
use pav_core::frame::SimEvent;
use pav_core::fxdef::{DistortDef, EmitterDef};
use pav_core::play::{Atmosphere, DrawItem, DrawList};
use pav_core::sim::Sim;
use pav_core::{Color, Look, Shape};

use crate::game::{BlackTap, Body, Mode};

// Draw-list emitter keys: a kind in the high bits, an index below.
const K_FIRE: u64 = 1 << 40;
const K_SMOKE: u64 = 2 << 40;
const K_BUBBLES: u64 = 3 << 40;
const K_SPRAY: u64 = 4 << 40;
const K_SPOUT: u64 = 5 << 40;
const K_SPOUT_MIST: u64 = 6 << 40;
const K_RAIN: u64 = 7 << 40;
const K_LEAP: u64 = 8 << 40;
const K_LEAP_FEATHERS: u64 = 9 << 40;
const K_SPARKLE: u64 = 10 << 40;
const K_FIREFLY: u64 = 11 << 40;
const K_TORCH: u64 = 12 << 40;
const K_WRECK: u64 = 13 << 40;

/// Ambient life farther than this from the eye isn't drawn (m).
const GULL_RANGE: f32 = 380.0;

fn hash(a: u64, b: u64) -> u64 {
    let mut h = a.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ b.wrapping_add(0x632B_E59B_D9B4_E019);
    h ^= h >> 30;
    h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h ^= h >> 27;
    h = h.wrapping_mul(0x94D0_49BB_1331_11EB);
    h ^ (h >> 31)
}

/// A number in 0..1 from two integers (stable across runs and machines).
fn h01(a: u64, b: u64) -> f32 {
    (hash(a, b) >> 40) as f32 / (1u64 << 24) as f32
}

fn emitter(preset: &str, rate: f32, color: &str, size: f32, speed: f32, life: f32, area: Vec3) -> EmitterDef {
    EmitterDef { preset: preset.into(), rate, color: color.into(), size, speed, life, area, offset: Vec3::ZERO, ground: None }
}

fn prim(out: &mut DrawList, shape: Shape, pos: Vec3, rot: Quat, color: &str, emissive: f32, look: Look) {
    out.push(DrawItem::Prim { shape, pos, rot, color: Color::hex(color), emissive, look });
}

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// How far into dusk the light is (0 by day, 1 at sunset and after): torches and fireflies.
pub fn dusk(a: &Atmosphere) -> f32 {
    let low_sun = smoothstep(13.0, 6.0, a.sun_elevation);
    // Not in the white mist or under the storm's rain.
    let mist = smoothstep(0.85, 0.95, a.ambient);
    low_sun * (1.0 - mist) * (1.0 - smoothstep(0.3, 0.7, a.rain))
}

// ---------------------------------------------------------------- lightning

/// A lightning strike in progress.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Strike {
    /// Which strike (its slot on the clock): seeds the bolt's shape.
    pub k: u64,
    /// Where it hits the sea or land.
    pub pos: Vec3,
    /// Seconds since it struck.
    pub age: f32,
    /// Flash brightness now (0..1): a bright stroke, a flicker, a return stroke, a fade.
    pub brightness: f32,
}

/// Seconds per lightning slot: at most one strike in each.
const STRIKE_SLOT: f32 = 5.5;
const STRIKE_LEN: f32 = 0.45;

/// The strike lighting the storm at time `t` (heavy rain only), around `around` (the player),
/// mostly in front of a camera turned to `facing` (degrees clockwise from north, as the game's
/// camera yaw): a pure function of these, so the sky, the bolt, the thunder and the rumble agree
/// in the window, the tools and replays.
pub fn lightning(seed: u64, rain: f32, around: Vec3, facing: f32, t: f32) -> Option<Strike> {
    if rain <= 0.5 || t < 0.0 {
        return None;
    }
    let k = (t / STRIKE_SLOT).floor() as u64;
    let h = hash(seed ^ 0x11_6874, k);
    // About one slot in four stays dark.
    if h01(h, 1) < 0.25 {
        return None;
    }
    let start = k as f32 * STRIKE_SLOT + h01(h, 2) * (STRIKE_SLOT - STRIKE_LEN - 0.1);
    let age = t - start;
    if !(0.0..STRIKE_LEN).contains(&age) {
        return None;
    }
    let brightness = if age < 0.06 {
        1.0
    } else if age < 0.11 {
        0.25
    } else if age < 0.2 {
        0.85
    } else {
        (1.0 - (age - 0.2) / (STRIKE_LEN - 0.2)).max(0.0) * 0.6
    };
    // In view: 25°–80° to either side of where the camera looks (the middle is often the
    // player's own sails); now and then anywhere.
    let side = if h01(h, 6) < 0.5 { -1.0 } else { 1.0 };
    let off = if h01(h, 5) < 0.8 { side * (25.0 + h01(h, 3) * 55.0) } else { h01(h, 3) * 360.0 };
    let a = (facing + off).to_radians();
    let d = 110.0 + h01(h, 4) * 230.0;
    let pos = Vec3::new(around.x + a.sin() * d, 0.0, around.z - a.cos() * d);
    Some(Strike { k, pos, age, brightness })
}

fn bolt(out: &mut DrawList, s: &Strike) {
    if s.brightness < 0.2 {
        return;
    }
    let col = Color::hex("#e6edff");
    let glow = 10.0 * s.brightness;
    let top = 240.0;
    let n = 14;
    let mut p = s.pos + Vec3::Y * top;
    let mut pts = vec![p];
    for i in 1..=n {
        let y = top * (1.0 - i as f32 / n as f32);
        let j = Vec2::new(h01(s.k, i * 2) - 0.5, h01(s.k, i * 2 + 1) - 0.5) * 22.0;
        let lean = (s.pos - Vec3::new(p.x, 0.0, p.z)) * 0.15;
        p = Vec3::new(p.x + j.x + lean.x, y, p.z + j.y + lean.z);
        pts.push(p);
    }
    *pts.last_mut().unwrap() = s.pos;
    for w in pts.windows(2) {
        out.push(DrawItem::Line { a: w[0], b: w[1], radius: 0.7, color: col, emissive: glow });
    }
    // Two forks off the stroke.
    for (b, from) in [(0u64, 4usize), (1, 8)] {
        let mut q = pts[from];
        let dir = Vec2::new(h01(s.k, 100 + b) - 0.5, h01(s.k, 200 + b) - 0.5).normalize_or(Vec2::X);
        for i in 0..5u64 {
            let j = Vec2::new(h01(s.k, 300 + b * 10 + i) - 0.5, h01(s.k, 400 + b * 10 + i) - 0.5) * 8.0;
            let next = Vec3::new(q.x + dir.x * 9.0 + j.x, q.y - 13.0, q.z + dir.y * 9.0 + j.y);
            out.push(DrawItem::Line { a: q, b: next, radius: 0.35, color: col, emissive: glow * 0.7 });
            q = next;
        }
    }
    out.push(DrawItem::Light { pos: s.pos + Vec3::Y * 30.0, color: col, intensity: 30.0 * s.brightness, radius: 260.0 });
}

// ---------------------------------------------------------------- gulls

/// A gull: a white body and two wings, each an inner panel and a grey, dark-tipped outer one
/// bent further (the gull's "M"), flapping by `flap` (radians up), heading along `fwd`.
fn gull(out: &mut DrawList, at: Vec3, fwd: Vec3, flap: f32, size: f32) {
    let fwd = fwd.normalize_or(Vec3::Z);
    let right = Vec3::Y.cross(fwd).normalize_or(Vec3::X);
    let up = fwd.cross(right);
    let base = Quat::from_mat3(&Mat3::from_cols(right, up, fwd));
    prim(
        out,
        Shape::Capsule { radius: 0.11 * size, half_height: 0.2 * size },
        at,
        Quat::from_rotation_arc(Vec3::Y, fwd),
        "#f6f6f2",
        0.12,
        Look::Lit,
    );
    for side in [1.0f32, -1.0] {
        let inner = Quat::from_axis_angle(fwd, side * flap);
        let half = Vec3::new(0.2 * size, 0.012 * size, 0.1 * size);
        let root = at + up * 0.04 * size;
        let mid = root + inner * (right * side * 0.4 * size);
        prim(out, Shape::Box { half }, root + inner * (right * side * 0.2 * size), inner * base, "#eceeee", 0.08, Look::Lit);
        // The outer wing droops on the downstroke and lifts less on the up.
        let outer = Quat::from_axis_angle(fwd, side * (flap * 0.6 - 0.25));
        let half = Vec3::new(0.22 * size, 0.01 * size, 0.07 * size);
        prim(out, Shape::Box { half }, mid + outer * (right * side * 0.22 * size), outer * base, "#8d9296", 0.04, Look::Lit);
    }
}

/// A gull's wing angle: bursts of flapping between glides.
fn flap(t: f32, phase: f32) -> f32 {
    if (t * 0.45 + phase).sin() > -0.3 { 0.18 + 0.55 * (t * 9.0 + phase * 7.0).sin() } else { 0.14 }
}

/// Where a flock's path is at time `t`.
#[derive(Clone, Copy)]
enum Path {
    /// Round an ellipse (centre, radii) at a height, `speed` m/s (negative: the other way).
    Ellipse { center: Vec2, radii: Vec2, height: f32, speed: f32, phase: f32 },
    /// Wheeling over the player's ship.
    Ship { radius: f32, height: f32 },
}

struct Flock {
    path: Path,
    gulls: u32,
    seed: u64,
}

fn path_at(g: &BlackTap, p: &Path, t: f32) -> Vec3 {
    match *p {
        Path::Ellipse { center, radii, height, speed, phase } => {
            let r = (radii.x + radii.y) * 0.5;
            let a = phase + t * speed / r.max(1.0);
            Vec3::new(center.x + a.cos() * radii.x, height + (t * 0.3 + phase).sin() * 3.0, center.y + a.sin() * radii.y)
        }
        Path::Ship { radius, height } => {
            let Some(s) = g.sea.my_ship() else { return Vec3::new(0.0, -100.0, 0.0) };
            let a = t * 0.42;
            s.world(Vec3::new(a.cos() * radius, height + (t * 0.7).sin() * 2.0, a.sin() * radius * 1.3))
        }
    }
}

fn flocks(g: &BlackTap) -> Vec<Flock> {
    let w = g.world();
    let mut out = Vec::new();
    for (i, s) in w.shapes.iter().enumerate() {
        let c = Vec2::from(s.center);
        let r = Vec2::from(s.radii);
        let i = i as u64;
        let small = r.x.max(r.y) < 40.0;
        // Small rocks: tight wheels above them (two flocks); islands: along the coast.
        let (radii, height, n, count) = if small {
            (r * 1.7 + Vec2::splat(8.0), s.height + 14.0, 8, 2)
        } else {
            (r * 0.98, s.height.max(8.0) * 0.5 + 16.0, 7, if r.x.max(r.y) > 100.0 { 2 } else { 1 })
        };
        for k in 0..count {
            let seed = hash(i, k);
            let dir = if k % 2 == 0 { 1.0 } else { -1.0 };
            out.push(Flock {
                path: Path::Ellipse {
                    center: c,
                    radii: radii * (1.0 - 0.18 * k as f32),
                    height: height + k as f32 * 5.0,
                    speed: dir * (6.0 + h01(seed, 1) * 2.0),
                    phase: h01(seed, 2) * TAU,
                },
                gulls: n,
                seed,
            });
        }
    }
    // The harbour.
    if let Some(d) = w.marker("dryport") {
        out.push(Flock {
            path: Path::Ellipse {
                center: Vec2::new(d.x, d.z + 30.0),
                radii: Vec2::new(28.0, 20.0),
                height: 20.0,
                speed: 6.5,
                phase: 0.0,
            },
            gulls: 7,
            seed: 77,
        });
    }
    // A few follow the Black Tap when she is near land.
    if let Some(s) = g.sea.my_ship() {
        let near_land = w.shapes.iter().any(|sh| {
            let d = (s.pos - Vec2::from(sh.center)) / (Vec2::from(sh.radii) + Vec2::splat(260.0));
            d.length() < 1.0
        });
        if near_land && s.alive() {
            out.push(Flock { path: Path::Ship { radius: 13.0, height: 17.0 }, gulls: 5, seed: 91 });
        }
    }
    out
}

fn draw_flock(g: &BlackTap, out: &mut DrawList, f: &Flock, t: f32, eye: Vec3, budget: &mut u32) {
    let head = path_at(g, &f.path, t);
    if head.distance(eye) > GULL_RANGE || head.y < -50.0 {
        return;
    }
    for i in 0..f.gulls {
        if *budget == 0 {
            return;
        }
        *budget -= 1;
        let a = h01(f.seed, 10 + i as u64) * TAU;
        let b = h01(f.seed, 20 + i as u64) * TAU;
        let lag = i as f32 * 0.55 + h01(f.seed, 30 + i as u64) * 0.4;
        // Followers trail the leader along the path and wheel about it (boids, by the clock).
        let at = |t: f32| {
            let c = path_at(g, &f.path, t - lag);
            let r = 2.5 + 1.5 * (t * 0.5 + a).sin();
            c + Vec3::new((t * 0.8 + a).cos() * r, (t * 0.9 + b).sin() * 1.3, (t * 0.7 + b).sin() * r)
        };
        let p = at(t);
        // Never one in the camera's face.
        if p.distance(eye) < 4.0 {
            continue;
        }
        let fwd = at(t + 0.1) - p;
        gull(out, p, Vec3::new(fwd.x, fwd.y * 0.3, fwd.z), flap(t, a), 1.2);
    }
}

// ---------------------------------------------------------------- static spots

/// Torches and the lighthouse lamp from the level's structures (cached per geometry file).
struct Spots {
    /// (base of the pole, pole height).
    torches: Vec<(Vec3, f32)>,
    lamps: Vec<Vec3>,
    /// Trees that drop leaves: (base, canopy height).
    trees: Vec<(Vec3, f32)>,
}

fn spots(g: &BlackTap) -> Arc<Spots> {
    static CACHE: Mutex<BTreeMap<String, Arc<Spots>>> = Mutex::new(BTreeMap::new());
    let w = g.world();
    let mut cache = CACHE.lock().unwrap_or_else(|p| p.into_inner());
    if let Some(s) = cache.get(&w.geometry) {
        return s.clone();
    }
    let p = crate::island::prebuilt_file(&w.geometry);
    let m = &p.structures.markers;
    let mut torches = Vec::new();
    let mut lamps = Vec::new();
    for (name, &pos) in m {
        let (owner, what) = name.rsplit_once('.').unwrap_or((name.as_str(), ""));
        match what {
            // On the fort's walkways and bastions (standing torches).
            "walk" if owner.contains("wall") => torches.push((pos + Vec3::Y * 0.02, 1.7)),
            "top" if owner.contains("bastion") => torches.push((pos + Vec3::Y * 0.02, 1.7)),
            // Harbour moles: a beacon.
            "beacon" => torches.push((pos - Vec3::Y * 1.2, 1.4)),
            // Either side of the gate.
            "outside" if owner.contains("gate") => {
                torches.push((pos + Vec3::new(-4.2, 0.0, 0.0), 2.2));
                torches.push((pos + Vec3::new(4.2, 0.0, 0.0), 2.2));
            }
            // Beside doors (off to the side along the facade, when the roof says which way).
            "door" => {
                let side = m
                    .get(&format!("{owner}.roof"))
                    .map(|r| {
                        let n = Vec2::new(pos.x - r.x, pos.z - r.z).normalize_or(Vec2::Y);
                        Vec3::new(-n.y, 0.0, n.x) * 1.4 - Vec3::new(n.x, 0.0, n.y) * 0.4
                    })
                    .unwrap_or(Vec3::ZERO);
                torches.push((pos + side, 2.2));
            }
            "light" if owner.contains("lighthouse") => lamps.push(pos),
            _ => {}
        }
    }
    let trees = p
        .vegetation
        .props
        .iter()
        .filter_map(|v| {
            let h = if v.asset.contains("jungle_tree") {
                9.0
            } else if v.asset.contains("palm") {
                7.5
            } else {
                return None;
            };
            Some((v.pos, h * v.scale.max(0.3)))
        })
        .collect();
    let s = Arc::new(Spots { torches, lamps, trees });
    cache.insert(w.geometry.clone(), s.clone());
    s
}

/// A torch's flicker (0.7..1), smooth and different for each.
fn flicker(t: f32, i: f32) -> f32 {
    let n = (t * 11.0 + i).sin() * 0.5 + (t * 6.7 + i * 1.9).sin() * 0.3 + (t * 23.0 + i * 0.7).sin() * 0.2;
    0.85 + 0.15 * n
}

// ---------------------------------------------------------------- cues each tick

/// The moments effects need as cues, from the rules' `post` (they read the state and change
/// nothing but the feel): dust from the player's footfalls on sand and dirt, thunder with each
/// lightning strike (a rumble too), fires crackling aboard burning ships, the bow's spray.
pub fn tick_cues(g: &mut BlackTap, sim: &Sim, dt: f32, events: &mut Vec<SimEvent>) {
    let feet = BlackTap::player_feet(sim);
    let w = g.world();
    // Footfalls: sand on the beach, dust inland; nothing on stone, decks or in the water.
    if let (Some(f), Body::Foot) = (feet, g.body) {
        let sprinting = g.land.sprinting;
        let mut puffs = Vec::new();
        for e in events.iter() {
            let SimEvent::Step { pos } = e else { continue };
            if pos.distance(f) > 1.5 || pos.y < 0.4 {
                continue;
            }
            let xz = Vec2::new(pos.x, pos.z);
            if (pos.y - w.ground(xz)).abs() > 0.35 {
                continue;
            }
            use pav_core::island::Ground;
            let cue = match surface(&w, xz) {
                Some(Ground::Sand | Ground::WetSand) => "footfall_sand",
                Some(Ground::Dirt) => "footfall_dust",
                _ => continue,
            };
            puffs.push(SimEvent::Cue { cue: cue.into(), pos: *pos, gain: if sprinting { 1.4 } else { 1.0 }, pitch: 1.0 });
        }
        events.extend(puffs);
    }
    // Lightning: the crack and the thunder where it lands, and a rumble that fades with distance.
    let rain = crate::present::atmosphere(g).rain;
    let around = feet.unwrap_or_else(|| g.sea.my_ship().map(|s| Vec3::new(s.pos.x, 0.0, s.pos.y)).unwrap_or_default());
    if let Some(s) = lightning(g.seed, rain, around, g.cam.yaw, g.time)
        && s.age < dt
    {
        events.push(SimEvent::cue("lightning_crack", s.pos + Vec3::Y * 25.0));
        events.push(SimEvent::cue("thunder", s.pos));
        let k = 1.0 - ((s.pos.distance(around) - 110.0) / 230.0).clamp(0.0, 1.0);
        if g.mode != Mode::Title {
            g.shake(0.04 + 0.08 * k, (0.25 + 0.35 * k, 0.1, 300 + (250.0 * k) as u32));
        }
    }
    // Burning ships crackle; the Black Tap's bow throws spray when she runs fast.
    let tick = sim.state.tick;
    let c = g.content();
    for s in g.sea.ships.iter().filter(|s| !s.sunk) {
        let Some(cl) = c.classes.get(&s.class) else { continue };
        if s.fire > 0.0 && (tick + s.id as u64 * 17).is_multiple_of(80) {
            events.push(SimEvent::cue("fire_crackle", s.world(Vec3::new(0.0, cl.freeboard + 0.6, cl.length * 0.1))));
        }
        if s.id == g.sea.mine && g.body == Body::Helm && g.mode != Mode::Title && s.speed > 5.5 && tick.is_multiple_of(110) {
            events.push(SimEvent::Cue {
                cue: "bow_spray".into(),
                pos: s.world(Vec3::new(0.0, 0.6, cl.length * 0.5)),
                gain: (s.speed / 9.0).clamp(0.4, 1.0),
                pitch: 1.0,
            });
        }
    }
}

/// What the terrain is made of at `p` (the highest island there; dirt on its roads).
fn surface(w: &crate::world::World, p: Vec2) -> Option<pav_core::island::Ground> {
    let pre = crate::island::prebuilt_file(&w.geometry);
    let f = pre.fields.iter().filter(|f| f.contains(p)).max_by(|a, b| a.height(p).total_cmp(&b.height(p)))?;
    Some(if f.on_road(p) { pav_core::island::Ground::Dirt } else { f.ground(p) })
}

// ---------------------------------------------------------------- the frame

/// Draws this frame's effects and ambient life (see the module notes).
pub fn draw(g: &BlackTap, sim: &Sim, out: &mut DrawList) {
    let t = g.time;
    let a = crate::present::atmosphere(g);
    let wind = Vec3::new(a.wind.x, 0.0, a.wind.y);
    let cam = crate::present::camera(g, sim);
    let eye = cam.map(|c| c.eye).unwrap_or_default();
    let target = cam.map(|c| c.target).unwrap_or_default();
    let feet = BlackTap::player_feet(sim);
    sea_effects(g, out, eye, wind);
    weather(g, out, &a, feet, sim);
    moments(g, sim, out, feet, eye);
    // Ambient life (the gulls shelter from the storm).
    let mut budget = 30;
    if a.rain < 0.3 {
        for f in flocks(g) {
            draw_flock(g, out, &f, t, eye, &mut budget);
        }
    }
    if g.mode == Mode::Title {
        return;
    }
    let spots = spots(g);
    leaves(g, out, &spots, target, wind);
    let d = dusk(&a);
    if d > 0.05 {
        fireflies(g, out, target, d);
        torches(out, &spots, target, t, d);
    }
}

fn sea_effects(g: &BlackTap, out: &mut DrawList, eye: Vec3, wind: Vec3) {
    let c = g.content();
    let ocean = g.ocean();
    for s in &g.sea.ships {
        let Some(cl) = c.classes.get(&s.class) else { continue };
        let id = s.id as u64;
        let near = Vec3::new(s.pos.x, 0.0, s.pos.y).distance(eye) < 420.0;
        if s.sunk || !near {
            continue;
        }
        // Deck fire: flames, dark smoke leaning downwind, a haze over it.
        if s.fire > 0.0 {
            let p = s.world(Vec3::new(0.0, cl.freeboard + 0.4, cl.length * 0.1));
            let k = (s.fire / 3.0).clamp(0.3, 1.0);
            out.push(DrawItem::Emitter {
                key: K_FIRE | id,
                def: emitter("fire", 70.0 * k, "", 2.6, 1.6, 1.3, Vec3::new(cl.beam * 0.25, 0.2, cl.length * 0.08)),
                pos: p,
                drift: wind * 0.12,
            });
            out.push(DrawItem::Emitter {
                key: K_SMOKE | id,
                def: emitter("smoke", 30.0 * k, "#4d4743", 7.0, 2.4, 1.8, Vec3::new(1.4, 0.4, 1.4)),
                pos: p + Vec3::Y * 1.5,
                drift: wind * 0.6,
            });
            out.push(DrawItem::Distortion {
                def: DistortDef { kind: "haze".into(), radius: 3.0, strength: 0.35, period: 1.5, offset: Vec3::Y * 3.0 },
                pos: p,
            });
        }
        // Going down: bubbles and white water along the hull.
        if s.sinking > 0.0 {
            let p = Vec3::new(s.pos.x, ocean.height(s.pos) - 0.2, s.pos.y);
            out.push(DrawItem::Emitter {
                key: K_BUBBLES | id,
                def: emitter("bubbles", 70.0, "#e8f6ff", 4.0, 1.6, 0.5, Vec3::new(cl.beam * 0.6, 0.2, cl.length * 0.35)),
                pos: p,
                drift: Vec3::ZERO,
            });
            wreckage(out, &ocean, s.pos, id, s.sinking * 8.0, s.sinking, g.time, wind);
        }
        // Bow spray when under way.
        if s.alive() && s.speed > 3.0 {
            let k = ((s.speed - 3.0) / 6.0).clamp(0.0, 1.0);
            let bow = s.world(Vec3::new(0.0, 0.5, cl.length * 0.5));
            let fwd = s.forward();
            out.push(DrawItem::Emitter {
                key: K_SPRAY | id,
                def: emitter(
                    "fountain",
                    40.0 + 90.0 * k,
                    "#f2f8ff",
                    3.6,
                    0.55 + 0.3 * k,
                    0.8,
                    Vec3::new(cl.beam * 0.35, 0.1, 0.5),
                ),
                pos: bow,
                drift: Vec3::new(fwd.x, 0.0, fwd.y) * s.speed * 0.7,
            });
        }
    }
    // Wreckage where ships went down.
    for w in &g.sea.wrecks {
        let age = g.sea.clock - w.at;
        if Vec3::new(w.pos.x, 0.0, w.pos.y).distance(eye) < 420.0 {
            if age < 14.0 {
                out.push(DrawItem::Emitter {
                    key: K_WRECK | w.id as u64,
                    def: emitter("bubbles", 40.0 * (1.0 - age / 14.0), "#e8f6ff", 3.0, 1.4, 0.5, Vec3::new(5.0, 0.2, 5.0)),
                    pos: Vec3::new(w.pos.x, ocean.height(w.pos) - 0.2, w.pos.y),
                    drift: Vec3::ZERO,
                });
            }
            wreckage(out, &ocean, w.pos, w.id as u64, 8.0 + age, 1.0, g.time, wind);
        }
    }
    // Cannonballs: a pale streak behind each.
    for s in g.sea.shots.shots.iter().filter(|s| s.delay <= 0.0) {
        let speed = s.vel.length();
        if speed < 1.0 {
            continue;
        }
        let len = (speed * 0.07).clamp(1.0, 6.0);
        let back = s.pos - s.vel / speed * len;
        out.push(DrawItem::Line { a: back, b: s.pos, radius: 0.09, color: Color::hex("#ece6d8"), emissive: 0.8 });
        if s.kind == pav_kit::ballistics::ShotKind::Mortar {
            out.push(DrawItem::Light { pos: s.pos, color: Color::hex("#ff9a4a"), intensity: 1.5, radius: 8.0 });
        }
    }
    // Waterspouts: a column of spray turning round the funnel, mist at the foot, a haze.
    for (i, sp) in g.sea.spouts.iter().enumerate() {
        let base = Vec3::new(sp.pos.x, ocean.height(sp.pos), sp.pos.y);
        if base.distance(eye) > 500.0 {
            continue;
        }
        let i = i as u64;
        let fade = (sp.life / 3.0).clamp(0.0, 1.0);
        for k in 0..48u64 {
            let rise = (g.time * 5.0 + h01(i, k) * 44.0) % 44.0;
            let r = sp.radius * (0.18 + rise / 44.0 * 0.45) * fade;
            let ang = g.time * (2.8 - rise * 0.02) + h01(i, k + 100) * TAU;
            let p = base + Vec3::new(ang.cos() * r, rise, ang.sin() * r);
            let size = 0.3 + h01(i, k + 200) * 0.5;
            prim(out, Shape::Sphere { radius: size * fade }, p, Quat::IDENTITY, "#e6edf1", 0.15, Look::Lit);
        }
        out.push(DrawItem::Emitter {
            key: K_SPOUT | i,
            def: emitter("fountain", 90.0 * fade, "#eef4f8", 2.2, 1.1, 1.2, Vec3::new(sp.radius * 0.5, 0.2, sp.radius * 0.5)),
            pos: base + Vec3::Y * 0.3,
            drift: Vec3::ZERO,
        });
        out.push(DrawItem::Emitter {
            key: K_SPOUT_MIST | i,
            def: emitter("steam", 30.0 * fade, "#dfe6ea", 9.0, 2.5, 1.6, Vec3::new(sp.radius * 0.6, 0.5, sp.radius * 0.6)),
            pos: base + Vec3::Y * 1.0,
            drift: wind * 0.3,
        });
        out.push(DrawItem::Distortion {
            def: DistortDef { kind: "haze".into(), radius: sp.radius * 1.1, strength: 0.4, period: 1.5, offset: Vec3::Y * 16.0 },
            pos: base,
        });
    }
}

/// Planks, a barrel and a spar floating where a ship went down, spreading, then sinking away.
#[allow(clippy::too_many_arguments)]
fn wreckage(out: &mut DrawList, ocean: &pav_core::ocean::Ocean, at: Vec2, id: u64, age: f32, appear: f32, time: f32, wind: Vec3) {
    let pieces = (9.0 * appear.clamp(0.0, 1.0)).ceil() as u64;
    let gone = smoothstep(55.0, 75.0, age);
    if gone >= 1.0 {
        return;
    }
    for i in 0..pieces {
        let a = h01(id, i) * TAU;
        let r = 1.5 + h01(id, i + 20) * 4.0 + age.min(40.0) * 0.12;
        let xz = at + Vec2::new(a.cos(), a.sin()) * r + Vec2::new(wind.x, wind.z) * age.min(60.0) * 0.04;
        let y = ocean.height(xz) + 0.05 - gone * 1.2;
        let bob =
            Quat::from_rotation_x((time * 1.3 + i as f32).sin() * 0.12) * Quat::from_rotation_z((time + i as f32).cos() * 0.1);
        let rot = Quat::from_rotation_y(a * 2.0 + time * 0.05) * bob;
        let p = Vec3::new(xz.x, y, xz.y);
        match i % 5 {
            0 => prim(
                out,
                Shape::Cylinder { radius: 0.32, half_height: 0.42 },
                p,
                rot * Quat::from_rotation_z(1.5),
                "#6b4a2f",
                0.0,
                Look::Lit,
            ),
            1 => prim(out, Shape::Box { half: Vec3::new(2.4, 0.09, 0.12) }, p, rot, "#5a3e28", 0.0, Look::Lit),
            _ => prim(out, Shape::Box { half: Vec3::new(1.0, 0.07, 0.22) }, p, rot, "#8b6b4a", 0.0, Look::Lit),
        }
    }
}

fn weather(g: &BlackTap, out: &mut DrawList, a: &Atmosphere, feet: Option<Vec3>, sim: &Sim) {
    let _ = sim;
    let Some(feet) = feet else { return };
    // Rain bouncing round the player (the streaks and the rings on the sea are the sky's).
    if a.rain > 0.2 {
        out.push(DrawItem::Emitter {
            key: K_RAIN,
            def: emitter("fountain", 220.0 * a.rain, "#dfe8f2", 1.3, 0.24, 0.35, Vec3::new(7.0, 0.0, 7.0)),
            pos: feet + Vec3::Y * 0.05,
            drift: Vec3::ZERO,
        });
    }
    if let Some(s) = lightning(g.seed, a.rain, feet, g.cam.yaw, g.time) {
        bolt(out, &s);
    }
}

fn moments(g: &BlackTap, sim: &Sim, out: &mut DrawList, feet: Option<Vec3>, eye: Vec3) {
    let t = g.time;
    // The Leap of Spirits: a trail of light and feathers, and the arc drawn behind.
    if let (Some(l), Some(p)) = (&g.stealth.leap, feet) {
        out.push(DrawItem::Emitter {
            key: K_LEAP,
            def: emitter("magic", 45.0, "#c9a24a", 0.9, 0.15, 1.6, Vec3::splat(0.3)),
            pos: p + Vec3::Y * 1.0,
            drift: Vec3::ZERO,
        });
        out.push(DrawItem::Emitter {
            key: K_LEAP_FEATHERS,
            def: emitter("confetti", 18.0, "#f4f1e8", 2.0, 0.2, 0.8, Vec3::splat(0.35)),
            pos: p + Vec3::Y * 1.2,
            drift: Vec3::ZERO,
        });
        let k = (l.t / l.len).clamp(0.0, 1.0);
        let arc = |k: f32| {
            let h = (k * std::f32::consts::PI).sin() * (2.5 + l.from.distance(l.to) * 0.06);
            l.from.lerp(l.to, k * k * (3.0 - 2.0 * k) * 0.3 + k * 0.7) + Vec3::Y * (h + 1.0)
        };
        // A short ribbon behind, thinning toward its tail.
        let k0 = (k - 0.12).max(0.0);
        for i in 0..6 {
            let (u, v) = (k0 + (k - k0) * i as f32 / 6.0, k0 + (k - k0) * (i + 1) as f32 / 6.0);
            out.push(DrawItem::Line {
                a: arc(u),
                b: arc(v),
                radius: 0.015 + 0.006 * i as f32,
                color: Color::hex("#f2dc9a"),
                emissive: 0.9,
            });
        }
    }
    // Collectibles near the camera sparkle.
    let w = g.world();
    let target = feet.unwrap_or(eye);
    let mut shown = 0;
    for (i, c) in w.collectibles.iter().enumerate() {
        if g.stealth.collected.contains(&c.name) || shown >= 6 {
            continue;
        }
        let at = Vec3::from(c.at) + Vec3::Y * 0.6;
        if at.distance(target) > 70.0 {
            continue;
        }
        shown += 1;
        out.push(DrawItem::Emitter {
            key: K_SPARKLE | i as u64,
            def: emitter("magic", 7.0, "#ffd27a", 0.9, 0.35, 1.3, Vec3::splat(0.35)),
            pos: at,
            drift: Vec3::ZERO,
        });
        // A twinkle: a four-point star that grows and shrinks (emissive under 0.5: a prim
        // brighter than that also lights its surroundings).
        let s = 0.5 + 0.5 * (t * 5.0 + i as f32 * 1.7).sin();
        let star = at + Vec3::Y * 0.55;
        let face = Quat::from_rotation_y(t * 1.5 + i as f32);
        for r in [0.0f32, std::f32::consts::FRAC_PI_2] {
            prim(
                out,
                Shape::Box { half: Vec3::new(0.02, 0.12 + 0.2 * s, 0.02) },
                star,
                face * Quat::from_rotation_z(r + 0.785),
                "#fff1c0",
                0.45,
                Look::Unlit,
            );
        }
    }
    // Gull Sight: a ring sweeping out from the player while it is on.
    if g.gull {
        if let Some(p) = feet {
            for k in 0..2 {
                let ph = ((t / 2.2) + k as f32 * 0.5).fract();
                out.push(DrawItem::Decal {
                    center: p + Vec3::Y * 0.08,
                    radius: 1.5 + 34.0 * ph,
                    ring: 0.95 - 0.02 * ph,
                    color: Color::hex("#9fd3ff"),
                    alpha: (1.0 - ph) * 0.55,
                });
            }
        }
    }
    // The survey: gulls sweep round the perch against the camera's turn, then away.
    if let (Some((_, st)), Some(p)) = (&g.stealth.survey, feet) {
        let k = st / crate::traverse::SURVEY_SECONDS;
        let away = smoothstep(0.75, 1.0, k);
        for i in 0..7u64 {
            let r = 3.5 + h01(5, i) * 4.0 + away * 30.0;
            let a = -st * 1.7 + i as f32 * 0.9;
            let h = 0.8 + h01(6, i) * 3.5 + away * 8.0;
            let at = |a: f32| p + Vec3::new(a.cos() * r, h, a.sin() * r);
            let pos = at(a);
            let fwd = at(a - 0.05) - pos;
            gull(out, pos, fwd, flap(st * 2.0, i as f32), 1.0);
        }
    }
    let _ = sim;
}

fn leaves(g: &BlackTap, out: &mut DrawList, spots: &Spots, target: Vec3, wind: Vec3) {
    if g.body != Body::Foot {
        return;
    }
    let mut near: Vec<(f32, usize)> = spots
        .trees
        .iter()
        .enumerate()
        .map(|(i, (p, _))| (Vec2::new(p.x - target.x, p.z - target.z).length_squared(), i))
        .filter(|(d, _)| *d < 45.0 * 45.0)
        .collect();
    near.sort_by(|a, b| a.0.total_cmp(&b.0));
    let t = g.time;
    for &(_, i) in near.iter().take(8) {
        let (base, h) = spots.trees[i];
        for k in 0..4u64 {
            let seed = hash(i as u64, k);
            let period = 6.0 + h01(seed, 1) * 3.0;
            let tau = ((t + h01(seed, 2) * period) / period).fract();
            let ang = h01(seed, 3) * TAU;
            let r = 1.0 + h01(seed, 4) * 2.5;
            let fall = tau * (h - 0.3);
            let sway = (tau * TAU * 1.6 + ang).sin() * 0.9;
            let side = Vec3::new(ang.cos(), 0.0, ang.sin());
            let p = base
                + Vec3::new(ang.cos() * r, h - fall, ang.sin() * r)
                + Vec3::new(-side.z, 0.0, side.x) * sway
                + wind * (tau * period * 0.12);
            let fade = 1.0 - smoothstep(0.85, 1.0, tau);
            let rot = Quat::from_rotation_y(t * 1.1 + ang) * Quat::from_rotation_x(sway * 0.8) * Quat::from_rotation_z(tau * 9.0);
            let color = if h01(seed, 5) < 0.35 { "#d2c25a" } else { "#7fb04a" };
            prim(out, Shape::Box { half: Vec3::new(0.17, 0.012, 0.09) * fade }, p, rot, color, 0.2, Look::Lit);
        }
    }
}

fn fireflies(g: &BlackTap, out: &mut DrawList, target: Vec3, d: f32) {
    let w = g.world();
    let mut near: Vec<(f32, usize)> = w
        .zones
        .iter()
        .enumerate()
        .filter(|(_, z)| z.name.contains("grass"))
        .map(|(i, z)| (z.center().distance(target), i))
        .filter(|(dist, _)| *dist < 70.0)
        .collect();
    near.sort_by(|a, b| a.0.total_cmp(&b.0));
    for &(_, i) in near.iter().take(4) {
        let z = &w.zones[i];
        let half = ((z.max - z.min) * 0.5).min(Vec3::splat(9.0));
        out.push(DrawItem::Emitter {
            key: K_FIREFLY | i as u64,
            def: emitter("fireflies", 9.0 * d, "#d8ff6a", 1.6, 1.0, 1.0, Vec3::new(half.x, 0.7, half.z)),
            pos: Vec3::new(z.center().x, z.min.y + 1.4, z.center().z),
            drift: Vec3::ZERO,
        });
    }
}

fn torches(out: &mut DrawList, spots: &Spots, target: Vec3, t: f32, d: f32) {
    let mut near: Vec<(f32, usize)> =
        spots.torches.iter().enumerate().map(|(i, (p, _))| (p.distance(target), i)).filter(|(dist, _)| *dist < 140.0).collect();
    near.sort_by(|a, b| a.0.total_cmp(&b.0));
    for (n, &(_, i)) in near.iter().take(24).enumerate() {
        let (base, h) = spots.torches[i];
        prim(
            out,
            Shape::Cylinder { radius: 0.05, half_height: h * 0.5 },
            base + Vec3::Y * h * 0.5,
            Quat::IDENTITY,
            "#3a2a1c",
            0.0,
            Look::Lit,
        );
        let top = base + Vec3::Y * (h + 0.12);
        prim(out, Shape::Cylinder { radius: 0.13, half_height: 0.1 }, top, Quat::IDENTITY, "#2a2420", 0.0, Look::Lit);
        // Flames, and light, on the nearest only (the renderer keeps 64 lights).
        if n < 10 {
            let f = flicker(t, i as f32 * 2.3);
            prim(
                out,
                Shape::Sphere { radius: 0.12 + 0.03 * f },
                top + Vec3::Y * 0.18,
                Quat::IDENTITY,
                "#ffc46a",
                0.45,
                Look::Unlit,
            );
            out.push(DrawItem::Emitter {
                key: K_TORCH | i as u64,
                def: emitter("fire", 18.0, "", 0.9, 0.7, 0.9, Vec3::new(0.08, 0.04, 0.08)),
                pos: top + Vec3::Y * 0.12,
                drift: Vec3::ZERO,
            });
            out.push(DrawItem::Light {
                pos: top + Vec3::Y * 0.5,
                color: Color::hex("#ff9f4a"),
                intensity: 1.4 * d * f,
                radius: 8.0,
            });
        }
    }
    for &lamp in &spots.lamps {
        if lamp.distance(target) < 900.0 {
            prim(out, Shape::Sphere { radius: 0.9 }, lamp, Quat::IDENTITY, "#fff0c0", 0.45, Look::Unlit);
            out.push(DrawItem::Light { pos: lamp, color: Color::hex("#ffe2a0"), intensity: 6.0 * d, radius: 60.0 });
        }
    }
}
