use rndr_core::{
    Caret, Clip, Color, Label, Rect, World, CELL_H, CELL_W, TEXT_ORIGIN_X, TEXT_ORIGIN_Y,
};
use rndr_paint::{glyph_uv, paint, solid_uv};

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
