//! Titles, tasks, footnotes, autolinks, math, and Medousa spans.
//!
//! Each case round-trips the original bytes, shows up on the typed view, and
//! reparses incrementally to the same tree as a full parse.

use rndr_markdown::{
    apply_text, apply_view, parse, Command, InlineMark, SyntaxKind, TypedBlock, TypedView,
};

fn assert_lossless(src: &str) {
    let tree = parse(src);
    assert_eq!(tree.concat(), src, "concat {src:?}");
    assert_eq!(parse(&tree.concat()), tree, "reparse {src:?}");
}

fn has_kind(src: &str, kind: SyntaxKind) -> bool {
    fn walk(node: &rndr_markdown::Green, kind: SyntaxKind) -> bool {
        node.kind() == kind || node.children().iter().any(|child| walk(child, kind))
    }
    walk(&parse(src), kind)
}

fn marker(src: &str) -> String {
    fn walk(node: &rndr_markdown::Green) -> Option<String> {
        if node.kind() == SyntaxKind::ListMarker {
            return Some(node.concat());
        }
        node.children().iter().find_map(walk)
    }
    walk(&parse(src)).unwrap_or_default()
}

fn round_trip(src: &str, at: usize, text: &str) {
    let tree = parse(src);
    let applied = apply_view(
        src,
        &tree,
        &Command::Insert {
            at,
            text: text.into(),
        },
    )
    .unwrap();
    let via_text = apply_text(src, &applied.splices);
    assert_eq!(applied.bytes, via_text);
    assert_eq!(
        parse(&via_text),
        applied.tree,
        "incremental diverged after inserting {text:?} at {at} in {src:?}"
    );
}

#[test]
fn link_and_image_titles_round_trip_and_are_visible() {
    for src in [
        "[l](u \"t\")\n",
        "![a](p \"t\")\n",
        "[l](u 't')\n",
        "[l](<u> \"t\")\n",
    ] {
        assert_lossless(src);
    }
    assert!(has_kind("[l](u \"t\")\n", SyntaxKind::Link));
    assert!(has_kind("![a](p \"t\")\n", SyntaxKind::Image));
    let tree = parse("[l](u \"t\")\n");
    let view = TypedView::new("[l](u \"t\")\n", &tree);
    let InlineMark::Link {
        destination, title, ..
    } = view.marks().into_iter().next().unwrap()
    else {
        panic!("expected a link");
    };
    assert_eq!(destination, "u");
    assert_eq!(title, Some("t"));
    let image = parse("![a](p \"t\")\n");
    let view = TypedView::new("![a](p \"t\")\n", &image);
    let InlineMark::Image {
        alt,
        destination,
        title,
        ..
    } = view.marks().into_iter().next().unwrap()
    else {
        panic!("expected an image");
    };
    assert_eq!(alt, "a");
    assert_eq!(destination, "p");
    assert_eq!(title, Some("t"));
    round_trip("[l](u \"t\")\n", 1, "X");
    round_trip("![a](p \"t\")\n", 2, "X");
}

#[test]
fn bracket_notes_stay_in_the_item_and_ordered_boxes_stay_ordered() {
    assert_lossless("- [Note] text\n");
    assert_eq!(marker("- [Note] text\n"), "- ");
    assert!(parse("- [Note] text\n").concat().contains("[Note] text"));
    assert_eq!(marker("- [ ] buy\n"), "- [ ] ");
    assert_eq!(marker("- [x] buy\n"), "- [x] ");
    assert_eq!(marker("- [X] buy\n"), "- [X] ");
    assert_eq!(marker("1. [ ] buy\n"), "1. ");
    assert!(parse("1. [ ] buy\n").concat().contains("[ ] buy"));
    round_trip("- [Note] text\n", 2, "Z");
    round_trip("1. [ ] buy\n", 3, "Z");
}

#[test]
fn footnotes_autolinks_and_math_round_trip() {
    let src = "See [^1] and ^[inline].\n\n[^1]: The note\n  continues\n";
    assert_lossless(src);
    assert!(has_kind(src, SyntaxKind::FootnoteRef));
    assert!(has_kind(src, SyntaxKind::FootnoteInline));
    assert!(has_kind(src, SyntaxKind::FootnoteDef));
    let tree = parse(src);
    let view = TypedView::new(src, &tree);
    assert!(view.blocks().iter().any(|block| matches!(
        block,
        TypedBlock::Footnote { id, .. } if *id == "1"
    )));
    let marks = view.marks();
    assert!(marks
        .iter()
        .any(|mark| matches!(mark, InlineMark::Footnote { id, .. } if *id == "1")));
    assert!(marks
        .iter()
        .any(|mark| matches!(mark, InlineMark::Footnote { id, .. } if *id == "inline")));

    for src in [
        "Go to https://example.com/a.\n",
        "See www.example.com/docs.\n",
        "Mail ada@example.com now\n",
        "Angle <https://example.com> stays\n",
    ] {
        assert_lossless(src);
        assert!(has_kind(src, SyntaxKind::Autolink), "{src}");
    }
    let url = "Go to https://example.com/a.\n";
    let tree = parse(url);
    let view = TypedView::new(url, &tree);
    let InlineMark::Autolink { target, .. } = &view.marks()[0] else {
        panic!("expected an autolink");
    };
    assert_eq!(*target, "https://example.com/a");

    assert_lossless("Energy $E=mc^2$ here\n");
    assert!(has_kind("Energy $E=mc^2$ here\n", SyntaxKind::Math));
    assert!(!has_kind("Costs $5 and $10\n", SyntaxKind::Math));
    round_trip(src, 5, "Q");
    round_trip("Energy $E=mc^2$ here\n", 8, "Z");
    round_trip("Go to https://example.com/a.\n", 8, "Z");
}

#[test]
fn medousa_spans_round_trip_and_incremental_reparse_matches() {
    let samples = [
        "> [!note] Title\n> Body line\n",
        "> [!WARNING] Watch\n> out\n",
        "A ==highlight== word\n",
        "See [[Note]] and [[Target|Label]]\n",
        "See [[Note#Heading|Alias]]\n",
        "![[shot.png]]\n",
        "![[shot.png|400]]\n",
        "![[shot.png|400x240]]\n",
        "![[shot.png|caption]]\n",
        "Paint {{red|rose}} and {{#0af|hex}}\n",
        "Font {{font:serif|words}} and {{size:lg|big}}\n",
        "::block::\nHello\n::end::\n",
        "::block::\nHello\n::end block::\n",
    ];
    for src in samples {
        assert_lossless(src);
        round_trip(src, src.len() / 2, "Z");
    }
    let callout = "> [!note] Title\n> Body line\n";
    let tree = parse(callout);
    let view = TypedView::new(callout, &tree);
    match &view.blocks()[0] {
        TypedBlock::Callout { tone, title, .. } => {
            assert_eq!(*tone, "note");
            assert_eq!(*title, "Title");
        }
        other => panic!("expected a callout, got {other:?}"),
    }
    let styled = "::block::\nHello\n::end::\n";
    let tree = parse(styled);
    let view = TypedView::new(styled, &tree);
    assert!(matches!(view.blocks()[0], TypedBlock::StyledBlock { .. }));

    let wiki = "See [[Target|Label]]\n";
    let tree = parse(wiki);
    let view = TypedView::new(wiki, &tree);
    let InlineMark::WikiLink { target, label, .. } = &view.marks()[0] else {
        panic!("expected a wikilink");
    };
    assert_eq!(*target, "Target");
    assert_eq!(*label, Some("Label"));

    let embed = "![[shot.png|400]]\n";
    let tree = parse(embed);
    let view = TypedView::new(embed, &tree);
    let InlineMark::WikiEmbed { target, extra, .. } = &view.marks()[0] else {
        panic!("expected an embed");
    };
    assert_eq!(*target, "shot.png");
    assert_eq!(*extra, Some("400"));

    assert!(has_kind("A ==highlight== word\n", SyntaxKind::Highlight));
    assert!(has_kind("Paint {{red|rose}}\n", SyntaxKind::StyleSpan));
    assert!(has_kind("{{font:mono|code}}\n", SyntaxKind::StyleSpan));
    assert!(has_kind("{{size:12px|n}}\n", SyntaxKind::StyleSpan));
    assert!(!has_kind("{{nope|x}}\n", SyntaxKind::StyleSpan));
    assert!(!has_kind("===\n", SyntaxKind::Highlight));
}
