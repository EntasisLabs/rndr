//! Display-list renderer.
//!
//! UI quads stay on the signed-distance pipeline at one sample. Paths and
//! layer groups are drawn into an offscreen target. That target is
//! multisampled at 4× when the format can resolve it, then resolved before a
//! blur, an opacity composite, or the quad pass. [`rndr_paint`] documents why
//! the edges are multisampled instead of drawn as a coverage fringe: one
//! tessellation, no halos on joins, and the swapchain stays single-sample.
//!
//! A drop shadow is drawn into the shape's own pixel grid, expanded by the
//! blur kernel, then a separable Gaussian runs on that grid and the result is
//! sampled with a linear filter. The grid stays fine enough that a zoomed
//! view does not stretch the falloff into flat blocks. The texture moves with
//! the shape, so a pan does not rebuild the blur, and the viewport clips the
//! shadow only when the quad is composited.

use std::collections::HashMap;

use rndr_core::World;
use rndr_paint::{
    Affine, ClipShape, Effect, FillRule, FillStyle, Frame, ImageOp, Mesh, MeshCache, Op, Path,
    PathOp, SceneLayer, TextSource, OUTLINE_PX,
};

use crate::color::color_for_surface;
use crate::decode::{downsample, mip_chain};
use crate::gpu::{self, INSTANCE_STRIDE};
use crate::images::CpuImage;

const PATH_STRIDE: usize = 24;
const SPRITE_STRIDE: usize = 72;

/// How many sigmas of the Gaussian are kept. Past this the weight is noise,
/// and the blur target is padded by the same distance so the tail is not clamped.
const BLUR_SPREAD: f32 = 3.5;
/// Largest `i` in the blur loop. `blur_factor` keeps the radius inside this.
const BLUR_MAX_RADIUS: i32 = 80;
/// Device pixels one blur texel may cover before the linear upsample looks stepped.
const BLUR_MAX_SPACING: f32 = 4.0;

const PATH_SHADER: &str = r#"
struct Viewport { size: vec2<f32>, origin: vec2<f32> }
@group(0) @binding(0) var<uniform> viewport: Viewport;
struct Vin {
    @location(0) pos: vec2<f32>,
    @location(1) color: vec4<f32>,
}
struct Vout {
    @builtin(position) clip: vec4<f32>,
    @location(0) color: vec4<f32>,
}
@vertex
fn vs(in: Vin) -> Vout {
    var out: Vout;
    let local = in.pos - viewport.origin;
    let ndc = vec2<f32>(local.x / viewport.size.x * 2.0 - 1.0, 1.0 - local.y / viewport.size.y * 2.0);
    out.clip = vec4<f32>(ndc, 0.0, 1.0);
    out.color = in.color;
    return out;
}
@fragment
fn fs(in: Vout) -> @location(0) vec4<f32> { return in.color; }
"#;

const SPRITE_SHADER: &str = r#"
struct Viewport { size: vec2<f32>, origin: vec2<f32> }
@group(0) @binding(0) var<uniform> viewport: Viewport;
@group(0) @binding(1) var tex: texture_2d<f32>;
@group(0) @binding(2) var tex_samp: sampler;
struct Vin {
    @location(0) pos: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) local: vec2<f32>,
    @location(4) extra: vec4<f32>,
    @location(5) scissor: vec4<f32>,
}
struct Vout {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) local: vec2<f32>,
    @location(3) extra: vec4<f32>,
    @location(4) scissor: vec4<f32>,
}
@vertex
fn vs(in: Vin) -> Vout {
    var out: Vout;
    let local = in.pos - viewport.origin;
    let ndc = vec2<f32>(local.x / viewport.size.x * 2.0 - 1.0, 1.0 - local.y / viewport.size.y * 2.0);
    out.clip = vec4<f32>(ndc, 0.0, 1.0);
    out.uv = in.uv;
    out.color = in.color;
    out.local = in.local;
    out.extra = in.extra;
    out.scissor = in.scissor;
    return out;
}
// `local` and `extra.xyz` are device pixels: position in the quad, radius,
// width, height. A negative scissor width means the sprite is not masked.
// The radius is clamped to the box so a large value stays a round rect.
// `fwidth` is at least one device pixel, so the edge stays anti-aliased
// when the quad is rotated and a small icon does not lose its corner.
fn cover(local: vec2<f32>, extra: vec4<f32>) -> f32 {
    let radius = extra.x;
    let size = vec2<f32>(extra.y, extra.z);
    if (radius <= 0.0 || size.x <= 0.0 || size.y <= 0.0) { return 1.0; }
    let half = size * 0.5;
    let r = min(radius, min(half.x, half.y));
    let q = abs(local - half) - half + vec2<f32>(r);
    let dist = length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - r;
    let fw = max(fwidth(dist), 1.0);
    return clamp(0.5 - dist / fw, 0.0, 1.0);
}
@fragment
fn fs(in: Vout) -> @location(0) vec4<f32> {
    // `clip` is the target's framebuffer position. Scissors are device pixels,
    // so a shadow target whose origin is not the screen origin adds it back.
    let px = in.clip.xy + viewport.origin;
    let box = in.scissor;
    if (box.z >= 0.0 && (px.x < box.x || px.y < box.y || px.x >= box.x + box.z || px.y >= box.y + box.w)) {
        discard;
    }
    let mode = in.extra.w;
    var uv = in.uv;
    // Image texels are straight alpha. A sample on the texture boundary
    // mixes the edge texel with the clamp color, which is transparent black,
    // and that mix darkens the fringe. Stay on the outer texel center.
    // The gradient is taken from the unclamped uv so a minified image still
    // selects a mip from the size of the quad.
    if (mode < 0.5) {
        let dims = max(vec2<f32>(textureDimensions(tex)), vec2<f32>(1.0));
        let half_texel = 0.5 / dims;
        uv = clamp(in.uv, half_texel, vec2<f32>(1.0) - half_texel);
    }
    let sample = textureSampleGrad(tex, tex_samp, uv, dpdx(in.uv), dpdy(in.uv));
    var rgb = sample.rgb;
    var a = sample.a * in.color.a;
    if (mode > 0.5 && mode < 1.5) {
        rgb = in.color.rgb;
        a = sample.r * in.color.a;
    } else if (mode > 3.5) {
        rgb = in.color.rgb;
        // A 1:1 shadow quad is one texel per device pixel. Load that texel.
        // Interpolated UVs shift by a fraction when the rasterizer clips the
        // quad to the viewport, and that shift rounds a few texels off.
        // `scissor.xy` is the quad origin in device pixels; a negative width
        // still means "no mask".
        let dims_i = textureDimensions(tex);
        let dims = vec2<f32>(dims_i);
        if (abs(in.extra.y - dims.x) < 0.5 && abs(in.extra.z - dims.y) < 0.5) {
            let texel = vec2<i32>(floor(px - in.scissor.xy));
            let clamped = clamp(texel, vec2<i32>(0), vec2<i32>(dims_i) - vec2<i32>(1));
            a = textureLoad(tex, clamped, 0).a * in.color.a;
        } else {
            a = sample.a * in.color.a;
        }
    }
    a = a * cover(in.local, in.extra);
    return vec4<f32>(rgb, a);
}
"#;

const BLUR_SHADER: &str = r#"
struct Blur {
    dir: vec2<f32>,
    sigma: f32,
    mode: f32,
    map: vec4<f32>,
}
@group(0) @binding(0) var<uniform> blur: Blur;
@group(0) @binding(1) var tex: texture_2d<f32>;
@group(0) @binding(2) var tex_samp: sampler;
struct Vin { @location(0) pos: vec2<f32>, @location(1) uv: vec2<f32> }
struct Vout { @builtin(position) clip: vec4<f32>, @location(0) uv: vec2<f32> }
@vertex
fn vs(in: Vin) -> Vout {
    var out: Vout;
    out.clip = vec4<f32>(in.pos, 0.0, 1.0);
    out.uv = in.uv;
    return out;
}
@fragment
fn fs(in: Vout) -> @location(0) vec4<f32> {
    // mode > 0 copies the layer into a padded, possibly coarser target.
    // `map` sends dest uv to source uv. Outside the layer is transparent,
    // which is the padding the Gaussian needs so the tail is not clamped.
    if (blur.mode > 0.5) {
        let uv = blur.map.xy + in.uv * blur.map.zw;
        if (uv.x < -0.001 || uv.y < -0.001 || uv.x > 1.001 || uv.y > 1.001) {
            return vec4<f32>(0.0);
        }
        return textureSampleLevel(tex, tex_samp, clamp(uv, vec2<f32>(0.0), vec2<f32>(1.0)), 0.0);
    }
    // One texel per tap. `sigma` is in texels of this target. A wide blur
    // is a coarser target, not a tap that skips texels: skipping leaves a
    // flat step between taps when the source edge is hard. The loop bound
    // matches `BLUR_MAX_RADIUS`.
    let sigma = max(blur.sigma, 0.0001);
    let radius = min(i32(ceil(sigma * 3.5)), 80);
    var sum = vec4<f32>(0.0);
    var norm = 0.0;
    for (var i: i32 = -80; i <= 80; i = i + 1) {
        if (i < -radius || i > radius) {
            continue;
        }
        let dist = f32(i);
        let x = dist / sigma;
        let w = exp(-0.5 * x * x);
        sum = sum + textureSampleLevel(tex, tex_samp, in.uv + blur.dir * dist, 0.0) * w;
        norm = norm + w;
    }
    return sum / max(norm, 0.0001);
}
"#;

/// 4× when `format` can store and resolve a multisampled texture, otherwise 1.
pub(crate) fn choose_samples(adapter: &wgpu::Adapter, format: wgpu::TextureFormat) -> u32 {
    let flags = adapter.get_texture_format_features(format).flags;
    if flags.sample_count_supported(4)
        && flags.contains(wgpu::TextureFormatFeatureFlags::MULTISAMPLE_RESOLVE)
    {
        4
    } else {
        1
    }
}

struct PipeSet {
    samples: u32,
    path: wgpu::RenderPipeline,
    path_clip: wgpu::RenderPipeline,
    path_in: wgpu::RenderPipeline,
    sprite: wgpu::RenderPipeline,
    sprite_in: wgpu::RenderPipeline,
}

#[derive(Clone)]
struct TargetUse {
    samples: u32,
    color: wgpu::TextureView,
    msaa: Option<wgpu::TextureView>,
    stencil: wgpu::TextureView,
    bind: wgpu::BindGroup,
}

struct PoolSlot {
    width: u32,
    height: u32,
    samples: u32,
    color_view: wgpu::TextureView,
    msaa_view: Option<wgpu::TextureView>,
    stencil_view: wgpu::TextureView,
    bind: wgpu::BindGroup,
    stamp: u64,
    _keep: Vec<wgpu::Texture>,
}

#[derive(Clone)]
struct GpuTile {
    u0: f32,
    v0: f32,
    u1: f32,
    v1: f32,
    bind: wgpu::BindGroup,
}

struct GpuImage {
    tiles: Vec<GpuTile>,
    bytes: u64,
    stamp: u64,
}

struct SpriteDraw {
    bytes: Vec<u8>,
    bind: wgpu::BindGroup,
}

struct BlitDraw {
    bind: wgpu::BindGroup,
    shadow: Option<wgpu::BindGroup>,
    opacity: f32,
    shadow_color: [f32; 4],
    offset: [f32; 2],
    /// Device pixels of transparent padding around a viewport-sized shadow.
    /// The shadow quad grows by this on every side. Zero draws the layer rect.
    /// Ignored when [`Self::shadow_place`] is set.
    shadow_pad: f32,
    /// Device-pixel rect of a bounds-local shadow texture: x, y, width, height.
    /// The quad is this rect plus `offset`. `None` uses the viewport rect.
    shadow_place: Option<[f32; 4]>,
    /// Device-pixel bounds of the composited layer, min x/y then max x/y.
    /// A parent shadow uses this instead of the viewport-sized blit quad.
    extent: Option<[f32; 4]>,
}

enum Drawn {
    Paths(Vec<u8>),
    Sprite(SpriteDraw),
    Blit(BlitDraw),
}

#[derive(Clone)]
enum Cmd {
    Paths {
        offset: u64,
        verts: u32,
    },
    Sprite {
        offset: u64,
        verts: u32,
        bind: wgpu::BindGroup,
    },
}

struct GeomPass {
    target: TargetUse,
    scissor: Option<(u32, u32, u32, u32)>,
    clip_offset: Option<u64>,
    clip_verts: u32,
    cmds: Vec<Cmd>,
    /// Replaces the frame viewport for this pass, then the frame viewport is
    /// copied back. `None` draws in the screen viewport (origin zero).
    viewport: Option<wgpu::Buffer>,
}

struct BlurJob {
    view: wgpu::TextureView,
    bind: wgpu::BindGroup,
}

#[allow(clippy::large_enum_variant)]
enum Pass {
    Geom(GeomPass),
    Blur(BlurJob),
}

enum RootCmd {
    Quads(u32, u32),
    Sprite {
        offset: u64,
        verts: u32,
        bind: wgpu::BindGroup,
    },
}

struct Plan {
    path_bytes: Vec<u8>,
    sprite_bytes: Vec<u8>,
    passes: Vec<Pass>,
    root: Vec<RootCmd>,
}

/// Path meshes, image textures, and offscreen layers for one device.
///
/// Resources are declared before `queue` and `device`. Drop runs in field
/// order, and the queue waits for the device to go idle, so textures and
/// buffers have to be gone before that wait.
pub(crate) struct SceneGpu {
    format: wgpu::TextureFormat,
    max_dim: u32,
    uniform: wgpu::Buffer,
    /// Screen viewport (origin zero). Copied back over `uniform` after a
    /// shadow pass that borrowed it.
    viewport_frame: wgpu::Buffer,
    path_layout: wgpu::BindGroupLayout,
    sprite_layout: wgpu::BindGroupLayout,
    blur_layout: wgpu::BindGroupLayout,
    quad_layout: wgpu::BindGroupLayout,
    blur_pipeline: wgpu::RenderPipeline,
    blur_quad: wgpu::Buffer,
    image_sampler: wgpu::Sampler,
    glyph_sampler: wgpu::Sampler,
    glyph_sampler_linear: wgpu::Sampler,
    plain: PipeSet,
    msaa: Option<PipeSet>,
    sprite_swap: wgpu::RenderPipeline,
    quad_pipeline: wgpu::RenderPipeline,
    quad_geo: wgpu::Buffer,
    quad_buffer: wgpu::Buffer,
    path_buffer: wgpu::Buffer,
    sprite_buffer: wgpu::Buffer,
    path_cap: u64,
    sprite_cap: u64,
    quad_cap: u64,
    meshes: MeshCache,
    images: HashMap<u64, GpuImage>,
    image_bytes: u64,
    pool: Vec<PoolSlot>,
    frame: u64,
    uploaded: Vec<u64>,
    kept: Vec<wgpu::Buffer>,
    /// After the resources above. Drop waits for submitted work.
    queue: wgpu::Queue,
    /// After `queue`, so the idle wait still has a device.
    device: wgpu::Device,
}

impl Drop for SceneGpu {
    fn drop(&mut self) {
        // The window path does not take the headless lock. Drain submitted
        // work while the device is still alive; field drop then releases
        // textures before the queue's idle wait.
        let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
    }
}

impl SceneGpu {
    pub(crate) fn new(
        device: wgpu::Device,
        queue: wgpu::Queue,
        format: wgpu::TextureFormat,
        samples: u32,
    ) -> Self {
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("scene-viewport"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let viewport_frame = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("scene-viewport-frame"),
            size: 16,
            usage: wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let path_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("path-bg"),
            entries: &[buffer_entry(0, wgpu::ShaderStages::VERTEX)],
        });
        let sprite_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("sprite-bg"),
            entries: &[
                buffer_entry(0, wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT),
                texture_entry(1, wgpu::ShaderStages::FRAGMENT),
                sampler_entry(2, wgpu::ShaderStages::FRAGMENT),
            ],
        });
        let blur_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("blur-bg"),
            entries: &[
                buffer_entry(0, wgpu::ShaderStages::FRAGMENT),
                texture_entry(1, wgpu::ShaderStages::FRAGMENT),
                sampler_entry(2, wgpu::ShaderStages::FRAGMENT),
            ],
        });
        let path_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("path"),
            source: wgpu::ShaderSource::Wgsl(PATH_SHADER.into()),
        });
        let sprite_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("sprite"),
            source: wgpu::ShaderSource::Wgsl(SPRITE_SHADER.into()),
        });
        let blur_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("blur"),
            source: wgpu::ShaderSource::Wgsl(BLUR_SHADER.into()),
        });
        let image_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("image"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });
        let glyph_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("glyph"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let glyph_sampler_linear = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("glyph-linear"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });
        let plain = pipe_set(
            &device,
            &path_shader,
            &sprite_shader,
            &path_layout,
            &sprite_layout,
            format,
            1,
        );
        let msaa = if samples > 1 {
            Some(pipe_set(
                &device,
                &path_shader,
                &sprite_shader,
                &path_layout,
                &sprite_layout,
                format,
                samples,
            ))
        } else {
            None
        };
        let sprite_swap = sprite_pipeline(&device, &sprite_shader, &sprite_layout, format, 1, None);
        const BLUR_ATTRS: [wgpu::VertexAttribute; 2] =
            wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2];
        let blur_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("blur"),
            layout: Some(
                &device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("blur-pl"),
                    bind_group_layouts: &[Some(&blur_layout)],
                    immediate_size: 0,
                }),
            ),
            vertex: wgpu::VertexState {
                module: &blur_shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: 16,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &BLUR_ATTRS,
                })],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &blur_shader,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(color_target(format, wgpu::ColorWrites::ALL))],
            }),
            multiview_mask: None,
            cache: None,
        });
        let blur_quad = init_buffer(
            &device,
            &queue,
            "blur-quad",
            &blur_quad_bytes(),
            wgpu::BufferUsages::VERTEX,
        );
        let (quad_layout, quad_pipeline) = quad_pipeline(&device, format);
        let quad_geo = init_buffer(
            &device,
            &queue,
            "quad-geo",
            &unit_quad_bytes(),
            wgpu::BufferUsages::VERTEX,
        );
        let max_dim = device.limits().max_texture_dimension_2d.max(1);
        let quad_buffer = empty_vertex(&device, "quads", 256);
        let path_buffer = empty_vertex(&device, "paths", 256);
        let sprite_buffer = empty_vertex(&device, "sprites", 256);
        Self {
            format,
            max_dim,
            uniform,
            viewport_frame,
            path_layout,
            sprite_layout,
            blur_layout,
            quad_layout,
            blur_pipeline,
            blur_quad,
            image_sampler,
            glyph_sampler,
            glyph_sampler_linear,
            plain,
            msaa,
            sprite_swap,
            quad_pipeline,
            quad_geo,
            quad_buffer,
            path_buffer,
            sprite_buffer,
            path_cap: 256,
            sprite_cap: 256,
            quad_cap: 256,
            meshes: MeshCache::new(16_384),
            images: HashMap::new(),
            image_bytes: 0,
            pool: Vec::new(),
            frame: 0,
            uploaded: Vec::new(),
            kept: Vec::new(),
            queue,
            device,
        }
    }

    /// Draw `frame` into `target`. `target` is single-sample. Returns image ids
    /// whose pixels were uploaded, so the caller can drop the CPU copy.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn render(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        frame: &Frame,
        images: &[(u64, CpuImage)],
        budget: u64,
        coverage: &wgpu::TextureView,
        color_atlas: &wgpu::TextureView,
        width: u32,
        height: u32,
        scale: f32,
        clear: wgpu::Color,
        world: Option<&World>,
    ) -> Vec<u64> {
        self.frame = self.frame.wrapping_add(1);
        self.uploaded.clear();
        self.kept.clear();
        for (id, image) in images {
            self.ensure_image(*id, image, budget);
        }
        let viewport = viewport_bytes([width.max(1) as f32, height.max(1) as f32], [0.0, 0.0]);
        self.queue.write_buffer(&self.uniform, 0, &viewport);
        self.queue.write_buffer(&self.viewport_frame, 0, &viewport);
        let glyph = self.glyph_sampler.clone();
        let glyph_linear = self.glyph_sampler_linear.clone();
        let coverage_bind = self.sprite_bind(coverage, &glyph);
        let color_bind = self.sprite_bind(color_atlas, &glyph);
        let coverage_linear = self.sprite_bind(coverage, &glyph_linear);
        let color_linear = self.sprite_bind(color_atlas, &glyph_linear);
        let quad_bind = self.quad_bind(coverage, color_atlas);
        let mut plan = Plan {
            path_bytes: Vec::new(),
            sprite_bytes: Vec::new(),
            passes: Vec::new(),
            root: Vec::new(),
        };
        self.build_root(
            &mut plan,
            frame,
            width.max(1),
            height.max(1),
            scale,
            &coverage_bind,
            &color_bind,
            &coverage_linear,
            &color_linear,
            world,
        );
        self.path_buffer = grow(
            &self.device,
            &self.queue,
            &self.path_buffer,
            &mut self.path_cap,
            &plan.path_bytes,
            "paths",
        );
        self.sprite_buffer = grow(
            &self.device,
            &self.queue,
            &self.sprite_buffer,
            &mut self.sprite_cap,
            &plan.sprite_bytes,
            "sprites",
        );
        let mut quad_bytes = Vec::new();
        gpu::encode_instances(&frame.quads, scale, self.format, &mut quad_bytes);
        self.quad_buffer = grow(
            &self.device,
            &self.queue,
            &self.quad_buffer,
            &mut self.quad_cap,
            &quad_bytes,
            "quads",
        );
        self.execute(encoder, &plan, target, &quad_bind, clear);
        self.uploaded.clone()
    }

    #[allow(clippy::too_many_arguments)]
    fn build_root(
        &mut self,
        plan: &mut Plan,
        frame: &Frame,
        width: u32,
        height: u32,
        scale: f32,
        coverage: &wgpu::BindGroup,
        color_atlas: &wgpu::BindGroup,
        coverage_linear: &wgpu::BindGroup,
        color_linear: &wgpu::BindGroup,
        world: Option<&World>,
    ) {
        let mut quad_at = 0u32;
        let mut op_at = 0usize;
        while quad_at < frame.quads.len() as u32 || op_at < frame.draws.len() {
            let next_z = frame
                .draws
                .get(op_at)
                .map(|placed| placed.z)
                .unwrap_or(u32::MAX);
            if quad_at < next_z && (quad_at as usize) < frame.quads.len() {
                let end = next_z.min(frame.quads.len() as u32);
                if end > quad_at {
                    plan.root.push(RootCmd::Quads(quad_at, end));
                }
                quad_at = end;
                continue;
            }
            if op_at >= frame.draws.len() {
                break;
            }
            let z = frame.draws[op_at].z;
            let mut group = Vec::new();
            while op_at < frame.draws.len() && frame.draws[op_at].z == z {
                group.push(frame.draws[op_at].op.clone());
                op_at += 1;
            }
            let drawn = self.flatten(
                plan,
                &group,
                Affine::IDENTITY,
                scale,
                width,
                height,
                coverage,
                color_atlas,
                coverage_linear,
                color_linear,
                world,
            );
            self.emit_root(plan, drawn, width, height, scale);
        }
    }

    fn emit_root(
        &mut self,
        plan: &mut Plan,
        drawn: Vec<Drawn>,
        width: u32,
        height: u32,
        scale: f32,
    ) {
        let mut batch = Vec::new();
        for item in drawn {
            match item {
                Drawn::Paths(bytes) => batch.extend(bytes),
                Drawn::Sprite(sprite) => {
                    self.flush_paths(plan, &mut batch, width, height, scale);
                    push_root_sprite(plan, sprite);
                }
                Drawn::Blit(blit) => {
                    self.flush_paths(plan, &mut batch, width, height, scale);
                    for sprite in blit_sprites(blit, width, height) {
                        push_root_sprite(plan, sprite);
                    }
                }
            }
        }
        self.flush_paths(plan, &mut batch, width, height, scale);
    }

    fn flush_paths(
        &mut self,
        plan: &mut Plan,
        batch: &mut Vec<u8>,
        width: u32,
        height: u32,
        scale: f32,
    ) {
        if batch.is_empty() {
            return;
        }
        let verts = std::mem::take(batch);
        let baked = self.bake(
            plan,
            vec![Drawn::Paths(verts)],
            &SceneLayer::default(),
            Affine::IDENTITY,
            scale,
            width,
            height,
        );
        for sprite in blit_sprites(baked, width, height) {
            push_root_sprite(plan, sprite);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn flatten(
        &mut self,
        plan: &mut Plan,
        ops: &[Op],
        base: Affine,
        scale: f32,
        width: u32,
        height: u32,
        coverage: &wgpu::BindGroup,
        color_atlas: &wgpu::BindGroup,
        coverage_linear: &wgpu::BindGroup,
        color_linear: &wgpu::BindGroup,
        world: Option<&World>,
    ) -> Vec<Drawn> {
        let mut stack = vec![base];
        let mut out = Vec::new();
        let mut index = 0;
        while index < ops.len() {
            let current = *stack.last().unwrap_or(&Affine::IDENTITY);
            match &ops[index] {
                Op::PushTransform(affine) => {
                    // The child maps local space. The stack already holds the
                    // parent, so the child runs first (`Affine::then` is
                    // "self first"). `current.then(child)` did the reverse
                    // and a rotated child drifted under a non-identity camera.
                    stack.push(pushed(current, *affine));
                    index += 1;
                }
                Op::PopTransform => {
                    if stack.len() > 1 {
                        stack.pop();
                    }
                    index += 1;
                }
                Op::PushLayer(layer) => {
                    let end = matching_pop(ops, index);
                    if layer.opacity > 0.0 || layer.effect.is_some() {
                        let inner = self.flatten(
                            plan,
                            &ops[index + 1..end],
                            current,
                            scale,
                            width,
                            height,
                            coverage,
                            color_atlas,
                            coverage_linear,
                            color_linear,
                            world,
                        );
                        let baked = self.bake(plan, inner, layer, current, scale, width, height);
                        out.push(Drawn::Blit(baked));
                    }
                    index = if end < ops.len() { end + 1 } else { end };
                }
                Op::PopLayer => index += 1,
                Op::Path(path) => {
                    let bytes = self.path_bytes(path, current, scale);
                    if !bytes.is_empty() {
                        out.push(Drawn::Paths(bytes));
                    }
                    index += 1;
                }
                Op::Image(image) => {
                    out.extend(
                        self.image_sprites(image, current, scale)
                            .into_iter()
                            .map(Drawn::Sprite),
                    );
                    index += 1;
                }
                Op::Glyphs(glyphs) => {
                    let device = glyphs.em * current.uniform_scale() * scale;
                    let choice = glyph_choice(device, current);
                    let mut outlined = false;
                    if choice.outlines {
                        if let Some(source) = glyphs.source.as_deref() {
                            outlined = self.push_outlines(&mut out, source, current, scale, world);
                        }
                    }
                    if !outlined {
                        let (cover, color) = if choice.linear_sprites {
                            (coverage_linear, color_linear)
                        } else {
                            (coverage, color_atlas)
                        };
                        out.extend(
                            self.glyph_sprites(glyphs, current, scale, cover, color)
                                .into_iter()
                                .map(Drawn::Sprite),
                        );
                    }
                    index += 1;
                }
            }
        }
        out
    }

    fn push_outlines(
        &mut self,
        out: &mut Vec<Drawn>,
        source: &TextSource,
        affine: Affine,
        scale: f32,
        world: Option<&World>,
    ) -> bool {
        let Some(world) = world else {
            return false;
        };
        let paths = crate::outline::outlines(world, source);
        if paths.is_empty() {
            return false;
        }
        for (path, color) in paths {
            let op = PathOp {
                path,
                fill: Some(FillStyle {
                    color,
                    rule: FillRule::NonZero,
                }),
                stroke: None,
            };
            let bytes = self.path_bytes(&op, affine, scale);
            if !bytes.is_empty() {
                out.push(Drawn::Paths(bytes));
            }
        }
        true
    }

    fn path_bytes(&mut self, op: &PathOp, affine: Affine, scale: f32) -> Vec<u8> {
        let mut bytes = Vec::new();
        let tess_scale = affine.uniform_scale() * scale;
        if let Some(fill) = &op.fill {
            let mesh = self.meshes.fill(&op.path, fill.rule, tess_scale);
            write_mesh(
                &mut bytes,
                &mesh,
                affine,
                scale,
                color_for_surface(fill.color, self.format),
            );
        }
        if let Some(stroke) = &op.stroke {
            let mesh = self.meshes.stroke(&op.path, &stroke.stroke, tess_scale);
            write_mesh(
                &mut bytes,
                &mesh,
                affine,
                scale,
                color_for_surface(stroke.color, self.format),
            );
        }
        bytes
    }

    fn image_sprites(&mut self, image: &ImageOp, affine: Affine, scale: f32) -> Vec<SpriteDraw> {
        let Some(gpu) = self.images.get_mut(&image.id) else {
            return Vec::new();
        };
        gpu.stamp = self.frame;
        let tiles = gpu.tiles.clone();
        let mut out = Vec::new();
        for tile in tiles {
            let Some((u0, u1)) = overlap(image.uv[0], image.uv[2], tile.u0, tile.u1) else {
                continue;
            };
            let Some((v0, v1)) = overlap(image.uv[1], image.uv[3], tile.v0, tile.v1) else {
                continue;
            };
            let span_u = (tile.u1 - tile.u0).abs().max(1.0e-6);
            let span_v = (tile.v1 - tile.v0).abs().max(1.0e-6);
            let tu0 = (u0 - tile.u0) / span_u;
            let tu1 = (u1 - tile.u0) / span_u;
            let tv0 = (v0 - tile.v0) / span_v;
            let tv1 = (v1 - tile.v0) / span_v;
            let du = (image.uv[2] - image.uv[0]).abs().max(1.0e-6);
            let dv = (image.uv[3] - image.uv[1]).abs().max(1.0e-6);
            let x0 = (u0 - image.uv[0]) / du * image.dest.w;
            let x1 = (u1 - image.uv[0]) / du * image.dest.w;
            let y0 = (v0 - image.uv[1]) / dv * image.dest.h;
            let y1 = (v1 - image.uv[1]) / dv * image.dest.h;
            let mut bytes = Vec::new();
            let local_px = affine.uniform_scale() * scale;
            let scissor = match image.clip {
                Some(rect) => [
                    rect.x * scale,
                    rect.y * scale,
                    rect.w * scale,
                    rect.h * scale,
                ],
                None => NO_SCISSOR,
            };
            push_quad(
                &mut bytes,
                affine,
                scale,
                local_px,
                image.dest.x,
                image.dest.y,
                x0,
                y0,
                x1,
                y1,
                [tu0, tv0, tu1, tv1],
                [1.0, 1.0, 1.0, image.opacity],
                image.radius,
                image.dest.w,
                image.dest.h,
                0.0,
                scissor,
            );
            out.push(SpriteDraw {
                bytes,
                bind: tile.bind,
            });
        }
        out
    }

    fn glyph_sprites(
        &self,
        glyphs: &rndr_paint::GlyphOp,
        affine: Affine,
        scale: f32,
        coverage: &wgpu::BindGroup,
        color_atlas: &wgpu::BindGroup,
    ) -> Vec<SpriteDraw> {
        let mut coverage_bytes = Vec::new();
        let mut color_bytes = Vec::new();
        for sprite in glyphs.sprites.iter() {
            let color = color_for_surface(sprite.color, self.format);
            let bucket = if sprite.atlas == 0 {
                &mut coverage_bytes
            } else {
                &mut color_bytes
            };
            let mode = if sprite.atlas == 0 { 1.0 } else { 2.0 };
            push_quad(
                bucket, affine, scale, 1.0, sprite.x, sprite.y, 0.0, 0.0, sprite.w, sprite.h,
                sprite.uv, color, 0.0, sprite.w, sprite.h, mode, NO_SCISSOR,
            );
        }
        let mut out = Vec::new();
        if !coverage_bytes.is_empty() {
            out.push(SpriteDraw {
                bytes: coverage_bytes,
                bind: coverage.clone(),
            });
        }
        if !color_bytes.is_empty() {
            out.push(SpriteDraw {
                bytes: color_bytes,
                bind: color_atlas.clone(),
            });
        }
        out
    }

    #[allow(clippy::too_many_arguments)]
    fn bake(
        &mut self,
        plan: &mut Plan,
        drawn: Vec<Drawn>,
        layer: &SceneLayer,
        base: Affine,
        scale: f32,
        width: u32,
        height: u32,
    ) -> BlitDraw {
        let samples = self.msaa.as_ref().map(|set| set.samples).unwrap_or(1);
        let target = self.alloc(width, height, samples);
        let scissor = match &layer.clip {
            Some(ClipShape::Scissor(rect)) => scissor_px(*rect, scale, width, height),
            _ => None,
        };
        let (clip_offset, clip_verts) = match &layer.clip {
            Some(ClipShape::RoundRect { rect, radius }) => {
                self.clip_bytes(plan, &Path::round_rect(*rect, *radius), base, scale)
            }
            Some(ClipShape::Path(path)) => self.clip_bytes(plan, path, base, scale),
            _ => (None, 0),
        };
        let extent = content_extent(&drawn);
        let cmds = pack_cmds(plan, drawn, width, height);
        let replay = layer.effect.is_some().then(|| cmds.clone());
        let bind = target.bind.clone();
        plan.passes.push(Pass::Geom(GeomPass {
            target,
            scissor,
            clip_offset,
            clip_verts,
            cmds,
            viewport: None,
        }));
        let shadow = self.shadow(
            plan,
            layer,
            base,
            scale,
            width,
            height,
            &bind,
            replay,
            clip_offset,
            clip_verts,
            extent,
        );
        let mut published = extent;
        if let Some(place) = shadow.place {
            let x = place[0] + shadow.offset[0];
            let y = place[1] + shadow.offset[1];
            union_box(&mut published, [x, y, x + place[2], y + place[3]]);
        } else if shadow.bind.is_some() {
            let pad = shadow.pad;
            let x = shadow.offset[0] - pad;
            let y = shadow.offset[1] - pad;
            union_box(
                &mut published,
                [
                    x,
                    y,
                    x + width as f32 + pad * 2.0,
                    y + height as f32 + pad * 2.0,
                ],
            );
        }
        BlitDraw {
            bind,
            shadow: shadow.bind,
            opacity: layer.opacity.clamp(0.0, 1.0),
            shadow_color: shadow.color,
            offset: shadow.offset,
            shadow_pad: shadow.pad,
            shadow_place: shadow.place,
            extent: published,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn shadow(
        &mut self,
        plan: &mut Plan,
        layer: &SceneLayer,
        base: Affine,
        scale: f32,
        width: u32,
        height: u32,
        color_bind: &wgpu::BindGroup,
        replay: Option<Vec<Cmd>>,
        clip_offset: Option<u64>,
        clip_verts: u32,
        extent: Option<[f32; 4]>,
    ) -> ShadowBlit {
        let Some(Effect::DropShadow {
            offset,
            blur,
            color,
        }) = layer.effect
        else {
            return ShadowBlit::none();
        };
        let px = base.uniform_scale() * scale;
        let offset_px = [offset[0] * px, offset[1] * px];
        let sigma = (blur * px).max(0.0);
        let tint = color_for_surface(color, self.format);
        if tint[3] <= 0.0 || width == 0 || height == 0 {
            return ShadowBlit::solid(None, tint, offset_px, 0.0);
        }
        let pad = if sigma < 0.5 { 0.0 } else { blur_pad(sigma) };
        if let (Some(replay), Some(extent)) = (replay, extent) {
            if let Some((x0, y0, tw, th)) =
                snapped_shadow_rect(extent[0], extent[1], extent[2], extent[3], pad)
            {
                if tw >= 1 && th >= 1 && tw <= self.max_dim && th <= self.max_dim {
                    return self.shadow_bounds(
                        plan,
                        replay,
                        clip_offset,
                        clip_verts,
                        sigma,
                        tint,
                        offset_px,
                        pad,
                        x0,
                        y0,
                        tw,
                        th,
                    );
                }
            }
        }
        self.shadow_viewport(plan, color_bind, width, height, sigma, tint, offset_px)
    }

    /// Rasterize `replay` into a texture that covers the shape plus the kernel,
    /// then blur there. `x0` and `y0` are the texture's device-pixel origin.
    #[allow(clippy::too_many_arguments)]
    fn shadow_bounds(
        &mut self,
        plan: &mut Plan,
        replay: Vec<Cmd>,
        clip_offset: Option<u64>,
        clip_verts: u32,
        sigma: f32,
        tint: [f32; 4],
        offset_px: [f32; 2],
        _pad: f32,
        x0: f32,
        y0: f32,
        tw: u32,
        th: u32,
    ) -> ShadowBlit {
        let samples = self.msaa.as_ref().map(|set| set.samples).unwrap_or(1);
        let target = self.alloc(tw, th, samples);
        let source = target.color.clone();
        let colored = target.bind.clone();
        // Device positions are `mesh + translation`. Adding a large translation
        // rounds the fraction, so `device - origin` is not the same bits after
        // an integer pan. Re-encode in the texture grid with the origin removed
        // first, and draw that texture with a zero origin.
        let replay = retarget_shadow_cmds(plan, &replay, [x0, y0]);
        let (clip_offset, clip_verts) =
            retarget_shadow_clip(plan, clip_offset, clip_verts, [x0, y0]);
        let viewport = self.viewport_buffer([tw as f32, th as f32], [0.0, 0.0]);
        // No screen scissor: the viewport clip belongs on the composite, not
        // on the blur source. A rounded-rect or path clip is part of the shape.
        plan.passes.push(Pass::Geom(GeomPass {
            target,
            scissor: None,
            clip_offset,
            clip_verts,
            cmds: replay,
            viewport: Some(viewport),
        }));
        let place = [x0, y0, tw as f32, th as f32];
        if sigma < 0.5 {
            return ShadowBlit {
                bind: Some(colored),
                color: tint,
                offset: offset_px,
                pad: 0.0,
                place: Some(place),
            };
        }
        let (bw, bh, sigma_texel) = blur_dims(sigma, tw, th, 0.0, self.max_dim);
        let mut source = source;
        let mut result = colored;
        // A 1:1 copy would quantize sRGB again. The texture is already padded.
        if bw != tw || bh != th {
            let resampled = self.alloc(bw, bh, 1);
            let resample = self.blur_bind(&source, false, 0.0, bw, bh, 1.0, [0.0, 0.0, 1.0, 1.0]);
            source = resampled.color.clone();
            plan.passes.push(Pass::Blur(BlurJob {
                view: source.clone(),
                bind: resample,
            }));
            result = resampled.bind.clone();
        }
        for horizontal in [true, false] {
            let blur_target = self.alloc(bw, bh, 1);
            let bind = self.blur_bind(&source, horizontal, sigma_texel, bw, bh, 0.0, [0.0; 4]);
            let view = blur_target.color.clone();
            plan.passes.push(Pass::Blur(BlurJob {
                view: view.clone(),
                bind,
            }));
            result = blur_target.bind;
            source = view;
        }
        ShadowBlit {
            bind: Some(result),
            color: tint,
            offset: offset_px,
            pad: 0.0,
            place: Some(place),
        }
    }

    /// Viewport-sized blur, used when the shape does not fit in one texture.
    #[allow(clippy::too_many_arguments)]
    fn shadow_viewport(
        &mut self,
        plan: &mut Plan,
        color_bind: &wgpu::BindGroup,
        width: u32,
        height: u32,
        sigma: f32,
        tint: [f32; 4],
        offset_px: [f32; 2],
    ) -> ShadowBlit {
        if sigma < 0.5 {
            return ShadowBlit::solid(Some(color_bind.clone()), tint, offset_px, 0.0);
        }
        let Some(source) = plan_color_view(plan) else {
            return ShadowBlit::solid(Some(color_bind.clone()), tint, offset_px, 0.0);
        };
        let pad = blur_pad(sigma);
        let (bw, bh, sigma_texel) = blur_dims(sigma, width, height, pad, self.max_dim);
        let expanded_w = width as f32 + pad * 2.0;
        let expanded_h = height as f32 + pad * 2.0;
        // Dest uv 0 is `pad` device pixels before the layer origin, so the
        // Gaussian sees transparent texels instead of a clamped edge.
        let map = [
            -pad / width as f32,
            -pad / height as f32,
            expanded_w / width as f32,
            expanded_h / height as f32,
        ];
        let resampled = self.alloc(bw, bh, 1);
        let resample = self.blur_bind(&source, false, 0.0, bw, bh, 1.0, map);
        let mut source = resampled.color.clone();
        plan.passes.push(Pass::Blur(BlurJob {
            view: source.clone(),
            bind: resample,
        }));
        let mut result = resampled.bind.clone();
        for horizontal in [true, false] {
            let target = self.alloc(bw, bh, 1);
            let bind = self.blur_bind(&source, horizontal, sigma_texel, bw, bh, 0.0, [0.0; 4]);
            let view = target.color.clone();
            plan.passes.push(Pass::Blur(BlurJob {
                view: view.clone(),
                bind,
            }));
            result = target.bind;
            source = view;
        }
        ShadowBlit::solid(Some(result), tint, offset_px, pad)
    }

    fn viewport_buffer(&mut self, size: [f32; 2], origin: [f32; 2]) -> wgpu::Buffer {
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("shadow-viewport"),
            size: 16,
            usage: wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.queue
            .write_buffer(&buffer, 0, &viewport_bytes(size, origin));
        self.kept.push(buffer.clone());
        buffer
    }

    fn clip_bytes(
        &mut self,
        plan: &mut Plan,
        path: &Path,
        affine: Affine,
        scale: f32,
    ) -> (Option<u64>, u32) {
        let mesh = self
            .meshes
            .fill(path, FillRule::NonZero, affine.uniform_scale() * scale);
        let mut bytes = Vec::new();
        write_mesh(&mut bytes, &mesh, affine, scale, [1.0, 1.0, 1.0, 1.0]);
        if bytes.is_empty() {
            return (None, 0);
        }
        let offset = plan.path_bytes.len() as u64;
        let verts = (bytes.len() / PATH_STRIDE) as u32;
        plan.path_bytes.extend(bytes);
        (Some(offset), verts)
    }

    fn execute(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        plan: &Plan,
        target: &wgpu::TextureView,
        quad_bind: &wgpu::BindGroup,
        clear: wgpu::Color,
    ) {
        for pass in &plan.passes {
            match pass {
                Pass::Geom(geom) => self.run_geom(encoder, geom),
                Pass::Blur(job) => self.run_blur(encoder, job),
            }
        }
        self.run_root(encoder, plan, target, quad_bind, clear);
    }

    fn run_geom(&self, encoder: &mut wgpu::CommandEncoder, pass: &GeomPass) {
        if let Some(viewport) = &pass.viewport {
            encoder.copy_buffer_to_buffer(viewport, 0, &self.uniform, 0, 16);
        }
        let pipes = if pass.target.samples > 1 {
            self.msaa.as_ref().unwrap_or(&self.plain)
        } else {
            &self.plain
        };
        let color_view = pass.target.msaa.as_ref().unwrap_or(&pass.target.color);
        let resolve = pass.target.msaa.as_ref().map(|_| &pass.target.color);
        let path_bind = self.path_bind();
        {
            let mut render = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("layer"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: color_view,
                    depth_slice: None,
                    resolve_target: resolve,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &pass.target.stencil,
                    depth_ops: None,
                    stencil_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(0),
                        store: wgpu::StoreOp::Discard,
                    }),
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            if let Some((x, y, w, h)) = pass.scissor {
                render.set_scissor_rect(x, y, w, h);
            }
            if let Some(offset) = pass.clip_offset {
                render.set_pipeline(&pipes.path_clip);
                render.set_stencil_reference(1);
                render.set_bind_group(0, &path_bind, &[]);
                let end = offset + pass.clip_verts as u64 * PATH_STRIDE as u64;
                render.set_vertex_buffer(0, self.path_buffer.slice(offset..end));
                render.draw(0..pass.clip_verts, 0..1);
            }
            let inside = pass.clip_offset.is_some();
            for cmd in &pass.cmds {
                match cmd {
                    Cmd::Paths { offset, verts } if *verts > 0 => {
                        render.set_pipeline(if inside { &pipes.path_in } else { &pipes.path });
                        render.set_stencil_reference(1);
                        render.set_bind_group(0, &path_bind, &[]);
                        let end = *offset + *verts as u64 * PATH_STRIDE as u64;
                        render.set_vertex_buffer(0, self.path_buffer.slice(*offset..end));
                        render.draw(0..*verts, 0..1);
                    }
                    Cmd::Sprite {
                        offset,
                        verts,
                        bind,
                    } if *verts > 0 => {
                        render.set_pipeline(if inside {
                            &pipes.sprite_in
                        } else {
                            &pipes.sprite
                        });
                        render.set_stencil_reference(1);
                        render.set_bind_group(0, bind, &[]);
                        let end = *offset + *verts as u64 * SPRITE_STRIDE as u64;
                        render.set_vertex_buffer(0, self.sprite_buffer.slice(*offset..end));
                        render.draw(0..*verts, 0..1);
                    }
                    _ => {}
                }
            }
        }
        if pass.viewport.is_some() {
            encoder.copy_buffer_to_buffer(&self.viewport_frame, 0, &self.uniform, 0, 16);
        }
    }

    fn run_blur(&self, encoder: &mut wgpu::CommandEncoder, job: &BlurJob) {
        let mut render = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("blur"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &job.view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        render.set_pipeline(&self.blur_pipeline);
        render.set_bind_group(0, &job.bind, &[]);
        render.set_vertex_buffer(0, self.blur_quad.slice(..));
        render.draw(0..6, 0..1);
    }

    fn run_root(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        plan: &Plan,
        target: &wgpu::TextureView,
        quad_bind: &wgpu::BindGroup,
        clear: wgpu::Color,
    ) {
        let mut render = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("root"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(clear),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        for cmd in &plan.root {
            match cmd {
                RootCmd::Quads(start, end) if end > start => {
                    render.set_pipeline(&self.quad_pipeline);
                    render.set_bind_group(0, quad_bind, &[]);
                    render.set_vertex_buffer(0, self.quad_geo.slice(..));
                    render.set_vertex_buffer(1, self.quad_buffer.slice(..));
                    render.draw(0..4, *start..*end);
                }
                RootCmd::Sprite {
                    offset,
                    verts,
                    bind,
                } if *verts > 0 => {
                    render.set_pipeline(&self.sprite_swap);
                    render.set_bind_group(0, bind, &[]);
                    let end = *offset + *verts as u64 * SPRITE_STRIDE as u64;
                    render.set_vertex_buffer(0, self.sprite_buffer.slice(*offset..end));
                    render.draw(0..*verts, 0..1);
                }
                _ => {}
            }
        }
    }

    fn ensure_image(&mut self, id: u64, image: &CpuImage, budget: u64) {
        if self.images.contains_key(&id) {
            if let Some(cached) = self.images.get_mut(&id) {
                cached.stamp = self.frame;
            }
            return;
        }
        if image.rgba.len() < (image.width as usize) * (image.height as usize) * 4
            || image.width == 0
        {
            return;
        }
        let mut width = image.width;
        let mut height = image.height;
        let mut pixels = image.rgba.to_vec();
        let mut chain = mip_chain(&pixels, width, height);
        let mut bytes = chain
            .iter()
            .map(|(_, _, level)| level.len() as u64)
            .sum::<u64>();
        while bytes > budget && (width > 1 || height > 1) {
            let (nw, nh, next) = downsample(&pixels, width, height);
            width = nw;
            height = nh;
            pixels = next;
            chain = mip_chain(&pixels, width, height);
            bytes = chain.iter().map(|(_, _, level)| level.len() as u64).sum();
        }
        if bytes > budget {
            return;
        }
        self.evict_until(budget.saturating_sub(bytes));
        let limit = self.max_dim;
        let format = self.texel_format();
        let sampler = self.image_sampler.clone();
        let mut tiles = Vec::new();
        let mut stored = 0u64;
        let mut y = 0u32;
        while y < height {
            let mut x = 0u32;
            let th = (height - y).min(limit);
            while x < width {
                let tw = (width - x).min(limit);
                let cropped = crop(&pixels, width, x, y, tw, th);
                let levels = mip_chain(&cropped, tw, th);
                stored += levels
                    .iter()
                    .map(|(_, _, level)| level.len() as u64)
                    .sum::<u64>();
                let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("image"),
                    size: wgpu::Extent3d {
                        width: tw,
                        height: th,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: levels.len() as u32,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                    view_formats: &[],
                });
                for (level, (lw, lh, data)) in levels.iter().enumerate() {
                    self.queue.write_texture(
                        wgpu::TexelCopyTextureInfo {
                            texture: &texture,
                            mip_level: level as u32,
                            origin: wgpu::Origin3d::ZERO,
                            aspect: wgpu::TextureAspect::All,
                        },
                        data,
                        wgpu::TexelCopyBufferLayout {
                            offset: 0,
                            bytes_per_row: Some(4 * *lw),
                            rows_per_image: Some(*lh),
                        },
                        wgpu::Extent3d {
                            width: *lw,
                            height: *lh,
                            depth_or_array_layers: 1,
                        },
                    );
                }
                let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
                let bind = self.sprite_bind(&view, &sampler);
                let _ = texture;
                tiles.push(GpuTile {
                    u0: x as f32 / width as f32,
                    v0: y as f32 / height as f32,
                    u1: (x + tw) as f32 / width as f32,
                    v1: (y + th) as f32 / height as f32,
                    bind,
                });
                x += tw;
            }
            y += th;
        }
        self.image_bytes += stored;
        self.images.insert(
            id,
            GpuImage {
                tiles,
                bytes: stored,
                stamp: self.frame,
            },
        );
        self.uploaded.push(id);
    }

    fn evict_until(&mut self, budget: u64) {
        while self.image_bytes > budget && !self.images.is_empty() {
            let oldest = self
                .images
                .iter()
                .min_by_key(|(_, image)| image.stamp)
                .map(|(id, _)| *id);
            let Some(id) = oldest else { break };
            if let Some(image) = self.images.remove(&id) {
                self.image_bytes = self.image_bytes.saturating_sub(image.bytes);
            }
        }
    }

    fn alloc(&mut self, width: u32, height: u32, samples: u32) -> TargetUse {
        let width = width.max(1);
        let height = height.max(1);
        if self.pool.len() > 24 {
            if let Some(index) = self.pool.iter().position(|slot| slot.stamp != self.frame) {
                self.pool.remove(index);
            }
        }
        if let Some(index) = self.pool.iter().position(|slot| {
            slot.width == width
                && slot.height == height
                && slot.samples == samples
                && slot.stamp != self.frame
        }) {
            self.pool[index].stamp = self.frame;
            return self.pool[index].to_use();
        }
        let slot = self.create_slot(width, height, samples);
        self.pool.push(slot);
        self.pool.last().unwrap().to_use()
    }

    fn create_slot(&self, width: u32, height: u32, samples: u32) -> PoolSlot {
        let extent = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        let color = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("layer"),
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let msaa = if samples > 1 {
            Some(self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("layer-msaa"),
                size: extent,
                mip_level_count: 1,
                sample_count: samples,
                dimension: wgpu::TextureDimension::D2,
                format: self.format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            }))
        } else {
            None
        };
        let stencil = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("layer-stencil"),
            size: extent,
            mip_level_count: 1,
            sample_count: samples,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth24PlusStencil8,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let color_view = color.create_view(&wgpu::TextureViewDescriptor::default());
        let msaa_view = msaa
            .as_ref()
            .map(|texture| texture.create_view(&wgpu::TextureViewDescriptor::default()));
        let stencil_view = stencil.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = self.image_sampler.clone();
        let bind = self.sprite_bind(&color_view, &sampler);
        let mut keep = vec![color, stencil];
        if let Some(msaa) = msaa {
            keep.push(msaa);
        }
        PoolSlot {
            width,
            height,
            samples,
            color_view,
            msaa_view,
            stencil_view,
            bind,
            stamp: self.frame,
            _keep: keep,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn blur_bind(
        &mut self,
        src: &wgpu::TextureView,
        horizontal: bool,
        sigma: f32,
        width: u32,
        height: u32,
        mode: f32,
        map: [f32; 4],
    ) -> wgpu::BindGroup {
        let mut data = [0u8; 32];
        let (dx, dy) = if horizontal {
            (1.0 / width.max(1) as f32, 0.0)
        } else {
            (0.0, 1.0 / height.max(1) as f32)
        };
        data[0..4].copy_from_slice(&dx.to_le_bytes());
        data[4..8].copy_from_slice(&dy.to_le_bytes());
        data[8..12].copy_from_slice(&sigma.to_le_bytes());
        data[12..16].copy_from_slice(&mode.to_le_bytes());
        for (index, value) in map.iter().enumerate() {
            let at = 16 + index * 4;
            data[at..at + 4].copy_from_slice(&value.to_le_bytes());
        }
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("blur-params"),
            size: 32,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.queue.write_buffer(&buffer, 0, &data);
        let sampler = self.image_sampler.clone();
        let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("blur"),
            layout: &self.blur_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(src),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
        self.kept.push(buffer);
        bind
    }

    fn texel_format(&self) -> wgpu::TextureFormat {
        if self.format.is_srgb() {
            wgpu::TextureFormat::Rgba8UnormSrgb
        } else {
            wgpu::TextureFormat::Rgba8Unorm
        }
    }

    fn sprite_bind(&self, view: &wgpu::TextureView, sampler: &wgpu::Sampler) -> wgpu::BindGroup {
        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("sprite"),
            layout: &self.sprite_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        })
    }

    fn path_bind(&self) -> wgpu::BindGroup {
        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("path"),
            layout: &self.path_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: self.uniform.as_entire_binding(),
            }],
        })
    }

    fn quad_bind(
        &self,
        coverage: &wgpu::TextureView,
        color: &wgpu::TextureView,
    ) -> wgpu::BindGroup {
        let sampler = self.glyph_sampler.clone();
        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("quad"),
            layout: &self.quad_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(coverage),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(color),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        })
    }
}

impl PoolSlot {
    fn to_use(&self) -> TargetUse {
        TargetUse {
            samples: self.samples,
            color: self.color_view.clone(),
            msaa: self.msaa_view.clone(),
            stencil: self.stencil_view.clone(),
            bind: self.bind.clone(),
        }
    }
}

fn plan_color_view(plan: &Plan) -> Option<wgpu::TextureView> {
    plan.passes.iter().rev().find_map(|pass| match pass {
        Pass::Geom(geom) => Some(geom.target.color.clone()),
        Pass::Blur(_) => None,
    })
}

fn matching_pop(ops: &[Op], start: usize) -> usize {
    let mut depth = 0i32;
    for (index, op) in ops.iter().enumerate().skip(start) {
        match op {
            Op::PushLayer(_) => depth += 1,
            Op::PopLayer => {
                depth -= 1;
                if depth == 0 {
                    return index;
                }
            }
            _ => {}
        }
    }
    ops.len()
}

fn pack_cmds(plan: &mut Plan, drawn: Vec<Drawn>, width: u32, height: u32) -> Vec<Cmd> {
    let mut cmds = Vec::new();
    let mut acc = Vec::new();
    for item in drawn {
        match item {
            Drawn::Paths(bytes) => acc.extend(bytes),
            Drawn::Sprite(sprite) => {
                flush_paths_acc(plan, &mut acc, &mut cmds);
                push_cmd_sprite(plan, &mut cmds, sprite);
            }
            Drawn::Blit(blit) => {
                flush_paths_acc(plan, &mut acc, &mut cmds);
                for sprite in blit_sprites(blit, width, height) {
                    push_cmd_sprite(plan, &mut cmds, sprite);
                }
            }
        }
    }
    flush_paths_acc(plan, &mut acc, &mut cmds);
    cmds
}

fn flush_paths_acc(plan: &mut Plan, acc: &mut Vec<u8>, cmds: &mut Vec<Cmd>) {
    if acc.is_empty() {
        return;
    }
    let bytes = std::mem::take(acc);
    let offset = plan.path_bytes.len() as u64;
    let verts = (bytes.len() / PATH_STRIDE) as u32;
    plan.path_bytes.extend(bytes);
    cmds.push(Cmd::Paths { offset, verts });
}

fn push_cmd_sprite(plan: &mut Plan, cmds: &mut Vec<Cmd>, sprite: SpriteDraw) {
    let verts = (sprite.bytes.len() / SPRITE_STRIDE) as u32;
    if verts == 0 {
        return;
    }
    let offset = plan.sprite_bytes.len() as u64;
    plan.sprite_bytes.extend(sprite.bytes);
    cmds.push(Cmd::Sprite {
        offset,
        verts,
        bind: sprite.bind,
    });
}

fn push_root_sprite(plan: &mut Plan, sprite: SpriteDraw) {
    let verts = (sprite.bytes.len() / SPRITE_STRIDE) as u32;
    if verts == 0 {
        return;
    }
    let offset = plan.sprite_bytes.len() as u64;
    plan.sprite_bytes.extend(sprite.bytes);
    plan.root.push(RootCmd::Sprite {
        offset,
        verts,
        bind: sprite.bind,
    });
}

fn blit_sprites(blit: BlitDraw, width: u32, height: u32) -> Vec<SpriteDraw> {
    let mut out = Vec::new();
    if let Some(shadow) = blit.shadow {
        if blit.shadow_color[3] > 0.0 {
            let mut bytes = Vec::new();
            if let Some(place) = blit.shadow_place {
                push_quad(
                    &mut bytes,
                    Affine::translate(place[0] + blit.offset[0], place[1] + blit.offset[1]),
                    1.0,
                    1.0,
                    0.0,
                    0.0,
                    0.0,
                    0.0,
                    place[2],
                    place[3],
                    [0.0, 0.0, 1.0, 1.0],
                    blit.shadow_color,
                    0.0,
                    place[2],
                    place[3],
                    4.0,
                    [
                        place[0] + blit.offset[0],
                        place[1] + blit.offset[1],
                        -1.0,
                        0.0,
                    ],
                );
            } else {
                let pad = if blit.shadow_pad.is_finite() && blit.shadow_pad > 0.0 {
                    blit.shadow_pad
                } else {
                    0.0
                };
                let ox = blit.offset[0] - pad;
                let oy = blit.offset[1] - pad;
                let w = width as f32 + pad * 2.0;
                let h = height as f32 + pad * 2.0;
                push_quad(
                    &mut bytes,
                    Affine::translate(ox, oy),
                    1.0,
                    1.0,
                    0.0,
                    0.0,
                    0.0,
                    0.0,
                    w,
                    h,
                    [0.0, 0.0, 1.0, 1.0],
                    blit.shadow_color,
                    0.0,
                    w,
                    h,
                    4.0,
                    [ox, oy, -1.0, 0.0],
                );
            }
            out.push(SpriteDraw {
                bytes,
                bind: shadow,
            });
        }
    }
    if blit.opacity > 0.0 {
        let mut bytes = Vec::new();
        push_screen(
            &mut bytes,
            [0.0, 0.0],
            width,
            height,
            [1.0, 1.0, 1.0, blit.opacity],
            3.0,
        );
        out.push(SpriteDraw {
            bytes,
            bind: blit.bind,
        });
    }
    out
}

/// Child transform first, then the affine already on the stack.
///
/// [`Affine::then`] applies `self` first. A pushed transform is the child's
/// local map, and `current` is everything outside it, including the camera.
fn pushed(current: Affine, child: Affine) -> Affine {
    child.then(current)
}

/// How a glyph run is drawn once the device size and the affine are known.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct GlyphChoice {
    /// Fill from face outlines when the engine can supply them.
    outlines: bool,
    /// Sample atlas sprites linearly when outlines are unavailable.
    /// Axis-aligned sprites stay on the nearest sampler.
    linear_sprites: bool,
}

fn axis_aligned(affine: Affine) -> bool {
    let scale = affine.uniform_scale().max(1.0e-6);
    let slack = scale * 1.0e-3;
    affine.m[1].abs() <= slack && affine.m[2].abs() <= slack
}

fn glyph_choice(device: f32, affine: Affine) -> GlyphChoice {
    let rotated = !axis_aligned(affine);
    GlyphChoice {
        outlines: device >= OUTLINE_PX || rotated,
        linear_sprites: rotated,
    }
}

/// Affine in effect just before `ops[index]`, starting from identity.
///
/// Paths, images, and glyphs are all drawn with this affine. A child push
/// runs before the parent already on the stack. The draw path keeps its own
/// stack; this walk exists so a test can name the matrix at one command.
#[cfg(test)]
fn current_affine(ops: &[Op], index: usize) -> Affine {
    let mut stack = vec![Affine::IDENTITY];
    for op in ops.iter().take(index) {
        match op {
            Op::PushTransform(affine) => {
                let current = *stack.last().unwrap_or(&Affine::IDENTITY);
                stack.push(pushed(current, *affine));
            }
            Op::PopTransform if stack.len() > 1 => {
                stack.pop();
            }
            _ => {}
        }
    }
    *stack.last().unwrap_or(&Affine::IDENTITY)
}

fn write_mesh(out: &mut Vec<u8>, mesh: &Mesh, affine: Affine, scale: f32, color: [f32; 4]) {
    for tri in mesh.indices.as_chunks::<3>().0 {
        for index in tri {
            let point = mesh
                .vertices
                .get(*index as usize)
                .copied()
                .unwrap_or([0.0, 0.0]);
            let mapped = affine.transform_point(point);
            push_f32(
                out,
                &[
                    mapped[0] * scale,
                    mapped[1] * scale,
                    color[0],
                    color[1],
                    color[2],
                    color[3],
                ],
            );
        }
    }
}

/// No sprite scissor. A negative width skips the mask in the shader.
const NO_SCISSOR: [f32; 4] = [0.0, 0.0, -1.0, 0.0];

#[allow(clippy::too_many_arguments)]
fn push_quad(
    out: &mut Vec<u8>,
    affine: Affine,
    scale: f32,
    local_scale: f32,
    ox: f32,
    oy: f32,
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    uv: [f32; 4],
    color: [f32; 4],
    radius: f32,
    dest_w: f32,
    dest_h: f32,
    mode: f32,
    scissor: [f32; 4],
) {
    let corners = [
        (x0, y0, uv[0], uv[1]),
        (x1, y0, uv[2], uv[1]),
        (x0, y1, uv[0], uv[3]),
        (x1, y0, uv[2], uv[1]),
        (x0, y1, uv[0], uv[3]),
        (x1, y1, uv[2], uv[3]),
    ];
    let local_scale = if local_scale.is_finite() && local_scale > 0.0 {
        local_scale
    } else {
        1.0
    };
    for (x, y, u, v) in corners {
        let mapped = affine.transform_point([ox + x, oy + y]);
        push_f32(
            out,
            &[
                mapped[0] * scale,
                mapped[1] * scale,
                u,
                v,
                color[0],
                color[1],
                color[2],
                color[3],
                x * local_scale,
                y * local_scale,
                radius * local_scale,
                dest_w * local_scale,
                dest_h * local_scale,
                mode,
                scissor[0],
                scissor[1],
                scissor[2],
                scissor[3],
            ],
        );
    }
}

fn push_screen(
    out: &mut Vec<u8>,
    offset: [f32; 2],
    width: u32,
    height: u32,
    color: [f32; 4],
    mode: f32,
) {
    push_screen_padded(out, offset, 0.0, width, height, color, mode);
}

/// Fullscreen quad expanded by `pad` device pixels on every side.
///
/// The shadow texture includes that padding, so the quad has to cover it or
/// the falloff is cropped back to the layer rectangle.
fn push_screen_padded(
    out: &mut Vec<u8>,
    offset: [f32; 2],
    pad: f32,
    width: u32,
    height: u32,
    color: [f32; 4],
    mode: f32,
) {
    let pad = if pad.is_finite() && pad > 0.0 {
        pad
    } else {
        0.0
    };
    let w = width as f32 + pad * 2.0;
    let h = height as f32 + pad * 2.0;
    push_quad(
        out,
        Affine::translate(offset[0] - pad, offset[1] - pad),
        1.0,
        1.0,
        0.0,
        0.0,
        0.0,
        0.0,
        w,
        h,
        [0.0, 0.0, 1.0, 1.0],
        color,
        0.0,
        w,
        h,
        mode,
        NO_SCISSOR,
    );
}

struct ShadowBlit {
    bind: Option<wgpu::BindGroup>,
    color: [f32; 4],
    offset: [f32; 2],
    pad: f32,
    place: Option<[f32; 4]>,
}

impl ShadowBlit {
    fn none() -> Self {
        Self {
            bind: None,
            color: [0.0; 4],
            offset: [0.0; 2],
            pad: 0.0,
            place: None,
        }
    }

    fn solid(bind: Option<wgpu::BindGroup>, color: [f32; 4], offset: [f32; 2], pad: f32) -> Self {
        Self {
            bind,
            color,
            offset,
            pad,
            place: None,
        }
    }
}

/// Device-pixel bounds of `drawn`, min x/y then max x/y.
///
/// Blit quads are the size of the viewport they were baked into. Their
/// `extent` is the content, so a parent shadow does not grow to the screen.
fn content_extent(drawn: &[Drawn]) -> Option<[f32; 4]> {
    let mut acc = None;
    for item in drawn {
        match item {
            Drawn::Paths(bytes) => union_positions(&mut acc, bytes, PATH_STRIDE),
            Drawn::Sprite(sprite) => union_positions(&mut acc, &sprite.bytes, SPRITE_STRIDE),
            Drawn::Blit(blit) => {
                if let Some(extent) = blit.extent {
                    union_box(&mut acc, extent);
                }
            }
        }
    }
    acc
}

fn union_positions(acc: &mut Option<[f32; 4]>, bytes: &[u8], stride: usize) {
    if stride < 8 {
        return;
    }
    let mut offset = 0;
    while offset + 8 <= bytes.len() {
        let x = f32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
        let y = f32::from_le_bytes(bytes[offset + 4..offset + 8].try_into().unwrap());
        if x.is_finite() && y.is_finite() {
            union_point(acc, x, y);
        }
        offset += stride;
    }
}

fn union_point(acc: &mut Option<[f32; 4]>, x: f32, y: f32) {
    match acc {
        Some(box_) => {
            box_[0] = box_[0].min(x);
            box_[1] = box_[1].min(y);
            box_[2] = box_[2].max(x);
            box_[3] = box_[3].max(y);
        }
        None => *acc = Some([x, y, x, y]),
    }
}

fn union_box(acc: &mut Option<[f32; 4]>, rect: [f32; 4]) {
    if !rect.iter().all(|value| value.is_finite()) {
        return;
    }
    union_point(acc, rect[0], rect[1]);
    union_point(acc, rect[2], rect[3]);
}

/// Texture rect covering `min`..`max` expanded by `pad`, snapped so an integer
/// translation of the box moves the origin by that integer and keeps the size.
///
/// Returns origin x, origin y, width, height. `None` when the box is empty
/// or not finite.
fn snapped_shadow_rect(
    min_x: f32,
    min_y: f32,
    max_x: f32,
    max_y: f32,
    pad: f32,
) -> Option<(f32, f32, u32, u32)> {
    if ![min_x, min_y, max_x, max_y, pad]
        .iter()
        .all(|value| value.is_finite())
        || max_x <= min_x
        || max_y <= min_y
    {
        return None;
    }
    let x0 = (min_x - pad).floor();
    let y0 = (min_y - pad).floor();
    let x1 = (max_x + pad).ceil();
    let y1 = (max_y + pad).ceil();
    let width = (x1 - x0).round();
    let height = (y1 - y0).round();
    if width < 1.0 || height < 1.0 || width > u32::MAX as f32 || height > u32::MAX as f32 {
        return None;
    }
    Some((x0, y0, width as u32, height as u32))
}

/// Copy shadow geometry into texture space.
///
/// `device - origin` is not stable across an integer pan: the translation is
/// added before the origin is removed, and the fraction rounds differently.
/// Positions are quantized to 1/256 px, which is coarser than that rounding
/// and much finer than a coverage sample, so both pans land on the same bits.
fn retarget_shadow_cmds(plan: &mut Plan, cmds: &[Cmd], origin: [f32; 2]) -> Vec<Cmd> {
    cmds.iter()
        .map(|cmd| match cmd {
            Cmd::Paths { offset, verts } => {
                let new_offset = plan.path_bytes.len() as u64;
                copy_retarget(
                    &mut plan.path_bytes,
                    *offset,
                    *verts,
                    PATH_STRIDE,
                    origin,
                    false,
                );
                Cmd::Paths {
                    offset: new_offset,
                    verts: *verts,
                }
            }
            Cmd::Sprite {
                offset,
                verts,
                bind,
            } => {
                let new_offset = plan.sprite_bytes.len() as u64;
                copy_retarget(
                    &mut plan.sprite_bytes,
                    *offset,
                    *verts,
                    SPRITE_STRIDE,
                    origin,
                    true,
                );
                Cmd::Sprite {
                    offset: new_offset,
                    verts: *verts,
                    bind: bind.clone(),
                }
            }
        })
        .collect()
}

fn retarget_shadow_clip(
    plan: &mut Plan,
    offset: Option<u64>,
    verts: u32,
    origin: [f32; 2],
) -> (Option<u64>, u32) {
    let Some(offset) = offset else {
        return (None, 0);
    };
    if verts == 0 {
        return (None, 0);
    }
    let new_offset = plan.path_bytes.len() as u64;
    copy_retarget(
        &mut plan.path_bytes,
        offset,
        verts,
        PATH_STRIDE,
        origin,
        false,
    );
    (Some(new_offset), verts)
}

fn copy_retarget(
    dest: &mut Vec<u8>,
    offset: u64,
    verts: u32,
    stride: usize,
    origin: [f32; 2],
    sprite: bool,
) {
    let start = offset as usize;
    let bytes = verts as usize * stride;
    let end = start + bytes;
    if end > dest.len() || stride < 8 {
        return;
    }
    let copied = dest[start..end].to_vec();
    let base = dest.len();
    dest.extend_from_slice(&copied);
    let mut at = base;
    while at + 8 <= base + bytes {
        let x = f32::from_le_bytes(dest[at..at + 4].try_into().unwrap());
        let y = f32::from_le_bytes(dest[at + 4..at + 8].try_into().unwrap());
        let x = stable_shadow_local(x, origin[0]);
        let y = stable_shadow_local(y, origin[1]);
        dest[at..at + 4].copy_from_slice(&x.to_le_bytes());
        dest[at + 4..at + 8].copy_from_slice(&y.to_le_bytes());
        if sprite && at + 64 <= base + bytes {
            // Scissor xy is in device pixels. Width stays; a negative width
            // is "no mask" and the xy are ignored.
            let sx = f32::from_le_bytes(dest[at + 56..at + 60].try_into().unwrap());
            let sy = f32::from_le_bytes(dest[at + 60..at + 64].try_into().unwrap());
            let sx = stable_shadow_local(sx, origin[0]);
            let sy = stable_shadow_local(sy, origin[1]);
            dest[at + 56..at + 60].copy_from_slice(&sx.to_le_bytes());
            dest[at + 60..at + 64].copy_from_slice(&sy.to_le_bytes());
        }
        at += stride;
    }
}

fn stable_shadow_local(device: f32, origin: f32) -> f32 {
    let local = device - origin;
    if !local.is_finite() {
        return 0.0;
    }
    const QUANTUM: f32 = 1.0 / 256.0;
    (local / QUANTUM).round() * QUANTUM
}

fn viewport_bytes(size: [f32; 2], origin: [f32; 2]) -> [u8; 16] {
    let mut out = [0u8; 16];
    out[0..4].copy_from_slice(&size[0].to_le_bytes());
    out[4..8].copy_from_slice(&size[1].to_le_bytes());
    out[8..12].copy_from_slice(&origin[0].to_le_bytes());
    out[12..16].copy_from_slice(&origin[1].to_le_bytes());
    out
}

/// Device pixels of padding on every side of a blur whose sigma is `sigma_px`.
fn blur_pad(sigma_px: f32) -> f32 {
    if !sigma_px.is_finite() || sigma_px <= 0.0 {
        return 0.0;
    }
    (sigma_px * BLUR_SPREAD).ceil() + 1.0
}

/// Device pixels covered by one blur texel.
///
/// Sigma at or under 8 stays on the device grid. Wider blurs coarsen to
/// [`BLUR_MAX_SPACING`] so the linear upsample does not turn into flat steps,
/// and coarsen further only when [`BLUR_SPREAD`] sigmas would not fit in
/// [`BLUR_MAX_RADIUS`] taps.
fn blur_factor(sigma_px: f32) -> f32 {
    if !sigma_px.is_finite() || sigma_px <= 8.0 {
        return 1.0;
    }
    let min_factor = (sigma_px * BLUR_SPREAD / BLUR_MAX_RADIUS as f32)
        .ceil()
        .max(1.0);
    min_factor.max(BLUR_MAX_SPACING)
}

/// Width, height, and sigma in texels for the padded blur target.
fn blur_dims(sigma_px: f32, width: u32, height: u32, pad: f32, max_dim: u32) -> (u32, u32, f32) {
    let expanded_w = width as f32 + pad * 2.0;
    let expanded_h = height as f32 + pad * 2.0;
    let max_side = max_dim.max(1);
    let mut factor = blur_factor(sigma_px).max(1.0);
    let fit = (expanded_w / max_side as f32)
        .max(expanded_h / max_side as f32)
        .ceil()
        .max(1.0);
    if fit > factor {
        factor = fit;
    }
    let bw = (expanded_w / factor).ceil().clamp(1.0, max_side as f32) as u32;
    let bh = (expanded_h / factor).ceil().clamp(1.0, max_side as f32) as u32;
    let sigma_texel = sigma_px * bw as f32 / expanded_w.max(1.0);
    (bw, bh, sigma_texel)
}

fn push_f32(out: &mut Vec<u8>, values: &[f32]) {
    for value in values {
        out.extend_from_slice(&value.to_le_bytes());
    }
}

fn overlap(a0: f32, a1: f32, b0: f32, b1: f32) -> Option<(f32, f32)> {
    let u0 = a0.max(b0);
    let u1 = a1.min(b1);
    if u1 - u0 > 1.0e-5 {
        Some((u0, u1))
    } else {
        None
    }
}

fn scissor_px(
    rect: rndr_core::Rect,
    scale: f32,
    width: u32,
    height: u32,
) -> Option<(u32, u32, u32, u32)> {
    if width == 0 || height == 0 {
        return None;
    }
    let x0 = (rect.x * scale).floor().max(0.0) as u32;
    let y0 = (rect.y * scale).floor().max(0.0) as u32;
    let x1 = ((rect.x + rect.w) * scale).ceil().max(0.0) as u32;
    let y1 = ((rect.y + rect.h) * scale).ceil().max(0.0) as u32;
    let x = x0.min(width.saturating_sub(1));
    let y = y0.min(height.saturating_sub(1));
    let w = x1.saturating_sub(x).min(width - x);
    let h = y1.saturating_sub(y).min(height - y);
    if w == 0 || h == 0 {
        None
    } else {
        Some((x, y, w, h))
    }
}

fn crop(src: &[u8], stride: u32, x: u32, y: u32, w: u32, h: u32) -> Vec<u8> {
    let mut out = vec![0u8; (w as usize) * (h as usize) * 4];
    for row in 0..h {
        let from = ((y + row) * stride + x) as usize * 4;
        let to = from + w as usize * 4;
        let dest = row as usize * w as usize * 4;
        if to <= src.len() {
            out[dest..dest + w as usize * 4].copy_from_slice(&src[from..to]);
        }
    }
    out
}

fn buffer_entry(binding: u32, visibility: wgpu::ShaderStages) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn texture_entry(binding: u32, visibility: wgpu::ShaderStages) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

fn sampler_entry(binding: u32, visibility: wgpu::ShaderStages) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
        count: None,
    }
}

fn color_target(format: wgpu::TextureFormat, write: wgpu::ColorWrites) -> wgpu::ColorTargetState {
    wgpu::ColorTargetState {
        format,
        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
        write_mask: write,
    }
}

fn empty_vertex(device: &wgpu::Device, label: &str, size: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn init_buffer(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: &str,
    bytes: &[u8],
    usage: wgpu::BufferUsages,
) -> wgpu::Buffer {
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: bytes.len() as u64,
        usage: usage | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(&buffer, 0, bytes);
    buffer
}

fn grow(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    buffer: &wgpu::Buffer,
    cap: &mut u64,
    bytes: &[u8],
    label: &str,
) -> wgpu::Buffer {
    if bytes.is_empty() {
        return buffer.clone();
    }
    let buffer = if bytes.len() as u64 > *cap {
        *cap = (bytes.len() as u64).next_power_of_two().max(256);
        empty_vertex(device, label, *cap)
    } else {
        buffer.clone()
    };
    queue.write_buffer(&buffer, 0, bytes);
    buffer
}

fn unit_quad_bytes() -> Vec<u8> {
    let mut bytes = Vec::new();
    for [x, y] in [[0.0f32, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]] {
        push_f32(&mut bytes, &[x, y]);
    }
    bytes
}

fn blur_quad_bytes() -> Vec<u8> {
    let mut bytes = Vec::new();
    for (x, y, u, v) in [
        (-1.0, -1.0, 0.0, 1.0),
        (1.0, -1.0, 1.0, 1.0),
        (-1.0, 1.0, 0.0, 0.0),
        (1.0, -1.0, 1.0, 1.0),
        (-1.0, 1.0, 0.0, 0.0),
        (1.0, 1.0, 1.0, 0.0),
    ] {
        push_f32(&mut bytes, &[x, y, u, v]);
    }
    bytes
}

fn quad_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
) -> (wgpu::BindGroupLayout, wgpu::RenderPipeline) {
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("quad-bg"),
        entries: &[
            buffer_entry(0, wgpu::ShaderStages::VERTEX),
            texture_entry(1, wgpu::ShaderStages::FRAGMENT),
            sampler_entry(2, wgpu::ShaderStages::FRAGMENT),
            texture_entry(3, wgpu::ShaderStages::FRAGMENT),
            sampler_entry(4, wgpu::ShaderStages::FRAGMENT),
        ],
    });
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("quads"),
        source: wgpu::ShaderSource::Wgsl(gpu::QUAD_SHADER.into()),
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("quad-pl"),
        bind_group_layouts: &[Some(&layout)],
        immediate_size: 0,
    });
    const QUAD_ATTRS: [wgpu::VertexAttribute; 1] = wgpu::vertex_attr_array![0 => Float32x2];
    const INSTANCE_ATTRS: [wgpu::VertexAttribute; 5] = wgpu::vertex_attr_array![
        1 => Float32x4, 2 => Float32x4, 3 => Float32x4, 4 => Float32x4, 5 => Float32x4,
    ];
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("quads"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs"),
            compilation_options: Default::default(),
            buffers: &[
                Some(wgpu::VertexBufferLayout {
                    array_stride: 8,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &QUAD_ATTRS,
                }),
                Some(wgpu::VertexBufferLayout {
                    array_stride: INSTANCE_STRIDE,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &INSTANCE_ATTRS,
                }),
            ],
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleStrip,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs"),
            compilation_options: Default::default(),
            targets: &[Some(color_target(format, wgpu::ColorWrites::ALL))],
        }),
        multiview_mask: None,
        cache: None,
    });
    (layout, pipeline)
}

fn pipe_set(
    device: &wgpu::Device,
    path_shader: &wgpu::ShaderModule,
    sprite_shader: &wgpu::ShaderModule,
    path_layout: &wgpu::BindGroupLayout,
    sprite_layout: &wgpu::BindGroupLayout,
    format: wgpu::TextureFormat,
    samples: u32,
) -> PipeSet {
    PipeSet {
        samples,
        path: path_pipeline(
            device,
            path_shader,
            path_layout,
            format,
            samples,
            ClipMode::Draw,
        ),
        path_clip: path_pipeline(
            device,
            path_shader,
            path_layout,
            format,
            samples,
            ClipMode::Write,
        ),
        path_in: path_pipeline(
            device,
            path_shader,
            path_layout,
            format,
            samples,
            ClipMode::Inside,
        ),
        sprite: sprite_pipeline(
            device,
            sprite_shader,
            sprite_layout,
            format,
            samples,
            Some(wgpu::CompareFunction::Always),
        ),
        sprite_in: sprite_pipeline(
            device,
            sprite_shader,
            sprite_layout,
            format,
            samples,
            Some(wgpu::CompareFunction::Equal),
        ),
    }
}

#[derive(Clone, Copy)]
enum ClipMode {
    Draw,
    Write,
    Inside,
}

fn path_pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    layout: &wgpu::BindGroupLayout,
    format: wgpu::TextureFormat,
    samples: u32,
    clip: ClipMode,
) -> wgpu::RenderPipeline {
    const ATTRS: [wgpu::VertexAttribute; 2] =
        wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x4];
    let (compare, pass_op, write) = match clip {
        ClipMode::Draw => (
            wgpu::CompareFunction::Always,
            wgpu::StencilOperation::Keep,
            wgpu::ColorWrites::ALL,
        ),
        ClipMode::Write => (
            wgpu::CompareFunction::Always,
            wgpu::StencilOperation::Replace,
            wgpu::ColorWrites::empty(),
        ),
        ClipMode::Inside => (
            wgpu::CompareFunction::Equal,
            wgpu::StencilOperation::Keep,
            wgpu::ColorWrites::ALL,
        ),
    };
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("path-pl"),
        bind_group_layouts: &[Some(layout)],
        immediate_size: 0,
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("path"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs"),
            compilation_options: Default::default(),
            buffers: &[Some(wgpu::VertexBufferLayout {
                array_stride: PATH_STRIDE as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &ATTRS,
            })],
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            ..Default::default()
        },
        depth_stencil: Some(stencil_state(compare, pass_op)),
        multisample: wgpu::MultisampleState {
            count: samples,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs"),
            compilation_options: Default::default(),
            targets: &[Some(color_target(format, write))],
        }),
        multiview_mask: None,
        cache: None,
    })
}

fn sprite_pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    layout: &wgpu::BindGroupLayout,
    format: wgpu::TextureFormat,
    samples: u32,
    stencil: Option<wgpu::CompareFunction>,
) -> wgpu::RenderPipeline {
    const ATTRS: [wgpu::VertexAttribute; 6] = wgpu::vertex_attr_array![
        0 => Float32x2, 1 => Float32x2, 2 => Float32x4, 3 => Float32x2, 4 => Float32x4, 5 => Float32x4,
    ];
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("sprite-pl"),
        bind_group_layouts: &[Some(layout)],
        immediate_size: 0,
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("sprite"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs"),
            compilation_options: Default::default(),
            buffers: &[Some(wgpu::VertexBufferLayout {
                array_stride: SPRITE_STRIDE as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &ATTRS,
            })],
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            ..Default::default()
        },
        depth_stencil: stencil.map(|compare| stencil_state(compare, wgpu::StencilOperation::Keep)),
        multisample: wgpu::MultisampleState {
            count: samples,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs"),
            compilation_options: Default::default(),
            targets: &[Some(color_target(format, wgpu::ColorWrites::ALL))],
        }),
        multiview_mask: None,
        cache: None,
    })
}

fn stencil_state(
    compare: wgpu::CompareFunction,
    pass_op: wgpu::StencilOperation,
) -> wgpu::DepthStencilState {
    let face = wgpu::StencilFaceState {
        compare,
        fail_op: wgpu::StencilOperation::Keep,
        depth_fail_op: wgpu::StencilOperation::Keep,
        pass_op,
    };
    wgpu::DepthStencilState {
        format: wgpu::TextureFormat::Depth24PlusStencil8,
        depth_write_enabled: Some(false),
        depth_compare: Some(wgpu::CompareFunction::Always),
        stencil: wgpu::StencilState {
            front: face,
            back: face,
            read_mask: 0xff,
            write_mask: 0xff,
        },
        bias: wgpu::DepthBiasState::default(),
    }
}

#[cfg(test)]
mod tests {
    use std::f32::consts::PI;

    use super::*;

    #[test]
    fn rotated_glyphs_use_outlines_and_a_linear_fallback() {
        let upright = Affine::scale(2.0).then(Affine::translate(3.0, 4.0));
        let small = glyph_choice(32.0, upright);
        assert!(!small.outlines);
        assert!(!small.linear_sprites);
        let large = glyph_choice(OUTLINE_PX, upright);
        assert!(large.outlines);
        assert!(!large.linear_sprites, "axis-aligned sprites stay nearest");
        let turned = glyph_choice(32.0, Affine::rotate(0.2));
        assert!(turned.outlines);
        assert!(turned.linear_sprites);
    }

    #[test]
    fn a_rotated_child_runs_before_the_camera() {
        // Glyphs and outlines use this same affine. Identity on the outside
        // hides a reversed multiply, so the camera scale has to be non-1.
        let camera = Affine::scale(2.0);
        let child = Affine::rotate(PI / 2.0).then(Affine::translate(30.0, 10.0));
        let ops = [
            Op::PushTransform(camera),
            Op::PushTransform(child),
            Op::PopTransform,
            Op::PopTransform,
        ];
        let mapped = current_affine(&ops, 2).transform_point([10.0, 0.0]);
        assert!(
            (mapped[0] - 60.0).abs() < 1.0e-3 && (mapped[1] - 40.0).abs() < 1.0e-3,
            "mapped {mapped:?}"
        );
    }

    #[test]
    fn a_zoomed_shadow_stays_on_a_fine_grid_and_keeps_its_tail() {
        // 10 wheel notches at 1.21× from the default view is about 6.7×.
        // A 16px sigma there used to step about 35 device pixels.
        let sigma = 16.0 * 6.7;
        let factor = blur_factor(sigma);
        assert!(factor <= BLUR_MAX_SPACING + 1.0, "blur texel {factor} px");
        let pad = blur_pad(sigma);
        assert!(pad >= sigma * 3.0, "pad {pad} sigma {sigma}");
        let (bw, bh, sigma_texel) = blur_dims(sigma, 1280, 800, pad, 8192);
        let spacing = (1280.0 + pad * 2.0) / bw as f32;
        assert!(spacing <= 8.0, "spacing {spacing} grid {bw}x{bh}");
        let radius = (sigma_texel * BLUR_SPREAD).ceil();
        assert!(
            radius <= BLUR_MAX_RADIUS as f32 + 0.5,
            "radius {radius} sigma_texel {sigma_texel}"
        );
        assert!(blur_factor(2.0) == 1.0);
        assert!(blur_pad(0.0) == 0.0);
    }

    #[test]
    fn an_integer_pan_slides_the_shadow_texture_without_resizing_it() {
        let pad = blur_pad(8.0);
        let (x0, y0, w, h) = snapped_shadow_rect(68.0, 262.0, 238.0, 362.0, pad).unwrap();
        let (x1, y1, w1, h1) = snapped_shadow_rect(108.0, 282.0, 278.0, 382.0, pad).unwrap();
        assert_eq!((w, h), (w1, h1));
        assert_eq!((x1 - x0, y1 - y0), (40.0, 20.0));
        assert!(x0 <= 68.0 - pad);
        assert!(y0 <= 262.0 - pad);
        assert!(x0 + w as f32 >= 238.0 + pad);
        assert!(y0 + h as f32 >= 362.0 + pad);
        // An integer pan rounds `mesh + translation` before the origin is
        // removed. Both results have to quantize to the same texture position.
        let on_screen = stable_shadow_local(f32::from_bits(0x429b_265c), 39.0);
        let clipped = stable_shadow_local(f32::from_bits(0xc01b_3490), -41.0);
        assert_eq!(on_screen.to_bits(), clipped.to_bits());
        assert!(snapped_shadow_rect(1.0, 1.0, 1.0, 2.0, 0.0).is_none());
        assert!(snapped_shadow_rect(0.0, 0.0, f32::NAN, 1.0, 1.0).is_none());
    }
}
