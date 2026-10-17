//! The sea: a deterministic sum of Gerstner waves, `f(x, z, t, sea_state)`. Swimming,
//! buoyancy, ships and cannonballs read it on the CPU; the renderer displaces its ocean mesh
//! with the same parameters, so what you see is what the simulation feels.
//!
//! Six wave slots with fixed wavelengths and directions (relative to the wind) are shared by
//! every sea state: states differ only in amplitude and steepness, so blending two states over
//! time never makes the waves jump.

use glam::{Vec2, Vec3};
use serde::{Deserialize, Serialize};

/// Gravity for the deep-water dispersion relation (m/s²).
const G: f32 = 9.81;

/// One Gerstner wave.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Wave {
    /// Unit direction of travel in the XZ plane (x east, y = world z south).
    pub dir: Vec2,
    /// Crest-to-crest length (m).
    pub wavelength: f32,
    /// Crest height above the mean (m).
    pub amplitude: f32,
    /// 0 = sine waves, 1 = sharpest crests (Gerstner Q; normalised so crests never loop).
    pub steepness: f32,
    /// Phase offset (radians), so slots don't all crest at the origin.
    pub phase: f32,
}

impl Wave {
    fn k(&self) -> f32 {
        std::f32::consts::TAU / self.wavelength.max(0.1)
    }
    fn omega(&self) -> f32 {
        (G * self.k()).sqrt()
    }
}

/// The waves on the sea right now. Lives in the game's state (set every tick), so every system
/// reads the same surface.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct SeaState {
    pub waves: [Wave; 6],
    /// Small ripples drawn by the renderer only (normal-map strength, 0..1).
    pub detail: f32,
    /// Foam on crests (renderer only, 0..1).
    pub foam: f32,
}

/// Wavelengths (m) and direction offsets from the wind (degrees) of the six slots.
const SLOTS: [(f32, f32, f32); 6] =
    [(64.0, 0.0, 0.3), (41.0, 23.0, 2.1), (27.0, -31.0, 4.4), (17.0, 47.0, 1.2), (10.5, -12.0, 5.3), (6.2, 66.0, 3.7)];

/// Named sea states.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SeaPreset {
    Flat,
    Calm,
    Breezy,
    Rough,
    Storm,
}

impl SeaPreset {
    pub const ALL: [SeaPreset; 5] = [SeaPreset::Flat, SeaPreset::Calm, SeaPreset::Breezy, SeaPreset::Rough, SeaPreset::Storm];

    pub fn name(self) -> &'static str {
        match self {
            SeaPreset::Flat => "flat",
            SeaPreset::Calm => "calm",
            SeaPreset::Breezy => "breezy",
            SeaPreset::Rough => "rough",
            SeaPreset::Storm => "storm",
        }
    }

    pub fn from_name(n: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.name() == n)
    }
}

impl SeaState {
    /// A preset with its waves turned to the wind (`wind_dir_deg`: the direction the wind
    /// blows toward, degrees clockwise from +X seen from above... measured as atan2(z, x)).
    pub fn preset(p: SeaPreset, wind_dir_deg: f32) -> Self {
        // Amplitudes per slot (m) and steepness.
        let (amps, steep, detail, foam): ([f32; 6], f32, f32, f32) = match p {
            SeaPreset::Flat => ([0.0; 6], 0.0, 0.1, 0.0),
            SeaPreset::Calm => ([0.14, 0.09, 0.06, 0.035, 0.02, 0.012], 0.35, 0.35, 0.05),
            SeaPreset::Breezy => ([0.32, 0.2, 0.13, 0.08, 0.045, 0.025], 0.5, 0.55, 0.2),
            SeaPreset::Rough => ([0.75, 0.5, 0.3, 0.17, 0.09, 0.05], 0.6, 0.75, 0.45),
            SeaPreset::Storm => ([1.05, 0.72, 0.45, 0.26, 0.13, 0.07], 0.7, 1.0, 0.8),
        };
        let mut waves = [Wave { dir: Vec2::X, wavelength: 1.0, amplitude: 0.0, steepness: 0.0, phase: 0.0 }; 6];
        for (i, (len, off, phase)) in SLOTS.iter().enumerate() {
            let a = (wind_dir_deg + off).to_radians();
            waves[i] = Wave { dir: Vec2::new(a.cos(), a.sin()), wavelength: *len, amplitude: amps[i], steepness: steep, phase: *phase };
        }
        Self { waves, detail, foam }
    }

    /// Linear blend from `a` (t = 0) to `b` (t = 1). Wavelengths and directions come from `a`
    /// when both share the wind (presets always do), so the result is continuous in t.
    pub fn blend(a: &SeaState, b: &SeaState, t: f32) -> SeaState {
        let t = t.clamp(0.0, 1.0);
        let mut out = *a;
        for i in 0..6 {
            let (wa, wb) = (a.waves[i], b.waves[i]);
            out.waves[i].amplitude = wa.amplitude + (wb.amplitude - wa.amplitude) * t;
            out.waves[i].steepness = wa.steepness + (wb.steepness - wa.steepness) * t;
            out.waves[i].dir = wa.dir.lerp(wb.dir, t).normalize_or(wa.dir);
        }
        out.detail = a.detail + (b.detail - a.detail) * t;
        out.foam = a.foam + (b.foam - a.foam) * t;
        out
    }

    /// The largest possible crest height (sum of amplitudes).
    pub fn max_height(&self) -> f32 {
        self.waves.iter().map(|w| w.amplitude).sum()
    }

    /// Gerstner Q per slot, scaled so the sum of Q·A·k stays ≤ 0.95 (no looping crests).
    fn q(&self) -> [f32; 6] {
        let total: f32 = self.waves.iter().map(|w| w.steepness * w.amplitude * w.k()).sum();
        let scale = if total > 0.95 { 0.95 / total } else { 1.0 };
        let mut q = [0.0; 6];
        for (i, w) in self.waves.iter().enumerate() {
            q[i] = w.steepness * scale;
        }
        q
    }

    /// Where the water particle that rests at `p0` (x, z) is at time `t`: (dx, height, dz).
    pub fn displacement(&self, p0: Vec2, t: f32) -> Vec3 {
        let q = self.q();
        let mut d = Vec3::ZERO;
        for (i, w) in self.waves.iter().enumerate() {
            if w.amplitude == 0.0 {
                continue;
            }
            let k = w.k();
            let ph = k * w.dir.dot(p0) - w.omega() * t + w.phase;
            let (s, c) = ph.sin_cos();
            d.x += q[i] * w.amplitude * w.dir.x * c;
            d.z += q[i] * w.amplitude * w.dir.y * c;
            d.y += w.amplitude * s;
        }
        d
    }

    /// The rest position whose displaced particle lands on `p` (x, z): inverts the horizontal
    /// shift with fixed-point iterations.
    fn rest_of(&self, p: Vec2, t: f32) -> Vec2 {
        let mut p0 = p;
        for _ in 0..3 {
            let d = self.displacement(p0, t);
            p0 = p - Vec2::new(d.x, d.z);
        }
        p0
    }

    /// Water height (y) at the world point `p` (x, z) at time `t`.
    pub fn height(&self, p: Vec2, t: f32) -> f32 {
        if self.max_height() == 0.0 {
            return 0.0;
        }
        self.displacement(self.rest_of(p, t), t).y
    }

    /// Surface height, normal and water velocity at `p`.
    pub fn sample(&self, p: Vec2, t: f32) -> Surface {
        if self.max_height() == 0.0 {
            return Surface { height: 0.0, normal: Vec3::Y, velocity: Vec3::ZERO };
        }
        let p0 = self.rest_of(p, t);
        let q = self.q();
        let (mut nx, mut ny, mut nz) = (0.0, 1.0, 0.0);
        let mut h = 0.0;
        let mut vel = Vec3::ZERO;
        for (i, w) in self.waves.iter().enumerate() {
            if w.amplitude == 0.0 {
                continue;
            }
            let k = w.k();
            let om = w.omega();
            let ph = k * w.dir.dot(p0) - om * t + w.phase;
            let (s, c) = ph.sin_cos();
            let wa = k * w.amplitude;
            // Normal of a Gerstner surface (GPU Gems 1, ch. 1), evaluated at the rest point.
            nx -= w.dir.x * wa * c;
            nz -= w.dir.y * wa * c;
            ny -= q[i] * wa * s;
            h += w.amplitude * s;
            vel.x += q[i] * w.amplitude * w.dir.x * om * s;
            vel.z += q[i] * w.amplitude * w.dir.y * om * s;
            vel.y -= w.amplitude * om * c;
        }
        Surface { height: h, normal: Vec3::new(nx, ny, nz).normalize_or(Vec3::Y), velocity: vel }
    }
}

impl Default for SeaState {
    fn default() -> Self {
        Self::preset(SeaPreset::Calm, 45.0)
    }
}

/// The sea surface at a point.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Surface {
    pub height: f32,
    pub normal: Vec3,
    pub velocity: Vec3,
}

/// Seabed depth over an area, for the renderer's shallows and for anything that asks how deep
/// the water is (grounding, swimming). Heights are metres (negative below sea level) on a grid.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Seabed {
    /// World x, z of cell (0, 0)'s corner.
    pub origin: Vec2,
    /// Cell size (m).
    pub cell: f32,
    pub width: usize,
    pub depth: usize,
    /// Ground height per cell, row-major (z rows); `deep` outside.
    pub heights: Vec<f32>,
    /// Height of the open-sea floor outside the grid.
    pub deep: f32,
}

impl Seabed {
    /// Ground height at `p` (bilinear), `deep` outside the grid.
    pub fn height(&self, p: Vec2) -> f32 {
        if self.width < 2 || self.depth < 2 || self.cell <= 0.0 {
            return self.deep;
        }
        let g = (p - self.origin) / self.cell - Vec2::splat(0.5);
        if g.x < 0.0 || g.y < 0.0 || g.x >= (self.width - 1) as f32 || g.y >= (self.depth - 1) as f32 {
            return self.deep;
        }
        let (x0, z0) = (g.x as usize, g.y as usize);
        let (fx, fz) = (g.x - x0 as f32, g.y - z0 as f32);
        let at = |x: usize, z: usize| self.heights[z * self.width + x];
        let a = at(x0, z0) + (at(x0 + 1, z0) - at(x0, z0)) * fx;
        let b = at(x0, z0 + 1) + (at(x0 + 1, z0 + 1) - at(x0, z0 + 1)) * fx;
        a + (b - a) * fz
    }
}
