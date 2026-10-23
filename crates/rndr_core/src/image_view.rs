//! Image painted in a layout rect. List rows use this for icons.
//!
//! The id matches a texture the renderer decoded. Paint records an image
//! command in the display list and does not change the quads of any other
//! entity. A missing texture draws nothing until the decode finishes.
//!
//! Layout centers the view on its parent's cross axis, so a 16px icon in a
//! 32px row sits clear of a rounded selection. The recorded command carries
//! the entity clip, and the sprite is masked to it.

use crate::world::Component;

impl Component for ImageView {}

/// Image inside the entity's [`crate::Rect`].
///
/// Spawn it on a list-row child with a fixed size. The rect is the destination.
/// [`Self::uv`] crops the texture. [`Self::radius`] rounds the corners in
/// logical points. The renderer tiles a texture larger than the device limit
/// and samples mipmaps when the view is zoomed out.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ImageView {
    /// Texture id passed to the image loader.
    pub image: u64,
    /// Crop as `(u0, v0, u1, v1)` in the 0–1 range of the texture.
    /// `(0, 0, 1, 1)` is the whole image.
    pub uv: [f32; 4],
    /// Corner radius in logical points. Zero is square.
    pub radius: f32,
}

impl ImageView {
    /// Whole image, square corners.
    pub fn new(image: u64) -> Self {
        Self {
            image,
            uv: [0.0, 0.0, 1.0, 1.0],
            radius: 0.0,
        }
    }

    /// Store `radius` and return the view.
    pub fn with_radius(mut self, radius: f32) -> Self {
        self.radius = radius;
        self
    }

    /// Store `uv` and return the view.
    pub fn with_uv(mut self, uv: [f32; 4]) -> Self {
        self.uv = uv;
        self
    }
}
