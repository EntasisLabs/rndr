use rndr_core::{App, Color, FontStyle, TextStyle, Viewport, FONT_WEIGHT_REGULAR};
use rndr_paint::{paint, quads_snapshot, Frame};
use rndr_term::{feed_terminal, open_terminal, TerminalOptions, TerminalPlugin};

#[test]
fn colored_cells_paint_a_glyph_and_a_cursor() {
    let mut app = App::new();
    app.add_plugin(TerminalPlugin);
    app.world_mut()
        .insert_resource(Viewport::new(8.0 * 8.0, 8.0 * 2.0));
    let entity = app.world_mut().spawn(Color::rgb(0.0, 0.0, 0.0));
    let mut options = TerminalOptions::offline();
    options.cols = 8;
    options.rows = 2;
    options.cell = Some((8.0, 8.0));
    options.font = Some(TextStyle {
        family: "bitmap".into(),
        size: 8.0,
        weight: FONT_WEIGHT_REGULAR,
        style: FontStyle::Normal,
        ..TextStyle::default()
    });
    open_terminal(app.world_mut(), entity, options).unwrap();
    assert!(feed_terminal(app.world_mut(), entity, b"\x1b[31mHi\x1b[0m"));
    app.update();

    let painted = Frame {
        quads: paint(app.world()),
        slots: Vec::new(),
        draws: Vec::new(),
    };
    let glyphs: Vec<_> = painted
        .quads
        .iter()
        .filter(|quad| quad.uv[2] > quad.uv[0] + 0.01)
        .collect();
    assert!(glyphs.len() >= 2, "expected Hi plus the cursor glyph");
    assert!(
        glyphs
            .iter()
            .any(|quad| quad.color.r > 0.7 && quad.color.g < 0.05),
        "red glyph missing"
    );
    insta::assert_snapshot!(quads_snapshot(&painted));
}
