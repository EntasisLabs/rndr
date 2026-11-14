//! Per-keystroke cost: splice, reparse, and relayout of the edited block.
//!
//! ```text
//! cargo bench -p rndr_markdown_ui --bench keystroke
//! ```
//!
//! The session still runs the confirming full parse on every edit. That cost
//! is included in the splice+reparse number. Relayout measures only the
//! edited block.

use std::time::Instant;

use rndr_core::{BuiltinText, TextStyle};
use rndr_markdown::{Command, Session};
use rndr_markdown_ui::{measure_block, project, Palette};

fn mixed(bytes: usize) -> String {
    let para = "The quick brown fox jumps over the lazy dog. *emph* and **strong** and `code`.\n\n";
    let fence = "```callout\ntone: info\nbody: A short note in the fence.\n```\n\n";
    let mut out = String::with_capacity(bytes + 128);
    let mut n = 0u32;
    while out.len() < bytes {
        out.push_str(para);
        n += 1;
        if n.is_multiple_of(40) {
            out.push_str(fence);
        }
    }
    out
}

fn bench(label: &str, src: &str) {
    let open = Instant::now();
    let mut session = Session::open(src);
    let open_us = open.elapsed().as_secs_f64() * 1_000_000.0;
    let engine = BuiltinText::new();
    let style = TextStyle::default();
    let palette = Palette::default();
    let mut blocks = project(session.bytes(), session.tree(), &palette);
    let at = src.find("dog").unwrap_or(10);
    session.move_caret(at, at).unwrap();
    // Warm the confirming parse and the line cache.
    for _ in 0..3 {
        let _ = session.edit(
            Command::Insert {
                at,
                text: "x".into(),
            },
            0,
        );
        let _ = session.undo(1);
    }
    let mut edit_us = 0.0;
    let mut project_us = 0.0;
    let mut measure_us = 0.0;
    let mut widened = 0;
    let rounds = 11;
    for i in 0..rounds {
        let now = (i as u64) * 50;
        let started = Instant::now();
        session
            .edit(
                Command::Insert {
                    at,
                    text: "k".into(),
                },
                now,
            )
            .unwrap();
        edit_us += started.elapsed().as_secs_f64() * 1_000_000.0;
        let span = session.last_edit().unwrap();
        widened += usize::from(span.widened_to_root);
        let started = Instant::now();
        rndr_markdown_ui::update_projection(
            &mut blocks,
            session.bytes(),
            session.tree(),
            span,
            &palette,
        );
        project_us += started.elapsed().as_secs_f64() * 1_000_000.0;
        let started = Instant::now();
        if let Some(block) = blocks
            .iter()
            .find(|block| block.span.start <= span.old_start && span.old_start <= block.span.end)
        {
            let _ = measure_block(&engine, block, 640.0, &style);
        }
        measure_us += started.elapsed().as_secs_f64() * 1_000_000.0;
        session.undo(now + 1).unwrap();
    }
    let blocks_n = project(session.bytes(), session.tree(), &palette)
        .iter()
        .filter(|block| block.kind != rndr_markdown_ui::DisplayKind::Gap)
        .count();
    println!(
        "{label}: bytes={} blocks={blocks_n} open={open_us:.1}µs splice+reparse={:.1}µs project={:.1}µs measure={:.1}µs widened={widened}/{}",
        src.len(),
        edit_us / rounds as f64,
        project_us / rounds as f64,
        measure_us / rounds as f64,
        rounds,
    );
}

fn main() {
    let small = "\
# Notes

A paragraph with *marks* and `code`.

- alpha
- beta

```callout
tone: tip
body: Hello
```
";
    bench("small", small);
    let large = mixed(1024 * 1024);
    bench("1MiB", &large);
    let huge = mixed(5 * 1024 * 1024);
    bench("5MiB", &huge);
}
