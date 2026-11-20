//! Opt-in release CPU characterization of retained, non-virtualized rows.
//! Not native shaping, input-to-present latency, or GPU timing.
//! cargo test -p rndr_paint --release --test frame_scale_cost -- --ignored --nocapture
use rndr_core::*;
use std::{hint::black_box, time::Instant};

fn fixture(rows: usize, hidden: bool) -> App {
    let mut app = App::new();
    app.world_mut()
        .insert_resource(Viewport::new(1200.0, 800.0));
    let root = app.world_mut().spawn((
        Layout {
            width: Size::Fill,
            height: Size::Fill,
            ..Layout::column()
        },
        Scroll::default(),
    ));
    let content = app.world_mut().spawn(Layout {
        width: Size::Fill,
        height: Size::Hug,
        ..Layout::column()
    });
    set_parent(app.world_mut(), content, root);
    for i in 0..rows {
        let row = app.world_mut().spawn((
            Layout {
                width: Size::Fill,
                height: Size::Px(24.0),
                ..Layout::column()
            },
            Label::new(format!("Row {i}: retained explorer entry")),
        ));
        set_parent(app.world_mut(), row, content);
    }
    if hidden {
        app.world_mut().insert(content, Hidden);
    }
    for _ in 0..3 {
        black_box(rndr_paint::frame(&mut app));
    }
    app
}

fn sample(mut work: impl FnMut()) -> (u128, u128) {
    let mut samples = Vec::new();
    for _ in 0..41 {
        let start = Instant::now();
        work();
        samples.push(start.elapsed().as_nanos());
    }
    samples.sort_unstable();
    (samples[20], samples[38])
}

#[test]
#[ignore = "opt-in release characterization; no timing threshold assertions"]
fn retained_frame_scale_cost() {
    println!("rows,hidden,update_ns_p50,update_ns_p95,layout_ns_p50,layout_ns_p95,paint_ns_p50,paint_ns_p95,hit_ns_p50,hit_ns_p95,text_ns_p50,text_ns_p95,quads");
    for rows in [100, 1_000, 5_000] {
        for hidden in [false, true] {
            let mut app = fixture(rows, hidden);
            let update = sample(|| app.update());
            let layout = sample(|| layout_propagate(app.world_mut()));
            let paint = sample(|| {
                black_box(rndr_paint::paint(app.world()));
            });
            let hit = sample(|| {
                black_box(hit_test(app.world(), 20.0, 20.0));
            });
            let text = sample(|| apply_text_layout(app.world_mut()));
            let quads = rndr_paint::paint(app.world()).len();
            println!(
                "{rows},{hidden},{},{},{},{},{},{},{},{},{},{},{quads}",
                update.0,
                update.1,
                layout.0,
                layout.1,
                paint.0,
                paint.1,
                hit.0,
                hit.1,
                text.0,
                text.1
            );
        }
    }
}

#[test]
fn fixture_retains_rows_but_bounds_visible_output() {
    let small = fixture(100, false);
    let large = fixture(1_000, false);
    let hidden = fixture(1_000, true);
    assert_eq!(small.world().entities_with::<Label>().len(), 100);
    assert_eq!(large.world().entities_with::<Label>().len(), 1_000);
    assert_eq!(hidden.world().entities_with::<Label>().len(), 1_000);
    assert_eq!(
        rndr_paint::quads_snapshot(&rndr_paint::Frame {
            quads: rndr_paint::paint(small.world()),
            slots: Vec::new(),
            draws: Vec::new()
        }),
        rndr_paint::quads_snapshot(&rndr_paint::Frame {
            quads: rndr_paint::paint(large.world()),
            slots: Vec::new(),
            draws: Vec::new()
        })
    );
    assert!(!rndr_paint::paint(small.world()).is_empty());
    assert!(rndr_paint::paint(hidden.world()).is_empty());
    assert!(hit_test(large.world(), 20.0, 20.0).is_some());
}
