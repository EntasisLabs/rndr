use rndr_core::{
    set_parent, App, Caret, Clip, Color, DirtyMark, Hidden, Label, Layout, Rect, Size, Spans,
    TextLayout, TextSpan, TextStyle, Theme, Viewport, World, TEXT_ORIGIN_X, TEXT_ORIGIN_Y,
};
use rndr_paint::{frame, paint, solid_uv, Quad};

fn rect() -> Rect {
    Rect {
        x: 10.0,
        y: 20.0,
        w: 160.0,
        h: 48.0,
    }
}

fn assert_glyphs(quads: &[Quad], color: Color) {
    assert_eq!(
        quads.len(),
        2,
        "AB must produce two glyphs, not a background"
    );
    for quad in quads {
        assert_ne!(quad.uv, solid_uv(), "must sample glyph coverage");
        assert!(quad.uv[2] > quad.uv[0] && quad.uv[3] > quad.uv[1]);
        assert_eq!(quad.color, color);
        assert!(quad.color.a > 0.0);
        assert!(quad.rect.w > 0.0 && quad.rect.h > 0.0);
        assert!(quad.rect.x >= rect().x + TEXT_ORIGIN_X);
        assert!(quad.rect.y >= rect().y + TEXT_ORIGIN_Y);
        assert_eq!(quad.clip, Clip::from_rect(rect()));
    }
    assert!(quads[1].rect.x > quads[0].rect.x);
}

#[test]
fn fallback_label_without_background_emits_real_glyph_geometry() {
    let mut world = World::new();
    let entity = world.spawn((rect(), Label::new("AB"), TextStyle::default()));
    assert_glyphs(&paint(&world), Theme::default().text);
    assert!(
        !world.has::<TextLayout>(entity),
        "paint must not mutate the world"
    );
}

#[test]
fn frame_lays_out_and_paints_an_uncolored_label() {
    let mut app = App::new();
    app.world_mut().insert_resource(Viewport::new(300.0, 200.0));
    let entity = app.world_mut().spawn((
        Layout {
            width: Size::Px(160.0),
            height: Size::Px(48.0),
            ..Layout::column()
        },
        Label::new("AB"),
        TextStyle::default(),
    ));
    let painted = frame(&mut app);
    assert!(app.world().has::<TextLayout>(entity));
    assert_eq!(painted.quads.len(), 2);
    assert!(painted.quads.iter().all(|q| q.uv != solid_uv()
        && q.rect.w > 0.0
        && q.rect.h > 0.0
        && q.color == Theme::default().text));
}

#[test]
fn explicit_background_alpha_does_not_control_glyph_alpha() {
    for background in [Color::rgba(0.2, 0.3, 0.4, 0.0), Color::rgb(0.2, 0.3, 0.4)] {
        let mut world = World::new();
        world.spawn((rect(), background, Label::new("AB")));
        let quads = paint(&world);
        assert_eq!(quads.len(), 3);
        assert_eq!(quads[0].uv, solid_uv());
        assert_eq!(quads[0].color, background);
        assert_glyphs(&quads[1..], Theme::default().text);
    }
}

#[test]
fn label_color_and_theme_work_without_background() {
    let mut world = World::new();
    let color = Color::rgb(0.8, 0.1, 0.2);
    world.spawn((rect(), Label::new("AB").with_color(color)));
    assert_glyphs(&paint(&world), color);
    let mut themed = World::new();
    themed.insert_resource(Theme {
        text: color,
        ..Theme::default()
    });
    themed.spawn((rect(), Label::new("AB")));
    assert_glyphs(&paint(&themed), color);
}

#[test]
fn hidden_and_clipped_uncolored_labels_stay_hidden() {
    let mut world = World::new();
    world.spawn((rect(), Label::new("AB"), Hidden));
    world.spawn((
        rect(),
        Label::new("AB"),
        Clip {
            x: 0.0,
            y: 0.0,
            w: 1.0,
            h: 1.0,
        },
    ));
    assert!(paint(&world).is_empty());
}

#[test]
fn text_marks_keep_their_geometry_and_order_without_a_background() {
    let mut world = World::new();
    let entity = world.spawn((
        rect(),
        Label::new("AB"),
        Caret {
            anchor: 0,
            index: 1,
        },
        DirtyMark,
        Spans {
            runs: vec![
                TextSpan::new(0, 2, Color::rgb(0.2, 0.8, 0.4)).highlight(Color::rgb(0.3, 0.2, 0.1))
            ],
        },
    ));
    let without = paint(&world);
    assert!(
        without.len() > 4,
        "must include marks, selection, glyphs, and caret"
    );
    world.insert(entity, Color::rgba(0.0, 0.0, 0.0, 0.0));
    let with = paint(&world);
    assert_eq!(with.len(), without.len() + 1);
    assert_eq!(with[0].color.a, 0.0, "focus must preserve background alpha");
    for (actual, expected) in without.iter().zip(&with[1..]) {
        assert_eq!(actual.rect, expected.rect);
        assert_eq!(actual.color, expected.color);
        assert_eq!(actual.uv, expected.uv);
        assert_eq!(actual.clip, expected.clip);
        assert_eq!(actual.radius, expected.radius);
        assert_eq!(actual.atlas, expected.atlas);
    }
}

#[test]
fn uncolored_containers_do_not_fill_or_block_children() {
    let mut world = World::new();
    let parent = world.spawn((rect(),));
    let child = world.spawn((rect(), Label::new("AB")));
    set_parent(&mut world, child, parent);
    assert_glyphs(&paint(&world), Theme::default().text);
}
