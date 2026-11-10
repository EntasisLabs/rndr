//! Unique-paragraph scale characterization.
//!
//! `editor_scale_cost` repeats one line. Paragraph reuse then matches on text
//! equality inside a single build, so that probe does not show shaping cost or
//! the prefix-only paragraph cache. This fixture gives every line its own text.
//!
//! Ignored on purpose: debug CI must not run an 80k-line release probe.
//! ```text
//! cargo test -p rndr_core --release --test editor_scale_unique -- --ignored --nocapture
//! ```
//! Timings are evidence, not thresholds. `App::update` does not paint, so the
//! scrollbar column is a direct `scrollbar_geometry` loop. That query is what
//! paint calls once per scrollbar each frame.
use rndr_core::editing::*;
use rndr_core::*;
use std::time::Instant;

fn source(lines: usize) -> String {
    (0..lines)
        .map(|i| format!("row {i:06} alpha beta gamma delta\n"))
        .collect()
}

fn percentile(samples: &mut [u128], index: usize) -> u128 {
    samples.sort_unstable();
    samples[index.min(samples.len() - 1)]
}

fn boot(text: &str) -> (App, Entity, Entity, DocumentId) {
    let mut app = App::new();
    app.add_plugin(Chrome);
    let mut docs = SourceDocuments::default();
    let doc = docs.open(text);
    app.world_mut().insert_resource(docs);
    let root = app.world_mut().spawn((Layout {
        width: Size::Px(664.0),
        height: Size::Px(480.0),
        ..Layout::row()
    },));
    let editor = app.world_mut().spawn((
        Layout {
            width: Size::Px(640.0),
            height: Size::Px(480.0),
            ..Layout::column()
        },
        Label::new(""),
        Buffer::from_str(""),
        Editor::default(),
        CapturesKeys,
    ));
    set_parent(app.world_mut(), editor, root);
    let bar = app.world_mut().spawn((
        Layout {
            width: Size::Px(24.0),
            height: Size::Fill,
            ..Layout::column()
        },
        Scrollbar {
            target: editor,
            axis: Axis::Column,
        },
    ));
    set_parent(app.world_mut(), bar, root);
    attach_source(app.world_mut(), editor, doc).unwrap();
    app.world_mut().insert_resource(Viewport::new(700.0, 520.0));
    (app, editor, bar, doc)
}

#[test]
#[ignore = "opt-in release scale characterization; no timing threshold assertions"]
fn unique_line_editor_costs() {
    println!(
        "lines,bytes,cold_us,idle_p50,edit_tail_p50,resize_p50,scrollbar_geometry_us,paragraph_cache_bytes,row_geometry_bytes,visible_glyphs,label_bytes"
    );
    for lines in [2_000usize, 20_000, 80_000] {
        let text = source(lines);
        let (mut app, editor, bar, doc) = boot(&text);
        let cold = Instant::now();
        for _ in 0..3 {
            app.update();
        }
        let cold = cold.elapsed().as_micros();
        let baseline = *app.world().get::<EditorLayoutStats>(editor).unwrap();
        let mut idle = Vec::new();
        for _ in 0..11 {
            let start = Instant::now();
            app.update();
            idle.push(start.elapsed().as_micros());
        }
        let after_idle = *app.world().get::<EditorLayoutStats>(editor).unwrap();
        assert_eq!(baseline.assemblies, after_idle.assemblies);
        assert_eq!(baseline.bytes_shaped, after_idle.bytes_shaped);

        let mut edit = Vec::new();
        for i in 0..5 {
            let start = Instant::now();
            let session = app
                .world_mut()
                .resource_mut::<SourceDocuments>()
                .unwrap()
                .get_mut(doc)
                .unwrap();
            let at = session.text().len() - 2;
            session
                .commit(SourceBatch {
                    document: doc,
                    base: session.revision(),
                    origin: EditOrigin::Host(1),
                    changes: vec![SourceChange {
                        range: at..at + 1,
                        replacement: if i % 2 == 0 { "Z" } else { "a" }.into(),
                    }],
                })
                .unwrap();
            app.update();
            edit.push(start.elapsed().as_micros());
            app.world_mut()
                .resource_mut::<SourceDocuments>()
                .unwrap()
                .get_mut(doc)
                .unwrap()
                .drain_events()
                .for_each(drop);
        }

        let mut resize = Vec::new();
        for i in 0..3 {
            app.world_mut().get_mut::<Layout>(editor).unwrap().width =
                Size::Px(if i % 2 == 0 { 480.0 } else { 640.0 });
            let start = Instant::now();
            for _ in 0..2 {
                app.update();
            }
            resize.push(start.elapsed().as_micros());
        }

        let geometry_before = scrollbar_geometry(app.world(), bar).unwrap();
        assert!(geometry_before.max > 0.0);
        let samples = 2_000u128;
        let start = Instant::now();
        for _ in 0..samples {
            let geometry = scrollbar_geometry(app.world(), bar).unwrap();
            assert_eq!(geometry.max, geometry_before.max);
            assert_eq!(geometry.value, geometry_before.value);
        }
        let geometry_us = start.elapsed().as_nanos() as f64 / samples as f64 / 1_000.0;

        let stats = *app.world().get::<EditorLayoutStats>(editor).unwrap();
        let visible = app.world().get::<TextLayout>(editor).unwrap().glyphs.len();
        let label = app.world().get::<Label>(editor).unwrap().text.len();
        assert!(stats.paragraph_cache_bytes <= 8 * 1024 * 1024);
        assert!(
            label < 10_000,
            "visible label grew with the document: {label}"
        );
        assert!(
            visible < 10_000,
            "visible glyphs grew with the document: {visible}"
        );
        assert!(stats.row_geometry_bytes > label);
        println!(
            "{lines},{},{cold},{},{},{},{geometry_us:.2},{},{},{visible},{label}",
            text.len(),
            percentile(&mut idle, 5),
            percentile(&mut edit, 2),
            percentile(&mut resize, 1),
            stats.paragraph_cache_bytes,
            stats.row_geometry_bytes
        );
    }
}

fn wheel(app: &mut App, editor: Entity, delta_y: f32) {
    let rect = *app.world().get::<Rect>(editor).unwrap();
    let input = app.world_mut().resource_mut::<Input>().unwrap();
    input.pointer.position = [rect.x + 8.0, rect.y + 8.0];
    input.scroll = [0.0, delta_y];
    apply_scroll(app.world_mut());
    app.update();
}

/// Scroll identity and folds across an edit and a resize.
/// Ignored with the other scale probe; correctness is also locked by non-ignored tests.
#[test]
#[ignore = "opt-in release scale characterization; no timing threshold assertions"]
fn scroll_and_fold_stability() {
    use rndr_core::editing::{EditOrigin, SourceBatch, SourceChange};
    use rndr_core::source_folding::{set_source_folds, SourceFolds};
    let text = "header\nhidden line stays folded\ntail\n";
    let hidden = "hidden line stays folded\n";
    let fold = text.find(hidden).unwrap()..text.find(hidden).unwrap() + hidden.len();
    let (mut app, editor, _bar, doc) = boot(&("prefix\n".repeat(40) + text + &"row\n".repeat(40)));
    for _ in 0..3 {
        app.update();
    }
    let rev = app
        .world()
        .resource::<SourceDocuments>()
        .unwrap()
        .get(doc)
        .unwrap()
        .revision();
    let fold = fold.start + "prefix\n".repeat(40).len()..fold.end + "prefix\n".repeat(40).len();
    set_source_folds(app.world_mut(), editor, doc, rev, vec![fold]).unwrap();
    app.update();
    let row_h = app.world().get::<TextLayout>(editor).unwrap().lines[0].h;
    wheel(&mut app, editor, -(row_h - 2.0));
    let after_fraction = app.world().get::<Editor>(editor).unwrap().offset;
    app.world_mut().get_mut::<Layout>(editor).unwrap().width = Size::Px(420.0);
    app.update();
    app.update();
    let after_resize = app.world().get::<Editor>(editor).unwrap().offset;
    wheel(&mut app, editor, -4.0);
    let after_followup = app.world().get::<Editor>(editor).unwrap().offset;
    let session = app
        .world_mut()
        .resource_mut::<SourceDocuments>()
        .unwrap()
        .get_mut(doc)
        .unwrap();
    session
        .commit(SourceBatch {
            document: doc,
            base: session.revision(),
            origin: EditOrigin::Host(1),
            changes: vec![SourceChange {
                range: 0..0,
                replacement: "INSERTED\n".into(),
            }],
        })
        .unwrap();
    app.update();
    let folds = app
        .world()
        .get::<SourceFolds>(editor)
        .map(|f| f.ranges().len())
        .unwrap_or(0);
    let offset = app.world().get::<Editor>(editor).unwrap().offset;
    let source = app
        .world()
        .resource::<SourceDocuments>()
        .unwrap()
        .get(doc)
        .unwrap()
        .text()
        .to_string();
    let fold_text = app
        .world()
        .get::<SourceFolds>(editor)
        .and_then(|folds| folds.ranges().first().cloned())
        .map(|range| source[range].to_string())
        .unwrap_or_default();
    println!(
        "row_h={row_h:.3} offset_after_fraction={after_fraction:.3} offset_after_resize={after_resize:.3} offset_after_followup_wheel={after_followup:.3} folds_after_insert={folds} fold_text={fold_text:?} offset_after_insert={offset:.3}"
    );
    assert_eq!(
        folds, 1,
        "a whole-line fold must survive an insert above it"
    );
    assert_eq!(fold_text, "hidden line stays folded\n");
    assert!(
        after_followup > after_resize + 1.0,
        "sub-row remainder must survive resize: fraction={after_fraction} resize={after_resize} followup={after_followup}"
    );
}
