use rndr_liquid::{
    parse_fence, split_icons, InlineIcon, KanbanCard, LiquidBlock, LiquidDocument, Segment,
};

#[test]
fn unparseable_fences_stay_markdown() {
    let source = "```rust\nfn main() {}\n```\n\n```card\n```\n";
    let doc = LiquidDocument::parse(source);
    assert!(doc.blocks.is_empty());
    assert_eq!(doc.markdown, source);
}

#[test]
fn nested_blocks_resolve_innermost_first() {
    let doc = LiquidDocument::parse(
        "```report\ntitle: Weekly\n\nOpening.\n\n```callout\ntone: tip\nbody: Inner\n```\n\n## Metrics\n\nMore.\n```",
    );
    assert_eq!(doc.blocks.len(), 2);
    assert_eq!(doc.blocks[0].kind(), "callout");
    let [Segment::Markdown(_), Segment::Block(outer), Segment::Markdown(_)] = doc.segments()[..]
    else {
        panic!("{:?}", doc.segments());
    };
    let Some(LiquidBlock::Report(report)) = doc.block(outer) else {
        panic!()
    };
    let inner: Vec<_> = rndr_liquid::segments(&report.body)
        .into_iter()
        .filter_map(|s| match s {
            Segment::Block(i) => Some(i),
            Segment::Markdown(_) => None,
        })
        .collect();
    assert_eq!(inner, [0]);
}

#[test]
fn documentation_fences_are_shielded() {
    let source = "````markdown\n```card\ntitle: Example\n```\n````\n";
    let doc = LiquidDocument::parse(source);
    assert!(doc.blocks.is_empty());
    assert_eq!(doc.markdown, source);
}

#[test]
fn icons_are_canonicalized_outside_fences() {
    let doc = LiquidDocument::parse("Go {{icon:Home}} {{icon:skull}}!\n```\n{{icon:home}}\n```");
    assert_eq!(doc.markdown, "Go {{icon:house}} !\n```\n{{icon:home}}\n```");
    assert_eq!(
        split_icons("a {{icon:house}} b"),
        [
            InlineIcon::Text("a "),
            InlineIcon::Icon("house"),
            InlineIcon::Text(" b")
        ]
    );
}

#[test]
fn kanban_columns_and_default_board() {
    let Some(LiquidBlock::Kanban(board)) =
        parse_fence("kanban", "## Todo\n- [ ] Write\n- [x] Read\n## Done\n")
    else {
        panic!()
    };
    assert_eq!(board.columns.len(), 2);
    assert_eq!(
        board.columns[0].cards,
        [
            KanbanCard {
                text: "Write".into(),
                checked: false
            },
            KanbanCard {
                text: "Read".into(),
                checked: true
            }
        ]
    );
    let Some(LiquidBlock::Kanban(empty)) = parse_fence("kanban", "") else {
        panic!()
    };
    let titles: Vec<_> = empty.columns.iter().map(|c| c.title.as_str()).collect();
    assert_eq!(titles, ["Backlog", "Doing", "Done"]);
}

#[test]
fn aliases_resolve_to_canonical_kinds() {
    for (alias, body, kind) in [
        ("action_row", "Open | intent: open", "actions"),
        ("chip_group", "Alpha\nBeta", "chips"),
        (
            "procedure",
            "title: Brew\n---\nlabel: Boil\n---\nlabel: Pour",
            "recipe",
        ),
    ] {
        assert_eq!(
            parse_fence(alias, body).map(|b| b.kind()),
            Some(kind),
            "{alias}"
        );
    }
}

#[test]
fn blocks_round_trip_through_serde() {
    let doc = LiquidDocument::parse(
        "```chart\ntype: donut\ncenterLabel: Total\n| Kind | Share |\n|---|---|\n| A | 60% |\n| B | 40% |\n```",
    );
    let json = serde_json::to_string(&doc.blocks[0]).unwrap();
    assert!(
        json.starts_with(r#"{"kind":"chart","props":{"type":"donut""#),
        "{json}"
    );
    let back: LiquidBlock = serde_json::from_str(&json).unwrap();
    assert_eq!(back, doc.blocks[0]);
}
