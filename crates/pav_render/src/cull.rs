//! CPU culling of mesh and SDF instances by bounding sphere: outside the view frustum, or past
//! the distance limit of their category (`flags::CULL_*`). Shadow casters are tested against
//! the volumes that receive shadows instead (the sun's shadow box, the shadowed lamps' reach),
//! so a caster just off screen still throws its shadow into view.

use glam::{Mat4, Vec3, Vec4};

use crate::scene::{Scene, flags};

/// The six planes of a view-projection's clip volume (depth 0..1), pointing inward.
#[derive(Clone, Copy, Debug)]
pub struct Planes([Vec4; 6]);

impl Planes {
    pub fn from_matrix(m: Mat4) -> Self {
        let (r0, r1, r2, r3) = (m.row(0), m.row(1), m.row(2), m.row(3));
        let mut p = [r3 + r0, r3 - r0, r3 + r1, r3 - r1, r2, r3 - r2];
        for q in &mut p {
            let l = q.truncate().length();
            if l > 0.0 {
                *q /= l;
            }
        }
        Self(p)
    }

    /// Whether a sphere touches the volume.
    pub fn sphere(&self, c: Vec3, r: f32) -> bool {
        self.0.iter().all(|p| p.truncate().dot(c) + p.w >= -r)
    }
}

/// What a culled instance missed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Verdict {
    /// Drawn in the main pass.
    pub draw: bool,
    /// Drawn into the shadow maps (before its own NO_SHADOW flag).
    pub shadow: bool,
    /// Removed by its distance limit (neither drawn nor shadowing).
    pub far: bool,
}

pub struct Culler {
    view: Option<Planes>,
    sun: Option<Planes>,
    lamps: Vec<(Vec3, f32)>,
    eye: Vec3,
    limits: [(u32, f32); 3],
    /// Extra reach of swaying instances: a fraction of their radius and metres.
    sway: (f32, f32),
}

impl Culler {
    /// `view_proj` the camera's (standard depth), `sun` the sun's shadow matrix when it casts
    /// shadows, `lamps` the shadow-casting point lights (position, radius).
    pub fn new(scene: &Scene, view_proj: Mat4, sun: Option<Mat4>, lamps: Vec<(Vec3, f32)>) -> Self {
        let on = scene.frustum_cull;
        let w = scene.wind.strength.max(0.0);
        Self {
            view: on.then(|| Planes::from_matrix(view_proj)),
            sun: if on { sun.map(Planes::from_matrix) } else { None },
            lamps: if on { lamps } else { Vec::new() },
            eye: scene.camera.eye,
            limits: [
                (flags::CULL_SMALL, scene.cull_small),
                (flags::CULL_VEGETATION, scene.cull_vegetation),
                (flags::CULL_STRUCTURE, scene.cull_structure),
            ],
            sway: (0.25 * w, 0.5 * w),
        }
    }

    /// The bounding sphere of a mesh instance whose mesh has the local sphere (`c`, `r`).
    pub fn mesh_sphere(transform: &Mat4, c: Vec3, r: f32) -> (Vec3, f32) {
        let s = transform
            .x_axis
            .truncate()
            .length()
            .max(transform.y_axis.truncate().length())
            .max(transform.z_axis.truncate().length());
        (transform.transform_point3(c), r * s)
    }

    pub fn test(&self, c: Vec3, r: f32, f: u32) -> Verdict {
        let r = if f & (flags::SWAY | flags::GRASS) != 0 { r * (1.0 + self.sway.0) + self.sway.1 } else { r };
        let limit = self.limits.iter().filter(|(bit, d)| f & bit != 0 && *d > 0.0).map(|(_, d)| *d).fold(f32::INFINITY, f32::min);
        if limit.is_finite() && c.distance(self.eye) - r > limit {
            return Verdict { draw: false, shadow: false, far: true };
        }
        let draw = self.view.as_ref().is_none_or(|p| p.sphere(c, r));
        let shadow = (self.sun.is_none() && self.lamps.is_empty() && self.view.is_none())
            || self.sun.as_ref().is_some_and(|p| p.sphere(c, r))
            || self.lamps.iter().any(|(p, reach)| p.distance(c) < reach + r);
        Verdict { draw, shadow, far: false }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::CameraData;

    #[test]
    fn spheres_in_and_out_of_view() {
        let cam = CameraData::default();
        let p = Planes::from_matrix(cam.proj * cam.view);
        // The default camera at (0, 10, 10) looks at the origin.
        assert!(p.sphere(Vec3::ZERO, 0.5));
        assert!(!p.sphere(Vec3::new(0.0, 10.0, 30.0), 1.0), "behind the camera");
        assert!(!p.sphere(Vec3::new(400.0, 0.0, 0.0), 1.0), "far to the side");
        assert!(p.sphere(Vec3::new(0.0, 10.0, 12.0), 2.5), "straddles the eye");
        assert!(!p.sphere(Vec3::new(0.0, -400.0, -400.0), 1.0), "past the far plane");
    }

    #[test]
    fn distance_limits_by_category() {
        let scene = Scene { cull_small: 50.0, ..Default::default() };
        let cam = scene.camera;
        let c = Culler::new(&scene, cam.proj * cam.view, None, Vec::new());
        let far = Vec3::new(0.0, 0.0, -60.0);
        assert!(c.test(far, 0.5, flags::CULL_SMALL).far);
        assert!(!c.test(far, 0.5, 0).far);
        assert!(!c.test(far, 0.5, flags::CULL_VEGETATION).far, "vegetation has no limit here");
    }
}
