//! The game's atmosphere reaches the render scene (sun, sky, fog, sea, seabed, rain, grade,
//! wind), and the ocean the GPU draws moves exactly like the simulation's sea.

use std::sync::Arc;

use glam::{Vec2, Vec3};
use pav_core::Color;
use pav_core::ocean::{SeaPreset, SeaState, Seabed};
use pav_core::play::Atmosphere;
use pav_render::scene::{Ocean, Scene};
use pav_render::sea::grid_center;
use pav_view::atmos::{self, FromSea};

fn seas() -> Vec<SeaState> {
    let mut v: Vec<SeaState> = SeaPreset::ALL.iter().map(|&p| SeaState::preset(p, 37.0)).collect();
    v.push(SeaState::blend(&SeaState::preset(SeaPreset::Rough, 37.0), &SeaState::preset(SeaPreset::Storm, 37.0), 0.4));
    // Steep enough that the Gerstner Q normalisation kicks in.
    let mut steep = SeaState::preset(SeaPreset::Storm, -120.0);
    for w in &mut steep.waves {
        w.steepness = 1.0;
        w.amplitude *= 1.5;
    }
    v.push(steep);
    v
}

/// `Ocean::displacement` is the vertex shader's formula on the packed uniforms (phases taken
/// from the snapped grid centre); it must land where `SeaState::displacement` puts the water.
#[test]
fn the_gpu_ocean_moves_like_the_simulation() {
    let mut worst = 0.0f32;
    for sea in seas() {
        for t in [0.0f32, 3.7, 41.25, 96.5] {
            let ocean = Ocean::from_sea(&sea, t);
            for eye in [Vec3::new(0.0, 12.0, 0.0), Vec3::new(-75.0, 2.0, 140.0), Vec3::new(203.0, 60.0, -18.0)] {
                let center = grid_center(eye);
                for i in 0..40 {
                    let a = i as f32 * 2.399;
                    let r = (i as f32 * 3.1) % 90.0;
                    let p0 = Vec2::new(eye.x, eye.z) + Vec2::new(a.cos(), a.sin()) * r;
                    let gpu = ocean.displacement(p0, center);
                    let cpu = sea.displacement(p0, t);
                    worst = worst.max((gpu - cpu).abs().max_element());
                }
            }
        }
    }
    assert!(worst < 1e-4, "GPU and CPU water differ by {worst} m");
}

#[test]
fn from_sea_packs_the_waves_and_normalises_q_like_the_cpu() {
    for sea in seas() {
        let o = Ocean::from_sea(&sea, 5.0);
        let total: f32 = sea.waves.iter().map(|w| w.steepness * w.amplitude * std::f32::consts::TAU / w.wavelength).sum();
        let scale = if total > 0.95 { 0.95 / total } else { 1.0 };
        for (g, c) in o.waves.iter().zip(&sea.waves) {
            assert_eq!(g.dir, c.dir);
            assert_eq!(g.amplitude, c.amplitude);
            assert_eq!(g.phase, c.phase);
            assert!((g.q - c.steepness * scale).abs() < 1e-6);
            assert!((g.k - std::f32::consts::TAU / c.wavelength).abs() < 1e-6);
            assert!((g.omega - (9.81 * g.k).sqrt()).abs() < 1e-6);
        }
        let qka: f32 = o.waves.iter().map(|w| w.q * w.k * w.amplitude).sum();
        assert!(qka <= 0.95 + 1e-5, "crests never loop ({qka})");
        assert_eq!(o.time, 5.0);
        assert_eq!(o.detail, sea.detail);
        assert_eq!(o.foam_amount, sea.foam);
    }
}

fn shelf() -> Arc<Seabed> {
    let n = 32;
    let heights = (0..n * n).map(|i| -((i % n) as f32) * 0.5).collect();
    Arc::new(Seabed { origin: Vec2::new(-16.0, -16.0), cell: 1.0, width: n, depth: n, heights, deep: -40.0 })
}

fn applied(a: &Atmosphere) -> Scene {
    let mut scene = Scene::default();
    atmos::apply(a, Vec3::new(3.0, 12.0, 4.0), Vec3::new(10.0, 1.0, -60.0), &mut scene);
    scene
}

#[test]
fn the_atmosphere_reaches_the_scene() {
    let bed = shelf();
    let a = Atmosphere {
        sun_azimuth: 250.0,
        sun_elevation: 12.0,
        sun_color: Color::hex("#ffb878"),
        sun_intensity: 1.3,
        sky_zenith: Color::hex("#3d6fae"),
        sky_horizon: Color::hex("#f6cf98"),
        fog_color: Color::hex("#f0cfa4"),
        fog_start: 260.0,
        fog_end: 1700.0,
        cloud_cover: 0.4,
        rain: 0.3,
        wind: Vec2::new(5.0, -3.0),
        exposure: 1.2,
        saturation: 1.1,
        contrast: 1.05,
        bloom: 0.45,
        shadow_radius: 90.0,
        gull: 0.5,
        sea: Some(SeaState::preset(SeaPreset::Breezy, 30.0)),
        sea_time: 12.5,
        seabed: Some(bed.clone()),
        ..Default::default()
    };
    let s = applied(&a);
    let lin = |c: Color| Vec3::from(c.0);
    // The sun shines from where the atmosphere puts it, in its colour.
    assert!(s.sun.direction.distance(a.sun_direction()) < 1e-5);
    let c = s.sun.color / s.sun.color.max_element();
    let want = lin(a.sun_color) / lin(a.sun_color).max_element();
    assert!(c.distance(want) < 0.05, "sun colour {c} vs {want}");
    assert_eq!(s.sun.shadow_center, Vec3::new(10.0, 1.0, -60.0));
    assert_eq!(s.sun.shadow_radius, 90.0);
    // The sky.
    let sky = s.sky.expect("a sky");
    assert_eq!(sky.zenith, lin(a.sky_zenith));
    assert_eq!(sky.horizon, lin(a.sky_horizon));
    assert!(sky.sun_dir.distance(a.sun_direction()) < 1e-5);
    assert!((sky.clouds.cover - 0.4).abs() < 1e-6);
    assert_eq!(s.clear_color, lin(a.sky_horizon));
    // Fog along the view ray.
    assert!(s.fog.enabled && s.fog_ray.is_some());
    assert_eq!((s.fog.start, s.fog.end), (260.0, 1700.0));
    assert!(s.fog.color.distance(lin(a.fog_color)) < 1e-6);
    // The sea: the simulation's waves at the simulation's time, over its seabed.
    let o = s.ocean.as_ref().expect("an ocean");
    let want = Ocean::from_sea(a.sea.as_ref().unwrap(), a.sea_time);
    assert_eq!(o.waves, want.waves);
    assert_eq!(o.time, 12.5);
    let map = o.seabed.as_ref().expect("a seabed");
    assert_eq!((map.width, map.depth, map.cell, map.origin, map.deep), (bed.width, bed.depth, bed.cell, bed.origin, bed.deep));
    assert_eq!(map.heights, bed.heights);
    // The same seabed gives the same copy (the renderer uploads it once).
    let again = applied(&a);
    assert!(Arc::ptr_eq(map, again.ocean.as_ref().unwrap().seabed.as_ref().unwrap()));
    // Rain, grade, Gull Sight and one wind.
    let rain = s.rain.expect("rain");
    assert!((rain.amount - 0.3).abs() < 1e-6 && rain.wind == a.wind);
    assert_eq!((s.post.bloom, s.post.exposure, s.post.saturation, s.filter.contrast), (0.45, 1.2, 1.1, 1.05));
    assert_eq!(s.gull, 0.5);
    assert!(s.wind.direction.distance(a.wind.normalize()) < 1e-6 && s.wind.strength > 0.5);
    // A pure function of its inputs.
    let b = applied(&a);
    assert_eq!(format!("{:?}", b.sky), format!("{:?}", s.sky));
    assert_eq!(format!("{:?}", b.ocean.map(|o| o.waves)), format!("{:?}", s.ocean.map(|o| o.waves)));
}

#[test]
fn no_sea_no_ocean_and_dry_weather_no_rain() {
    let s = applied(&Atmosphere::default());
    assert!(s.ocean.is_none() && s.rain.is_none());
    assert!(s.sky.is_some());
}

#[test]
fn a_setting_sun_lights_from_above_the_horizon_and_lightning_flashes() {
    let low = Atmosphere { sun_elevation: -2.0, ..Default::default() };
    let s = applied(&low);
    assert!(s.sun.direction.y < -0.05, "light still comes from above: {}", s.sun.direction);
    assert!(s.sky.unwrap().sun_dir.distance(low.sun_direction()) < 1e-5, "the sky draws the sun where it is");
    let calm = applied(&Atmosphere::default());
    let flash = applied(&Atmosphere { lightning: 1.0, ..Default::default() });
    assert!(flash.sun.color.length() > calm.sun.color.length() + 0.5);
    assert!(flash.sky.unwrap().flash > 0.99);
}

/// The whole frame on the GPU (lavapipe in CI): sky over the horizon, water below it, the ocean
/// within its vertex budget.
#[test]
fn sea_and_sky_render() {
    use pav_render::{Renderer, capture, gpu};
    let hd = match gpu::Headless::new() {
        Ok(h) => h,
        Err(e) => {
            eprintln!("sea and sky render skipped: no GPU adapter ({e:#})");
            return;
        }
    };
    let mut r = Renderer::new(&hd.device, &hd.queue);
    let a = Atmosphere { sea: Some(SeaState::preset(SeaPreset::Breezy, 0.0)), sea_time: 3.0, ..Default::default() };
    let eye = Vec3::new(0.0, 12.0, 0.0);
    let target = Vec3::new(0.0, 8.0, -100.0);
    let mut scene = Scene::default();
    let fwd = (target - eye).normalize();
    scene.camera.view = glam::camera::rh::view::look_to_mat4(eye, fwd, Vec3::Y);
    scene.camera.proj = glam::camera::rh::proj::directx::perspective(55f32.to_radians(), 1.0, 0.1, 1800.0);
    scene.camera.eye = eye;
    scene.camera.forward = fwd;
    atmos::apply(&a, eye, target, &mut scene);
    let size = 64;
    let px = capture::render_to_rgba(&mut r, &scene, size, size).expect("capture");
    let at = |x: u32, y: u32| {
        let i = ((y * size + x) * 4) as usize;
        Vec3::new(px[i] as f32, px[i + 1] as f32, px[i + 2] as f32)
    };
    let sky = at(32, 2);
    let sea = at(32, 60);
    assert!(sky.z > 120.0 && sky.length() > sea.length(), "sky {sky} over sea {sea}");
    assert!(sea.z > sea.x, "the sea is blue: {sea}");
    assert!(r.stats.ocean_vertices > 10_000 && r.stats.ocean_vertices <= 120_000, "{}", r.stats.ocean_vertices);
}
