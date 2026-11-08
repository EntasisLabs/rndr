//! Parity with the TypeScript reference implementation.
//!
//! `fixtures/medousa_golden.jsonl` pairs inputs (Medousa's Liquid test
//! suites and documentation) with the reference preprocessor's output,
//! canonicalized: each embed becomes `⟦kind {sorted JSON props}⟧`, icon
//! spans become `{{icon:id}}`, and the static kanban preview becomes
//! `⟦kanban⟧`.

use rndr_liquid::{segments, LiquidBlock, LiquidDocument, Segment};
use serde_json::Value;

fn canon_text(text: &str, doc: &LiquidDocument) -> String {
    let mut out = String::new();
    for segment in segments(text) {
        match segment {
            Segment::Markdown(md) => out.push_str(md),
            Segment::Block(i) => out.push_str(&canon_block(&doc.blocks[i], doc)),
        }
    }
    out
}

fn canon_block(block: &LiquidBlock, doc: &LiquidDocument) -> String {
    if let LiquidBlock::Kanban(_) = block {
        return "⟦kanban⟧".into();
    }
    let value = serde_json::to_value(block).unwrap();
    let mut json = String::new();
    write_json(&value["props"], doc, &mut json);
    format!("⟦{} {json}⟧", block.kind())
}

fn write_json(value: &Value, doc: &LiquidDocument, out: &mut String) {
    match value {
        Value::Object(map) => {
            let mut keys: Vec<_> = map.keys().collect();
            keys.sort();
            out.push('{');
            for (i, k) in keys.into_iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&serde_json::to_string(k).unwrap());
                out.push(':');
                write_json(&map[k], doc, out);
            }
            out.push('}');
        }
        Value::Array(items) => {
            out.push('[');
            for (i, v) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_json(v, doc, out);
            }
            out.push(']');
        }
        Value::String(s) => out.push_str(&serde_json::to_string(&canon_text(s, doc)).unwrap()),
        Value::Number(n) => match n.as_f64() {
            Some(f) if f.fract() == 0.0 && f.abs() < 1e15 => out.push_str(&(f as i64).to_string()),
            _ => out.push_str(&n.to_string()),
        },
        other => out.push_str(&other.to_string()),
    }
}

#[test]
fn matches_reference_implementation() {
    let fixtures = match std::env::var("LIQUID_GOLDEN_FILE") {
        Ok(path) => std::fs::read_to_string(path).unwrap(),
        Err(_) => include_str!("fixtures/medousa_golden.jsonl").to_string(),
    };
    let mut failures = Vec::new();
    let mut total = 0;
    for (line_no, line) in fixtures.lines().enumerate() {
        let fixture: Value = serde_json::from_str(line).unwrap();
        let input = fixture["input"].as_str().unwrap();
        let expected = fixture["expected"].as_str().unwrap();
        let doc = LiquidDocument::parse(input);
        let actual = canon_text(&doc.markdown, &doc);
        total += 1;
        if actual != expected {
            failures.push((line_no + 1, input.to_string(), expected.to_string(), actual));
        }
    }
    if let Ok(dir) = std::env::var("LIQUID_GOLDEN_DUMP") {
        for (n, input, expected, actual) in &failures {
            std::fs::write(format!("{dir}/{n}.input.md"), input).unwrap();
            std::fs::write(format!("{dir}/{n}.expected.md"), expected).unwrap();
            std::fs::write(format!("{dir}/{n}.actual.md"), actual).unwrap();
        }
    }
    let lines: Vec<_> = failures.iter().map(|f| f.0).collect();
    assert!(
        failures.is_empty(),
        "{}/{total} fixtures differ: lines {lines:?}",
        failures.len()
    );
}
