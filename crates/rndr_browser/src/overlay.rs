//! Trusted host overlay images, never populated by guest HTML.
use crate::{admit_client_rect, ArtifactError, ClientRect};

/// Maximum bytes across all submitted overlay images (16 MiB).
pub const MAX_OVERLAY_BYTES: usize = 16 * 1024 * 1024;
/// Maximum overlay count, ordered back-to-front.
pub const MAX_OVERLAYS: usize = 32;
/// Maximum pixel width or height of a host overlay.
pub const MAX_OVERLAY_SIDE: u32 = 4096;

/// A trusted host popup painted above the isolated guest with real transparency.
/// No guest screenshot or command bridge is involved. Native image storage is
/// separate from WebKit's isolated document and cannot be accessed by scripts.
#[derive(Clone, Debug, PartialEq)]
pub struct OverlayBitmap {
    /// Image placement in logical top-left window client coordinates.
    pub bounds: ClientRect,
    /// Pixel width, independent of logical size (supports HiDPI).
    pub width: u32,
    /// Pixel height.
    pub height: u32,
    /// Straight-alpha sRGB RGBA8, top row first, exactly width * height * 4.
    pub rgba: Vec<u8>,
    /// Optional interactive popup body, in logical window client coordinates.
    /// Pointer events inside it are routed to the native RNDR parent view.
    /// None makes the bitmap completely pointer-transparent (e.g. a tooltip).
    /// Transparent shadow margins should not be included in this rectangle.
    pub pointer_bounds: Option<ClientRect>,
}

pub(crate) fn validate(overlays: &[OverlayBitmap]) -> Result<(), ArtifactError> {
    if overlays.len() > MAX_OVERLAYS {
        return Err(ArtifactError::Overlay("too many overlays"));
    }
    let mut total = 0usize;
    for o in overlays {
        admit_client_rect(o.bounds.x, o.bounds.y, o.bounds.width, o.bounds.height)
            .map_err(ArtifactError::Rect)?;
        if o.bounds.width == 0.
            || o.bounds.height == 0.
            || o.width == 0
            || o.height == 0
            || o.width > MAX_OVERLAY_SIDE
            || o.height > MAX_OVERLAY_SIDE
        {
            return Err(ArtifactError::Overlay("invalid overlay dimensions"));
        }
        if let Some(r) = o.pointer_bounds {
            admit_client_rect(r.x, r.y, r.width, r.height).map_err(ArtifactError::Rect)?;
        }
        let bytes = (o.width as usize)
            .checked_mul(o.height as usize)
            .and_then(|n| n.checked_mul(4))
            .ok_or(ArtifactError::Overlay("overlay size overflow"))?;
        total = total
            .checked_add(bytes)
            .ok_or(ArtifactError::Overlay("overlay size overflow"))?;
        if total > MAX_OVERLAY_BYTES || bytes != o.rgba.len() {
            return Err(ArtifactError::Overlay(
                "overlay byte limit or length mismatch",
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn image() -> OverlayBitmap {
        OverlayBitmap {
            bounds: ClientRect::try_new(10., 20., 2., 2.).unwrap(),
            width: 2,
            height: 2,
            rgba: vec![255; 16],
            pointer_bounds: None,
        }
    }
    #[test]
    fn admission_checks_exact_lengths_count_bytes_and_finiteness() {
        assert!(validate(&[]).is_ok());
        assert!(validate(&[image()]).is_ok());
        let mut o = image();
        o.rgba.pop();
        assert!(validate(&[o]).is_err());
        assert!(validate(&vec![image(); MAX_OVERLAYS + 1]).is_err());
        let mut o = image();
        o.width = MAX_OVERLAY_SIDE + 1;
        assert!(validate(&[o]).is_err());
        let mut o = image();
        o.bounds.x = f32::NAN;
        assert!(validate(&[o]).is_err());
        let mut o = image();
        o.pointer_bounds = Some(ClientRect {
            x: 0.,
            y: 0.,
            width: f32::INFINITY,
            height: 1.,
        });
        assert!(validate(&[o]).is_err());
        let mut o = image();
        o.width = 2048;
        o.height = 2048;
        o.rgba = vec![0; MAX_OVERLAY_BYTES];
        assert!(validate(&[o.clone()]).is_ok());
        assert!(validate(&[o, image()]).is_err());
    }
}
