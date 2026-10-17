//! Plain-data description of one frame. The renderer knows nothing about the simulation;
//! a view layer turns game state into a `Scene` every frame.

use glam::{Mat4, Vec3};

pub use crate::mesh::MeshKey;

/// Surface style, per object.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[repr(u32)]
pub enum Style {
    /// Albedo only (plus cast shadows as a darker tone): the clean 2D look.
    Flat = 0,
    /// Toon bands, rim light.
    #[default]
    Cel = 1,
    /// Smooth lighting with specular.
    Lit = 2,
    /// Pure albedo, ignores all lighting and shadows (UI-like markers, emissive signs).
    Unlit = 3,
}

impl Style {
    pub const ALL: [Style; 4] = [Style::Flat, Style::Cel, Style::Lit, Style::Unlit];
    pub fn name(self) -> &'static str {
        match self {
            Style::Flat => "flat",
            Style::Cel => "cel",
            Style::Lit => "lit",
            Style::Unlit => "unlit",
        }
    }
}

pub mod flags {
    /// Never removed by cutaway or occlusion fade (the player, markers).
    pub const NO_CUT: u32 = 1;
    /// Does not cast shadows.
    pub const NO_SHADOW: u32 = 2;
    /// Does not receive shadows.
    pub const NO_RECEIVE_SHADOW: u32 = 4;
    /// Cutaway lowers individual vertices near the player (for big meshes like terrain).
    pub const CUT_VERTEX: u32 = 8;
    /// Both sides are real surfaces (cloth), not the inside of a solid.
    pub const TWO_SIDED: u32 = 16;
    /// Drawn as pixel art (in blocks of `FilterSettings::pixel_art` pixels) when that is
    /// above 1; the rest of the screen stays sharp.
    pub const PIXEL: u32 = 32;
    /// Part of the characters and objects (not the environment): filters aimed at one part
    /// of the scene (`Part`) tell the two apart by this flag.
    pub const OBJECT: u32 = 64;
    /// Sways in the wind like leaves (canopies, bushes, banners): vertices move more the
    /// higher they are above the bottom of the instance.
    pub const SWAY: u32 = 128;
    /// Bends like grass: far in gusts, and away from the wind's pusher (the player).
    pub const GRASS: u32 = 256;
    /// Seen through walls (Gull Sight): drawn normally, then again where something hides it,
    /// as a translucent silhouette in its colour with a bright rim. `Scene::gull` greys out
    /// everything else.
    pub const XRAY: u32 = 512;
    /// A small prop: not drawn (nor its shadow) beyond `Scene::cull_small` metres from the eye.
    pub const CULL_SMALL: u32 = 1024;
    /// Vegetation: not drawn beyond `Scene::cull_vegetation` metres.
    pub const CULL_VEGETATION: u32 = 2048;
    /// A structure: not drawn beyond `Scene::cull_structure` metres.
    pub const CULL_STRUCTURE: u32 = 4096;
}

/// Wind for swaying foliage and grass (vertex shader).
#[derive(Clone, Copy, Debug, Default)]
pub struct Wind {
    /// Horizontal direction the wind blows toward (x, z).
    pub direction: glam::Vec2,
    /// 0 = still air.
    pub strength: f32,
    /// How quickly gusts roll across (1 = normal).
    pub gusts: f32,
    /// Grass bends away from this point (the player's feet) within `push_radius` (0 = off).
    pub pusher: Vec3,
    pub push_radius: f32,
}

/// Which part of the scene a filter applies to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[repr(u32)]
pub enum Part {
    /// Everything on screen.
    #[default]
    All = 0,
    /// Instances flagged `flags::OBJECT` (characters, props, loot, projectiles, effects).
    Objects = 1,
    /// Everything else: level geometry, fixed scenery and the sky.
    Environment = 2,
}

/// Painterly and print looks redrawn from the finished picture.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[repr(u32)]
pub enum Stylize {
    #[default]
    Off = 0,
    /// Oil paint: a generalized Kuwahara filter (flat strokes that keep their edges) on canvas.
    Paint = 1,
    /// Print: rotated dot screens (CMYK comics, or black dots for newsprint).
    Halftone = 2,
    /// Text mode: each cell becomes a character picked by its brightness.
    Ascii = 3,
    /// Pencil on paper: hatching that thickens with darkness, plus contour lines.
    Sketch = 4,
}

/// How a screen transition covers the picture.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[repr(u32)]
pub enum Transition {
    #[default]
    Fade = 0,
    /// A circle closing on `transition_center`.
    Iris = 1,
    /// Diamonds growing in a wave across the screen.
    Diamonds = 2,
    /// Random blocks.
    Dissolve = 3,
    /// Pixels growing into big blocks, then black.
    Mosaic = 4,
    /// Horizontal blinds.
    Blinds = 5,
}

/// A mesh rebuilt every frame (soft bodies), in world space.
#[derive(Clone, Debug)]
pub struct DynamicMesh {
    pub data: crate::mesh::MeshData,
    pub color: Vec3,
    pub emissive: f32,
    pub style: Style,
    pub flags: u32,
    pub group: u32,
    /// Procedural material: `slot | seed << 12` (see `MeshInstance::material`; its local
    /// frame is the world's).
    pub material: u32,
}

#[derive(Clone, Copy, Debug)]
pub struct MeshInstance {
    pub mesh: MeshKey,
    pub transform: Mat4,
    /// Linear RGB albedo.
    pub color: Vec3,
    /// Emissive strength (0 = none). Adds `color * emissive` on top of lighting.
    pub emissive: f32,
    pub style: Style,
    pub flags: u32,
    /// Outline group: edges are drawn where the group changes. 0 = background, use 1 for
    /// static level geometry and unique ids for objects.
    pub group: u32,
    /// Procedural material: a row of `Scene::materials` in bits 0-11 (0 = none, the plain
    /// colour) and a per-instance seed in bits 12-31 (`material_param`). `color` and
    /// `emissive` are the material's base colour and glow, so one row serves every tint.
    pub material: u32,
    /// Half extents of the part in its own frame, for the material's axis map and gradients.
    /// ZERO derives them from the transform's columns (meshes that fill the unit cube);
    /// meshes built at their real size, like rounded boxes, set them.
    pub ext: Vec3,
}

/// Analytic SDF primitive, ray-traced per pixel: a rounded cone between `a` and `b` with
/// radii `ra`/`rb`. Equal radii give a capsule; `a == b` gives a sphere.
#[derive(Clone, Copy, Debug)]
pub struct SdfInstance {
    pub a: Vec3,
    pub b: Vec3,
    pub ra: f32,
    pub rb: f32,
    pub color: Vec3,
    pub emissive: f32,
    pub style: Style,
    pub flags: u32,
    pub group: u32,
    /// Procedural material, as `MeshInstance::material`; the local frame runs along a→b.
    pub material: u32,
}

impl SdfInstance {
    pub fn sphere(center: Vec3, r: f32, color: Vec3) -> Self {
        Self { a: center, b: center, ra: r, rb: r, color, emissive: 0.0, style: Style::Cel, flags: 0, group: 1, material: 0 }
    }
    pub fn capsule(a: Vec3, b: Vec3, r: f32, color: Vec3) -> Self {
        Self { a, b, ra: r, rb: r, color, emissive: 0.0, style: Style::Cel, flags: 0, group: 1, material: 0 }
    }
}

/// Material slots per scene (bits 0-11 of an instance's `material`; slot 0 is none).
pub const MATERIAL_SLOTS: u32 = 4096;

/// An instance's `material` word: the table row `slot` and a per-instance `seed` (20 bits).
pub fn material_param(slot: u32, seed: u32) -> u32 {
    (slot & (MATERIAL_SLOTS - 1)) | (seed & 0xfffff) << 12
}

/// One pattern layer of a material row, laid out as `MatLayer` in `shaders/materials.wgsl`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuLayer {
    /// x: kernel | variant<<8 | axis<<12 | facing<<16 | mask<<20 | paint<<24 | ncells<<28;
    /// y: salt.
    pub head: [u32; 4],
    /// rgb linear colour (paint 0) or r the linear shade factor (paint 1); a strength.
    pub paint: [f32; 4],
    pub a: [f32; 4],
    pub b: [f32; 4],
    /// x vary, y cavity, z glow, w bevel.
    pub c: [f32; 4],
    /// Up to four sRGB8 cell colours (r in the low byte).
    pub cells: [u32; 4],
    /// xyz: unit axis for 1D patterns.
    pub dir: [f32; 4],
}

/// One row of the material table (368 bytes), laid out as `Material` in
/// `shaders/materials.wgsl`. `pav_core::material` packs it; the renderer only uploads it.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuMaterial {
    /// x layer count, y map (0 object, 1 world, 2 axis), z bit 0: turn.
    pub head: [u32; 4],
    /// x roughness (< 0 = the legacy shading), y metallic, z 1/scale.
    pub pbr: [f32; 4],
    pub layers: [GpuLayer; 3],
}

impl GpuMaterial {
    /// u32 words in a row.
    pub const WORDS: usize = 92;

    /// A row from its words as packed (`pav_core::material::GpuMaterial::words`).
    pub fn from_words(words: &[u32; Self::WORDS]) -> Self {
        bytemuck::pod_read_unaligned(bytemuck::cast_slice(words))
    }
}

const _: () = assert!(std::mem::size_of::<GpuMaterial>() == GpuMaterial::WORDS * 4);

#[derive(Clone, Copy, Debug)]
pub struct CameraData {
    pub view: Mat4,
    pub proj: Mat4,
    pub eye: Vec3,
    pub forward: Vec3,
    pub ortho: bool,
}

impl Default for CameraData {
    fn default() -> Self {
        let eye = Vec3::new(0.0, 10.0, 10.0);
        let forward = (-eye).normalize();
        Self {
            view: glam::camera::rh::view::look_to_mat4(eye, forward, Vec3::Y),
            proj: glam::camera::rh::proj::directx::perspective(45f32.to_radians(), 16.0 / 9.0, 0.1, 500.0),
            eye,
            forward,
            ortho: false,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Sun {
    /// Direction the light travels (from the sun toward the ground).
    pub direction: Vec3,
    /// Linear RGB, already multiplied by intensity.
    pub color: Vec3,
    pub shadows: bool,
    /// Shadows are computed in a square region around this point.
    pub shadow_center: Vec3,
    pub shadow_radius: f32,
}

impl Default for Sun {
    fn default() -> Self {
        Self {
            direction: Vec3::new(-0.35, -1.0, -0.55).normalize(),
            color: Vec3::new(1.0, 0.95, 0.85) * 0.55,
            shadows: true,
            shadow_center: Vec3::ZERO,
            shadow_radius: 30.0,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct PointLight {
    pub position: Vec3,
    /// Linear RGB times intensity.
    pub color: Vec3,
    pub radius: f32,
    /// Casts shadows (the brightest few such lights get shadow maps).
    pub shadows: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct Ambient {
    pub sky: Vec3,
    pub ground: Vec3,
}

impl Default for Ambient {
    fn default() -> Self {
        Self { sky: Vec3::new(0.52, 0.56, 0.66), ground: Vec3::new(0.36, 0.33, 0.30) }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tonemap {
    Clamp = 0,
    /// Identity below 0.8, smooth roll-off above: keeps authored colors exact.
    SoftKnee = 1,
    Aces = 2,
}

#[derive(Clone, Copy, Debug)]
pub struct StyleSettings {
    /// Brightness of shadowed areas in the flat style (0..1).
    pub flat_shadow: f32,
    /// Number of light bands in the cel style.
    pub cel_bands: f32,
    pub rim: f32,
    pub specular: f32,
}

impl Default for StyleSettings {
    fn default() -> Self {
        Self { flat_shadow: 0.62, cel_bands: 2.0, rim: 0.25, specular: 0.35 }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct PostSettings {
    pub outlines: bool,
    /// Outline thickness in pixels.
    pub outline_px: f32,
    /// Relative depth jump that counts as an edge.
    pub outline_depth: f32,
    /// Normal dot product below which a crease counts as an edge.
    pub outline_normal: f32,
    /// Outline = pixel color * darken (when `outline_color` is None).
    pub outline_darken: f32,
    pub outline_color: Option<Vec3>,
    pub exposure: f32,
    pub tonemap: Tonemap,
    pub saturation: f32,
    /// Bloom strength (0 = off) and the HDR brightness where glow starts.
    pub bloom: f32,
    pub bloom_threshold: f32,
    /// Screen distortion (shockwaves, haze) on/off.
    pub distortion: bool,
    /// Screen-space global illumination: bounce light and ambient occlusion (0 = off).
    pub gi: f32,
    /// Which part of the scene gets outlines.
    pub outline_part: Part,
    /// Hazy air (volumetric light): density at the haze's base height (0 = off), the height
    /// (m) over which it thins out and the base height itself; sunlight scattered toward the
    /// camera where the sun's shadow map lets it through (light shafts), how strongly that
    /// light scatters forward (0 = evenly .. 0.9 = mostly toward the sun) and lamp halos (point
    /// lights glowing in the air, 0 = off).
    pub haze: f32,
    pub haze_height: f32,
    pub haze_base: f32,
    pub shafts: f32,
    pub shafts_forward: f32,
    pub halos: f32,
}

impl Default for PostSettings {
    fn default() -> Self {
        Self {
            outlines: true,
            outline_px: 1.0,
            outline_depth: 0.02,
            outline_normal: 0.6,
            outline_darken: 0.35,
            outline_color: None,
            exposure: 1.0,
            tonemap: Tonemap::SoftKnee,
            saturation: 1.0,
            bloom: 0.35,
            bloom_threshold: 1.2,
            distortion: true,
            gi: 0.0,
            outline_part: Part::All,
            haze: 0.0,
            haze_height: 4.0,
            haze_base: 0.0,
            shafts: 1.0,
            shafts_forward: 0.5,
            halos: 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DistortKind {
    /// Shockwave ring pushing outward (`progress` 0..1 = ring radius / fade).
    Ring = 0,
    /// Heat haze wobble.
    Haze = 1,
    /// Magnifying lens (negative strength shrinks).
    Lens = 2,
    /// Rings travelling outward.
    Ripple = 3,
}

/// A screen distortion source, drawn as a camera-facing disc.
#[derive(Clone, Copy, Debug)]
pub struct Distortion {
    pub pos: Vec3,
    pub radius: f32,
    /// Offset as a fraction of the disc's screen radius (0.05-0.3 typical).
    pub strength: f32,
    pub kind: DistortKind,
    pub progress: f32,
}

/// New particles to emit this frame (they live on the GPU afterwards).
#[derive(Clone, Copy, Debug)]
pub struct ParticleBurst {
    pub count: u32,
    pub pos: Vec3,
    /// Spawn inside this box (half extents) around `pos`.
    pub area: Vec3,
    pub vel: Vec3,
    /// Random extra speed in any direction (m/s).
    pub spread: f32,
    pub gravity: f32,
    pub drag: f32,
    /// Life range (s).
    pub life: (f32, f32),
    /// Size at birth and at death (m, radius).
    pub size: (f32, f32),
    /// Linear RGB (may exceed 1 for glow) + alpha, at birth and at death.
    pub color0: glam::Vec4,
    pub color1: glam::Vec4,
    /// Adds light instead of covering (fire, sparks, magic).
    pub additive: bool,
    /// Streaks along the velocity (sparks, rain).
    pub stretch: bool,
    /// Bounce off this height (None = fall forever).
    pub floor: Option<f32>,
    pub bounce: f32,
    /// Swirling noise strength (smoke, fireflies).
    pub turbulence: f32,
    /// Born as if emitted over the last `prewarm` seconds (spread-out ages), so a new
    /// emitter starts in full swing.
    pub prewarm: f32,
}

impl Default for ParticleBurst {
    fn default() -> Self {
        Self {
            count: 0,
            pos: Vec3::ZERO,
            area: Vec3::ZERO,
            vel: Vec3::ZERO,
            spread: 1.0,
            gravity: 0.0,
            drag: 0.0,
            life: (1.0, 1.5),
            size: (0.1, 0.0),
            color0: glam::Vec4::ONE,
            color1: glam::Vec4::new(1.0, 1.0, 1.0, 0.0),
            additive: true,
            stretch: false,
            floor: None,
            bounce: 0.3,
            turbulence: 0.0,
            prewarm: 0.0,
        }
    }
}

/// Screen filters applied after tonemapping (retro looks and grading).
#[derive(Clone, Copy, Debug)]
pub struct FilterSettings {
    /// Pixel block size (1 = off).
    pub pixelate: f32,
    /// CRT tube curvature (0 = flat).
    pub curvature: f32,
    /// Scanline darkness (0 = off) and period in pixels.
    pub scanlines: f32,
    pub scanline_px: f32,
    /// Ordered dithering amount, colour levels per channel (< 2 = off), palette (0 = none,
    /// 1 Game Boy, 2 PICO-8, 3 CGA, 4 1-bit, 5 amber).
    pub dither: f32,
    pub levels: f32,
    pub palette: u32,
    /// Filters only right of this screen fraction (0 = whole screen): side-by-side comparisons.
    pub split: f32,
    pub temperature: f32,
    pub tint: f32,
    pub contrast: f32,
    pub brightness: f32,
    pub vignette: f32,
    pub grain: f32,
    /// Chromatic aberration.
    pub chroma: f32,
    /// Saturation of the filtered side (1 = unchanged, 0 = grey).
    pub saturation: f32,
    /// Pixel-art block size for objects with `flags::PIXEL` (1 = off), their colour levels
    /// per channel (below 2 = unchanged) and a dark one-block outline around them.
    pub pixel_art: f32,
    pub pixel_levels: f32,
    pub pixel_outline: bool,
    /// The part of the scene each filter applies to: colour reduction (palette, levels,
    /// dither), grading (temperature, tint, contrast, brightness, saturation), scanlines,
    /// grain and chromatic aberration. Curvature, vignette and `pixelate` are whole-screen.
    pub color_part: Part,
    pub grade_part: Part,
    pub scanline_part: Part,
    pub grain_part: Part,
    pub chroma_part: Part,
    /// A painterly or print look over the picture, its size in pixels (brush radius, dot
    /// cell, character width, hatching spacing), how much of it shows (0..1), how much of the
    /// scene's colour it keeps (0 = ink only, 1 = full colour) and the part it applies to.
    pub stylize: Stylize,
    pub stylize_size: f32,
    pub stylize_mix: f32,
    pub stylize_color: f32,
    pub stylize_part: Part,
    /// Screen transition: how covered the screen is (0 = clear, 1 = covered), its kind and
    /// its centre (screen fraction, 0..1 from the top left; the iris closes on it).
    pub transition: f32,
    pub transition_kind: Transition,
    pub transition_center: glam::Vec2,
}

impl Default for FilterSettings {
    fn default() -> Self {
        Self {
            pixelate: 1.0,
            curvature: 0.0,
            scanlines: 0.0,
            scanline_px: 3.0,
            dither: 0.0,
            levels: 0.0,
            palette: 0,
            split: 0.0,
            temperature: 0.0,
            tint: 0.0,
            contrast: 1.0,
            brightness: 1.0,
            vignette: 0.0,
            grain: 0.0,
            chroma: 0.0,
            saturation: 1.0,
            pixel_art: 1.0,
            pixel_levels: 0.0,
            pixel_outline: true,
            color_part: Part::All,
            grade_part: Part::All,
            scanline_part: Part::All,
            grain_part: Part::All,
            chroma_part: Part::All,
            stylize: Stylize::Off,
            stylize_size: 4.0,
            stylize_mix: 1.0,
            stylize_color: 1.0,
            stylize_part: Part::All,
            transition: 0.0,
            transition_kind: Transition::Fade,
            transition_center: glam::Vec2::splat(0.5),
        }
    }
}

/// Distance fog around a centre point (usually the camera target), blending into `color`.
#[derive(Clone, Copy, Debug)]
pub struct Fog {
    pub enabled: bool,
    pub center: Vec3,
    /// Horizontal distance where fog starts / is complete (m).
    pub start: f32,
    pub end: f32,
    /// Linear RGB (usually the sky colour).
    pub color: Vec3,
}

impl Default for Fog {
    fn default() -> Self {
        Self { enabled: false, center: Vec3::ZERO, start: 60.0, end: 90.0, color: Vec3::ONE }
    }
}

/// Fog along the view ray (aerial perspective), for `Scene::fog_ray`: with it the fog of
/// `Scene::fog` grows with the distance from the eye instead of the horizontal distance from
/// its centre, the air thins with height, and the sky fades into the fog colour at the horizon.
#[derive(Clone, Copy, Debug)]
pub struct FogRay {
    /// How quickly the fog thins with height above `base` (1/m; 0 = the same everywhere).
    pub height_falloff: f32,
    /// Height where the fog is densest (usually sea level).
    pub base: f32,
    /// Sunlight scattered in the fog toward the sun (0 = plain fog colour).
    pub sun_glow: f32,
}

impl Default for FogRay {
    fn default() -> Self {
        Self { height_falloff: 1.0 / 90.0, base: 0.0, sun_glow: 0.35 }
    }
}

/// Procedural clouds on a curved plane over the camera (part of [`Sky`]).
#[derive(Clone, Copy, Debug)]
pub struct Clouds {
    /// Fraction of the sky covered (0..1).
    pub cover: f32,
    /// Opacity of the clouds (0..1).
    pub density: f32,
    /// Linear RGB of the sunlit tops and of the shaded undersides.
    pub color: Vec3,
    pub shade: Vec3,
    /// How far the wind has carried the clouds (m, world x and z).
    pub offset: glam::Vec2,
    /// Height of the cloud plane above the eye (m) and how quickly it curves down toward the
    /// horizon (0 = flat; larger keeps distant clouds bigger).
    pub height: f32,
    pub curve: f32,
    /// Size of a cloud (m).
    pub size: f32,
    /// Slow change of the cloud shapes (s).
    pub time: f32,
}

impl Default for Clouds {
    fn default() -> Self {
        Self {
            cover: 0.35,
            density: 0.9,
            color: Vec3::ONE,
            shade: Vec3::new(0.55, 0.6, 0.7),
            offset: glam::Vec2::ZERO,
            height: 900.0,
            curve: 0.12,
            size: 700.0,
            time: 0.0,
        }
    }
}

/// The sky, drawn on every pixel that no geometry covers (before bloom, haze and tonemapping).
#[derive(Clone, Copy, Debug)]
pub struct Sky {
    /// Linear RGB straight up, at the horizon and below it.
    pub zenith: Vec3,
    pub horizon: Vec3,
    pub ground: Vec3,
    /// Direction the sunlight travels (as `Sun::direction`).
    pub sun_dir: Vec3,
    /// Colour of the sun's disc (linear HDR: values over 1 bloom).
    pub sun_color: Vec3,
    /// Angular radius of the disc (degrees) and strength of the glow around it (0..1).
    pub sun_size: f32,
    pub sun_glow: f32,
    pub clouds: Clouds,
    /// Storm: darker sky and heavier, darker clouds (0..1).
    pub storm: f32,
    /// Lightning: sky and clouds flash (0..1).
    pub flash: f32,
}

impl Default for Sky {
    fn default() -> Self {
        Self {
            zenith: srgb(0x2f, 0x6d, 0xb5),
            horizon: srgb(0xbf, 0xe0, 0xf2),
            ground: srgb(0x6f, 0x8f, 0xa4),
            sun_dir: Sun::default().direction,
            sun_color: Vec3::new(1.0, 0.95, 0.85) * 12.0,
            sun_size: 1.2,
            sun_glow: 0.5,
            clouds: Clouds::default(),
            storm: 0.0,
            flash: 0.0,
        }
    }
}

/// One Gerstner wave of an [`Ocean`], as `pav_core::ocean` evaluates it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct OceanWave {
    /// Unit direction of travel (world x, z).
    pub dir: glam::Vec2,
    /// Wavelength (m), wavenumber (2π / wavelength) and angular frequency (rad/s).
    pub wavelength: f32,
    pub k: f32,
    pub omega: f32,
    /// Crest height above the mean (m).
    pub amplitude: f32,
    /// Gerstner Q, already normalised over all six waves.
    pub q: f32,
    /// Phase offset (radians).
    pub phase: f32,
}

/// Ground height under the sea on a grid, as `pav_core::ocean::Seabed`: heights (m) at cell
/// centres, `deep` outside. The renderer uploads it when the `Arc` changes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SeabedMap {
    /// World x, z of cell (0, 0)'s corner.
    pub origin: glam::Vec2,
    /// Cell size (m).
    pub cell: f32,
    pub width: usize,
    pub depth: usize,
    /// Row-major (z rows).
    pub heights: Vec<f32>,
    pub deep: f32,
}

/// The open sea: a camera-centred grid displaced by six Gerstner waves on the GPU, exactly as
/// `pav_core::ocean::SeaState::displacement` moves the water on the CPU (near the camera; far
/// away the short waves fade out where the grid can no longer hold them). Opaque, writes
/// depth; lit by `Scene::sun`, fogged by `Scene::fog`.
#[derive(Clone, Debug)]
pub struct Ocean {
    pub waves: [OceanWave; 6],
    /// Sea time (s): the ocean function's `t`.
    pub time: f32,
    /// Mean sea level (world y).
    pub level: f32,
    /// Linear RGB: open water, mid water, shallows and foam.
    pub deep: Vec3,
    pub mid: Vec3,
    pub shallow: Vec3,
    pub foam: Vec3,
    /// Small ripples (0..1) and crest foam (0..1).
    pub detail: f32,
    pub foam_amount: f32,
    /// Sky colours the water reflects (Fresnel), overhead and at the horizon.
    pub sky_zenith: Vec3,
    pub sky_horizon: Vec3,
    /// Strength of the sun's glitter on the water.
    pub glint: f32,
    /// Water shallower than this (m) gets surf foam (needs a seabed).
    pub shore_foam: f32,
    pub seabed: Option<std::sync::Arc<SeabedMap>>,
    /// Grid cells across each of the six LOD rings (128 ≈ 80k vertices; 64 for low quality).
    pub cells: u32,
}

impl Default for Ocean {
    fn default() -> Self {
        Self {
            waves: [OceanWave::default(); 6],
            time: 0.0,
            level: 0.0,
            deep: srgb(0x0b, 0x3d, 0x5c),
            mid: srgb(0x12, 0x70, 0x8a),
            shallow: srgb(0x3f, 0xc1, 0xc0),
            foam: srgb(0xf4, 0xf8, 0xf6),
            detail: 0.5,
            foam_amount: 0.3,
            sky_zenith: srgb(0x2f, 0x6d, 0xb5),
            sky_horizon: srgb(0xbf, 0xe0, 0xf2),
            glint: 1.0,
            shore_foam: 1.5,
            seabed: None,
            cells: 128,
        }
    }
}

impl Ocean {
    /// The waves' phase constants for a grid centred at `center` (world x, z): for each wave,
    /// `k·dot(dir, center) + phase − ω·time` reduced to [0, 2π) in double precision. The GPU then
    /// evaluates `k·dot(dir, p0 − center) + that`, which keeps the shader's sine arguments small.
    pub fn phases_at(&self, center: glam::Vec2) -> [f32; 6] {
        let mut out = [0.0; 6];
        for (o, w) in out.iter_mut().zip(&self.waves) {
            let ph = w.k as f64 * (w.dir.x as f64 * center.x as f64 + w.dir.y as f64 * center.y as f64) + w.phase as f64
                - w.omega as f64 * self.time as f64;
            *o = ph.rem_euclid(std::f64::consts::TAU) as f32;
        }
        out
    }

    /// Where the water particle resting at `p0` (x, z) is now, (dx, height above `level`, dz),
    /// evaluated exactly as the vertex shader does with the grid centred at `center` (all waves
    /// at full strength: the shader fades short waves only far from the eye).
    pub fn displacement(&self, p0: glam::Vec2, center: glam::Vec2) -> Vec3 {
        let pc = self.phases_at(center);
        let r = p0 - center;
        let mut d = Vec3::ZERO;
        for (w, pc) in self.waves.iter().zip(pc) {
            if w.amplitude == 0.0 {
                continue;
            }
            // `wrap_phase` in sea.wgsl.
            let ph = w.k * w.dir.dot(r) + pc;
            let ph = ph - std::f32::consts::TAU * (ph / std::f32::consts::TAU).floor();
            let (s, c) = ph.sin_cos();
            d.x += w.q * w.amplitude * w.dir.x * c;
            d.z += w.q * w.amplitude * w.dir.y * c;
            d.y += w.amplitude * s;
        }
        d
    }
}

/// Rain: streaks falling around the camera, slanted by the wind, and rings on the ocean.
#[derive(Clone, Copy, Debug)]
pub struct Rain {
    /// 0 = dry, 1 = downpour.
    pub amount: f32,
    /// Wind (m/s, world x and z): slants the streaks.
    pub wind: glam::Vec2,
    /// Linear RGB of the streaks (lit by the sky).
    pub color: Vec3,
}

impl Default for Rain {
    fn default() -> Self {
        Self { amount: 0.0, wind: glam::Vec2::ZERO, color: Vec3::new(0.7, 0.75, 0.8) }
    }
}

/// Camera helpers that remove geometry hiding the player.
#[derive(Clone, Copy, Debug, Default)]
pub struct Cutaway {
    /// Point of interest (usually the player's head).
    pub focus: Vec3,
    /// Remove geometry above `cut_height` within `cut_radius` (horizontal) of the focus.
    pub height_cut: bool,
    pub cut_height: f32,
    pub cut_radius: f32,
    /// Dither out geometry between the camera and the focus.
    pub fade: bool,
    pub fade_radius: f32,
    /// Remove everything closer to the camera than this distance in front of the focus
    /// (side views, where walls between camera and player hide everything). 0 = off.
    pub front_cut: f32,
}

#[derive(Clone, Debug)]
pub struct Scene {
    pub camera: CameraData,
    pub sun: Sun,
    pub ambient: Ambient,
    pub point_lights: Vec<PointLight>,
    pub meshes: Vec<MeshInstance>,
    pub sdfs: Vec<SdfInstance>,
    /// Linear RGB.
    pub clear_color: Vec3,
    pub style: StyleSettings,
    pub post: PostSettings,
    pub filter: FilterSettings,
    pub cutaway: Cutaway,
    pub fog: Fog,
    /// Data for every `MeshKey::Custom` referenced this frame (uploaded when missing).
    pub custom_meshes: Vec<(MeshKey, std::sync::Arc<crate::mesh::MeshData>)>,
    /// Text in the world.
    pub texts: Vec<crate::text::Text3d>,
    /// Meshes rebuilt every frame (soft bodies).
    pub dynamic: Vec<DynamicMesh>,
    /// Particles born this frame.
    pub particles: Vec<ParticleBurst>,
    pub distortions: Vec<Distortion>,
    pub wind: Wind,
    pub time: f32,
    /// The procedural material table: row 0 is none (instances with material 0 draw exactly
    /// as before materials existed). Uploaded when `materials_version` changes, so keep the
    /// same `Arc` and version while the rows are unchanged.
    pub materials: std::sync::Arc<Vec<GpuMaterial>>,
    pub materials_version: u64,
    /// The sky behind everything (None: the clear colour, as before).
    pub sky: Option<Sky>,
    /// The open sea (None: no ocean).
    pub ocean: Option<Ocean>,
    pub rain: Option<Rain>,
    /// Fog along the view ray instead of around `fog.center` (None: the horizontal fog).
    pub fog_ray: Option<FogRay>,
    /// Skip mesh and SDF instances outside the view (their shadows are kept when they fall in
    /// the sun's shadow square or a shadowed lamp's reach).
    pub frustum_cull: bool,
    /// Distances (m from the eye) beyond which instances flagged `CULL_SMALL`,
    /// `CULL_VEGETATION` and `CULL_STRUCTURE` are not drawn (0 = no limit).
    pub cull_small: f32,
    pub cull_vegetation: f32,
    pub cull_structure: f32,
    /// Gull Sight: greys and darkens everything except `XRAY` instances (0..1).
    pub gull: f32,
}

impl Default for Scene {
    fn default() -> Self {
        Self {
            camera: CameraData::default(),
            sun: Sun::default(),
            ambient: Ambient::default(),
            point_lights: Vec::new(),
            meshes: Vec::new(),
            sdfs: Vec::new(),
            clear_color: srgb(0x8f, 0xb8, 0xd8),
            style: StyleSettings::default(),
            post: PostSettings::default(),
            filter: FilterSettings::default(),
            cutaway: Cutaway::default(),
            fog: Fog::default(),
            custom_meshes: Vec::new(),
            texts: Vec::new(),
            dynamic: Vec::new(),
            particles: Vec::new(),
            distortions: Vec::new(),
            wind: Wind::default(),
            time: 0.0,
            materials: std::sync::Arc::default(),
            materials_version: 0,
            sky: None,
            ocean: None,
            rain: None,
            fog_ray: None,
            frustum_cull: true,
            cull_small: 0.0,
            cull_vegetation: 0.0,
            cull_structure: 0.0,
            gull: 0.0,
        }
    }
}

/// Converts an 8-bit sRGB color to linear RGB.
pub fn srgb(r: u8, g: u8, b: u8) -> Vec3 {
    Vec3::new(srgb_to_linear(r as f32 / 255.0), srgb_to_linear(g as f32 / 255.0), srgb_to_linear(b as f32 / 255.0))
}

/// Parses "#rrggbb" (or "rrggbb") into linear RGB.
pub fn hex(s: &str) -> Option<Vec3> {
    let s = s.trim_start_matches('#');
    if s.len() != 6 {
        return None;
    }
    let v = u32::from_str_radix(s, 16).ok()?;
    Some(srgb((v >> 16) as u8, (v >> 8) as u8, v as u8))
}

pub fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

pub fn linear_to_srgb(c: f32) -> f32 {
    if c <= 0.0031308 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 }
}
