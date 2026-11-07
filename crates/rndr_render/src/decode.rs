//! PNG, JPEG, and WebP bytes to straight RGBA.
//!
//! Decoding runs on the executor that [`crate::load_image`] spawns, which is
//! off the UI thread when the desktop pool is installed. The UI thread only
//! copies the finished buffer into the image store.

use std::io::Cursor;

/// Decoded 8-bit straight RGBA, top row first.
#[derive(Clone, Debug)]
pub struct Raster {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// `width * height * 4` bytes, RGBA.
    pub rgba: Vec<u8>,
}

/// Why [`crate::decode`] rejected `bytes`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DecodeError {
    /// The bytes are not a PNG, JPEG, or WebP.
    Unknown,
    /// The container was recognized and the payload is not valid.
    Invalid,
    /// Width or height is zero, or the buffer is shorter than the image.
    Empty,
}

/// Decode a PNG, JPEG, or WebP, or with the `svg` feature an SVG or SVGZ.
///
/// PNG and WebP keep their alpha. JPEG is opaque. An SVG is rasterized at
/// twice its own size; use [`decode_svg`] for an exact pixel size. Other formats return
/// [`DecodeError::Unknown`].
pub fn decode_image(bytes: &[u8]) -> Result<Raster, DecodeError> {
    #[cfg(feature = "svg")]
    if is_svg(bytes) {
        let tree = svg_tree(bytes)?;
        return rasterize_svg(&tree, 2.0);
    }
    if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        decode_png(bytes)
    } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        decode_jpeg(bytes)
    } else if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        decode_webp(bytes)
    } else {
        Err(DecodeError::Unknown)
    }
}

/// Rasterize SVG or gzipped SVGZ bytes to fit inside `width` by `height`
/// pixels, keeping the aspect ratio. The raster is the fitted size, not
/// padded. Text must already be converted to paths; no fonts are loaded.
#[cfg(feature = "svg")]
pub fn decode_svg(bytes: &[u8], width: u32, height: u32) -> Result<Raster, DecodeError> {
    let tree = svg_tree(bytes)?;
    let size = tree.size();
    let scale = (width as f32 / size.width()).min(height as f32 / size.height());
    rasterize_svg(&tree, scale)
}

#[cfg(feature = "svg")]
fn is_svg(bytes: &[u8]) -> bool {
    if bytes.starts_with(&[0x1f, 0x8b]) {
        return true;
    }
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(4096)]);
    let text = head.trim_start_matches('\u{feff}').trim_start();
    text.starts_with('<') && head.contains("<svg")
}

#[cfg(feature = "svg")]
fn svg_tree(bytes: &[u8]) -> Result<resvg::usvg::Tree, DecodeError> {
    resvg::usvg::Tree::from_data(bytes, &resvg::usvg::Options::default())
        .map_err(|_| DecodeError::Invalid)
}

#[cfg(feature = "svg")]
fn rasterize_svg(tree: &resvg::usvg::Tree, scale: f32) -> Result<Raster, DecodeError> {
    const MAX_SIDE: f32 = 16384.0;
    let size = tree.size();
    let width = (size.width() * scale).round();
    let height = (size.height() * scale).round();
    if !(width >= 1.0 && height >= 1.0 && width <= MAX_SIDE && height <= MAX_SIDE) {
        return Err(DecodeError::Empty);
    }
    let (width, height) = (width as u32, height as u32);
    let mut pixmap = resvg::tiny_skia::Pixmap::new(width, height).ok_or(DecodeError::Empty)?;
    let transform = resvg::tiny_skia::Transform::from_scale(
        width as f32 / size.width(),
        height as f32 / size.height(),
    );
    resvg::render(tree, transform, &mut pixmap.as_mut());
    let mut rgba = pixmap.take();
    for i in (0..rgba.len()).step_by(4) {
        let alpha = u32::from(rgba[i + 3]);
        if alpha > 0 && alpha < 255 {
            for channel in &mut rgba[i..i + 3] {
                *channel = ((u32::from(*channel) * 255 + alpha / 2) / alpha).min(255) as u8;
            }
        }
    }
    Ok(Raster {
        width,
        height,
        rgba,
    })
}

fn decode_png(bytes: &[u8]) -> Result<Raster, DecodeError> {
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().map_err(|_| DecodeError::Invalid)?;
    let mut buf = vec![0; reader.output_buffer_size().ok_or(DecodeError::Invalid)?];
    let frame = reader
        .next_frame(&mut buf)
        .map_err(|_| DecodeError::Invalid)?;
    let data = &buf[..frame.buffer_size()];
    let rgba = match frame.color_type {
        png::ColorType::Rgba => data.to_vec(),
        png::ColorType::Rgb => expand_rgb(data),
        png::ColorType::Grayscale => expand_gray(data),
        png::ColorType::GrayscaleAlpha => expand_gray_alpha(data),
        _ => return Err(DecodeError::Invalid),
    };
    finish(frame.width, frame.height, rgba)
}

fn decode_jpeg(bytes: &[u8]) -> Result<Raster, DecodeError> {
    let mut decoder = jpeg_decoder::Decoder::new(Cursor::new(bytes));
    let pixels = decoder.decode().map_err(|_| DecodeError::Invalid)?;
    let info = decoder.info().ok_or(DecodeError::Invalid)?;
    let rgba = match info.pixel_format {
        jpeg_decoder::PixelFormat::RGB24 => expand_rgb(&pixels),
        jpeg_decoder::PixelFormat::L8 => expand_gray(&pixels),
        jpeg_decoder::PixelFormat::L16 => {
            let gray: Vec<u8> = pixels.as_chunks::<2>().0.iter().map(|px| px[0]).collect();
            expand_gray(&gray)
        }
        jpeg_decoder::PixelFormat::CMYK32 => cmyk_to_rgba(&pixels),
    };
    finish(u32::from(info.width), u32::from(info.height), rgba)
}

fn decode_webp(bytes: &[u8]) -> Result<Raster, DecodeError> {
    let mut decoder =
        image_webp::WebPDecoder::new(Cursor::new(bytes)).map_err(|_| DecodeError::Invalid)?;
    let (width, height) = decoder.dimensions();
    let size = decoder.output_buffer_size().ok_or(DecodeError::Invalid)?;
    let mut buf = vec![0; size];
    decoder
        .read_image(&mut buf)
        .map_err(|_| DecodeError::Invalid)?;
    let rgba = if decoder.has_alpha() {
        buf
    } else {
        expand_rgb(&buf)
    };
    finish(width, height, rgba)
}

fn finish(width: u32, height: u32, rgba: Vec<u8>) -> Result<Raster, DecodeError> {
    if width == 0 || height == 0 {
        return Err(DecodeError::Empty);
    }
    let need = (width as usize)
        .saturating_mul(height as usize)
        .saturating_mul(4);
    if rgba.len() < need {
        return Err(DecodeError::Invalid);
    }
    Ok(Raster {
        width,
        height,
        rgba: rgba[..need].to_vec(),
    })
}

fn expand_rgb(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() / 3 * 4);
    for px in data.as_chunks::<3>().0 {
        out.extend_from_slice(&[px[0], px[1], px[2], 255]);
    }
    out
}

fn expand_gray(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() * 4);
    for px in data {
        out.extend_from_slice(&[*px, *px, *px, 255]);
    }
    out
}

fn expand_gray_alpha(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() / 2 * 4);
    for px in data.as_chunks::<2>().0 {
        out.extend_from_slice(&[px[0], px[0], px[0], px[1]]);
    }
    out
}

fn cmyk_to_rgba(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len());
    for px in data.as_chunks::<4>().0 {
        let (c, m, y, k) = (px[0] as u16, px[1] as u16, px[2] as u16, px[3] as u16);
        let r = (255 - c) * (255 - k) / 255;
        let g = (255 - m) * (255 - k) / 255;
        let b = (255 - y) * (255 - k) / 255;
        out.extend_from_slice(&[r as u8, g as u8, b as u8, 255]);
    }
    out
}

/// Box-filter `rgba` down by half on each axis. A 1×N image halves the long side.
pub fn downsample(rgba: &[u8], width: u32, height: u32) -> (u32, u32, Vec<u8>) {
    if width <= 1 && height <= 1 {
        return (width.max(1), height.max(1), rgba.to_vec());
    }
    let nw = (width / 2).max(1);
    let nh = (height / 2).max(1);
    let mut out = vec![0u8; (nw as usize) * (nh as usize) * 4];
    for y in 0..nh {
        for x in 0..nw {
            let mut acc = [0u32; 4];
            let mut n = 0u32;
            for dy in 0..2 {
                for dx in 0..2 {
                    let sx = (x * 2 + dx).min(width - 1);
                    let sy = (y * 2 + dy).min(height - 1);
                    let i = ((sy * width + sx) as usize) * 4;
                    if i + 3 < rgba.len() {
                        for c in 0..4 {
                            acc[c] += u32::from(rgba[i + c]);
                        }
                        n += 1;
                    }
                }
            }
            let Some(n) = std::num::NonZeroU32::new(n) else {
                continue;
            };
            let o = ((y * nw + x) as usize) * 4;
            for c in 0..4 {
                out[o + c] = (acc[c] / n) as u8;
            }
        }
    }
    (nw, nh, out)
}

/// Mip chain, level 0 first. Each step is [`downsample`].
pub fn mip_chain(rgba: &[u8], width: u32, height: u32) -> Vec<(u32, u32, Vec<u8>)> {
    let mut levels = vec![(width, height, rgba.to_vec())];
    while levels.last().is_some_and(|(w, h, _)| *w > 1 || *h > 1) {
        let (w, h, pixels) = levels.last().unwrap();
        levels.push(downsample(pixels, *w, *h));
    }
    levels
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "svg")]
    const SQUARE: &str = r##"<?xml version="1.0"?>
<svg xmlns="http://www.w3.org/2000/svg" width="16" height="8" viewBox="0 0 16 8">
  <rect x="0" y="0" width="8" height="8" fill="#ff0000"/>
  <rect x="8" y="0" width="8" height="8" fill="#0000ff" fill-opacity="0.5"/>
</svg>"##;

    #[cfg(feature = "svg")]
    #[test]
    fn svg_rasterizes_at_twice_size_or_a_requested_fit() {
        let raster = decode_image(SQUARE.as_bytes()).unwrap();
        assert_eq!((raster.width, raster.height), (32, 16));
        assert_eq!(&raster.rgba[0..4], &[255, 0, 0, 255]);
        let right = (8 * 32 + 24) * 4;
        assert_eq!(
            &raster.rgba[right..right + 4],
            &[0, 0, 255, 128],
            "straight alpha"
        );
        let fitted = decode_svg(SQUARE.as_bytes(), 64, 64).unwrap();
        assert_eq!((fitted.width, fitted.height), (64, 32));
        assert_eq!(decode_image(b"<svg").err(), Some(DecodeError::Invalid));
        assert_eq!(
            decode_svg(SQUARE.as_bytes(), 0, 64).err(),
            Some(DecodeError::Empty)
        );
    }

    fn encode_png(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut encoder = png::Encoder::new(&mut out, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(rgba).unwrap();
        drop(writer);
        out
    }

    #[test]
    fn png_round_trips_a_red_pixel() {
        let png = encode_png(1, 1, &[255, 0, 0, 128]);
        let raster = decode_image(&png).unwrap();
        assert_eq!(raster.width, 1);
        assert_eq!(raster.rgba, vec![255, 0, 0, 128]);
    }

    #[test]
    fn webp_round_trips_rgba() {
        let mut bytes = Vec::new();
        image_webp::WebPEncoder::new(&mut bytes)
            .encode(&[0, 255, 0, 255], 1, 1, image_webp::ColorType::Rgba8)
            .unwrap();
        let raster = decode_image(&bytes).unwrap();
        assert_eq!(raster.rgba, vec![0, 255, 0, 255]);
    }

    #[test]
    fn garbage_is_unknown_and_a_bad_jpeg_is_invalid() {
        assert_eq!(decode_image(b"nope").unwrap_err(), DecodeError::Unknown);
        assert_eq!(
            decode_image(&[0xff, 0xd8, 0xff, 0x00]).unwrap_err(),
            DecodeError::Invalid
        );
    }

    #[test]
    fn downsample_averages_a_two_by_two() {
        let rgba = [0, 0, 0, 255, 100, 0, 0, 255, 0, 0, 0, 255, 100, 0, 0, 255];
        let (w, h, out) = downsample(&rgba, 2, 2);
        assert_eq!((w, h), (1, 1));
        assert_eq!(out[0], 50);
        assert_eq!(out[3], 255);
    }
}
