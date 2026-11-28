//! Real packed-atlas regressions: terminal rows are unchanged while other text
//! grows/recycles their shared atlas. No native window or principal app needed.
use std::sync::Arc;

use rndr_core::{
    App, Color, Entity, Label, Layout, MonoCache, MonoCell, MonoGrid, PlacedGlyph, Schedule, Size,
    TextEngine, TextLayout, TextSpan, TextStyle, Viewport,
};

use super::{ParleyText, TextEngineConfig};

fn setup(size: u32, max: u32) -> (App, Arc<ParleyText>, Entity) {
    let engine = Arc::new(ParleyText::with_config(TextEngineConfig {
        system_fonts: false,
        atlas_size: size,
        max_atlas: max,
    }));
    let mut app = App::new();
    app.world_mut()
        .insert_resource(engine.clone() as Arc<dyn TextEngine>);
    app.world_mut().insert_resource(Viewport::new(640.0, 480.0));
    let entity = add_grid(&mut app, "M", 28.0);
    (app, engine, entity)
}

fn add_grid(app: &mut App, text: &str, size: f32) -> Entity {
    let mut grid = MonoGrid::blank(text.chars().count() as u16, 1, size, size * 1.5);
    grid.font = TextStyle {
        family: "Inter".into(),
        size,
        ..TextStyle::default()
    };
    for (cell, ch) in grid.cells.iter_mut().zip(text.chars()) {
        *cell = MonoCell::new(ch, Color::rgb(1.0, 1.0, 1.0), grid.default_bg); // design-lint: synthetic test color
    }
    app.world_mut().spawn((
        Layout {
            width: Size::Px(500.0),
            height: Size::Px(80.0),
            ..Layout::row()
        },
        grid,
    ))
}

// Compare actual atlas texels, not ch metadata (which can look correct while UVs
// sample somebody else's glyph). Tests use a single glyph at an integral origin.
fn ink(engine: &ParleyText, glyph: &PlacedGlyph) -> Vec<u8> {
    let (width, height, bpp, bytes) = if glyph.atlas == 0 {
        let a = engine.coverage_atlas().unwrap();
        (a.width, a.height, 1, a.r8.to_vec())
    } else {
        let a = engine.color_atlas().unwrap();
        (a.width, a.height, 4, a.rgba.to_vec())
    };
    let [u0, v0, u1, v1] = glyph.uv;
    let x0 = (u0 * width as f32).round() as usize;
    let y0 = (v0 * height as f32).round() as usize;
    let x1 = (u1 * width as f32).round() as usize;
    let y1 = (v1 * height as f32).round() as usize;
    let mut pixels = Vec::new();
    for y in y0..y1 {
        pixels.extend_from_slice(
            &bytes[(y * width as usize + x0) * bpp..(y * width as usize + x1) * bpp],
        );
    }
    assert!(pixels.iter().any(|&p| p != 0), "empty glyph sample");
    pixels
}

fn glyph(app: &App, entity: Entity) -> PlacedGlyph {
    app.world().get::<MonoCache>(entity).unwrap().rows[0].glyphs[0].clone()
}

#[test]
fn unchanged_mono_rows_survive_shared_atlas_growth() {
    let (mut app, engine, terminal) = setup(48, 512);
    app.update();
    let original = ink(&engine, &glyph(&app, terminal));
    let built = app.world().get::<MonoCache>(terminal).unwrap().built;
    let before = engine.cache_epoch();
    engine.layout(
        "W",
        &TextStyle {
            family: "Inter".into(),
            size: 100.0,
            ..TextStyle::default()
        },
        &[],
        f32::INFINITY,
    );
    assert!(engine.cache_epoch() > before, "did not force growth");
    app.update();
    assert!(app.world().get::<MonoCache>(terminal).unwrap().built > built);
    assert_eq!(ink(&engine, &glyph(&app, terminal)), original);
    let warmed = app.world().get::<MonoCache>(terminal).unwrap().shaped;
    for _ in 0..8 {
        app.update();
    }
    assert_eq!(
        app.world().get::<MonoCache>(terminal).unwrap().shaped,
        warmed
    );
}

#[test]
fn unchanged_mono_rows_pin_glyphs_during_shared_text_recycling() {
    let (mut app, engine, terminal) = setup(96, 96);
    app.update();
    let original = ink(&engine, &glyph(&app, terminal));
    let built = app.world().get::<MonoCache>(terminal).unwrap().built;
    let epoch = engine.cache_epoch();
    for i in 0..96 {
        app.update();
        // Simulates newly visible chat text, with deliberately limited capacity.
        // Both text lanes share exactly the same real Parley engine.
        let ch = char::from_u32(33 + (i % 90)).unwrap().to_string();
        engine.layout(
            &ch,
            &TextStyle {
                family: "Inter".into(),
                size: 31.0,
                ..TextStyle::default()
            },
            &[],
            f32::INFINITY,
        );
        assert_eq!(ink(&engine, &glyph(&app, terminal)), original, "frame {i}");
    }
    assert!(engine.eviction_count() > 0, "did not force recycling");
    assert_eq!(
        engine.cache_epoch(),
        epoch,
        "recycling must not require an epoch bump"
    );
    assert_eq!(app.world().get::<MonoCache>(terminal).unwrap().built, built);
}

#[test]
fn later_grid_growth_repairs_earlier_grid_and_label_in_same_frame() {
    let (mut app, engine, terminal) = setup(48, 512);
    let label = app.world_mut().spawn((
        Layout {
            width: Size::Px(100.0),
            height: Size::Px(50.0),
            ..Layout::row()
        },
        Label::new("A"),
    ));
    app.update();
    let original = ink(&engine, &glyph(&app, terminal));
    let old = engine.cache_epoch();
    let large = add_grid(&mut app, "W", 100.0);
    let mut calls = 0u32;
    app.add_system(Schedule::Update, |world| {
        *world.resource_mut::<u32>().unwrap() += 1;
    });
    app.world_mut().insert_resource(calls);
    let painted = rndr_paint::frame(&mut app);
    calls = *app.world().resource::<u32>().unwrap();
    assert_eq!(calls, 1, "retry ran application update again");
    assert!(engine.cache_epoch() > old);
    assert_eq!(ink(&engine, &glyph(&app, terminal)), original);
    let stored = &app.world().get::<TextLayout>(label).unwrap().glyphs;
    let fresh = engine.layout(
        "A",
        &rndr_core::style_for(app.world(), label),
        &[],
        f32::INFINITY,
    );
    assert_eq!(stored[0].uv, fresh.glyphs[0].uv);
    assert!(!glyph(&app, large).uv.iter().all(|&v| v == 0.0));
    assert!(!painted.quads.is_empty());
    let retained = glyph(&app, terminal).uv;
    rndr_paint::frame_live(&mut app);
    assert_eq!(glyph(&app, terminal).uv, retained);
}

#[test]
fn engine_replacement_and_shaping_context_rebuild_without_row_edits() {
    let (mut app, _, terminal) = setup(128, 512);
    app.update();
    let before = app.world().get::<MonoCache>(terminal).unwrap().built;
    let engine = Arc::new(ParleyText::bundled());
    app.world_mut()
        .insert_resource(engine.clone() as Arc<dyn TextEngine>);
    app.update();
    assert_eq!(
        app.world().get::<MonoCache>(terminal).unwrap().built,
        before + 1
    );
    let grid = app.world_mut().get_mut::<MonoGrid>(terminal).unwrap();
    grid.font.size = 36.0;
    grid.cell_w = 40.0;
    app.update();
    assert_eq!(
        app.world().get::<MonoCache>(terminal).unwrap().built,
        before + 2
    );
    let grid = app.world().get::<MonoGrid>(terminal).unwrap();
    let fresh = engine.layout(
        "M",
        &grid.font,
        &[TextSpan::new(0, 1, grid.cells[0].fg)],
        f32::INFINITY,
    );
    assert_eq!(glyph(&app, terminal).uv, fresh.glyphs[0].uv);
}
