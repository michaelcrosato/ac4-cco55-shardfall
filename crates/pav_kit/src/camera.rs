//! The camera director: a third-person orbit the game owns (yaw and pitch turn with
//! `InputFrame::look`, so replays include the camera), with per-context presets (on foot, at
//! the helm, climbing, aiming...), a spring arm that keeps the eye out of walls, auto-recentre
//! behind the player, and an eye that never dips under the sea.

use std::collections::BTreeMap;

use glam::{Vec2, Vec3};
use pav_core::entity::EntityId;
use pav_core::play::{CameraHint, orbit_offset};
use pav_core::sim::Sim;
use serde::{Deserialize, Serialize};

/// One camera context's numbers (`camera.toml`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CameraContext {
    /// Arm length (m).
    pub distance: f32,
    /// Default pitch (degrees, > 0 looks down) and limits.
    pub pitch: f32,
    pub min_pitch: f32,
    pub max_pitch: f32,
    pub fov: f32,
    /// Sideways offset of the eye (m, + = right).
    pub shoulder: f32,
    /// Pivot height above the target (m).
    pub height: f32,
    /// Seconds for the pivot to catch up (0 = locked).
    pub lag: f32,
    pub far: f32,
    /// Recentre behind the movement after this many seconds without look input (0 = never).
    pub recenter: f32,
}

impl Default for CameraContext {
    fn default() -> Self {
        Self {
            distance: 5.5,
            pitch: 12.0,
            min_pitch: -20.0,
            max_pitch: 70.0,
            fov: 60.0,
            shoulder: 0.45,
            height: 1.6,
            lag: 0.08,
            far: 1800.0,
            recenter: 2.5,
        }
    }
}

/// Every context by name, from data.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CameraContexts(pub BTreeMap<String, CameraContext>);

impl CameraContexts {
    pub fn parse(text: &str) -> anyhow::Result<Self> {
        Ok(Self(toml::from_str(text)?))
    }

    /// The named context, or the default one.
    pub fn get(&self, name: &str) -> CameraContext {
        self.0.get(name).cloned().unwrap_or_default()
    }
}

/// The live camera (game state: serde, cloned with snapshots).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OrbitCamera {
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
    pub fov: f32,
    pub shoulder: f32,
    pub height: f32,
    pub far: f32,
    /// Smoothed pivot (target + height).
    pub pivot: Vec3,
    /// Current arm length after collisions.
    pub arm: f32,
    /// Seconds since the last look input.
    pub idle_look: f32,
    /// The context in effect.
    pub context: String,
    /// The next hint is a hard cut.
    pub cut: bool,
    started: bool,
}

impl Default for OrbitCamera {
    fn default() -> Self {
        let c = CameraContext::default();
        Self {
            yaw: 0.0,
            pitch: c.pitch,
            distance: c.distance,
            fov: c.fov,
            shoulder: c.shoulder,
            height: c.height,
            far: c.far,
            pivot: Vec3::ZERO,
            arm: c.distance,
            idle_look: 0.0,
            context: String::new(),
            cut: true,
            started: false,
        }
    }
}

/// What the camera must not go through or under.
pub struct Surroundings<'a> {
    pub sim: &'a Sim,
    /// Ignored by the spring arm (the player, the ship you stand on).
    pub exclude: Option<EntityId>,
    /// Sea height under a point, if there is a sea.
    pub sea: Option<&'a dyn Fn(Vec2) -> f32>,
}

fn approach(x: f32, to: f32, rate: f32, dt: f32) -> f32 {
    x + (to - x) * (1.0 - (-rate * dt).exp())
}

/// Shortest signed difference between two angles (degrees).
pub fn angle_diff(to: f32, from: f32) -> f32 {
    (to - from + 540.0).rem_euclid(360.0) - 180.0
}

impl OrbitCamera {
    /// Switches context (blends its numbers over the next frames).
    pub fn set_context(&mut self, name: &str, ctx: &CameraContext, snap: bool) {
        if self.context != name || snap {
            self.context = name.to_string();
            if snap || !self.started {
                self.distance = ctx.distance;
                self.fov = ctx.fov;
                self.shoulder = ctx.shoulder;
                self.height = ctx.height;
                self.pitch = ctx.pitch;
                self.far = ctx.far;
                self.cut = true;
            }
        }
    }

    /// Faces the camera along a heading (degrees, 0 = north) at the context's pitch.
    pub fn face(&mut self, yaw: f32, ctx: &CameraContext) {
        self.yaw = yaw;
        self.pitch = ctx.pitch;
        self.cut = true;
    }

    /// One tick: turn with `look` (degrees), follow `target`, recentre behind `heading` (the
    /// way the player moves, degrees) when idle, keep the eye clear.
    pub fn update(
        &mut self,
        target: Vec3,
        look: Vec2,
        heading: Option<f32>,
        ctx: &CameraContext,
        around: &Surroundings,
        dt: f32,
    ) {
        if look != Vec2::ZERO {
            self.idle_look = 0.0;
        } else {
            self.idle_look += dt;
        }
        self.yaw = (self.yaw + look.x).rem_euclid(360.0);
        self.pitch = (self.pitch - look.y).clamp(ctx.min_pitch, ctx.max_pitch);
        if let Some(h) = heading.filter(|_| ctx.recenter > 0.0 && self.idle_look > ctx.recenter) {
            self.yaw = (self.yaw + angle_diff(h, self.yaw) * (1.0 - (-1.6 * dt).exp())).rem_euclid(360.0);
            self.pitch = approach(self.pitch, ctx.pitch, 1.2, dt);
        }
        self.distance = approach(self.distance, ctx.distance, 4.0, dt);
        self.fov = approach(self.fov, ctx.fov, 4.0, dt);
        self.shoulder = approach(self.shoulder, ctx.shoulder, 4.0, dt);
        self.height = approach(self.height, ctx.height, 4.0, dt);
        self.far = ctx.far;
        let want = target + Vec3::Y * self.height;
        if !self.started || self.cut {
            self.pivot = want;
            self.started = true;
        } else if ctx.lag > 0.0 {
            let k = 1.0 - (-dt / ctx.lag).exp();
            self.pivot += (want - self.pivot) * k;
            // Never trail too far behind something fast (a ship at full sail).
            let gap = want - self.pivot;
            if gap.length() > 3.0 {
                self.pivot = want - gap.normalize() * 3.0;
            }
        } else {
            self.pivot = want;
        }
        // Spring arm: pull in fast when something is in the way, ease back out.
        let ideal = self.ideal_eye();
        let dir = ideal - self.pivot;
        let len = dir.length().max(1e-3);
        let mut clear = len;
        if let Some(hit) = around.sim.raycast(self.pivot, dir / len, len + 0.25, around.exclude) {
            if hit.entity.is_none() {
                clear = ((hit.point - self.pivot).length() - 0.25).max(0.6);
            }
        }
        self.arm = if clear < self.arm || self.cut { clear } else { approach(self.arm, clear, 3.0, dt) };
    }

    fn ideal_eye(&self) -> Vec3 {
        let y = self.yaw.to_radians();
        let right = Vec3::new(y.cos(), 0.0, y.sin());
        self.pivot + orbit_offset(self.yaw, self.pitch, self.distance) + right * self.shoulder
    }

    /// The camera for this frame. `sea` keeps the eye 0.6 m above the water.
    pub fn hint(&mut self, sea: Option<&dyn Fn(Vec2) -> f32>) -> CameraHint {
        let ideal = self.ideal_eye();
        let dir = (ideal - self.pivot).normalize_or(Vec3::Z);
        let mut eye = self.pivot + dir * self.arm.min(self.distance + 0.5);
        if let Some(h) = sea {
            eye.y = eye.y.max(h(Vec2::new(eye.x, eye.z)) + 0.6);
        }
        let y = self.yaw.to_radians();
        let right = Vec3::new(y.cos(), 0.0, y.sin());
        let target = self.pivot + right * self.shoulder * 0.5;
        let cut = std::mem::take(&mut self.cut);
        CameraHint { eye, target, fov: self.fov, far: self.far, cut, fade_occluders: true }
    }

    /// The hint without changing state (read-only callers).
    pub fn peek(&self, sea: Option<&dyn Fn(Vec2) -> f32>) -> CameraHint {
        let mut c = self.clone();
        c.hint(sea)
    }

    /// Horizontal forward of the camera (x, z).
    pub fn forward(&self) -> Vec2 {
        let y = self.yaw.to_radians();
        Vec2::new(y.sin(), -y.cos())
    }

    /// A stick/keys axis (x right, y forward) in world space for this camera.
    pub fn relative(&self, axis: Vec2) -> Vec2 {
        let f = self.forward();
        let r = Vec2::new(-f.y, f.x);
        f * axis.y + r * axis.x
    }
}

/// A scripted shot: eye and target over time (cinematics, the title orbit, attract cuts).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Shot {
    pub eye: Vec3,
    pub target: Vec3,
    pub fov: f32,
}

/// A slow orbit around a point (title screen, survey).
pub fn orbit_shot(center: Vec3, radius: f32, height: f32, deg_per_s: f32, t: f32, fov: f32) -> CameraHint {
    let a = (t * deg_per_s).to_radians();
    let eye = center + Vec3::new(a.sin() * radius, height, a.cos() * radius);
    CameraHint { eye, target: center, fov, far: 1800.0, cut: false, fade_occluders: false }
}
