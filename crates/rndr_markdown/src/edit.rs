//! Visual edits become splices. The tree is updated by reparsing the edited
//! block and its neighboring siblings, widening toward the root when that
//! local parse does not resync with the following block.

use std::ops::Range;

use crate::green::Green;
use crate::inline::{self, Defs};
use crate::kind::SyntaxKind;
use crate::parse::{self, parse};

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
/// resulting bytes when the local reparse resyncs or widens to the root.
pub fn apply_view(bytes: &str, tree: &Green, cmd: &Command) -> Result<Applied, EditError> {
    let splices = compile(bytes, tree, cmd)?;
    let splices: Vec<Splice> = splices.into_iter().filter(|s| !s.is_noop(bytes)).collect();
    if splices.is_empty() {
        return Ok(Applied {
            splices,
            bytes: bytes.to_string(),
            tree: tree.clone(),
            widened_to_root: false,
        });
    }
    let (next, next_tree, widened) = reparse_all(tree, bytes, &splices);
    debug_assert_eq!(next_tree.concat(), next);
    Ok(Applied {
        splices,
        bytes: next,
        tree: next_tree,
        widened_to_root: widened,
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
            boundary(bytes, *start)?;
            boundary(bytes, *end)?;
            if *start > *end {
                return Err(EditError::InvalidRange);
            }
            if *start == *end {
                return Ok(vec![]);
            }
            if let Some(existing) = emphasis_covering(tree, 0, *start, *end) {
                return Ok(existing);
            }
            Ok(vec![
                Splice {
                    start: *start,
                    end: *start,
                    replacement: "*".to_string(),
                },
                Splice {
                    start: *end,
                    end: *end,
                    replacement: "*".to_string(),
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
        Command::SetField { start, end, value } => {
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
    }
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
            '*' | '_' | '~' | '[' | ']' | '(' | ')' | '\\' | '#' | '!' | '&' | '<' | '>' | '`'
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
        '*' | '_' | '~' | '[' | ']' | '(' | ')' | '\\' | '#' | '!' | '&' | '<' | '>' | '`' | '\n'
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

fn emphasis_covering(node: &Green, offset: usize, start: usize, end: usize) -> Option<Vec<Splice>> {
    if matches!(node.kind(), SyntaxKind::Emphasis | SyntaxKind::Strong) {
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
        if let Some(found) = emphasis_covering(child, at, start, end) {
            return Some(found);
        }
        at += child.width();
    }
    None
}

fn reparse_all(tree: &Green, bytes: &str, splices: &[Splice]) -> (String, Green, bool) {
    if splices.len() == 1 {
        let splice = &splices[0];
        let next = apply_text(bytes, splices);
        let delta = next.len() as isize - bytes.len() as isize;
        let re = reparse_span(tree, bytes, &next, splice.start, splice.end, delta);
        return (next, re.tree, re.widened_to_root);
    }
    let mut tree = tree.clone();
    let mut text = bytes.to_string();
    let mut widened = false;
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
        let re = reparse_span(&tree, &text, &next, start, end, delta);
        widened |= re.widened_to_root;
        applied_delta += delta;
        text = next;
        tree = re.tree;
    }
    (text, tree, widened)
}

struct Reparsed {
    tree: Green,
    widened_to_root: bool,
}

fn reparse_span(
    old: &Green,
    _old_bytes: &str,
    new_bytes: &str,
    edit_start: usize,
    edit_end: usize,
    delta: isize,
) -> Reparsed {
    let ranges = child_ranges(old);
    if ranges.is_empty() {
        return Reparsed {
            tree: parse(new_bytes),
            widened_to_root: true,
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
                };
            }
        } else {
            let defs_before = defs_before(old, win_lo);
            let defs_after = defs_after(old, win_hi);
            let parsed =
                parse::parse_with(&new_bytes[new_lo..follow_end], &defs_before, &defs_after);
            if let Some(window) = resynced(
                parsed.children(),
                new_hi - new_lo,
                follow_old.map(|i| &ranges[i].2),
            ) {
                let mut children = Vec::new();
                children.extend(old.children()[..lo].iter().cloned());
                children.extend(window);
                children.extend(old.children()[hi..].iter().cloned());
                let mut tree = Green::branch(SyntaxKind::Document, children);
                let old_defs = parse::definitions_in(old);
                let new_defs = parse::definitions_in(&tree);
                if !same_defs(&old_defs, &new_defs) {
                    tree = parse::reresolve(&tree, &new_defs);
                }
                if tree.concat() == new_bytes && tree == parse(new_bytes) {
                    return Reparsed {
                        tree,
                        widened_to_root: lo == 0 && hi == ranges.len(),
                    };
                }
            }
        }
        if lo == 0 && hi == ranges.len() {
            return Reparsed {
                tree: parse(new_bytes),
                widened_to_root: true,
            };
        }
        lo = lo.saturating_sub(1);
        if hi < ranges.len() {
            hi += 1;
        }
    }
}

fn child_ranges(doc: &Green) -> Vec<(usize, usize, Green)> {
    let mut at = 0;
    let mut out = Vec::new();
    for child in doc.children() {
        let start = at;
        at += child.width();
        out.push((start, at, child.clone()));
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

fn resynced(nodes: &[Green], window: usize, follow: Option<&Green>) -> Option<Vec<Green>> {
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
        Some(old) => {
            if suffix.len() == 1
                && suffix[0].kind() == old.kind()
                && suffix[0].concat() == old.concat()
            {
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
