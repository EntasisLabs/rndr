//! Every byte of a Liquid fence is either a fence line or a body part.
//!
//! Golden documents round-trip through the concrete tree. Block scalars,
//! `---` groups, and pipe tables are parts, not dropped lines.

use rndr_liquid::FENCE_LANGS;
use rndr_markdown::{parse, FencePart, TypedView};
use serde_json::Value;

fn assert_tiled(src: &str) {
    let tree = parse(src);
    assert_eq!(tree.concat(), src, "concat");
    assert_eq!(parse(&tree.concat()), tree, "reparse");
    let view = TypedView::new(src, &tree);
    for fence in view.liquid_fences() {
        let mut cursor = fence.body.start;
        for part in &fence.parts {
            let range = part.range();
            assert!(
                range.start == cursor && range.end >= range.start,
                "fence {:?} part {:?} left a gap at {cursor} (body {:?})\n{}",
                fence.canonical,
                std::mem::discriminant(part),
                fence.body,
                &src[fence.range.clone()],
            );
            assert!(src.is_char_boundary(range.start) && src.is_char_boundary(range.end));
            cursor = range.end;
        }
        assert_eq!(
            cursor, fence.body.end,
            "uncovered tail in {}",
            fence.canonical
        );
        let covered: usize = fence.parts.iter().map(|part| part.range().len()).sum();
        assert_eq!(covered, fence.body.len());
    }
}

#[test]
fn golden_fences_round_trip_with_no_invisible_bytes() {
    let fixtures = include_str!("../../rndr_liquid/tests/fixtures/medousa_golden.jsonl");
    let mut kinds = std::collections::BTreeSet::new();
    let mut documents = 0;
    for line in fixtures.lines() {
        let fixture: Value = serde_json::from_str(line).unwrap();
        let input = fixture["input"].as_str().unwrap();
        documents += 1;
        assert_tiled(input);
        let tree = parse(input);
        for fence in TypedView::new(input, &tree).liquid_fences() {
            kinds.insert(fence.canonical);
        }
    }
    assert!(documents > 20, "{documents}");
    let expected = [
        "accordion",
        "actions",
        "block",
        "brief",
        "callout",
        "card",
        "carousel",
        "chart",
        "chips",
        "cite",
        "code",
        "compare",
        "dashboard",
        "decision",
        "feed",
        "kanban",
        "media",
        "plan",
        "recipe",
        "report",
        "section",
        "shortlist",
        "slides",
        "steps",
        "tabs",
        "timeline",
        "tree",
    ];
    for kind in expected {
        assert!(kinds.contains(kind), "missing {kind} in {kinds:?}");
        assert!(
            FENCE_LANGS.contains(&kind),
            "{kind} is not a fence language"
        );
    }
    assert_eq!(expected.len(), 27);
}

#[test]
fn block_scalar_value_includes_the_continuation() {
    let src = "```callout\ntone: tip\nbody: |\n  Hello\n  there\n```\n";
    assert_tiled(src);
    let tree = parse(src);
    let fence = TypedView::new(src, &tree).liquid_fences().remove(0);
    assert!(fence.valid);
    let body = fence
        .fields
        .iter()
        .find(|field| field.key == "body")
        .unwrap();
    assert!(body.multiline);
    assert!(body.markdown);
    assert!(body.value.contains("Hello"), "{:?}", body.value);
    assert!(body.value.contains("there"), "{:?}", body.value);
    assert!(!body.value.is_empty());
    assert!(src[body.value_span.clone()].contains("there"));
    assert!(!src[body.range.clone()].contains("tone"));
}

#[test]
fn separators_group_items_and_stay_in_the_body() {
    let src = "```timeline\ntitle: Road\n---\nlabel: One\nts: Monday\nlane: product\n---\nlabel: Two\nts: Tuesday\n```\n";
    assert_tiled(src);
    let tree = parse(src);
    let fence = TypedView::new(src, &tree).liquid_fences().remove(0);
    assert!(fence.valid, "{src}");
    let separators = fence
        .parts
        .iter()
        .filter(|part| matches!(part, FencePart::Separator { .. }))
        .count();
    assert_eq!(separators, 2);
    assert!(fence.groups.len() >= 3, "{:?}", fence.groups.len());
    let labels: Vec<_> = fence
        .fields
        .iter()
        .filter(|field| field.key == "label")
        .map(|field| field.value)
        .collect();
    assert_eq!(labels, ["One", "Two"]);
}

#[test]
fn compare_pipe_table_cells_keep_their_bytes() {
    let src = "```compare\ntitle: Pick\n| | A | B |\n|---|---|\n| Cost | 1 | 2 |\n```\n";
    assert_tiled(src);
    let tree = parse(src);
    let fence = TypedView::new(src, &tree).liquid_fences().remove(0);
    assert!(fence.valid, "{src}");
    let FencePart::Table { rows, .. } = fence
        .parts
        .iter()
        .find(|part| matches!(part, FencePart::Table { .. }))
        .unwrap()
    else {
        panic!("no table");
    };
    assert!(rows.len() >= 3, "{}", rows.len());
    assert!(rows.iter().any(|row| row.separator));
    let cells: Vec<_> = rows
        .iter()
        .flat_map(|row| row.cells.iter().map(|cell| cell.text.trim()))
        .filter(|cell| !cell.is_empty())
        .collect();
    assert!(cells.contains(&"Cost"), "{cells:?}");
    assert!(cells.contains(&"1"), "{cells:?}");
    assert!(cells.contains(&"2"), "{cells:?}");
    assert!(cells.iter().any(|cell| cell.contains("---")), "{cells:?}");
}

#[test]
fn pipe_row_keeps_each_field() {
    let src = "```card\n| title: Sol | body: Flagship |\n```\n";
    assert_tiled(src);
    let tree = parse(src);
    let fence = TypedView::new(src, &tree).liquid_fences().remove(0);
    let keys: Vec<_> = fence.fields.iter().map(|field| field.key.trim()).collect();
    assert_eq!(keys, ["title", "body"]);
    assert_eq!(
        fence
            .fields
            .iter()
            .find(|field| field.key.trim() == "body")
            .unwrap()
            .value,
        "Flagship"
    );
}
