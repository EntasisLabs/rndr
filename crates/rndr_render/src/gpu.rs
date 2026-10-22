use std::sync::Arc;

use rndr_paint::Quad;
use winit::dpi::PhysicalSize;
use winit::window::Window;

use crate::color::{clear_color, color_for_surface};

const QUAD: [[f32; 2]; 4] = [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]];
const INSTANCE_FLOATS: usize = 20;
const INSTANCE_STRIDE: u64 = (INSTANCE_FLOATS * 4) as u64;
const INITIAL_INSTANCES: u64 = 2048;

const SHADER: &str = r#"
struct VsIn {
    @location(0) quad: vec2<f32>,
    @location(1) rect: vec4<f32>,
    @location(2) color: vec4<f32>,
    @location(3) uv_rect: vec4<f32>,
    @location(4) clip: vec4<f32>,
    @location(5) radius: vec4<f32>,
}

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) clip: vec4<f32>,
    @location(3) rect: vec4<f32>,
    @location(4) radius: f32,
    @location(5) glyph_atlas: f32,
}

struct ViewportUniform {
    size: vec2<f32>,
    _pad: vec2<f32>,
}

@group(0) @binding(0) var<uniform> viewport: ViewportUniform;
@group(0) @binding(1) var atlas: texture_2d<f32>;
@group(0) @binding(2) var atlas_samp: sampler;
@group(0) @binding(3) var color_atlas: texture_2d<f32>;
@group(0) @binding(4) var color_samp: sampler;

@vertex
fn vs(in: VsIn) -> VsOut {
    let px = in.rect.xy + in.quad * in.rect.zw;
    let ndc = vec2<f32>(
        px.x / viewport.size.x * 2.0 - 1.0,
        1.0 - px.y / viewport.size.y * 2.0,
    );
    var out: VsOut;
    out.pos = vec4<f32>(ndc, 0.0, 1.0);
    out.color = in.color;
    out.uv = mix(in.uv_rect.xy, in.uv_rect.zw, in.quad);
    out.clip = in.clip;
    out.rect = in.rect;
    out.radius = in.radius.x;
    out.glyph_atlas = in.radius.y;
    return out;
}

@fragment
fn fs(in: VsOut) -> @location(0) vec4<f32> {
    let px = in.pos.xy;
    let clip = in.clip;
    if px.x < clip.x || px.y < clip.y || px.x >= clip.x + clip.z || px.y >= clip.y + clip.w {
        discard;
    }
    // RGB is already in the surface working space: linear on an sRGB target,
    // authored sRGB on an 8-bit Unorm target. Coverage alpha is the mask.
    // Color glyphs (emoji) sample the sRGB color atlas; `glyph_atlas` > 0.5
    // selects that path. The instance flag lives in `radius.y`.
    let coverage = textureSample(atlas, atlas_samp, in.uv).r;
    let sprite = textureSample(color_atlas, color_samp, in.uv);
    let color_glyph = in.glyph_atlas > 0.5;
    let rgb = select(in.color.rgb, sprite.rgb, color_glyph);
    let alpha = select(in.color.a * coverage, sprite.a * in.color.a, color_glyph);
    let corner = round_rect(px, in.rect, in.radius);
    return vec4<f32>(rgb, alpha * corner);
}

fn round_rect(px: vec2<f32>, rect: vec4<f32>, radius: f32) -> f32 {
    if radius <= 0.0 {
        return 1.0;
    }
    let half = rect.zw * 0.5;
    let q = abs(px - (rect.xy + half)) - half + vec2<f32>(radius);
    let dist = length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - radius;
    return clamp(0.5 - dist, 0.0, 1.0);
}
"#;

pub struct Gpu {
    window: Arc<Window>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    /// When set, the pass clears to transparent instead of the opaque canvas.
    clear_transparent: bool,
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    uniform: wgpu::Buffer,
    /// Kept so a later atlas upload can rebuild the bind group. The builtin
    /// engine never uploads, so these handles are absent without `parley`.
    #[cfg(feature = "parley")]
    bind_layout: wgpu::BindGroupLayout,
    #[cfg(feature = "parley")]
    sampler: wgpu::Sampler,
    #[cfg(feature = "parley")]
    coverage: wgpu::Texture,
    #[cfg(feature = "parley")]
    coverage_view: wgpu::TextureView,
    #[cfg(feature = "parley")]
    coverage_size: (u32, u32),
    #[cfg(feature = "parley")]
    color_tex: wgpu::Texture,
    #[cfg(feature = "parley")]
    color_view: wgpu::TextureView,
    #[cfg(feature = "parley")]
    color_size: (u32, u32),
    #[cfg(feature = "parley")]
    color_format: wgpu::TextureFormat,
    quad: wgpu::Buffer,
    instances: wgpu::Buffer,
    instance_capacity: u64,
    staging: Vec<u8>,
}

/// Bytes of one atlas image handed to [`Gpu::new`].
pub struct AtlasBytes<'a> {
    pub width: u32,
    pub height: u32,
    pub pixels: &'a [u8],
}

impl Gpu {
    pub async fn new(
        display: winit::event_loop::OwnedDisplayHandle,
        window: Arc<Window>,
        coverage_atlas: AtlasBytes<'_>,
        color_atlas: AtlasBytes<'_>,
        transparent: bool,
    ) -> Self {
        let atlas_w = coverage_atlas.width;
        let atlas_h = coverage_atlas.height;
        let atlas_pixels = coverage_atlas.pixels;
        let color_w = color_atlas.width;
        let color_h = color_atlas.height;
        let color_pixels = color_atlas.pixels;
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_with_display_handle(
            Box::new(display),
        ));
        let surface = instance
            .create_surface(window.clone())
            .expect("create surface");
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: Some(&surface),
                ..Default::default()
            })
            .await
            .expect("adapter");
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await
            .expect("device");

        let size = window.inner_size();
        let width = size.width.max(1);
        let height = size.height.max(1);
        // Keep the adapter's format. `color_for_surface` decodes authored sRGB
        // when this format encodes on store, so the theme matches either way.
        let mut config = surface
            .get_default_config(&adapter, width, height)
            .expect("surface config");
        config.present_mode = wgpu::PresentMode::AutoVsync;
        let clear_transparent = if transparent {
            let caps = surface.get_capabilities(&adapter);
            let mode = [
                wgpu::CompositeAlphaMode::PostMultiplied,
                wgpu::CompositeAlphaMode::PreMultiplied,
            ]
            .into_iter()
            .find(|mode| caps.alpha_modes.contains(mode));
            if let Some(mode) = mode {
                config.alpha_mode = mode;
                true
            } else {
                false
            }
        } else {
            false
        };
        surface.configure(&device, &config);

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("rndr"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });

        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("viewport"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let coverage = device.create_texture(&texture_desc(
            "atlas",
            atlas_w,
            atlas_h,
            wgpu::TextureFormat::R8Unorm,
        ));
        write_full(&queue, &coverage, atlas_w, atlas_h, 1, atlas_pixels);
        let coverage_view = coverage.create_view(&wgpu::TextureViewDescriptor::default());
        let color_format = if config.format.is_srgb() {
            wgpu::TextureFormat::Rgba8UnormSrgb
        } else {
            wgpu::TextureFormat::Rgba8Unorm
        };
        let color_w = color_w.max(1);
        let color_h = color_h.max(1);
        let color_tex =
            device.create_texture(&texture_desc("color-atlas", color_w, color_h, color_format));
        write_full(&queue, &color_tex, color_w, color_h, 4, color_pixels);
        let color_view = color_tex.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let bind_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rndr-bg"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
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
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let bind_group = bind_group_of(
            &device,
            &bind_layout,
            &uniform,
            &coverage_view,
            &sampler,
            &color_view,
        );

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("rndr-pl"),
            bind_group_layouts: &[Some(&bind_layout)],
            immediate_size: 0,
        });

        const QUAD_ATTRS: [wgpu::VertexAttribute; 1] = wgpu::vertex_attr_array![0 => Float32x2];
        const INSTANCE_ATTRS: [wgpu::VertexAttribute; 5] = wgpu::vertex_attr_array![
            1 => Float32x4,
            2 => Float32x4,
            3 => Float32x4,
            4 => Float32x4,
            5 => Float32x4,
        ];
        let quad_layout = wgpu::VertexBufferLayout {
            array_stride: 8,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &QUAD_ATTRS,
        };
        let instance_layout = wgpu::VertexBufferLayout {
            array_stride: INSTANCE_STRIDE,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &INSTANCE_ATTRS,
        };

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("rndr-quads"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[Some(quad_layout), Some(instance_layout)],
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
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    // Straight alpha over. An sRGB target blends in linear light
                    // after `color_for_surface` decodes the authored channels.
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });

        let mut quad_bytes = Vec::with_capacity(32);
        for [x, y] in QUAD {
            quad_bytes.extend_from_slice(&x.to_le_bytes());
            quad_bytes.extend_from_slice(&y.to_le_bytes());
        }
        let quad = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("quad"),
            size: quad_bytes.len() as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&quad, 0, &quad_bytes);

        let instances = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("instances"),
            size: INITIAL_INSTANCES * INSTANCE_STRIDE,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        #[cfg(not(feature = "parley"))]
        let _keep_alive = (
            bind_layout,
            sampler,
            coverage,
            coverage_view,
            color_tex,
            color_view,
            color_format,
        );
        Self {
            window,
            device,
            queue,
            surface,
            config,
            pipeline,
            bind_group,
            uniform,
            #[cfg(feature = "parley")]
            bind_layout,
            #[cfg(feature = "parley")]
            sampler,
            #[cfg(feature = "parley")]
            coverage,
            #[cfg(feature = "parley")]
            coverage_view,
            #[cfg(feature = "parley")]
            coverage_size: (atlas_w, atlas_h),
            #[cfg(feature = "parley")]
            color_tex,
            #[cfg(feature = "parley")]
            color_view,
            #[cfg(feature = "parley")]
            color_size: (color_w, color_h),
            #[cfg(feature = "parley")]
            color_format,
            quad,
            instances,
            instance_capacity: INITIAL_INSTANCES,
            staging: Vec::new(),
            clear_transparent,
        }
    }

    /// Replace atlas texels. A size change recreates the texture and rebinds it.
    /// Otherwise only dirty regions are copied, each padded to the 256-byte row rule.
    #[cfg(feature = "parley")]
    pub fn upload_atlas(&mut self, kind: AtlasKind, upload: &AtlasUpload) {
        let bpp = upload.bpp.max(1);
        let current = match kind {
            AtlasKind::Coverage => self.coverage_size,
            AtlasKind::Color => self.color_size,
        };
        let width = upload.width.max(1);
        let height = upload.height.max(1);
        if width != current.0 || height != current.1 {
            let format = match kind {
                AtlasKind::Coverage => wgpu::TextureFormat::R8Unorm,
                AtlasKind::Color => self.color_format,
            };
            let texture = self
                .device
                .create_texture(&texture_desc("atlas", width, height, format));
            write_full(&self.queue, &texture, width, height, bpp, &upload.pixels);
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            match kind {
                AtlasKind::Coverage => {
                    self.coverage = texture;
                    self.coverage_view = view;
                    self.coverage_size = (width, height);
                }
                AtlasKind::Color => {
                    self.color_tex = texture;
                    self.color_view = view;
                    self.color_size = (width, height);
                }
            }
            self.bind_group = bind_group_of(
                &self.device,
                &self.bind_layout,
                &self.uniform,
                &self.coverage_view,
                &self.sampler,
                &self.color_view,
            );
            return;
        }
        let texture = match kind {
            AtlasKind::Coverage => &self.coverage,
            AtlasKind::Color => &self.color_tex,
        };
        if upload.full {
            write_full(&self.queue, texture, width, height, bpp, &upload.pixels);
            return;
        }
        for region in &upload.regions {
            write_region(
                &self.queue,
                texture,
                TexelRect {
                    x: region.x,
                    y: region.y,
                    width: region.width,
                    height: region.height,
                    bpp,
                    pixels: &region.bytes,
                },
            );
        }
    }

    pub fn window(&self) -> &Window {
        &self.window
    }

    pub fn resize(&mut self, size: PhysicalSize<u32>) {
        if size.width == 0 || size.height == 0 {
            return;
        }
        self.config.width = size.width;
        self.config.height = size.height;
        self.surface.configure(&self.device, &self.config);
    }

    /// Upload quads in points and present them at a physical pixel size.
    /// Returns false when the surface was not presented, so the caller can keep
    /// this frame's input edges for the next redraw.
    pub fn present(&mut self, quads: &[Quad], width: u32, height: u32, scale: f32) -> bool {
        encode_instances(quads, scale, self.config.format, &mut self.staging);
        let needed = self.staging.len() as u64 / INSTANCE_STRIDE;
        if needed > self.instance_capacity {
            self.instance_capacity = needed.max(self.instance_capacity * 2);
            self.instances = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("instances"),
                size: self.instance_capacity * INSTANCE_STRIDE,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }

        let mut uniform = [0u8; 16];
        uniform[0..4].copy_from_slice(&(width as f32).to_le_bytes());
        uniform[4..8].copy_from_slice(&(height as f32).to_le_bytes());
        self.queue.write_buffer(&self.uniform, 0, &uniform);

        if !self.staging.is_empty() {
            self.queue.write_buffer(&self.instances, 0, &self.staging);
        }

        let texture = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t)
            | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.surface.configure(&self.device, &self.config);
                return false;
            }
            wgpu::CurrentSurfaceTexture::Timeout
            | wgpu::CurrentSurfaceTexture::Occluded
            | wgpu::CurrentSurfaceTexture::Validation => return false,
        };
        let view = texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("rndr-frame"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("rndr-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(if self.clear_transparent {
                            wgpu::Color::TRANSPARENT
                        } else {
                            clear_color(self.config.format)
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            if needed > 0 {
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(0, &self.bind_group, &[]);
                pass.set_vertex_buffer(0, self.quad.slice(..));
                pass.set_vertex_buffer(1, self.instances.slice(..));
                pass.draw(0..4, 0..needed as u32);
            }
        }
        self.queue.submit(Some(encoder.finish()));
        self.queue.present(texture);
        true
    }
}

/// Which glyph atlas an [`AtlasUpload`] writes.
#[cfg(feature = "parley")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AtlasKind {
    Coverage,
    Color,
}

/// Dirty or full image for one atlas. `regions` is empty when `full` is set.
#[cfg(feature = "parley")]
pub struct AtlasUpload {
    pub width: u32,
    pub height: u32,
    pub bpp: usize,
    pub full: bool,
    pub pixels: Vec<u8>,
    pub regions: Vec<AtlasRegion>,
}

#[cfg(feature = "parley")]
pub struct AtlasRegion {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub bytes: Vec<u8>,
}

fn texture_desc(
    label: &str,
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
) -> wgpu::TextureDescriptor<'_> {
    wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: width.max(1),
            height: height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    }
}

fn bind_group_of(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    uniform: &wgpu::Buffer,
    coverage: &wgpu::TextureView,
    sampler: &wgpu::Sampler,
    color: &wgpu::TextureView,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("rndr-bg"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(coverage),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::TextureView(color),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    })
}

const ROW_ALIGN: usize = 256;

struct TexelRect<'a> {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    bpp: usize,
    pixels: &'a [u8],
}

fn write_full(
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    width: u32,
    height: u32,
    bpp: usize,
    pixels: &[u8],
) {
    if width == 0 || height == 0 || pixels.is_empty() {
        return;
    }
    write_region(
        queue,
        texture,
        TexelRect {
            x: 0,
            y: 0,
            width,
            height,
            bpp,
            pixels,
        },
    );
}

fn write_region(queue: &wgpu::Queue, texture: &wgpu::Texture, rect: TexelRect<'_>) {
    let TexelRect {
        x,
        y,
        width,
        height,
        bpp,
        pixels,
    } = rect;
    if width == 0 || height == 0 || bpp == 0 {
        return;
    }
    let row = width as usize * bpp;
    let aligned = row.div_ceil(ROW_ALIGN) * ROW_ALIGN;
    let mut padded = vec![0u8; aligned * height as usize];
    for row_index in 0..height as usize {
        let src = row_index * row;
        let dst = row_index * aligned;
        let available = pixels.len().saturating_sub(src);
        let n = row.min(available);
        if n > 0 {
            padded[dst..dst + n].copy_from_slice(&pixels[src..src + n]);
        }
    }
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d { x, y, z: 0 },
            aspect: wgpu::TextureAspect::All,
        },
        &padded,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(aligned as u32),
            rows_per_image: Some(height),
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
}

/// Layout quads are points. The uniform and the fragment clip are physical pixels.
/// Every quad color — solid fills, borders, glyph tints, carets, selections, and
/// marks — is authored sRGB and is decoded here when `format` is sRGB.
fn encode_instances(quads: &[Quad], scale: f32, format: wgpu::TextureFormat, out: &mut Vec<u8>) {
    out.clear();
    out.reserve(quads.len() * INSTANCE_FLOATS * 4);
    for quad in quads {
        let [r, g, b, a] = color_for_surface(quad.color, format);
        for value in [
            quad.rect.x * scale,
            quad.rect.y * scale,
            quad.rect.w * scale,
            quad.rect.h * scale,
            r,
            g,
            b,
            a,
            quad.uv[0],
            quad.uv[1],
            quad.uv[2],
            quad.uv[3],
            quad.clip.x * scale,
            quad.clip.y * scale,
            quad.clip.w * scale,
            quad.clip.h * scale,
            quad.radius * scale,
            quad.atlas as f32,
            0.0,
            0.0,
        ] {
            out.extend_from_slice(&value.to_le_bytes());
        }
    }
}

#[cfg(test)]
mod tests {
    use rndr_core::{Clip, Color, Rect, Theme};
    use rndr_paint::Quad;

    use super::encode_instances;
    use crate::color::color_for_surface;

    fn quad(color: Color, w: f32) -> Quad {
        Quad {
            rect: Rect {
                x: 1.0,
                y: 2.0,
                w,
                h: 4.0,
            },
            color,
            uv: [0.1, 0.2, 0.3, 0.4],
            clip: Clip {
                x: 0.0,
                y: 0.0,
                w: 100.0,
                h: 80.0,
            },
            radius: 0.0,
            atlas: 0,
        }
    }

    fn read_f32(bytes: &[u8], float_index: usize) -> f32 {
        let start = float_index * 4;
        f32::from_le_bytes(bytes[start..start + 4].try_into().unwrap())
    }

    #[test]
    fn upload_encodes_fill_border_and_glyph_tint_for_both_formats() {
        let theme = Theme::default();
        let formats = [
            wgpu::TextureFormat::Bgra8UnormSrgb,
            wgpu::TextureFormat::Rgba8UnormSrgb,
            wgpu::TextureFormat::Bgra8Unorm,
            wgpu::TextureFormat::Rgba8Unorm,
        ];
        for format in formats {
            let mut bytes = Vec::new();
            // Canvas fill, a one-point strip (a border), and the glyph tint.
            encode_instances(
                &[
                    quad(theme.canvas, 40.0),
                    quad(theme.seam, 1.0),
                    quad(theme.text, 8.0),
                ],
                2.0,
                format,
                &mut bytes,
            );
            for (index, color) in [theme.canvas, theme.seam, theme.text]
                .into_iter()
                .enumerate()
            {
                let base = index * super::INSTANCE_FLOATS;
                let expected = color_for_surface(color, format);
                assert_eq!(read_f32(&bytes, base), 2.0, "scale applies to x");
                assert_eq!(
                    [
                        read_f32(&bytes, base + 4),
                        read_f32(&bytes, base + 5),
                        read_f32(&bytes, base + 6),
                        read_f32(&bytes, base + 7),
                    ],
                    expected,
                    "quad {index} on {format:?}"
                );
            }
        }
    }
}
