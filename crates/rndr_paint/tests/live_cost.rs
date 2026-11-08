//! Headless CPU cost of a live-only frame against a full frame. Not a GPU or
//! native latency measurement.
//! cargo test -p rndr_paint --release --test live_cost -- --ignored --nocapture

use std::time::Instant;

use rndr_core::{set_parent, App, Color, Label, Layout, Live, LiveOffset, Size, Viewport, Wrap};
use rndr_paint::{frame, frame_live};

fn p50(mut samples: Vec<u128>) -> u128 {
    samples.sort_unstable();
    samples[samples.len() / 2]
}

#[test]
#[ignore = "opt-in release characterization; no timing threshold assertions"]
fn live_only_frame_cost() {
    println!("cards,full_us_p50,live_us_p50,ratio");
    for cards in [100, 1_000, 5_000] {
        let mut app = App::new();
        app.world_mut()
            .insert_resource(Viewport::new(1200.0, 800.0));
        let root = app.world_mut().spawn((
            Layout {
                width: Size::Fill,
                height: Size::Fill,
                gap: 4.0,
                ..Layout::row()
            },
            Wrap::default(),
        ));
        for i in 0..cards {
            let card = app.world_mut().spawn((
                Layout {
                    width: Size::Px(110.0),
                    height: Size::Hug,
                    padding: 4.0,
                    ..Layout::column()
                },
                Color::rgb(0.2, 0.2, 0.25),
            ));
            let label = app.world_mut().spawn((
                Layout {
                    width: Size::Fill,
                    height: Size::Hug,
                    ..Layout::column()
                },
                Label::new(format!("card {i} with wrapping text")),
            ));
            set_parent(app.world_mut(), label, card);
            set_parent(app.world_mut(), card, root);
        }
        let cursor = app.world_mut().spawn((
            Layout {
                width: Size::Px(12.0),
                height: Size::Px(12.0),
                ..Layout::column()
            },
            Color::rgb(1.0, 0.3, 0.3),
        ));
        let at = Live::new([0.0_f32, 0.0]);
        app.world_mut().insert(cursor, LiveOffset::new(&at));
        set_parent(app.world_mut(), cursor, root);
        for _ in 0..3 {
            frame(&mut app);
        }
        let mut full = Vec::new();
        let mut live = Vec::new();
        for i in 0..60 {
            at.set([i as f32, 0.0]);
            let start = Instant::now();
            std::hint::black_box(frame(&mut app));
            full.push(start.elapsed().as_micros());
            at.set([i as f32, 1.0]);
            let start = Instant::now();
            std::hint::black_box(frame_live(&mut app));
            live.push(start.elapsed().as_micros());
        }
        let (full, live) = (p50(full), p50(live));
        println!(
            "{cards},{full},{live},{:.1}",
            full as f64 / live.max(1) as f64
        );
    }
}
