//! The sea and the sky at every atmosphere preset: a camera 12 m over the water looking at the
//! horizon, an island with a shelf of shallows (and the same heights as the ocean's seabed), a
//! ship and a barrel riding the simulation's waves, a far island in the haze. Saves one PNG per
//! preset, a swimmer's view (2 m), a high view (60 m), a Gull Sight shot, a grid-snap pair and a
//! contact sheet to `out/sea_sky/`, and prints the frame times.
//!
//! `cargo run -p pav_view --example sea_sky [-- OUT_DIR]`
use std::sync::Arc;
use std::time::Instant;

use glam::{Mat4, Quat, Vec2, Vec3};
use pav_core::Color;
use pav_core::ocean::{SeaPreset, SeaState, Seabed};
use pav_core::play::Atmosphere;
use pav_render::scene::{self as rs, CameraData, MeshInstance, Scene, Style, flags};
use pav_render::{MeshData, MeshKey, Renderer, capture, gpu, mesh::Vertex};
use pav_view::atmos;

const W: u32 = 960;
const H: u32 = 540;
/// The camera looks this way (degrees clockwise from north): toward the late sun.
const LOOK_AZ: f32 = 236.0;
const ISLAND: (f32, f32) = (212.0, 190.0);
const SHIP: (f32, f32) = (246.0, 78.0);
const FAR_ISLAND: (f32, f32) = (262.0, 1150.0);
const WIND_DEG: f32 = 200.0;

fn toward(az: f32, dist: f32) -> Vec2 {
    let a = az.to_radians();
    Vec2::new(a.sin(), -a.cos()) * dist
}

fn hex(s: &str) -> Color {
    Color::hex(s)
}

// ---------------------------------------------------------------- the island

/// Ground height of the near island at a point (m, sea level 0): a jungle hill, a sand beach,
/// a wide shelf of shallows, then the drop to open water.
fn island_height(p: Vec2) -> f32 {
    let c = toward(ISLAND.0, ISLAND.1);
    let d = p - c;
    let th = d.y.atan2(d.x);
    let r = d.length() / (1.0 + 0.1 * (3.0 * th + 1.0).sin() + 0.05 * (7.0 * th + 2.0).sin());
    let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t.clamp(0.0, 1.0);
    let ss = |a: f32, b: f32, x: f32| {
        let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    };
    if r < 34.0 {
        lerp(13.0, 6.0, ss(0.0, 34.0, r))
    } else if r < 52.0 {
        lerp(6.0, 1.4, ss(34.0, 52.0, r))
    } else if r < 62.0 {
        lerp(1.4, -0.7, (r - 52.0) / 10.0)
    } else if r < 150.0 {
        lerp(-0.7, -4.0, ss(62.0, 150.0, r))
    } else {
        lerp(-4.0, -30.0, ss(150.0, 230.0, r))
    }
}

fn seabed() -> Arc<Seabed> {
    let c = toward(ISLAND.0, ISLAND.1);
    let cell = 1.0;
    let n = 520;
    let origin = c - Vec2::splat(n as f32 * cell * 0.5);
    let mut heights = Vec::with_capacity(n * n);
    for z in 0..n {
        for x in 0..n {
            heights.push(island_height(origin + Vec2::new(x as f32 + 0.5, z as f32 + 0.5) * cell));
        }
    }
    Arc::new(Seabed { origin, cell, width: n, depth: n, heights, deep: -30.0 })
}

fn vertex(p: Vec3, c: Vec3) -> Vertex {
    Vertex { pos: p.to_array(), normal: [0.0, 1.0, 0.0], uv: [0.0; 2], color: [c.x, c.y, c.z, 1.0] }
}

fn smooth_normals(m: &mut MeshData) {
    let mut acc = vec![Vec3::ZERO; m.vertices.len()];
    for t in m.indices.as_chunks::<3>().0 {
        let [a, b, c] = [t[0], t[1], t[2]].map(|i| Vec3::from(m.vertices[i as usize].pos));
        let n = (b - a).cross(c - a);
        for &i in t {
            acc[i as usize] += n;
        }
    }
    for (v, n) in m.vertices.iter_mut().zip(acc) {
        v.normal = n.normalize_or(Vec3::Y).to_array();
    }
}

/// The island's terrain down to below the water (the seabed carries on from there).
fn island_mesh() -> MeshData {
    let c = toward(ISLAND.0, ISLAND.1);
    let (rings, segs) = (70, 120);
    let mut m = MeshData::default();
    let sand = lin("#e8d3a2");
    let wet = lin("#b89e72");
    let grass = lin("#6aa84f");
    let jungle = lin("#2f6d3a");
    for i in 0..=rings {
        let r = 80.0 * (i as f32 / rings as f32).powf(0.8);
        for j in 0..segs {
            let a = j as f32 / segs as f32 * std::f32::consts::TAU;
            let p = c + Vec2::new(a.cos(), a.sin()) * r;
            let h = island_height(p);
            let col = if h > 3.2 {
                jungle.lerp(grass, ((h - 6.0) / 6.0).clamp(0.0, 1.0) * 0.4)
            } else if h > 1.9 {
                grass
            } else if h > 0.5 {
                sand
            } else {
                wet
            };
            m.vertices.push(vertex(Vec3::new(p.x - c.x, h, p.y - c.y), col));
        }
    }
    for i in 0..rings {
        for j in 0..segs {
            let a = i * segs + j;
            let b = i * segs + (j + 1) % segs;
            let (c2, d) = (a + segs, b + segs);
            m.indices.extend([a, c2, b, b, c2, d].map(|k| k as u32));
        }
    }
    smooth_normals(&mut m);
    m
}

fn lin(s: &str) -> Vec3 {
    Vec3::from(Color::hex(s).0)
}

fn inst(mesh: MeshKey, t: Mat4, color: Vec3, f: u32, group: u32) -> MeshInstance {
    MeshInstance { mesh, transform: t, color, emissive: 0.0, style: Style::Lit, flags: f, group, material: 0, ext: Vec3::ZERO }
}

fn boxed(center: Vec3, size: Vec3, rot: Quat, color: Vec3, f: u32, group: u32) -> MeshInstance {
    inst(MeshKey::Cube, Mat4::from_scale_rotation_translation(size, rot, center), color, f, group)
}

/// The world: the island (terrain, palms, a hut), the far island, a ship and a barrel riding
/// the waves at sea time `t`.
fn world(scene: &mut Scene, sea: &SeaState, t: f32) {
    let key = MeshKey::Custom(0x15_1a_d0);
    scene.custom_meshes.push((key, Arc::new(island_mesh())));
    let c = toward(ISLAND.0, ISLAND.1);
    scene.meshes.push(inst(key, Mat4::from_translation(Vec3::new(c.x, 0.0, c.y)), Vec3::ONE, flags::CULL_STRUCTURE, 1));
    // Palms and a hut.
    let trunk = lin("#8b6b4a");
    let frond = lin("#3f8a3a");
    for (k, (a, r)) in
        [(0.3f32, 40.0f32), (1.4, 45.0), (2.2, 30.0), (3.6, 44.0), (4.4, 38.0), (5.5, 46.0), (0.9, 20.0)].iter().enumerate()
    {
        let p = c + Vec2::new(a.cos(), a.sin()) * *r;
        let h = island_height(p);
        let lean = Quat::from_rotation_z(0.25 * (k as f32 * 1.7).sin()) * Quat::from_rotation_x(0.2 * (k as f32 * 2.3).cos());
        let top = Vec3::new(p.x, h, p.y) + lean * Vec3::new(0.0, 9.0, 0.0);
        scene.meshes.push(inst(
            MeshKey::Cylinder,
            Mat4::from_scale_rotation_translation(
                Vec3::new(0.5, 9.0, 0.5),
                lean,
                Vec3::new(p.x, h, p.y) + lean * Vec3::new(0.0, 4.5, 0.0),
            ),
            trunk,
            flags::CULL_VEGETATION,
            1,
        ));
        for f in 0..5 {
            let ang = f as f32 * 1.256 + k as f32;
            let rot = Quat::from_rotation_y(ang) * Quat::from_rotation_z(-0.45);
            scene.meshes.push(boxed(
                top + Quat::from_rotation_y(ang) * Vec3::new(2.0, -0.3, 0.0),
                Vec3::new(4.5, 0.15, 1.2),
                rot,
                frond,
                flags::CULL_VEGETATION | flags::SWAY,
                1,
            ));
        }
    }
    let hut = c + toward(ISLAND.0 + 160.0, 30.0) * 0.0 + Vec2::new(-26.0, 12.0);
    let hh = island_height(hut);
    scene.meshes.push(boxed(Vec3::new(hut.x, hh + 1.6, hut.y), Vec3::new(6.0, 3.2, 5.0), Quat::IDENTITY, lin("#cbbd9b"), 0, 2));
    scene.meshes.push(boxed(Vec3::new(hut.x, hh + 3.6, hut.y), Vec3::new(7.0, 0.8, 6.0), Quat::IDENTITY, lin("#b5543a"), 0, 2));

    // The far island, in the haze.
    let f = toward(FAR_ISLAND.0, FAR_ISLAND.1);
    scene.meshes.push(inst(
        MeshKey::Cone,
        Mat4::from_scale_rotation_translation(Vec3::new(420.0, 85.0, 300.0), Quat::IDENTITY, Vec3::new(f.x, 30.0, f.y)),
        lin("#4f7a4a"),
        flags::CULL_STRUCTURE,
        3,
    ));
    scene.meshes.push(inst(
        MeshKey::Cylinder,
        Mat4::from_scale_rotation_translation(Vec3::new(470.0, 4.0, 350.0), Quat::IDENTITY, Vec3::new(f.x, -1.0, f.y)),
        lin("#d8c49a"),
        flags::CULL_STRUCTURE,
        3,
    ));

    // A ship riding the waves: placed on the simulation's surface (height and normal).
    let s = toward(SHIP.0, SHIP.1);
    let surf = sea.sample(s, t);
    let heading = Quat::from_rotation_y(0.9);
    let tilt = Quat::from_rotation_arc(Vec3::Y, surf.normal.lerp(Vec3::Y, 0.4).normalize());
    let rot = tilt * heading;
    let base = Vec3::new(s.x, surf.height, s.y);
    let hull = lin("#6b4a2f");
    let o = flags::OBJECT;
    let part = |scene: &mut Scene, local: Vec3, size: Vec3, color: Vec3, mesh: MeshKey| {
        scene.meshes.push(inst(mesh, Mat4::from_scale_rotation_translation(size, rot, base + rot * local), color, o, 40));
    };
    part(scene, Vec3::new(0.0, 0.4, 0.0), Vec3::new(4.2, 2.6, 15.0), hull, MeshKey::Cube);
    part(scene, Vec3::new(0.0, 0.4, -8.2), Vec3::new(3.0, 2.4, 2.6), hull, MeshKey::Cube);
    part(scene, Vec3::new(0.0, 2.3, 5.5), Vec3::new(3.8, 1.4, 3.5), lin("#7d5a3a"), MeshKey::Cube);
    for (z, hgt) in [(-3.5f32, 13.0f32), (2.5, 15.0)] {
        part(scene, Vec3::new(0.0, 1.7 + hgt * 0.5, z), Vec3::new(0.35, hgt, 0.35), lin("#5a4030"), MeshKey::Cylinder);
        part(scene, Vec3::new(0.0, 1.7 + hgt * 0.6, z + 0.3), Vec3::new(5.5, hgt * 0.5, 0.12), lin("#efe7d6"), MeshKey::Cube);
    }
    // A barrel bobbing nearby.
    let b = toward(SHIP.0 - 12.0, 26.0);
    let sb = sea.sample(b, t);
    scene.meshes.push(inst(
        MeshKey::Cylinder,
        Mat4::from_scale_rotation_translation(
            Vec3::new(0.9, 1.2, 0.9),
            Quat::from_rotation_arc(Vec3::Y, sb.normal),
            Vec3::new(b.x, sb.height + 0.1, b.y),
        ),
        lin("#c9822b"),
        o,
        41,
    ));
}

// ---------------------------------------------------------------- presets

fn preset(name: &str, sea_time: f32, bed: &Arc<Seabed>) -> Atmosphere {
    let wind = toward(WIND_DEG, 1.0);
    let wind_deg = wind.y.atan2(wind.x).to_degrees();
    let sea = |p: SeaPreset| SeaState::preset(p, wind_deg);
    let base = Atmosphere { sea_time, seabed: Some(bed.clone()), far: 1800.0, ..Default::default() };
    match name {
        "golden_hour" => Atmosphere {
            sun_azimuth: 250.0,
            sun_elevation: 12.0,
            sun_color: hex("#ffb878"),
            sun_intensity: 1.3,
            ambient_sky: hex("#8fb0d8"),
            ambient_ground: hex("#a08060"),
            ambient: 0.5,
            sky_zenith: hex("#3d6fae"),
            sky_horizon: hex("#f6cf98"),
            fog_color: hex("#f0cfa4"),
            fog_start: 260.0,
            fog_end: 1700.0,
            cloud_cover: 0.38,
            cloud_color: hex("#ffffff"),
            wind: wind * 6.0,
            saturation: 1.08,
            contrast: 1.03,
            bloom: 0.4,
            sea: Some(sea(SeaPreset::Breezy)),
            ..base
        },
        "afternoon" => Atmosphere {
            sun_azimuth: 205.0,
            sun_elevation: 52.0,
            sun_color: hex("#fff2de"),
            sun_intensity: 1.15,
            ambient_sky: hex("#9fc4e8"),
            ambient_ground: hex("#8a7a62"),
            ambient: 0.55,
            sky_zenith: hex("#2a62b0"),
            sky_horizon: hex("#c4e2f3"),
            fog_color: hex("#cfe5f2"),
            fog_start: 380.0,
            fog_end: 1800.0,
            cloud_cover: 0.32,
            wind: wind * 5.0,
            sea: Some(sea(SeaPreset::Calm)),
            ..base
        },
        "sunset" => Atmosphere {
            sun_azimuth: 252.0,
            sun_elevation: 2.5,
            sun_color: hex("#ff8040"),
            sun_intensity: 1.15,
            ambient_sky: hex("#7a7ab4"),
            ambient_ground: hex("#8a5a4a"),
            ambient: 0.45,
            sky_zenith: hex("#2b3a7a"),
            sky_horizon: hex("#ff9e5e"),
            fog_color: hex("#f2a274"),
            fog_start: 220.0,
            fog_end: 1500.0,
            cloud_cover: 0.48,
            cloud_color: hex("#ffd2b4"),
            wind: wind * 6.0,
            bloom: 0.45,
            saturation: 1.1,
            sea: Some(sea(SeaPreset::Breezy)),
            ..base
        },
        "storm" => Atmosphere {
            sun_azimuth: 230.0,
            sun_elevation: 30.0,
            sun_color: hex("#cfd6de"),
            sun_intensity: 0.7,
            ambient_sky: hex("#5d6b78"),
            ambient_ground: hex("#30363a"),
            ambient: 0.65,
            sky_zenith: hex("#262d34"),
            sky_horizon: hex("#5c6871"),
            fog_color: hex("#56626b"),
            fog_start: 90.0,
            fog_end: 800.0,
            cloud_cover: 0.96,
            cloud_color: hex("#8d969e"),
            rain: 0.8,
            lightning: 0.35,
            wind: wind * 13.0,
            saturation: 0.85,
            bloom: 0.3,
            sea: Some(sea(SeaPreset::Storm)),
            ..base
        },
        "dawn_after_storm" => Atmosphere {
            sun_azimuth: 95.0,
            sun_elevation: 8.0,
            sun_color: hex("#ffcaa0"),
            sun_intensity: 1.0,
            ambient_sky: hex("#a8b8d8"),
            ambient_ground: hex("#7a7066"),
            ambient: 0.55,
            sky_zenith: hex("#5a80ba"),
            sky_horizon: hex("#f3d6c6"),
            fog_color: hex("#e9d6cf"),
            fog_start: 200.0,
            fog_end: 1400.0,
            cloud_cover: 0.55,
            cloud_color: hex("#fff0e2"),
            wetness: 0.6,
            wind: wind * 7.0,
            sea: Some(SeaState::blend(&sea(SeaPreset::Rough), &sea(SeaPreset::Breezy), 0.5)),
            ..base
        },
        "memory_mist" => Atmosphere {
            sun_azimuth: 220.0,
            sun_elevation: 40.0,
            sun_color: hex("#ffffff"),
            sun_intensity: 0.8,
            ambient_sky: hex("#e4edf2"),
            ambient_ground: hex("#c8c8c8"),
            ambient: 0.85,
            sky_zenith: hex("#dfe8ee"),
            sky_horizon: hex("#f3f6f8"),
            fog_color: hex("#eef2f5"),
            fog_start: 15.0,
            fog_end: 260.0,
            cloud_cover: 0.2,
            wind: wind * 2.0,
            saturation: 0.6,
            exposure: 1.05,
            sea: Some(sea(SeaPreset::Calm)),
            ..base
        },
        _ => base,
    }
}

const PRESETS: [&str; 6] = ["golden_hour", "afternoon", "sunset", "storm", "dawn_after_storm", "memory_mist"];

// ---------------------------------------------------------------- shots

fn camera(eye: Vec3, target: Vec3) -> CameraData {
    let forward = (target - eye).normalize();
    CameraData {
        view: glam::camera::rh::view::look_to_mat4(eye, forward, Vec3::Y),
        proj: glam::camera::rh::proj::directx::perspective(55f32.to_radians(), W as f32 / H as f32, 0.1, 1800.0),
        eye,
        forward,
        ortho: false,
    }
}

fn scene_for(a: &Atmosphere, eye: Vec3, target: Vec3) -> Scene {
    let mut scene = Scene { camera: camera(eye, target), time: a.sea_time, ..Default::default() };
    scene.post.tonemap = rs::Tonemap::Aces;
    scene.post.outline_part = rs::Part::Objects;
    scene.style.specular = 0.25;
    scene.cull_small = 120.0;
    scene.cull_vegetation = 400.0;
    scene.cull_structure = 2000.0;
    atmos::apply(a, eye, target, &mut scene);
    world(&mut scene, a.sea.as_ref().expect("sea"), a.sea_time);
    scene
}

fn horizon_shot(a: &Atmosphere, height: f32, pitch_deg: f32) -> Scene {
    let eye = Vec3::new(0.0, height, 0.0);
    let d = toward(LOOK_AZ, 1.0);
    let p = pitch_deg.to_radians();
    let target = eye + Vec3::new(d.x * p.cos(), -p.sin(), d.y * p.cos()) * 100.0;
    scene_for(a, eye, target)
}

/// The island's beach from 40 m out, 9 m up: shallows and surf up close.
fn shore_shot(a: &Atmosphere) -> Scene {
    let c = toward(ISLAND.0, ISLAND.1);
    let out = (-c).normalize();
    let side = Vec2::new(-out.y, out.x);
    let eye2 = c + out * 100.0 + side * 25.0;
    let look = c + out * 50.0 - side * 10.0;
    scene_for(a, Vec3::new(eye2.x, 9.0, eye2.y), Vec3::new(look.x, 0.0, look.y))
}

/// Gull Sight: a wall on the beach hides a red box (an enemy) and a blue capsule (an ally); a
/// gold box (the target) stands in the open.
fn gull_shot(a: &Atmosphere) -> Scene {
    let c = toward(ISLAND.0, ISLAND.1);
    let toward_cam = (-c).normalize();
    let spot = c + toward_cam * 54.0;
    let eye2 = spot + toward_cam * 26.0;
    let eye = Vec3::new(eye2.x, 7.0, eye2.y);
    let ground = island_height(spot).max(0.0);
    let target = Vec3::new(spot.x, ground + 1.5, spot.y);
    let mut s = scene_for(&Atmosphere { gull: 0.85, ..a.clone() }, eye, target);
    let side = Vec2::new(-toward_cam.y, toward_cam.x);
    let rot = Quat::from_rotation_y(-toward_cam.y.atan2(toward_cam.x) + std::f32::consts::FRAC_PI_2);
    let wall = spot + toward_cam * 1.5;
    s.meshes.push(boxed(Vec3::new(wall.x, ground + 1.6, wall.y), Vec3::new(9.0, 3.2, 0.8), rot, lin("#8c8577"), 0, 60));
    let red = spot - toward_cam * 1.5 + side * 1.5;
    s.meshes.push(boxed(
        Vec3::new(red.x, ground + 1.0, red.y),
        Vec3::new(1.2, 2.0, 1.2),
        rot,
        Vec3::new(0.85, 0.06, 0.04),
        flags::XRAY | flags::OBJECT,
        61,
    ));
    let blue = spot - toward_cam * 2.0 - side * 2.0;
    let mut cap = rs::SdfInstance::capsule(
        Vec3::new(blue.x, ground + 0.5, blue.y),
        Vec3::new(blue.x, ground + 1.9, blue.y),
        0.45,
        Vec3::new(0.1, 0.3, 0.95),
    );
    cap.flags = flags::XRAY | flags::OBJECT;
    cap.group = 62;
    s.sdfs.push(cap);
    let gold = spot + side * 6.5 + toward_cam * 3.0;
    s.meshes.push(boxed(
        Vec3::new(gold.x, ground + 0.8, gold.y),
        Vec3::new(1.0, 1.6, 1.0),
        rot,
        Vec3::new(0.95, 0.65, 0.1),
        flags::XRAY | flags::OBJECT,
        63,
    ));
    s
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = std::path::PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| "out/sea_sky".into()));
    std::fs::create_dir_all(&out)?;
    let gpu = gpu::Headless::new()?;
    println!("adapter: {}", gpu::describe(&gpu.adapter.get_info()));
    let mut r = Renderer::new(&gpu.device, &gpu.queue);
    let bed = seabed();
    let t = 37.0;
    if std::env::var_os("SEA_SKY_PROFILE").is_some() {
        return profile(&mut r, &bed, t);
    }
    let mut sheet = Vec::new();
    let mut times = Vec::new();
    let mut render = |r: &mut Renderer, name: &str, scene: &Scene| -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let _ = capture::render_to_rgba(r, scene, W, H)?;
        let start = Instant::now();
        let px = capture::render_to_rgba(r, scene, W, H)?;
        let ms = start.elapsed().as_secs_f64() * 1000.0;
        times.push(ms);
        capture::save_png(&out.join(format!("{name}.png")), W, H, &px)?;
        let s = r.stats;
        println!(
            "{name}: {ms:.0} ms; {} meshes drawn, culled {} by view and {} by distance, {} shadow casters, {} xray, ocean {} vertices, {} draw calls",
            s.mesh_instances,
            s.culled_frustum,
            s.culled_distance,
            s.shadow_instances,
            s.xray_instances,
            s.ocean_vertices,
            s.draw_calls
        );
        Ok(px)
    };
    for name in PRESETS {
        let a = preset(name, t, &bed);
        sheet.push(render(&mut r, name, &horizon_shot(&a, 12.0, 4.0))?);
    }
    let (w, h, px) = capture::tile_frames(&sheet, W, H, 3);
    capture::save_png(&out.join("contact_sheet.png"), w, h, &px)?;

    let noon = preset("afternoon", t, &bed);
    let storm = preset("storm", t, &bed);
    let mut extra = vec![
        render(&mut r, "swimmer_2m", &horizon_shot(&preset("golden_hour", t, &bed), 2.0, 1.0))?,
        render(&mut r, "high_60m", &horizon_shot(&noon, 60.0, 22.0))?,
        render(&mut r, "storm_high_60m", &horizon_shot(&storm, 60.0, 18.0))?,
        render(&mut r, "gull_sight", &gull_shot(&noon))?,
    ];
    // The grid moves in 32 m steps: two frames either side of a step should match closely.
    let mut a = horizon_shot(&noon, 30.0, 12.0);
    let mut b = a.clone();
    for (s, x) in [(&mut a, 15.9f32), (&mut b, 16.1)] {
        let eye = Vec3::new(x, 30.0, 0.0);
        let target = eye + (s.camera.forward * 100.0);
        s.camera = camera(eye, target);
    }
    let pa = render(&mut r, "snap_a", &a)?;
    let pb = render(&mut r, "snap_b", &b)?;
    let mean_diff =
        |a: &[u8], b: &[u8]| a.iter().zip(b).map(|(x, y)| (*x as f64 - *y as f64).abs()).sum::<f64>() / a.len() as f64;
    // The same 0.2 m move away from a step, for comparison (the camera moving is a change too).
    let (mut c, mut d) = (a.clone(), b.clone());
    for (s, x) in [(&mut c, 5.9f32), (&mut d, 6.1)] {
        let eye = Vec3::new(x, 30.0, 0.0);
        s.camera = camera(eye, eye + s.camera.forward * 100.0);
    }
    let pc = capture::render_to_rgba(&mut r, &c, W, H)?;
    let pd = capture::render_to_rgba(&mut r, &d, W, H)?;
    println!(
        "grid snap: mean channel difference for a 0.2 m move across a 32 m step {:.2}, and within a step {:.2}",
        mean_diff(&pa, &pb),
        mean_diff(&pc, &pd)
    );
    extra.push(pa);
    extra.push(render(&mut r, "shore", &shore_shot(&preset("golden_hour", t, &bed)))?);
    let (w, h, px) = capture::tile_frames(&extra, W, H, 3);
    capture::save_png(&out.join("views_sheet.png"), w, h, &px)?;
    let mean = times.iter().sum::<f64>() / times.len() as f64;
    println!("mean frame (render + readback) at {W}x{H}: {mean:.0} ms over {} captures", times.len());
    println!("wrote {}", out.display());
    Ok(())
}

/// `SEA_SKY_PROFILE=1`: what each part costs (best of five captures, render and readback).
fn profile(r: &mut Renderer, bed: &Arc<Seabed>, t: f32) -> Result<(), Box<dyn std::error::Error>> {
    let full = horizon_shot(&preset("storm", t, bed), 12.0, 4.0);
    let variants: [(&str, Box<dyn Fn(&mut Scene)>); 7] = [
        ("full storm frame", Box::new(|_| {})),
        ("no rain", Box::new(|s| s.rain = None)),
        (
            "no rain, no sky (clear colour)",
            Box::new(|s| {
                s.rain = None;
                s.sky = None;
            }),
        ),
        (
            "no rain, no ocean",
            Box::new(|s| {
                s.rain = None;
                s.ocean = None;
            }),
        ),
        (
            "no rain, low ocean grid (64 cells)",
            Box::new(|s| {
                s.rain = None;
                if let Some(o) = &mut s.ocean {
                    o.cells = 64;
                }
            }),
        ),
        (
            "nothing but the objects",
            Box::new(|s| {
                s.rain = None;
                s.sky = None;
                s.ocean = None;
            }),
        ),
        ("no shadows", Box::new(|s| s.sun.shadows = false)),
    ];
    for (name, change) in &variants {
        let mut scene = full.clone();
        change(&mut scene);
        let mut best = f64::MAX;
        for _ in 0..5 {
            let start = Instant::now();
            let _ = capture::render_to_rgba(r, &scene, W, H)?;
            best = best.min(start.elapsed().as_secs_f64() * 1000.0);
        }
        println!("{name}: {best:.0} ms");
    }
    Ok(())
}
