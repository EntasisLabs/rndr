//! Typed view over a concrete tree.
//!
//! The view borrows spans. It does not own bytes, history, or ids. Liquid
//! fields are the fence's own field nodes; [`rndr_liquid::parse_fence`] is
//! asked only whether the body is valid, and its props are not written back.

use std::ops::Range;

use rndr_liquid::{parse_fence, FENCE_LANGS};

use crate::green::Green;
use crate::kind::SyntaxKind;
use crate::parse;
use crate::table::{self, Alignment};

/// A borrow of one parsed document.
///
/// The source and the tree are borrowed separately. Returned spans point at
/// `source`, so they outlive the tree borrow.
pub struct TypedView<'source, 'tree> {
    source: &'source str,
    root: &'tree Green,
}

impl<'source, 'tree> TypedView<'source, 'tree> {
    /// View `root`, which must be a parse of `source`.
    pub fn new(source: &'source str, root: &'tree Green) -> Self {
        Self { source, root }
    }

    /// Top-level blocks, in source order. Blank lines are [`TypedBlock::Gap`].
    pub fn blocks(&self) -> Vec<TypedBlock<'source>> {
        let mut at = 0;
        let mut out = Vec::new();
        for child in self.root.children() {
            out.push(block_at(self.source, child, at));
            at += child.width();
        }
        out
    }

    /// Every Liquid fence in the tree, including fences nested in prose.
    pub fn liquid_fences(&self) -> Vec<LiquidFence<'source>> {
        let mut out = Vec::new();
        walk_liquid(self.source, self.root, 0, &mut out);
        out
    }

    /// Every GFM table in the tree, including tables nested in a list or quote.
    pub fn tables(&self) -> Vec<Table<'source>> {
        let mut out = Vec::new();
        walk_tables(self.source, self.root, 0, &mut out);
        out
    }
}

/// One top-level block as the editor shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TypedBlock<'a> {
    /// A blank line owned by the document.
    Gap {
        /// Byte range of the line, including its ending.
        range: Range<usize>,
    },
    /// A paragraph. `text` is the paragraph's source, markers included.
    Paragraph {
        /// Source bytes.
        text: &'a str,
        /// Byte range.
        range: Range<usize>,
    },
    /// A heading. The level is read from the marker; the marker bytes stay put.
    Heading {
        /// 1 through 6.
        level: u8,
        /// Byte range.
        range: Range<usize>,
    },
    /// A Liquid fence.
    Liquid(LiquidFence<'a>),
    /// A GFM table. Alignment is read from the delimiter row.
    Table(Table<'a>),
    /// Any other block. Unknown constructs arrive here as opaque blocks.
    Other {
        /// Syntax kind.
        kind: SyntaxKind,
        /// Byte range.
        range: Range<usize>,
    },
}

/// A GFM table. The delimiter row is not a body row; its colons are [`Table::alignments`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Table<'a> {
    /// One alignment per header column, in order.
    pub alignments: Vec<Alignment>,
    /// The header row.
    pub header: TableRow<'a>,
    /// Body rows, which may be shorter or longer than [`Self::header`].
    pub body: Vec<TableRow<'a>>,
    /// Byte range of the delimiter row, including its line ending.
    pub delimiter: Range<usize>,
    /// Byte range of the whole table.
    pub range: Range<usize>,
}

impl<'a> Table<'a> {
    /// Columns declared by the delimiter row.
    pub fn columns(&self) -> usize {
        self.alignments.len()
    }
}

/// One header or body row. Pipes and the line ending sit in [`Self::range`]
/// and outside the cells.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TableRow<'a> {
    /// Cells in source order, including a short row's missing tail and a long
    /// row's extra cells.
    pub cells: Vec<TableCell<'a>>,
    /// Byte range of the row, including pipes and the line ending.
    pub range: Range<usize>,
}

/// One cell. Padding spaces are part of [`Self::text`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TableCell<'a> {
    /// Source bytes, including the padding around the content.
    pub text: &'a str,
    /// Byte range of [`Self::text`].
    pub range: Range<usize>,
    /// Trimmed content inside [`Self::range`]. Empty when the cell is blank.
    pub content: Range<usize>,
    /// Inline nodes of the cell, in source order.
    pub inlines: Vec<Inline<'a>>,
}

/// One inline node. The bytes are a borrow of the cell, not a copy.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Inline<'a> {
    /// Syntax kind.
    pub kind: SyntaxKind,
    /// Source bytes of this node, markers included.
    pub text: &'a str,
    /// Byte range of [`Self::text`].
    pub range: Range<usize>,
    /// Children, when the node is a branch.
    pub children: Vec<Inline<'a>>,
}

/// A Liquid fence and the field spans inside it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiquidFence<'a> {
    /// Info string as written on the opening line, lowercased for matching.
    pub info: &'a str,
    /// Canonical kind. Aliases such as `procedure` stay in [`Self::info`].
    pub canonical: &'static str,
    /// Whether [`parse_fence`] accepts the body.
    pub valid: bool,
    /// Fields, comments, and separators in source order.
    pub fields: Vec<Field<'a>>,
    /// Byte range of the whole fence.
    pub range: Range<usize>,
}

/// One field, comment, or separator inside a fence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Field<'a> {
    /// Key spelling, or empty for a comment or separator.
    pub key: &'a str,
    /// Value bytes, excluding surrounding quotes.
    pub value: &'a str,
    /// Absolute byte range of [`Self::value`].
    pub value_span: Range<usize>,
    /// This node is a `#` comment line.
    pub comment: bool,
    /// The value is markdown for this fence kind.
    pub markdown: bool,
}

impl<'a> Field<'a> {
    /// Nested concrete tree of a markdown value.
    ///
    /// The tree is a parse of the value span. Editing the value is still a
    /// splice of that span on the outer document.
    pub fn markdown_tree(&self) -> Option<Green> {
        self.markdown.then(|| parse(self.value))
    }
}

fn block_at<'a>(source: &'a str, node: &Green, at: usize) -> TypedBlock<'a> {
    let range = at..at + node.width();
    match node.kind() {
        SyntaxKind::BlockGap => TypedBlock::Gap { range },
        SyntaxKind::Paragraph => TypedBlock::Paragraph {
            text: &source[range.clone()],
            range,
        },
        SyntaxKind::Heading => TypedBlock::Heading {
            level: heading_level(node),
            range,
        },
        SyntaxKind::LiquidFence => TypedBlock::Liquid(liquid_at(source, node, at)),
        SyntaxKind::Table => match table_from(source, node, at) {
            Some(table) => TypedBlock::Table(table),
            None => TypedBlock::Other {
                kind: SyntaxKind::Table,
                range,
            },
        },
        kind => TypedBlock::Other { kind, range },
    }
}

fn walk_tables<'a>(source: &'a str, node: &Green, at: usize, out: &mut Vec<Table<'a>>) {
    if node.kind() == SyntaxKind::Table {
        if let Some(table) = table_from(source, node, at) {
            out.push(table);
        }
        return;
    }
    let mut cursor = at;
    for child in node.children() {
        walk_tables(source, child, cursor, out);
        cursor += child.width();
    }
}

fn table_from<'a>(source: &'a str, node: &Green, at: usize) -> Option<Table<'a>> {
    let mut cursor = at;
    let mut rows = Vec::new();
    for child in node.children() {
        let end = cursor + child.width();
        if child.kind() == SyntaxKind::TableRow {
            rows.push((child, cursor..end));
        }
        cursor = end;
    }
    if rows.len() < 2 {
        return None;
    }
    let delimiter = rows[1].1.clone();
    let delimiter_cells = row_cells(source, rows[1].0, rows[1].1.start);
    let alignments = delimiter_cells
        .iter()
        .map(|cell| table::alignment_of(cell.text))
        .collect();
    let header = row_view(source, rows[0].0, rows[0].1.clone());
    let mut body = Vec::new();
    for (row, range) in rows.into_iter().skip(2) {
        body.push(row_view(source, row, range));
    }
    Some(Table {
        alignments,
        header,
        body,
        delimiter,
        range: at..at + node.width(),
    })
}

fn row_view<'a>(source: &'a str, node: &Green, range: Range<usize>) -> TableRow<'a> {
    TableRow {
        cells: row_cells(source, node, range.start),
        range,
    }
}

fn row_cells<'a>(source: &'a str, node: &Green, at: usize) -> Vec<TableCell<'a>> {
    let mut cursor = at;
    let mut cells = Vec::new();
    for child in node.children() {
        let end = cursor + child.width();
        if child.kind() == SyntaxKind::TableCell {
            let text = &source[cursor..end];
            cells.push(TableCell {
                text,
                range: cursor..end,
                content: table::content_span(text, cursor),
                inlines: inline_tree(source, child, cursor),
            });
        }
        cursor = end;
    }
    cells
}

fn inline_tree<'a>(source: &'a str, node: &Green, at: usize) -> Vec<Inline<'a>> {
    let mut cursor = at;
    let mut out = Vec::new();
    for child in node.children() {
        let end = cursor + child.width();
        out.push(Inline {
            kind: child.kind(),
            text: &source[cursor..end],
            range: cursor..end,
            children: inline_tree(source, child, cursor),
        });
        cursor = end;
    }
    out
}

fn heading_level(node: &Green) -> u8 {
    let marker = node
        .children()
        .iter()
        .find(|c| c.kind() == SyntaxKind::AtxMarker)
        .map(Green::concat)
        .unwrap_or_default();
    let hashes = marker.bytes().filter(|b| *b == b'#').count();
    if (1..=6).contains(&hashes) {
        hashes as u8
    } else if node
        .children()
        .iter()
        .any(|c| c.kind() == SyntaxKind::SetextUnderline && c.concat().contains('='))
    {
        1
    } else {
        2
    }
}

fn walk_liquid<'a>(source: &'a str, node: &Green, at: usize, out: &mut Vec<LiquidFence<'a>>) {
    if node.kind() == SyntaxKind::LiquidFence {
        out.push(liquid_at(source, node, at));
    }
    let mut child_at = at;
    for child in node.children() {
        walk_liquid(source, child, child_at, out);
        child_at += child.width();
    }
}

fn liquid_at<'a>(source: &'a str, node: &Green, at: usize) -> LiquidFence<'a> {
    let range = at..at + node.width();
    let mut child_at = at;
    let mut info = "";
    let mut body_start = None;
    let mut body_end = None;
    let mut fields = Vec::new();
    for child in node.children() {
        let child_end = child_at + child.width();
        if child.kind() == SyntaxKind::FenceLine && info.is_empty() {
            info = fence_info(&source[child_at..child_end]);
            body_start = Some(child_end);
        } else if child.kind() == SyntaxKind::FenceLine {
            body_end = Some(child_at);
        } else {
            fields.extend(fields_in(source, child, child_at, ""));
        }
        child_at = child_end;
    }
    let canonical = canonical_kind(info);
    let body = match (body_start, body_end) {
        (Some(start), Some(end)) if end >= start => &source[start..end],
        (Some(start), None) => &source[start..range.end],
        _ => "",
    };
    let kind_for_fields = canonical;
    // Re-walk fields now that the kind is known, so markdown flags are set.
    let mut typed = Vec::new();
    let mut child_at = at;
    for child in node.children() {
        typed.extend(fields_in(source, child, child_at, kind_for_fields));
        child_at += child.width();
    }
    let _ = fields;
    LiquidFence {
        info,
        canonical,
        valid: parse_fence(canonical, body.trim_end_matches(['\n', '\r'])).is_some()
            || parse_fence(info, body.trim_end_matches(['\n', '\r'])).is_some(),
        fields: typed,
        range,
    }
}

fn fence_info(line: &str) -> &str {
    let line = line.trim_end_matches(['\n', '\r']).trim();
    let line = line.trim_start_matches(['`', '~']);
    line.split_whitespace().next().unwrap_or("")
}

fn canonical_kind(info: &str) -> &'static str {
    match info.to_lowercase().as_str() {
        "action_row" => "actions",
        "chip_group" => "chips",
        "procedure" => "recipe",
        other => FENCE_LANGS
            .iter()
            .find(|lang| **lang == other)
            .copied()
            .unwrap_or("code"),
    }
}

fn fields_in<'a>(source: &'a str, node: &Green, at: usize, kind: &str) -> Vec<Field<'a>> {
    match node.kind() {
        SyntaxKind::Field => {
            let mut key = "";
            let mut value = "";
            let mut value_span = at..at;
            let mut cursor = at;
            for child in node.children() {
                let end = cursor + child.width();
                if child.kind() == SyntaxKind::FieldKey {
                    key = &source[cursor..end];
                }
                if child.kind() == SyntaxKind::FieldValue {
                    value = &source[cursor..end];
                    value_span = cursor..end;
                }
                cursor = end;
            }
            vec![Field {
                key,
                value,
                value_span,
                comment: false,
                markdown: is_markdown_field(kind, key),
            }]
        }
        SyntaxKind::FieldComment => vec![Field {
            key: "",
            value: &source[at..at + node.width()],
            value_span: at..at + node.width(),
            comment: true,
            markdown: false,
        }],
        SyntaxKind::FieldSeparator | SyntaxKind::PipeRow | SyntaxKind::BlockGap => Vec::new(),
        _ => {
            let mut out = Vec::new();
            let mut cursor = at;
            for child in node.children() {
                out.extend(fields_in(source, child, cursor, kind));
                cursor += child.width();
            }
            out
        }
    }
}

fn is_markdown_field(kind: &str, key: &str) -> bool {
    let key = key.trim().to_ascii_lowercase();
    matches!(
        (kind, key.as_str()),
        ("card", "summary")
            | (
                "callout"
                    | "brief"
                    | "report"
                    | "section"
                    | "block"
                    | "slides"
                    | "tabs"
                    | "steps"
                    | "recipe"
                    | "accordion",
                "body"
            )
    )
}
