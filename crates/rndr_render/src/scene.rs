//! Display-list renderer.
//!
//! UI quads stay on the signed-distance pipeline at one sample. Paths and
//! layer groups are drawn into an offscreen target. That target is
//! multisampled at 4× when the format can resolve it, then resolved before a
//! blur, an opacity composite, or the quad pass. [`rndr_paint`] documents why
//! the edges are multisampled instead of drawn as a coverage fringe: one
//! tessellation, no halos on joins, and the swapchain stays single-sample.
//!
//! A drop shadow is two fullscreen passes, horizontal then vertical, over the
//! group's alpha. [`FilterPass`] is that list. Another effect is another
//! variant and another pass in [`SceneGpu::shadow`].

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
const SPRITE_STRIDE: usize = 56;

/// One fullscreen pass a resolved layer can run. A drop shadow schedules two
/// of these, horizontal then vertical.
#[derive(Clone, Copy, Debug)]
enum FilterPass {
    /// Gaussian of `sigma_px` device pixels along one axis.
    Gaussian { horizontal: bool, sigma_px: f32 },
}

const PATH_SHADER: &str = r#"
struct Viewport { size: vec2<f32> }
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
    let ndc = vec2<f32>(in.pos.x / viewport.size.x * 2.0 - 1.0, 1.0 - in.pos.y / viewport.size.y * 2.0);
    out.clip = vec4<f32>(ndc, 0.0, 1.0);
    out.color = in.color;
    return out;
}
@fragment
fn fs(in: Vout) -> @location(0) vec4<f32> { return in.color; }
"#;

const SPRITE_SHADER: &str = r#"
struct Viewport { size: vec2<f32> }
@group(0) @binding(0) var<uniform> viewport: Viewport;
@group(0) @binding(1) var tex: texture_2d<f32>;
@group(0) @binding(2) var tex_samp: sampler;
struct Vin {
    @location(0) pos: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) local: vec2<f32>,
    @location(4) extra: vec4<f32>,
}
struct Vout {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) local: vec2<f32>,
    @location(3) extra: vec4<f32>,
}
@vertex
fn vs(in: Vin) -> Vout {
    var out: Vout;
    let ndc = vec2<f32>(in.pos.x / viewport.size.x * 2.0 - 1.0, 1.0 - in.pos.y / viewport.size.y * 2.0);
    out.clip = vec4<f32>(ndc, 0.0, 1.0);
    out.uv = in.uv;
    out.color = in.color;
    out.local = in.local;
    out.extra = in.extra;
    return out;
}
fn cover(local: vec2<f32>, extra: vec4<f32>) -> f32 {
    let radius = extra.x;
    if (radius <= 0.0 || extra.y <= 0.0 || extra.z <= 0.0) { return 1.0; }
    let half = vec2<f32>(extra.y, extra.z) * 0.5;
    let p = abs(local - half) - (half - vec2<f32>(radius));
    let dist = length(max(p, vec2<f32>(0.0))) + min(max(p.x, p.y), 0.0) - radius;
    let fw = max(fwidth(dist), 0.0001);
    return clamp(0.5 - dist / fw, 0.0, 1.0);
}
@fragment
fn fs(in: Vout) -> @location(0) vec4<f32> {
    let sample = textureSample(tex, tex_samp, in.uv);
    let mode = in.extra.w;
    var rgb = sample.rgb;
    var a = sample.a * in.color.a;
    if (mode > 0.5 && mode < 1.5) {
        rgb = in.color.rgb;
        a = sample.r * in.color.a;
    } else if (mode > 3.5) {
        rgb = in.color.rgb;
        a = sample.a * in.color.a;
    }
    a = a * cover(in.local, in.extra);
    return vec4<f32>(rgb, a);
}
"#;

const BLUR_SHADER: &str = r#"
struct Blur { dir: vec2<f32>, sigma: f32, _pad: f32 }
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
    let sigma = max(blur.sigma, 0.5);
    let step = max(sigma / 3.0, 1.0);
    let base = blur.dir * step;
    var sum = vec4<f32>(0.0);
    var norm = 0.0;
    for (var i: i32 = -7; i <= 7; i = i + 1) {
        let x = f32(i) / sigma;
        let w = exp(-0.5 * x * x);
        sum = sum + textureSample(tex, tex_samp, in.uv + base * f32(i)) * w;
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
}

enum Drawn {
    Paths(Vec<u8>),
    Sprite(SpriteDraw),
    Blit(BlitDraw),
}

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
pub(crate) struct SceneGpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    format: wgpu::TextureFormat,
    max_dim: u32,
    uniform: wgpu::Buffer,
    path_layout: wgpu::BindGroupLayout,
    sprite_layout: wgpu::BindGroupLayout,
    blur_layout: wgpu::BindGroupLayout,
    quad_layout: wgpu::BindGroupLayout,
    blur_pipeline: wgpu::RenderPipeline,
    blur_quad: wgpu::Buffer,
    image_sampler: wgpu::Sampler,
    glyph_sampler: wgpu::Sampler,
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
        let path_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("path-bg"),
            entries: &[buffer_entry(0, wgpu::ShaderStages::VERTEX)],
        });
        let sprite_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("sprite-bg"),
            entries: &[
                buffer_entry(0, wgpu::ShaderStages::VERTEX),
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
            device,
            queue,
            format,
            max_dim,
            uniform,
            path_layout,
            sprite_layout,
            blur_layout,
            quad_layout,
            blur_pipeline,
            blur_quad,
            image_sampler,
            glyph_sampler,
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
        let mut viewport = [0u8; 16];
        viewport[0..4].copy_from_slice(&(width.max(1) as f32).to_le_bytes());
        viewport[4..8].copy_from_slice(&(height.max(1) as f32).to_le_bytes());
        self.queue.write_buffer(&self.uniform, 0, &viewport);
        let glyph = self.glyph_sampler.clone();
        let coverage_bind = self.sprite_bind(coverage, &glyph);
        let color_bind = self.sprite_bind(color_atlas, &glyph);
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
        world: Option<&World>,
    ) -> Vec<Drawn> {
        let mut stack = vec![base];
        let mut out = Vec::new();
        let mut index = 0;
        while index < ops.len() {
            let current = *stack.last().unwrap_or(&Affine::IDENTITY);
            match &ops[index] {
                Op::PushTransform(affine) => {
                    stack.push(current.then(*affine));
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
                    let mut outlined = false;
                    if device >= OUTLINE_PX {
                        if let Some(source) = glyphs.source.as_deref() {
                            outlined = self.push_outlines(&mut out, source, current, scale, world);
                        }
                    }
                    if !outlined {
                        out.extend(
                            self.glyph_sprites(glyphs, current, scale, coverage, color_atlas)
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
            push_quad(
                &mut bytes,
                affine,
                scale,
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
                bucket, affine, scale, sprite.x, sprite.y, 0.0, 0.0, sprite.w, sprite.h, sprite.uv,
                color, 0.0, sprite.w, sprite.h, mode,
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
        let cmds = pack_cmds(plan, drawn, width, height);
        let bind = target.bind.clone();
        plan.passes.push(Pass::Geom(GeomPass {
            target,
            scissor,
            clip_offset,
            clip_verts,
            cmds,
        }));
        let (shadow, shadow_color, offset) =
            self.shadow(plan, layer, base, scale, width, height, &bind);
        BlitDraw {
            bind,
            shadow,
            opacity: layer.opacity.clamp(0.0, 1.0),
            shadow_color,
            offset,
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
    ) -> (Option<wgpu::BindGroup>, [f32; 4], [f32; 2]) {
        let Some(Effect::DropShadow {
            offset,
            blur,
            color,
        }) = layer.effect
        else {
            return (None, [0.0; 4], [0.0; 2]);
        };
        let px = base.uniform_scale() * scale;
        let offset_px = [offset[0] * px, offset[1] * px];
        let sigma = (blur * px).max(0.0);
        let tint = color_for_surface(color, self.format);
        if tint[3] <= 0.0 {
            return (None, tint, offset_px);
        }
        if sigma < 0.5 {
            return (Some(color_bind.clone()), tint, offset_px);
        }
        let passes = [
            FilterPass::Gaussian {
                horizontal: true,
                sigma_px: sigma,
            },
            FilterPass::Gaussian {
                horizontal: false,
                sigma_px: sigma,
            },
        ];
        let mut source_view = plan_color_view(plan);
        // The geometry pass just pushed is the layer. Its color view is the blur source.
        if source_view.is_none() {
            return (Some(color_bind.clone()), tint, offset_px);
        }
        let mut source = source_view.take().unwrap();
        let mut result = color_bind.clone();
        for pass in passes {
            let FilterPass::Gaussian {
                horizontal,
                sigma_px,
            } = pass;
            let target = self.alloc(width, height, 1);
            let bind = self.blur_bind(&source, horizontal, sigma_px, width, height);
            let view = target.color.clone();
            plan.passes.push(Pass::Blur(BlurJob {
                view: view.clone(),
                bind,
            }));
            result = target.bind.clone();
            source = view;
        }
        (Some(result), tint, offset_px)
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
        let pipes = if pass.target.samples > 1 {
            self.msaa.as_ref().unwrap_or(&self.plain)
        } else {
            &self.plain
        };
        let color_view = pass.target.msaa.as_ref().unwrap_or(&pass.target.color);
        let resolve = pass.target.msaa.as_ref().map(|_| &pass.target.color);
        let path_bind = self.path_bind();
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

    fn blur_bind(
        &mut self,
        src: &wgpu::TextureView,
        horizontal: bool,
        sigma: f32,
        width: u32,
        height: u32,
    ) -> wgpu::BindGroup {
        let mut data = [0u8; 16];
        let (dx, dy) = if horizontal {
            (1.0 / width.max(1) as f32, 0.0)
        } else {
            (0.0, 1.0 / height.max(1) as f32)
        };
        data[0..4].copy_from_slice(&dx.to_le_bytes());
        data[4..8].copy_from_slice(&dy.to_le_bytes());
        data[8..12].copy_from_slice(&sigma.to_le_bytes());
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("blur-params"),
            size: 16,
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
            push_screen(
                &mut bytes,
                blit.offset,
                width,
                height,
                blit.shadow_color,
                4.0,
            );
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

#[allow(clippy::too_many_arguments)]
fn push_quad(
    out: &mut Vec<u8>,
    affine: Affine,
    scale: f32,
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
) {
    let corners = [
        (x0, y0, uv[0], uv[1]),
        (x1, y0, uv[2], uv[1]),
        (x0, y1, uv[0], uv[3]),
        (x1, y0, uv[2], uv[1]),
        (x0, y1, uv[0], uv[3]),
        (x1, y1, uv[2], uv[3]),
    ];
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
                x,
                y,
                radius,
                dest_w,
                dest_h,
                mode,
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
    push_quad(
        out,
        Affine::translate(offset[0], offset[1]),
        1.0,
        0.0,
        0.0,
        0.0,
        0.0,
        width as f32,
        height as f32,
        [0.0, 0.0, 1.0, 1.0],
        color,
        0.0,
        width as f32,
        height as f32,
        mode,
    );
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
    const ATTRS: [wgpu::VertexAttribute; 5] = wgpu::vertex_attr_array![
        0 => Float32x2, 1 => Float32x2, 2 => Float32x4, 3 => Float32x2, 4 => Float32x4,
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
