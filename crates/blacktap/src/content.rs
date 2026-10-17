//! The game's data, parsed from `games/blacktap/` (see `crate::data`): camera contexts,
//! atmosphere presets, the attract playlist and the front end's text.

use std::collections::BTreeMap;
use std::sync::Arc;

use anyhow::{Context as _, Result, anyhow};
use glam::Vec2;
use pav_core::Color;
use pav_core::ocean::{SeaPreset, SeaState};
use pav_core::play::{Atmosphere, DemoSegment};
use pav_kit::camera::CameraContexts;
use serde::Deserialize;

/// One atmosphere preset as written in `atmosphere.toml` (sRGB hex colours).
#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub struct AtmosphereDef {
    pub sun_azimuth: f32,
    pub sun_elevation: f32,
    pub sun_color: String,
    pub sun_intensity: f32,
    pub ambient_sky: String,
    pub ambient_ground: String,
    pub ambient: f32,
    pub sky_zenith: String,
    pub sky_horizon: String,
    pub fog_color: String,
    pub fog_start: f32,
    pub fog_end: f32,
    pub cloud_cover: f32,
    pub cloud_color: String,
    pub rain: f32,
    pub wetness: f32,
    pub wind: [f32; 2],
    pub exposure: f32,
    pub saturation: f32,
    pub contrast: f32,
    pub bloom: f32,
    pub sea: String,
}

impl Default for AtmosphereDef {
    fn default() -> Self {
        Self {
            sun_azimuth: 225.0,
            sun_elevation: 35.0,
            sun_color: "#fff1dc".into(),
            sun_intensity: 1.0,
            ambient_sky: "#9fc4e8".into(),
            ambient_ground: "#8a7a62".into(),
            ambient: 0.55,
            sky_zenith: "#2f6db5".into(),
            sky_horizon: "#bfe0f2".into(),
            fog_color: "#bfe0f2".into(),
            fog_start: 250.0,
            fog_end: 1500.0,
            cloud_cover: 0.3,
            cloud_color: "#ffffff".into(),
            rain: 0.0,
            wetness: 0.0,
            wind: [4.0, -4.0],
            exposure: 1.0,
            saturation: 1.0,
            contrast: 1.0,
            bloom: 0.35,
            sea: "calm".into(),
        }
    }
}

impl AtmosphereDef {
    pub fn to_atmosphere(&self) -> Atmosphere {
        let wind = Vec2::from(self.wind);
        let wind_deg = wind.y.atan2(wind.x).to_degrees();
        let sea = SeaPreset::from_name(&self.sea).map(|p| SeaState::preset(p, wind_deg));
        Atmosphere {
            sun_azimuth: self.sun_azimuth,
            sun_elevation: self.sun_elevation,
            sun_color: Color::hex(&self.sun_color),
            sun_intensity: self.sun_intensity,
            ambient_sky: Color::hex(&self.ambient_sky),
            ambient_ground: Color::hex(&self.ambient_ground),
            ambient: self.ambient,
            sky_zenith: Color::hex(&self.sky_zenith),
            sky_horizon: Color::hex(&self.sky_horizon),
            fog_color: Color::hex(&self.fog_color),
            fog_start: self.fog_start,
            fog_end: self.fog_end,
            cloud_cover: self.cloud_cover,
            cloud_color: Color::hex(&self.cloud_color),
            rain: self.rain,
            wetness: self.wetness,
            wind,
            exposure: self.exposure,
            saturation: self.saturation,
            contrast: self.contrast,
            bloom: self.bloom,
            sea,
            ..Atmosphere::default()
        }
    }
}

#[derive(Debug, Default, Deserialize)]
pub struct Chapter {
    pub label: String,
    pub scene: String,
}

/// `game.toml`: the front end's identity and scene keys.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct GameInfo {
    pub title: String,
    pub tagline: String,
    pub id: String,
    pub save_key: String,
    pub title_scene: String,
    pub new_game_scene: String,
    pub training_scene: Option<String>,
    pub chapter: Vec<Chapter>,
}

#[derive(Debug, Default, Deserialize)]
pub struct CreditBlock {
    pub heading: String,
    pub body: String,
}

#[derive(Debug, Default, Deserialize)]
struct Credits {
    #[serde(default)]
    block: Vec<CreditBlock>,
}

#[derive(Debug, Default, Deserialize)]
struct ShipsFile {
    #[serde(default)]
    class: pav_kit::naval::Classes,
    #[serde(default)]
    style: BTreeMap<String, pav_kit::shipyard::ShipStyle>,
}

/// Everything parsed, shared by every scene built from it.
#[derive(Debug, Default)]
pub struct Content {
    pub info: GameInfo,
    pub camera: CameraContexts,
    pub atmospheres: BTreeMap<String, AtmosphereDef>,
    pub demo: Vec<DemoSegment>,
    pub credits: Vec<CreditBlock>,
    pub classes: pav_kit::naval::Classes,
    pub styles: BTreeMap<String, pav_kit::shipyard::ShipStyle>,
    /// The six Rounds, in order.
    pub missions: Vec<pav_kit::mission::MissionDef>,
    pub dialog: pav_kit::dialog::DialogBank,
}

impl Content {
    /// A preset by name (the default atmosphere when unknown).
    pub fn atmosphere(&self, name: &str) -> Atmosphere {
        self.atmospheres.get(name).map(|d| d.to_atmosphere()).unwrap_or_default()
    }
}

fn file(path: &str) -> Result<std::borrow::Cow<'static, str>> {
    crate::data(path).ok_or_else(|| anyhow!("games/blacktap/{path} is missing"))
}

/// Parses every data file (each scene build calls it, so edits on disk show at `load`).
pub fn load() -> Result<Arc<Content>> {
    let info: GameInfo = toml::from_str(&file("game.toml")?).context("game.toml")?;
    let camera = CameraContexts::parse(&file("camera.toml")?).context("camera.toml")?;
    let atmospheres: BTreeMap<String, AtmosphereDef> = toml::from_str(&file("atmosphere.toml")?).context("atmosphere.toml")?;
    let demo = pav_kit::demo::parse_playlist(&file("demo.toml")?).context("demo.toml")?;
    let credits: Credits = toml::from_str(&file("credits.toml")?).context("credits.toml")?;
    let ships: ShipsFile = toml::from_str(&file("ships.toml")?).context("ships.toml")?;
    let mut missions = Vec::new();
    for n in 1..=6 {
        let path = format!("missions/r{n}.toml");
        missions.push(pav_kit::mission::MissionDef::parse(&file(&path)?).with_context(|| path.clone())?);
    }
    let dialog = pav_kit::dialog::DialogBank::parse(&file("dialog.toml")?).context("dialog.toml")?;
    Ok(Arc::new(Content {
        info,
        camera,
        atmospheres,
        demo,
        credits: credits.block,
        classes: ships.class,
        styles: ships.style,
        missions,
        dialog,
    }))
}
