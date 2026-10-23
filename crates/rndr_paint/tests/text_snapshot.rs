use rndr_core::{layout_text, Color, Font, Label, Rect, Spans, TextSpan, World};
use rndr_paint::{paint, quads_snapshot, Frame, LatinFace};

fn rect() -> Rect {
    Rect {
        x: 10.0,
        y: 20.0,
        w: 160.0,
        h: 36.0,
    }
}

#[test]
fn styled_label_snapshot_has_two_colors_and_a_highlight() {
    let mut world = World::new();
    let label_color = Color::rgb(0.55, 0.56, 0.58);
    let go = Color::rgb(0.93, 0.94, 0.96);
    let hint = Color::rgb(0.45, 0.62, 0.95);
    let mark = Color::rgba(0.95, 0.78, 0.25, 0.85);
    world.spawn((
        rect(),
        Color::rgb(0.16, 0.17, 0.19),
        Label::new("GoXnow").with_color(label_color),
        Spans {
            runs: vec![
                TextSpan::new(0, 2, go),
                TextSpan::new(3, 6, hint).highlight(mark),
            ],
        },
    ));

    let painted = Frame {
        quads: paint(&world),
        slots: Vec::new(),
        draws: Vec::new(),
    };
    insta::assert_snapshot!(quads_snapshot(&painted));

    let glyphs: Vec<_> = painted
        .quads
        .iter()
        .filter(|quad| quad.uv[2] > quad.uv[0] + 0.01)
        .collect();
    assert_eq!(glyphs.len(), 6);
    assert_eq!(glyphs[0].color, go);
    assert_eq!(glyphs[2].color, label_color);
    assert_eq!(glyphs[3].color, hint);
    assert!(painted.quads.iter().any(|quad| quad.color == mark));
}

#[test]
fn latin_memory_uses_real_advances_and_rn_kerning() {
    let memory = layout_text(&LatinFace, "memory", f32::INFINITY);
    let e = memory.glyphs.iter().find(|glyph| glyph.ch == 'e').unwrap();
    // 'm' is 12px of ink with a 1px left bearing. The old advance of 10
    // pulled 'e' back into that ink.
    assert!(e.x > 13.5, "e starts at {}", e.x);

    let pair = layout_text(&LatinFace, "rn", f32::INFINITY);
    let kern = LatinFace.kerning('r', 'n');
    assert!(kern > 0.2, "r-n kern {kern}");
    // The caret after 'r' sits on r's advance. Kerning is applied when the
    // pen enters 'n', so it shows up in n's glyph and in the caret after n.
    let after_r = pair.caret_xy(1).0;
    assert!((after_r - LatinFace.advance('r')).abs() < 0.001);
    let n = pair.glyphs.iter().find(|glyph| glyph.ch == 'n').unwrap();
    // Ink starts at the kerned pen plus n's stored left bearing (1px).
    let bearing = n.x - (after_r + kern);
    assert!(
        (bearing - 1.0).abs() < 0.001,
        "n ink at {}, bearing {bearing}",
        n.x
    );
    let after_n = pair.caret_xy(2).0;
    let expected = after_r + kern + LatinFace.advance('n');
    assert!(
        (after_n - expected).abs() < 0.001,
        "end pen {after_n} expected {expected}"
    );

    let mid = after_r * 0.5;
    assert_eq!(pair.index_at(mid - 0.1, 1.0), 0);
    assert_eq!(pair.index_at(mid + 0.1, 1.0), 1);
    let between = (after_r + after_n) * 0.5;
    assert_eq!(pair.index_at(between + 0.1, 1.0), 2);
}

#[test]
fn latin_caret_tracks_the_pen_not_the_cell_grid() {
    let layout = layout_text(&LatinFace, "memory", f32::INFINITY);
    let x = layout.caret_xy(1).0;
    assert_eq!(layout.index_at(x * 0.25, 1.0), 0);
    assert_eq!(layout.index_at(x * 0.75, 1.0), 1);
}
