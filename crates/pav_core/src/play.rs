//! The presentation channel of games on the hook (hook v2): everything a game shows and plays
//! besides its entities, as plain data in each frame (`RenderFrame::play`). The view draws it
//! (camera, draw list, atmosphere, effects), the app plays it (UI, music, rumble) and the tools
//! reproduce the same picture headlessly. No graphics here: only numbers, colours and text.
//!
//! A game fills a [`PlayFrame`] through the `Game` methods `camera`, `ui`, `draw`,
//! `atmosphere`, `audio` and `effects` (all optional). See `docs/MAKE_A_GAME.md`.

use std::sync::Arc;

use glam::{Quat, Vec2, Vec3};
use serde::{Deserialize, Serialize};

use crate::color::Color;
use crate::entity::EntityId;
use crate::fxdef::{DistortDef, EmitterDef};
use crate::ocean::{SeaState, Seabed};
use crate::shape::{Look, Shape};

/// One frame of a game's presentation.
#[derive(Clone, Debug, Default)]
pub struct PlayFrame {
    pub camera: Option<CameraHint>,
    pub ui: UiModel,
    pub draw: DrawList,
    pub atmosphere: Option<Atmosphere>,
    pub audio: AudioState,
    pub effects: FrameEffects,
}

// ---------------------------------------------------------------- camera

/// Where the game wants the camera. The game resolves collisions (it has the world), so the
/// view only places the eye and blends.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct CameraHint {
    pub eye: Vec3,
    pub target: Vec3,
    /// Vertical field of view (degrees).
    pub fov: f32,
    /// Far plane (m).
    pub far: f32,
    /// A hard cut: don't interpolate from the previous frame.
    #[serde(default)]
    pub cut: bool,
    /// Dither out geometry between the eye and the target (the player stays visible).
    #[serde(default = "yes")]
    pub fade_occluders: bool,
}

fn yes() -> bool {
    true
}

impl CameraHint {
    /// An orbit: `yaw` 0 looks toward -Z (north), increasing clockwise seen from above;
    /// `pitch` > 0 looks down.
    pub fn orbit(target: Vec3, yaw_deg: f32, pitch_deg: f32, distance: f32, fov: f32) -> Self {
        let eye = target + orbit_offset(yaw_deg, pitch_deg, distance);
        Self { eye, target, fov, far: 600.0, cut: false, fade_occluders: true }
    }

    /// Unit horizontal forward (x, z) of this camera.
    pub fn forward_xz(&self) -> Vec2 {
        let d = self.target - self.eye;
        Vec2::new(d.x, d.z).normalize_or(Vec2::new(0.0, -1.0))
    }

    /// Converts a stick/keys axis (x right, y up = forward) into a world move direction.
    pub fn relative_move(&self, axis: Vec2) -> Vec2 {
        let f = self.forward_xz();
        let r = Vec2::new(-f.y, f.x);
        f * axis.y + r * axis.x
    }
}

/// Offset from target to eye for an orbit camera.
pub fn orbit_offset(yaw_deg: f32, pitch_deg: f32, distance: f32) -> Vec3 {
    let (y, p) = (yaw_deg.to_radians(), pitch_deg.to_radians());
    // Looking along (sin y, -cos y) horizontally; the eye sits behind and above.
    let back = Vec3::new(-y.sin(), 0.0, y.cos());
    (back * p.cos() + Vec3::Y * p.sin()) * distance
}

// ---------------------------------------------------------------- UI

/// The game's HUD and in-play overlays for one frame (drawn by `pav_ui`).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct UiModel {
    pub hud: Vec<HudItem>,
    pub objective: Option<Objective>,
    /// Contextual prompts, bottom right, at most three ("△ Take the helm").
    pub prompts: Vec<Prompt>,
    pub subtitles: Vec<Subtitle>,
    pub banner: Option<Banner>,
    pub toasts: Vec<Toast>,
    /// World-space markers projected by the UI.
    pub markers: Vec<WorldMarker>,
    pub awareness: Vec<AwarenessArc>,
    pub compass: Option<Compass>,
    pub overlay: Option<Overlay>,
    /// Attract mode: the segment's caption and progress (None in normal play).
    pub demo: Option<DemoState>,
    pub screen: ScreenFx,
    /// Asks the front end to do something (leave to the title after the results...).
    pub request: Option<UiRequest>,
    /// The world map (static layers shared between frames) and live icons.
    #[serde(skip)]
    pub map: Option<Arc<MapData>>,
    pub map_icons: Vec<MapIcon>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HudItem {
    /// The player's health (0..1) in segments; `heat` 0..3 wanted level.
    Health { frac: f32, segments: u8, heat: u8 },
    /// At the helm: hull and sails (0..1), wind relative to the bow (degrees, 0 = from ahead),
    /// wind strength 0..1, sail level 0..2, speed (knots), weapons' readiness 0..1.
    Ship { hull: f32, sails: f32, wind_rel: f32, wind: f32, sail: u8, knots: f32, weapons: Vec<WeaponPip> },
    /// A labelled progress bar (fort health, boarding).
    Bar { label: String, frac: f32, color: String },
    /// A counter (reales, timber, kegs).
    Counter { icon: String, label: String, value: String },
    /// The selected tool and how many are left.
    Tool { name: String, count: u32 },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WeaponPip {
    pub name: String,
    /// 0..1 reloaded.
    pub ready: f32,
    /// The weapon the camera currently chooses.
    pub active: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Objective {
    /// "Second Round — Broadside".
    pub title: String,
    /// The current step ("Sink the gunboats").
    pub text: String,
    /// Optional objectives: (text, done).
    pub optional: Vec<(String, bool)>,
    /// Seconds since the objective last changed (the UI fades it after a while).
    pub age: f32,
}

/// A button prompt: `action` is a binding name ("interact"), the UI draws its glyph.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Prompt {
    pub action: String,
    pub label: String,
    #[serde(default)]
    pub hold: bool,
}

impl Prompt {
    pub fn new(action: &str, label: impl Into<String>) -> Self {
        Self { action: action.into(), label: label.into(), hold: false }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Subtitle {
    pub speaker: String,
    /// Speaker colour, sRGB hex.
    pub color: String,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Banner {
    pub text: String,
    pub sub: String,
    /// Seconds shown so far (the UI animates in and out).
    pub age: f32,
    /// sRGB hex accent.
    pub color: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Toast {
    pub text: String,
    pub age: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarkerKind {
    Objective,
    Waypoint,
    Enemy,
    Target,
    Ally,
    Collectible,
    Interact,
    /// A telegraphed impact (drawn in the world as a decal; listed for off-screen pips).
    Threat,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorldMarker {
    pub pos: Vec3,
    pub kind: MarkerKind,
    pub label: String,
    /// A health bar under the marker (0..1).
    pub bar: Option<f32>,
    /// Show the distance.
    pub distance: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Awareness {
    Unaware,
    Suspicious,
    Investigating,
    Searching,
    Alerted,
    Combat,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct AwarenessArc {
    /// Above the head.
    pub pos: Vec3,
    pub fill: f32,
    pub state: Awareness,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Compass {
    /// Camera heading, degrees clockwise from north (-Z).
    pub heading: f32,
    pub marks: Vec<CompassMark>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CompassMark {
    /// Bearing, degrees clockwise from north.
    pub bearing: f32,
    pub kind: MarkerKind,
    pub distance: f32,
}

/// Full-screen treatments.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ScreenFx {
    /// Cinematic bars, 0..1.
    pub letterbox: f32,
    /// Damage vignette, 0..1.
    pub vignette: f32,
    /// Low-health / Gull Sight desaturation, 0..1.
    pub desaturate: f32,
    /// Fade to black, 0..1.
    pub fade: f32,
    /// Fade to white (Last Words mist), 0..1.
    pub white: f32,
}

/// Overlays the game draws over play; it handles their input itself (from `InputFrame`), so
/// the bot and replays drive them like a player.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Overlay {
    /// Two to four big tiles (plunder, upgrades, results' choices).
    Tiles {
        title: String,
        subtitle: String,
        tiles: Vec<Tile>,
        focus: usize,
    },
    /// The tool wheel.
    Wheel {
        slots: Vec<Tile>,
        selected: usize,
    },
    Results(Results),
    /// The full-screen map (the app also opens it from the pause menu).
    Map {
        cursor: Vec2,
        zoom: f32,
    },
    /// A line of a cinematic scene ("Hold ◯ to skip").
    Cinematic {
        skip: f32,
    },
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Tile {
    pub name: String,
    pub desc: String,
    /// Cost or count text ("120 reales"), may be empty.
    pub cost: String,
    /// A vector icon name the UI knows ("hull", "shot", "sail", "keg", "repair", "cargo"...).
    pub icon: String,
    #[serde(default)]
    pub disabled: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Results {
    pub title: String,
    /// Pour %, 0..100.
    pub pour: f32,
    /// Full Pour checklist (text, done).
    pub checklist: Vec<(String, bool)>,
    /// (label, value) rows: time, reales, collectibles...
    pub stats: Vec<(String, String)>,
    pub tiles: Vec<Tile>,
    pub focus: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DemoState {
    pub caption: String,
    /// The segment ended (watchdog, time limit or its showcase is complete).
    pub done: bool,
    /// Showcase events so far and the distinct mechanics shown.
    pub events: u32,
    pub mechanics: Vec<String>,
    /// Stalled (no progress for the watchdog's limit).
    pub stalled: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UiRequest {
    /// Back to the title screen.
    Title,
    /// Open the Replay a Round menu.
    Replay,
    /// Save progress now (the game's `progress()` is current).
    Save,
}

/// A static world map: vector layers in world coordinates (x, z).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MapData {
    /// World rectangle the map covers: min and max (x, z).
    pub min: Vec2,
    pub max: Vec2,
    /// Closed coastlines and contour lines: (points, sRGB hex, width).
    pub lines: Vec<(Vec<Vec2>, String, f32)>,
    /// Filled shapes (building footprints, roads): (points, sRGB hex).
    pub fills: Vec<(Vec<Vec2>, String)>,
    /// Place names.
    pub labels: Vec<(Vec2, String)>,
    /// Fog-of-war cells revealed (grid over min..max), `fog_size` per side; empty = all clear.
    pub fog_size: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MapIcon {
    pub at: Vec2,
    /// "player", "objective", "perch", "perch_done", "chest", "sheet", "bottle", "tap",
    /// "flotsam", "ship", "enemy_ship", "waypoint", "fort", "tavern".
    pub icon: String,
    /// Facing (degrees) for arrows.
    pub yaw: f32,
}

// ---------------------------------------------------------------- draw list

/// Things the game draws from its own state, without entities.
#[derive(Clone, Debug, Default)]
pub struct DrawList {
    pub items: Vec<DrawItem>,
}

impl DrawList {
    pub fn push(&mut self, item: DrawItem) {
        self.items.push(item);
    }
}

#[derive(Clone, Debug)]
pub enum DrawItem {
    /// A primitive shape.
    Prim { shape: Shape, pos: Vec3, rot: Quat, color: Color, emissive: f32, look: Look },
    /// A mesh made by game code (ship hulls, terrain decals), placed by a transform. The view
    /// uploads it once per `Mesh::id` and reuses it.
    Mesh { mesh: Arc<Mesh>, pos: Vec3, rot: Quat, scale: Vec3, color: Color, emissive: f32 },
    /// A mesh rebuilt every frame (sails, flags), placed by a transform so it moves smoothly
    /// with what carries it.
    Dynamic { mesh: Mesh, pos: Vec3, rot: Quat, color: Color, two_sided: bool },
    /// A flat ring or disc on the ground or sea (telegraphs, waypoints): `ring` 0 = full disc,
    /// 0.9 = thin ring.
    Decal { center: Vec3, radius: f32, ring: f32, color: Color, alpha: f32 },
    /// A thin line (rigging, ropes, ballistic arcs).
    Line { a: Vec3, b: Vec3, radius: f32, color: Color, emissive: f32 },
    /// Gull Sight: draw this entity's silhouette in `color` through walls.
    Highlight { entity: EntityId, color: Color },
    /// A point light (lanterns, muzzle flashes, lightning).
    Light { pos: Vec3, color: Color, intensity: f32, radius: f32 },
    /// Text in the world (signs).
    Text { pos: Vec3, text: String, size: f32, color: Color, yaw: f32 },
    /// A particle emitter that lives while the game draws it (deck fires, torches, spray,
    /// fireflies). `key` names it from frame to frame (the view carries its fractional
    /// particles and starts a new key in full swing); `drift` (m/s) is added to every
    /// particle's launch velocity, so smoke leans downwind.
    Emitter { key: u64, def: EmitterDef, pos: Vec3, drift: Vec3 },
    /// A screen distortion (heat haze over a fire, the swirl of a waterspout).
    Distortion { def: DistortDef, pos: Vec3 },
}

/// Vertex data for [`DrawItem::Mesh`] and [`DrawItem::Dynamic`]. Counter-clockwise winding seen
/// from outside; colours are linear RGB multiplied with the item's colour.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Mesh {
    /// Stable identity for GPU caching (hash the generator's inputs).
    pub id: u64,
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub colors: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
    /// How far away the view stops drawing it (see [`Cull`]).
    pub cull: Cull,
}

/// Distance class of a [`Mesh`]: the view culls small props, vegetation and structures at
/// their own distances (`ViewSettings`), and never culls `Always` (ships, terrain, the hero).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Cull {
    #[default]
    Always,
    Small,
    Vegetation,
    Structure,
}

impl Mesh {
    pub fn new(id: u64) -> Self {
        Self { id, ..Default::default() }
    }

    /// Adds a vertex, returning its index.
    pub fn vertex(&mut self, p: Vec3, n: Vec3, c: [f32; 3]) -> u32 {
        self.positions.push(p.to_array());
        self.normals.push(n.normalize_or(Vec3::Y).to_array());
        self.colors.push(c);
        (self.positions.len() - 1) as u32
    }

    pub fn tri(&mut self, a: u32, b: u32, c: u32) {
        self.indices.extend([a, b, c]);
    }

    /// A flat-shaded quad (a, b, c, d counter-clockwise seen from the front).
    pub fn quad(&mut self, a: Vec3, b: Vec3, c: Vec3, d: Vec3, color: [f32; 3]) {
        let n = (b - a).cross(d - a).normalize_or(Vec3::Y);
        let i = [a, b, c, d].map(|p| self.vertex(p, n, color));
        self.tri(i[0], i[1], i[2]);
        self.tri(i[0], i[2], i[3]);
    }

    /// Recomputes smooth normals from the triangles.
    pub fn smooth_normals(&mut self) {
        let mut acc = vec![Vec3::ZERO; self.positions.len()];
        for t in self.indices.as_chunks::<3>().0 {
            let [a, b, c] = t.map(|i| Vec3::from(self.positions[i as usize]));
            let n = (b - a).cross(c - a);
            for &i in t {
                acc[i as usize] += n;
            }
        }
        self.normals = acc.into_iter().map(|n| n.normalize_or(Vec3::Y).to_array()).collect();
    }

    /// Appends another mesh transformed by `rot` then `pos`.
    pub fn append(&mut self, other: &Mesh, pos: Vec3, rot: Quat) {
        let base = self.positions.len() as u32;
        for i in 0..other.positions.len() {
            self.positions.push((pos + rot * Vec3::from(other.positions[i])).to_array());
            self.normals.push((rot * Vec3::from(other.normals[i])).to_array());
            self.colors.push(other.colors[i]);
        }
        self.indices.extend(other.indices.iter().map(|i| i + base));
    }

    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }
}

// ---------------------------------------------------------------- atmosphere

/// A full description of sky, light, sea and weather; the view blends toward it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Atmosphere {
    /// Degrees: azimuth clockwise from north (-Z), elevation above the horizon.
    pub sun_azimuth: f32,
    pub sun_elevation: f32,
    pub sun_color: Color,
    pub sun_intensity: f32,
    pub ambient_sky: Color,
    pub ambient_ground: Color,
    pub ambient: f32,
    pub sky_zenith: Color,
    pub sky_horizon: Color,
    pub fog_color: Color,
    /// Fog along the view ray (m).
    pub fog_start: f32,
    pub fog_end: f32,
    pub cloud_cover: f32,
    pub cloud_color: Color,
    pub rain: f32,
    /// Lightning flash brightness right now (0..1).
    pub lightning: f32,
    pub wetness: f32,
    /// Wind (m/s) on the XZ plane: drives sway, smoke, clouds and rain slant.
    pub wind: Vec2,
    /// Colour grade.
    pub exposure: f32,
    pub saturation: f32,
    pub contrast: f32,
    pub bloom: f32,
    /// Far plane (m).
    pub far: f32,
    /// Sun shadow radius around the camera target (m).
    pub shadow_radius: f32,
    /// The sea, when this scene has an ocean.
    #[serde(skip)]
    pub sea: Option<SeaState>,
    /// Sea time (s): the ocean function's `t`.
    pub sea_time: f32,
    /// Seabed heights for shallows colour (shared, rarely changes).
    #[serde(skip)]
    pub seabed: Option<Arc<Seabed>>,
    /// Desaturate and darken everything but highlights (Gull Sight), 0..1.
    pub gull: f32,
}

impl Default for Atmosphere {
    fn default() -> Self {
        Self {
            sun_azimuth: 225.0,
            sun_elevation: 35.0,
            sun_color: Color::hex("#fff1dc"),
            sun_intensity: 1.0,
            ambient_sky: Color::hex("#9fc4e8"),
            ambient_ground: Color::hex("#8a7a62"),
            ambient: 0.55,
            sky_zenith: Color::hex("#2f6db5"),
            sky_horizon: Color::hex("#bfe0f2"),
            fog_color: Color::hex("#bfe0f2"),
            fog_start: 250.0,
            fog_end: 1500.0,
            cloud_cover: 0.3,
            cloud_color: Color::hex("#ffffff"),
            rain: 0.0,
            lightning: 0.0,
            wetness: 0.0,
            wind: Vec2::new(4.0, -4.0),
            exposure: 1.0,
            saturation: 1.0,
            contrast: 1.0,
            bloom: 0.35,
            far: 1800.0,
            shadow_radius: 70.0,
            sea: None,
            sea_time: 0.0,
            seabed: None,
            gull: 0.0,
        }
    }
}

impl Atmosphere {
    /// Blend from `a` to `b` (t 0..1), colours in linear space.
    pub fn blend(a: &Atmosphere, b: &Atmosphere, t: f32) -> Atmosphere {
        let t = t.clamp(0.0, 1.0);
        let f = |x: f32, y: f32| x + (y - x) * t;
        let c = |x: Color, y: Color| Color([f(x.0[0], y.0[0]), f(x.0[1], y.0[1]), f(x.0[2], y.0[2])]);
        // Azimuth: the short way round.
        let mut daz = (b.sun_azimuth - a.sun_azimuth).rem_euclid(360.0);
        if daz > 180.0 {
            daz -= 360.0;
        }
        Atmosphere {
            sun_azimuth: (a.sun_azimuth + daz * t).rem_euclid(360.0),
            sun_elevation: f(a.sun_elevation, b.sun_elevation),
            sun_color: c(a.sun_color, b.sun_color),
            sun_intensity: f(a.sun_intensity, b.sun_intensity),
            ambient_sky: c(a.ambient_sky, b.ambient_sky),
            ambient_ground: c(a.ambient_ground, b.ambient_ground),
            ambient: f(a.ambient, b.ambient),
            sky_zenith: c(a.sky_zenith, b.sky_zenith),
            sky_horizon: c(a.sky_horizon, b.sky_horizon),
            fog_color: c(a.fog_color, b.fog_color),
            fog_start: f(a.fog_start, b.fog_start),
            fog_end: f(a.fog_end, b.fog_end),
            cloud_cover: f(a.cloud_cover, b.cloud_cover),
            cloud_color: c(a.cloud_color, b.cloud_color),
            rain: f(a.rain, b.rain),
            lightning: f(a.lightning, b.lightning),
            wetness: f(a.wetness, b.wetness),
            wind: a.wind.lerp(b.wind, t),
            exposure: f(a.exposure, b.exposure),
            saturation: f(a.saturation, b.saturation),
            contrast: f(a.contrast, b.contrast),
            bloom: f(a.bloom, b.bloom),
            far: f(a.far, b.far),
            shadow_radius: f(a.shadow_radius, b.shadow_radius),
            sea: match (&a.sea, &b.sea) {
                (Some(x), Some(y)) => Some(SeaState::blend(x, y, t)),
                (x, y) => {
                    if t < 0.5 {
                        *x
                    } else {
                        *y
                    }
                }
            },
            sea_time: f(a.sea_time, b.sea_time),
            seabed: if t < 0.5 { a.seabed.clone() } else { b.seabed.clone() },
            gull: f(a.gull, b.gull),
        }
    }

    /// Unit direction the sunlight travels (from the sun toward the ground).
    pub fn sun_direction(&self) -> Vec3 {
        let (az, el) = (self.sun_azimuth.to_radians(), self.sun_elevation.to_radians());
        let to_sun = Vec3::new(az.sin() * el.cos(), el.sin(), -az.cos() * el.cos());
        -to_sun
    }
}

// ---------------------------------------------------------------- audio

/// What should be playing: the app's audio layer crossfades toward it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AudioState {
    pub music: Option<MusicCue>,
    /// Ambience layers by name and level (0..1).
    pub ambience: Vec<(String, f32)>,
    /// Where the listener is (default: the camera target) and its facing (degrees).
    pub listener: Option<(Vec3, f32)>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MusicCue {
    /// A song or state from the game's music bank.
    pub song: String,
    /// 0..1: which stems play.
    pub intensity: f32,
}

// ---------------------------------------------------------------- effects

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct FrameEffects {
    /// Camera shake strength (0..1).
    pub shake: f32,
    /// Colour washes on entities: (entity, linear rgb, strength).
    pub tints: Vec<(EntityId, [f32; 3], f32)>,
    /// Screen transition: (kind name: "fade", "iris", "dissolve"..., coverage 0..1).
    pub transition: Option<(String, f32)>,
    /// Controller rumble started by the tick this frame shows: a game empties its list at the
    /// start of every tick, and the app's simulation host collects each tick's cues once (so
    /// none is lost or doubled however fast the window draws).
    pub rumble: Vec<RumbleCue>,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct RumbleCue {
    /// Low- and high-frequency motor strength (0..1) and duration (ms).
    pub low: f32,
    pub high: f32,
    pub ms: u32,
}

// ---------------------------------------------------------------- front end

/// What the app's front end needs from a game before any scene runs: its name, theme,
/// menus, the attract playlist and its data banks.
#[derive(Clone, Debug, Default)]
pub struct FrontEndInfo {
    pub title: String,
    pub tagline: String,
    /// Unique build ID shown small on the title ("AssassinsKegBlackTap-W9").
    pub id: String,
    /// Scenes: the title backdrop, a new game, the developer level.
    pub title_scene: String,
    pub new_game_scene: String,
    pub training_scene: Option<String>,
    /// Replay a Round: (label, scene).
    pub chapters: Vec<(String, String)>,
    /// Attract mode segments in order.
    pub demo: Vec<DemoSegment>,
    /// Credits & About text blocks: (heading, body).
    pub credits: Vec<(String, String)>,
    /// UI theme (TOML text, `pav_ui::Theme`).
    pub theme: String,
    /// Input bindings (TOML text, see `input::Bindings`).
    pub bindings: String,
    /// Sound, music and FX banks (TOML), by name ("sounds", "music", "fx", "ambience").
    pub banks: Vec<(String, String)>,
    /// Songs in ABC notation: (name, text).
    pub songs: Vec<(String, String)>,
    /// Key prefix for saved settings and progress ("blacktap").
    pub save_key: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DemoSegment {
    pub name: String,
    pub scene: String,
    pub caption: String,
    /// Seeds to cycle through, one per loop of the playlist.
    pub seeds: Vec<u64>,
    /// Time limit (s).
    pub seconds: f32,
}
