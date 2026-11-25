//! Lucide icon coverage, baked the same way as the Inter atlas.
//!
//! The bytes are `icon_atlas.bin`. Lucide is © Lucide Contributors, ISC. The
//! license text is `Lucide-LICENSE.txt` at the crate root. Source SVGs and
//! `tools/bake_icons` reproduce the atlas. At runtime this face is pure data:
//! a Private Use scalar samples one cell and the text color tints it.

use std::sync::{Arc, OnceLock};

use rndr_core::{ColorAtlas, Font, GlyphBox, Icon};

/// Cell edge in the baked atlas, in pixels. Advances are in points at this em.
pub const ICON_CELL: u32 = 16;
/// Distance between cell origins, including a one-pixel gutter.
pub const ICON_PITCH: u32 = 17;
/// Cells across the atlas.
pub const ICON_COLS: u32 = 16;
const ICON_EM: f32 = 16.0;

const ICON_ATLAS: &[u8] = include_bytes!("icon_atlas.bin");

/// Lucide SVG source for an icon scalar. Renderers with a vector rasterizer
/// draw from this at the exact pixel size instead of scaling the 16px atlas.
pub fn icon_svg(ch: char) -> Option<&'static str> {
    let icon = Icon::from_scalar(ch)?;
    crate::icon_svgs::ICON_SVGS.get(icon.index()).copied()
}

/// Width and height of the baked coverage atlas, in pixels.
pub fn icon_atlas_size() -> (u32, u32) {
    let rows = (Icon::COUNT as u32).div_ceil(ICON_COLS);
    (ICON_COLS * ICON_PITCH, rows * ICON_PITCH)
}

/// Install the icon face on the builtin shaper.
///
/// The first call wins. Later calls are ignored. Frame and paint call this
/// before measuring, and the desktop renderer calls it before uploading atlases.
pub fn install_icons() {
    rndr_core::install_fallback_font(Arc::new(IconFace));
}

/// Coverage face for [`Icon`] scalars. Not a full text face: other scalars
/// are uncovered so the primary face keeps them.
#[derive(Clone, Copy, Debug, Default)]
pub struct IconFace;

impl IconFace {
    fn cell(icon: Icon) -> Option<(u32, u32)> {
        let (width, height) = icon_atlas_size();
        if ICON_ATLAS.len() != (width * height) as usize {
            return None;
        }
        let index = icon.index() as u32;
        let col = index % ICON_COLS;
        let row = index / ICON_COLS;
        Some((col * ICON_PITCH, row * ICON_PITCH))
    }
}

impl Font for IconFace {
    fn line_height(&self) -> f32 {
        ICON_EM
    }

    fn em_size(&self) -> f32 {
        ICON_EM
    }

    fn family_name(&self) -> &'static str {
        "rndr-icons"
    }

    fn advance(&self, ch: char) -> f32 {
        if Icon::from_scalar(ch).is_some() {
            ICON_EM
        } else {
            0.0
        }
    }

    fn covers(&self, ch: char) -> bool {
        Icon::from_scalar(ch).is_some()
    }

    fn glyph_box(&self, ch: char) -> Option<GlyphBox> {
        Icon::from_scalar(ch)?;
        Some(GlyphBox {
            w: ICON_EM,
            h: ICON_EM,
            bearing_x: 0.0,
            bearing_y: 0.0,
        })
    }

    fn glyph_uv(&self, ch: char) -> [f32; 4] {
        let Some(icon) = Icon::from_scalar(ch) else {
            return self.solid_uv();
        };
        let Some((x, y)) = Self::cell(icon) else {
            return self.solid_uv();
        };
        let (width, height) = icon_atlas_size();
        let aw = width as f32;
        let ah = height as f32;
        let w = ICON_CELL as f32;
        let h = ICON_CELL as f32;
        [
            x as f32 / aw,
            y as f32 / ah,
            (x as f32 + w) / aw,
            (y as f32 + h) / ah,
        ]
    }

    fn glyph_atlas(&self, ch: char) -> u8 {
        if Icon::from_scalar(ch).is_some() {
            2
        } else {
            0
        }
    }

    fn solid_uv(&self) -> [f32; 4] {
        let (width, height) = icon_atlas_size();
        let aw = width as f32;
        let ah = height as f32;
        let u = (aw - 0.5) / aw;
        let v = (ah - 0.5) / ah;
        [u, v, u, v]
    }

    fn atlas(&self) -> Option<Arc<rndr_core::FontAtlas>> {
        None
    }

    fn color_atlas(&self) -> Option<Arc<ColorAtlas>> {
        Some(Arc::clone(icon_color_atlas()))
    }
}

fn icon_color_atlas() -> &'static Arc<ColorAtlas> {
    static COLOR: OnceLock<Arc<ColorAtlas>> = OnceLock::new();
    COLOR.get_or_init(|| {
        let (width, height) = icon_atlas_size();
        let mut rgba = vec![0u8; ICON_ATLAS.len() * 4];
        for (index, coverage) in ICON_ATLAS.iter().enumerate() {
            let pixel = index * 4;
            rgba[pixel] = 255;
            rgba[pixel + 1] = 255;
            rgba[pixel + 2] = 255;
            rgba[pixel + 3] = *coverage;
        }
        Arc::new(ColorAtlas {
            width,
            height,
            rgba: Arc::from(rgba),
        })
    })
}

/// Icon coverage scaled to `px` pixels on a side.
///
/// The builtin face stretches the 16px cell with the font size. The desktop
/// shaper inserts this mask into its own coverage atlas at the pixel size it
/// is drawing, so the quad stays one em and the text color tints it.
pub fn icon_coverage(ch: char, px: u32) -> Option<Vec<u8>> {
    let icon = Icon::from_scalar(ch)?;
    let (x0, y0) = IconFace::cell(icon)?;
    let (width, _) = icon_atlas_size();
    let mut src = vec![0u8; (ICON_CELL * ICON_CELL) as usize];
    for y in 0..ICON_CELL {
        for x in 0..ICON_CELL {
            src[(y * ICON_CELL + x) as usize] = ICON_ATLAS[((y0 + y) * width + x0 + x) as usize];
        }
    }
    let dst_n = px.max(1);
    if dst_n == ICON_CELL {
        return Some(src);
    }
    // Bilinear, sampling cell centres: a scaled atlas cell stays smooth rather
    // than stepping into blocks. Renderers that can should use `icon_svg`.
    let sample = |x: i64, y: i64| -> f32 {
        let x = x.clamp(0, ICON_CELL as i64 - 1) as u32;
        let y = y.clamp(0, ICON_CELL as i64 - 1) as u32;
        f32::from(src[(y * ICON_CELL + x) as usize])
    };
    let ratio = ICON_CELL as f32 / dst_n as f32;
    let mut dst = vec![0u8; (dst_n * dst_n) as usize];
    for y in 0..dst_n {
        let fy = (y as f32 + 0.5) * ratio - 0.5;
        let (y0, ty) = (fy.floor() as i64, fy - fy.floor());
        for x in 0..dst_n {
            let fx = (x as f32 + 0.5) * ratio - 0.5;
            let (x0, tx) = (fx.floor() as i64, fx - fx.floor());
            let top = sample(x0, y0) * (1.0 - tx) + sample(x0 + 1, y0) * tx;
            let bottom = sample(x0, y0 + 1) * (1.0 - tx) + sample(x0 + 1, y0 + 1) * tx;
            dst[(y * dst_n + x) as usize] = (top * (1.0 - ty) + bottom * ty).round() as u8;
        }
    }
    Some(dst)
}
