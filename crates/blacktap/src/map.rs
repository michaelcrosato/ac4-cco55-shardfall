//! The world map: coastlines and hills traced from the island heightfields, roads, building
//! footprints and place names (static, built once per level), and live icons — the player,
//! ships, flotsam, objectives, the fort and the tavern, lookouts, and the collectibles a
//! survey has revealed.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use glam::{Vec2, Vec3};
use pav_core::play::{MapData, MapIcon, MarkerKind, WorldMarker};
use pav_core::sim::Sim;

use crate::game::BlackTap;
use crate::world::World;

/// Contour heights drawn inland (m): each closed ring tints the land a little darker.
const HILLS: [f32; 2] = [12.0, 26.0];

/// The static layers for the current level (built once per geometry file).
pub fn map(g: &BlackTap) -> Arc<MapData> {
    static CACHE: Mutex<BTreeMap<String, Arc<MapData>>> = Mutex::new(BTreeMap::new());
    let w = g.world();
    let mut c = CACHE.lock().unwrap_or_else(|p| p.into_inner());
    c.entry(w.geometry.clone()).or_insert_with(|| Arc::new(build(&w))).clone()
}

fn build(w: &World) -> MapData {
    let half = w.bounds.hard.max(50.0);
    let mut m = MapData { min: Vec2::splat(-half), max: Vec2::splat(half), ..Default::default() };
    let pre = crate::island::prebuilt_file(&w.geometry);
    if pre.fields.is_empty() {
        // No generated terrain: the level's island ellipses.
        for s in &w.shapes {
            let (c, r) = (Vec2::from(s.center), Vec2::from(s.radii));
            let ring: Vec<Vec2> = (0..=48)
                .map(|i| {
                    let a = i as f32 / 48.0 * std::f32::consts::TAU;
                    c + Vec2::new(a.cos() * r.x, a.sin() * r.y)
                })
                .collect();
            m.lines.push((ring, "#5a3c24".into(), 2.5));
        }
    }
    for f in &pre.fields {
        for ring in pav_core::island::contours(f, 0.05, 2, 0.8) {
            m.lines.push((ring, "#5a3c24".into(), 2.5));
        }
        for h in HILLS {
            for ring in pav_core::island::contours(f, h, 2, 1.0) {
                m.lines.push((ring, "#8a6a44".into(), 1.0));
            }
        }
        for r in &f.def.roads {
            m.lines.push((r.points.iter().map(|p| Vec2::from(*p)).collect(), "#9c7448".into(), 1.6));
        }
    }
    for fp in &pre.structures.footprints {
        let size = fp.max - fp.min;
        if size.x * size.y < 6.0 {
            continue;
        }
        let ring = vec![fp.min, Vec2::new(fp.max.x, fp.min.y), fp.max, Vec2::new(fp.min.x, fp.max.y)];
        m.fills.push((ring, "#6b4a30".into()));
    }
    if w.map_labels.is_empty() {
        m.labels.extend(w.shapes.iter().map(|s| (Vec2::from(s.center), s.name.clone())));
    } else {
        m.labels.extend(w.map_labels.iter().map(|l| (Vec2::from(l.at), l.text.clone())));
    }
    m
}

/// Live icons: the player, ships, flotsam, objectives, landmarks, lookouts and what the
/// surveys revealed.
pub fn icons(g: &BlackTap, sim: &Sim, markers: &[WorldMarker]) -> Vec<MapIcon> {
    let w = g.world();
    let xz = |p: Vec3| Vec2::new(p.x, p.z);
    let mut out = Vec::new();
    for (name, icon) in [("fort_keep", "fort"), ("tavern", "tavern")] {
        if let Some(p) = w.marker(name) {
            out.push(MapIcon { at: xz(p), icon: icon.into(), yaw: 0.0 });
        }
    }
    for p in &w.perches {
        let done = g.mission.flag(&format!("surveyed_{}", p.name)) || g.stealth.revealed.contains(&p.name);
        out.push(MapIcon { at: xz(Vec3::from(p.at)), icon: if done { "perch_done" } else { "perch" }.into(), yaw: 0.0 });
    }
    for c in &w.collectibles {
        if g.stealth.collected.contains(&c.name) || !revealed(g, &w, Vec3::from(c.at)) {
            continue;
        }
        let icon = match c.kind.as_str() {
            "keg_tap" => "tap",
            k @ ("sheet" | "bottle" | "chest") => k,
            _ => continue,
        };
        out.push(MapIcon { at: xz(Vec3::from(c.at)), icon: icon.into(), yaw: 0.0 });
    }
    for f in &g.sea.floating {
        out.push(MapIcon { at: f.pos, icon: "flotsam".into(), yaw: 0.0 });
    }
    for s in g.sea.ships.iter().filter(|s| s.alive()) {
        let icon = if s.id == g.sea.mine { "ship" } else { "enemy_ship" };
        out.push(MapIcon { at: s.pos, icon: icon.into(), yaw: s.heading });
    }
    for mk in markers.iter().filter(|m| m.kind == MarkerKind::Objective) {
        out.push(MapIcon { at: xz(mk.pos), icon: "objective".into(), yaw: 0.0 });
    }
    if let Some(p) = BlackTap::player_feet(sim) {
        out.push(MapIcon { at: xz(p), icon: "player".into(), yaw: g.cam.yaw });
    }
    out
}

/// Whether a surveyed lookout covers `p`.
fn revealed(g: &BlackTap, w: &World, p: Vec3) -> bool {
    w.perches.iter().any(|q| {
        let done = g.mission.flag(&format!("surveyed_{}", q.name)) || g.stealth.revealed.contains(&q.name);
        done && Vec2::new(q.at[0] - p.x, q.at[2] - p.z).length() < q.reveals.max(150.0)
    })
}
