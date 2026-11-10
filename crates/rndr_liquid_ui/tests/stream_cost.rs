//! Release streaming cost. Ignored in CI; no timing threshold.
//!
//! ```sh
//! cargo test -p rndr_liquid_ui --release --test stream_cost -- --ignored --nocapture
//! ```
use rndr_core::*;
use rndr_liquid_ui::*;
use rndr_ui::{apply_theme_bridge, UiPlugin, UiTheme};
use std::time::Instant;

fn pct(v: &mut [u128], p: f64) -> u128 {
    v.sort_unstable();
    v[((v.len() as f64 - 1.0) * p).round() as usize]
}

fn document(total: usize, blocks: usize) -> String {
    let fences: Vec<String> = (0..blocks)
        .map(|i| format!("\n```callout\ntone: note\nbody: {i}\n```\n"))
        .collect();
    let fence_chars: usize = fences.iter().map(|f| f.chars().count()).sum();
    let prose_unit = "Streaming prose with a [link](https://example.com) and `inline code` and more words to fill the line. ";
    let mut prose = String::new();
    let budget = total.saturating_sub(fence_chars);
    while prose.chars().count() < budget {
        prose.push_str(prose_unit);
        if prose.len() % 500 < prose_unit.len() {
            prose.push_str("\n\n");
        }
    }
    let prose: Vec<char> = prose.chars().take(budget).collect();
    if blocks == 0 {
        return prose.into_iter().collect();
    }
    let gap = prose.len() / blocks;
    let mut out = String::new();
    let mut at = 0;
    for (i, fence) in fences.iter().enumerate() {
        let end = if i + 1 == blocks {
            prose.len()
        } else {
            ((i + 1) * gap).min(prose.len())
        };
        out.extend(prose[at..end].iter());
        out.push_str(fence);
        at = end;
    }
    out
}

fn app() -> (App, Entity) {
    let mut app = App::new();
    app.add_plugin(UiPlugin);
    app.add_plugin(MarkdownPlugin);
    app.add_plugin(LiquidUiPlugin::new(NoHost));
    app.world_mut().insert_resource(Viewport::new(960.0, 640.0));
    apply_theme_bridge(app.world_mut(), UiTheme::default());
    let view = liquid_view(app.world_mut(), "");
    app.world_mut()
        .get_mut::<MarkdownView>(view)
        .unwrap()
        .set_follow(true);
    (app, view)
}

#[test]
#[ignore = "release characterization of liquid streaming; no timing threshold"]
fn liquid_stream_cost() {
    println!("mode,chars,blocks,bytes,updates,total_ms,p50_us,p95_us,max_us,mounted,md_blocks");
    for chars in [1_000usize, 10_000, 100_000] {
        for blocks in [2usize, 50] {
            let full = document(chars, blocks);
            let (mut app, view) = app();
            rndr_paint::frame(&mut app);
            let mut samples = Vec::new();
            let started = Instant::now();
            let mut at = 0usize;
            while at < full.len() {
                let mut end = (at + 256).min(full.len());
                while !full.is_char_boundary(end) {
                    end += 1;
                }
                let t = Instant::now();
                push_liquid_str(app.world_mut(), view, &full[at..end]);
                rndr_paint::frame(&mut app);
                samples.push(t.elapsed().as_micros());
                at = end;
            }
            let mounted = app.world().get::<LiquidView>(view).unwrap().mounted().len();
            let md = app
                .world()
                .get::<MarkdownView>(view)
                .unwrap()
                .blocks()
                .len();
            let max = samples.iter().copied().max().unwrap_or(0);
            println!(
                "push_liquid_str,{},{blocks},{},{},{},{},{},{max},{mounted},{md}",
                full.chars().count(),
                full.len(),
                samples.len(),
                started.elapsed().as_millis(),
                pct(&mut samples, 0.5),
                pct(&mut samples, 0.95),
            );
        }
    }
}
