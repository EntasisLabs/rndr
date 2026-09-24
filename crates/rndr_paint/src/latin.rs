//! Proportional Latin face. Rasterized once from Inter Regular at 16px.
//! Inter is © 2020 The Inter Project Authors, SIL Open Font License 1.1.
//! Advances are in points. The atlas is one coverage sample per pixel.

use std::sync::{Arc, OnceLock};

use rndr_core::{Font, FontAtlas, GlyphBox};

include!("latin_metrics.rs");

const ATLAS: &[u8] = include_bytes!("latin_atlas.bin");

/// ASCII proportional face. Insert as a [`rndr_core::TextFace`].
#[derive(Clone, Copy, Debug, Default)]
pub struct LatinFace;

impl LatinFace {
    fn index(ch: char) -> Option<usize> {
        let code = ch as u32;
        if (32..127).contains(&code) {
            Some((code - 32) as usize)
        } else {
            None
        }
    }
}

impl Font for LatinFace {
    fn line_height(&self) -> f32 {
        LATIN_LINE
    }

    fn advance(&self, ch: char) -> f32 {
        if ch == '\n' {
            return 0.0;
        }
        Self::index(ch)
            .map(|i| LATIN_ADV[i])
            .unwrap_or(LATIN_ADV[0])
    }

    fn glyph_box(&self, ch: char) -> Option<GlyphBox> {
        let i = Self::index(ch)?;
        let w = LATIN_W[i] as f32;
        let h = LATIN_H[i] as f32;
        if w <= 0.0 || h <= 0.0 {
            return None;
        }
        Some(GlyphBox {
            w,
            h,
            bearing_x: LATIN_LEFT[i] as f32,
            bearing_y: LATIN_ASC - LATIN_TOP[i] as f32,
        })
    }

    fn glyph_uv(&self, ch: char) -> [f32; 4] {
        let Some(i) = Self::index(ch) else {
            return self.solid_uv();
        };
        let w = LATIN_W[i] as f32;
        let h = LATIN_H[i] as f32;
        if w <= 0.0 || h <= 0.0 {
            return self.solid_uv();
        }
        let col = (i as u32) % 16;
        let row = (i as u32) / 16;
        let x = col as f32 * LATIN_CELL_W as f32;
        let y = row as f32 * LATIN_CELL_H as f32;
        let aw = LATIN_ATLAS_W as f32;
        let ah = LATIN_ATLAS_H as f32;
        [x / aw, y / ah, (x + w) / aw, (y + h) / ah]
    }

    fn solid_uv(&self) -> [f32; 4] {
        let aw = LATIN_ATLAS_W as f32;
        let ah = LATIN_ATLAS_H as f32;
        let v = (ah - 0.5) / ah;
        [0.5 / aw, v, 0.5 / aw, v]
    }

    fn atlas(&self) -> Option<Arc<FontAtlas>> {
        static ATLAS_FACE: OnceLock<Arc<FontAtlas>> = OnceLock::new();
        Some(Arc::clone(ATLAS_FACE.get_or_init(|| {
            Arc::new(FontAtlas {
                width: LATIN_ATLAS_W,
                height: LATIN_ATLAS_H,
                r8: Arc::from(ATLAS),
            })
        })))
    }
}
