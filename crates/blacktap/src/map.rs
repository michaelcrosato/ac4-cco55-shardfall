//! The world map: coastlines from the islands, labels, revealed icons.

use std::sync::{Arc, OnceLock};

use glam::Vec2;
use pav_core::play::{MapData, MapIcon};
use pav_core::sim::Sim;

use crate::game::BlackTap;

/// The static layers (built once).
pub fn map(g: &BlackTap) -> Arc<MapData> {
    static MAP: OnceLock<Arc<MapData>> = OnceLock::new();
    MAP.get_or_init(|| {
        let w = g.world();
        let mut m = MapData { min: Vec2::splat(-600.0), max: Vec2::splat(600.0), ..Default::default() };
        for s in &w.shapes {
            let c = Vec2::from(s.center);
            let r = Vec2::from(s.radii);
            let ring = |k: f32| {
                (0..48)
                    .map(|i| {
                        let a = i as f32 / 48.0 * std::f32::consts::TAU;
                        c + Vec2::new(a.cos() * r.x * k, a.sin() * r.y * k)
                    })
                    .collect::<Vec<_>>()
            };
            m.fills.push((ring(1.0), "#d9c08a".into()));
            m.lines.push((ring(1.0), "#5a3c24".into(), 2.0));
            m.lines.push((ring(1.0 + s.shelf / r.min_element().max(1.0)), "#8fc1a9".into(), 1.0));
            m.labels.push((c, s.name.clone()));
        }
        Arc::new(m)
    })
    .clone()
}

/// Live icons: the player, ships, objectives.
pub fn icons(g: &BlackTap, sim: &Sim) -> Vec<MapIcon> {
    let mut out = Vec::new();
    if let Some(p) = BlackTap::player_feet(sim) {
        out.push(MapIcon { at: Vec2::new(p.x, p.z), icon: "player".into(), yaw: g.cam.yaw });
    }
    for s in g.sea.ships.iter().filter(|s| s.alive()) {
        let icon = if s.id == g.sea.mine { "ship" } else { "enemy_ship" };
        out.push(MapIcon { at: s.pos, icon: icon.into(), yaw: s.heading });
    }
    for f in &g.sea.floating {
        out.push(MapIcon { at: f.pos, icon: "flotsam".into(), yaw: 0.0 });
    }
    out
}
