//! The frame renderer: shadow pass -> MSAA scene pass (HDR color + normal/group + depth)
//! -> composite (outlines, tonemap) into any target view.
//!
//! The scene pass uses reversed depth (1 at the near plane, 0 at the far plane, compared with
//! `Greater`): with a float depth buffer that keeps depth precise out to the horizon. The
//! camera's projection stays the usual one; the renderer flips it (`reversed_z`). Shadow maps
//! keep the usual direction.

use std::collections::HashMap;

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3, Vec4};
use wgpu::util::DeviceExt;

use crate::cull::{Culler, Verdict};
use crate::mesh::{MeshData, MeshKey, Vertex};
use crate::scene::{CameraData, Scene, Tonemap};

pub const HDR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
pub const NORMAL_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
pub const SHADOW_SIZE: u32 = 2048;
pub const SAMPLES: u32 = 4;
const MAX_POINT_LIGHTS: usize = 64;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Globals {
    view_proj: [[f32; 4]; 4],
    inv_view_proj: [[f32; 4]; 4],
    light_view_proj: [[f32; 4]; 4],
    eye: [f32; 4],
    forward: [f32; 4],
    sun_dir: [f32; 4],
    sun_color: [f32; 4],
    sky_color: [f32; 4],
    ground_color: [f32; 4],
    cut: [f32; 4],
    cut2: [f32; 4],
    params: [f32; 4],
    style: [f32; 4],
    viewport: [f32; 4],
    fog: [f32; 4],
    fog2: [f32; 4],
    cut3: [f32; 4],
    wind: [f32; 4],
    push: [f32; 4],
    fog3: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuPointLight {
    pos_radius: [f32; 4],
    color: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuMeshInstance {
    model: [[f32; 4]; 4],
    color: [f32; 4],
    /// style, flags, group, material (slot | seed << 12).
    params: [u32; 4],
    /// xyz: half extents for materials (0 = from the columns).
    ext: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuSdfInstance {
    a: [f32; 4],
    b: [f32; 4],
    color: [f32; 4],
    params: [u32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuGlyph {
    corner: [f32; 4],
    ax: [f32; 4],
    ay: [f32; 4],
    uv: [f32; 4],
    color: [f32; 4],
    params: [u32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct PostUniform {
    inv_proj: [[f32; 4]; 4],
    view: [[f32; 4]; 4],
    gi: [f32; 4],
    outline_color: [f32; 4],
    outline: [f32; 4],
    tone: [f32; 4],
    misc: [f32; 4],
    fwd: [f32; 4],
    filt: [[f32; 4]; 9],
    // Hazy air: unprojecting to world space, the sun's shadow map and the lights.
    inv_view: [[f32; 4]; 4],
    light_vp: [[f32; 4]; 4],
    sun_dir: [f32; 4],
    sun_color: [f32; 4],
    sky: [f32; 4],
    air: [f32; 4],
    air2: [f32; 4],
}

struct GpuMesh {
    vbuf: wgpu::Buffer,
    ibuf: wgpu::Buffer,
    index_count: u32,
    /// Bounding sphere in the mesh's own frame (centre, radius), for culling.
    bounds: (Vec3, f32),
    /// Frame counter when last used (custom meshes are evicted when unused).
    last_used: u64,
}

struct FrameTargets {
    size: (u32, u32),
    hdr_ms: wgpu::TextureView,
    hdr: wgpu::TextureView,
    normal_ms: wgpu::TextureView,
    depth_ms: wgpu::TextureView,
    post_bg: wgpu::BindGroup,
}

/// A growable GPU buffer.
pub(crate) struct DynBuffer {
    pub(crate) buf: wgpu::Buffer,
    cap: u64,
    usage: wgpu::BufferUsages,
    label: &'static str,
}

impl DynBuffer {
    pub(crate) fn new(device: &wgpu::Device, label: &'static str, usage: wgpu::BufferUsages, cap: u64) -> Self {
        let usage = usage | wgpu::BufferUsages::COPY_DST;
        let buf =
            device.create_buffer(&wgpu::BufferDescriptor { label: Some(label), size: cap, usage, mapped_at_creation: false });
        Self { buf, cap, usage, label }
    }
    /// Uploads `data`; returns true if the buffer was reallocated.
    pub(crate) fn write(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, data: &[u8]) -> bool {
        let mut grew = false;
        if data.len() as u64 > self.cap {
            self.cap = (data.len() as u64).next_power_of_two();
            self.buf = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(self.label),
                size: self.cap,
                usage: self.usage,
                mapped_at_creation: false,
            });
            grew = true;
        }
        if !data.is_empty() {
            queue.write_buffer(&self.buf, 0, data);
        }
        grew
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct RenderStats {
    pub mesh_instances: usize,
    pub sdf_instances: usize,
    pub glyphs: usize,
    pub draw_calls: usize,
    pub meshes_loaded: usize,
    /// Particle slots in use (alive or recently dead).
    pub particles: usize,
    /// Instanced mesh draws plus dynamic meshes (the main pass).
    pub batches: usize,
    /// Triangles drawn by the main pass: instances times their mesh, plus dynamic meshes.
    pub triangles: usize,
    /// GPU bytes of every loaded mesh (vertices and indices).
    pub mesh_bytes: usize,
    /// Loaded `MeshKey::Custom` meshes.
    pub custom_meshes: usize,
    /// Point lights in this frame (after the nearest-lights limit).
    pub lights: usize,
    /// Procedural material rows in the scene's table (besides row 0, none).
    pub materials: usize,
    /// Mesh and SDF instances not drawn because they were outside the view (their shadows
    /// may still be drawn), and because they were past their category's distance limit.
    pub culled_frustum: usize,
    pub culled_distance: usize,
    /// Mesh and SDF instances drawn into the shadow maps.
    pub shadow_instances: usize,
    /// Instances drawn again as Gull Sight silhouettes (`flags::XRAY`).
    pub xray_instances: usize,
    /// Vertices of the ocean grid (0 without an ocean).
    pub ocean_vertices: usize,
}

pub struct Renderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    meshes: HashMap<MeshKey, GpuMesh>,
    globals_buf: wgpu::Buffer,
    shadow_globals_buf: wgpu::Buffer,
    lights: DynBuffer,
    globals_layout: wgpu::BindGroupLayout,
    globals_bg: wgpu::BindGroup,
    shadow_bg: wgpu::BindGroup,
    shadow_view: wgpu::TextureView,
    shadow_sampler: wgpu::Sampler,
    mesh_shadow_pipeline: wgpu::RenderPipeline,
    sdf_shadow_pipeline: wgpu::RenderPipeline,
    /// The scene pass's pipelines for the usual depth direction, and for reversed depth (built
    /// the first time a scene needs them).
    pipes: ScenePipelines,
    pipes_rev: Option<ScenePipelines>,
    scene_shader: wgpu::ShaderModule,
    main_layout: wgpu::PipelineLayout,
    text_layout: wgpu::PipelineLayout,
    sea: crate::sea::SeaPass,
    post_layout: wgpu::BindGroupLayout,
    post_pipeline_layout: wgpu::PipelineLayout,
    post_shader: wgpu::ShaderModule,
    post_pipelines: HashMap<wgpu::TextureFormat, wgpu::RenderPipeline>,
    post_buf: wgpu::Buffer,
    targets: Option<FrameTargets>,
    mesh_instances: DynBuffer,
    sdf_instances: DynBuffer,
    text_bg: wgpu::BindGroup,
    text_instances: DynBuffer,
    atlas: crate::text::FontAtlas,
    dyn_vertices: DynBuffer,
    dyn_indices: DynBuffer,
    bloom: crate::fx::Bloom,
    distort: crate::fx::Distort,
    particles: crate::fx::Particles,
    lin_sampler: wgpu::Sampler,
    point_shadows: crate::shadows::PointShadows,
    upscale: crate::upscale::Upscale,
    frame: u64,
    pub stats: RenderStats,
    /// The scene is drawn at this fraction of the target's size and stretched over it
    /// (phones: a fraction of their device pixels). 1 = full resolution.
    pub render_scale: f32,
    /// Point lights that may cast shadows (each costs six depth passes), at most
    /// `shadows::MAX_SHADOW_LIGHTS`.
    pub max_shadow_lights: usize,
    /// The procedural material table (group 1, binding 2 of the mesh and SDF pipelines).
    materials: MaterialBuffer,
}

/// The scene shader: the shared globals (common.wgsl) and the material library in front of
/// scene.wgsl. `examples/dump_wgsl.rs` writes it out for `scripts/check-wgsl.sh`.
pub const SCENE_WGSL: &str =
    concat!(include_str!("shaders/common.wgsl"), include_str!("shaders/materials.wgsl"), include_str!("shaders/scene.wgsl"));

/// Every WGSL module the renderer compiles, by name, exactly as compiled.
pub const WGSL_MODULES: &[(&str, &str)] = &[
    ("scene", SCENE_WGSL),
    ("sea", crate::sea::SEA_WGSL),
    ("post", include_str!("shaders/post.wgsl")),
    ("bloom", include_str!("shaders/bloom.wgsl")),
    ("distort", include_str!("shaders/distort.wgsl")),
    ("particle_update", include_str!("shaders/particle_update.wgsl")),
    ("particle_draw", include_str!("shaders/particle_draw.wgsl")),
    ("upscale", crate::upscale::SHADER),
];

/// Rows the material buffer starts with; it grows to the next power of two.
const MATERIAL_ROWS: usize = 16;

/// The material storage buffer and its bind group, re-uploaded when the scene's
/// `materials_version` changes.
struct MaterialBuffer {
    layout: wgpu::BindGroupLayout,
    buf: wgpu::Buffer,
    bg: wgpu::BindGroup,
    rows: usize,
    version: Option<u64>,
    /// Rows in the scene's table (instances naming a row past it draw plain).
    len: usize,
}

impl MaterialBuffer {
    fn new(device: &wgpu::Device) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("materials"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let (buf, bg) = Self::alloc(device, &layout, MATERIAL_ROWS);
        Self { layout, buf, bg, rows: MATERIAL_ROWS, version: None, len: 0 }
    }

    fn alloc(device: &wgpu::Device, layout: &wgpu::BindGroupLayout, rows: usize) -> (wgpu::Buffer, wgpu::BindGroup) {
        let buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("materials"),
            size: (rows * std::mem::size_of::<crate::scene::GpuMaterial>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("materials"),
            layout,
            entries: &[wgpu::BindGroupEntry { binding: 2, resource: buf.as_entire_binding() }],
        });
        (buf, bg)
    }

    fn upload(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, scene: &Scene) {
        if self.version == Some(scene.materials_version) {
            return;
        }
        let table = scene.materials.as_slice();
        if table.len() > self.rows {
            self.rows = table.len().next_power_of_two();
            (self.buf, self.bg) = Self::alloc(device, &self.layout, self.rows);
        }
        if !table.is_empty() {
            queue.write_buffer(&self.buf, 0, bytemuck::cast_slice(table));
        }
        self.len = table.len();
        self.version = Some(scene.materials_version);
    }

    /// An instance's material word, or 0 when it names a row the table doesn't have.
    fn param(&self, material: u32) -> u32 {
        if (material & (crate::scene::MATERIAL_SLOTS - 1)) as usize >= self.len { 0 } else { material }
    }
}

fn vertex_layouts() -> [wgpu::VertexBufferLayout<'static>; 2] {
    const VERT: [wgpu::VertexAttribute; 4] =
        wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2, 9 => Float32x4];
    const INST: [wgpu::VertexAttribute; 7] = wgpu::vertex_attr_array![
        3 => Float32x4, 4 => Float32x4, 5 => Float32x4, 6 => Float32x4, 7 => Float32x4, 8 => Uint32x4, 10 => Float32x4
    ];
    [
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &VERT,
        },
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<GpuMeshInstance>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &INST,
        },
    ]
}

fn sdf_layout() -> wgpu::VertexBufferLayout<'static> {
    const ATTR: [wgpu::VertexAttribute; 4] =
        wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4, 2 => Float32x4, 3 => Uint32x4];
    wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<GpuSdfInstance>() as u64,
        step_mode: wgpu::VertexStepMode::Instance,
        attributes: &ATTR,
    }
}

fn glyph_layout() -> wgpu::VertexBufferLayout<'static> {
    const ATTR: [wgpu::VertexAttribute; 6] = wgpu::vertex_attr_array![
        0 => Float32x4, 1 => Float32x4, 2 => Float32x4, 3 => Float32x4, 4 => Float32x4, 5 => Uint32x4
    ];
    wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<GpuGlyph>() as u64,
        step_mode: wgpu::VertexStepMode::Instance,
        attributes: &ATTR,
    }
}

fn mat(m: Mat4) -> [[f32; 4]; 4] {
    m.to_cols_array_2d()
}

/// `proj` with depth reversed: z' = w - z, so the near plane maps to 1 and the far plane to 0.
pub fn reversed_z(proj: Mat4) -> Mat4 {
    Mat4::from_cols(Vec4::X, Vec4::Y, Vec4::new(0.0, 0.0, -1.0, 0.0), Vec4::new(0.0, 0.0, 1.0, 1.0)) * proj
}

/// The bounding sphere of a mesh's vertices (centre of their box, farthest vertex).
fn mesh_bounds(data: &MeshData) -> (Vec3, f32) {
    if data.vertices.is_empty() {
        return (Vec3::ZERO, 0.0);
    }
    let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    for v in &data.vertices {
        let p = Vec3::from(v.pos);
        lo = lo.min(p);
        hi = hi.max(p);
    }
    let c = (lo + hi) * 0.5;
    let r = data.vertices.iter().map(|v| Vec3::from(v.pos).distance(c)).fold(0.0, f32::max);
    (c, r)
}

/// The scene pass's pipelines whose depth test depends on the depth direction.
struct ScenePipelines {
    mesh: wgpu::RenderPipeline,
    sdf: wgpu::RenderPipeline,
    text: wgpu::RenderPipeline,
    mesh_xray: wgpu::RenderPipeline,
    sdf_xray: wgpu::RenderPipeline,
}

impl ScenePipelines {
    /// `reversed`: depth 1 at the near plane, 0 at the far plane (nearer = greater).
    fn new(
        device: &wgpu::Device,
        shader: &wgpu::ShaderModule,
        main_layout: &wgpu::PipelineLayout,
        text_layout: &wgpu::PipelineLayout,
        reversed: bool,
    ) -> Self {
        use wgpu::CompareFunction as C;
        let (nearer, nearer_eq, farther) =
            if reversed { (C::Greater, C::GreaterEqual, C::Less) } else { (C::Less, C::LessEqual, C::Greater) };
        let color_targets = [
            Some(wgpu::ColorTargetState { format: HDR_FORMAT, blend: None, write_mask: wgpu::ColorWrites::ALL }),
            Some(wgpu::ColorTargetState { format: NORMAL_FORMAT, blend: None, write_mask: wgpu::ColorWrites::ALL }),
        ];
        let depth_main = Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(true),
            depth_compare: Some(nearer),
            stencil: Default::default(),
            bias: Default::default(),
        });
        // Gull Sight: only where something nearer hides the instance, without writing depth;
        // the colour blends over the scene with a constant 0.45 of it left (the shader
        // premultiplies by 0.55) and the alpha keeps the larger marker.
        let depth_xray = Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(false),
            depth_compare: Some(farther),
            stencil: Default::default(),
            bias: Default::default(),
        });
        let xray_targets = [
            Some(wgpu::ColorTargetState {
                format: HDR_FORMAT,
                blend: Some(wgpu::BlendState {
                    color: wgpu::BlendComponent {
                        src_factor: wgpu::BlendFactor::One,
                        dst_factor: wgpu::BlendFactor::Constant,
                        operation: wgpu::BlendOperation::Add,
                    },
                    alpha: wgpu::BlendComponent {
                        src_factor: wgpu::BlendFactor::One,
                        dst_factor: wgpu::BlendFactor::One,
                        operation: wgpu::BlendOperation::Max,
                    },
                }),
                write_mask: wgpu::ColorWrites::ALL,
            }),
            Some(wgpu::ColorTargetState { format: NORMAL_FORMAT, blend: None, write_mask: wgpu::ColorWrites::empty() }),
        ];
        let ms = wgpu::MultisampleState { count: SAMPLES, mask: !0, alpha_to_coverage_enabled: false };
        let vl = vertex_layouts();
        let sdf_buffers = [sdf_layout()];
        let pipeline = |label: &str,
                        layout: &wgpu::PipelineLayout,
                        vs: &str,
                        fs: &str,
                        buffers: &[Option<wgpu::VertexBufferLayout>],
                        cull_mode: Option<wgpu::Face>,
                        depth_stencil: Option<wgpu::DepthStencilState>,
                        multisample: wgpu::MultisampleState,
                        targets: &[Option<wgpu::ColorTargetState>]| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(layout),
                vertex: wgpu::VertexState {
                    module: shader,
                    entry_point: Some(vs),
                    compilation_options: Default::default(),
                    buffers,
                },
                primitive: wgpu::PrimitiveState { cull_mode, ..Default::default() },
                depth_stencil,
                multisample,
                fragment: Some(wgpu::FragmentState {
                    module: shader,
                    entry_point: Some(fs),
                    compilation_options: Default::default(),
                    targets,
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let mesh_buffers = [Some(vl[0].clone()), Some(vl[1].clone())];
        let sdf_buffers = [Some(sdf_buffers[0].clone())];
        // No culling: back faces become visible only where cutaway opened a solid, and are
        // drawn as flat caps.
        let mesh =
            pipeline("mesh", main_layout, "vs_mesh", "fs_mesh", &mesh_buffers, None, depth_main.clone(), ms, &color_targets);
        let sdf = pipeline(
            "sdf",
            main_layout,
            "vs_sdf",
            "fs_sdf",
            &sdf_buffers,
            Some(wgpu::Face::Front),
            depth_main,
            ms,
            &color_targets,
        );
        let mesh_xray = pipeline(
            "mesh xray",
            main_layout,
            "vs_mesh",
            "fs_xray",
            &mesh_buffers,
            Some(wgpu::Face::Back),
            depth_xray.clone(),
            ms,
            &xray_targets,
        );
        let sdf_xray = pipeline(
            "sdf xray",
            main_layout,
            "vs_sdf",
            "fs_sdf_xray",
            &sdf_buffers,
            Some(wgpu::Face::Front),
            depth_xray,
            ms,
            &xray_targets,
        );
        // Text: an alpha-to-coverage pipeline pulled a little toward the camera.
        let bias = if reversed { 1 } else { -1 };
        let text = pipeline(
            "text",
            text_layout,
            "vs_text",
            "fs_text",
            &[Some(glyph_layout())],
            None,
            Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(nearer_eq),
                stencil: Default::default(),
                bias: wgpu::DepthBiasState { constant: 4 * bias, slope_scale: bias as f32, clamp: 0.0 },
            }),
            wgpu::MultisampleState { count: SAMPLES, mask: !0, alpha_to_coverage_enabled: true },
            &color_targets,
        );
        Self { mesh, sdf, text, mesh_xray, sdf_xray }
    }
}

impl Renderer {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let scene_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("scene.wgsl"),
            source: wgpu::ShaderSource::Wgsl(SCENE_WGSL.into()),
        });
        let post_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("post.wgsl"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/post.wgsl").into()),
        });

        let uniform_entry = |binding: u32| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let globals_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("globals"),
            entries: &[
                uniform_entry(0),
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                    count: None,
                },
                // Point light shadow faces and their matrices.
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 5,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let shadow_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("shadow globals"),
            entries: &[uniform_entry(0)],
        });

        let globals_size = std::mem::size_of::<Globals>() as u64;
        let globals_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("globals"),
            size: globals_size,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let shadow_globals_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("shadow globals"),
            size: globals_size,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let lights = DynBuffer::new(
            device,
            "point lights",
            wgpu::BufferUsages::STORAGE,
            (std::mem::size_of::<GpuPointLight>() * MAX_POINT_LIGHTS) as u64,
        );

        let shadow_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("shadow map"),
            size: wgpu::Extent3d { width: SHADOW_SIZE, height: SHADOW_SIZE, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let shadow_view = shadow_tex.create_view(&Default::default());
        let shadow_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("shadow sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        });

        let point_shadows = crate::shadows::PointShadows::new(device, &shadow_layout, globals_size);
        let globals_bg = Self::make_globals_bg(
            device,
            &globals_layout,
            &globals_buf,
            &lights.buf,
            &shadow_view,
            &shadow_sampler,
            &point_shadows,
        );
        let shadow_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("shadow globals"),
            layout: &shadow_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: shadow_globals_buf.as_entire_binding() }],
        });

        let materials = MaterialBuffer::new(device);
        let sea = crate::sea::SeaPass::new(device, queue, &globals_layout);
        let main_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("main"),
            bind_group_layouts: &[Some(&globals_layout), Some(&materials.layout)],
            immediate_size: 0,
        });
        let shadow_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("shadow"),
            bind_group_layouts: &[Some(&shadow_layout)],
            immediate_size: 0,
        });

        let depth_shadow = Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::Less),
            stencil: Default::default(),
            bias: wgpu::DepthBiasState { constant: 2, slope_scale: 2.0, clamp: 0.0 },
        });

        let vl = vertex_layouts();
        let sdf_buffers = [sdf_layout()];
        let mesh_shadow_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("mesh shadow"),
            layout: Some(&shadow_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &scene_shader,
                entry_point: Some("vs_mesh_shadow"),
                compilation_options: Default::default(),
                buffers: &[Some(vl[0].clone()), Some(vl[1].clone())],
            },
            primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
            depth_stencil: depth_shadow.clone(),
            multisample: Default::default(),
            fragment: None,
            multiview_mask: None,
            cache: None,
        });
        let sdf_shadow_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("sdf shadow"),
            layout: Some(&shadow_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &scene_shader,
                entry_point: Some("vs_sdf"),
                compilation_options: Default::default(),
                buffers: &[Some(sdf_buffers[0].clone())],
            },
            primitive: wgpu::PrimitiveState { cull_mode: Some(wgpu::Face::Front), ..Default::default() },
            depth_stencil: depth_shadow,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &scene_shader,
                entry_point: Some("fs_sdf_shadow"),
                compilation_options: Default::default(),
                targets: &[],
            }),
            multiview_mask: None,
            cache: None,
        });

        let post_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("post"),
            entries: &[
                uniform_entry(0),
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
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: true,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: true,
                    },
                    count: None,
                },
                // Bloom glow, distortion offsets, linear sampler.
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 5,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 6,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                // Hazy air: the sun's shadow map, its comparison sampler and the point lights.
                wgpu::BindGroupLayoutEntry {
                    binding: 7,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 8,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 9,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let post_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("post"),
            bind_group_layouts: &[Some(&post_layout)],
            immediate_size: 0,
        });
        let post_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("post"),
            size: std::mem::size_of::<PostUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Text: SDF font atlas + an alpha-to-coverage pipeline.
        let atlas = crate::text::FontAtlas::new();
        let atlas_tex = device.create_texture_with_data(
            queue,
            &wgpu::TextureDescriptor {
                label: Some("font atlas"),
                size: wgpu::Extent3d { width: atlas.width, height: atlas.height, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::R8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            &atlas.pixels,
        );
        let atlas_view = atlas_tex.create_view(&Default::default());
        let atlas_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("font sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let text_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("text"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let text_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("text"),
            layout: &text_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&atlas_view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&atlas_sampler) },
            ],
        });
        let text_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("text"),
            bind_group_layouts: &[Some(&globals_layout), Some(&text_layout)],
            immediate_size: 0,
        });
        let pipes = ScenePipelines::new(device, &scene_shader, &main_layout, &text_pipeline_layout, false);
        let text_instances = DynBuffer::new(device, "glyphs", wgpu::BufferUsages::VERTEX, 16 * 1024);
        let dyn_vertices = DynBuffer::new(device, "dynamic vertices", wgpu::BufferUsages::VERTEX, 64 * 1024);
        let dyn_indices = DynBuffer::new(device, "dynamic indices", wgpu::BufferUsages::INDEX, 32 * 1024);

        let mesh_instances = DynBuffer::new(device, "mesh instances", wgpu::BufferUsages::VERTEX, 64 * 1024);
        let sdf_instances = DynBuffer::new(device, "sdf instances", wgpu::BufferUsages::VERTEX, 16 * 1024);

        Self {
            device: device.clone(),
            queue: queue.clone(),
            meshes: HashMap::new(),
            globals_buf,
            shadow_globals_buf,
            lights,
            globals_layout,
            globals_bg,
            shadow_bg,
            shadow_view,
            shadow_sampler,
            mesh_shadow_pipeline,
            sdf_shadow_pipeline,
            pipes,
            pipes_rev: None,
            scene_shader,
            main_layout,
            text_layout: text_pipeline_layout,
            sea,
            post_layout,
            post_pipeline_layout,
            post_shader,
            post_pipelines: HashMap::new(),
            post_buf,
            targets: None,
            mesh_instances,
            sdf_instances,
            text_bg,
            text_instances,
            atlas,
            dyn_vertices,
            dyn_indices,
            bloom: crate::fx::Bloom::new(device),
            distort: crate::fx::Distort::new(device),
            particles: crate::fx::Particles::new(device),
            lin_sampler: crate::fx::linear_sampler(device),
            point_shadows,
            upscale: crate::upscale::Upscale::new(device),
            frame: 0,
            stats: RenderStats::default(),
            render_scale: 1.0,
            max_shadow_lights: crate::shadows::MAX_SHADOW_LIGHTS,
            materials,
        }
    }

    fn make_globals_bg(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        globals: &wgpu::Buffer,
        lights: &wgpu::Buffer,
        shadow_view: &wgpu::TextureView,
        shadow_sampler: &wgpu::Sampler,
        points: &crate::shadows::PointShadows,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("globals"),
            layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: globals.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: lights.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(shadow_view) },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::Sampler(shadow_sampler) },
                wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::TextureView(&points.view) },
                wgpu::BindGroupEntry { binding: 5, resource: points.mats.as_entire_binding() },
            ],
        })
    }

    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    /// Clears transient particles when switching between the game and an isolated preview.
    pub fn clear_particles(&mut self) {
        self.particles.clear();
    }

    /// Uploads (or replaces) a mesh under `key`.
    pub fn upsert_mesh(&mut self, key: MeshKey, data: &MeshData) {
        let vbuf = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("mesh vertices"),
            contents: bytemuck::cast_slice(&data.vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let ibuf = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("mesh indices"),
            contents: bytemuck::cast_slice(&data.indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        self.meshes.insert(
            key,
            GpuMesh { vbuf, ibuf, index_count: data.indices.len() as u32, bounds: mesh_bounds(data), last_used: self.frame },
        );
    }

    pub fn has_mesh(&self, key: MeshKey) -> bool {
        self.meshes.contains_key(&key)
    }

    pub fn remove_mesh(&mut self, key: MeshKey) {
        self.meshes.remove(&key);
    }

    fn ensure_targets(&mut self, size: (u32, u32)) {
        if self.targets.as_ref().is_some_and(|t| t.size == size) {
            return;
        }
        let extent = wgpu::Extent3d { width: size.0.max(1), height: size.1.max(1), depth_or_array_layers: 1 };
        let make = |label: &str, format: wgpu::TextureFormat, samples: u32, usage: wgpu::TextureUsages| {
            self.device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: extent,
                    mip_level_count: 1,
                    sample_count: samples,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage,
                    view_formats: &[],
                })
                .create_view(&Default::default())
        };
        let rt = wgpu::TextureUsages::RENDER_ATTACHMENT;
        let tb = wgpu::TextureUsages::TEXTURE_BINDING;
        let hdr_ms = make("hdr ms", HDR_FORMAT, SAMPLES, rt);
        let hdr = make("hdr", HDR_FORMAT, 1, rt | tb);
        let normal_ms = make("normal ms", NORMAL_FORMAT, SAMPLES, rt | tb);
        let depth_ms = make("depth ms", DEPTH_FORMAT, SAMPLES, rt | tb);
        self.bloom.resize(&self.device, &self.queue, size, &hdr);
        self.distort.resize(&self.device, size);
        let post_bg = self.make_post_bg(&hdr, &depth_ms, &normal_ms);
        self.targets = Some(FrameTargets { size, hdr_ms, hdr, normal_ms, depth_ms, post_bg });
    }

    fn make_post_bg(
        &self,
        hdr: &wgpu::TextureView,
        depth_ms: &wgpu::TextureView,
        normal_ms: &wgpu::TextureView,
    ) -> wgpu::BindGroup {
        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("post"),
            layout: &self.post_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: self.post_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(hdr) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(depth_ms) },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(normal_ms) },
                wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::TextureView(self.bloom.view().unwrap()) },
                wgpu::BindGroupEntry { binding: 5, resource: wgpu::BindingResource::TextureView(self.distort.view().unwrap()) },
                wgpu::BindGroupEntry { binding: 6, resource: wgpu::BindingResource::Sampler(&self.lin_sampler) },
                wgpu::BindGroupEntry { binding: 7, resource: wgpu::BindingResource::TextureView(&self.shadow_view) },
                wgpu::BindGroupEntry { binding: 8, resource: wgpu::BindingResource::Sampler(&self.shadow_sampler) },
                wgpu::BindGroupEntry { binding: 9, resource: self.lights.buf.as_entire_binding() },
            ],
        })
    }

    fn post_pipeline(&mut self, format: wgpu::TextureFormat) -> &wgpu::RenderPipeline {
        if !self.post_pipelines.contains_key(&format) {
            let p = self.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("post"),
                layout: Some(&self.post_pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &self.post_shader,
                    entry_point: Some("vs_full"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &self.post_shader,
                    entry_point: Some("fs_post"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState { format, blend: None, write_mask: wgpu::ColorWrites::ALL })],
                }),
                multiview_mask: None,
                cache: None,
            });
            self.post_pipelines.insert(format, p);
        }
        &self.post_pipelines[&format]
    }

    /// Light view-projection for the sun, snapped to shadow texels to avoid shimmering.
    fn light_matrix(scene: &Scene) -> (Mat4, f32) {
        let dir = scene.sun.direction.normalize_or(Vec3::NEG_Y);
        let r = scene.sun.shadow_radius.max(1.0);
        let up = if dir.y.abs() > 0.99 { Vec3::Z } else { Vec3::Y };
        let right = dir.cross(up).normalize();
        let up2 = right.cross(dir);
        let texel = 2.0 * r / SHADOW_SIZE as f32;
        let c = scene.sun.shadow_center;
        let (cx, cy) = (c.dot(right), c.dot(up2));
        let snapped = c + right * ((cx / texel).round() * texel - cx) + up2 * ((cy / texel).round() * texel - cy);
        let depth = r * 6.0;
        let view = glam::camera::rh::view::look_to_mat4(snapped - dir * depth * 0.5, dir, up2);
        let proj = glam::camera::rh::proj::directx::orthographic(-r, r, -r, r, 0.0, depth);
        (proj * view, texel)
    }

    /// The size the scene is drawn at for a target of `size` (see `render_scale`).
    pub fn internal_size(&self, size: (u32, u32)) -> (u32, u32) {
        let s = self.render_scale.clamp(0.1, 1.0);
        if s >= 0.999 {
            return size;
        }
        (((size.0 as f32 * s).round() as u32).max(1), ((size.1 as f32 * s).round() as u32).max(1))
    }

    /// Records the whole frame into `encoder`, writing the final image into `target`.
    pub fn render(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        scene: &Scene,
        target: &wgpu::TextureView,
        target_format: wgpu::TextureFormat,
        size: (u32, u32),
    ) {
        let full = size;
        let size = self.internal_size(full);
        self.ensure_targets(size);
        self.frame += 1;

        // Custom meshes: upload missing ones, evict ones unused for a while.
        for (key, data) in &scene.custom_meshes {
            match self.meshes.get_mut(key) {
                Some(m) => m.last_used = self.frame,
                None => self.upsert_mesh(*key, data),
            }
        }
        let frame = self.frame;
        self.meshes.retain(|k, m| !matches!(k, MeshKey::Custom(_)) || frame - m.last_used < 300);

        // Make sure every referenced mesh exists.
        for inst in &scene.meshes {
            if !self.meshes.contains_key(&inst.mesh) {
                if let Some(data) = inst.mesh.generate() {
                    self.upsert_mesh(inst.mesh, &data);
                }
            }
        }

        // Globals. Open-world scenes (with a sky or an ocean) draw with reversed depth (see the
        // module docs); the others keep the usual direction, exactly as before.
        let reversed = scene.sky.is_some() || scene.ocean.is_some();
        if reversed && self.pipes_rev.is_none() {
            self.pipes_rev =
                Some(ScenePipelines::new(&self.device, &self.scene_shader, &self.main_layout, &self.text_layout, true));
        }
        let cam = &scene.camera;
        let rcam = if reversed { CameraData { proj: reversed_z(cam.proj), ..*cam } } else { *cam };
        let view_proj = rcam.proj * cam.view;
        let (light_vp, texel) = Self::light_matrix(scene);
        let sun_dir = scene.sun.direction.normalize_or(Vec3::NEG_Y);
        // The lights nearest the point of interest, if there are too many.
        let mut light_order: Vec<usize> = (0..scene.point_lights.len()).collect();
        if light_order.len() > MAX_POINT_LIGHTS {
            let focus = scene.cutaway.focus;
            let score = |i: usize| {
                let l = &scene.point_lights[i];
                (l.position.distance(focus) - l.radius).max(0.0)
            };
            light_order.sort_by(|&a, &b| score(a).total_cmp(&score(b)));
            light_order.truncate(MAX_POINT_LIGHTS);
        }
        let n_lights = light_order.len();
        let cut = &scene.cutaway;
        let mut globals = Globals {
            view_proj: mat(view_proj),
            inv_view_proj: mat(view_proj.inverse()),
            light_view_proj: mat(light_vp),
            eye: [cam.eye.x, cam.eye.y, cam.eye.z, if cam.ortho { 0.0 } else { 1.0 }],
            forward: cam.forward.normalize_or(Vec3::NEG_Z).extend(0.0).to_array(),
            sun_dir: [sun_dir.x, sun_dir.y, sun_dir.z, if scene.sun.shadows { 1.0 } else { 0.0 }],
            sun_color: scene.sun.color.extend(1.0).to_array(),
            sky_color: scene.ambient.sky.extend(1.0).to_array(),
            ground_color: scene.ambient.ground.extend(1.0).to_array(),
            cut: [cut.focus.x, cut.focus.y, cut.focus.z, cut.cut_height],
            cut2: [cut.cut_radius, if cut.height_cut { 1.0 } else { 0.0 }, cut.fade_radius, if cut.fade { 1.0 } else { 0.0 }],
            // w: metres per pixel at 1 m (perspective) or in all (orthographic), for materials.
            params: [scene.time, n_lights as f32, texel, 2.0 / (cam.proj.y_axis.y.abs().max(1e-6) * size.1.max(1) as f32)],
            style: [scene.style.flat_shadow, scene.style.cel_bands, scene.style.rim, scene.style.specular],
            viewport: [size.0 as f32, size.1 as f32, 1.0 / size.0.max(1) as f32, 1.0 / size.1.max(1) as f32],
            fog: scene.fog.color.extend(if scene.fog.enabled { 1.0 } else { 0.0 }).to_array(),
            fog2: [scene.fog.center.x, scene.fog.center.z, scene.fog.start, scene.fog.end.max(scene.fog.start + 0.01)],
            cut3: [cut.front_cut, if cut.front_cut > 0.0 { 1.0 } else { 0.0 }, 0.0, 0.0],
            wind: {
                let w = &scene.wind;
                let d = w.direction.normalize_or(glam::Vec2::X);
                [d.x, d.y, w.strength, w.gusts]
            },
            push: scene.wind.pusher.extend(scene.wind.push_radius).to_array(),
            fog3: match &scene.fog_ray {
                Some(r) => [1.0, r.height_falloff.max(0.0), r.base, r.sun_glow.max(0.0)],
                None => [0.0; 4],
            },
        };
        self.queue.write_buffer(&self.globals_buf, 0, bytemuck::bytes_of(&globals));
        self.materials.upload(&self.device, &self.queue, scene);
        self.sea.prepare(&self.device, &self.queue, scene, reversed);
        let globals_sun_dir = globals.sun_dir;
        globals.view_proj = mat(light_vp);
        globals.eye = [0.0, 0.0, 0.0, 0.0];
        globals.forward = sun_dir.extend(0.0).to_array();
        globals.cut2 = [0.0; 4];
        globals.cut3 = [0.0; 4];
        self.queue.write_buffer(&self.shadow_globals_buf, 0, bytemuck::bytes_of(&globals));

        // Shadow-casting point lights nearest the camera target get the shadow slots.
        let mut casters: Vec<usize> = light_order.iter().copied().filter(|&i| scene.point_lights[i].shadows).collect();
        casters.sort_by(|&a, &b| {
            let d = |i: usize| scene.point_lights[i].position.distance_squared(scene.cutaway.focus);
            d(a).total_cmp(&d(b))
        });
        casters.truncate(crate::shadows::MAX_SHADOW_LIGHTS.min(self.max_shadow_lights));
        let shadow_slots = casters.len();
        let mut mats: Vec<[[f32; 4]; 4]> = Vec::with_capacity(shadow_slots * 6);
        for (slot, &i) in casters.iter().enumerate() {
            let l = &scene.point_lights[i];
            for (f, (dir, m)) in crate::shadows::face_matrices(l.position, l.radius).into_iter().enumerate() {
                mats.push(mat(m));
                let mut g = globals;
                g.view_proj = mat(m);
                g.eye = [l.position.x, l.position.y, l.position.z, 1.0];
                g.forward = dir.extend(0.0).to_array();
                self.queue.write_buffer(&self.point_shadows.faces[slot * 6 + f].0, 0, bytemuck::bytes_of(&g));
            }
        }
        if !mats.is_empty() {
            self.queue.write_buffer(&self.point_shadows.mats, 0, bytemuck::cast_slice(&mats));
        }
        let mut lights: Vec<GpuPointLight> = light_order
            .iter()
            .map(|&i| (i, &scene.point_lights[i]))
            .map(|(i, l)| GpuPointLight {
                pos_radius: [l.position.x, l.position.y, l.position.z, l.radius],
                // w = shadow slot + 1 (0 = no shadows).
                color: l.color.extend(casters.iter().position(|&c| c == i).map(|s| s as f32 + 1.0).unwrap_or(0.0)).to_array(),
            })
            .collect();
        if lights.is_empty() {
            lights.push(GpuPointLight::zeroed());
        }
        if self.lights.write(&self.device, &self.queue, bytemuck::cast_slice(&lights)) {
            // A new lights buffer: the composite reads it too (lamp halos).
            if let Some(t) = &self.targets {
                let bg = self.make_post_bg(&t.hdr, &t.depth_ms, &t.normal_ms);
                self.targets.as_mut().unwrap().post_bg = bg;
            }
            self.globals_bg = Self::make_globals_bg(
                &self.device,
                &self.globals_layout,
                &self.globals_buf,
                &self.lights.buf,
                &self.shadow_view,
                &self.shadow_sampler,
                &self.point_shadows,
            );
        }

        // Culling: what the camera sees is drawn; what shadows the visible area casts.
        let lamps = casters.iter().map(|&i| (scene.point_lights[i].position, scene.point_lights[i].radius)).collect();
        let culler = Culler::new(scene, cam.proj * cam.view, scene.sun.shadows.then_some(light_vp), lamps);
        let (mut culled_frustum, mut culled_distance) = (0, 0);
        let mut tally = |v: Verdict| {
            if v.far {
                culled_distance += 1;
            } else if !v.draw {
                culled_frustum += 1;
            }
        };

        // Instances, grouped by mesh: the drawn ones, the dynamic meshes, then the shadow
        // casters and the Gull Sight silhouettes, each in runs of one mesh.
        let mut order: Vec<(usize, Verdict)> = Vec::with_capacity(scene.meshes.len());
        for (i, m) in scene.meshes.iter().enumerate() {
            let Some(gm) = self.meshes.get(&m.mesh) else { continue };
            let (c, r) = Culler::mesh_sphere(&m.transform, gm.bounds.0, gm.bounds.1);
            let v = culler.test(c, r, m.flags);
            tally(v);
            if v.draw || v.shadow {
                order.push((i, v));
            }
        }
        order.sort_by_key(|&(i, _)| scene.meshes[i].mesh);
        let materials = &self.materials;
        let gpu_of = |m: &crate::scene::MeshInstance| GpuMeshInstance {
            model: mat(m.transform),
            color: m.color.extend(m.emissive).to_array(),
            params: [m.style as u32, m.flags, m.group, materials.param(m.material)],
            ext: m.ext.extend(0.0).to_array(),
        };
        let mut gpu_meshes: Vec<GpuMeshInstance> = Vec::with_capacity(order.len() * 2);
        // Appends the instances `pick` chooses, returning their runs (mesh, first, end).
        let runs = |gpu_meshes: &mut Vec<GpuMeshInstance>, pick: &dyn Fn(u32, Verdict) -> bool| {
            let mut out: Vec<(MeshKey, u32, u32)> = Vec::new();
            for &(i, v) in &order {
                let m = &scene.meshes[i];
                if !pick(m.flags, v) {
                    continue;
                }
                let idx = gpu_meshes.len() as u32;
                gpu_meshes.push(gpu_of(m));
                match out.last_mut() {
                    Some(b) if b.0 == m.mesh => b.2 = idx + 1,
                    _ => out.push((m.mesh, idx, idx + 1)),
                }
            }
            out
        };
        let batches = runs(&mut gpu_meshes, &|_, v| v.draw);
        let drawn_meshes = gpu_meshes.len();
        // Dynamic meshes: one instance each (identity transform), after the regular ones.
        let mut dyn_draws: Vec<(u32, u32, i32, u32, bool)> = Vec::new(); // first index, count, base vertex, instance, shadow
        {
            let mut verts: Vec<Vertex> = Vec::new();
            let mut idx: Vec<u32> = Vec::new();
            for d in &scene.dynamic {
                if d.data.indices.is_empty() {
                    continue;
                }
                let inst = gpu_meshes.len() as u32;
                gpu_meshes.push(GpuMeshInstance {
                    model: mat(Mat4::IDENTITY),
                    color: d.color.extend(d.emissive).to_array(),
                    params: [d.style as u32, d.flags, d.group, self.materials.param(d.material)],
                    ext: [0.0; 4],
                });
                dyn_draws.push((
                    idx.len() as u32,
                    d.data.indices.len() as u32,
                    verts.len() as i32,
                    inst,
                    d.flags & crate::scene::flags::NO_SHADOW == 0,
                ));
                verts.extend_from_slice(&d.data.vertices);
                idx.extend_from_slice(&d.data.indices);
            }
            if !dyn_draws.is_empty() {
                self.dyn_vertices.write(&self.device, &self.queue, bytemuck::cast_slice(&verts));
                self.dyn_indices.write(&self.device, &self.queue, bytemuck::cast_slice(&idx));
            }
        }
        let main_batches = batches.len() + dyn_draws.len();
        let main_triangles =
            batches.iter().map(|b| (b.2 - b.1) as usize * self.meshes[&b.0].index_count as usize / 3).sum::<usize>()
                + dyn_draws.iter().map(|d| d.1 as usize / 3).sum::<usize>();
        let shadow_start = gpu_meshes.len();
        let shadow_batches = runs(&mut gpu_meshes, &|f, v| v.shadow && f & crate::scene::flags::NO_SHADOW == 0);
        let shadow_meshes = gpu_meshes.len() - shadow_start;
        let xray_start = gpu_meshes.len();
        let xray_batches = runs(&mut gpu_meshes, &|f, v| v.draw && f & crate::scene::flags::XRAY != 0);
        let xray_meshes = gpu_meshes.len() - xray_start;
        self.mesh_instances.write(&self.device, &self.queue, bytemuck::cast_slice(&gpu_meshes));

        // SDFs: the drawn ones, the shadow casters, the silhouettes.
        let mut gpu_sdfs: Vec<GpuSdfInstance> = Vec::with_capacity(scene.sdfs.len());
        let mut sdf_shadow: Vec<GpuSdfInstance> = Vec::new();
        let mut sdf_xray: Vec<GpuSdfInstance> = Vec::new();
        for s in &scene.sdfs {
            let v = culler.test((s.a + s.b) * 0.5, (s.b - s.a).length() * 0.5 + s.ra.max(s.rb), s.flags);
            tally(v);
            let inst = GpuSdfInstance {
                a: [s.a.x, s.a.y, s.a.z, s.ra],
                b: [s.b.x, s.b.y, s.b.z, s.rb],
                color: s.color.extend(s.emissive).to_array(),
                params: [s.style as u32, s.flags, s.group, self.materials.param(s.material)],
            };
            if v.draw {
                gpu_sdfs.push(inst);
                if s.flags & crate::scene::flags::XRAY != 0 {
                    sdf_xray.push(inst);
                }
            }
            if v.shadow && s.flags & crate::scene::flags::NO_SHADOW == 0 {
                sdf_shadow.push(GpuSdfInstance { color: [0.0; 4], params: [0; 4], ..inst });
            }
        }
        let sdf_count = gpu_sdfs.len() as u32;
        gpu_sdfs.extend_from_slice(&sdf_shadow);
        let sdf_shadow_end = gpu_sdfs.len() as u32;
        gpu_sdfs.extend_from_slice(&sdf_xray);
        let sdf_xray_end = gpu_sdfs.len() as u32;
        self.sdf_instances.write(&self.device, &self.queue, bytemuck::cast_slice(&gpu_sdfs));

        let mut quads = Vec::new();
        let mut gpu_glyphs: Vec<GpuGlyph> = Vec::new();
        for t in &scene.texts {
            quads.clear();
            crate::text::layout(&self.atlas, t, &mut quads);
            let n = t.right.cross(t.up).normalize_or(Vec3::Y);
            for q in &quads {
                gpu_glyphs.push(GpuGlyph {
                    corner: q.corner.extend(n.x).to_array(),
                    ax: q.ax.extend(n.y).to_array(),
                    ay: q.ay.extend(n.z).to_array(),
                    uv: q.uv,
                    color: t.color.extend(t.weight).to_array(),
                    // Group 0: no screen-space outlines around glyph strokes (they speckle small text).
                    params: [t.flags, 0, 0, 0],
                });
            }
        }
        let glyph_count = gpu_glyphs.len() as u32;
        if glyph_count > 0 {
            self.text_instances.write(&self.device, &self.queue, bytemuck::cast_slice(&gpu_glyphs));
        }

        let mut draw_calls = 0;

        // 0. Particles: birth and integration (compute).
        self.particles.prepare(encoder, &self.queue, &rcam, scene.time, &scene.particles);

        // 1. Shadow passes: the sun, then six faces per shadow-casting point light.
        let mut shadow_targets: Vec<(&wgpu::TextureView, &wgpu::BindGroup)> = Vec::new();
        if scene.sun.shadows {
            shadow_targets.push((&self.shadow_view, &self.shadow_bg));
        }
        for slot in 0..shadow_slots {
            for f in 0..6 {
                let i = slot * 6 + f;
                shadow_targets.push((&self.point_shadows.layers[i], &self.point_shadows.faces[i].1));
            }
        }
        if !scene.sun.shadows {
            // Keep the sun map cleared so nothing stale shadows the scene.
            encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("shadow clear"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.shadow_view,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
        }
        for (view, bg) in shadow_targets {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("shadow"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_bind_group(0, bg, &[]);
            pass.set_pipeline(&self.mesh_shadow_pipeline);
            pass.set_vertex_buffer(1, self.mesh_instances.buf.slice(..));
            for b in &shadow_batches {
                let mesh = &self.meshes[&b.0];
                if mesh.index_count > 0 {
                    pass.set_vertex_buffer(0, mesh.vbuf.slice(..));
                    pass.set_index_buffer(mesh.ibuf.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..mesh.index_count, 0, b.1..b.2);
                    draw_calls += 1;
                }
            }
            if !dyn_draws.is_empty() {
                pass.set_vertex_buffer(0, self.dyn_vertices.buf.slice(..));
                pass.set_index_buffer(self.dyn_indices.buf.slice(..), wgpu::IndexFormat::Uint32);
                for &(first, count, base, inst, shadow) in &dyn_draws {
                    if shadow {
                        pass.draw_indexed(first..first + count, base, inst..inst + 1);
                        draw_calls += 1;
                    }
                }
            }
            if sdf_shadow_end > sdf_count {
                pass.set_pipeline(&self.sdf_shadow_pipeline);
                pass.set_vertex_buffer(0, self.sdf_instances.buf.slice(..));
                pass.draw(0..36, sdf_count..sdf_shadow_end);
                draw_calls += 1;
            }
        }

        // 2. Scene pass: opaque geometry and the ocean, text, the sky on what is left, Gull
        // Sight silhouettes, then rain and particles over it all.
        let t = self.targets.as_ref().expect("targets");
        let pipes = if reversed { self.pipes_rev.as_ref().expect("reversed pipelines") } else { &self.pipes };
        {
            let cc = scene.clear_color;
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("scene"),
                color_attachments: &[
                    Some(wgpu::RenderPassColorAttachment {
                        view: &t.hdr_ms,
                        depth_slice: None,
                        resolve_target: Some(&t.hdr),
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color { r: cc.x as f64, g: cc.y as f64, b: cc.z as f64, a: 1.0 }),
                            store: wgpu::StoreOp::Discard,
                        },
                    }),
                    Some(wgpu::RenderPassColorAttachment {
                        view: &t.normal_ms,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                            store: wgpu::StoreOp::Store,
                        },
                    }),
                ],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &t.depth_ms,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(if reversed { 0.0 } else { 1.0 }),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_bind_group(0, &self.globals_bg, &[]);
            pass.set_bind_group(1, &self.materials.bg, &[]);
            pass.set_pipeline(&pipes.mesh);
            pass.set_vertex_buffer(1, self.mesh_instances.buf.slice(..));
            for b in &batches {
                let mesh = &self.meshes[&b.0];
                if mesh.index_count == 0 {
                    continue;
                }
                pass.set_vertex_buffer(0, mesh.vbuf.slice(..));
                pass.set_index_buffer(mesh.ibuf.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..mesh.index_count, 0, b.1..b.2);
                draw_calls += 1;
            }
            if !dyn_draws.is_empty() {
                pass.set_vertex_buffer(0, self.dyn_vertices.buf.slice(..));
                pass.set_index_buffer(self.dyn_indices.buf.slice(..), wgpu::IndexFormat::Uint32);
                for &(first, count, base, inst, _) in &dyn_draws {
                    pass.draw_indexed(first..first + count, base, inst..inst + 1);
                    draw_calls += 1;
                }
            }
            if sdf_count > 0 {
                pass.set_pipeline(&pipes.sdf);
                pass.set_vertex_buffer(0, self.sdf_instances.buf.slice(..));
                pass.draw(0..36, 0..sdf_count);
                draw_calls += 1;
            }
            if self.sea.draw_ocean(&mut pass, &self.globals_bg) {
                draw_calls += 1;
            }
            if glyph_count > 0 {
                pass.set_pipeline(&pipes.text);
                pass.set_bind_group(1, &self.text_bg, &[]);
                pass.set_vertex_buffer(0, self.text_instances.buf.slice(..));
                pass.draw(0..6, 0..glyph_count);
                draw_calls += 1;
            }
            if self.sea.draw_sky(&mut pass, &self.globals_bg) {
                draw_calls += 1;
            }
            if xray_meshes > 0 || sdf_xray_end > sdf_shadow_end {
                pass.set_bind_group(0, &self.globals_bg, &[]);
                pass.set_bind_group(1, &self.materials.bg, &[]);
                pass.set_blend_constant(wgpu::Color { r: 0.45, g: 0.45, b: 0.45, a: 0.45 });
                pass.set_pipeline(&pipes.mesh_xray);
                pass.set_vertex_buffer(1, self.mesh_instances.buf.slice(..));
                for b in &xray_batches {
                    let mesh = &self.meshes[&b.0];
                    if mesh.index_count > 0 {
                        pass.set_vertex_buffer(0, mesh.vbuf.slice(..));
                        pass.set_index_buffer(mesh.ibuf.slice(..), wgpu::IndexFormat::Uint32);
                        pass.draw_indexed(0..mesh.index_count, 0, b.1..b.2);
                        draw_calls += 1;
                    }
                }
                if sdf_xray_end > sdf_shadow_end {
                    pass.set_pipeline(&pipes.sdf_xray);
                    pass.set_vertex_buffer(0, self.sdf_instances.buf.slice(..));
                    pass.draw(0..36, sdf_shadow_end..sdf_xray_end);
                    draw_calls += 1;
                }
            }
            if self.sea.draw_rain(&mut pass, &self.globals_bg) {
                draw_calls += 1;
            }
            if self.particles.draw(&mut pass, reversed) {
                draw_calls += 1;
            }
        }

        // 2b. Bloom and distortion.
        let bloom = if scene.post.bloom > 0.0 {
            self.bloom.record(encoder, &self.queue, scene.post.bloom_threshold);
            scene.post.bloom
        } else {
            0.0
        };
        let distorted = scene.post.distortion
            && self.distort.record(encoder, &self.device, &self.queue, &rcam, scene.time, &scene.distortions);

        // 3. Composite.
        let p = &scene.post;
        let post = PostUniform {
            inv_proj: mat(rcam.proj.inverse()),
            view: mat(cam.view),
            // Screen-space bounce light: strength, world radius, pixels per metre at 1 m.
            gi: [p.gi, 1.6, size.1 as f32 * cam.proj.y_axis.y * 0.5, self.frame as f32],
            outline_color: match p.outline_color {
                Some(c) => c.extend(1.0).to_array(),
                None => [0.0; 4],
            },
            outline: [p.outline_px, p.outline_depth, p.outline_normal, if p.outlines { 1.0 } else { 0.0 }],
            tone: [
                p.exposure,
                match p.tonemap {
                    Tonemap::Clamp => 0.0,
                    Tonemap::SoftKnee => 1.0,
                    Tonemap::Aces => 2.0,
                },
                if target_format.is_srgb() { 0.0 } else { 1.0 },
                p.saturation,
            ],
            misc: [p.outline_darken, bloom, if distorted { 1.0 } else { 0.0 }, if reversed { 1.0 } else { 0.0 }],
            fwd: cam.forward.normalize_or(Vec3::NEG_Z).extend(0.0).to_array(),
            filt: {
                let f = &scene.filter;
                [
                    [f.pixelate, f.curvature, f.scanlines, f.scanline_px],
                    [f.dither, f.levels, f.palette as f32, f.split],
                    [f.temperature, f.tint, f.contrast, f.brightness],
                    [f.vignette, f.grain, f.chroma, scene.time],
                    [f.saturation, f.pixel_art, f.pixel_levels, if f.pixel_outline { 1.0 } else { 0.0 }],
                    [
                        p.outline_part as u32 as f32,
                        f.color_part as u32 as f32,
                        f.grade_part as u32 as f32,
                        f.scanline_part as u32 as f32,
                    ],
                    [f.grain_part as u32 as f32, f.chroma_part as u32 as f32, f.stylize_part as u32 as f32, 0.0],
                    [f.stylize as u32 as f32, f.stylize_size, f.stylize_mix, f.stylize_color],
                    [f.transition, f.transition_kind as u32 as f32, f.transition_center.x, f.transition_center.y],
                ]
            },
            inv_view: mat(cam.view.inverse()),
            light_vp: mat(light_vp),
            sun_dir: globals_sun_dir,
            sun_color: scene.sun.color.extend(n_lights as f32).to_array(),
            sky: scene.ambient.sky.extend(0.0).to_array(),
            air: [p.haze, p.haze_height.max(0.1), p.haze_base, p.shafts],
            air2: [p.shafts_forward.clamp(0.0, 0.95), p.halos, scene.gull.clamp(0.0, 1.0), 0.0],
        };
        self.queue.write_buffer(&self.post_buf, 0, bytemuck::bytes_of(&post));
        self.post_pipeline(target_format);
        // Below full resolution the composite writes a smaller image, stretched over the target.
        let small = (size != full).then(|| self.upscale.target(&self.device, target_format, size));
        let t = self.targets.as_ref().expect("targets");
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("post"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: small.as_ref().unwrap_or(target),
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.post_pipelines[&target_format]);
            pass.set_bind_group(0, &t.post_bg, &[]);
            pass.draw(0..3, 0..1);
            draw_calls += 1;
        }
        if small.is_some() {
            self.upscale.draw(&self.device, encoder, target);
            draw_calls += 1;
        }

        self.stats = RenderStats {
            mesh_instances: drawn_meshes,
            sdf_instances: sdf_count as usize,
            glyphs: glyph_count as usize,
            draw_calls,
            meshes_loaded: self.meshes.len(),
            particles: self.particles.slots() as usize,
            batches: main_batches,
            triangles: main_triangles,
            mesh_bytes: self.meshes.values().map(|m| (m.vbuf.size() + m.ibuf.size()) as usize).sum(),
            custom_meshes: self.meshes.keys().filter(|k| matches!(k, MeshKey::Custom(_))).count(),
            lights: n_lights,
            materials: self.materials.len.saturating_sub(1),
            culled_frustum,
            culled_distance,
            shadow_instances: shadow_meshes + (sdf_shadow_end - sdf_count) as usize,
            xray_instances: xray_meshes + (sdf_xray_end - sdf_shadow_end) as usize,
            ocean_vertices: self.sea.ocean_vertices(),
        };
    }
}
