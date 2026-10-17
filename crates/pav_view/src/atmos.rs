//! The game's [`Atmosphere`] (sky, light, sea, weather) applied to a render scene: the sun and
//! ambient light, the sky with its sun disc and clouds, fog along the view ray, the ocean with
//! the simulation's own waves, rain, lightning, the colour grade, the sun's shadow square and
//! the wind. [`apply`] depends only on its inputs, so a capture of the same frame always shows
//! the same sky.

use std::sync::{Arc, Mutex, Weak};

use glam::{Vec2, Vec3};
use pav_core::color::Color;
use pav_core::ocean::{SeaState, Seabed};
use pav_core::play::Atmosphere;
use pav_render::scene::{self as rs, Scene};

/// Sunlight is held at least this high (degrees) so a low or setting sun never lights the world
/// from below; the sky still draws the sun where it really is.
const MIN_LIGHT_ELEVATION: f32 = 4.0;

/// Builds the renderer's ocean from the simulation's sea state: the GPU then moves the water
/// exactly as `SeaState::displacement` does (`Ocean::displacement` is the shader's formula).
pub trait FromSea {
    fn from_sea(sea: &SeaState, time: f32) -> Self;
}

impl FromSea for rs::Ocean {
    fn from_sea(sea: &SeaState, time: f32) -> Self {
        // `Wave::k`, `Wave::omega` and `SeaState::q`, step for step (same f32 operations).
        const G: f32 = 9.81;
        let k = |w: &pav_core::ocean::Wave| std::f32::consts::TAU / w.wavelength.max(0.1);
        let total: f32 = sea.waves.iter().map(|w| w.steepness * w.amplitude * k(w)).sum();
        let scale = if total > 0.95 { 0.95 / total } else { 1.0 };
        let waves = sea.waves.map(|w| rs::OceanWave {
            dir: w.dir,
            wavelength: w.wavelength.max(0.1),
            k: k(&w),
            omega: (G * k(&w)).sqrt(),
            amplitude: w.amplitude,
            q: w.steepness * scale,
            phase: w.phase,
        });
        rs::Ocean { waves, time, detail: sea.detail, foam_amount: sea.foam, ..Default::default() }
    }
}

fn lin(c: Color) -> Vec3 {
    Vec3::from(c.0)
}

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The renderer's copy of a seabed, made once per `Arc` (the renderer re-uploads its texture
/// only when the `Arc` it is given changes, so the same seabed must give the same copy).
pub fn seabed_map(bed: &Arc<Seabed>) -> Arc<rs::SeabedMap> {
    static LAST: Mutex<Option<(Weak<Seabed>, Arc<rs::SeabedMap>)>> = Mutex::new(None);
    let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((w, map)) = last.as_ref() {
        if w.upgrade().is_some_and(|b| Arc::ptr_eq(&b, bed)) {
            return map.clone();
        }
    }
    let map = Arc::new(rs::SeabedMap {
        origin: bed.origin,
        cell: bed.cell,
        width: bed.width,
        depth: bed.depth,
        heights: bed.heights.clone(),
        deep: bed.deep,
    });
    *last = Some((Arc::downgrade(bed), map.clone()));
    map
}

/// The far plane of a camera's projection (m; read back from f32 matrix entries, so within
/// about 1% when it is thousands of times the near plane).
pub fn far_plane(cam: &rs::CameraData) -> f32 {
    let p = &cam.proj;
    let near = p.w_axis.z / p.z_axis.z;
    if cam.ortho { near - 1.0 / p.z_axis.z } else { p.w_axis.z / (p.z_axis.z + 1.0) }
}

/// Moves a camera's far plane (a right-handed, depth 0..1 projection as glam's `directx`
/// builders make), keeping its field of view and near plane.
pub fn set_far_plane(cam: &mut rs::CameraData, far: f32) {
    let p = &mut cam.proj;
    let near = p.w_axis.z / p.z_axis.z;
    let far = far.max(near * 2.0);
    if cam.ortho {
        p.z_axis.z = 1.0 / (near - far);
        p.w_axis.z = near / (near - far);
    } else {
        p.z_axis.z = far / (near - far);
        p.w_axis.z = near * far / (near - far);
    }
}

/// Fits the sun's shadow square to the camera: `reach` times the radius `apply` set, centred
/// ahead of the target along the view (a third of the radius), so what the camera sees in front
/// of its focus is shadowed and little is spent behind the eye. Looking straight down keeps it
/// on the target.
pub fn fit_shadow(scene: &mut Scene, eye: Vec3, target: Vec3, reach: f32) {
    let r = (scene.sun.shadow_radius * reach.max(0.05)).max(1.0);
    scene.sun.shadow_radius = r;
    let ahead = Vec2::new(target.x - eye.x, target.z - eye.z);
    let len = ahead.length();
    if len < 1e-3 {
        scene.sun.shadow_center = target;
        return;
    }
    // Full lead once the camera is as far behind its target as it is above it.
    let level = (len / ((target.y - eye.y).abs() + len)).clamp(0.0, 1.0);
    let lead = ahead / len * (r / 3.0) * level;
    scene.sun.shadow_center = target + Vec3::new(lead.x, 0.0, lead.y);
}

/// How stormy `a` is (0..1): rain under a heavy sky.
pub fn storminess(a: &Atmosphere) -> f32 {
    ((a.rain * 1.4).min(1.0) * smoothstep(0.45, 0.9, a.cloud_cover)).clamp(0.0, 1.0)
}

/// Sets everything `a` describes on `scene`, for a camera at `eye` looking at `target` (the
/// sun's shadow square is centred on the target). The camera's far plane is pushed out to
/// `a.far` when it is nearer (never pulled in): the islands on the horizon stay drawn.
pub fn apply(a: &Atmosphere, eye: Vec3, target: Vec3, scene: &mut Scene) {
    let _ = eye;
    if a.far > far_plane(&scene.camera) * 1.02 {
        set_far_plane(&mut scene.camera, a.far);
    }
    let flash = a.lightning.clamp(0.0, 1.0);
    let storm = storminess(a);
    let cover = a.cloud_cover.clamp(0.0, 1.0);

    // The sun: where the sky shows it, and the light it gives (held above the horizon,
    // fading out as it sets; lightning boosts it for a moment).
    let sun_dir = a.sun_direction();
    let up = (-sun_dir).y;
    let set = smoothstep(-0.06, 0.08, up);
    let light_dir = {
        let min = MIN_LIGHT_ELEVATION.to_radians().sin();
        let to_sun = -sun_dir;
        if to_sun.y >= min {
            sun_dir
        } else {
            let h = Vec2::new(to_sun.x, to_sun.z).normalize_or(Vec2::Y) * (1.0 - min * min).sqrt();
            -Vec3::new(h.x, min, h.y)
        }
    };
    let sun_col = lin(a.sun_color) * a.sun_intensity.max(0.0);
    let bolt = Vec3::new(0.85, 0.9, 1.0) * flash;
    scene.sun.direction = light_dir;
    scene.sun.color = sun_col * set * (1.0 - 0.75 * storm) + bolt * 0.9;
    scene.sun.shadow_center = target;
    scene.sun.shadow_radius = a.shadow_radius.max(1.0);
    let amb = a.ambient.max(0.0);
    scene.ambient.sky = lin(a.ambient_sky) * amb + bolt * 0.45;
    scene.ambient.ground = lin(a.ambient_ground) * amb + bolt * 0.15;

    // The sky.
    let zenith = lin(a.sky_zenith);
    let horizon = lin(a.sky_horizon);
    let fog = lin(a.fog_color);
    let peak = sun_col.max_element().max(1e-3);
    let sun_tint = sun_col / peak;
    let cloud = lin(a.cloud_color);
    let lit =
        cloud * (sun_tint * (0.8 * set * a.sun_intensity.min(1.5) * (1.0 - 0.6 * storm)) + lin(a.ambient_sky) * (amb * 0.55));
    let shade = cloud * zenith.lerp(horizon, 0.6) * (0.78 - 0.4 * storm) + lin(a.ambient_ground) * (amb * 0.08);
    scene.clear_color = horizon;
    scene.sky = Some(rs::Sky {
        zenith,
        horizon,
        ground: fog,
        sun_dir,
        sun_color: sun_col * 8.0 * (1.0 - 0.85 * storm),
        sun_size: 1.3,
        sun_glow: 0.45 * (1.0 - 0.8 * storm),
        clouds: rs::Clouds {
            cover,
            density: (0.82 + 0.18 * storm).min(1.0),
            color: lit,
            shade,
            offset: a.wind * (a.sea_time * 3.0),
            height: 900.0,
            curve: 0.12,
            size: 650.0,
            time: a.sea_time,
        },
        storm,
        flash,
    });

    // Fog along the view ray: aerial perspective out to the horizon.
    scene.fog = rs::Fog {
        enabled: true,
        center: target,
        start: a.fog_start.max(0.0),
        end: a.fog_end.max(a.fog_start + 1.0),
        color: fog + bolt * 0.15,
    };
    scene.fog_ray = Some(rs::FogRay { height_falloff: 1.0 / 110.0, base: 0.0, sun_glow: 0.35 * (1.0 - storm) * set });

    // The sea.
    scene.ocean = a.sea.as_ref().map(|sea| {
        let mut o = rs::Ocean::from_sea(sea, a.sea_time);
        o.sky_zenith = zenith;
        o.sky_horizon = horizon.lerp(fog, 0.5);
        o.glint = 0.6 * (1.0 - 0.9 * storm) * (1.0 - 0.5 * cover * cover) * set;
        // A storm greys the water.
        let grey = Vec3::splat(o.deep.dot(Vec3::new(0.3, 0.5, 0.2)));
        o.deep = o.deep.lerp(grey, 0.45 * storm);
        o.mid = o.mid.lerp(Vec3::splat(o.mid.dot(Vec3::new(0.3, 0.5, 0.2))), 0.45 * storm);
        o.shallow = o.shallow.lerp(Vec3::splat(o.shallow.dot(Vec3::new(0.3, 0.5, 0.2))), 0.4 * storm);
        o.seabed = a.seabed.as_ref().map(seabed_map);
        o
    });

    // Rain.
    scene.rain = (a.rain > 0.001).then(|| rs::Rain {
        amount: a.rain.clamp(0.0, 1.0),
        wind: a.wind,
        color: lin(a.ambient_sky) * (amb * 1.3) + sun_col * 0.1 + bolt,
    });

    // The grade.
    scene.post.bloom = a.bloom.max(0.0);
    scene.post.exposure = a.exposure.max(0.0);
    scene.post.saturation = a.saturation.max(0.0);
    scene.filter.contrast = a.contrast.max(0.0);
    scene.gull = a.gull.clamp(0.0, 1.0);

    // One wind for sway, clouds and rain.
    let speed = a.wind.length();
    scene.wind.direction = a.wind.normalize_or(Vec2::X);
    scene.wind.strength = (speed / 6.0).min(2.5);
    scene.wind.gusts = 1.0 + storm;
}
