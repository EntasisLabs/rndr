//! Release characterization of a long markdown document.
//! Paragraphs are unique. No timing threshold.
//!
//! ```text
//! cargo test -p rndr_core --release --test markdown_virtual_cost -- --ignored --nocapture
//! ```
use std::time::Instant;

use rndr_core::{markdown_view, App, Input, Label, Layout, MarkdownPlugin, Viewport};

fn distinct(blocks: usize) -> String {
    let mut source = String::new();
    for index in 0..blocks {
        if index > 0 {
            source.push_str("\n\n");
        }
        source.push_str(&format!(
            "Paragraph {index} alpha beta gamma delta epsilon zeta {index}."
        ));
    }
    source
}

fn rss_kib() -> u64 {
    let text = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    text.lines()
        .find_map(|line| line.strip_prefix("VmRSS:"))
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|n| n.parse().ok())
        .unwrap_or(0)
}

fn p50(samples: &mut [u128]) -> u128 {
    samples.sort_unstable();
    samples[samples.len() / 2]
}

#[test]
#[ignore = "opt-in release characterization of markdown virtualization; no timing threshold"]
fn markdown_virtual_cost() {
    println!("blocks,entities,labels,rss_kib,cold_us,idle_p50_us,scroll_p50_us,edit_us");
    for blocks in [2_000, 8_000, 20_000] {
        let mut app = App::new();
        app.add_plugin(MarkdownPlugin);
        app.world_mut().insert_resource(Viewport::new(960.0, 640.0));
        let view = markdown_view(app.world_mut(), distinct(blocks));
        let cold_at = Instant::now();
        app.update();
        let cold = cold_at.elapsed().as_micros();
        app.update();
        app.update();
        let entities = app.world().entities_with::<Layout>().len();
        let labels = app.world().entities_with::<Label>().len();
        let rss = rss_kib();
        let mut idle = Vec::new();
        for _ in 0..11 {
            let start = Instant::now();
            app.update();
            idle.push(start.elapsed().as_micros());
        }
        let mut scroll = Vec::new();
        for _ in 0..11 {
            {
                let input = app.world_mut().resource_mut::<Input>().unwrap();
                input.pointer.position = [48.0, 48.0];
                input.scroll = [0.0, 80.0];
            }
            let start = Instant::now();
            app.update();
            scroll.push(start.elapsed().as_micros());
        }
        let start = Instant::now();
        app.world_mut()
            .get_mut::<rndr_core::MarkdownView>(view)
            .unwrap()
            .push_str("\n\nTail paragraph omega unique 99999.\n");
        app.update();
        let edit = start.elapsed().as_micros();
        println!(
            "{blocks},{entities},{labels},{rss},{cold},{},{},{edit}",
            p50(&mut idle),
            p50(&mut scroll)
        );
    }
}
