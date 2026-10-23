//! Render a display list into an RGBA image and encode it as PNG.
//!
//! The target is [`wgpu::TextureFormat::Rgba8UnormSrgb`] when that format can
//! be rendered, so an authored red channel reads back near 255. Paths use the
//! same multisampled layer as an on-screen frame. The output is capped at
//! [`EXPORT_MAX_SIDE`] pixels on a side and [`EXPORT_MAX_PIXELS`] in total.
//!
//! [`render_scene_rgba`] maps `rect` to the top-left of the image. The scene
//! camera is the on-screen view and is not applied: `rect` and `scale` are the
//! export camera. Device pixels are world units times `scale`.

use std::fmt;

use rndr_core::{text_engine, Rect, World};
use rndr_paint::{atlas_r8, Affine, Frame, Op, Placed, Scene, ATLAS_H, ATLAS_W};

use crate::images::{self, CpuImage};
use crate::scene::{choose_samples, SceneGpu};

/// Largest width or height [`render_frame_rgba`] will allocate.
pub const EXPORT_MAX_SIDE: u32 = 8192;

/// Largest `width * height` [`render_frame_rgba`] will allocate.
pub const EXPORT_MAX_PIXELS: u32 = 32_000_000;

/// Straight RGBA, top row first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RgbaImage {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// `width * height * 4` bytes.
    pub rgba: Vec<u8>,
}

/// Why an export did not produce an image.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExportError {
    /// The rounded size is empty, or `scale` is not a positive finite number.
    Empty,
    /// The rounded size exceeds [`EXPORT_MAX_SIDE`] or [`EXPORT_MAX_PIXELS`].
    TooLarge {
        /// Requested width in pixels.
        width: u32,
        /// Requested height in pixels.
        height: u32,
    },
    /// No graphics adapter was available.
    NoAdapter,
    /// The device rejected the target, or the readback did not map.
    Device,
    /// The PNG encoder rejected the buffer.
    Png,
}

impl fmt::Display for ExportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "export size is empty"),
            Self::TooLarge { width, height } => {
                write!(f, "export {width}x{height} exceeds the size cap")
            }
            Self::NoAdapter => write!(f, "no graphics adapter"),
            Self::Device => write!(f, "export device or readback failed"),
            Self::Png => write!(f, "png encode failed"),
        }
    }
}

impl std::error::Error for ExportError {}

/// Headless renderer. Reuse one for several exports so pipelines stay compiled.
pub struct Exporter {
    /// Kept alive for the device.
    _instance: wgpu::Instance,
    device: wgpu::Device,
    queue: wgpu::Queue,
    scene: SceneGpu,
    format: wgpu::TextureFormat,
    name: String,
    coverage: wgpu::Texture,
    coverage_view: wgpu::TextureView,
    coverage_size: (u32, u32),
    color_tex: wgpu::Texture,
    color_view: wgpu::TextureView,
    color_size: (u32, u32),
    color_format: wgpu::TextureFormat,
}

impl Exporter {
    /// Open a device with no window. Fails with [`ExportError::NoAdapter`] when
    /// the process has no graphics adapter.
    pub fn new() -> Result<Self, ExportError> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            compatible_surface: None,
            ..Default::default()
        }))
        .map_err(|_| ExportError::NoAdapter)?;
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("export"),
            ..Default::default()
        }))
        .map_err(|_| ExportError::Device)?;
        let format = export_format(&adapter);
        let samples = choose_samples(&adapter, format);
        let scene = SceneGpu::new(device.clone(), queue.clone(), format, samples);
        let color_format = if format.is_srgb() {
            wgpu::TextureFormat::Rgba8UnormSrgb
        } else {
            wgpu::TextureFormat::Rgba8Unorm
        };
        let (coverage, coverage_view) = atlas_texture(&device, 1, 1, wgpu::TextureFormat::R8Unorm);
        let (color_tex, color_view) = atlas_texture(&device, 1, 1, color_format);
        let name = adapter.get_info().name;
        Ok(Self {
            _instance: instance,
            device,
            queue,
            scene,
            format,
            name,
            coverage,
            coverage_view,
            coverage_size: (1, 1),
            color_tex,
            color_view,
            color_size: (1, 1),
            color_format,
        })
    }

    /// Adapter name, for a benchmark line.
    pub fn adapter_name(&self) -> &str {
        &self.name
    }

    /// Draw `frame` at `scale` device pixels per logical point.
    ///
    /// `world` supplies decoded images and glyph atlases. Pass `None` for a
    /// frame that has neither. The CPU image copies are left in place.
    pub fn render_frame(
        &mut self,
        world: Option<&World>,
        frame: &Frame,
        width: u32,
        height: u32,
        scale: f32,
    ) -> Result<RgbaImage, ExportError> {
        check_size(width, height)?;
        if let Some(world) = world {
            self.sync_atlases(world);
        }
        let images = world.map(images::pending).unwrap_or_default();
        let budget = world
            .map(images::budget_of)
            .unwrap_or(images::IMAGE_BUDGET_DEFAULT);
        self.draw(world, frame, &images, budget, width, height, scale)
    }

    /// Draw `scene` so `rect`'s top-left is the image origin.
    ///
    /// `scale` is device pixels per world unit. The scene camera is not
    /// applied.
    pub fn render_scene(
        &mut self,
        world: &World,
        scene: &Scene,
        rect: Rect,
        scale: f32,
    ) -> Result<RgbaImage, ExportError> {
        let (width, height) = pixel_size(rect, scale)?;
        let frame = frame_for_rect(scene, rect);
        self.render_frame(Some(world), &frame, width, height, scale)
    }

    #[allow(clippy::too_many_arguments)]
    fn draw(
        &mut self,
        world: Option<&World>,
        frame: &Frame,
        images: &[(u64, CpuImage)],
        budget: u64,
        width: u32,
        height: u32,
        scale: f32,
    ) -> Result<RgbaImage, ExportError> {
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("export"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("export"),
            });
        self.scene.render(
            &mut encoder,
            &view,
            frame,
            images,
            budget,
            &self.coverage_view,
            &self.color_view,
            width,
            height,
            scale,
            wgpu::Color::TRANSPARENT,
            world,
        );
        let padded = row_stride(width);
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("export-read"),
            size: u64::from(padded) * u64::from(height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit(Some(encoder.finish()));
        let slice = buffer.slice(..);
        let (send, recv) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = send.send(result);
        });
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|_| ExportError::Device)?;
        recv.recv()
            .map_err(|_| ExportError::Device)?
            .map_err(|_| ExportError::Device)?;
        let mapped = slice.get_mapped_range().map_err(|_| ExportError::Device)?;
        let mut rgba = vec![0u8; width as usize * height as usize * 4];
        let row = width as usize * 4;
        let stride = padded as usize;
        for y in 0..height as usize {
            let from = y * stride;
            let to = y * row;
            rgba[to..to + row].copy_from_slice(&mapped[from..from + row]);
        }
        drop(mapped);
        buffer.unmap();
        Ok(RgbaImage {
            width,
            height,
            rgba,
        })
    }

    fn sync_atlases(&mut self, world: &World) {
        let engine = text_engine(world);
        if let Some(atlas) = engine.coverage_atlas() {
            self.ensure_coverage(atlas.width, atlas.height, &atlas.r8);
        } else {
            let pixels = atlas_r8();
            self.ensure_coverage(ATLAS_W, ATLAS_H, &pixels);
        }
        if let Some(atlas) = engine.color_atlas() {
            if atlas.width > 0 && atlas.height > 0 {
                self.ensure_color(atlas.width, atlas.height, &atlas.rgba);
            }
        }
    }

    fn ensure_coverage(&mut self, width: u32, height: u32, pixels: &[u8]) {
        let width = width.max(1);
        let height = height.max(1);
        if self.coverage_size != (width, height) {
            let (texture, view) =
                atlas_texture(&self.device, width, height, wgpu::TextureFormat::R8Unorm);
            self.coverage = texture;
            self.coverage_view = view;
            self.coverage_size = (width, height);
        }
        write_atlas(&self.queue, &self.coverage, width, height, 1, pixels);
    }

    fn ensure_color(&mut self, width: u32, height: u32, pixels: &[u8]) {
        let width = width.max(1);
        let height = height.max(1);
        if self.color_size != (width, height) {
            let (texture, view) = atlas_texture(&self.device, width, height, self.color_format);
            self.color_tex = texture;
            self.color_view = view;
            self.color_size = (width, height);
        }
        write_atlas(&self.queue, &self.color_tex, width, height, 4, pixels);
    }
}

/// Draw `frame` with a fresh device. Prefer [`Exporter`] for more than one image.
pub fn render_frame_rgba(
    world: Option<&World>,
    frame: &Frame,
    width: u32,
    height: u32,
    scale: f32,
) -> Result<RgbaImage, ExportError> {
    check_size(width, height)?;
    Exporter::new()?.render_frame(world, frame, width, height, scale)
}

/// Draw `scene` with a fresh device. See [`Exporter::render_scene`].
pub fn render_scene_rgba(
    world: &World,
    scene: &Scene,
    rect: Rect,
    scale: f32,
) -> Result<RgbaImage, ExportError> {
    let (width, height) = pixel_size(rect, scale)?;
    let frame = frame_for_rect(scene, rect);
    render_frame_rgba(Some(world), &frame, width, height, scale)
}

/// Encode `image` as an 8-bit RGBA PNG.
pub fn encode_png(image: &RgbaImage) -> Result<Vec<u8>, ExportError> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, image.width, image.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(|_| ExportError::Png)?;
    writer
        .write_image_data(&image.rgba)
        .map_err(|_| ExportError::Png)?;
    drop(writer);
    Ok(out)
}

fn export_format(adapter: &wgpu::Adapter) -> wgpu::TextureFormat {
    let srgb = wgpu::TextureFormat::Rgba8UnormSrgb;
    let flags = adapter.get_texture_format_features(srgb).flags;
    if flags.contains(wgpu::TextureFormatFeatureFlags::BLENDABLE) {
        srgb
    } else {
        wgpu::TextureFormat::Rgba8Unorm
    }
}

fn atlas_texture(
    device: &wgpu::Device,
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("export-atlas"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}

fn write_atlas(
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    width: u32,
    height: u32,
    bpp: u32,
    pixels: &[u8],
) {
    let need = (width as usize)
        .saturating_mul(height as usize)
        .saturating_mul(bpp as usize);
    if pixels.len() < need || need == 0 {
        return;
    }
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &pixels[..need],
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(width * bpp),
            rows_per_image: Some(height),
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
}

fn frame_for_rect(scene: &Scene, rect: Rect) -> Frame {
    let shift = Affine::translate(-rect.x, -rect.y);
    let mut draws = Vec::with_capacity(scene.commands.len() + 2);
    draws.push(Placed {
        z: 0,
        op: Op::PushTransform(shift),
    });
    for op in &scene.commands {
        draws.push(Placed {
            z: 0,
            op: op.clone(),
        });
    }
    draws.push(Placed {
        z: 0,
        op: Op::PopTransform,
    });
    Frame {
        quads: Vec::new(),
        slots: Vec::new(),
        draws,
    }
}

fn pixel_size(rect: Rect, scale: f32) -> Result<(u32, u32), ExportError> {
    if !scale.is_finite() || scale <= 0.0 || !rect.w.is_finite() || !rect.h.is_finite() {
        return Err(ExportError::Empty);
    }
    let width = rect.w * scale;
    let height = rect.h * scale;
    if !(1.0..u32::MAX as f32).contains(&width) || !(1.0..u32::MAX as f32).contains(&height) {
        return Err(ExportError::Empty);
    }
    let width = width.round() as u32;
    let height = height.round() as u32;
    check_size(width, height)?;
    Ok((width, height))
}

fn check_size(width: u32, height: u32) -> Result<(), ExportError> {
    if width == 0 || height == 0 {
        return Err(ExportError::Empty);
    }
    let pixels = u64::from(width) * u64::from(height);
    if width > EXPORT_MAX_SIDE || height > EXPORT_MAX_SIDE || pixels > u64::from(EXPORT_MAX_PIXELS)
    {
        return Err(ExportError::TooLarge { width, height });
    }
    Ok(())
}

fn row_stride(width: u32) -> u32 {
    let bytes = width.saturating_mul(4);
    bytes.div_ceil(256) * 256
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_an_oversized_export() {
        let err = pixel_size(
            Rect {
                x: 0.0,
                y: 0.0,
                w: 9000.0,
                h: 16.0,
            },
            1.0,
        )
        .unwrap_err();
        assert!(matches!(
            err,
            ExportError::TooLarge {
                width: 9000,
                height: 16
            }
        ));
        let err = pixel_size(
            Rect {
                x: 0.0,
                y: 0.0,
                w: 8192.0,
                h: 8192.0,
            },
            1.0,
        )
        .unwrap_err();
        assert!(matches!(err, ExportError::TooLarge { .. }));
        let err = pixel_size(
            Rect {
                x: 0.0,
                y: 0.0,
                w: 10.0,
                h: 0.0,
            },
            1.0,
        )
        .unwrap_err();
        assert_eq!(err, ExportError::Empty);
    }
}
