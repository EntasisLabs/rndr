use std::sync::Arc;

use rndr_core::{
    text_engine, App, Axis, Chrome, Color, Label, Layout, List, ListRows, Rect, Size, TextEngine,
    TextFace, TextStyle, Viewport, World, FONT_WEIGHT_REGULAR,
};
use rndr_paint::{frame, quads_snapshot, LatinFace};

use super::{ParleyText, TextEngineConfig};

fn inter(size: f32) -> TextStyle {
    TextStyle {
        family: "Inter".into(),
        size,
        weight: FONT_WEIGHT_REGULAR,
        style: rndr_core::FontStyle::Normal,
        ..TextStyle::default()
    }
}

fn bundled() -> ParleyText {
    ParleyText::bundled()
}

#[test]
fn latin_kerning_pulls_av_together() {
    let engine = bundled();
    let style = inter(16.0);
    let pair = engine.layout("AV", &style, &[], f32::INFINITY);
    let a = engine.layout("A", &style, &[], f32::INFINITY);
    let v = engine.layout("V", &style, &[], f32::INFINITY);
    assert!(pair.glyphs.len() >= 2, "glyphs {}", pair.glyphs.len());
    let apart = a.width + v.width;
    assert!(pair.width < apart - 0.2, "AV {} vs A+V {apart}", pair.width);
}

#[test]
fn arabic_runs_right_to_left() {
    let engine = bundled();
    let text = "مرحبا";
    let layout = engine.layout(text, &inter(16.0), &[], f32::INFINITY);
    assert!(layout.glyphs.len() >= 2, "glyphs {}", layout.glyphs.len());
    let first = layout.glyphs.iter().find(|glyph| glyph.start == 0).unwrap();
    let last_start = text.chars().last().unwrap().len_utf8();
    let last_at = text.len() - last_start;
    let last = layout
        .glyphs
        .iter()
        .find(|glyph| glyph.start == last_at)
        .unwrap();
    assert!(
        first.x > last.x,
        "first glyph at {} should sit right of last at {}",
        first.x,
        last.x
    );
}

#[test]
fn cjk_breaks_inside_a_narrow_measure() {
    let engine = bundled();
    let text = "漢字".repeat(12);
    let layout = engine.layout(&text, &inter(16.0), &[], 40.0);
    assert!(
        layout.lines.len() > 1,
        "lines {} width {}",
        layout.lines.len(),
        layout.width
    );
    assert!(layout.width <= 40.0 + 1.0, "width {}", layout.width);
}

#[test]
fn emoji_flag_is_one_color_cluster() {
    let engine = bundled();
    let flag = "🇺🇸";
    let layout = engine.layout(flag, &inter(16.0), &[], f32::INFINITY);
    assert!(
        layout.glyphs.iter().any(|glyph| glyph.atlas == 1),
        "expected a color glyph, got {:?}",
        layout
            .glyphs
            .iter()
            .map(|glyph| (glyph.ch, glyph.atlas, glyph.start, glyph.end))
            .collect::<Vec<_>>()
    );
    let edges = &layout.lines[0].edges;
    assert_eq!(edges.first().map(|edge| edge.0), Some(0));
    assert_eq!(edges.last().map(|edge| edge.0), Some(flag.len()));
    assert_eq!(edges.len(), 2, "edges {edges:?}");
}

#[test]
fn scale_change_reshapes() {
    let engine = bundled();
    let style = inter(16.0);
    engine.layout("Hello", &style, &[], f32::INFINITY);
    let once = engine.shape_count();
    engine.layout("Hello", &style, &[], f32::INFINITY);
    assert_eq!(engine.shape_count(), once);
    engine.set_pixels_per_point(2.0);
    engine.layout("Hello", &style, &[], f32::INFINITY);
    assert!(engine.shape_count() > once);
}

#[test]
fn idle_frames_keep_packed_glyphs() {
    let engine = bundled();
    let style = inter(16.0);
    let first = engine.layout("A", &style, &[], f32::INFINITY);
    assert!(!first.glyphs.is_empty());
    let rasters = engine.raster_count();
    let evicted = engine.eviction_count();
    assert!(rasters > 0);
    for _ in 0..8 {
        engine.begin_frame();
    }
    let again = engine.layout("A", &style, &[], f32::INFINITY);
    assert_eq!(again.glyphs.len(), first.glyphs.len());
    assert_eq!(
        engine.raster_count(),
        rasters,
        "leaving the line cache re-rasterized a glyph that was still packed"
    );
    assert_eq!(engine.eviction_count(), evicted);
}

#[test]
fn unused_glyphs_are_evicted_when_the_shelf_is_full() {
    let engine = ParleyText::with_config(TextEngineConfig {
        system_fonts: false,
        atlas_size: 48,
        max_atlas: 48,
    });
    let style = inter(28.0);
    let first = engine.layout("M", &style, &[], f32::INFINITY);
    assert!(
        !first.glyphs.is_empty(),
        "M did not rasterize into a 48px atlas"
    );
    engine.begin_frame();
    engine.begin_frame();
    let before = engine.eviction_count();
    let second = engine.layout("W", &style, &[], f32::INFINITY);
    assert!(
        engine.eviction_count() > before,
        "evictions {} glyphs {:?}",
        engine.eviction_count(),
        second
            .glyphs
            .iter()
            .map(|glyph| (glyph.ch, glyph.w, glyph.h))
            .collect::<Vec<_>>()
    );
    assert!(!second.glyphs.is_empty());
}

#[test]
fn uploads_are_dirty_rects_until_the_atlas_grows() {
    let engine = bundled();
    let _ = engine.take_uploads();
    engine.layout("A", &inter(16.0), &[], f32::INFINITY);
    let dirty = engine.take_uploads();
    assert!(!dirty.coverage.full);
    assert!(!dirty.coverage.rects.is_empty());
    let again = engine.take_uploads();
    assert!(again.coverage.rects.is_empty());
    assert!(!again.coverage.full);
}

#[test]
fn scrolling_a_virtual_list_reshapes_only_new_rows() {
    let engine = Arc::new(bundled());
    let shared: Arc<dyn TextEngine> = Arc::clone(&engine) as Arc<dyn TextEngine>;
    let mut app = App::new();
    app.add_plugin(Chrome);
    app.world_mut().insert_resource(Arc::clone(&shared));
    app.world_mut().insert_resource(Viewport::new(200.0, 240.0));
    let list = app.world_mut().spawn((
        Layout {
            axis: Axis::Column,
            gap: 0.0,
            padding: 0.0,
            width: Size::Px(160.0),
            height: Size::Px(200.0),
            min_width: 0.0,
            min_height: 0.0,
        },
        Rect {
            x: 0.0,
            y: 0.0,
            w: 160.0,
            h: 200.0,
        },
        Color::rgb(0.1, 0.1, 0.12),
        List::new(1_000, 20.0),
        ListRows(fill_row),
    ));
    app.update();
    app.update();
    let warmed = shared.shape_count();
    assert!(warmed > 0, "rows were not shaped");
    app.update();
    assert_eq!(shared.shape_count(), warmed, "unchanged rows reshaped");

    app.world_mut().get_mut::<List>(list).unwrap().offset += 20.0;
    app.update();
    let scrolled = shared.shape_count();
    assert_eq!(scrolled - warmed, 1, "scroll shaped more than the new row");
    app.update();
    assert_eq!(shared.shape_count(), scrolled);
}

fn fill_row(world: &mut World, row: rndr_core::Entity, index: u32) {
    if let Some(label) = world.get_mut::<Label>(row) {
        label.text = format!("row {index}");
    }
}

#[test]
fn bundled_hello_and_resume_snapshots() {
    insta::assert_snapshot!("hello", paint_label("Hello"));
    insta::assert_snapshot!("resume", paint_label("Résumé"));
}

fn paint_label(text: &str) -> String {
    let mut app = App::new();
    let engine: Arc<dyn TextEngine> = Arc::new(bundled());
    app.world_mut().insert_resource(engine);
    app.world_mut()
        .insert_resource(TextFace(Arc::new(LatinFace)));
    app.world_mut().insert_resource(Viewport::new(220.0, 80.0));
    app.world_mut().spawn((
        Layout {
            axis: Axis::Column,
            gap: 0.0,
            padding: 0.0,
            width: Size::Px(200.0),
            height: Size::Px(48.0),
            min_width: 0.0,
            min_height: 0.0,
        },
        Color::rgb(0.12, 0.13, 0.15),
        Label::new(text),
    ));
    quads_snapshot(&frame(&mut app))
}

#[test]
fn hard_break_left_edge_stays_on_that_line() {
    let engine = bundled();
    let text = "line 5\nline 6";
    let layout = engine.layout(text, &inter(16.0), &[], 400.0);
    assert!(layout.lines.len() >= 2, "lines {}", layout.lines.len());
    for line in &layout.lines {
        if line.start == line.end {
            continue;
        }
        let y = line.y + 1.0;
        assert_eq!(
            layout.index_at(-8.0, y),
            line.start,
            "edges {:?}",
            line.edges
        );
        assert_eq!(
            layout.index_at(0.0, y),
            line.start,
            "edges {:?}",
            line.edges
        );
        let next = layout
            .lines
            .iter()
            .find(|other| other.start > line.start)
            .map(|other| other.start);
        for &(index, x) in &line.edges {
            if next == Some(index) {
                assert!(
                    x > line.width * 0.5,
                    "next line's byte {index} aliased to x={x} on {:?}",
                    line.edges
                );
            }
        }
    }
}

#[test]
fn wrapped_line_start_stays_on_that_line() {
    let engine = bundled();
    let text = "alpha beta gamma delta epsilon zeta eta theta iota kappa";
    let layout = engine.layout(text, &inter(16.0), &[], 80.0);
    assert!(layout.lines.len() >= 2, "lines {}", layout.lines.len());
    let first = &layout.lines[0];
    let second = &layout.lines[1];
    assert_eq!(layout.index_at(-2.0, first.y + 1.0), first.start);
    assert_eq!(layout.index_at(0.0, first.y + 1.0), first.start);
    assert_eq!(layout.index_at(0.0, second.y + 1.0), second.start);
    let boxes = layout.selection_boxes(first.start, first.end);
    assert!(!boxes.is_empty());
    assert!(
        boxes.iter().all(|span| span.w <= first.width + 1.0),
        "selection {:?} wider than line {}",
        boxes,
        first.width
    );
}

#[test]
fn engine_in_the_world_is_the_one_paint_uses() {
    let mut app = App::new();
    let engine: Arc<dyn TextEngine> = Arc::new(bundled());
    app.world_mut().insert_resource(Arc::clone(&engine));
    app.update();
    let installed = text_engine(app.world());
    assert!(installed.as_any().downcast_ref::<ParleyText>().is_some());
}
