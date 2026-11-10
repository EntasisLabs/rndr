//! Opt-in release probe for offscreen Liquid blocks. No timing thresholds.
//!
//! ```text
//! cargo test -p rndr_liquid_ui --release --test virtual_cost -- --ignored --nocapture
//! ```
use rndr_core::*;
use rndr_liquid_ui::*;
use rndr_ui::{apply_theme_bridge, UiPlugin, UiTheme};
use std::time::Instant;

fn rss_kib() -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .unwrap_or_default()
        .lines()
        .find_map(|line| line.strip_prefix("VmRSS:"))
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|n| n.parse().ok())
        .unwrap_or(0)
}

fn pct(values: &mut [u128], p: f64) -> u128 {
    values.sort_unstable();
    values[((values.len() as f64 - 1.0) * p).round() as usize]
}

fn unit(i: usize) -> String {
    let fence = match i % 4 {
        0 => format!("```callout\ntone: tip\ntitle: Tip {i}\nbody: Body {i} with **bold**.\n```"),
        1 => format!("```card\ntitle: Card {i}\nsubtitle: Sub {i}\nbody: Card body {i}.\n```"),
        2 => format!("```tabs\n## One {i}\nFirst panel {i}.\n## Two {i}\nSecond panel {i}.\n```"),
        _ => format!(
            "```chart\ntype: bar\ntitle: Chart {i}\n| Month | Users |\n|---|---|\n| Jan | {i} |\n| Feb | 14 |\n```"
        ),
    };
    format!("Paragraph {i} alpha beta gamma delta epsilon.\n\nAnother line {i} of prose here.\n\n{fence}\n\n")
}

#[test]
#[ignore = "opt-in release probe; no timing threshold assertions"]
fn liquid_density_cost() {
    println!(
        "liquid_blocks,md_blocks,mounted,layout_entities,rss_kib,cold_us,idle_p50_us,scroll_p50_us,scroll_p95_us"
    );
    for n in [20usize, 100, 400, 2_000] {
        let source: String = (0..n).map(unit).collect();
        let mut app = App::new();
        app.add_plugin(UiPlugin);
        app.add_plugin(MarkdownPlugin);
        app.add_plugin(LiquidUiPlugin::new(NoHost));
        app.world_mut().insert_resource(Viewport::new(960.0, 640.0));
        apply_theme_bridge(app.world_mut(), UiTheme::default());
        let view = liquid_view(app.world_mut(), &source);
        app.world_mut()
            .get_mut::<MarkdownView>(view)
            .unwrap()
            .set_follow(false);
        let started = Instant::now();
        rndr_paint::frame(&mut app);
        let cold = started.elapsed().as_micros();
        rndr_paint::frame(&mut app);
        rndr_paint::frame(&mut app);
        let mounted = app.world().get::<LiquidView>(view).unwrap().mounted().len();
        let blocks = app
            .world()
            .get::<MarkdownView>(view)
            .unwrap()
            .blocks()
            .len();
        let entities = app.world().entities_with::<Layout>().len();
        let rss = rss_kib();
        let mut idle = Vec::new();
        for _ in 0..21 {
            let started = Instant::now();
            rndr_paint::frame(&mut app);
            idle.push(started.elapsed().as_micros());
        }
        let mut scroll = Vec::new();
        for _ in 0..21 {
            {
                let input = app.world_mut().resource_mut::<Input>().unwrap();
                input.pointer.position = [48.0, 48.0];
                input.scroll = [0.0, 120.0];
            }
            let started = Instant::now();
            rndr_paint::frame(&mut app);
            scroll.push(started.elapsed().as_micros());
        }
        println!(
            "{n},{blocks},{mounted},{entities},{rss},{cold},{},{},{}",
            pct(&mut idle, 0.5),
            pct(&mut scroll.clone(), 0.5),
            pct(&mut scroll, 0.95)
        );
    }
}
