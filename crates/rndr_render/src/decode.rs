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

/// Decode a PNG, JPEG, or WebP.
///
/// PNG and WebP keep their alpha. JPEG is opaque. Other formats return
/// [`DecodeError::Unknown`].
pub fn decode_image(bytes: &[u8]) -> Result<Raster, DecodeError> {
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
