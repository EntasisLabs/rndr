use rndr_markdown::{parse, SyntaxKind};

fn assert_lossless(src: &str) {
    let tree = parse(src);
    assert_eq!(tree.concat(), src, "concat {src:?}");
    assert_eq!(parse(&tree.concat()), tree, "reparse {src:?}");
}

fn kinds(src: &str) -> Vec<SyntaxKind> {
    parse(src).children().iter().map(|n| n.kind()).collect()
}

fn has_kind(src: &str, kind: SyntaxKind) -> bool {
    fn walk(node: &rndr_markdown::Green, kind: SyntaxKind) -> bool {
        node.kind() == kind || node.children().iter().any(|c| walk(c, kind))
    }
    walk(&parse(src), kind)
}

#[test]
fn empty_and_gaps_round_trip() {
    for src in [
        "",
        "\n",
        "\n\n",
        "a",
        "a\n",
        "a\r\n",
        "a\r\nb\n",
        "\u{feff}a\n",
        "é\n",
        "a\u{0301}\n",
        "👩‍💻\n",
    ] {
        assert_lossless(src);
    }
}

#[test]
fn emphasis_spelling_survives() {
    assert_lossless("*a*");
    assert_lossless("_a_");
    assert_lossless("**a**");
    assert_lossless("*a *b* c*");
    let star = parse("*a*");
    let under = parse("_a_");
    assert!(star.concat().contains('*'));
    assert!(under.concat().contains('_'));
    assert!(!under.concat().contains('*'));
}

#[test]
fn hard_breaks_and_escapes_and_entities() {
    assert_lossless("a  \nb\n");
    assert_lossless("a\\\nb\n");
    assert_lossless("a\\*b\n");
    assert_lossless("a &amp; b\n");
}

#[test]
fn setext_thematic_lists_and_html() {
    assert_lossless("Foo\n---\n");
    assert_lossless("---\n");
    assert_eq!(kinds("Foo\n---\n"), vec![SyntaxKind::Heading]);
    assert_eq!(kinds("---\n"), vec![SyntaxKind::ThematicBreak]);
    assert_lossless("- a\nb\n");
    assert_lossless("- a\n- b\n");
    assert_lossless("- a\n\n- b\n");
    assert_lossless("<div>\n\nfoo\n");
    assert_lossless("<script>\nfoo\n\nbar\n</script>\n");
}

#[test]
fn liquid_fence_and_opaque_round_trip() {
    assert_lossless("```callout\ntone: tip\n# note\nbody: \"Hi\"\n```\n");
    assert_lossless("```rust\nfn main() {}\n```\n");
    assert_lossless("$$\nx\n$$\n");
}

#[test]
fn reference_definition_stays_in_place() {
    let src = "[foo]: /url \"title\"\n\n[foo]\n";
    assert_lossless(src);
    assert!(has_kind(src, SyntaxKind::LinkRefDef));
    assert!(has_kind(src, SyntaxKind::Link));
}

#[test]
fn hazards_have_the_right_shape() {
    assert_eq!(kinds("- a\nb\n"), vec![SyntaxKind::List]);
    assert_eq!(kinds("- a\n- b\n"), vec![SyntaxKind::List]);
    assert!(has_kind("- a\n\n- b\n", SyntaxKind::BlockGap));
    assert!(!has_kind("- a\n- b\n", SyntaxKind::BlockGap));
    assert_eq!(
        kinds("<div>\n\nfoo\n"),
        vec![
            SyntaxKind::HtmlBlock,
            SyntaxKind::BlockGap,
            SyntaxKind::Paragraph
        ]
    );
    assert_eq!(
        kinds("<script>\nfoo\n\nbar\n</script>\n"),
        vec![SyntaxKind::HtmlBlock]
    );
    assert!(has_kind("*a *b* c*", SyntaxKind::Emphasis));
    assert!(has_kind(
        "```callout\ntone: tip\nbody: Hi\n```\n",
        SyntaxKind::Field
    ));
    assert!(has_kind("_a_", SyntaxKind::Emphasis));
    assert!(has_kind("a  \nb\n", SyntaxKind::HardBreak));
    assert!(has_kind("a\\\nb\n", SyntaxKind::HardBreak));
    assert!(has_kind("a\\*b\n", SyntaxKind::Escape));
    assert!(has_kind("a &amp; b\n", SyntaxKind::Entity));
}
