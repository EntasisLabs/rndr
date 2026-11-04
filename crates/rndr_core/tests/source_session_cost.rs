//! Explicit opt-in model-only characterization, not a native frame benchmark.
//! cargo test -p rndr_core --release --test source_session_cost -- --ignored --nocapture
use rndr_core::editing::*;
use rndr_core::Buffer;
use std::hint::black_box;
use std::time::Instant;

#[test]
#[ignore = "opt-in release characterization; no machine-dependent timing assertions"]
fn model_costs() {
    println!("fixture,bytes,position,raw_buffer_insert_us_median,raw_buffer_insert_us_max,session_commit_undo_us_median,session_commit_undo_us_max");
    for (name, unit) in [
        ("ascii_lines", "alpha beta gamma\n"),
        ("unicode_lines", "é العربية 👩‍💻\r\n"),
        ("long_line", "x"),
    ] {
        for size in [10_240usize, 1_048_576, 10_485_760] {
            let source = unit.repeat(size.div_ceil(unit.len()));
            for (position, divisor) in [("begin", usize::MAX), ("middle", 2), ("end", 1)] {
                let mut offset = if divisor == usize::MAX {
                    0
                } else {
                    source.len() / divisor
                };
                while !source.is_char_boundary(offset) {
                    offset -= 1;
                }
                let (mut raw, mut managed) = (Vec::new(), Vec::new());
                for _ in 0..5 {
                    let mut buffer = Buffer::from_str(&source);
                    let start = Instant::now();
                    buffer.insert_str(offset, "hello");
                    black_box(&buffer);
                    raw.push(start.elapsed().as_micros());
                    let mut documents = SourceDocuments::default();
                    let id = documents.open(source.clone());
                    let session = documents.get_mut(id).unwrap();
                    let start = Instant::now();
                    session
                        .commit(SourceBatch {
                            document: id,
                            base: session.revision(),
                            origin: EditOrigin::Host(1),
                            changes: vec![SourceChange {
                                range: offset..offset,
                                replacement: "hello".into(),
                            }],
                        })
                        .unwrap();
                    session.undo().unwrap();
                    black_box(session.text());
                    managed.push(start.elapsed().as_micros());
                    assert_eq!(session.text(), source);
                }
                raw.sort_unstable();
                managed.sort_unstable();
                println!(
                    "{name},{},{position},{},{},{},{}",
                    source.len(),
                    raw[2],
                    raw[4],
                    managed[2],
                    managed[4]
                );
            }
        }
    }
}
