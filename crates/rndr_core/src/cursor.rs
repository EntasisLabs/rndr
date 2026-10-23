//! Custom cursor images and a rotation helper.
//!
//! [`CursorStyle::Image`] tells the desktop runner to use the [`CursorImage`]
//! resource. [`rotate_cursor`] builds a new image whose pixels and hotspot
//! follow `radians` clockwise in screen space (Y grows downward). The runner
//! turns that image into a platform cursor.

use crate::input::CursorStyle;

/// Straight RGBA8 cursor, row-major, origin top-left.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CursorImage {
    /// Pixel bytes. Length is `width * height * 4`.
    pub rgba: Vec<u8>,
    /// Width in pixels.
    pub width: u16,
    /// Height in pixels.
    pub height: u16,
    /// Hotspot column, inside the image.
    pub hotspot_x: u16,
    /// Hotspot row, inside the image.
    pub hotspot_y: u16,
}

impl CursorImage {
    /// `rgba` when its length is `width * height * 4` and the hotspot lies inside.
    ///
    /// A zero width or height is rejected. Dimensions above [`u16::MAX`] cannot
    /// be stored.
    pub fn new(
        width: u16,
        height: u16,
        hotspot_x: u16,
        hotspot_y: u16,
        rgba: Vec<u8>,
    ) -> Option<Self> {
        if width == 0 || height == 0 {
            return None;
        }
        let pixels = u32::from(width) * u32::from(height);
        if rgba.len() != pixels as usize * 4 {
            return None;
        }
        if hotspot_x >= width || hotspot_y >= height {
            return None;
        }
        Some(Self {
            rgba,
            width,
            height,
            hotspot_x,
            hotspot_y,
        })
    }
}

/// Rotate `image` clockwise by `radians` around its center.
///
/// A non-finite angle, or an angle within `1e-4` radians of zero, returns a
/// clone. The new bounds cover the rotated rectangle. Sampling is
/// nearest-neighbor. The hotspot is the same point after the rotation, clamped
/// into the new image.
pub fn rotate_cursor(image: &CursorImage, radians: f32) -> CursorImage {
    if !radians.is_finite() || radians.abs() < 1e-4 {
        return image.clone();
    }
    let (sin, cos) = radians.sin_cos();
    let width = f32::from(image.width);
    let height = f32::from(image.height);
    let corners = [(0.0, 0.0), (width, 0.0), (0.0, height), (width, height)];
    let center_x = width * 0.5;
    let center_y = height * 0.5;
    let rotated: Vec<(f32, f32)> = corners
        .into_iter()
        .map(|(x, y)| rotate_point(x - center_x, y - center_y, sin, cos))
        .collect();
    let min_x = rotated
        .iter()
        .map(|point| point.0)
        .fold(f32::INFINITY, f32::min);
    let max_x = rotated
        .iter()
        .map(|point| point.0)
        .fold(f32::NEG_INFINITY, f32::max);
    let min_y = rotated
        .iter()
        .map(|point| point.1)
        .fold(f32::INFINITY, f32::min);
    let max_y = rotated
        .iter()
        .map(|point| point.1)
        .fold(f32::NEG_INFINITY, f32::max);
    let new_width = (max_x - min_x).ceil().max(1.0) as u32;
    let new_height = (max_y - min_y).ceil().max(1.0) as u32;
    let new_width = new_width.min(u32::from(u16::MAX)) as u16;
    let new_height = new_height.min(u32::from(u16::MAX)) as u16;
    let mut rgba = vec![0u8; usize::from(new_width) * usize::from(new_height) * 4];
    for y in 0..new_height {
        for x in 0..new_width {
            let world_x = min_x + f32::from(x) + 0.5;
            let world_y = min_y + f32::from(y) + 0.5;
            let (src_x, src_y) = rotate_point(world_x, world_y, -sin, cos);
            let sample_x = (src_x + center_x).floor() as i32;
            let sample_y = (src_y + center_y).floor() as i32;
            if sample_x < 0
                || sample_y < 0
                || sample_x >= i32::from(image.width)
                || sample_y >= i32::from(image.height)
            {
                continue;
            }
            let from = (sample_y as usize * usize::from(image.width) + sample_x as usize) * 4;
            let to = (usize::from(y) * usize::from(new_width) + usize::from(x)) * 4;
            rgba[to..to + 4].copy_from_slice(&image.rgba[from..from + 4]);
        }
    }
    let (hot_x, hot_y) = rotate_point(
        f32::from(image.hotspot_x) + 0.5 - center_x,
        f32::from(image.hotspot_y) + 0.5 - center_y,
        sin,
        cos,
    );
    let hotspot_x = (hot_x - min_x).floor().clamp(0.0, f32::from(new_width - 1)) as u16;
    let hotspot_y = (hot_y - min_y)
        .floor()
        .clamp(0.0, f32::from(new_height - 1)) as u16;
    CursorImage {
        rgba,
        width: new_width,
        height: new_height,
        hotspot_x,
        hotspot_y,
    }
}

/// Clockwise rotation in a Y-down plane.
fn rotate_point(x: f32, y: f32, sin: f32, cos: f32) -> (f32, f32) {
    (x * cos + y * sin, -x * sin + y * cos)
}

/// The image the runner should display for `style`, when one is required.
pub fn cursor_image(style: CursorStyle, image: Option<&CursorImage>) -> Option<&CursorImage> {
    match style {
        CursorStyle::Image => image,
        CursorStyle::Arrow | CursorStyle::Hand => None,
    }
}
