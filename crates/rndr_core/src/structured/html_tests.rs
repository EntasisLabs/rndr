use super::*;

fn texts(blocks: &[Parsed]) -> Vec<String> {
    blocks
        .iter()
        .map(|b| b.runs.iter().map(|r| r.text.as_str()).collect())
        .collect()
}

#[test]
fn parse_keeps_marks_links_and_pills() {
    let blocks = parse(
        "<p>Hi <b>bold</b> <i>it</i> <u>u</u> <s>s</s> <code>c</code> \
         <a href=\"https://x.dev\">link</a> <span data-pill=\"user:1\">@al</span></p>",
    );
    assert_eq!(texts(&blocks), ["Hi bold it u s c link @al"]);
    let find = |t: &str| blocks[0].runs.iter().find(|r| r.text == t).unwrap();
    assert!(find("bold").marks.strong);
    assert!(find("it").marks.emphasis);
    assert!(find("u").marks.underline);
    assert!(find("s").marks.strike);
    assert!(find("c").marks.code);
    assert_eq!(find("link").marks.link.as_deref(), Some("https://x.dev"));
    assert_eq!(find("@al").marks.pill.as_deref(), Some("user:1"));
}

#[test]
fn parse_strips_unsafe_content_and_decodes_entities() {
    let blocks = parse(
        "<html><head><title>t</title><style>p{}</style></head><body>\
         <script>alert(1)</script><p onclick=\"x\">a &amp; b &lt;c&gt; &#x41;&#66;&nbsp;\
         <a href=\"javascript:alert(1)\">bad</a></p><!-- note --></body></html>",
    );
    assert_eq!(texts(&blocks), ["a & b <c> AB bad"]);
    assert!(blocks[0].runs.iter().all(|r| r.marks.link.is_none()));
}

#[test]
fn parse_blocks_lists_quotes_and_styles() {
    let blocks = parse(
        "<h2>Title</h2><ul><li>one<ul><li>nested</li></ul></li><li>two</li></ul>\
         <ol start=\"3\"><li>three</li></ol><blockquote>quoted<br>line</blockquote>\
         <b style=\"font-weight:normal\"><span style=\"font-weight:700;text-decoration:line-through\">g</span></b>\
         <pre>let x;\n  y</pre>",
    );
    assert_eq!(
        texts(&blocks),
        ["Title", "one", "nested", "two", "three", "quoted", "line", "g", "let x;", "  y"]
    );
    assert_eq!(blocks[0].kind, BlockKind::Heading(2));
    assert_eq!(blocks[2].lists, [ListKind::Bullet, ListKind::Bullet]);
    assert_eq!(blocks[4].lists, [ListKind::Ordered { start: 3 }]);
    assert!(blocks[5].quote && blocks[6].quote);
    let g = &blocks[7].runs[0].marks;
    assert!(g.strong && g.strike);
    assert!(blocks[9].runs[0].marks.code);
}
