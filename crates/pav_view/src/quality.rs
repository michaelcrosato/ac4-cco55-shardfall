//! Graphics quality for hook games (PLAN §9.18) and the level-of-detail helpers behind it.
//!
//! `view.quality` picks Low, Medium (the browser default) or High. High draws what the view
//! settings say; Medium and Low only ever lower them ([`ViewSettings::at_quality`]):
//!
//! | | Low | Medium | High |
//! |---|---|---|---|
//! | GI, sun shafts, haze, halos | off | GI off | as set |
//! | materials | first layer | first layer | as set |
//! | sun shadow map | 1024, 0.75 × reach | 2048, 1 × reach | `shadow_map`, `shadow_reach` |
//! | ocean grid (cells per ring) | 64 | 96 | `ocean_grid` |
//! | plants drawn | 50% | 75% | `vegetation` |
//! | terrain half resolution past | 80 m | 120 m | `terrain_lod` |
//! | distance culls (far forest) | 0.7 × (0.6 ×) | 1 × | 1 × |
//! | scenery under N px dropped | 4 | `cull_pixels` (3) | `cull_pixels` (3) |
//!
//! The render scale (Low 0.5 MP, Medium 1 MP, High full) is the app's (`pav_app::quality`):
//! tools draw at the size they are asked for.

use std::sync::Arc;

use glam::Vec3;
use pav_render::mesh::{MeshData, Vertex};

use crate::build::ViewSettings;
use crate::materials::MaterialDetail;

pav_core::choice_enum! {
    /// Hook games' graphics quality (`view.quality`).
    #[derive(Default, PartialOrd, Ord)]
    pub enum QualityChoice {
        Low => "low",
        Medium => "medium",
        #[default]
        High => "high",
    }
}

impl QualityChoice {
    pub const ALL: [QualityChoice; 3] = [QualityChoice::Low, QualityChoice::Medium, QualityChoice::High];

    /// Megapixels the app draws the scene at (None: every pixel).
    pub fn megapixels(self) -> Option<f32> {
        match self {
            QualityChoice::Low => Some(0.5),
            QualityChoice::Medium => Some(1.0),
            QualityChoice::High => None,
        }
    }
}

/// A distance setting where 0 means "never": lowered to at most `cap`.
fn lower_distance(v: f32, cap: f32) -> f32 {
    if v <= 0.0 { cap } else { v.min(cap) }
}

impl ViewSettings {
    /// The settings a hook game draws with at `self.quality` (see the module docs). Medium and
    /// Low never raise a setting.
    pub fn at_quality(&self) -> ViewSettings {
        let mut v = self.clone();
        match self.quality {
            QualityChoice::High => {}
            QualityChoice::Medium => {
                v.gi = 0.0;
                v.material_detail = v.material_detail.min(MaterialDetail::FirstLayer);
                v.shadow_map = v.shadow_map.min(2048);
                v.shadow_reach = v.shadow_reach.min(1.0);
                v.ocean_grid = v.ocean_grid.min(96);
                v.vegetation = v.vegetation.min(0.75);
                v.terrain_lod = lower_distance(v.terrain_lod, 120.0);
            }
            QualityChoice::Low => {
                v.gi = 0.0;
                v.shafts = 0.0;
                v.haze = 0.0;
                v.halos = 0.0;
                v.distortion = false;
                v.material_detail = v.material_detail.min(MaterialDetail::FirstLayer);
                v.shadow_map = v.shadow_map.min(1024);
                v.shadow_reach = v.shadow_reach.min(0.75);
                v.ocean_grid = v.ocean_grid.min(64);
                v.vegetation = v.vegetation.min(0.5);
                v.terrain_lod = lower_distance(v.terrain_lod, 80.0);
                v.cull_small *= 0.7;
                v.cull_vegetation *= 0.7;
                v.vegetation_far *= 0.6;
                v.cull_structure *= 0.7;
                v.cull_pixels = v.cull_pixels.max(4.0);
            }
        }
        v
    }
}

/// Whether a plant with this key is among the `share` (0..1) drawn: a fixed hash, so the same
/// plants stay and thinning never flickers.
pub fn keep_plant(key: u64, share: f32) -> bool {
    if share >= 1.0 {
        return true;
    }
    let mut z = key.wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^= z >> 31;
    ((z >> 40) as f32 / (1u64 << 24) as f32) < share
}

/// The share of plants drawn at `d` metres from the eye, for the vegetation distance `near`
/// (`view.cull_vegetation`) and the far forest's `far` (`view.vegetation_far`): every plant out
/// to 30% of `near`, then fewer, 30% at `near`, thinning on to 12% at `far` and none past it.
/// Grass tufts are pixels sooner: they fade out between 30% and 50% of `near`.
pub fn plant_falloff(d: f32, near: f32, far: f32, grass: bool) -> f32 {
    if near <= 0.0 {
        return 1.0;
    }
    let t = |a: f32, b: f32| ((d - a) / (b - a).max(1e-3)).clamp(0.0, 1.0);
    if grass {
        return 1.0 - t(0.3 * near, 0.5 * near);
    }
    if d <= near {
        1.0 - 0.7 * t(0.3 * near, near)
    } else if d <= far {
        0.3 - 0.18 * t(near, far)
    } else {
        0.0
    }
}

/// A plant's key: its outline group when it has its own (baked props), else where it stands.
pub fn plant_key(group: u32, at: Vec3) -> u64 {
    if group >= 2 {
        return group as u64;
    }
    let b = at.to_array().map(f32::to_bits);
    (b[0] as u64) << 32 ^ (b[2] as u64) ^ (b[1] as u64).rotate_left(17)
}

/// Depth (m) of the skirt hung from the half-resolution mesh's border.
const SKIRT: f32 = 3.0;

/// The half-resolution mesh of a terrain patch that is a height grid (islands: the first
/// `nx * nz` vertices are the grid, row by row, as `pav_core::island` builds them): every
/// other sample each way, with the full mesh's normals and colours, and a skirt hung 3 m down
/// from every border so no crack shows next to a full-resolution neighbour. None for patches
/// that are not such a grid (or too small to halve).
pub fn terrain_half_mesh(t: &pav_core::terrain::TerrainPatch) -> Option<Arc<MeshData>> {
    let g = t.heightfield.as_ref()?;
    let (nx, nz) = (g.nx, g.nz);
    if nx < 5 || nz < 5 || (nx - 1) % 2 != 0 || (nz - 1) % 2 != 0 || t.positions.len() < nx * nz {
        return None;
    }
    if t.normals.len() < nx * nz || t.colors.len() < nx * nz {
        return None;
    }
    // The grid really is the first vertices: check its corners.
    let corner = |i: usize, k: usize| {
        let p = t.positions[k * nx + i];
        let want = (g.origin.x + i as f32 * g.cell, g.origin.y + k as f32 * g.cell);
        (p[0] - want.0).abs() < 1e-3 && (p[2] - want.1).abs() < 1e-3
    };
    if !(corner(0, 0) && corner(nx - 1, 0) && corner(0, nz - 1) && corner(nx - 1, nz - 1)) {
        return None;
    }
    let (cx, cz) = ((nx - 1) / 2 + 1, (nz - 1) / 2 + 1);
    let vertex = |i: usize, k: usize| {
        let v = k * nx + i;
        let c = t.colors[v];
        Vertex { pos: t.positions[v], normal: t.normals[v], uv: [0.0, 0.0], color: [c[0], c[1], c[2], 1.0] }
    };
    let mut vertices = Vec::with_capacity(cx * cz + 4 * 2 * cx.max(cz));
    for k in 0..cz {
        for i in 0..cx {
            vertices.push(vertex(i * 2, k * 2));
        }
    }
    let at = |i: usize, k: usize| (k * cx + i) as u32;
    let mut indices = Vec::with_capacity((cx - 1) * (cz - 1) * 6 + 4 * 6 * cx.max(cz));
    for k in 0..cz - 1 {
        for i in 0..cx - 1 {
            // The full mesh's diagonal (rapier's split), so the two meshes agree where they can.
            let (a, b, c, d) = (at(i, k), at(i + 1, k), at(i, k + 1), at(i + 1, k + 1));
            indices.extend_from_slice(&[a, c, b, c, d, b]);
        }
    }
    let borders: [(Vec<(usize, usize)>, [f32; 3]); 4] = [
        ((0..cx).map(|i| (i, 0)).collect(), [0.0, 0.0, -1.0]),
        ((0..cx).map(|i| (i, cz - 1)).collect(), [0.0, 0.0, 1.0]),
        ((0..cz).map(|k| (0, k)).collect(), [-1.0, 0.0, 0.0]),
        ((0..cz).map(|k| (cx - 1, k)).collect(), [1.0, 0.0, 0.0]),
    ];
    for (edge, normal) in borders {
        let first = vertices.len() as u32;
        for &(i, k) in &edge {
            let v = vertices[at(i, k) as usize];
            let c = [v.color[0] * 0.7, v.color[1] * 0.7, v.color[2] * 0.7, 1.0];
            vertices.push(Vertex { normal, color: c, ..v });
            vertices.push(Vertex { pos: [v.pos[0], v.pos[1] - SKIRT, v.pos[2]], normal, color: c, ..v });
        }
        for e in 0..edge.len() as u32 - 1 {
            let (a, b) = (first + e * 2, first + e * 2 + 2);
            indices.extend_from_slice(&[a, a + 1, b + 1, a, b + 1, b]);
        }
    }
    Some(Arc::new(MeshData { vertices, indices }))
}

/// Distance (m) from `eye` to the box `lo`..`hi`.
pub fn box_distance(eye: Vec3, lo: Vec3, hi: Vec3) -> f32 {
    (eye.clamp(lo, hi) - eye).length()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn island_patch() -> pav_core::terrain::TerrainPatch {
        use pav_core::island::{Heightfield, IslandDef};
        let def: IslandDef = toml::from_str(
            r#"
            name = "test"
            seed = 3
            outline = [[0, 0], [60, 0], [60, 60], [0, 60]]
            peaks = [{ at = [30, 30], height = 12, radius = 30 }]
            "#,
        )
        .expect("island");
        let field: Heightfield = pav_core::island::generate(&def);
        let patches = pav_core::island::terrain_patches(&[&field]);
        patches.into_iter().max_by_key(|p| (p.max.y * 100.0) as i64).expect("a patch")
    }

    #[test]
    fn the_half_mesh_has_a_quarter_of_the_grid_and_meets_its_corners() {
        let t = island_patch();
        let g = t.heightfield.as_ref().unwrap();
        let half = terrain_half_mesh(&t).expect("islands halve");
        let (cx, cz) = ((g.nx - 1) / 2 + 1, (g.nz - 1) / 2 + 1);
        assert!(half.vertices.len() < t.positions.len() / 2, "{} of {}", half.vertices.len(), t.positions.len());
        assert!(half.indices.len() * 3 < t.indices.len() * 2, "{} of {}", half.indices.len(), t.indices.len());
        assert!(half.indices.iter().all(|&i| (i as usize) < half.vertices.len()));
        // Shared samples sit exactly where the full mesh has them (no cracks at even vertices).
        for k in 0..cz {
            for i in 0..cx {
                assert_eq!(half.vertices[k * cx + i].pos, t.positions[(k * 2) * g.nx + i * 2]);
            }
        }
        // The skirt reaches below every border sample.
        let low = half.vertices.iter().map(|v| v.pos[1]).fold(f32::MAX, f32::min);
        let border_low = (0..cx).map(|i| half.vertices[i].pos[1]).fold(f32::MAX, f32::min);
        assert!(low <= border_low - SKIRT + 1e-4);
    }

    #[test]
    fn meshes_that_are_not_grids_keep_full_resolution() {
        let mut t = island_patch();
        t.heightfield = None;
        assert!(terrain_half_mesh(&t).is_none());
    }

    #[test]
    fn presets_only_lower_settings_and_thin_plants_steadily() {
        let high = ViewSettings::default();
        assert_eq!(high.quality, QualityChoice::High);
        let same = high.at_quality();
        assert_eq!((same.ocean_grid, same.shadow_map, same.vegetation), (high.ocean_grid, high.shadow_map, high.vegetation));
        let low = ViewSettings { quality: QualityChoice::Low, gi: 0.8, haze: 0.5, ..Default::default() }.at_quality();
        assert_eq!((low.gi, low.haze, low.shafts), (0.0, 0.0, 0.0));
        assert_eq!((low.ocean_grid, low.shadow_map, low.vegetation), (64, 1024, 0.5));
        assert!(low.terrain_lod > 0.0 && low.terrain_lod <= 80.0);
        let medium = ViewSettings { quality: QualityChoice::Medium, ocean_grid: 48, ..Default::default() }.at_quality();
        assert_eq!(medium.ocean_grid, 48, "never raised");
        let kept = (0..10_000u64).filter(|&k| keep_plant(k, 0.5)).count();
        assert!((4_700..5_300).contains(&kept), "{kept}");
        assert!((0..1000u64).all(|k| keep_plant(k, 0.5) == keep_plant(k, 0.5)));
        assert!((0..1000u64).all(|k| keep_plant(k, 1.0)));
        assert_eq!(plant_falloff(10.0, 180.0, 700.0, false), 1.0);
        assert!((plant_falloff(180.0, 180.0, 700.0, false) - 0.3).abs() < 1e-5);
        assert!((plant_falloff(700.0, 180.0, 700.0, false) - 0.12).abs() < 1e-5);
        assert_eq!(plant_falloff(701.0, 180.0, 700.0, false), 0.0);
        assert_eq!(plant_falloff(300.0, 180.0, 0.0, false), 0.0, "no far forest");
        assert_eq!(plant_falloff(100.0, 180.0, 700.0, true), 0.0);
        assert_eq!(plant_falloff(500.0, 0.0, 0.0, true), 1.0, "no vegetation distance: no thinning");
    }
}
