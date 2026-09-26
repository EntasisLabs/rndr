use rndr_core::{layout_text, Font, Spans, TextSpan};
use rndr_core::{
    App, Caret, Clip, Color, Label, Radius, Rect, Schedule, Theme, World, CELL_H, CELL_W,
    TEXT_ORIGIN_X, TEXT_ORIGIN_Y,
};
use rndr_paint::{frame, glyph_uv, paint, solid_uv, LatinFace};

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
    let color = Color::rgb(0.2, 0.22, 0.24);
    world.spawn((rect(), color, Label::new("A B")));

    let quads = paint(&world);
    assert_eq!(quads.len(), 3);

    let solid = quads[0];
    assert_eq!(solid.rect, rect());
    assert_eq!(solid.color.r, color.r);
    assert_eq!(solid.color.g, color.g);
    assert_eq!(solid.color.b, color.b);
    assert_eq!(solid.color.a, color.a);
    assert_eq!(solid.uv, solid_uv());
    assert_eq!(solid.clip, Clip::from_rect(rect()));

    let a = quads[1];
    assert_eq!(
        a.rect,
        Rect {
            x: rect().x + TEXT_ORIGIN_X,
            y: rect().y + TEXT_ORIGIN_Y,
            w: CELL_W,
            h: CELL_H,
        }
    );
    assert_eq!(a.uv, glyph_uv(b'A'));

    let b = quads[2];
    assert_eq!(
        b.rect,
        Rect {
            x: rect().x + TEXT_ORIGIN_X + 2.0 * CELL_W,
            y: rect().y + TEXT_ORIGIN_Y,
            w: CELL_W,
            h: CELL_H,
        }
    );
    assert_eq!(b.uv, glyph_uv(b'B'));
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
                },
                TextSpan {
                    start: 1,
                    end: 2,
                    color: Color::rgb(0.0, 0.0, 1.0),
                    face: 1,
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
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].rect.x, 4.0);
    assert_eq!(first[0].rect.y, 8.0);

    let second = frame(&mut app);
    assert_eq!(second.len(), 1);
}
