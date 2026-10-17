//! Draws a hook game's presentation (`RenderFrame::play`, `pav_core::play`): the camera hint,
//! the draw list, the atmosphere (sun, ambient, sky colour, fog; the sea and sky passes come
//! from `atmos`) and frame effects (shake, transitions). Items are interpolated between the two
//! frames the view blends, matched by position in the list (games emit them in a stable order).

use std::collections::HashMap;
use std::sync::Arc;

use glam::{Mat4, Quat, Vec3};
use pav_core::Color;
use pav_core::play::{CameraHint, DrawItem, Mesh, PlayFrame};
use pav_render::mesh::{MeshData, MeshKey, Vertex};
use pav_render::scene::{self as rs, CameraData, MeshInstance, Scene, Style, flags as rflags};

fn v3(c: Color) -> Vec3 {
    Vec3::from(c.0)
}

/// The camera for a hint, blended between two frames (a cut snaps to the newer one).
pub fn hint_camera(prev: Option<&CameraHint>, curr: &CameraHint, alpha: f32, aspect: f32) -> CameraData {
    let (eye, target, fov) = match prev.filter(|_| !curr.cut) {
        Some(p) => (p.eye.lerp(curr.eye, alpha), p.target.lerp(curr.target, alpha), p.fov + (curr.fov - p.fov) * alpha),
        None => (curr.eye, curr.target, curr.fov),
    };
    camera_data(eye, target, fov, curr.far, aspect)
}

/// A perspective camera from an eye and a target.
pub fn camera_data(eye: Vec3, target: Vec3, fov_deg: f32, far: f32, aspect: f32) -> CameraData {
    let fwd = (target - eye).normalize_or(Vec3::NEG_Z);
    let up = if fwd.y.abs() > 0.999 { Vec3::Z } else { Vec3::Y };
    let view = glam::camera::rh::view::look_to_mat4(eye, fwd, up);
    let mut fov = fov_deg.clamp(5.0, 150.0).to_radians();
    if aspect < 1.0 && aspect > 0.0 {
        fov = (2.0 * ((fov * 0.5).tan() / aspect).atan()).min(150f32.to_radians());
    }
    // A long far plane needs a larger near plane for depth precision.
    let far = far.max(50.0);
    let near = (far / 6000.0).clamp(0.1, 0.5);
    let proj = glam::camera::rh::proj::directx::perspective(fov, aspect.max(0.01), near, far);
    CameraData { view, proj, eye, forward: fwd, ortho: false }
}

/// Caches GPU-ready copies of game meshes by their id.
#[derive(Default)]
pub struct PlayView {
    meshes: HashMap<u64, Arc<MeshData>>,
    rings: HashMap<u32, Arc<MeshData>>,
    /// Frame time of the last build (cache housekeeping).
    frames: u64,
    used: HashMap<u64, u64>,
}

/// Converts a game mesh to the renderer's vertex format.
pub fn mesh_data(m: &Mesh) -> MeshData {
    let vertices = (0..m.positions.len())
        .map(|i| {
            let c = m.colors.get(i).copied().unwrap_or([1.0; 3]);
            Vertex {
                pos: m.positions[i],
                normal: m.normals.get(i).copied().unwrap_or([0.0, 1.0, 0.0]),
                uv: [0.0, 0.0],
                color: [c[0], c[1], c[2], 1.0],
            }
        })
        .collect();
    MeshData { vertices, indices: m.indices.clone() }
}

/// A flat annulus in the unit disc (y = 0), inner radius `inner` (0..1).
fn ring_mesh(inner: f32) -> MeshData {
    let n = 48;
    let mut m = Mesh::new(0);
    for i in 0..n {
        let a0 = i as f32 / n as f32 * std::f32::consts::TAU;
        let a1 = (i + 1) as f32 / n as f32 * std::f32::consts::TAU;
        let (o0, o1) = (Vec3::new(a0.cos(), 0.0, a0.sin()) * 0.5, Vec3::new(a1.cos(), 0.0, a1.sin()) * 0.5);
        let (i0, i1) = (o0 * inner, o1 * inner);
        m.quad(i0, i1, o1, o0, [1.0; 3]);
    }
    for n in &mut m.normals {
        *n = [0.0, 1.0, 0.0];
    }
    mesh_data(&m)
}

fn item_pos(item: &DrawItem) -> Option<(Vec3, Quat)> {
    match item {
        DrawItem::Prim { pos, rot, .. } | DrawItem::Mesh { pos, rot, .. } | DrawItem::Dynamic { pos, rot, .. } => {
            Some((*pos, *rot))
        }
        DrawItem::Decal { center, .. } => Some((*center, Quat::IDENTITY)),
        DrawItem::Light { pos, .. } | DrawItem::Text { pos, .. } => Some((*pos, Quat::IDENTITY)),
        _ => None,
    }
}

fn same_kind(a: &DrawItem, b: &DrawItem) -> bool {
    match (a, b) {
        (DrawItem::Mesh { mesh: x, .. }, DrawItem::Mesh { mesh: y, .. }) => x.id == y.id,
        (DrawItem::Dynamic { mesh: x, .. }, DrawItem::Dynamic { mesh: y, .. }) => x.id == y.id,
        _ => std::mem::discriminant(a) == std::mem::discriminant(b),
    }
}

impl PlayView {
    /// Adds the play frame's draw list to the scene.
    pub fn emit(&mut self, scene: &mut Scene, prev: Option<&PlayFrame>, curr: &PlayFrame, alpha: f32) {
        self.frames += 1;
        let prev_items = prev.map(|p| &p.draw.items[..]).unwrap_or(&[]);
        for (i, item) in curr.draw.items.iter().enumerate() {
            // Interpolate the placement from the previous frame's matching item.
            let (mut pos, mut rot) = item_pos(item).unwrap_or((Vec3::ZERO, Quat::IDENTITY));
            if let Some(p) = prev_items.get(i).filter(|p| same_kind(p, item))
                && let (Some((pp, pr)), true) = (item_pos(p), item_pos(item).is_some())
                && pp.distance(pos) < 12.0
            {
                pos = pp.lerp(pos, alpha);
                rot = pr.slerp(rot, alpha);
            }
            self.emit_item(scene, item, pos, rot);
        }
        // Forget meshes unused for a while.
        if self.frames.is_multiple_of(600) {
            let now = self.frames;
            self.used.retain(|_, t| now - *t < 600);
            let used = &self.used;
            self.meshes.retain(|k, _| used.contains_key(k));
        }
    }

    fn mesh_key(&mut self, m: &Arc<Mesh>) -> (MeshKey, Arc<MeshData>) {
        self.used.insert(m.id, self.frames);
        let data = self.meshes.entry(m.id).or_insert_with(|| Arc::new(mesh_data(m))).clone();
        (MeshKey::Custom(m.id), data)
    }

    fn emit_item(&mut self, scene: &mut Scene, item: &DrawItem, pos: Vec3, rot: Quat) {
        match item {
            DrawItem::Prim { shape, color, emissive, look, .. } => {
                let style = match look {
                    pav_core::Look::Flat | pav_core::Look::Cutout => Style::Flat,
                    pav_core::Look::Cel => Style::Cel,
                    pav_core::Look::Lit => Style::Lit,
                    pav_core::Look::Unlit => Style::Unlit,
                };
                crate::build::emit_shape(
                    &mut scene.meshes,
                    &mut scene.sdfs,
                    &mut scene.point_lights,
                    shape,
                    pos,
                    rot,
                    v3(*color),
                    *emissive,
                    style,
                    2,
                    rflags::OBJECT,
                    0,
                );
            }
            DrawItem::Mesh { mesh, scale, color, emissive, .. } => {
                let (key, data) = self.mesh_key(mesh);
                if !scene.custom_meshes.iter().any(|(k, _)| *k == key) {
                    scene.custom_meshes.push((key, data));
                }
                let cull = match mesh.cull {
                    pav_core::play::Cull::Always => 0,
                    pav_core::play::Cull::Small => rflags::CULL_SMALL,
                    pav_core::play::Cull::Vegetation => rflags::CULL_VEGETATION | rflags::SWAY,
                    pav_core::play::Cull::Structure => rflags::CULL_STRUCTURE,
                };
                scene.meshes.push(MeshInstance {
                    mesh: key,
                    transform: Mat4::from_scale_rotation_translation(*scale, rot, pos),
                    color: v3(*color),
                    emissive: *emissive,
                    style: Style::Lit,
                    flags: rflags::OBJECT | cull,
                    group: 3 + (mesh.id as u32 & 0xffff),
                    material: 0,
                    ext: Vec3::ZERO,
                });
            }
            DrawItem::Dynamic { mesh, color, two_sided, .. } => {
                let mut data = mesh_data(mesh);
                let m = Mat4::from_rotation_translation(rot, pos);
                for v in &mut data.vertices {
                    v.pos = m.transform_point3(Vec3::from(v.pos)).to_array();
                    v.normal = (rot * Vec3::from(v.normal)).to_array();
                }
                scene.dynamic.push(rs::DynamicMesh {
                    data,
                    color: v3(*color),
                    emissive: 0.0,
                    style: Style::Lit,
                    flags: rflags::OBJECT | if *two_sided { rflags::TWO_SIDED } else { 0 },
                    group: 3 + (mesh.id as u32 & 0xffff),
                    material: 0,
                });
            }
            DrawItem::Decal { radius, ring, color, alpha, .. } => {
                let q = (ring.clamp(0.0, 0.98) * 50.0).round() as u32;
                let key = MeshKey::Custom(0x5ec_a1_0000 + q as u64);
                let data = self.rings.entry(q).or_insert_with(|| Arc::new(ring_mesh(q as f32 / 50.0))).clone();
                if !scene.custom_meshes.iter().any(|(k, _)| *k == key) {
                    scene.custom_meshes.push((key, data));
                }
                scene.meshes.push(MeshInstance {
                    mesh: key,
                    transform: Mat4::from_scale_rotation_translation(
                        Vec3::new(radius * 2.0, 1.0, radius * 2.0),
                        Quat::IDENTITY,
                        pos + Vec3::Y * 0.06,
                    ),
                    color: v3(*color),
                    emissive: 0.6 * alpha,
                    style: Style::Unlit,
                    flags: rflags::NO_SHADOW | rflags::NO_CUT | rflags::NO_RECEIVE_SHADOW,
                    group: 0,
                    material: 0,
                    ext: Vec3::ZERO,
                });
            }
            DrawItem::Line { a, b, radius, color, emissive } => {
                let d = *b - *a;
                let len = d.length();
                if len < 1e-4 {
                    return;
                }
                let rot = Quat::from_rotation_arc(Vec3::Y, d / len);
                scene.meshes.push(MeshInstance {
                    mesh: MeshKey::Cylinder,
                    transform: Mat4::from_scale_rotation_translation(
                        Vec3::new(radius * 2.0, len, radius * 2.0),
                        rot,
                        (*a + *b) * 0.5,
                    ),
                    color: v3(*color),
                    emissive: *emissive,
                    style: Style::Lit,
                    flags: rflags::OBJECT | rflags::NO_SHADOW,
                    group: 2,
                    material: 0,
                    ext: Vec3::ZERO,
                });
            }
            DrawItem::Highlight { .. } => {
                // Drawn by `highlights` over the objects (it needs the entity's instances).
            }
            DrawItem::Light { color, intensity, radius, .. } => {
                scene.point_lights.push(rs::PointLight {
                    position: pos,
                    color: v3(*color) * *intensity,
                    radius: *radius,
                    shadows: false,
                });
            }
            DrawItem::Text { text, size, color, yaw, .. } => {
                let y = yaw.to_radians();
                scene.texts.push(pav_render::text::Text3d {
                    text: text.clone(),
                    origin: pos,
                    right: Vec3::new(y.cos(), 0.0, y.sin()),
                    up: Vec3::Y,
                    size: *size,
                    color: v3(*color),
                    anchor: pav_render::text::Anchor::Center,
                    flags: 0,
                    weight: 0.0,
                });
            }
        }
    }
}

/// Gull Sight: the entities a [`DrawItem::Highlight`] names are drawn through walls in its
/// colour (their instances carry the entity's outline group, `id + 2`).
pub fn highlights(scene: &mut Scene, play: &PlayFrame) {
    let marks: Vec<(u32, Vec3)> = play
        .draw
        .items
        .iter()
        .filter_map(|i| match i {
            DrawItem::Highlight { entity, color } => Some((entity.0 + 2, v3(*color))),
            _ => None,
        })
        .collect();
    if marks.is_empty() {
        return;
    }
    let tint = |group: u32, flags: &mut u32, color: &mut Vec3| {
        if let Some((_, c)) = marks.iter().find(|(g, _)| *g == group) {
            *flags |= rflags::XRAY | rflags::NO_CUT;
            *color = color.lerp(*c, 0.7);
        }
    };
    for m in &mut scene.meshes {
        tint(m.group, &mut m.flags, &mut m.color);
    }
    for s in &mut scene.sdfs {
        tint(s.group, &mut s.flags, &mut s.color);
    }
    for d in &mut scene.dynamic {
        tint(d.group, &mut d.flags, &mut d.color);
    }
}

/// The parts of an `Atmosphere` the base renderer understands (sun, ambient, clear colour,
/// fog, bloom and grade). The sky, ocean and rain passes are applied by `atmos` on top.
pub fn apply_basic_atmosphere(a: &pav_core::play::Atmosphere, scene: &mut Scene, target: Vec3) {
    scene.sun.direction = a.sun_direction();
    let flash = 1.0 + a.lightning * 1.5;
    scene.sun.color = v3(a.sun_color) * a.sun_intensity * flash;
    scene.sun.shadow_center = target;
    scene.sun.shadow_radius = a.shadow_radius;
    scene.ambient.sky = v3(a.ambient_sky) * a.ambient * (1.0 + a.lightning);
    scene.ambient.ground = v3(a.ambient_ground) * a.ambient;
    scene.clear_color = v3(a.sky_horizon);
    scene.fog = rs::Fog { enabled: true, center: target, start: a.fog_start, end: a.fog_end, color: v3(a.fog_color) };
    scene.post.bloom = a.bloom;
    scene.post.exposure = a.exposure;
    scene.post.saturation = a.saturation;
    let w = a.wind;
    let len = w.length();
    if len > 0.01 {
        scene.wind.direction = w / len;
        scene.wind.strength = (len / 12.0).clamp(0.0, 1.5);
    }
}

/// Camera shake offset for a strength (deterministic in time).
pub fn shake_offset(strength: f32, time: f32) -> Vec3 {
    if strength <= 0.0 {
        return Vec3::ZERO;
    }
    let s = strength.min(1.0) * 0.25;
    Vec3::new((time * 37.0).sin() * s, (time * 53.0).sin() * s * 0.7, (time * 41.0).cos() * s)
}
