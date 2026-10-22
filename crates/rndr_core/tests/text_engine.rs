use std::sync::Arc;

use rndr_core::{
    App, Axis, BuiltinText, CapturesKeys, Caret, Chrome, Color, Focusable, Font, GlyphBox, Input,
    Label, Layout, Shell, Size, TextEngine, TextField, TextSpan, TextStyle, Viewport,
    FONT_WEIGHT_BOLD, FONT_WEIGHT_REGULAR, TEXT_ORIGIN_X, TEXT_ORIGIN_Y,
};

fn style() -> TextStyle {
    TextStyle::default()
}

#[test]
fn layout_cache_reuses_a_line_and_drops_it_after_two_idle_frames() {
    let engine = BuiltinText::new();
    let style = style();
    let width = f32::INFINITY;

    engine.layout("hello", &style, &[], width);
    engine.layout("hello", &style, &[], width);
    assert_eq!(engine.shape_count(), 1, "same line in one frame is reused");

    // A repeated line in one paragraph shares the shaped line.
    engine.layout("hello\nhello", &style, &[], width);
    assert_eq!(engine.shape_count(), 1);

    engine.layout("other", &style, &[], width);
    assert_eq!(engine.shape_count(), 2);

    // Still available on the next frame.
    engine.begin_frame();
    engine.layout("hello", &style, &[], width);
    assert_eq!(engine.shape_count(), 2);

    // Two frames with no use: the previous-frame slot is cleared.
    engine.begin_frame();
    engine.begin_frame();
    engine.layout("hello", &style, &[], width);
    assert_eq!(engine.shape_count(), 3);
}

#[test]
fn an_idle_line_is_evicted_while_a_touched_line_stays() {
    let engine = BuiltinText::new();
    let style = style();
    engine.layout("keep", &style, &[], f32::INFINITY);
    engine.layout("drop", &style, &[], f32::INFINITY);
    assert_eq!(engine.shape_count(), 2);

    engine.begin_frame();
    engine.layout("keep", &style, &[], f32::INFINITY);
    assert_eq!(engine.shape_count(), 2);

    engine.begin_frame();
    engine.layout("keep", &style, &[], f32::INFINITY);
    engine.layout("drop", &style, &[], f32::INFINITY);
    assert_eq!(engine.shape_count(), 3);
}

#[test]
fn caret_hit_test_uses_advances() {
    let engine = BuiltinText::new();
    let layout = engine.layout("ABC", &style(), &[], f32::INFINITY);
    let cell = layout.caret_xy(1).0;
    assert!(cell > 0.0);
    assert_eq!(layout.index_at(cell * 0.4, 1.0), 0);
    assert_eq!(layout.index_at(cell * 0.6, 1.0), 1);
    assert_eq!(layout.caret_xy(2).0, cell * 2.0);
    assert_eq!(layout.caret_xy(3).0, cell * 3.0);
    assert_eq!(layout.index_at(cell * 2.6, 1.0), 3);
}

struct KernFont;

impl Font for KernFont {
    fn line_height(&self) -> f32 {
        10.0
    }

    fn advance(&self, ch: char) -> f32 {
        if ch == '\n' {
            0.0
        } else {
            10.0
        }
    }

    fn kerning(&self, left: char, right: char) -> f32 {
        if left == 'r' && right == 'n' {
            8.0
        } else {
            0.0
        }
    }

    fn glyph_box(&self, ch: char) -> Option<GlyphBox> {
        if ch == ' ' || ch == '\n' {
            None
        } else {
            Some(GlyphBox {
                w: 10.0,
                h: 10.0,
                bearing_x: 0.0,
                bearing_y: 0.0,
            })
        }
    }

    fn family_name(&self) -> &'static str {
        "kern"
    }
}

#[test]
fn kerning_moves_the_caret_and_the_hit_test() {
    let engine = BuiltinText::new();
    engine.register_font("kern", Arc::new(KernFont));
    let style = TextStyle {
        family: "kern".into(),
        ..TextStyle::default()
    };
    let layout = engine.layout("rn", &style, &[], f32::INFINITY);
    assert!((layout.glyphs[1].x - 18.0).abs() < 0.01);
    assert!((layout.caret_xy(1).0 - 10.0).abs() < 0.01);
    assert!((layout.caret_xy(2).0 - 28.0).abs() < 0.01);
    // The extra 8 points sit in the advance of the pair. Without kerning, 16
    // would already be past the midpoint of 'n'.
    assert_eq!(layout.index_at(16.0, 1.0), 1);
    assert_eq!(layout.index_at(20.0, 1.0), 2);
}

#[test]
fn styled_run_sets_weight_and_a_highlight() {
    let engine = BuiltinText::new();
    let runs = [TextSpan::new(0, 1, Color::rgb(1.0, 0.0, 0.0))
        .weight(FONT_WEIGHT_BOLD)
        .highlight(Color::rgb(1.0, 1.0, 0.0))];
    let layout = engine.layout("ab", &style(), &runs, f32::INFINITY);
    assert_eq!(layout.glyphs[0].weight, FONT_WEIGHT_BOLD);
    assert_eq!(layout.glyphs[1].weight, FONT_WEIGHT_REGULAR);
    assert_eq!(layout.glyphs[0].color, Some(Color::rgb(1.0, 0.0, 0.0)));
    assert_eq!(layout.highlights.len(), 1);
    assert!(layout.highlights[0].w > 0.0);
    assert_eq!(layout.highlights[0].color, Color::rgb(1.0, 1.0, 0.0));
}

#[test]
fn fallback_list_tries_the_next_face() {
    let engine = BuiltinText::new();
    engine.register_font("only-a", Arc::new(OnlyA));
    assert_eq!(
        engine.fallbacks("only-a"),
        vec!["only-a".to_string(), "bitmap".to_string()]
    );
    let style = TextStyle {
        family: "only-a".into(),
        ..TextStyle::default()
    };
    let layout = engine.layout("ab", &style, &[], f32::INFINITY);
    assert_eq!(layout.glyphs.len(), 2);
    assert!((layout.glyphs[0].w - 4.0).abs() < 0.01);
    assert!((layout.glyphs[1].w - 16.0).abs() < 0.01);
    assert!((layout.glyphs[1].x - 4.0).abs() < 0.01);
    assert!(engine.color_atlas().is_none());
}

struct OnlyA;

impl Font for OnlyA {
    fn line_height(&self) -> f32 {
        8.0
    }

    fn advance(&self, ch: char) -> f32 {
        if ch == '\n' {
            0.0
        } else {
            4.0
        }
    }

    fn glyph_box(&self, ch: char) -> Option<GlyphBox> {
        if ch == 'a' {
            Some(GlyphBox {
                w: 4.0,
                h: 8.0,
                bearing_x: 0.0,
                bearing_y: 0.0,
            })
        } else {
            None
        }
    }

    fn family_name(&self) -> &'static str {
        "only-a"
    }
}

#[test]
fn a_field_click_hit_tests_through_the_engine() {
    let mut app = App::new();
    app.add_plugin(Shell);
    app.add_plugin(Chrome);
    app.world_mut().insert_resource(Viewport::new(240.0, 80.0));
    app.world_mut()
        .insert_resource(rndr_core::TextFace(Arc::new(KernFont)));
    let entity = app.world_mut().spawn((
        Layout {
            axis: Axis::Column,
            gap: 0.0,
            padding: 0.0,
            width: Size::Px(200.0),
            height: Size::Px(36.0),
            min_width: 0.0,
            min_height: 0.0,
        },
        Color::rgb(0.15, 0.16, 0.18),
        Label::new("rn"),
        TextField,
        CapturesKeys,
        Focusable,
    ));
    app.update();

    let width = app
        .world()
        .get::<rndr_core::TextLayout>(entity)
        .unwrap()
        .width;
    let style = rndr_core::style_of(app.world());
    let measured = rndr_core::text_engine(app.world()).measure("rn", &style, &[], f32::INFINITY);
    assert_eq!(width, measured.width);
    assert!(width > 16.0, "kerned pair is wider than one cell");

    let rect = *app.world().get::<rndr_core::Rect>(entity).unwrap();
    let input = app.world_mut().resource_mut::<Input>().unwrap();
    input.pointer.position = [rect.x + TEXT_ORIGIN_X + 16.0, rect.y + TEXT_ORIGIN_Y + 1.0];
    input.pointer.pressed = true;
    input.pointer.down = true;
    app.update();

    let caret = app.world().get::<Caret>(entity).expect("caret");
    assert_eq!(caret.index, 1);
    assert_eq!(caret.anchor, 1);
}
