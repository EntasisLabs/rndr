use rndr_core::{
    App, Buffer, CapturesKeys, Caret, CaretRow, Chrome, Color, Editor, Entity, Focus, Input, Key,
    Label, Layout, Rect, Size, Spans, TextLayout, TextSpan, Theme, Viewport, World, TEXT_ORIGIN_X,
    TEXT_ORIGIN_Y,
};
use rndr_paint::{paint, solid_uv, Quad};

fn boot(text: &str) -> (App, Entity) {
    let mut app = App::new();
    app.add_plugin(Chrome);
    app.world_mut().insert_resource(Viewport::new(300.0, 100.0));
    app.world_mut().insert_resource(Focus::default());
    let entity = app.world_mut().spawn((
        Layout {
            width: Size::Px(280.0),
            height: Size::Px(80.0),
            ..Layout::column()
        },
        Label::new(""),
        Buffer::from_str(text),
        Editor::default(),
        CapturesKeys,
    ));
    app.world_mut().resource_mut::<Focus>().unwrap().entity = Some(entity);
    app.update();
    app.update();
    (app, entity)
}

fn caret_quad(world: &World) -> Quad {
    let color = Theme::default().caret;
    *paint(world)
        .iter()
        .find(|quad| quad.uv == solid_uv() && quad.color == color && quad.rect.w == 2.0)
        .expect("visible caret quad")
}

#[test]
fn up_at_wrap_endpoint_paints_the_exact_upper_row_end() {
    let (mut app, e) = boot(&"a".repeat(32));
    app.world_mut().resource_mut::<Input>().unwrap().key = Some(Key::Up);
    app.update();
    let layout = app.world().get::<TextLayout>(e).unwrap();
    let caret = app.world().get::<Caret>(e).unwrap();
    let hint = app.world().get::<CaretRow>(e).unwrap();
    assert_eq!(caret.index, 16);
    assert_eq!(hint.row, 0);
    let rect = *app.world().get::<Rect>(e).unwrap();
    let quad = caret_quad(app.world());
    assert_eq!(quad.rect.x, rect.x + TEXT_ORIGIN_X + layout.lines[0].width);
    assert_eq!(quad.rect.y, rect.y + TEXT_ORIGIN_Y);
    app.world_mut().resource_mut::<Input>().unwrap().key = Some(Key::Down);
    app.update();
    assert_eq!(app.world().get::<Buffer>(e).unwrap().caret, 32);
    assert_eq!(
        caret_quad(app.world()).rect.y,
        rect.y + TEXT_ORIGIN_Y + 16.0
    );
}

#[test]
fn eof_typing_emits_a_caret_quad_inside_the_editor_content_clip() {
    let (mut app, e) = boot(&"line\n".repeat(100));
    assert!(app.world().get::<Caret>(e).is_none());
    app.world_mut().resource_mut::<Input>().unwrap().chars = "é".into();
    app.update();
    assert!(app.world().get::<Editor>(e).unwrap().offset > 0.0);
    let rect = app.world().get::<Rect>(e).unwrap();
    let quad = caret_quad(app.world());
    assert!(quad.rect.y >= rect.y + TEXT_ORIGIN_Y);
    assert!(quad.rect.y + quad.rect.h <= rect.y + rect.h - TEXT_ORIGIN_Y);
    assert_eq!(quad.clip.y, rect.y + TEXT_ORIGIN_Y);
    assert_eq!(quad.clip.h, rect.h - 2.0 * TEXT_ORIGIN_Y);
}

#[test]
fn scrolled_document_spans_rebase_glyph_colors_highlights_and_underlines() {
    let (mut app, e) = boot(&"abc\n".repeat(100));
    let tint = Color::rgb(0.9, 0.2, 0.1);
    let source = TextSpan::new(40, 43, tint)
        .underline()
        .highlight(Color::rgb(0.2, 0.1, 0.1));
    app.world_mut().insert(
        e,
        Spans {
            runs: vec![source.clone()],
        },
    );
    app.world_mut().get_mut::<Editor>(e).unwrap().offset = 10.0 * 16.0;
    app.update();
    let layout = app.world().get::<TextLayout>(e).unwrap();
    assert!(layout
        .glyphs
        .iter()
        .take(3)
        .all(|glyph| glyph.color == Some(tint)));
    assert_eq!(layout.highlights.len(), 1);
    assert_eq!(layout.highlights[0].y, 0.0);
    let rect = app.world().get::<Rect>(e).unwrap();
    let quads = paint(app.world());
    let underline = quads
        .iter()
        .find(|quad| quad.uv == solid_uv() && quad.color == tint)
        .expect("rebased underline");
    assert_eq!(underline.rect.x, rect.x + TEXT_ORIGIN_X);
    assert_eq!(underline.rect.y, rect.y + TEXT_ORIGIN_Y + 15.0);
    assert_eq!(app.world().get::<Spans>(e).unwrap().runs, vec![source]);
    // The fallback paint path must use identical wrapping and span mapping.
    app.world_mut().remove::<TextLayout>(e);
    let fallback = paint(app.world());
    assert_eq!(fallback.len(), quads.len());
    for (actual, expected) in fallback.iter().zip(&quads) {
        assert_eq!(actual.rect, expected.rect);
        assert_eq!(actual.color, expected.color);
        assert_eq!(actual.uv, expected.uv);
        assert_eq!(actual.clip, expected.clip);
        assert_eq!(actual.atlas, expected.atlas);
    }
}

#[test]
fn stale_row_hint_is_ignored_and_ime_cursor_wins_over_affinity() {
    let mut world = World::new();
    let rect = Rect {
        x: 0.0,
        y: 0.0,
        w: 280.0,
        h: 80.0,
    };
    let text = "a".repeat(32);
    let e = world.spawn((
        rect,
        Label::new(&text),
        Editor::default(),
        Caret {
            index: 16,
            anchor: 16,
        },
        CaretRow { index: 15, row: 0 },
    ));
    let mut layout = rndr_core::layout_of(&world, e, &text);
    world.insert(e, layout.clone());
    assert_eq!(caret_quad(&world).rect.y, TEXT_ORIGIN_Y + 16.0);
    world.insert(e, CaretRow { index: 16, row: 0 });
    assert_eq!(caret_quad(&world).rect.y, TEXT_ORIGIN_Y);
    layout.ime_cursor = Some(17);
    world.insert(e, layout);
    assert_eq!(caret_quad(&world).rect.y, TEXT_ORIGIN_Y + 16.0);
}
