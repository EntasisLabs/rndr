//! Fixed-pitch rows are shaped once and left alone when their generation holds.

use rndr_core::{App, Color, Layout, MonoCache, MonoCell, MonoGrid, MonoStyle, Size, Viewport};

fn fill(width: f32, height: f32) -> Layout {
    Layout {
        width: Size::Px(width),
        height: Size::Px(height),
        ..Layout::column()
    }
}

#[test]
fn unchanged_rows_are_not_reshaped() {
    let mut app = App::new();
    app.world_mut().insert_resource(Viewport::new(128.0, 64.0));
    let mut grid = MonoGrid::blank(4, 2, 16.0, 16.0);
    grid.cells[0] = MonoCell::new('A', Color::rgb(1.0, 0.2, 0.2), grid.default_bg);
    grid.cells[1] = MonoCell {
        style: MonoStyle {
            bold: true,
            underline: true,
            ..MonoStyle::default()
        },
        ..MonoCell::new('B', Color::rgb(0.2, 0.8, 0.3), Color::rgb(0.1, 0.1, 0.4))
    };
    let entity = app
        .world_mut()
        .spawn((fill(64.0, 32.0), Color::rgb(0.05, 0.05, 0.06), grid));

    app.update();
    let (built, shaped) = {
        let cache = app.world().get::<MonoCache>(entity).expect("cache");
        assert_eq!(cache.rows.len(), 2);
        assert!(cache.rows[0].glyphs.iter().any(|glyph| glyph.ch == 'A'));
        assert!(cache.rows[0].glyphs.iter().any(|glyph| glyph.weight == 700));
        assert!(!cache.rows[0].backgrounds.is_empty());
        assert!(!cache.rows[0].strokes.is_empty());
        assert!(cache.rows[1].glyphs.is_empty());
        (cache.built, cache.shaped)
    };
    assert_eq!(built, 2);
    assert_eq!(
        shaped, 2,
        "two style runs on the first row; a blank row does not call the engine"
    );

    app.update();
    let cache = app.world().get::<MonoCache>(entity).expect("cache");
    assert_eq!(cache.built, built);
    assert_eq!(cache.shaped, shaped);

    {
        let grid = app.world_mut().get_mut::<MonoGrid>(entity).expect("grid");
        grid.cells[4] = MonoCell::new('C', Color::rgb(0.9, 0.9, 0.2), grid.default_bg);
        grid.row_gen[1] = grid.row_gen[1].wrapping_add(1);
    }
    app.update();
    let cache = app.world().get::<MonoCache>(entity).expect("cache");
    assert_eq!(cache.built, built + 1);
    assert_eq!(cache.shaped, shaped + 1);
    assert_eq!(cache.rows[0].gen, 1);
    assert!(cache.rows[1].glyphs.iter().any(|glyph| glyph.ch == 'C'));
}
