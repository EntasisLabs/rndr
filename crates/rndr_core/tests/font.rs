use std::sync::Arc;

use rndr_core::{
    apply_text_layout, ensure_text_engine, field_text_width, layout_propagate, layout_text,
    layout_text_spans, measure_text, Axis, BitmapFont, Color, Font, GlyphBox, Label, Layout, Rect,
    Size, TextFace, TextLayout, TextOverflow, TextSpan, Viewport, World, CELL_W,
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

fn px(w: f32, h: f32) -> Layout {
    Layout {
        axis: Axis::Column,
        gap: 0.0,
        padding: 0.0,
        width: Size::Px(w),
        height: Size::Px(h),
        min_width: 0.0,
        min_height: 0.0,
    }
}

#[test]
fn ellipsis_stays_on_one_line_inside_the_label() {
    let mut world = World::new();
    world.insert_resource(Viewport::new(200.0, 80.0));
    ensure_text_engine(&mut world);
    let entity = world.spawn((
        px(80.0, 36.0),
        Label::new("Hello, world").overflow(TextOverflow::Ellipsis),
    ));
    layout_propagate(&mut world);
    apply_text_layout(&mut world);
    let rect = *world.get::<Rect>(entity).unwrap();
    assert_eq!(rect.w, 80.0);
    let layout = world.get::<TextLayout>(entity).unwrap();
    assert_eq!(layout.lines.len(), 1);
    let inner = field_text_width(rect.w, CELL_W);
    assert!(layout.width <= inner);
    let edges = &layout.lines[0].edges;
    let jump = edges[edges.len() - 1].0 - edges[edges.len() - 2].0;
    assert_eq!(jump, '…'.len_utf8());

    world.get_mut::<Label>(entity).unwrap().text = "Hi".to_string();
    apply_text_layout(&mut world);
    let layout = world.get::<TextLayout>(entity).unwrap();
    assert_eq!(layout.lines[0].end, 2);
    assert!(layout.glyphs.iter().all(|glyph| glyph.ch != '…'));

    world.get_mut::<Label>(entity).unwrap().text = "Hello\nworld".to_string();
    apply_text_layout(&mut world);
    let layout = world.get::<TextLayout>(entity).unwrap();
    assert_eq!(layout.lines.len(), 1);
    assert!(layout.width <= inner);
}

#[test]
fn wrap_keeps_every_line_inside_the_label() {
    let mut world = World::new();
    world.insert_resource(Viewport::new(200.0, 200.0));
    ensure_text_engine(&mut world);
    let entity = world.spawn((
        px(80.0, 80.0),
        Label::new("Hello, world").overflow(TextOverflow::Wrap),
    ));
    layout_propagate(&mut world);
    apply_text_layout(&mut world);
    let layout = world.get::<TextLayout>(entity).unwrap();
    let inner = field_text_width(80.0, CELL_W);
    assert!(layout.lines.len() > 1);
    assert!(layout.lines.iter().all(|line| line.width <= inner));
}

#[test]
fn an_ellipsis_label_hugs_a_single_line() {
    let mut world = World::new();
    world.insert_resource(Viewport::new(200.0, 200.0));
    ensure_text_engine(&mut world);
    let parent = world.spawn(px(80.0, 120.0));
    let child = world.spawn((
        Layout {
            axis: Axis::Column,
            gap: 0.0,
            padding: 0.0,
            width: Size::Fill,
            height: Size::Hug,
            min_width: 0.0,
            min_height: 0.0,
        },
        Label::new("Hello, world again").overflow(TextOverflow::Ellipsis),
    ));
    rndr_core::set_parent(&mut world, child, parent);
    layout_propagate(&mut world);
    layout_propagate(&mut world);
    let rect = *world.get::<Rect>(child).unwrap();
    assert_eq!(rect.h, 36.0);
    assert!(rect.w <= 80.0);
}
