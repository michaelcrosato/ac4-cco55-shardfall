//! Hook games' quality presets, camera-fitted shadows and far plane reach the render scene
//! (PLAN §9.18).

use std::sync::Arc;

use glam::Vec3;
use pav_core::Sim;
use pav_core::ocean::{SeaPreset, SeaState};
use pav_core::play::{Atmosphere, CameraHint, PlayFrame};
use pav_view::quality::QualityChoice;
use pav_view::{CameraRig, ViewBuilder, ViewSettings};

fn play_frame() -> pav_core::RenderFrame {
    let mut sim = Sim::new("empty", 1).unwrap();
    let mut f = sim.frame();
    let target = Vec3::new(100.0, 2.0, 50.0);
    let play = PlayFrame {
        camera: Some(CameraHint::orbit(target, 0.0, 20.0, 18.0, 55.0)),
        atmosphere: Some(Atmosphere { sea: Some(SeaState::preset(SeaPreset::Breezy, 30.0)), ..Default::default() }),
        ..Default::default()
    };
    f.play = Some(Arc::new(play));
    f
}

#[test]
fn presets_reach_the_scene() {
    let f = play_frame();
    let rig = CameraRig::default();
    let mut b = ViewBuilder::new();
    let radius = Atmosphere::default().shadow_radius;
    let mut last_radius = f32::MAX;
    for (q, cells, map) in [(QualityChoice::High, 128, 4096), (QualityChoice::Medium, 96, 2048), (QualityChoice::Low, 64, 1024)] {
        let view = ViewSettings { quality: q, gi: 0.6, haze: 0.4, ..Default::default() };
        let s = b.build(&f, &f, 1.0, &rig, 16.0 / 9.0, &view, Vec3::ZERO);
        assert_eq!(s.ocean.as_ref().map(|o| o.cells), Some(cells), "{q:?}");
        assert_eq!(s.sun.shadow_map, map, "{q:?}");
        assert!(s.sun.shadow_radius < last_radius, "{q:?} shadows reach less far");
        last_radius = s.sun.shadow_radius;
        assert_eq!(s.post.gi > 0.0, q == QualityChoice::High, "{q:?}: GI only on High");
        assert_eq!(s.post.haze > 0.0, q != QualityChoice::Low, "{q:?}: no haze on Low");
        assert!(s.frustum_cull && s.cull_pixels > 0.0 && s.cull_small > 0.0);
        // The far plane reaches the horizon (the game's 1,800 m; the Tab is 1,200 m across).
        assert!(pav_view::atmos::far_plane(&s.camera) > 1700.0);
    }
    assert!((ViewSettings::default().shadow_reach * radius - 98.0).abs() < 1.0, "High: ~100 m of shadows");
}

#[test]
fn the_shadow_square_follows_the_camera() {
    let f = play_frame();
    let hint = f.play.as_ref().unwrap().camera.unwrap();
    let s = ViewBuilder::new().build(&f, &f, 1.0, &CameraRig::default(), 16.0 / 9.0, &ViewSettings::default(), Vec3::ZERO);
    let c = s.sun.shadow_center;
    let ahead = Vec3::new(hint.target.x - hint.eye.x, 0.0, hint.target.z - hint.eye.z).normalize();
    let lead = (c - hint.target).dot(ahead);
    assert!(lead > 5.0 && lead <= s.sun.shadow_radius / 3.0 + 1e-3, "centred ahead of the target: {lead}");
    assert!((c - hint.target - ahead * lead).length() < 1e-3, "on the view line");
}
