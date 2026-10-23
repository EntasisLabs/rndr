//! Swapchain encoding for authored sRGB colors.
//!
//! Public [`Color`](rndr_core::Color) values are sRGB with straight alpha.
//! Paint leaves them that way, so snapshot dumps still show the theme. This
//! module is the only place that prepares a color for the swapchain.
//!
//! An sRGB surface format encodes on store and blends in linear light, so red,
//! green, and blue are decoded before they are uploaded or used as a clear. A
//! non-sRGB 8-bit format stores the authored channels. Alpha is coverage in
//! both cases and is not decoded.
//!
//! Solid fills, borders, glyph tints, carets, selections, and marks are quad
//! colors and all go through [`color_for_surface`]. The pass clear does too.

use rndr_core::Color;

/// Pass clear, authored in sRGB like every other [`Color`](rndr_core::Color).
pub(crate) const CLEAR: Color = Color::rgb(0.06, 0.07, 0.09);

/// Decode one IEC 61966-2-1 sRGB channel to linear light.
pub(crate) fn srgb_channel_to_linear(encoded: f32) -> f32 {
    if encoded <= 0.04045 {
        encoded / 12.92
    } else {
        ((encoded + 0.055) / 1.055).powf(2.4)
    }
}

/// Channels to upload or clear with for `format`.
///
/// sRGB formats blend and store in linear light, then encode on write, so
/// authored sRGB channels are decoded here. Other formats store the authored
/// channels directly. Alpha is coverage and is never decoded.
pub(crate) fn color_for_surface(color: Color, format: wgpu::TextureFormat) -> [f32; 4] {
    if format.is_srgb() {
        [
            srgb_channel_to_linear(color.r),
            srgb_channel_to_linear(color.g),
            srgb_channel_to_linear(color.b),
            color.a,
        ]
    } else {
        [color.r, color.g, color.b, color.a]
    }
}

/// Clear value for `format`, in the space [`color_for_surface`] uses.
pub(crate) fn clear_color(format: wgpu::TextureFormat) -> wgpu::Color {
    let [r, g, b, a] = color_for_surface(CLEAR, format);
    wgpu::Color {
        r: f64::from(r),
        g: f64::from(g),
        b: f64::from(b),
        a: f64::from(a),
    }
}

#[cfg(test)]
mod tests {
    use rndr_core::{Color, Theme};

    use super::*;

    const SRGB_FORMATS: [wgpu::TextureFormat; 2] = [
        wgpu::TextureFormat::Bgra8UnormSrgb,
        wgpu::TextureFormat::Rgba8UnormSrgb,
    ];
    const UNORM_FORMATS: [wgpu::TextureFormat; 2] = [
        wgpu::TextureFormat::Bgra8Unorm,
        wgpu::TextureFormat::Rgba8Unorm,
    ];

    fn assert_near(actual: f32, expected: f32) {
        let diff = (actual - expected).abs();
        assert!(diff <= 1.0e-5, "{actual} != {expected} (diff {diff})");
    }

    /// Hardware sRGB encode, so a test can check the stored framebuffer byte.
    fn linear_channel_to_srgb(linear: f32) -> f32 {
        if linear <= 0.0031308 {
            12.92 * linear
        } else {
            1.055 * linear.powf(1.0 / 2.4) - 0.055
        }
    }

    fn displayed(prepared: [f32; 4], format: wgpu::TextureFormat) -> [f32; 4] {
        if format.is_srgb() {
            [
                linear_channel_to_srgb(prepared[0]),
                linear_channel_to_srgb(prepared[1]),
                linear_channel_to_srgb(prepared[2]),
                prepared[3],
            ]
        } else {
            prepared
        }
    }

    #[test]
    fn srgb_transfer_decodes_known_channels() {
        assert_eq!(srgb_channel_to_linear(0.0), 0.0);
        assert_eq!(srgb_channel_to_linear(1.0), 1.0);
        assert_eq!(srgb_channel_to_linear(0.02), 0.02 / 12.92);
        assert_eq!(srgb_channel_to_linear(0.04045), 0.04045 / 12.92);
        // Mid grey and the dark theme red (8% sRGB is about 0.72% linear).
        assert_near(srgb_channel_to_linear(0.5), 0.21404114);
        assert_near(srgb_channel_to_linear(0.08), 0.007194409);
    }

    #[test]
    fn theme_canvas_matches_on_srgb_and_unorm_surfaces() {
        let canvas = Theme::default().canvas;
        assert_eq!(canvas, Color::rgb(0.08, 0.09, 0.10));
        let selection = Theme::default().selection;
        assert_eq!(selection.a, 0.88);
        let text = Theme::default().text;

        let linear_canvas = [0.007194409_f32, 0.008540382, 0.010022826, 1.0];
        let linear_clear = [0.00489631_f32, 0.00598106, 0.008540382, 1.0];

        for format in SRGB_FORMATS {
            assert!(format.is_srgb(), "{format:?} should encode on store");
            let prepared = color_for_surface(canvas, format);
            assert_near(prepared[0], linear_canvas[0]);
            assert_near(prepared[1], linear_canvas[1]);
            assert_near(prepared[2], linear_canvas[2]);
            assert_eq!(prepared[3], 1.0);
            let shown = displayed(prepared, format);
            assert_near(shown[0], canvas.r);
            assert_near(shown[1], canvas.g);
            assert_near(shown[2], canvas.b);
            assert_eq!(shown[3], canvas.a);

            let tint = color_for_surface(text, format);
            assert!(tint[0] < text.r);
            assert_eq!(tint[3], text.a);
            let shown_tint = displayed(tint, format);
            assert_near(shown_tint[0], text.r);
            assert_near(shown_tint[1], text.g);
            assert_near(shown_tint[2], text.b);

            let overlay = color_for_surface(selection, format);
            assert_eq!(overlay[3], selection.a);
            assert!(overlay[0] < selection.r);
            let shown_overlay = displayed(overlay, format);
            assert_near(shown_overlay[0], selection.r);
            assert_near(shown_overlay[1], selection.g);
            assert_near(shown_overlay[2], selection.b);
            assert_eq!(shown_overlay[3], selection.a);

            let clear = clear_color(format);
            assert_near(clear.r as f32, linear_clear[0]);
            assert_near(clear.g as f32, linear_clear[1]);
            assert_near(clear.b as f32, linear_clear[2]);
            assert_eq!(clear.a as f32, 1.0);
            let shown_clear = displayed(
                [
                    clear.r as f32,
                    clear.g as f32,
                    clear.b as f32,
                    clear.a as f32,
                ],
                format,
            );
            assert_near(shown_clear[0], CLEAR.r);
            assert_near(shown_clear[1], CLEAR.g);
            assert_near(shown_clear[2], CLEAR.b);
        }

        for format in UNORM_FORMATS {
            assert!(!format.is_srgb(), "{format:?} stores bytes as given");
            assert_eq!(
                color_for_surface(canvas, format),
                [canvas.r, canvas.g, canvas.b, canvas.a]
            );
            assert_eq!(
                color_for_surface(text, format),
                [text.r, text.g, text.b, text.a]
            );
            assert_eq!(
                color_for_surface(selection, format),
                [selection.r, selection.g, selection.b, selection.a]
            );
            let clear = clear_color(format);
            assert_eq!(clear.r as f32, CLEAR.r);
            assert_eq!(clear.g as f32, CLEAR.g);
            assert_eq!(clear.b as f32, CLEAR.b);
            assert_eq!(clear.a as f32, 1.0);
            let shown = displayed(color_for_surface(canvas, format), format);
            assert_eq!(shown, [canvas.r, canvas.g, canvas.b, canvas.a]);
        }
    }
}
