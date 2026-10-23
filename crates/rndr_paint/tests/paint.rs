use rndr_core::{
    layout_text, App, Axis, Buffer, Caret, Chrome, Clip, Color, Decoration, DecorationKind,
    Decorations, Editor, Font, Label, Layout, Prompt, Radius, Rect, Schedule, Size, Spans,
    TextSpan, Theme, Viewport, World, CELL_H, CELL_W, TEXT_ORIGIN_X, TEXT_ORIGIN_Y,
};
use rndr_paint::{frame, paint, quads_snapshot, solid_uv, Frame, LatinFace};

fn rect() -> Rect {
    Rect {
        x: 10.0,
        y: 20.0,
        w: 120.0,
        h: 36.0,
    }
}

#[test]
fn labeled_rect_paints_a_solid_quad_then_glyphs() {
    let mut world = World::new();
    world.spawn((rect(), Color::rgb(0.2, 0.22, 0.24), Label::new("A B")));
    let painted = Frame {
        quads: paint(&world),
        slots: Vec::new(),
    };
    insta::assert_snapshot!(quads_snapshot(&painted));
}

#[test]
fn glyphs_outside_the_clip_are_dropped() {
    let mut world = World::new();
    world.spawn((
        rect(),
        Clip {
            x: 0.0,
            y: 0.0,
            w: 400.0,
            h: 10.0,
        },
        Color::rgb(0.2, 0.22, 0.24),
        Label::new("AB"),
    ));
    assert!(paint(&world).is_empty());
}

#[test]
fn caret_sits_on_the_glyph_column() {
    let mut world = World::new();
    world.spawn((
        rect(),
        Color::rgb(0.2, 0.22, 0.24),
        Label::new("AB"),
        Caret {
            index: 1,
            anchor: 1,
        },
    ));

    let quads = paint(&world);
    let caret = quads.last().expect("caret quad");
    assert_eq!(
        caret.rect,
        Rect {
            x: rect().x + TEXT_ORIGIN_X + CELL_W,
            y: rect().y + TEXT_ORIGIN_Y,
            w: 2.0,
            h: CELL_H,
        }
    );
    assert_eq!(caret.uv, solid_uv());
}

fn spawn_swatch(world: &mut World) {
    world.spawn((
        Rect {
            x: 4.0,
            y: 8.0,
            w: 16.0,
            h: 16.0,
        },
        Color::rgb(0.1, 0.2, 0.3),
    ));
}

#[test]
fn radius_is_on_the_solid_and_not_the_glyphs() {
    let mut world = World::new();
    world.spawn((
        rect(),
        Color::rgb(0.2, 0.22, 0.24),
        Label::new("A"),
        Radius(6.0),
    ));
    let quads = paint(&world);
    assert_eq!(quads[0].radius, 6.0);
    assert_eq!(quads[1].radius, 0.0);
    assert_eq!(quads[1].color.r, Theme::default().text.r);
}

#[test]
fn two_spans_paint_two_colors() {
    let mut world = World::new();
    world.spawn((
        rect(),
        Color::rgb(0.2, 0.22, 0.24),
        Label::new("AB"),
        Spans {
            runs: vec![
                TextSpan {
                    start: 0,
                    end: 1,
                    color: Color::rgb(1.0, 0.0, 0.0),
                    face: 0,
                    weight: None,
                    highlight: None,
                    italic: false,
                },
                TextSpan {
                    start: 1,
                    end: 2,
                    color: Color::rgb(0.0, 0.0, 1.0),
                    face: 1,
                    weight: None,
                    highlight: None,
                    italic: false,
                },
            ],
        },
    ));
    let quads = paint(&world);
    let glyphs: Vec<_> = quads
        .iter()
        .filter(|quad| quad.radius == 0.0 && quad.uv != solid_uv())
        .collect();
    assert_eq!(glyphs.len(), 2);
    assert_eq!(glyphs[0].color.r, 1.0);
    assert_eq!(glyphs[1].color.b, 1.0);
}

#[test]
fn find_on_line_50_paints_only_when_visible() {
    let mut hidden = editor_with_find(0.0);
    let mut shown = editor_with_find(49.0 * CELL_H);
    let selection = Theme::default().selection;
    let count = |app: &mut App| {
        frame(app)
            .quads
            .into_iter()
            .filter(|quad| quad.color == selection)
            .count()
    };
    assert_eq!(count(&mut hidden), 0);
    assert_eq!(count(&mut shown), 1);
}

fn editor_with_find(offset: f32) -> App {
    let mut app = App::new();
    app.add_plugin(Chrome);
    app.world_mut().insert_resource(Viewport::new(300.0, 200.0));
    app.world_mut().spawn((
        Layout {
            axis: Axis::Column,
            gap: 0.0,
            padding: 0.0,
            width: Size::Px(280.0),
            height: Size::Px(200.0),
            min_width: 0.0,
            min_height: 0.0,
        },
        Color::rgb(0.14, 0.15, 0.16),
        Label::new(""),
        Buffer::from_str(&"a\n".repeat(80)),
        Editor { offset },
        Decorations {
            list: vec![Decoration {
                start: 49 * 2,
                end: 49 * 2 + 1,
                kind: DecorationKind::Find,
            }],
        },
    ));
    app.update();
    app.update();
    app
}

#[test]
fn latin_i_is_narrower_than_m() {
    assert!(LatinFace.advance('i') < LatinFace.advance('M'));
    let layout = layout_text(&LatinFace, "iM", f32::INFINITY);
    assert!(layout.glyphs[0].w < layout.glyphs[1].w);
    assert!(layout.glyphs[0].uv[2] > layout.glyphs[0].uv[0]);
    let atlas = LatinFace.atlas().expect("atlas");
    assert_eq!(atlas.r8.len(), (atlas.width * atlas.height) as usize);
    assert_eq!(
        atlas.r8[(atlas.height as usize - 1) * atlas.width as usize],
        255
    );
}

#[test]
fn frame_runs_startup_once_then_paints() {
    let mut app = App::new();
    app.add_system(Schedule::Startup, spawn_swatch);

    let first = frame(&mut app);
    assert_eq!(first.quads.len(), 1);
    assert!(first.slots.is_empty());
    assert_eq!(first.quads[0].rect.x, 4.0);
    assert_eq!(first.quads[0].rect.y, 8.0);

    let second = frame(&mut app);
    assert_eq!(second.quads.len(), 1);
}

#[test]
fn prompt_scroll_shifts_the_caret() {
    let mut world = World::new();
    let text = "a\nb\nc";
    world.spawn((
        rect(),
        Color::rgb(0.2, 0.22, 0.24),
        Label::new(text),
        Caret {
            index: text.len(),
            anchor: text.len(),
        },
        Prompt {
            scroll: CELL_H,
            ..Prompt::new()
        },
    ));
    let quads = paint(&world);
    let caret = quads.last().expect("caret quad");
    assert_eq!(
        caret.rect,
        Rect {
            x: rect().x + TEXT_ORIGIN_X + CELL_W,
            y: rect().y + TEXT_ORIGIN_Y + 2.0 * CELL_H - CELL_H,
            w: 2.0,
            h: CELL_H,
        }
    );
}

#[test]
fn prompt_text_is_clipped_to_the_content_band() {
    let mut world = World::new();
    world.spawn((
        rect(),
        Color::rgb(0.2, 0.22, 0.24),
        Label::new("a\nb"),
        Caret {
            index: 0,
            anchor: 0,
        },
        Prompt {
            scroll: CELL_H,
            ..Prompt::new()
        },
    ));
    let quads = paint(&world);
    let solid = solid_uv();
    let glyphs: Vec<_> = quads.iter().filter(|quad| quad.uv != solid).collect();
    assert_eq!(glyphs.len(), 1, "the line in the top padding is dropped");
    let content = Clip {
        x: rect().x,
        y: rect().y + TEXT_ORIGIN_Y,
        w: rect().w,
        h: rect().h - 2.0 * TEXT_ORIGIN_Y,
    };
    assert_eq!(glyphs[0].clip, content);
    let fill = quads.iter().find(|quad| quad.uv == solid).unwrap();
    assert_eq!(fill.clip, Clip::from_rect(rect()));
}
