//! Pixels for app GPU effects drawn at `EffectSlot`s.

use std::collections::HashMap;

use rndr_core::{set_parent, App, Color, EffectSlot, Entity, Layout, Opacity, Size, Viewport};
use rndr_render::{wgpu, EffectContext, ExportError, Exporter, GpuEffect, GpuEffects, RgbaImage};

const SHADER: &str = r#"
struct U { rect: vec4<f32>, size: vec4<f32>, color: vec4<f32>, mode: vec4<f32> }
@group(0) @binding(0) var<uniform> u: U;
@group(0) @binding(1) var back: texture_2d<f32>;

@vertex
fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let corner = vec2<f32>(f32(i & 1u), f32((i >> 1u) & 1u));
    let px = u.rect.xy + corner * u.rect.zw;
    let ndc = px / u.size.xy * 2.0 - 1.0;
    return vec4<f32>(ndc.x, -ndc.y, 0.0, 1.0);
}

@fragment
fn fs(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    if (u.mode.x > 0.5) {
        let under = textureLoad(back, vec2<i32>(p.xy), 0);
        return vec4<f32>(1.0 - under.rgb, 1.0);
    }
    return u.color;
}
"#;

/// Fills its slot with `data` as a colour, or inverts the backdrop.
#[derive(Default)]
struct Fill {
    pipes: HashMap<(wgpu::TextureFormat, u32), (wgpu::RenderPipeline, wgpu::BindGroupLayout)>,
}

impl GpuEffect for Fill {
    fn draw(&mut self, cx: &mut EffectContext<'_>) {
        let site = cx.site;
        let device = cx.device;
        let (pipe, layout) = self
            .pipes
            .entry((site.format, site.samples))
            .or_insert_with(|| pipeline(device, site.format, site.samples))
            .clone();
        let mode = if cx.backdrop().is_some() { 1.0 } else { 0.0 };
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
            0.0,
            0.0,
            0.0,
        ];
        let bytes: Vec<u8> = values.iter().flat_map(|v| v.to_le_bytes()).collect();
        let uniform = cx.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("fill"),
            size: bytes.len() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        cx.write_buffer(&uniform, 0, &bytes);
        let dummy;
        let back = match cx.backdrop() {
            Some(view) => view.clone(),
            None => {
                dummy = cx.device.create_texture(&wgpu::TextureDescriptor {
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
                dummy.create_view(&wgpu::TextureViewDescriptor::default())
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
        label: Some("fill"),
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
        label: Some("fill"),
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
                blend: None,
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

fn boxed(app: &mut App, w: f32, h: f32, color: Option<Color>) -> Entity {
    let layout = Layout {
        width: Size::Px(w),
        height: Size::Px(h),
        ..Layout::row()
    };
    match color {
        Some(color) => app.world_mut().spawn((layout, color)),
        None => app.world_mut().spawn((layout,)),
    }
}

fn pixel(image: &RgbaImage, x: u32, y: u32) -> [u8; 4] {
    let index = ((y * image.width + x) * 4) as usize;
    image.rgba[index..index + 4].try_into().unwrap()
}

fn render(app: &mut App, w: u32, h: u32) -> RgbaImage {
    let frame = rndr_paint::frame(app);
    let mut exporter = match Exporter::new() {
        Ok(exporter) => exporter,
        Err(ExportError::NoAdapter) => panic!("no graphics adapter"),
        Err(err) => panic!("{err}"),
    };
    exporter
        .render_frame(Some(app.world()), &frame, w, h, 1.0)
        .expect("render")
}

fn effect_app() -> App {
    let mut app = App::new();
    app.world_mut().insert_resource(Viewport::new(160.0, 40.0));
    let mut effects = GpuEffects::new();
    effects.insert(1, Fill::default());
    app.world_mut().insert_resource(effects);
    app
}

#[test]
fn effects_draw_in_paint_order_under_later_siblings() {
    let mut app = effect_app();
    let root = boxed(&mut app, 160.0, 40.0, Some(Color::rgb(1.0, 1.0, 1.0)));
    let slot = boxed(&mut app, 40.0, 40.0, None);
    app.world_mut()
        .insert(slot, EffectSlot::new(1).with_data([0.0, 0.0, 1.0, 1.0]));
    // A child of the slot paints after the effect, over its left half.
    let cover = boxed(&mut app, 20.0, 40.0, Some(Color::rgb(0.0, 1.0, 0.0)));
    set_parent(app.world_mut(), cover, slot);
    // An unregistered id draws nothing.
    let missing = boxed(&mut app, 40.0, 40.0, None);
    app.world_mut().insert(missing, EffectSlot::new(7));
    for child in [slot, missing] {
        set_parent(app.world_mut(), child, root);
    }
    let image = render(&mut app, 160, 40);
    assert_eq!(pixel(&image, 30, 20), [0, 0, 255, 255]);
    assert_eq!(pixel(&image, 10, 20), [0, 255, 0, 255]);
    assert_eq!(pixel(&image, 60, 20), [255, 255, 255, 255]);
}

#[test]
fn effects_inside_faded_groups_take_the_group_opacity() {
    let mut app = effect_app();
    let root = boxed(&mut app, 160.0, 40.0, Some(Color::rgb(1.0, 1.0, 1.0)));
    let group = boxed(&mut app, 40.0, 40.0, None);
    app.world_mut().insert(group, Opacity(0.5));
    let slot = boxed(&mut app, 40.0, 40.0, None);
    app.world_mut()
        .insert(slot, EffectSlot::new(1).with_data([0.0, 0.0, 0.0, 1.0]));
    set_parent(app.world_mut(), slot, group);
    set_parent(app.world_mut(), group, root);
    let image = render(&mut app, 160, 40);
    let half = pixel(&image, 20, 20);
    assert!(half[0] > 100 && half[0] < 230, "{half:?}");
    assert_eq!(half[0], half[1]);
    assert_eq!(pixel(&image, 60, 20), [255, 255, 255, 255]);
}

#[test]
fn backdrop_effects_read_what_was_drawn_before_them() {
    let mut app = effect_app();
    let root = boxed(&mut app, 160.0, 40.0, Some(Color::rgb(1.0, 1.0, 1.0)));
    let red = boxed(&mut app, 80.0, 40.0, Some(Color::rgb(1.0, 0.0, 0.0)));
    set_parent(app.world_mut(), red, root);
    // The right half of each box inverts what is beneath it.
    let white = boxed(&mut app, 80.0, 40.0, None);
    set_parent(app.world_mut(), white, root);
    for parent in [red, white] {
        let spacer = boxed(&mut app, 40.0, 40.0, None);
        let slot = boxed(&mut app, 40.0, 40.0, None);
        app.world_mut()
            .insert(slot, EffectSlot::new(1).with_backdrop());
        set_parent(app.world_mut(), spacer, parent);
        set_parent(app.world_mut(), slot, parent);
    }
    let image = render(&mut app, 160, 40);
    assert_eq!(pixel(&image, 20, 20), [255, 0, 0, 255]);
    assert_eq!(pixel(&image, 60, 20), [0, 255, 255, 255]);
    assert_eq!(pixel(&image, 100, 20), [255, 255, 255, 255]);
    assert_eq!(pixel(&image, 140, 20), [0, 0, 0, 255]);
}
