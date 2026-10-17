//! The sea and the sky on the GPU (`shaders/sea.wgsl`): the ocean's LOD grid and seabed
//! texture, the uniforms shared by the ocean, sky and rain passes, and their pipelines. All
//! three draw inside the scene pass (`Renderer::render`): the ocean with the opaque geometry,
//! the sky on the pixels left empty, the rain over everything. A scene with a sky or an ocean
//! always draws with reversed depth (far = 0); rain also works without them.

use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use glam::{Vec2, Vec3};

use crate::renderer::{DEPTH_FORMAT, HDR_FORMAT, NORMAL_FORMAT, SAMPLES};
use crate::scene::{Ocean, Scene, SeabedMap};

/// LOD rings of the ocean grid: spacing doubles from ring to ring (1 m near the eye, 32 m in
/// the last ring).
pub const RINGS: u32 = 6;
/// Spacing of the innermost ring (m).
pub const SPACING: f32 = 1.0;
/// How far the skirt reaches past the last ring (times its half size): to the horizon.
const SKIRT: f32 = 24.0;
/// Most streaks at full rain.
pub const MAX_RAIN: u32 = 10000;

/// The grid moves in steps of this size (m), so every ring's lattice stays fixed in the world
/// (no swimming) and each ring can morph onto the next one's lattice.
pub fn grid_snap() -> f32 {
    2.0 * SPACING * (1u32 << (RINGS - 2)) as f32
}

/// Half size (m) of ring `k` for `cells` cells across.
pub fn ring_half(cells: u32, k: u32) -> f32 {
    cells as f32 * 0.5 * SPACING * (1u32 << k) as f32
}

/// The ocean grid for `cells` cells across each ring (even, a multiple of 4): vertices
/// `[x, z, spacing, half size]` (rest position relative to the grid centre, its ring's spacing
/// and half size; spacing 0 on the skirt) and triangle indices. Ring 0 is a full square; each
/// further ring is a square annulus around the previous one with twice its spacing; the skirt
/// joins the last ring's edge to the horizon.
pub fn ocean_grid(cells: u32) -> (Vec<[f32; 4]>, Vec<u32>) {
    let cells = (cells.max(8) / 4 * 4) as i32;
    let half = cells / 2;
    let mut verts: Vec<[f32; 4]> = Vec::new();
    let mut idx: Vec<u32> = Vec::new();
    let mut outer: Vec<u32> = Vec::new();
    for k in 0..RINGS {
        let s = SPACING * (1u32 << k) as f32;
        let h = half as f32 * s;
        // Vertices of this ring (the hole's interior is the previous ring's).
        let hole = if k == 0 { -1 } else { half / 2 };
        let inside = |i: i32, j: i32| (i - half).abs() < hole && (j - half).abs() < hole;
        let mut map = vec![u32::MAX; ((cells + 1) * (cells + 1)) as usize];
        for j in 0..=cells {
            for i in 0..=cells {
                if inside(i, j) {
                    continue;
                }
                map[(j * (cells + 1) + i) as usize] = verts.len() as u32;
                // The last ring doesn't morph: its outer edge meets the skirt one to one.
                let hw = if k == RINGS - 1 { 1e9 } else { h };
                verts.push([(i - half) as f32 * s, (j - half) as f32 * s, s, hw]);
            }
        }
        let at = |i: i32, j: i32| map[(j * (cells + 1) + i) as usize];
        for j in 0..cells {
            for i in 0..cells {
                // A cell is in the hole when all its corners are on or inside the hole's edge.
                if k > 0
                    && (i - half).abs().max((i + 1 - half).abs()) <= hole
                    && (j - half).abs().max((j + 1 - half).abs()) <= hole
                {
                    continue;
                }
                let (a, b, c, d) = (at(i, j), at(i + 1, j), at(i + 1, j + 1), at(i, j + 1));
                // Every quad split along the same diagonal, so a collapsed (morphed) block of
                // four is exactly one coarse quad.
                idx.extend([a, c, b, a, d, c]);
            }
        }
        if k == RINGS - 1 {
            // The edge, walked around the square.
            for i in 0..cells {
                outer.push(at(i, 0));
            }
            for j in 0..cells {
                outer.push(at(cells, j));
            }
            for i in (1..=cells).rev() {
                outer.push(at(i, cells));
            }
            for j in (1..=cells).rev() {
                outer.push(at(0, j));
            }
        }
    }
    // The skirt: each edge vertex pushed far out along its direction from the centre.
    let first = verts.len() as u32;
    for &o in &outer {
        let v = verts[o as usize];
        verts.push([v[0] * SKIRT, v[1] * SKIRT, 0.0, 0.0]);
    }
    let n = outer.len() as u32;
    for e in 0..n {
        let f = (e + 1) % n;
        let (a, b, c, d) = (outer[e as usize], outer[f as usize], first + f, first + e);
        idx.extend([a, b, c, a, c, d]);
    }
    (verts, idx)
}

/// The rest position the vertex shader gives grid vertex `v` (its morph onto the coarser
/// lattice near its ring's edge), for tests.
pub fn morphed(v: [f32; 4]) -> Vec2 {
    let mut r = Vec2::new(v[0], v[1]);
    let s = v[2];
    if s > 0.0 {
        let edge = r.x.abs().max(r.y.abs());
        let m = ((edge - 0.72 * v[3]) / (0.2 * v[3])).clamp(0.0, 1.0);
        let gi = r / s;
        let odd = gi - 2.0 * (gi * 0.5).floor();
        r -= odd * s * m;
    }
    r
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(crate) struct SeaUniform {
    center: [f32; 4],
    wave_a: [[f32; 4]; 6],
    wave_b: [[f32; 4]; 6],
    deep: [f32; 4],
    mid: [f32; 4],
    shallow: [f32; 4],
    foam: [f32; 4],
    refl_zenith: [f32; 4],
    refl_horizon: [f32; 4],
    seabed: [f32; 4],
    seabed2: [f32; 4],
    fade: [f32; 4],
    misc: [f32; 4],
    zenith: [f32; 4],
    horizon: [f32; 4],
    ground: [f32; 4],
    to_sun: [f32; 4],
    sun_disc: [f32; 4],
    cloud: [f32; 4],
    cloud2: [f32; 4],
    cloud_lit: [f32; 4],
    cloud_shade: [f32; 4],
    rain: [f32; 4],
    rain_color: [f32; 4],
}

fn v4(v: Vec3, w: f32) -> [f32; 4] {
    [v.x, v.y, v.z, w]
}

/// The grid centre for an eye at `eye`: snapped to `grid_snap()`.
pub fn grid_center(eye: Vec3) -> Vec2 {
    let s = grid_snap();
    Vec2::new((eye.x / s).round() * s, (eye.z / s).round() * s)
}

fn pack(scene: &Scene, center: Vec2, seabed: Option<&SeabedMap>, seabed_dims: (usize, usize)) -> SeaUniform {
    let mut u = SeaUniform::zeroed();
    if let Some(o) = &scene.ocean {
        let ph = o.phases_at(center);
        for (i, w) in o.waves.iter().enumerate() {
            u.wave_a[i] = [w.dir.x, w.dir.y, w.k, w.amplitude];
            u.wave_b[i] = [w.q, ph[i], w.wavelength.max(0.1), w.omega];
        }
        u.center = [center.x, center.y, o.level, o.time];
        u.deep = v4(o.deep, o.detail);
        u.mid = v4(o.mid, o.foam_amount);
        u.shallow = v4(o.shallow, o.glint);
        u.foam = v4(o.foam, if seabed.is_some() { o.shore_foam } else { 0.0 });
        u.refl_zenith = v4(o.sky_zenith, scene.rain.map_or(0.0, |r| r.amount.clamp(0.0, 1.0)));
        u.refl_horizon = v4(o.sky_horizon, 1.0);
        if let Some(b) = seabed {
            let (w, d) = seabed_dims;
            let size = Vec2::new(b.cell * b.width as f32, b.cell * b.depth as f32);
            u.seabed = [b.origin.x, b.origin.y, 1.0 / size.x.max(1e-3), 1.0 / size.y.max(1e-3)];
            u.seabed2 = [b.deep, 1.0, 0.5 / w.max(1) as f32, 0.5 / d.max(1) as f32];
        }
        let last = ring_half(o.cells, RINGS - 1);
        u.fade = [6.0, 12.0, last * 0.6, last * 0.9];
        let wind = scene.wind.direction.normalize_or(Vec2::X);
        let qka: f32 = o.waves.iter().map(|w| w.q * w.k * w.amplitude).sum();
        let amp: f32 = o.waves.iter().map(|w| w.amplitude).sum();
        u.misc = [wind.x, wind.y, qka, amp];
    }
    if let Some(s) = &scene.sky {
        let to_sun = -s.sun_dir.normalize_or(Vec3::NEG_Y);
        let c = &s.clouds;
        u.zenith = v4(s.zenith, 1.0);
        u.horizon = v4(s.horizon, s.storm.clamp(0.0, 1.0));
        u.ground = v4(s.ground, s.flash.clamp(0.0, 1.0));
        u.to_sun = v4(to_sun, s.sun_size.max(0.01).to_radians());
        u.sun_disc = v4(s.sun_color, s.sun_glow);
        u.cloud = [c.cover.clamp(0.0, 1.0), c.density.clamp(0.0, 1.0), c.height.max(1.0), c.curve.max(0.01)];
        u.cloud2 = [c.offset.x, c.offset.y, 1.0 / c.size.max(1.0), c.time];
        u.cloud_lit = v4(c.color, 0.0);
        u.cloud_shade = v4(c.shade, 0.0);
    }
    if let Some(r) = &scene.rain {
        u.rain = [r.wind.x, r.wind.y, r.amount.clamp(0.0, 1.0), 9.0];
        u.rain_color = v4(r.color, 0.0);
    }
    u
}

/// f32 to IEEE half (round to nearest; overflow to infinity, tiny values to zero).
fn f16_bits(x: f32) -> u16 {
    let b = x.to_bits();
    let sign = ((b >> 16) & 0x8000) as u16;
    let exp = ((b >> 23) & 0xff) as i32 - 127 + 15;
    let man = b & 0x7f_ffff;
    if exp >= 31 {
        return sign | 0x7c00;
    }
    if exp <= 0 {
        if exp < -10 {
            return sign;
        }
        let m = (man | 0x80_0000) >> (1 - exp);
        return sign | ((m + 0x1000) >> 13) as u16;
    }
    let r = (exp as u32) << 10 | (man >> 13);
    // Round to nearest (a carry into the exponent is the correct result).
    let round = (man >> 12) & 1;
    sign | (r + round) as u16
}

struct Grid {
    cells: u32,
    vbuf: wgpu::Buffer,
    ibuf: wgpu::Buffer,
    indices: u32,
    vertices: u32,
}

pub(crate) struct SeaPass {
    layout: wgpu::BindGroupLayout,
    uniform: wgpu::Buffer,
    sampler: wgpu::Sampler,
    bg: wgpu::BindGroup,
    seabed: Option<Arc<SeabedMap>>,
    seabed_dims: (usize, usize),
    ocean: wgpu::RenderPipeline,
    sky: wgpu::RenderPipeline,
    /// Rain for the usual and the reversed depth direction.
    rain: [wgpu::RenderPipeline; 2],
    grid: Option<Grid>,
    // This frame.
    draw_ocean: bool,
    draw_sky: bool,
    rain_streaks: u32,
    reversed: bool,
}

impl SeaPass {
    pub(crate) fn new(device: &wgpu::Device, queue: &wgpu::Queue, globals_layout: &wgpu::BindGroupLayout) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("sea.wgsl"),
            source: wgpu::ShaderSource::Wgsl(SEA_WGSL.into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("sea"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sea"),
            size: std::mem::size_of::<SeaUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("seabed"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let dummy = Self::seabed_texture(device, queue, 1, 1, &[0]);
        let bg = Self::bind(device, &layout, &uniform, &dummy, &sampler);
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("sea"),
            bind_group_layouts: &[Some(globals_layout), Some(&layout)],
            immediate_size: 0,
        });
        let ms = wgpu::MultisampleState { count: SAMPLES, mask: !0, alpha_to_coverage_enabled: false };
        let depth = |write: bool, compare| {
            Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(write),
                depth_compare: Some(compare),
                stencil: Default::default(),
                bias: Default::default(),
            })
        };
        let premul = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
        };
        let targets = |blend: Option<wgpu::BlendState>, normals: bool| {
            [
                Some(wgpu::ColorTargetState { format: HDR_FORMAT, blend, write_mask: wgpu::ColorWrites::ALL }),
                Some(wgpu::ColorTargetState {
                    format: NORMAL_FORMAT,
                    blend: None,
                    write_mask: if normals { wgpu::ColorWrites::ALL } else { wgpu::ColorWrites::empty() },
                }),
            ]
        };
        let pipeline = |label: &str,
                        vs: &str,
                        fs: &str,
                        buffers: &[Option<wgpu::VertexBufferLayout>],
                        depth_stencil,
                        targets: &[Option<wgpu::ColorTargetState>]| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&pl),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some(vs),
                    compilation_options: Default::default(),
                    buffers,
                },
                primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
                depth_stencil,
                multisample: ms,
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(fs),
                    compilation_options: Default::default(),
                    targets,
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        const GRID_ATTR: [wgpu::VertexAttribute; 1] = wgpu::vertex_attr_array![0 => Float32x4];
        let grid_layout =
            wgpu::VertexBufferLayout { array_stride: 16, step_mode: wgpu::VertexStepMode::Vertex, attributes: &GRID_ATTR };
        let ocean = pipeline(
            "ocean",
            "vs_ocean",
            "fs_ocean",
            &[Some(grid_layout)],
            depth(true, wgpu::CompareFunction::Greater),
            &targets(None, true),
        );
        let sky =
            pipeline("sky", "vs_sky", "fs_sky", &[], depth(false, wgpu::CompareFunction::GreaterEqual), &targets(None, false));
        let rain = [wgpu::CompareFunction::LessEqual, wgpu::CompareFunction::GreaterEqual]
            .map(|c| pipeline("rain", "vs_rain", "fs_rain", &[], depth(false, c), &targets(Some(premul), false)));
        Self {
            layout,
            uniform,
            sampler,
            bg,
            seabed: None,
            seabed_dims: (1, 1),
            ocean,
            sky,
            rain,
            grid: None,
            draw_ocean: false,
            draw_sky: false,
            rain_streaks: 0,
            reversed: false,
        }
    }

    fn bind(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        uniform: &wgpu::Buffer,
        view: &wgpu::TextureView,
        sampler: &wgpu::Sampler,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("sea"),
            layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: uniform.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(view) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(sampler) },
            ],
        })
    }

    fn seabed_texture(device: &wgpu::Device, queue: &wgpu::Queue, w: usize, d: usize, texels: &[u16]) -> wgpu::TextureView {
        use wgpu::util::DeviceExt;
        device
            .create_texture_with_data(
                queue,
                &wgpu::TextureDescriptor {
                    label: Some("seabed"),
                    size: wgpu::Extent3d { width: w as u32, height: d as u32, depth_or_array_layers: 1 },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::R16Float,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                    view_formats: &[],
                },
                wgpu::util::TextureDataOrder::LayerMajor,
                bytemuck::cast_slice(texels),
            )
            .create_view(&Default::default())
    }

    /// Uploads this frame's uniforms (and the grid or seabed when they changed). `reversed`:
    /// the scene pass's depth direction (always reversed with an ocean or a sky).
    pub(crate) fn prepare(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, scene: &Scene, reversed: bool) {
        self.reversed = reversed;
        self.draw_ocean = scene.ocean.is_some() && reversed;
        self.draw_sky = scene.sky.is_some() && reversed;
        self.rain_streaks = scene.rain.map_or(0, |r| (r.amount.clamp(0.0, 1.0) * MAX_RAIN as f32) as u32);
        if !self.draw_ocean && !self.draw_sky && self.rain_streaks == 0 {
            return;
        }
        if let Some(o) = &scene.ocean {
            self.ensure_grid(device, o);
            self.ensure_seabed(device, queue, o);
        }
        let seabed = scene.ocean.as_ref().and_then(|o| o.seabed.as_deref());
        let u = pack(scene, grid_center(scene.camera.eye), seabed, self.seabed_dims);
        queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(&u));
    }

    fn ensure_grid(&mut self, device: &wgpu::Device, o: &Ocean) {
        let cells = (o.cells.clamp(16, 256) / 4) * 4;
        if self.grid.as_ref().is_some_and(|g| g.cells == cells) {
            return;
        }
        use wgpu::util::DeviceExt;
        let (verts, idx) = ocean_grid(cells);
        let vbuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("ocean grid"),
            contents: bytemuck::cast_slice(&verts),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let ibuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("ocean grid indices"),
            contents: bytemuck::cast_slice(&idx),
            usage: wgpu::BufferUsages::INDEX,
        });
        self.grid = Some(Grid { cells, vbuf, ibuf, indices: idx.len() as u32, vertices: verts.len() as u32 });
    }

    fn ensure_seabed(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, o: &Ocean) {
        let same = match (&self.seabed, &o.seabed) {
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        };
        if same {
            return;
        }
        self.seabed = o.seabed.clone();
        let view = match o.seabed.as_deref().filter(|b| b.width >= 2 && b.depth >= 2 && b.heights.len() >= b.width * b.depth) {
            Some(b) => {
                // Too big for a texture: keep every n-th height (the cell grows to match, so
                // the map still covers the same ground).
                let max = device.limits().max_texture_dimension_2d as usize;
                let step = b.width.max(b.depth).div_ceil(max).max(1);
                let (w, d) = (b.width.div_ceil(step), b.depth.div_ceil(step));
                let mut texels = Vec::with_capacity(w * d);
                for z in 0..d {
                    for x in 0..w {
                        let (sx, sz) = ((x * step).min(b.width - 1), (z * step).min(b.depth - 1));
                        texels.push(f16_bits(b.heights[sz * b.width + sx]));
                    }
                }
                self.seabed_dims = (w, d);
                Self::seabed_texture(device, queue, w, d, &texels)
            }
            None => {
                self.seabed_dims = (1, 1);
                Self::seabed_texture(device, queue, 1, 1, &[0])
            }
        };
        self.bg = Self::bind(device, &self.layout, &self.uniform, &view, &self.sampler);
    }

    /// Vertices of the ocean grid drawn this frame (0 without an ocean).
    pub(crate) fn ocean_vertices(&self) -> usize {
        if self.draw_ocean { self.grid.as_ref().map_or(0, |g| g.vertices as usize) } else { 0 }
    }

    pub(crate) fn draw_ocean(&self, pass: &mut wgpu::RenderPass<'_>, globals: &wgpu::BindGroup) -> bool {
        let Some(g) = self.grid.as_ref().filter(|_| self.draw_ocean) else { return false };
        pass.set_pipeline(&self.ocean);
        pass.set_bind_group(0, globals, &[]);
        pass.set_bind_group(1, &self.bg, &[]);
        pass.set_vertex_buffer(0, g.vbuf.slice(..));
        pass.set_index_buffer(g.ibuf.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..g.indices, 0, 0..1);
        true
    }

    pub(crate) fn draw_sky(&self, pass: &mut wgpu::RenderPass<'_>, globals: &wgpu::BindGroup) -> bool {
        if !self.draw_sky {
            return false;
        }
        pass.set_pipeline(&self.sky);
        pass.set_bind_group(0, globals, &[]);
        pass.set_bind_group(1, &self.bg, &[]);
        pass.draw(0..3, 0..1);
        true
    }

    pub(crate) fn draw_rain(&self, pass: &mut wgpu::RenderPass<'_>, globals: &wgpu::BindGroup) -> bool {
        if self.rain_streaks == 0 {
            return false;
        }
        pass.set_pipeline(&self.rain[self.reversed as usize]);
        pass.set_bind_group(0, globals, &[]);
        pass.set_bind_group(1, &self.bg, &[]);
        pass.draw(0..6, 0..self.rain_streaks);
        true
    }
}

/// The sea and sky shader, as compiled (`shaders/common.wgsl` in front of `shaders/sea.wgsl`).
pub const SEA_WGSL: &str = concat!(include_str!("shaders/common.wgsl"), include_str!("shaders/sea.wgsl"));

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_fits_the_budget_and_reaches_the_horizon() {
        let (v, i) = ocean_grid(128);
        assert!(v.len() <= 120_000, "{} vertices", v.len());
        assert_eq!(i.len() % 3, 0);
        assert!(i.iter().all(|&k| (k as usize) < v.len()));
        let far = v.iter().map(|p| p[0].abs().max(p[1].abs())).fold(0.0, f32::max);
        assert!(far > 20_000.0, "skirt reaches {far} m");
        let (v, _) = ocean_grid(64);
        assert!(v.len() < 30_000);
    }

    /// Every ring's outer edge, once morphed, lies on the next ring's lattice (no T-junctions),
    /// and its inner edge on its own (unmorphed).
    #[test]
    fn rings_meet_on_the_coarser_lattice() {
        let (v, _) = ocean_grid(64);
        for p in &v {
            let (s, h) = (p[2], p[3]);
            if s <= 0.0 || h > 1e8 {
                continue;
            }
            let edge = p[0].abs().max(p[1].abs());
            let m = morphed(*p);
            if (edge - h).abs() < 1e-3 {
                let coarse = 2.0 * s;
                assert!((m.x / coarse).fract().abs() < 1e-4 && (m.y / coarse).fract().abs() < 1e-4, "{p:?} -> {m}");
            }
            if (edge - h * 0.5).abs() < 1e-3 {
                assert_eq!(m, Vec2::new(p[0], p[1]));
            }
        }
    }

    #[test]
    fn half_floats() {
        for x in [0.0f32, 1.0, -2.5, 0.333, 12.75, -40.0, 1e-3] {
            let h = f16_bits(x);
            let sign = if h & 0x8000 != 0 { -1.0 } else { 1.0 };
            let e = ((h >> 10) & 0x1f) as i32;
            let m = (h & 0x3ff) as f32;
            let back = if e == 0 { sign * m * 2f32.powi(-24) } else { sign * (1.0 + m / 1024.0) * 2f32.powi(e - 15) };
            assert!((back - x).abs() <= x.abs() * 1e-3 + 1e-6, "{x} -> {back}");
        }
    }
}
