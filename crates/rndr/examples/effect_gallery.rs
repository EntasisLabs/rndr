//! Headless pixels for app GPU effects: an animated glow drawn by a custom
//! shader at three phases, and a frosted panel that blurs its backdrop.
//! Writes one PNG; no window.
//!
//! `cargo run -p rndr --example effect_gallery -- OUTPUT.png`
use std::collections::HashMap;

use rndr::wgpu;
use rndr::{
    encode_png, set_parent, App, Axis, Color, EffectContext, EffectSlot, Entity, Exporter,
    GpuEffect, GpuEffects, Gradient, GradientStop, Layout, Radius, Size, Viewport,
};

const W: u32 = 960;
const H: u32 = 240;
const GLOW: u64 = 1;
const FROST: u64 = 2;

const SHADER: &str = r#"
struct U { rect: vec4<f32>, size: vec4<f32>, data: vec4<f32>, mode: vec4<f32> }
@group(0) @binding(0) var<uniform> u: U;
@group(0) @binding(1) var back: texture_2d<f32>;

@vertex
fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let corner = vec2<f32>(f32(i & 1u), f32((i >> 1u) & 1u));
    let ndc = (u.rect.xy + corner * u.rect.zw) / u.size.xy * 2.0 - 1.0;
    return vec4<f32>(ndc.x, -ndc.y, 0.0, 1.0);
}

fn rounded(p: vec2<f32>, radius: f32) -> f32 {
    let half = u.rect.zw * 0.5;
    let q = abs(p - u.rect.xy - half) - half + radius;
    let d = length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - radius;
    return clamp(0.5 - d, 0.0, 1.0);
}

@fragment
fn fs(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    let scale = u.mode.y;
    if (u.mode.x > 0.5) {
        // Frosted glass: a 13x13 box blur of the backdrop, lightened.
        var sum = vec3<f32>(0.0);
        let dims = vec2<i32>(textureDimensions(back));
        for (var y = -6; y <= 6; y++) {
            for (var x = -6; x <= 6; x++) {
                let at = clamp(vec2<i32>(p.xy) + vec2<i32>(x, y) * 2, vec2<i32>(0), dims - 1);
                sum += textureLoad(back, at, 0).rgb;
            }
        }
        let frosted = mix(sum / 169.0, vec3<f32>(1.0), 0.35);
        let a = rounded(p.xy, 20.0 * scale);
        return vec4<f32>(frosted * a, a);
    }
    // Glow: a soft ring whose radius and brightness pulse with time + phase.
    let t = u.data.w + u.mode.z;
    let centre = u.rect.xy + u.rect.zw * 0.5;
    let r = length(p.xy - centre) / (min(u.rect.z, u.rect.w) * 0.5);
    let ring = 0.55 + 0.15 * sin(t * 6.2831);
    let glow = exp(-pow((r - ring) * 5.0, 2.0)) + 0.6 * exp(-r * r * 6.0);
    let a = clamp(glow, 0.0, 1.0) * (0.7 + 0.3 * cos(t * 6.2831));
    return vec4<f32>(u.data.rgb * a, a);
}
"#;

/// One effect type; `frost` picks the backdrop blur, otherwise a glow.
#[derive(Default)]
struct Shader {
    frost: bool,
    pipes: HashMap<(wgpu::TextureFormat, u32), (wgpu::RenderPipeline, wgpu::BindGroupLayout)>,
}

impl GpuEffect for Shader {
    fn draw(&mut self, cx: &mut EffectContext<'_>) {
        let site = cx.site;
        if self.frost && cx.backdrop().is_none() {
            return;
        }
        let device = cx.device;
        let (pipe, layout) = self
            .pipes
            .entry((site.format, site.samples))
            .or_insert_with(|| pipeline(device, site.format, site.samples))
            .clone();
        let mode = if self.frost { 1.0 } else { 0.0 };
        let values: [f32; 16] = [
            site.rect[0],
            site.rect[1],
            site.rect[2],
            site.rect[3],
            site.size[0] as f32,
            site.size[1] as f32,
            0.0,
            0.0,
            site.data[0],
            site.data[1],
            site.data[2],
            site.data[3],
            mode,
            site.scale,
            site.time,
            0.0,
        ];
        let bytes: Vec<u8> = values.iter().flat_map(|v| v.to_le_bytes()).collect();
        let uniform = cx.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("effect-uniform"),
            size: bytes.len() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        cx.write_buffer(&uniform, 0, &bytes);
        let fallback;
        let back = match cx.backdrop() {
            Some(view) => view.clone(),
            None => {
                fallback = cx.device.create_texture(&wgpu::TextureDescriptor {
                    label: None,
                    size: wgpu::Extent3d {
                        width: 1,
                        height: 1,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                });
                fallback.create_view(&wgpu::TextureViewDescriptor::default())
            }
        };
        let bind = cx.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&back),
                },
            ],
        });
        let mut pass = cx.begin_pass();
        pass.set_pipeline(&pipe);
        pass.set_bind_group(0, &bind, &[]);
        pass.draw(0..4, 0..1);
    }
}

fn pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    samples: u32,
) -> (wgpu::RenderPipeline, wgpu::BindGroupLayout) {
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("effect-gallery"),
        source: wgpu::ShaderSource::Wgsl(SHADER.into()),
    });
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
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
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
        ],
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[Some(&layout)],
        immediate_size: 0,
    });
    let pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("effect-gallery"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &module,
            entry_point: Some("vs"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: &module,
            entry_point: Some("fs"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleStrip,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState {
            count: samples,
            ..Default::default()
        },
        multiview_mask: None,
        cache: None,
    });
    (pipe, layout)
}

fn sized(w: f32, h: f32) -> Layout {
    Layout {
        width: Size::Px(w),
        height: Size::Px(h),
        ..Layout::row()
    }
}

fn child(app: &mut App, parent: Entity, bundle: impl rndr::Bundle) -> Entity {
    let entity = app.world_mut().spawn(bundle);
    set_parent(app.world_mut(), entity, parent);
    entity
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = std::env::args_os()
        .nth(1)
        .ok_or("usage: effect_gallery OUTPUT.png")?;
    let mut app = App::new();
    app.world_mut()
        .insert_resource(Viewport::new(W as f32, H as f32));
    let mut effects = GpuEffects::new();
    effects.insert(GLOW, Shader::default());
    effects.insert(
        FROST,
        Shader {
            frost: true,
            ..Shader::default()
        },
    );
    app.world_mut().insert_resource(effects);

    let row = app.world_mut().spawn((
        Layout {
            axis: Axis::Row,
            gap: 32.0,
            padding: 40.0,
            width: Size::Px(W as f32),
            height: Size::Px(H as f32),
            ..Layout::row()
        },
        Color::rgb(0.08, 0.09, 0.12),
    ));
    // The same glow effect at three phases of its cycle.
    for (index, tint) in [[0.3, 0.7, 1.0], [1.0, 0.45, 0.8], [0.4, 1.0, 0.6]]
        .into_iter()
        .enumerate()
    {
        let tile = child(
            &mut app,
            row,
            (
                sized(160.0, 160.0),
                Color::rgb(0.12, 0.13, 0.18),
                Radius(20.0),
            ),
        );
        child(
            &mut app,
            tile,
            (
                sized(160.0, 160.0),
                EffectSlot::new(GLOW)
                    .with_data([tint[0], tint[1], tint[2], index as f32 / 3.0])
                    .animated(),
            ),
        );
    }
    // A frosted panel overlapping coloured stripes: the negative gap pulls it
    // left over the stripes, which it blurs through its backdrop.
    let stage = child(
        &mut app,
        row,
        (
            Layout {
                gap: -200.0,
                ..sized(280.0, 160.0)
            },
            Gradient::Linear {
                angle: 0.0,
                stops: vec![
                    GradientStop::new(0.0, Color::rgb(0.95, 0.35, 0.35)),
                    GradientStop::new(0.5, Color::rgb(0.98, 0.8, 0.3)),
                    GradientStop::new(1.0, Color::rgb(0.25, 0.55, 0.95)),
                ],
            },
            Color::rgb(1.0, 1.0, 1.0),
            Radius(20.0),
        ),
    );
    let stripes = child(
        &mut app,
        stage,
        (Layout {
            gap: 16.0,
            padding: 20.0,
            ..sized(280.0, 160.0)
        },),
    );
    for shade in [0.1, 0.9, 0.1, 0.9, 0.1, 0.9] {
        child(
            &mut app,
            stripes,
            (
                sized(20.0, 120.0),
                Color::rgb(shade, shade, shade),
                Radius(6.0),
            ),
        );
    }
    child(
        &mut app,
        stage,
        (sized(180.0, 160.0), EffectSlot::new(FROST).with_backdrop()),
    );

    let frame = rndr::frame(&mut app);
    let image = Exporter::new()?.render_frame(Some(app.world()), &frame, W, H, 1.0)?;
    std::fs::write(out, encode_png(&image)?)?;
    Ok(())
}
