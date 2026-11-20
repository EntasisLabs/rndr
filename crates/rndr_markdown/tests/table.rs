//! GFM tables: grammar, the typed view, edits, and journal recovery.

use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

use rndr_markdown::{
    apply_view, parse, recover, Alignment, Command, EditError, Inline, SyntaxKind, Table,
    TypedBlock, TypedView,
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

fn top_kinds(src: &str) -> Vec<SyntaxKind> {
    parse(src)
        .children()
        .iter()
        .map(|node| node.kind())
        .collect()
}

fn only_table(src: &str) -> Table<'_> {
    let tree = parse(src);
    let view = TypedView::new(src, &tree);
    match view.blocks().as_slice() {
        [TypedBlock::Table(table)] => table.clone(),
        other => panic!("expected one table, got {other:?}\n{src:?}"),
    }
}

fn cell_text<'a>(table: &Table<'a>, body: Option<usize>, index: usize) -> &'a str {
    let row = match body {
        None => &table.header,
        Some(index) => &table.body[index],
    };
    row.cells[index].text
}

fn inline_has(inlines: &[Inline<'_>], kind: SyntaxKind) -> bool {
    inlines
        .iter()
        .any(|inline| inline.kind == kind || inline_has(&inline.children, kind))
}

fn inline_bytes(inlines: &[Inline<'_>]) -> String {
    inlines.iter().map(|inline| inline.text).collect()
}

fn edited(src: &str, cmd: Command) -> String {
    let tree = parse(src);
    let applied = apply_view(src, &tree, &cmd).unwrap_or_else(|err| panic!("{err:?} on {cmd:?}"));
    let full = parse(&applied.bytes);
    assert_eq!(
        applied.tree, full,
        "incremental reparse diverged\nbefore {src:?}\nafter {:?}\ncmd {cmd:?}",
        applied.bytes
    );
    assert_eq!(applied.tree.concat(), applied.bytes);
    assert_eq!(apply_rndr_text(src, &applied), applied.bytes);
    applied.bytes
}

fn apply_rndr_text(src: &str, applied: &rndr_markdown::Applied) -> String {
    rndr_markdown::apply_text(src, &applied.splices)
}

fn cell_command(src: &str, body: Option<usize>, index: usize, text: &str) -> Command {
    let table = only_table(src);
    let row = match body {
        None => &table.header,
        Some(index) => &table.body[index],
    };
    let cell = &row.cells[index];
    Command::SetTableCell {
        start: cell.range.start,
        end: cell.range.end,
        text: text.to_string(),
    }
}

#[test]
fn spec_examples_round_trip_and_expose_alignment() {
    let basic = "| foo | bar |\n| --- | --- |\n| baz | bim |\n";
    assert_lossless(basic);
    let table = only_table(basic);
    assert_eq!(table.alignments, vec![Alignment::None, Alignment::None]);
    assert_eq!(cell_text(&table, None, 0), " foo ");
    assert_eq!(cell_text(&table, None, 1), " bar ");
    assert_eq!(cell_text(&table, Some(0), 0), " baz ");
    assert_eq!(cell_text(&table, Some(0), 1), " bim ");
    assert_eq!(inline_bytes(&table.header.cells[0].inlines), " foo ");

    let aligned = "| abc | defghi |\n:-: | -----------:\nbar | baz\n";
    assert_lossless(aligned);
    let table = only_table(aligned);
    assert_eq!(table.alignments, vec![Alignment::Center, Alignment::Right]);
    assert_eq!(table.body.len(), 1);
    assert_eq!(cell_text(&table, Some(0), 0), "bar ");
    assert_eq!(cell_text(&table, Some(0), 1), " baz");

    let escaped = "| f\\|oo  |\n| ------ |\n| b `\\|` az |\n| b **\\|** im |\n";
    assert_lossless(escaped);
    let table = only_table(escaped);
    assert_eq!(table.columns(), 1);
    assert_eq!(table.body.len(), 2);
    assert!(inline_has(
        &table.header.cells[0].inlines,
        SyntaxKind::Escape
    ));
    assert!(inline_has(
        &table.body[0].cells[0].inlines,
        SyntaxKind::InlineCode
    ));
    assert!(inline_has(
        &table.body[1].cells[0].inlines,
        SyntaxKind::Strong
    ));
    assert_eq!(table.header.cells.len(), 1);
    assert!(table.header.cells[0].text.contains("\\|"));

    let interrupted = "| abc | def |\n| --- | --- |\n| bar | baz |\n> bar\n";
    assert_lossless(interrupted);
    assert_eq!(
        top_kinds(interrupted),
        vec![SyntaxKind::Table, SyntaxKind::BlockQuote]
    );

    let continued = "| abc | def |\n| --- | --- |\n| bar | baz\nbar\n\nbar\n";
    assert_lossless(continued);
    let table = only_table_among(continued);
    assert_eq!(table.body.len(), 2);
    assert_eq!(table.body[1].cells.len(), 1);
    assert_eq!(table.body[1].cells[0].text, "bar");
    assert_eq!(
        top_kinds(continued),
        vec![
            SyntaxKind::Table,
            SyntaxKind::BlockGap,
            SyntaxKind::Paragraph
        ]
    );

    let mismatch = "| abc | def |\n| --- |\n| bar |\n";
    assert_lossless(mismatch);
    assert_eq!(top_kinds(mismatch), vec![SyntaxKind::Paragraph]);
    assert!(!has_kind(mismatch, SyntaxKind::Table));

    let ragged = "| abc | def |\n| --- | --- |\n| bar |\n| bar | baz | boo |\n";
    assert_lossless(ragged);
    let table = only_table(ragged);
    assert_eq!(table.body[0].cells.len(), 1);
    assert_eq!(table.body[1].cells.len(), 3);
    assert_eq!(table.body[1].cells[2].text, " boo ");

    let head_only = "| abc | def |\n| --- | --- |\n";
    assert_lossless(head_only);
    assert!(only_table(head_only).body.is_empty());
}

#[test]
fn extension_cases_column_counts_escapes_and_inlines() {
    let counts = "| a | b | c |\n| --- | --- | ---\n| x\n| a | b\n| 1 | 2 | 3 | 4 | 5 |\n";
    assert_lossless(counts);
    let table = only_table(counts);
    assert_eq!(table.columns(), 3);
    assert_eq!(
        table
            .body
            .iter()
            .map(|row| row.cells.len())
            .collect::<Vec<_>>(),
        vec![1, 2, 5]
    );

    let bad = "| a | b | c |\n| --- | --- |\n| this | isn't | okay |\n";
    assert_lossless(bad);
    assert!(!has_kind(bad, SyntaxKind::Table));

    let pipes = "| a | b |\n| --- | --- |\n| Escaped pipes are \\|okay\\|. | Like \\| this. |\n| Within `\\|code\\| is okay` too. |\n";
    assert_lossless(pipes);
    let table = only_table(pipes);
    assert_eq!(table.body[0].cells.len(), 2);
    assert_eq!(table.body[1].cells.len(), 1);
    assert!(inline_has(
        &table.body[1].cells[0].inlines,
        SyntaxKind::InlineCode
    ));

    // A pipe inside a code span still splits. The block split happens first.
    let splits_code = "| a | b |\n| --- | --- |\n| `a|b` | c |\n";
    assert_lossless(splits_code);
    assert_eq!(only_table(splits_code).body[0].cells.len(), 3);

    let even_backslash = "| a \\\\| b |\n| --- | --- |\n";
    assert_lossless(even_backslash);
    let table = only_table(even_backslash);
    assert_eq!(table.header.cells.len(), 2);
    assert!(table.header.cells[0].text.ends_with("\\\\"));

    let odd_backslash = "| a \\\\\\| b |\n| --- |\n";
    assert_lossless(odd_backslash);
    let table = only_table(odd_backslash);
    assert_eq!(table.columns(), 1);
    assert!(table.header.cells[0].text.contains("\\\\\\|"));

    let one_dash = "| x |\n|-|\n";
    assert_lossless(one_dash);
    assert_eq!(only_table(one_dash).alignments, vec![Alignment::None]);

    let markers =
        "aaa | bbb | ccc | ddd | eee\n:-- | --- | :-: | --- | --:\nfff | ggg | hhh | iii | jjj\n";
    assert_lossless(markers);
    assert_eq!(
        only_table(markers).alignments,
        vec![
            Alignment::Left,
            Alignment::None,
            Alignment::Center,
            Alignment::None,
            Alignment::Right
        ]
    );

    let empty = "| a | b | c |\n| --- | --- | --- |\n| d || e |\n";
    assert_lossless(empty);
    let cells = &only_table(empty).body[0].cells;
    assert_eq!(cells.len(), 3);
    assert_eq!(cells[1].text, "");
    assert_eq!(cells[1].content, cells[1].range.start..cells[1].range.start);

    let loose = "| _abc_ | def |\n| ----- | ---- |\n| *ghi* | `jkl` |\n";
    assert_lossless(loose);
    let table = only_table(loose);
    assert!(inline_has(
        &table.header.cells[0].inlines,
        SyntaxKind::Emphasis
    ));
    assert!(inline_has(
        &table.body[0].cells[0].inlines,
        SyntaxKind::Emphasis
    ));
    assert!(inline_has(
        &table.body[0].cells[1].inlines,
        SyntaxKind::InlineCode
    ));

    let not_table = "| ---- | --- |\n";
    assert_lossless(not_table);
    assert!(!has_kind(not_table, SyntaxKind::Table));

    // A setext underline wins over a one-column delimiter with no pipe.
    assert_eq!(top_kinds("Foo\n---\n"), vec![SyntaxKind::Heading]);
    assert!(!has_kind("Foo\n---\n", SyntaxKind::Table));

    let oddly = "| a |\n--- |\n";
    assert_lossless(oddly);
    assert_eq!(only_table(oddly).columns(), 1);
}

#[test]
fn a_table_can_interrupt_a_paragraph_and_nest() {
    let src = "123\n456\n| a | b |\n| --- | --- |\nd | e\n";
    assert_lossless(src);
    assert_eq!(
        top_kinds(src),
        vec![SyntaxKind::Paragraph, SyntaxKind::Table]
    );
    let tree = parse(src);
    let view = TypedView::new(src, &tree);
    let TypedBlock::Paragraph { text, .. } = &view.blocks()[0] else {
        panic!("paragraph");
    };
    assert_eq!(*text, "123\n456\n");
    let TypedBlock::Table(table) = &view.blocks()[1] else {
        panic!("table");
    };
    assert_eq!(table.header.cells.len(), 2);
    assert_eq!(table.body[0].cells.len(), 2);

    // Column counts disagree, so the lines stay one paragraph.
    let mismatch = "foo\n| a | b |\n| --- |\nbar\n";
    assert_eq!(top_kinds(mismatch), vec![SyntaxKind::Paragraph]);

    let indented = "   | a | b |\n   | --- | --- |\n   | c | d |\n";
    assert_lossless(indented);
    assert_eq!(only_table(indented).alignments.len(), 2);

    let code = "    | a | b |\n    | --- | --- |\n";
    assert_lossless(code);
    assert_eq!(top_kinds(code), vec![SyntaxKind::IndentedCode]);

    let list = "- | a | b |\n  | --- | --- |\n  | c | d |\n";
    assert_lossless(list);
    assert_eq!(top_kinds(list), vec![SyntaxKind::List]);
    let tree = parse(list);
    let tables = TypedView::new(list, &tree).tables();
    assert_eq!(tables.len(), 1);
    assert_eq!(tables[0].header.cells.len(), 2);
    assert_eq!(tables[0].body[0].cells[0].text.trim(), "c");

    let quote = "> | a | b |\n> | --- | --- |\n> | c | d |\n";
    assert_lossless(quote);
    assert_eq!(top_kinds(quote), vec![SyntaxKind::BlockQuote]);
    let tree = parse(quote);
    assert_eq!(TypedView::new(quote, &tree).tables().len(), 1);

    let crlf = "| a | b |\r\n| --- | --- |\r\n| c | d |\r\n";
    assert_lossless(crlf);
    assert_eq!(only_table(crlf).body.len(), 1);
}

#[test]
fn edits_are_minimal_and_keep_padding() {
    let src = "| foo | bar |\n| --- | --- |\n| baz | bim |\n";
    let at = 0;

    let set = edited(src, cell_command(src, None, 0, "qux"));
    assert_eq!(set, "| qux | bar |\n| --- | --- |\n| baz | bim |\n");

    let piped = edited(src, cell_command(src, None, 0, "a|b"));
    assert_eq!(piped, "| a\\|b | bar |\n| --- | --- |\n| baz | bim |\n");
    assert_eq!(only_table(&piped).columns(), 2);
    assert!(only_table(&piped).header.cells[0].text.contains("\\|"));

    let already = edited(src, cell_command(src, Some(0), 1, "bim"));
    assert_eq!(
        already, src,
        "rewriting a cell with its own text is a no-op"
    );

    let insert_row = edited(src, Command::InsertTableRow { at, index: 0 });
    assert_eq!(
        insert_row,
        "| foo | bar |\n| --- | --- |\n|  |  |\n| baz | bim |\n"
    );
    assert_eq!(only_table(&insert_row).body.len(), 2);

    let append_row = edited(src, Command::InsertTableRow { at, index: 1 });
    assert_eq!(
        append_row,
        "| foo | bar |\n| --- | --- |\n| baz | bim |\n|  |  |\n"
    );

    let delete_row = edited(src, Command::DeleteTableRow { at, index: 0 });
    assert_eq!(delete_row, "| foo | bar |\n| --- | --- |\n");
    assert!(only_table(&delete_row).body.is_empty());

    let insert_col = edited(src, Command::InsertTableColumn { at, index: 1 });
    assert_eq!(
        insert_col,
        "| foo |  | bar |\n| --- | --- | --- |\n| baz |  | bim |\n"
    );
    let inserted = only_table(&insert_col);
    assert_eq!(inserted.columns(), 3);
    assert_eq!(inserted.alignments[1], Alignment::None);

    let append_col = edited(src, Command::InsertTableColumn { at, index: 2 });
    assert_eq!(
        append_col,
        "| foo | bar |  |\n| --- | --- | --- |\n| baz | bim |  |\n"
    );

    let delete_col = edited(src, Command::DeleteTableColumn { at, index: 0 });
    assert_eq!(delete_col, "| bar |\n| --- |\n| bim |\n");
    assert_eq!(only_table(&delete_col).columns(), 1);

    let align = edited(
        src,
        Command::SetTableAlignment {
            at,
            index: 1,
            alignment: Alignment::Right,
        },
    );
    assert_eq!(align, "| foo | bar |\n| --- | ---: |\n| baz | bim |\n");
    assert_eq!(
        only_table(&align).alignments,
        vec![Alignment::None, Alignment::Right]
    );

    let center = edited(
        &align,
        Command::SetTableAlignment {
            at,
            index: 0,
            alignment: Alignment::Center,
        },
    );
    assert_eq!(center, "| foo | bar |\n| :---: | ---: |\n| baz | bim |\n");

    let same_align = edited(
        src,
        Command::SetTableAlignment {
            at,
            index: 0,
            alignment: Alignment::None,
        },
    );
    assert_eq!(same_align, src);

    let asymmetric = "|  foo | bar |\n| --- | --- |\n| x | y |\n";
    let shifted = edited(asymmetric, cell_command(asymmetric, None, 0, "z"));
    assert_eq!(shifted, "|  z | bar |\n| --- | --- |\n| x | y |\n");

    let crlf = "| a | b |\r\n| --- | --- |\r\n| c | d |\r\n";
    let added = edited(crlf, Command::InsertTableRow { at, index: 1 });
    assert_eq!(
        added,
        "| a | b |\r\n| --- | --- |\r\n| c | d |\r\n|  |  |\r\n"
    );

    let tight = "a|b\n---|---\nc|d\n";
    let blank = edited(tight, Command::InsertTableRow { at, index: 1 });
    assert_lossless(&blank);
    let table = only_table(&blank);
    assert_eq!(table.columns(), 2);
    assert_eq!(table.body.len(), 2);
    assert_eq!(table.body[1].cells.len(), 2);
}

#[test]
fn a_short_row_grows_a_column_in_place_and_refuses_what_is_not_a_cell() {
    let src = "| a | b | c |\n| --- | --- | --- |\n| x\n";
    let widened = edited(src, Command::InsertTableColumn { at: 0, index: 2 });
    assert_lossless(&widened);
    let table = only_table(&widened);
    assert_eq!(table.columns(), 4);
    // The short row had one cell. Column 2 is past it, so the gap and the
    // new cell are both written and the original cell stays column 0.
    assert_eq!(table.body[0].cells.len(), 3);

    let tree = parse(src);
    let refused = apply_view(
        src,
        &tree,
        &Command::SetTableCell {
            start: 0,
            end: 1,
            text: "nope\n".into(),
        },
    );
    assert_eq!(refused, Err(EditError::Refused));
    assert_eq!(
        apply_view(src, &tree, &Command::DeleteTableRow { at: 0, index: 3 },),
        Err(EditError::Refused)
    );
    assert_eq!(
        apply_view(src, &tree, &Command::DeleteTableColumn { at: 0, index: 9 },),
        Err(EditError::Refused)
    );
    assert_eq!(
        apply_view(
            "paragraph\n",
            &parse("paragraph\n"),
            &Command::InsertTableRow { at: 0, index: 0 },
        ),
        Err(EditError::Refused)
    );
    assert_eq!(
        apply_view(
            src,
            &tree,
            &Command::SetTableCell {
                start: 1,
                end: 0,
                text: "x".into(),
            },
        ),
        Err(EditError::InvalidRange)
    );
}

#[test]
fn a_cell_edit_replays_from_the_journal() {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("rndr-md-table-{nanos}"));
    fs::create_dir_all(&dir).unwrap();
    let doc = dir.join("note.md");
    let journal = dir.join("note.md.journal");
    let src = "| foo | bar |\n| --- | --- |\n| baz | bim |\n";
    fs::write(&doc, src).unwrap();
    let mut session = rndr_markdown::Session::open_file(&doc, &journal).unwrap();
    let cmd = cell_command(src, Some(0), 0, "qux");
    session.edit(cmd, 0).unwrap();
    assert_eq!(
        session.bytes(),
        "| foo | bar |\n| --- | --- |\n| qux | bim |\n"
    );
    session.poll(100).unwrap();
    assert_eq!(fs::read(&doc).unwrap(), src.as_bytes());
    let recovered = recover(&doc, &journal).unwrap();
    assert_eq!(recovered, b"| foo | bar |\n| --- | --- |\n| qux | bim |\n");
    let _ = fs::remove_dir_all(&dir);
}

fn only_table_among(src: &str) -> Table<'_> {
    let tree = parse(src);
    let view = TypedView::new(src, &tree);
    view.blocks()
        .into_iter()
        .find_map(|block| match block {
            TypedBlock::Table(table) => Some(table),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no table in {src:?}"))
}
