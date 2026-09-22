use std::sync::Arc;

use rndr_core::{
    children_of, roots, App, Caret, Clip, Color, Focus, Input, Label, Parent, Rect, Viewport,
};
use winit::dpi::PhysicalSize;
use winit::window::Window;

use crate::font::{self, ATLAS_H, ATLAS_W};

const QUAD: [[f32; 2]; 4] = [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]];
const INSTANCE_FLOATS: usize = 16;
const INSTANCE_STRIDE: u64 = (INSTANCE_FLOATS * 4) as u64;
const INITIAL_INSTANCES: u64 = 2048;

const SHADER: &str = r#"
struct VsIn {
    @location(0) quad: vec2<f32>,
    @location(1) rect: vec4<f32>,
    @location(2) color: vec4<f32>,
    @location(3) uv_rect: vec4<f32>,
    @location(4) clip: vec4<f32>,
}

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) clip: vec4<f32>,
}

struct ViewportUniform {
    size: vec2<f32>,
    _pad: vec2<f32>,
}

@group(0) @binding(0) var<uniform> viewport: ViewportUniform;
@group(0) @binding(1) var atlas: texture_2d<f32>;
@group(0) @binding(2) var atlas_samp: sampler;

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
    return out;
}

@fragment
fn fs(in: VsOut) -> @location(0) vec4<f32> {
    let px = in.pos.xy;
    let clip = in.clip;
    if px.x < clip.x || px.y < clip.y || px.x >= clip.x + clip.z || px.y >= clip.y + clip.w {
        discard;
    }
    let coverage = textureSample(atlas, atlas_samp, in.uv).r;
    return vec4<f32>(in.color.rgb, in.color.a * coverage);
}
"#;

pub struct Gpu {
    window: Arc<Window>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    uniform: wgpu::Buffer,
    quad: wgpu::Buffer,
    instances: wgpu::Buffer,
    instance_capacity: u64,
}

impl Gpu {
    pub async fn new(display: winit::event_loop::OwnedDisplayHandle, window: Arc<Window>) -> Self {
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
        let mut config = surface
            .get_default_config(&adapter, width, height)
            .expect("surface config");
        config.present_mode = wgpu::PresentMode::AutoVsync;
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

        let atlas_pixels = font::atlas_r8();
        let atlas = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("atlas"),
            size: wgpu::Extent3d {
                width: ATLAS_W,
                height: ATLAS_H,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &atlas,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &atlas_pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(ATLAS_W),
                rows_per_image: Some(ATLAS_H),
            },
            wgpu::Extent3d {
                width: ATLAS_W,
                height: ATLAS_H,
                depth_or_array_layers: 1,
            },
        );
        let atlas_view = atlas.create_view(&wgpu::TextureViewDescriptor::default());
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
            ],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rndr-bg"),
            layout: &bind_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&atlas_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("rndr-pl"),
            bind_group_layouts: &[Some(&bind_layout)],
            immediate_size: 0,
        });

        const QUAD_ATTRS: [wgpu::VertexAttribute; 1] = wgpu::vertex_attr_array![0 => Float32x2];
        const INSTANCE_ATTRS: [wgpu::VertexAttribute; 4] = wgpu::vertex_attr_array![
            1 => Float32x4,
            2 => Float32x4,
            3 => Float32x4,
            4 => Float32x4,
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

        Self {
            window,
            device,
            queue,
            surface,
            config,
            pipeline,
            bind_group,
            uniform,
            quad,
            instances,
            instance_capacity: INITIAL_INSTANCES,
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

    pub fn draw(&mut self, app: &mut App) {
        let size = self.window.inner_size();
        if let Some(viewport) = app.world_mut().resource_mut::<Viewport>() {
            viewport.width = size.width.max(1) as f32;
            viewport.height = size.height.max(1) as f32;
        }
        app.update();

        let instances = collect_instances(app);
        app.render();

        let needed = instances.len() as u64 / INSTANCE_FLOATS as u64;
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
        uniform[0..4].copy_from_slice(&(size.width.max(1) as f32).to_le_bytes());
        uniform[4..8].copy_from_slice(&(size.height.max(1) as f32).to_le_bytes());
        self.queue.write_buffer(&self.uniform, 0, &uniform);

        let mut bytes = Vec::with_capacity(instances.len() * 4);
        for value in &instances {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        if !bytes.is_empty() {
            self.queue.write_buffer(&self.instances, 0, &bytes);
        }

        let texture = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t)
            | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.surface.configure(&self.device, &self.config);
                return;
            }
            wgpu::CurrentSurfaceTexture::Timeout
            | wgpu::CurrentSurfaceTexture::Occluded
            | wgpu::CurrentSurfaceTexture::Validation => return,
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
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.06,
                            g: 0.07,
                            b: 0.09,
                            a: 1.0,
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

        if let Some(input) = app.world_mut().resource_mut::<Input>() {
            input.pressed = false;
            input.scroll = [0.0, 0.0];
            input.key = None;
            input.chars.clear();
        }
    }
}

fn collect_instances(app: &App) -> Vec<f32> {
    let world = app.world();
    let mut out = Vec::new();
    for root in roots::<Rect>(world) {
        walk(world, root, &mut |rect, color, clip, label, caret| {
            push_instance(&mut out, rect, color, font::solid_uv(), clip);
            if let Some(label) = label {
                push_label(&mut out, rect, label, clip);
            }
            if let Some(caret) = caret {
                push_caret(&mut out, rect, label, caret, clip);
            }
        });
    }
    out
}

fn walk(
    world: &rndr_core::World,
    entity: rndr_core::Entity,
    visit: &mut impl FnMut(Rect, Color, Clip, Option<&Label>, Option<&Caret>),
) {
    if !world.is_alive(entity) {
        return;
    }
    if let (Some(rect), Some(color)) = (world.get::<Rect>(entity), world.get::<Color>(entity)) {
        let clip = world
            .get::<Clip>(entity)
            .copied()
            .unwrap_or_else(|| Clip::from_rect(*rect));
        let focused = world.resource::<Focus>().and_then(|f| f.entity);
        let highlight = focused == Some(entity)
            || world
                .get::<Parent>(entity)
                .is_some_and(|parent| focused == Some(parent.entity))
            || world.has::<Caret>(entity);
        let color = if highlight { lighten(*color) } else { *color };
        visit(
            *rect,
            color,
            clip,
            world.get::<Label>(entity),
            world.get::<Caret>(entity),
        );
    }
    for child in children_of(world, entity) {
        walk(world, child, visit);
    }
}

fn lighten(color: Color) -> Color {
    Color::rgba(
        color.r + (1.0 - color.r) * 0.25,
        color.g + (1.0 - color.g) * 0.25,
        color.b + (1.0 - color.b) * 0.25,
        color.a,
    )
}

fn push_instance(out: &mut Vec<f32>, rect: Rect, color: Color, uv: [f32; 4], clip: Clip) {
    out.extend_from_slice(&[
        rect.x, rect.y, rect.w, rect.h, color.r, color.g, color.b, color.a, uv[0], uv[1], uv[2],
        uv[3], clip.x, clip.y, clip.w, clip.h,
    ]);
}

fn push_label(out: &mut Vec<f32>, rect: Rect, label: &Label, clip: Clip) {
    let scale = 2.0;
    let glyph_w = font::GLYPH_W as f32 * scale;
    let glyph_h = font::GLYPH_H as f32 * scale;
    let mut x = rect.x + 12.0;
    let mut y = rect.y + 8.0;
    let color = Color::rgb(0.94, 0.95, 0.97);
    for ch in label.text.bytes() {
        if ch == b'\n' {
            x = rect.x + 12.0;
            y += glyph_h;
            continue;
        }
        if ch == b' ' {
            x += glyph_w;
            continue;
        }
        let uv = font::glyph_uv(ch);
        push_instance(
            out,
            Rect {
                x,
                y,
                w: glyph_w,
                h: glyph_h,
            },
            color,
            uv,
            clip,
        );
        x += glyph_w;
    }
}

fn push_caret(out: &mut Vec<f32>, rect: Rect, label: Option<&Label>, caret: &Caret, clip: Clip) {
    let scale = 2.0;
    let glyph_w = font::GLYPH_W as f32 * scale;
    let glyph_h = font::GLYPH_H as f32 * scale;
    let text = label.map(|label| label.text.as_str()).unwrap_or("");
    let (col, line) = caret_col_line(text, caret.index);
    let x = rect.x + 12.0 + col as f32 * glyph_w;
    let y = rect.y + 8.0 + line as f32 * glyph_h;
    push_instance(
        out,
        Rect {
            x,
            y,
            w: 2.0,
            h: glyph_h,
        },
        Color::rgb(0.94, 0.95, 0.97),
        font::solid_uv(),
        clip,
    );
}

fn caret_col_line(text: &str, index: usize) -> (usize, usize) {
    let index = index.min(text.len());
    let mut line = 0usize;
    let mut col = 0usize;
    for (i, ch) in text.bytes().enumerate() {
        if i == index {
            break;
        }
        if ch == b'\n' {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
    }
    (col, line)
}
