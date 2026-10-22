use std::sync::Arc;

use rndr_core::{
    layout_text, layout_text_spans, measure_text, BitmapFont, Color, Font, GlyphBox, TextFace,
    TextLayout, TextSpan, CELL_W,
};

struct WideO;

impl Font for WideO {
    fn line_height(&self) -> f32 {
        10.0
    }

    fn advance(&self, ch: char) -> f32 {
        if ch == 'o' {
            10.0
        } else {
            4.0
        }
    }

    fn glyph_box(&self, ch: char) -> Option<GlyphBox> {
        let w = self.advance(ch);
        Some(GlyphBox {
            w,
            h: 10.0,
            bearing_x: 0.0,
            bearing_y: 0.0,
        })
    }
}

#[test]
fn bitmap_places_glyphs_on_the_cell_grid() {
    let layout = layout_text(&BitmapFont, "A B", f32::INFINITY);
    assert_eq!(layout.glyphs.len(), 2);
    assert_eq!(layout.glyphs[0].ch, 'A');
    assert_eq!(layout.glyphs[0].x, 0.0);
    assert_eq!(layout.glyphs[1].ch, 'B');
    assert_eq!(layout.glyphs[1].x, 2.0 * CELL_W);
    assert_eq!(layout.index_at(CELL_W + 1.0, 1.0), 1);
    assert_eq!(layout.caret_xy(1).0, CELL_W);
}

#[test]
fn proportional_face_wraps_on_advances() {
    let layout = layout_text(&WideO, "aao", 12.0);
    let ranges: Vec<_> = layout
        .lines
        .iter()
        .map(|line| (line.start, line.end))
        .collect();
    assert_eq!(ranges, vec![(0, 2), (2, 3)]);
    let o = layout.glyphs.iter().find(|glyph| glyph.ch == 'o').unwrap();
    assert_eq!(o.x, 0.0);
    assert_eq!(o.y, 10.0);
    assert_eq!(o.w, 10.0);
}

#[test]
fn wrap_inside_a_span_keeps_every_glyph() {
    let spans = [TextSpan {
        start: 0,
        end: 4,
        color: Color::rgb(0.2, 0.4, 0.8),
        face: 0,
        weight: None,
        highlight: None,
    }];
    let layout = layout_text_spans(&BitmapFont, "abcd", 32.0, &spans);
    assert_eq!(layout.glyphs.len(), 4);
    assert!(layout
        .glyphs
        .iter()
        .all(|glyph| glyph.color == Some(spans[0].color)));
    assert!(layout.lines.len() >= 2);
}

#[test]
fn measure_matches_layout_width_and_line_count() {
    let layout = layout_text(&WideO, "aao", 12.0);
    let measured = measure_text(&WideO, "aao", 12.0);
    assert_eq!(measured.line_count, layout.lines.len());
    assert_eq!(measured.width, layout.width);
    assert_eq!(measured.height, layout.height);
    let empty = measure_text(&BitmapFont, "", f32::INFINITY);
    let empty_layout = layout_text(&BitmapFont, "", f32::INFINITY);
    assert_eq!(empty.line_count, empty_layout.lines.len());
    assert_eq!(empty.width, empty_layout.width);
}

#[test]
fn text_face_is_what_layout_measures() {
    let face = TextFace(Arc::new(WideO));
    let layout = layout_text(face.0.as_ref(), "a", f32::INFINITY);
    assert_eq!(layout.glyphs[0].w, 4.0);
    let _stored: TextLayout = layout;
}
