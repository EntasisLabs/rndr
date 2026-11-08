//! GPU textures for [`rndr_core::FrameSurface`] frames.
//!
//! RGBA and BGRA frames upload into one sRGB texture. NV12 uploads its two
//! planes and a pass converts them to RGBA once per frame, so drawing goes
//! through the image sprite path with its corners, clips and groups.

use rndr_core::{PixelFormat, VideoFrame, YuvMatrix, YuvRange};

const NV12_SHADER: &str = r#"
struct U { mode: vec4<f32> }
@group(0) @binding(0) var<uniform> u: U;
@group(0) @binding(1) var luma: texture_2d<f32>;
@group(0) @binding(2) var chroma: texture_2d<f32>;

@vertex
fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let p = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(p * 2.0 - 1.0, 0.0, 1.0);
}

fn decode(v: vec3<f32>) -> vec3<f32> {
    return select(pow((v + 0.055) / 1.055, vec3<f32>(2.4)), v / 12.92, v <= vec3<f32>(0.04045));
}

// `mode.x` is BT.709, `mode.y` full range, `mode.z` an sRGB target that
// encodes on store, so the shader hands it linear values.
@fragment
fn fs(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    let texel = vec2<i32>(floor(p.xy));
    var y = textureLoad(luma, texel, 0).r;
    var c = textureLoad(chroma, texel / 2, 0).rg - vec2<f32>(128.0 / 255.0);
    if (u.mode.y < 0.5) {
        y = (y - 16.0 / 255.0) * (255.0 / 219.0);
        c = c * (255.0 / 224.0);
    }
    var kr = 0.299;
    var kb = 0.114;
    if (u.mode.x > 0.5) {
        kr = 0.2126;
        kb = 0.0722;
    }
    let r = y + 2.0 * (1.0 - kr) * c.y;
    let b = y + 2.0 * (1.0 - kb) * c.x;
    let g = (y - kr * r - kb * b) / (1.0 - kr - kb);
    var rgb = clamp(vec3<f32>(r, g, b), vec3<f32>(0.0), vec3<f32>(1.0));
    if (u.mode.z > 0.5) {
        rgb = decode(rgb);
    }
    return vec4<f32>(rgb, 1.0);
}
"#;

/// Textures for one surface on one device.
#[derive(Default)]
pub(crate) struct GpuSurface {
    /// [`rndr_core::FrameSurface::version`] last taken.
    pub version: u64,
    /// Scene frame that last drew it.
    pub stamp: u64,
    /// Sprite binding for the drawn texture. `None` draws nothing.
    pub bind: Option<wgpu::BindGroup>,
    shape: Option<(PixelFormat, u32, u32)>,
    planes: Vec<wgpu::Texture>,
    shown: Option<wgpu::Texture>,
    uniform: Option<wgpu::Buffer>,
    convert: Option<wgpu::BindGroup>,
}

/// The NV12 conversion pipeline for one output format.
pub(crate) struct Nv12Pipe {
    format: wgpu::TextureFormat,
    layout: wgpu::BindGroupLayout,
    pipeline: wgpu::RenderPipeline,
}

impl Nv12Pipe {
    pub(crate) fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let texture = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: false },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("nv12-bg"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                texture(1),
                texture(2),
            ],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("nv12"),
            source: wgpu::ShaderSource::Wgsl(NV12_SHADER.into()),
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("nv12"),
            layout: Some(
                &device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("nv12-pl"),
                    bind_group_layouts: &[Some(&layout)],
                    immediate_size: 0,
                }),
            ),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        Self {
            format,
            layout,
            pipeline,
        }
    }
}

/// What the drawn texture must match.
#[derive(Clone, Copy)]
pub(crate) struct Target {
    /// The scene target encodes sRGB on store.
    pub srgb: bool,
    /// Largest texture side the device accepts.
    pub max_dim: u32,
}

/// Upload `frame` into `surface`. Returns the texture to draw, or `None` when
/// the frame cannot be shown (larger than `max_dim`).
pub(crate) fn upload(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    encoder: &mut wgpu::CommandEncoder,
    nv12: &mut Option<Nv12Pipe>,
    surface: &mut GpuSurface,
    frame: &VideoFrame,
    target: Target,
) -> Option<wgpu::TextureView> {
    let Target { srgb, max_dim } = target;
    let (width, height) = (frame.width, frame.height);
    if width == 0 || height == 0 || width > max_dim || height > max_dim {
        surface.shape = None;
        return None;
    }
    let shape = (frame.format, width, height);
    let rebuild = surface.shape != Some(shape);
    let rgb_format = |base: wgpu::TextureFormat| match (base, srgb) {
        (wgpu::TextureFormat::Bgra8Unorm, true) => wgpu::TextureFormat::Bgra8UnormSrgb,
        (_, true) => wgpu::TextureFormat::Rgba8UnormSrgb,
        (wgpu::TextureFormat::Bgra8Unorm, false) => wgpu::TextureFormat::Bgra8Unorm,
        _ => wgpu::TextureFormat::Rgba8Unorm,
    };
    let make = |label, w, h, format, usage| {
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage,
            view_formats: &[],
        })
    };
    let sampled = wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST;
    let write = |texture: &wgpu::Texture, bytes: &[u8], bpp: u32, w: u32, h: u32| {
        queue.write_texture(
            texture.as_image_copy(),
            &bytes[..(bpp * w * h) as usize],
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(bpp * w),
                rows_per_image: Some(h),
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
    };
    match frame.format {
        PixelFormat::Rgba | PixelFormat::Bgra => {
            if rebuild {
                let base = if frame.format == PixelFormat::Bgra {
                    wgpu::TextureFormat::Bgra8Unorm
                } else {
                    wgpu::TextureFormat::Rgba8Unorm
                };
                surface.planes = vec![make("surface", width, height, rgb_format(base), sampled)];
                surface.shown = None;
                surface.convert = None;
            }
            write(&surface.planes[0], &frame.planes[0], 4, width, height);
            surface.shape = Some(shape);
            Some(surface.planes[0].create_view(&Default::default()))
        }
        PixelFormat::Nv12 => {
            let out_format = rgb_format(wgpu::TextureFormat::Rgba8Unorm);
            if nv12.as_ref().is_none_or(|pipe| pipe.format != out_format) {
                *nv12 = Some(Nv12Pipe::new(device, out_format));
            }
            let pipe = nv12.as_ref()?;
            let (cw, ch) = (width.div_ceil(2), height.div_ceil(2));
            if rebuild || surface.convert.is_none() {
                let luma = make(
                    "surface-y",
                    width,
                    height,
                    wgpu::TextureFormat::R8Unorm,
                    sampled,
                );
                let chroma = make("surface-uv", cw, ch, wgpu::TextureFormat::Rg8Unorm, sampled);
                let shown = make(
                    "surface-rgb",
                    width,
                    height,
                    out_format,
                    wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT,
                );
                let uniform = device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("surface-yuv"),
                    size: 16,
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                let luma_view = luma.create_view(&Default::default());
                let chroma_view = chroma.create_view(&Default::default());
                surface.convert = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("nv12"),
                    layout: &pipe.layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: uniform.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::TextureView(&luma_view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::TextureView(&chroma_view),
                        },
                    ],
                }));
                surface.planes = vec![luma, chroma];
                surface.shown = Some(shown);
                surface.uniform = Some(uniform);
            }
            let (Some(shown), Some(uniform), Some(convert)) =
                (&surface.shown, &surface.uniform, &surface.convert)
            else {
                return None;
            };
            write(&surface.planes[0], &frame.planes[0], 1, width, height);
            write(&surface.planes[1], &frame.planes[1], 2, cw, ch);
            let mode: [f32; 4] = [
                (frame.matrix == YuvMatrix::Bt709) as u8 as f32,
                (frame.range == YuvRange::Full) as u8 as f32,
                srgb as u8 as f32,
                0.0,
            ];
            let bytes: Vec<u8> = mode.iter().flat_map(|v| v.to_le_bytes()).collect();
            queue.write_buffer(uniform, 0, &bytes);
            let view = shown.create_view(&Default::default());
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("nv12"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                pass.set_pipeline(&pipe.pipeline);
                pass.set_bind_group(0, convert, &[]);
                pass.draw(0..3, 0..1);
            }
            surface.shape = Some(shape);
            Some(view)
        }
    }
}
