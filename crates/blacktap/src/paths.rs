//! Walking paths for the people on the islands: a `pav_core::nav25` grid per island (roofs,
//! walls and streets as separate layers), built once per level file right after its geometry,
//! before anything that moves is spawned, so the grid is the same whenever the level loads.

use std::collections::BTreeMap;
use std::sync::Mutex;

use glam::{Vec2, Vec3};
use pav_core::nav25::{FlowField, NavGrid25};
use pav_core::sim::Sim;
use std::sync::Arc;

use crate::stealth::Npc;

/// Grid cell (m): fine enough for alleys between the town's houses.
const CELL: f32 = 0.75;
/// Probes start this high (above the keep's roof and the hills).
const TOP: f32 = 60.0;
/// A path is asked again after this long, or when the goal moves.
const REPATH: f32 = 3.0;

static GRIDS: Mutex<BTreeMap<String, Vec<NavGrid25>>> = Mutex::new(BTreeMap::new());

/// Builds the level's grids (once per geometry file) from the islands' areas in `sim`.
pub fn build(sim: &Sim, geometry: &str) {
    let mut grids = GRIDS.lock().unwrap_or_else(|p| p.into_inner());
    if grids.contains_key(geometry) {
        return;
    }
    let pre = crate::island::prebuilt_file(geometry);
    let built =
        pre.fields.iter().map(|f| NavGrid25::build(sim, f.origin, f.max(), CELL, TOP)).filter(|g| g.node_count() > 0).collect();
    grids.insert(geometry.to_string(), built);
}

/// The grid under `p`, if the level has one there.
fn grid_at(geometry: &str, p: Vec3) -> Option<NavGrid25> {
    let grids = GRIDS.lock().unwrap_or_else(|p| p.into_inner());
    grids.get(geometry)?.iter().find(|g| g.cell_at(p).is_some()).cloned()
}

/// A path from `from` to `to` (waypoints after the start), if both are on one grid.
pub fn path(geometry: &str, from: Vec3, to: Vec3) -> Option<Vec<Vec3>> {
    grid_at(geometry, from)?.path(from, to, 8000)
}

/// Movement toward `to` for a person, round walls and houses: short hops go straight, longer
/// ones follow a path kept on the person and asked again when the goal moves.
pub fn walk(geometry: &str, n: &mut Npc, feet: Vec3, to: Vec3, speed: f32, dt: f32) -> (Vec2, Option<f32>) {
    n.path_t -= dt;
    let flat = |v: Vec3| Vec2::new(v.x, v.z);
    if flat(to - feet).length() < 4.0 {
        n.path.clear();
        n.path_to = None;
        return crate::stealth::walk_to(feet, to, speed);
    }
    let stale = n.path_to.is_none_or(|p| flat(p - to).length() > 2.0) || n.path_t <= 0.0;
    if stale {
        n.path_t = REPATH;
        n.path_to = Some(to);
        n.path = path(geometry, feet, to).unwrap_or_default();
    }
    while n.path.first().is_some_and(|w| flat(*w - feet).length() < 0.9) {
        n.path.remove(0);
    }
    let next = n.path.first().copied().unwrap_or(to);
    crate::stealth::walk_to(feet, next, speed)
}

/// Flow fields toward goals (snapped to 1.5 m), cached: each is a pure function of the level and
/// its goal, so whoever asks gets the same field and stays deterministic.
static FIELDS: Mutex<BTreeMap<(String, [i32; 3]), Arc<FlowField>>> = Mutex::new(BTreeMap::new());

/// Which way to walk from `from` toward `goal` round whatever is in the way (None off the grids
/// or out of reach).
pub fn flow_dir(geometry: &str, from: Vec3, goal: Vec3) -> Option<Vec2> {
    let key =
        (geometry.to_string(), [(goal.x / 1.5).round() as i32, (goal.y / 1.5).round() as i32, (goal.z / 1.5).round() as i32]);
    let cached = FIELDS.lock().unwrap_or_else(|p| p.into_inner()).get(&key).cloned();
    let field = match cached {
        Some(f) => f,
        None => {
            let grid = grid_at(geometry, goal).or_else(|| grid_at(geometry, from))?;
            let snapped = Vec3::new(key.1[0] as f32, key.1[1] as f32, key.1[2] as f32) * 1.5;
            let f = Arc::new(grid.flow_field(Vec3::new(snapped.x, goal.y, snapped.z), 500.0));
            let mut all = FIELDS.lock().unwrap_or_else(|p| p.into_inner());
            if all.len() >= 48 {
                all.clear();
            }
            all.insert(key, f.clone());
            f
        }
    };
    field.dir(from)
}

/// The way to walk from `from` toward `to` (a unit x, z direction): straight when nothing stands
/// in the way, else round it by the level's flow field. Only goals at least `min` metres away
/// detour (close ones are reached by the last steps); people's capsules don't count as walls.
pub fn steer(sim: &Sim, geometry: &str, from: Vec3, to: Vec3, min: f32) -> Vec2 {
    let d = Vec2::new(to.x - from.x, to.z - from.z);
    let l = d.length();
    let straight = d / l.max(1e-3);
    if l < min.max(1.5) {
        return straight;
    }
    let dir = Vec3::new(straight.x, 0.0, straight.y);
    let reach = (l - 2.0).clamp(0.5, 8.0);
    let blocked = [0.5f32, 1.1].iter().any(|h| {
        sim.raycast(from + Vec3::Y * *h, dir, reach, None).is_some_and(|hit| hit.normal.y.abs() < 0.5 && hit.entity.is_none())
    });
    if !blocked {
        return straight;
    }
    flow_dir(geometry, from, to).unwrap_or(straight)
}
