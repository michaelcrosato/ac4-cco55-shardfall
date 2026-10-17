//! The FX bank: what a game's [`SimEvent::Cue`] looks like (particle bursts from the presets in
//! `fx.rs`, shockwaves, water dents, flashes of light, hit sparks), from data (`fx.toml`):
//!
//! ```toml
//! [cue.cannon_fire]
//! particles = [{ preset = "smoke", count = 30, spread = 3.0, size = 1.4, life = 1.6, up = 1.0 },
//!              { preset = "sparks", count = 20, color = "#ffd27a" }]
//! shock = 3.0        # a shockwave of this radius (m)
//! water = 1.5        # dents the water surface (m)
//! flash = { color = "#ffb060", intensity = 4.0, radius = 12.0, seconds = 0.12 }
//! ```
//! A particle layer may also lean with the game's wind (`wind = 0.6`: that share of the wind's
//! speed added to the launch, and the drag eased so it keeps drifting) and override the preset's
//! `drag`, `gravity` and `floor` (a height above the cue where falling particles land).
//! Cues without an entry show nothing (the sound bank may still play them).

use std::collections::BTreeMap;

use glam::{Vec3, Vec4};
use pav_render::scene::ParticleBurst;
use serde::Deserialize;

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct CueParticles {
    pub preset: String,
    pub count: u32,
    /// Multipliers (0 = the preset's).
    pub spread: f32,
    pub size: f32,
    pub life: f32,
    /// Upward launch speed (m/s).
    pub up: f32,
    /// Spawn box half-size (m, 0 = the preset's).
    pub area: f32,
    pub color: String,
    /// Height above the cue's position.
    pub offset: f32,
    /// Share of the wind's speed added to the launch velocity (smoke drifting downwind).
    pub wind: f32,
    /// Overrides of the preset (None = the preset's).
    pub drag: Option<f32>,
    pub gravity: Option<f32>,
    /// Falling particles land this far above the cue's position (splinters on a deck).
    pub floor: Option<f32>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub struct Flash {
    pub color: String,
    pub intensity: f32,
    pub radius: f32,
    pub seconds: f32,
}

impl Default for Flash {
    fn default() -> Self {
        Self { color: "#ffd27a".into(), intensity: 3.0, radius: 8.0, seconds: 0.15 }
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct CueFx {
    pub particles: Vec<CueParticles>,
    pub shock: f32,
    pub water: f32,
    pub flash: Option<Flash>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct FxBank {
    #[serde(default)]
    pub cue: BTreeMap<String, CueFx>,
}

impl FxBank {
    pub fn parse(text: &str) -> Result<Self, String> {
        toml::from_str(text).map_err(|e| e.to_string())
    }

    /// The bursts for a cue at `pos` (empty when unknown), in still air.
    pub fn bursts(&self, name: &str, pos: Vec3, gain: f32) -> Vec<ParticleBurst> {
        self.bursts_in_wind(name, pos, gain, glam::Vec2::ZERO)
    }

    /// The bursts for a cue at `pos` with the wind blowing (m/s, x and z).
    pub fn bursts_in_wind(&self, name: &str, pos: Vec3, gain: f32, wind: glam::Vec2) -> Vec<ParticleBurst> {
        let Some(fx) = self.cue.get(name) else { return Vec::new() };
        fx.particles
            .iter()
            .map(|p| {
                let (mut b, _) = crate::fx::preset(&p.preset);
                b.pos = pos + Vec3::Y * p.offset;
                b.count = ((if p.count > 0 { p.count } else { b.count.max(12) }) as f32 * gain.clamp(0.2, 2.0)) as u32;
                if p.spread > 0.0 {
                    b.spread *= p.spread;
                }
                if p.size > 0.0 {
                    b.size = (b.size.0 * p.size, b.size.1 * p.size);
                }
                if p.life > 0.0 {
                    b.life = (b.life.0 * p.life, b.life.1 * p.life);
                }
                if p.area > 0.0 {
                    b.area = Vec3::splat(p.area);
                }
                b.vel += Vec3::Y * p.up;
                if p.wind != 0.0 && wind != glam::Vec2::ZERO {
                    b.vel += Vec3::new(wind.x, 0.0, wind.y) * p.wind;
                    b.drag = b.drag.min(0.3);
                }
                if let Some(d) = p.drag {
                    b.drag = d.max(0.0);
                }
                if let Some(g) = p.gravity {
                    b.gravity = g;
                }
                if let Some(f) = p.floor {
                    b.floor = Some(pos.y + f);
                }
                if !p.color.is_empty() {
                    let c = pav_core::Color::hex(&p.color).0;
                    let glow = if b.additive { 2.0 } else { 1.0 };
                    b.color0 = Vec4::new(c[0] * glow, c[1] * glow, c[2] * glow, b.color0.w);
                    b.color1 = Vec4::new(c[0], c[1], c[2], b.color1.w);
                }
                b.prewarm = 0.0;
                b
            })
            .collect()
    }
}
