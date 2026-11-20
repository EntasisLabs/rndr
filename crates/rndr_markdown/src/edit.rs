//! Visual edits become splices. The tree is updated by reparsing a window
//! around the edit. The window grows until both edges are closed cuts of this
//! grammar: a list separated from the edit by only blank lines is included,
//! and a list that still ends on a blank line is not published until the next
//! non-blank line has been parsed. A closed window matches a from-scratch
//! parse. Release builds do not parse the rest of the document to confirm it.

use std::ops::Range;

use crate::green::Green;
use crate::inline::{self, Defs};
use crate::kind::SyntaxKind;
use crate::parse::{self, parse};
use crate::table::{self, Alignment};

/// A single UTF-8 scalar-aligned replacement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Splice {
    /// Start of the replaced range, in bytes.
    pub start: usize,
    /// End of the replaced range, in bytes.
    pub end: usize,
    /// Bytes written in place of the range.
    pub replacement: String,
}

impl Splice {
    /// Whether this splice would change `bytes`.
    pub fn is_noop(&self, bytes: &str) -> bool {
        self.start <= self.end
            && self.end <= bytes.len()
            && bytes.is_char_boundary(self.start)
            && bytes.is_char_boundary(self.end)
            && bytes[self.start..self.end] == self.replacement
    }
}

/// Why a command was not applied. The document is unchanged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditError {
    /// A byte offset splits a scalar.
    InvalidRange,
    /// The command cannot be expressed by a local splice.
    Refused,
    /// A composition owns the caret. Commit or cancel it first.
    Composing,
}

/// One command the typed view can issue.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    /// Insert `text` at a byte offset. Active markup characters are escaped
    /// when the raw insert would become syntax.
    Insert {
        /// Byte offset.
        at: usize,
        /// Characters to insert.
        text: String,
    },
    /// Delete a byte range.
    Delete {
        /// Start of the range.
        start: usize,
        /// End of the range.
        end: usize,
    },
    /// Toggle emphasis on a byte range. An existing `_` or `*` pair is reused.
    ToggleEmphasis {
        /// Start of the emphasized text.
        start: usize,
        /// End of the emphasized text.
        end: usize,
    },
    /// Toggle strong on a byte range. An existing `**` or `__` pair is reused.
    ToggleStrong {
        /// Start of the strong text.
        start: usize,
        /// End of the strong text.
        end: usize,
    },
    /// Toggle strikethrough on a byte range. An existing `~~` pair is reused.
    ToggleStrike {
        /// Start of the struck text.
        start: usize,
        /// End of the struck text.
        end: usize,
    },
    /// Toggle inline code on a byte range. An existing backtick run is reused.
    /// A new span uses a run long enough for the body, with the padding space
    /// CommonMark requires when the body starts or ends with a backtick.
    ToggleCode {
        /// Start of the code text.
        start: usize,
        /// End of the code text.
        end: usize,
    },
    /// Toggle a link on a byte range. An existing link whose label is exactly
    /// that range is unwrapped. Otherwise the printer inserts `[text](destination)`.
    ToggleLink {
        /// Start of the label.
        start: usize,
        /// End of the label.
        end: usize,
        /// Destination used when the span is not already a link.
        destination: String,
    },
    /// Split a block at a byte offset by inserting a blank line.
    Split {
        /// Byte offset.
        at: usize,
    },
    /// Replace a Liquid field's value span. Quotes around that span stay.
    SetField {
        /// Start of the value span.
        start: usize,
        /// End of the value span.
        end: usize,
        /// New value bytes.
        value: String,
    },
    /// Insert printer output that has no source span yet.
    InsertNew {
        /// Byte offset.
        at: usize,
        /// Printer output.
        text: String,
    },
    /// Insert a body row. `at` is any byte offset inside the table. `index`
    /// is the body-row index; `index == body_len` appends after the last row.
    ///
    /// The new row copies a neighboring row's pipes, indent, and cell padding.
    /// A tight row with no outer pipe gains the pipes an empty cell needs so
    /// the column count stays put. The delimiter row is not copied.
    InsertTableRow {
        /// Byte offset inside the table.
        at: usize,
        /// Body row index, or the body length to append.
        index: usize,
    },
    /// Delete a body row, including its line ending. The header and the
    /// delimiter stay.
    DeleteTableRow {
        /// Byte offset inside the table.
        at: usize,
        /// Body row index.
        index: usize,
    },
    /// Insert a column at `index`. `index == columns` appends. Each row gains
    /// one cell. A short row is padded with empty cells out to `index` so the
    /// new cell lands in that column. Padding is copied from the neighbor cell.
    /// The delimiter cell is a hyphen run of the neighbor's length, unmarked.
    InsertTableColumn {
        /// Byte offset inside the table.
        at: usize,
        /// Column index, or the column count to append.
        index: usize,
    },
    /// Delete column `index` from the header, the delimiter, and every body
    /// row that has that cell. A shorter row is left alone.
    DeleteTableColumn {
        /// Byte offset inside the table.
        at: usize,
        /// Column index.
        index: usize,
    },
    /// Set one delimiter cell's alignment. The dash run and the surrounding
    /// padding stay; only the colons change.
    SetTableAlignment {
        /// Byte offset inside the table.
        at: usize,
        /// Column index.
        index: usize,
        /// Alignment written into the delimiter cell.
        alignment: Alignment,
    },
    /// Replace a cell's text. `start..end` is the cell's range or its trimmed
    /// content range. Padding outside the trimmed content stays. `|` is
    /// written as `\|` so the column does not split. A newline is refused.
    SetTableCell {
        /// Start of the cell, or of its trimmed content.
        start: usize,
        /// End of that same span.
        end: usize,
        /// Replacement text. `|` is escaped. Newlines are refused.
        text: String,
    },
    /// Replace a source range with bytes that are already markdown.
    ///
    /// Unlike [`Command::Insert`], the replacement is not escaped. Structural
    /// commands (a heading marker, a list marker, a pasted markdown fragment)
    /// use this so the reparse can see the new syntax.
    Replace {
        /// Start of the range.
        start: usize,
        /// End of the range.
        end: usize,
        /// Bytes written in place of the range.
        text: String,
    },
}

/// The splice list and the tree produced by applying a command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Applied {
    /// Splices that were applied, in order. Empty when the command was a no-op.
    pub splices: Vec<Splice>,
    /// Document bytes after the splices.
    pub bytes: String,
    /// Concrete tree of [`Self::bytes`].
    pub tree: Green,
    /// The reparse grew until it covered the whole document.
    pub widened_to_root: bool,
    /// Top-level children replaced in place, when the root node was reused.
    pub(crate) replaced: Option<std::ops::Range<usize>>,
}

/// Apply `splices` to `bytes`. Ranges are interpreted against the original
/// `bytes` and must be nonoverlapping and in order.
pub fn apply_text(bytes: &str, splices: &[Splice]) -> String {
    if splices.is_empty() {
        return bytes.to_string();
    }
    let mut out = String::new();
    let mut cursor = 0;
    for splice in splices {
        out.push_str(&bytes[cursor..splice.start]);
        out.push_str(&splice.replacement);
        cursor = splice.end;
    }
    out.push_str(&bytes[cursor..]);
    out
}

/// Compile `cmd` and reparse. The returned tree matches [`parse`] of the
/// resulting bytes: the window is a closed cut, or the reparse grew to the root.
pub fn apply_view(bytes: &str, tree: &Green, cmd: &Command) -> Result<Applied, EditError> {
    apply_owned(bytes, tree.clone(), cmd).map_err(|(_, err)| err)
}

/// [`apply_view`] taking ownership of `tree` so a uniquely held root can be
/// grafted without cloning every sibling.
pub(crate) fn apply_owned(
    bytes: &str,
    tree: Green,
    cmd: &Command,
) -> Result<Applied, (Green, EditError)> {
    let splices = match compile(bytes, &tree, cmd) {
        Ok(splices) => splices,
        Err(err) => return Err((tree, err)),
    };
    let splices: Vec<Splice> = splices.into_iter().filter(|s| !s.is_noop(bytes)).collect();
    if splices.is_empty() {
        return Ok(Applied {
            splices,
            bytes: bytes.to_string(),
            tree,
            widened_to_root: false,
            replaced: None,
        });
    }
    let (next, next_tree, widened, replaced) = reparse_all(tree, bytes, &splices);
    debug_assert_eq!(next_tree.concat(), next);
    Ok(Applied {
        splices,
        bytes: next,
        tree: next_tree,
        widened_to_root: widened,
        replaced,
    })
}

/// Canonical bytes for a new Liquid fence. Existing fences are not reprinted.
pub fn print_liquid(kind: &str, fields: &[(&str, &str)]) -> String {
    let mut out = format!("```{kind}\n");
    for (key, value) in fields {
        if value.contains('\n') {
            out.push_str(key);
            out.push_str(": |\n");
            for line in value.split('\n') {
                out.push_str(line);
                out.push('\n');
            }
        } else {
            out.push_str(key);
            out.push_str(": ");
            out.push_str(value);
            out.push('\n');
        }
    }
    out.push_str("```\n");
    out
}

fn compile(bytes: &str, tree: &Green, cmd: &Command) -> Result<Vec<Splice>, EditError> {
    match cmd {
        Command::Insert { at, text } => {
            boundary(bytes, *at)?;
            if text.is_empty() {
                return Ok(vec![]);
            }
            let raw = Splice {
                start: *at,
                end: *at,
                replacement: text.clone(),
            };
            if inserted_is_literal(bytes, &raw) {
                return Ok(vec![raw]);
            }
            Ok(vec![Splice {
                start: *at,
                end: *at,
                replacement: escape_literal(text),
            }])
        }
        Command::Delete { start, end } => {
            boundary(bytes, *start)?;
            boundary(bytes, *end)?;
            if *start > *end {
                return Err(EditError::InvalidRange);
            }
            Ok(vec![Splice {
                start: *start,
                end: *end,
                replacement: String::new(),
            }])
        }
        Command::ToggleEmphasis { start, end } => {
            toggle_mark(bytes, tree, *start, *end, SyntaxKind::Emphasis, "*", "*")
        }
        Command::ToggleStrong { start, end } => {
            toggle_mark(bytes, tree, *start, *end, SyntaxKind::Strong, "**", "**")
        }
        Command::ToggleStrike { start, end } => toggle_mark(
            bytes,
            tree,
            *start,
            *end,
            SyntaxKind::Strikethrough,
            "~~",
            "~~",
        ),
        Command::ToggleCode { start, end } => {
            boundary(bytes, *start)?;
            boundary(bytes, *end)?;
            if *start > *end {
                return Err(EditError::InvalidRange);
            }
            if *start == *end {
                return Ok(vec![]);
            }
            if let Some(existing) = mark_covering(tree, 0, *start, *end, SyntaxKind::InlineCode) {
                return Ok(existing);
            }
            let body = &bytes[*start..*end];
            let (open, close) = code_delimiters(body);
            Ok(vec![
                Splice {
                    start: *start,
                    end: *start,
                    replacement: open,
                },
                Splice {
                    start: *end,
                    end: *end,
                    replacement: close,
                },
            ])
        }
        Command::ToggleLink {
            start,
            end,
            destination,
        } => {
            boundary(bytes, *start)?;
            boundary(bytes, *end)?;
            if *start > *end {
                return Err(EditError::InvalidRange);
            }
            if *start == *end {
                return Ok(vec![]);
            }
            if let Some(existing) = link_covering(tree, 0, *start, *end) {
                return Ok(existing);
            }
            let dest = link_destination(destination);
            Ok(vec![
                Splice {
                    start: *start,
                    end: *start,
                    replacement: "[".to_string(),
                },
                Splice {
                    start: *end,
                    end: *end,
                    replacement: format!("]({dest})"),
                },
            ])
        }
        Command::Split { at } => {
            boundary(bytes, *at)?;
            Ok(vec![Splice {
                start: *at,
                end: *at,
                replacement: "\n\n".to_string(),
            }])
        }
        Command::SetField { start, end, value }
        | Command::Replace {
            start,
            end,
            text: value,
        } => {
            boundary(bytes, *start)?;
            boundary(bytes, *end)?;
            if *start > *end {
                return Err(EditError::InvalidRange);
            }
            Ok(vec![Splice {
                start: *start,
                end: *end,
                replacement: value.clone(),
            }])
        }
        Command::InsertNew { at, text } => {
            boundary(bytes, *at)?;
            Ok(vec![Splice {
                start: *at,
                end: *at,
                replacement: text.clone(),
            }])
        }
        Command::InsertTableRow { at, index } => table_edits(
            bytes,
            tree,
            &table::TableEdit::InsertRow {
                at: *at,
                index: *index,
            },
        ),
        Command::DeleteTableRow { at, index } => table_edits(
            bytes,
            tree,
            &table::TableEdit::DeleteRow {
                at: *at,
                index: *index,
            },
        ),
        Command::InsertTableColumn { at, index } => table_edits(
            bytes,
            tree,
            &table::TableEdit::InsertColumn {
                at: *at,
                index: *index,
            },
        ),
        Command::DeleteTableColumn { at, index } => table_edits(
            bytes,
            tree,
            &table::TableEdit::DeleteColumn {
                at: *at,
                index: *index,
            },
        ),
        Command::SetTableAlignment {
            at,
            index,
            alignment,
        } => table_edits(
            bytes,
            tree,
            &table::TableEdit::SetAlignment {
                at: *at,
                index: *index,
                alignment: *alignment,
            },
        ),
        Command::SetTableCell { start, end, text } => table_edits(
            bytes,
            tree,
            &table::TableEdit::SetCell {
                start: *start,
                end: *end,
                text: text.clone(),
            },
        ),
    }
}

fn table_edits(
    bytes: &str,
    tree: &Green,
    edit: &table::TableEdit,
) -> Result<Vec<Splice>, EditError> {
    table::plan(bytes, tree, edit)
        .map(|edits| {
            edits
                .into_iter()
                .map(|edit| Splice {
                    start: edit.start,
                    end: edit.end,
                    replacement: edit.replacement,
                })
                .collect()
        })
        .map_err(|fault| match fault {
            table::TableFault::Range => EditError::InvalidRange,
            table::TableFault::Refused => EditError::Refused,
        })
}

fn boundary(bytes: &str, at: usize) -> Result<(), EditError> {
    if at <= bytes.len() && bytes.is_char_boundary(at) {
        Ok(())
    } else {
        Err(EditError::InvalidRange)
    }
}

fn escape_literal(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        if matches!(
            c,
            '*' | '_'
                | '~'
                | '['
                | ']'
                | '('
                | ')'
                | '\\'
                | '#'
                | '!'
                | '&'
                | '<'
                | '>'
                | '`'
                | '$'
                | '='
                | '{'
                | '}'
        ) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

fn inserted_is_literal(bytes: &str, splice: &Splice) -> bool {
    if !splice.replacement.chars().any(is_active) {
        return true;
    }
    let new_bytes = apply_text(bytes, std::slice::from_ref(splice));
    let tree = parse(&new_bytes);
    let range = splice.start..splice.start + splice.replacement.len();
    leaves_are_literal(&tree, 0, range)
}

fn is_active(c: char) -> bool {
    matches!(
        c,
        '*' | '_'
            | '~'
            | '['
            | ']'
            | '('
            | ')'
            | '\\'
            | '#'
            | '!'
            | '&'
            | '<'
            | '>'
            | '`'
            | '$'
            | '='
            | '{'
            | '}'
            | '\n'
    )
}

fn leaves_are_literal(node: &Green, offset: usize, range: Range<usize>) -> bool {
    if node.children().is_empty() {
        let end = offset + node.width();
        if offset < range.end && end > range.start {
            return matches!(
                node.kind(),
                SyntaxKind::Text
                    | SyntaxKind::SoftBreak
                    | SyntaxKind::HardBreak
                    | SyntaxKind::LineEnding
                    | SyntaxKind::Escape
                    | SyntaxKind::FieldValue
                    | SyntaxKind::LinePrefix
                    | SyntaxKind::BlockGap
            );
        }
        return true;
    }
    let mut at = offset;
    for child in node.children() {
        if !leaves_are_literal(child, at, range.clone()) {
            return false;
        }
        at += child.width();
    }
    true
}

fn toggle_mark(
    bytes: &str,
    tree: &Green,
    start: usize,
    end: usize,
    kind: SyntaxKind,
    open: &str,
    close: &str,
) -> Result<Vec<Splice>, EditError> {
    boundary(bytes, start)?;
    boundary(bytes, end)?;
    if start > end {
        return Err(EditError::InvalidRange);
    }
    if start == end {
        return Ok(vec![]);
    }
    if let Some(existing) = mark_covering(tree, 0, start, end, kind) {
        return Ok(existing);
    }
    Ok(vec![
        Splice {
            start,
            end: start,
            replacement: open.to_string(),
        },
        Splice {
            start: end,
            end,
            replacement: close.to_string(),
        },
    ])
}

fn mark_covering(
    node: &Green,
    offset: usize,
    start: usize,
    end: usize,
    kind: SyntaxKind,
) -> Option<Vec<Splice>> {
    if node.kind() == kind {
        let children = node.children();
        if children.len() >= 2
            && children
                .first()
                .is_some_and(|c| c.kind() == SyntaxKind::Delimiter)
            && children
                .last()
                .is_some_and(|c| c.kind() == SyntaxKind::Delimiter)
        {
            let open = children[0].width();
            let close = children.last().unwrap().width();
            let inner_start = offset + open;
            let inner_end = offset + node.width() - close;
            if inner_start == start && inner_end == end {
                let close_at = offset + node.width() - close;
                return Some(vec![
                    Splice {
                        start: offset,
                        end: offset + open,
                        replacement: String::new(),
                    },
                    Splice {
                        start: close_at,
                        end: close_at + close,
                        replacement: String::new(),
                    },
                ]);
            }
        }
    }
    let mut at = offset;
    for child in node.children() {
        if let Some(found) = mark_covering(child, at, start, end, kind) {
            return Some(found);
        }
        at += child.width();
    }
    None
}

fn code_delimiters(body: &str) -> (String, String) {
    let mut longest = 0usize;
    let mut run = 0usize;
    for c in body.chars() {
        if c == '`' {
            run += 1;
            longest = longest.max(run);
        } else {
            run = 0;
        }
    }
    let ticks = "`".repeat(longest + 1);
    if body.starts_with('`') || body.ends_with('`') {
        (format!("{ticks} "), format!(" {ticks}"))
    } else {
        (ticks.clone(), ticks)
    }
}

fn link_destination(destination: &str) -> String {
    if destination.contains(|c: char| c.is_whitespace() || c == '(' || c == ')') {
        format!("<{destination}>")
    } else {
        destination.to_string()
    }
}

fn link_covering(node: &Green, offset: usize, start: usize, end: usize) -> Option<Vec<Splice>> {
    if node.kind() == SyntaxKind::Link {
        let children = node.children();
        if let Some(open) = children
            .first()
            .filter(|c| c.kind() == SyntaxKind::Delimiter)
        {
            let mut at = offset + open.width();
            let mut inner_end = None;
            let mut close_at = None;
            for child in children.iter().skip(1) {
                if child.kind() == SyntaxKind::Delimiter && child.concat() == "]" {
                    inner_end = Some(at);
                    close_at = Some(at);
                    break;
                }
                at += child.width();
            }
            if inner_end == Some(end) && offset + open.width() == start {
                let close_at = close_at?;
                return Some(vec![
                    Splice {
                        start: offset,
                        end: offset + open.width(),
                        replacement: String::new(),
                    },
                    Splice {
                        start: close_at,
                        end: offset + node.width(),
                        replacement: String::new(),
                    },
                ]);
            }
        }
    }
    let mut at = offset;
    for child in node.children() {
        if let Some(found) = link_covering(child, at, start, end) {
            return Some(found);
        }
        at += child.width();
    }
    None
}

fn reparse_all(
    tree: Green,
    bytes: &str,
    splices: &[Splice],
) -> (String, Green, bool, Option<std::ops::Range<usize>>) {
    if splices.len() == 1 {
        let splice = &splices[0];
        let next = apply_text(bytes, splices);
        let delta = next.len() as isize - bytes.len() as isize;
        let re = reparse_span(tree, bytes, &next, splice.start, splice.end, delta);
        return (next, re.tree, re.widened_to_root, re.replaced);
    }
    let mut tree = tree;
    let mut text = bytes.to_string();
    let mut widened = false;
    let mut replaced = None;
    let mut applied_delta: isize = 0;
    for splice in splices {
        let start = (splice.start as isize + applied_delta) as usize;
        let end = (splice.end as isize + applied_delta) as usize;
        let one = Splice {
            start,
            end,
            replacement: splice.replacement.clone(),
        };
        let next = apply_text(&text, std::slice::from_ref(&one));
        let delta = next.len() as isize - text.len() as isize;
        let re = reparse_span(tree, &text, &next, start, end, delta);
        widened |= re.widened_to_root;
        replaced = re.replaced;
        applied_delta += delta;
        text = next;
        tree = re.tree;
    }
    (text, tree, widened, replaced)
}

struct Reparsed {
    tree: Green,
    widened_to_root: bool,
    replaced: Option<std::ops::Range<usize>>,
}

fn reparse_span(
    mut old: Green,
    _old_bytes: &str,
    new_bytes: &str,
    edit_start: usize,
    edit_end: usize,
    delta: isize,
) -> Reparsed {
    let ranges = child_ranges(&old);
    if ranges.is_empty() {
        return Reparsed {
            tree: parse(new_bytes),
            widened_to_root: true,
            replaced: None,
        };
    }
    let mut lo = 0;
    let mut hi = ranges.len();
    let mut found = false;
    for (i, range) in ranges.iter().enumerate() {
        let (start, end) = (range.0, range.1);
        let hits = if edit_start == edit_end {
            start <= edit_start && edit_start <= end
        } else {
            start < edit_end && end > edit_start
        };
        if hits {
            if !found {
                lo = i;
                found = true;
            }
            hi = i + 1;
        }
    }
    if !found {
        return Reparsed {
            tree: parse(new_bytes),
            widened_to_root: true,
            replaced: None,
        };
    }
    lo = lo.saturating_sub(1);
    if hi < ranges.len() {
        hi += 1;
    }
    loop {
        let win_lo = ranges[lo].0;
        let win_hi = ranges[hi - 1].1;
        let new_lo = map_pos(win_lo, edit_start, edit_end, delta);
        let new_hi = map_pos(win_hi, edit_start, edit_end, delta);
        let (follow_end, follow_old) = if hi < ranges.len() {
            (map_pos(ranges[hi].1, edit_start, edit_end, delta), Some(hi))
        } else {
            (new_hi, None)
        };
        if new_lo > new_hi
            || follow_end < new_hi
            || follow_end > new_bytes.len()
            || new_hi > new_bytes.len()
        {
            if lo == 0 && hi == ranges.len() {
                return Reparsed {
                    tree: parse(new_bytes),
                    widened_to_root: true,
                    replaced: None,
                };
            }
        } else {
            let slice = &new_bytes[new_lo..follow_end];
            // Definitions only change inlines that contain `[`. A window
            // without one resolves the same way with an empty map, and the
            // scan of every other block stays off this path.
            let (defs_before, defs_after) = if slice.contains('[') {
                (defs_before(&old, win_lo), defs_after(&old, win_hi))
            } else {
                (Defs::default(), Defs::default())
            };
            let parsed = parse::parse_with(slice, &defs_before, &defs_after);
            // A list stops at a blank line only after it has seen the next
            // non-blank line. Ending the slice on that blank makes the gap
            // look like a sibling, while a from-scratch parse may keep the
            // blank and pull the later item into the list.
            let right_open = list_lookahead_open(parsed.children(), follow_end, new_bytes.len());
            let left_at = cover_affected_list(&ranges, lo, edit_start, edit_end);
            if !right_open && left_at == lo {
                let follow = follow_old.map(|i| (ranges[i].2, &new_bytes[new_hi..follow_end]));
                if let Some(window) = resynced(parsed.children(), new_hi - new_lo, follow) {
                    let prefix: usize = old.children()[..lo].iter().map(Green::width).sum();
                    let middle: usize = window.iter().map(Green::width).sum();
                    let suffix: usize = old.children()[hi..].iter().map(Green::width).sum();
                    if prefix + middle + suffix == new_bytes.len() {
                        let touched = defs_touched(&old.children()[lo..hi], &window);
                        let same_len = window.len() == hi - lo;
                        let old_defs = touched.then(|| parse::definitions_in(&old));
                        let owned = std::mem::replace(
                            &mut old,
                            Green::branch(SyntaxKind::Document, Vec::new()),
                        );
                        let mut tree = graft(owned, lo, hi, window);
                        if let Some(old_defs) = old_defs {
                            let new_defs = parse::definitions_in(&tree);
                            if !same_defs(&old_defs, &new_defs) {
                                tree = parse::reresolve(&tree, &new_defs);
                            }
                        }
                        #[cfg(debug_assertions)]
                        {
                            assert_eq!(tree.concat(), new_bytes);
                            assert_eq!(
                                tree,
                                parse(new_bytes),
                                "incremental reparse diverged from a full parse"
                            );
                        }
                        return Reparsed {
                            tree,
                            widened_to_root: lo == 0 && hi == ranges.len(),
                            replaced: same_len.then_some(lo..hi),
                        };
                    }
                }
            }
            if widen_for_list(&mut lo, &mut hi, &ranges, left_at, right_open) {
                continue;
            }
        }
        if lo == 0 && hi == ranges.len() {
            return Reparsed {
                tree: parse(new_bytes),
                widened_to_root: true,
                replaced: None,
            };
        }
        let old_lo = lo;
        let old_hi = hi;
        lo = lo.saturating_sub(1);
        if hi < ranges.len() {
            hi += 1;
        }
        if lo == old_lo && hi == old_hi {
            return Reparsed {
                tree: parse(new_bytes),
                widened_to_root: true,
                replaced: None,
            };
        }
    }
}

/// A slice that ends in a list and then only blank lines has not decided
/// whether those blanks belong to the list. At EOF the decision is final.
fn list_lookahead_open(nodes: &[Green], slice_end: usize, doc_len: usize) -> bool {
    if slice_end >= doc_len {
        return false;
    }
    let mut i = nodes.len();
    while i > 0 && nodes[i - 1].kind() == SyntaxKind::BlockGap {
        i -= 1;
    }
    i < nodes.len() && i > 0 && nodes[i - 1].kind() == SyntaxKind::List
}

/// Index of a list that sits before `lo` with only blank lines between it and
/// the window, when the edit touches the blanks or the following block.
/// Otherwise `lo`.
fn cover_affected_list(
    ranges: &[(usize, usize, SyntaxKind)],
    lo: usize,
    edit_start: usize,
    edit_end: usize,
) -> usize {
    let mut i = lo;
    while i > 0 && ranges[i - 1].2 == SyntaxKind::BlockGap {
        i -= 1;
    }
    if i == 0 || ranges[i - 1].2 != SyntaxKind::List {
        return lo;
    }
    let list_idx = i - 1;
    let region_start = ranges[list_idx].1;
    let mut continuation = list_idx + 1;
    while continuation < ranges.len() && ranges[continuation].2 == SyntaxKind::BlockGap {
        continuation += 1;
    }
    let region_end = if continuation < ranges.len() {
        ranges[continuation].1
    } else {
        ranges.last().map(|range| range.1).unwrap_or(region_start)
    };
    let touches = if edit_start == edit_end {
        edit_start >= region_start && edit_start < region_end
    } else {
        edit_start < region_end && edit_end > region_start
    };
    if touches {
        list_idx
    } else {
        lo
    }
}

/// Follow index of the next non-blank block after `hi`. That block is the line
/// a list examines once it has seen a run of blanks. `hi` itself stays the
/// current follow, so the returned index is greater when more siblings exist.
fn cover_list_lookahead(ranges: &[(usize, usize, SyntaxKind)], hi: usize) -> usize {
    if hi >= ranges.len() {
        return hi;
    }
    let mut i = hi + 1;
    while i < ranges.len() && ranges[i].2 == SyntaxKind::BlockGap {
        i += 1;
    }
    i
}

fn widen_for_list(
    lo: &mut usize,
    hi: &mut usize,
    ranges: &[(usize, usize, SyntaxKind)],
    left_at: usize,
    right_open: bool,
) -> bool {
    let old_lo = *lo;
    let old_hi = *hi;
    if left_at < *lo {
        *lo = left_at;
    }
    if right_open {
        let jumped = cover_list_lookahead(ranges, *hi);
        if jumped > *hi {
            *hi = jumped;
        }
    }
    *lo != old_lo || *hi != old_hi
}

fn graft(mut old: Green, lo: usize, hi: usize, window: Vec<Green>) -> Green {
    match old.splice_children(lo, hi, window) {
        Ok(()) => old,
        Err(window) => {
            let mut children = Vec::with_capacity(old.children().len() - (hi - lo) + window.len());
            children.extend(old.children()[..lo].iter().cloned());
            children.extend(window);
            children.extend(old.children()[hi..].iter().cloned());
            Green::branch(old.kind(), children)
        }
    }
}

fn child_ranges(doc: &Green) -> Vec<(usize, usize, SyntaxKind)> {
    let mut at = 0;
    let mut out = Vec::with_capacity(doc.children().len());
    for child in doc.children() {
        let start = at;
        at += child.width();
        out.push((start, at, child.kind()));
    }
    out
}

fn map_pos(pos: usize, edit_start: usize, edit_end: usize, delta: isize) -> usize {
    if pos <= edit_start {
        pos
    } else if pos >= edit_end {
        (pos as isize + delta).max(0) as usize
    } else {
        edit_start
    }
}

fn resynced(
    nodes: &[Green],
    window: usize,
    follow: Option<(SyntaxKind, &str)>,
) -> Option<Vec<Green>> {
    let mut acc = 0;
    let mut split = None;
    for (i, node) in nodes.iter().enumerate() {
        if acc == window {
            split = Some(i);
            break;
        }
        acc += node.width();
        if acc > window {
            return None;
        }
    }
    if split.is_none() && acc == window {
        split = Some(nodes.len());
    }
    let split = split?;
    let suffix = &nodes[split..];
    match follow {
        None => suffix.is_empty().then(|| nodes[..split].to_vec()),
        Some((kind, bytes)) => {
            if suffix.len() == 1 && suffix[0].kind() == kind && suffix[0].concat() == bytes {
                Some(nodes[..split].to_vec())
            } else {
                None
            }
        }
    }
}

fn defs_before(tree: &Green, end: usize) -> Defs {
    let mut defs = Defs::default();
    collect_defs_between(tree, 0, 0, end, &mut defs);
    defs
}

fn defs_after(tree: &Green, start: usize) -> Defs {
    let mut defs = Defs::default();
    collect_defs_between(tree, 0, start, tree.width(), &mut defs);
    defs
}

fn collect_defs_between(node: &Green, at: usize, start: usize, end: usize, defs: &mut Defs) {
    let node_end = at + node.width();
    if node_end <= start || at >= end {
        return;
    }
    if node.kind() == SyntaxKind::LinkRefDef && at >= start && node_end <= end {
        let text = node.concat();
        if let Some((label, dest, _)) = inline::parse_reference_def(&text, 0, text.len()) {
            defs.insert_first(&label, &dest);
        }
        return;
    }
    let mut child_at = at;
    for child in node.children() {
        collect_defs_between(child, child_at, start, end, defs);
        child_at += child.width();
    }
}

fn defs_touched(old_span: &[Green], window: &[Green]) -> bool {
    old_span.iter().any(contains_refdef) || window.iter().any(contains_refdef)
}

fn contains_refdef(node: &Green) -> bool {
    node.kind() == SyntaxKind::LinkRefDef || node.children().iter().any(contains_refdef)
}

fn same_defs(left: &Defs, right: &Defs) -> bool {
    let a: Vec<_> = left
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    let b: Vec<_> = right
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    a == b
}
