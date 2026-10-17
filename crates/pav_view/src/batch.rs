//! Static batching for hook games (PLAN §9.18): a region's plain-coloured static pieces (no
//! material, no glow, no sway) are merged into one mesh per style, flags and outline group, so
//! a town of a few thousand boxes and parts submits a few dozen instances. Each merged mesh is
//! in world space with the pieces' colours in its vertices; it draws exactly as its pieces did
//! (same style, outline group and distance class; mirrored pieces keep their winding).

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use glam::{Mat3, Mat4, Vec3};
use pav_render::mesh::{MeshData, MeshKey, Vertex};
use pav_render::scene::{MeshInstance, flags};

/// Most vertices in one merged mesh (a bigger group is split).
const MAX_VERTICES: usize = 60_000;

/// Whether an instance can join a merged mesh.
fn mergeable(m: &MeshInstance) -> bool {
    let animated = flags::SWAY | flags::GRASS | flags::XRAY | flags::OBJECT | flags::TWO_SIDED | flags::PIXEL;
    m.material == 0 && m.emissive == 0.0 && m.flags & animated == 0
}

/// Merges the mergeable instances of `meshes` (groups of two or more) into world-space meshes:
/// they leave `meshes`, one instance per merged mesh joins it, and the merged meshes are
/// returned to be registered as custom meshes (`MeshKey::Custom` keys derived from `salt`).
/// `custom` holds the data of the custom meshes the instances use.
pub fn merge_statics(meshes: &mut Vec<MeshInstance>, custom: &[(u64, Arc<MeshData>)], salt: u64) -> Vec<(u64, Arc<MeshData>)> {
    let mut built: HashMap<MeshKey, Option<Arc<MeshData>>> = HashMap::new();
    let mut data_of = |key: MeshKey| -> Option<Arc<MeshData>> {
        built
            .entry(key)
            .or_insert_with(|| match key {
                MeshKey::Custom(k) => custom.iter().find(|(c, _)| *c == k).map(|(_, d)| d.clone()),
                k => k.generate().map(Arc::new),
            })
            .clone()
    };
    // Groups in a fixed order (style, flags, outline group), instances in list order.
    let mut groups: BTreeMap<(u32, u32, u32), Vec<usize>> = BTreeMap::new();
    for (i, m) in meshes.iter().enumerate() {
        if mergeable(m) && data_of(m.mesh).is_some() {
            groups.entry((m.style as u32, m.flags, m.group)).or_default().push(i);
        }
    }
    let mut taken = vec![false; meshes.len()];
    let mut merged_instances = Vec::new();
    let mut out = Vec::new();
    for members in groups.values().filter(|g| g.len() >= 2) {
        let mut data = MeshData::default();
        let mut flush = |data: &mut MeshData, first: &MeshInstance| {
            if data.indices.is_empty() {
                return;
            }
            let key = mix(salt ^ mix(out.len() as u64 + 1));
            out.push((key, Arc::new(std::mem::take(data))));
            merged_instances.push(MeshInstance {
                mesh: MeshKey::Custom(key),
                transform: Mat4::IDENTITY,
                color: Vec3::ONE,
                emissive: 0.0,
                // World-space meshes lower per vertex under a height cut.
                flags: first.flags | flags::CUT_VERTEX,
                ext: Vec3::ZERO,
                ..*first
            });
        };
        let first = meshes[members[0]];
        for &i in members {
            let m = &meshes[i];
            let src = data_of(m.mesh).expect("checked above");
            if data.vertices.len() + src.vertices.len() > MAX_VERTICES {
                flush(&mut data, &first);
            }
            append(&mut data, &src, &m.transform, m.color);
            taken[i] = true;
        }
        flush(&mut data, &first);
    }
    let mut k = 0;
    meshes.retain(|_| {
        k += 1;
        !taken[k - 1]
    });
    meshes.extend(merged_instances);
    out
}

/// Appends `src` placed by `t` and tinted `color` (vertex colour times instance colour).
fn append(dst: &mut MeshData, src: &MeshData, t: &Mat4, color: Vec3) {
    let base = dst.vertices.len() as u32;
    let m = Mat3::from_mat4(*t);
    let (c0, c1, c2) = (m.x_axis, m.y_axis, m.z_axis);
    let mirrored = c0.cross(c1).dot(c2) < 0.0;
    // The cofactor matrix carries normals through any scale (as the shader does).
    let mut cof = Mat3::from_cols(c1.cross(c2), c2.cross(c0), c0.cross(c1));
    if mirrored {
        cof = -cof;
    }
    for v in &src.vertices {
        let pos = t.transform_point3(Vec3::from(v.pos));
        let n = (cof * Vec3::from(v.normal)).normalize_or(Vec3::Y);
        let c = v.color;
        dst.vertices.push(Vertex {
            pos: pos.to_array(),
            normal: n.to_array(),
            uv: v.uv,
            color: [c[0] * color.x, c[1] * color.y, c[2] * color.z, c[3]],
        });
    }
    for tri in src.indices.as_chunks::<3>().0 {
        if mirrored {
            dst.indices.extend([base + tri[0], base + tri[2], base + tri[1]]);
        } else {
            dst.indices.extend([base + tri[0], base + tri[1], base + tri[2]]);
        }
    }
}

/// A 64-bit mix (splitmix64's finaliser).
pub fn mix(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Quat;
    use pav_render::scene::Style;

    fn cube(at: Vec3, scale: Vec3, color: Vec3, group: u32) -> MeshInstance {
        MeshInstance {
            mesh: MeshKey::Cube,
            transform: Mat4::from_scale_rotation_translation(scale, Quat::from_rotation_y(0.3), at),
            color,
            emissive: 0.0,
            style: Style::Cel,
            flags: flags::CULL_STRUCTURE,
            group,
            material: 0,
            ext: Vec3::ZERO,
        }
    }

    #[test]
    fn plain_pieces_merge_per_group_and_the_rest_stay() {
        let mut list = vec![
            cube(Vec3::ZERO, Vec3::ONE, Vec3::X, 1),
            cube(Vec3::new(3.0, 0.0, 0.0), Vec3::new(-1.0, 2.0, 1.0), Vec3::Y, 1),
            cube(Vec3::new(6.0, 0.0, 0.0), Vec3::ONE, Vec3::Z, 7),
            MeshInstance { material: 5, ..cube(Vec3::new(9.0, 0.0, 0.0), Vec3::ONE, Vec3::ONE, 1) },
        ];
        let merged = merge_statics(&mut list, &[], 42);
        assert_eq!(merged.len(), 1, "group 1's two plain cubes; group 7 has one; the material stays");
        assert_eq!(list.len(), 3);
        let m = list.iter().find(|m| matches!(m.mesh, MeshKey::Custom(_))).expect("the merged instance");
        assert_eq!((m.group, m.transform, m.color), (1, Mat4::IDENTITY, Vec3::ONE));
        assert!(m.flags & flags::CUT_VERTEX != 0 && m.flags & flags::CULL_STRUCTURE != 0);
        let data = &merged[0].1;
        assert_eq!(data.vertices.len(), 48);
        // The first cube's vertices are where it stood, in its colour, normals unit length.
        let cube = MeshKey::Cube.generate().unwrap();
        let t = Mat4::from_scale_rotation_translation(Vec3::ONE, Quat::from_rotation_y(0.3), Vec3::ZERO);
        for (a, b) in data.vertices[..24].iter().zip(&cube.vertices) {
            assert!(Vec3::from(a.pos).distance(t.transform_point3(Vec3::from(b.pos))) < 1e-5);
            assert_eq!([a.color[0], a.color[1], a.color[2]], [1.0, 0.0, 0.0]);
            assert!((Vec3::from(a.normal).length() - 1.0).abs() < 1e-4);
        }
        // The mirrored cube's triangles still face outward (winding flipped back).
        for tri in data.indices[36..].as_chunks::<3>().0 {
            let p = |i: u32| Vec3::from(data.vertices[i as usize].pos);
            let face = (p(tri[1]) - p(tri[0])).cross(p(tri[2]) - p(tri[0]));
            let centre = (p(tri[0]) + p(tri[1]) + p(tri[2])) / 3.0;
            assert!(face.dot(centre - Vec3::new(3.0, 0.0, 0.0)) > 0.0, "outward");
        }
    }
}
