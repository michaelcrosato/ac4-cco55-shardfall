//! Renders scenes that existed before the sea and sky (a town room through the view builder,
//! a renderer smoke scene) and compares them with a reference directory, so renderer changes
//! can prove they leave existing scenes unchanged.
//!
//! `cargo run -p pav_view --example render_regress -- OUT [REFERENCE]`
use std::path::{Path, PathBuf};

use glam::{Mat4, Quat, Vec3};
use pav_core::Sim;
use pav_render::*;
use pav_view::build::{ViewBuilder, ViewSettings};
use pav_view::camera::{CameraParams, CameraRig};

const W: u32 = 640;
const H: u32 = 360;

fn smoke() -> Scene {
    let mut scene = Scene::default();
    let eye = Vec3::new(0.0, 12.0, 9.0);
    let fwd = (Vec3::ZERO - eye).normalize();
    scene.camera = CameraData {
        view: glam::camera::rh::view::look_to_mat4(eye, fwd, Vec3::Y),
        proj: glam::camera::rh::proj::directx::perspective(40f32.to_radians(), 16.0 / 9.0, 0.1, 200.0),
        eye,
        forward: fwd,
        ortho: false,
    };
    let inst = |mesh, t: Mat4, c: Vec3, style, group| MeshInstance {
        mesh,
        transform: t,
        color: c,
        emissive: 0.0,
        style,
        flags: 0,
        group,
        material: 0,
        ext: Vec3::ZERO,
    };
    scene.meshes.push(inst(
        MeshKey::Cube,
        Mat4::from_scale_rotation_translation(Vec3::new(20.0, 1.0, 20.0), Quat::IDENTITY, Vec3::new(0.0, -0.5, 0.0)),
        srgb(120, 170, 110),
        Style::Cel,
        1,
    ));
    for (i, style) in Style::ALL.iter().enumerate() {
        let x = i as f32 * 2.5 - 3.75;
        scene.meshes.push(inst(
            MeshKey::Cube,
            Mat4::from_rotation_translation(Quat::from_rotation_y(0.5), Vec3::new(x, 0.5, 2.0)),
            srgb(220, 120, 80),
            *style,
            10 + i as u32,
        ));
        let mut s = SdfInstance::sphere(Vec3::new(x, 0.7, -0.5), 0.7, srgb(240, 200, 60));
        s.style = *style;
        s.group = 30 + i as u32;
        scene.sdfs.push(s);
        scene.meshes.push(inst(
            MeshKey::Cylinder,
            Mat4::from_scale_rotation_translation(Vec3::new(0.8, 1.6, 0.8), Quat::IDENTITY, Vec3::new(x, 0.8, -5.5)),
            srgb(150, 150, 160),
            *style,
            50 + i as u32,
        ));
    }
    scene.point_lights.push(PointLight {
        position: Vec3::new(0.0, 1.5, 1.0),
        color: Vec3::new(1.0, 0.5, 0.2) * 2.0,
        radius: 5.0,
        shadows: true,
    });
    scene.texts.push(Text3d {
        text: "Pavilion".into(),
        origin: Vec3::new(-2.0, 2.5, 0.0),
        right: Vec3::X,
        up: Vec3::Y,
        size: 0.8,
        color: Vec3::ONE,
        anchor: Anchor::Center,
        flags: 0,
        weight: 0.0,
    });
    scene.fog = Fog { enabled: true, center: Vec3::ZERO, start: 8.0, end: 30.0, color: scene.clear_color };
    scene.post.haze = 0.6;
    scene.post.gi = 0.5;
    scene
}

fn room(name: &str, params: CameraParams) -> Scene {
    let mut sim = Sim::new(name, 7).expect("room");
    for _ in 0..20 {
        sim.step(&Default::default());
    }
    let frame = sim.frame();
    let mut rig = CameraRig::new(params);
    let focus = frame.focus;
    rig.target = focus;
    ViewBuilder::new().build(&frame, &frame, 1.0, &rig, W as f32 / H as f32, &ViewSettings::default(), focus)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| "out/regress".into()));
    let reference = std::env::args().nth(2).map(PathBuf::from);
    std::fs::create_dir_all(&out)?;
    let gpu = gpu::Headless::new()?;
    let mut r = Renderer::new(&gpu.device, &gpu.queue);
    let persp = CameraParams::default();
    let ortho = CameraParams { ortho: true, ..CameraParams::default() };
    let low = CameraParams { tilt: 12.0, distance: 14.0, ..CameraParams::default() };
    let scenes =
        [("smoke", smoke()), ("town", room("town", persp)), ("town_ortho", room("town", ortho)), ("town_low", room("town", low))];
    let mut worst = 0u32;
    for (name, scene) in &scenes {
        // Twice: the second frame has warm caches (and settles particles' first frame).
        let _ = capture::render_to_rgba(&mut r, scene, W, H)?;
        let px = capture::render_to_rgba(&mut r, scene, W, H)?;
        let path = out.join(format!("{name}.png"));
        capture::save_png(&path, W, H, &px)?;
        std::fs::write(out.join(format!("{name}.rgba")), &px)?;
        let mut line = format!("{name}: {:?}", r.stats);
        if let Some(dir) = &reference {
            let (max, diff) = compare(&dir.join(format!("{name}.rgba")), &px)?;
            worst = worst.max(max);
            line = format!("{name}: max channel diff {max}, pixels differing {diff}");
        }
        println!("{line}");
    }
    if reference.is_some() {
        println!("worst channel difference: {worst}");
    }
    Ok(())
}

fn compare(path: &Path, px: &[u8]) -> std::io::Result<(u32, usize)> {
    let buf = std::fs::read(path)?;
    let mut max = 0u32;
    let mut count = 0;
    for (a, b) in buf.chunks(4).zip(px.chunks(4)) {
        let d = (0..3).map(|i| (a[i] as i32 - b[i] as i32).unsigned_abs()).max().unwrap_or(0);
        max = max.max(d);
        if d > 2 {
            count += 1;
        }
    }
    Ok((max, count))
}
