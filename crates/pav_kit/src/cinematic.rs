//! Scripted scenes: shot lists (eye and target keys over time, field of view, easing,
//! letterbox) that the game plays through its camera hint, skippable by holding a button; a
//! skip lands in the same end state as watching.
//!
//! ```toml
//! [scene.fort_captured]
//! letterbox = true
//! [[scene.fort_captured.shot]]
//! at = 0.0
//! eye = [0, 4, 8]        # relative to the scene's anchor (the game picks it)
//! target = [0, 2, 0]
//! fov = 50
//! cut = true
//! ```

use std::collections::BTreeMap;

use glam::Vec3;
use pav_core::play::CameraHint;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ShotKey {
    /// Seconds from the scene's start.
    pub at: f32,
    pub eye: [f32; 3],
    pub target: [f32; 3],
    pub fov: f32,
    /// A hard cut to this key (else eased from the previous).
    pub cut: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SceneDef {
    pub shot: Vec<ShotKey>,
    pub letterbox: bool,
    /// Total length (s); default: the last key + 1.5 s.
    pub length: f32,
    /// Slow motion during the scene (1 = normal).
    pub time_scale: f32,
    /// Dialog lines started at given times: (seconds, line id).
    pub lines: Vec<(f32, String)>,
}

impl SceneDef {
    pub fn duration(&self) -> f32 {
        if self.length > 0.0 { self.length } else { self.shot.last().map(|k| k.at + 1.5).unwrap_or(2.0) }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Scenes {
    #[serde(default)]
    pub scene: BTreeMap<String, SceneDef>,
}

impl Scenes {
    pub fn parse(text: &str) -> anyhow::Result<Self> {
        Ok(toml::from_str(text)?)
    }
}

/// A scene playing (game state).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Playing {
    pub name: String,
    pub t: f32,
    /// World anchor and its yaw (degrees): keys are relative to it.
    pub anchor: Vec3,
    pub yaw: f32,
    /// Seconds the skip button has been held.
    pub skip: f32,
    /// Lines already started (indices into the scene's `lines`).
    pub said: usize,
}

fn ease(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

impl Playing {
    pub fn new(name: &str, anchor: Vec3, yaw: f32) -> Self {
        Self { name: name.into(), anchor, yaw, ..Default::default() }
    }

    fn world(&self, p: [f32; 3]) -> Vec3 {
        let y = self.yaw.to_radians();
        let (s, c) = y.sin_cos();
        // Local x right, z back toward the viewer (so a scene's keys turn with its anchor).
        let v = Vec3::from(p);
        self.anchor + Vec3::new(v.x * c - v.z * s, v.y, v.x * s + v.z * c)
    }

    /// Advances; returns line ids to start and whether the scene is over (finished or
    /// skipped after holding skip for 0.6 s).
    pub fn tick(&mut self, def: &SceneDef, dt: f32, skip_held: bool) -> (Vec<String>, bool) {
        self.t += dt;
        self.skip = if skip_held { self.skip + dt } else { 0.0 };
        let mut lines = Vec::new();
        while self.said < def.lines.len() && def.lines[self.said].0 <= self.t {
            lines.push(def.lines[self.said].1.clone());
            self.said += 1;
        }
        (lines, self.t >= def.duration() || self.skip >= 0.6)
    }

    /// The camera now.
    pub fn camera(&self, def: &SceneDef) -> Option<CameraHint> {
        let keys = &def.shot;
        let first = keys.first()?;
        let i = keys.iter().rposition(|k| k.at <= self.t).unwrap_or(0);
        let a = &keys[i];
        let (eye, target, fov, cut) = match keys.get(i + 1).filter(|b| !b.cut) {
            Some(b) => {
                let u = ease(((self.t - a.at) / (b.at - a.at).max(1e-3)).clamp(0.0, 1.0));
                let e = self.world(a.eye).lerp(self.world(b.eye), u);
                let tg = self.world(a.target).lerp(self.world(b.target), u);
                (e, tg, a.fov + (b.fov - a.fov) * u, false)
            }
            None => (self.world(a.eye), self.world(a.target), a.fov, false),
        };
        let just_cut = a.cut && (self.t - a.at) < 0.02 || (i == 0 && self.t < 0.02 && first.cut);
        Some(CameraHint { eye, target, fov: if fov > 1.0 { fov } else { 50.0 }, far: 1800.0, cut: just_cut || cut, fade_occluders: false })
    }
}
