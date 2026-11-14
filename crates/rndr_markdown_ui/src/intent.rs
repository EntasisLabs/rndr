//! Visual commands compiled to session splices.
//!
//! Typed characters go through [`Command::Insert`], which escapes a character
//! that would change the parse. Structure and paste go through
//! [`Command::Replace`] or [`Command::Split`], which leave the new bytes
//! unescaped so the reparse can see them.

use rndr_markdown::{print_liquid, Command};
use unicode_segmentation::UnicodeSegmentation;

use crate::project::{DisplayBlock, DisplayKind};

/// Delete the selection, or the previous visible character, or the marker at
/// the start of a block.
pub fn backspace(bytes: &str, block: &DisplayBlock, anchor: usize, head: usize) -> Command {
    if anchor != head {
        return delete_between(anchor, head);
    }
    if at_text_start(block, head) {
        if let Some(marker) = block.marker.clone() {
            return Command::Replace {
                start: marker.start,
                end: marker.end,
                text: String::new(),
            };
        }
        return delete_break_before(bytes, head);
    }
    let label = block.source_to_label(head);
    let previous = prev_boundary(&block.label_text(), label);
    let range = block.display_range(previous, label);
    Command::Delete {
        start: range.start,
        end: range.end,
    }
}

/// Delete the selection or the next visible character.
pub fn delete_forward(block: &DisplayBlock, anchor: usize, head: usize) -> Command {
    if anchor != head {
        return delete_between(anchor, head);
    }
    let label = block.source_to_label(head);
    let text = block.label_text();
    let next = next_boundary(&text, label);
    let range = block.display_range(label, next);
    if range.start == range.end {
        return Command::Delete {
            start: head,
            end: head,
        };
    }
    Command::Delete {
        start: range.start,
        end: range.end,
    }
}

/// Enter. A list item gains a sibling marker. An empty item leaves the list.
/// Any other block splits.
pub fn split(bytes: &str, block: &DisplayBlock, at: usize) -> Command {
    let empty_item = block.text.trim().is_empty()
        && at_text_start(block, at)
        && matches!(
            block.kind,
            DisplayKind::Bullet | DisplayKind::Ordered(_) | DisplayKind::Task(_)
        );
    if empty_item {
        return Command::Replace {
            start: block.span.start,
            end: block.span.end,
            text: "\n".into(),
        };
    }
    if let Some(marker) = block.marker.as_ref() {
        if matches!(
            block.kind,
            DisplayKind::Bullet | DisplayKind::Ordered(_) | DisplayKind::Task(_)
        ) {
            let copied = bytes.get(marker.clone()).unwrap_or("- ");
            return Command::Replace {
                start: at,
                end: at,
                text: format!("\n{copied}"),
            };
        }
    }
    Command::Split { at }
}

/// Turn the block into a heading of `level`, or back into a paragraph at level 0.
pub fn set_heading(block: &DisplayBlock, level: u8) -> Command {
    if level == 0 {
        if let Some(marker) = block.marker.clone() {
            return Command::Replace {
                start: marker.start,
                end: marker.end,
                text: String::new(),
            };
        }
        return Command::Replace {
            start: block.span.start,
            end: block.span.start,
            text: String::new(),
        };
    }
    let level = level.clamp(1, 6);
    let marks = format!("{} ", "#".repeat(level as usize));
    if let Some(marker) = block.marker.clone() {
        Command::Replace {
            start: marker.start,
            end: marker.end,
            text: marks,
        }
    } else {
        Command::Replace {
            start: block.span.start,
            end: block.span.start,
            text: marks,
        }
    }
}

/// Prefix the block with a bullet, or remove the marker when it is already a bullet.
pub fn toggle_bullet(block: &DisplayBlock) -> Command {
    if matches!(block.kind, DisplayKind::Bullet) {
        if let Some(marker) = block.marker.clone() {
            return Command::Replace {
                start: marker.start,
                end: marker.end,
                text: String::new(),
            };
        }
    }
    Command::Replace {
        start: block.span.start,
        end: block.span.start,
        text: "- ".into(),
    }
}

/// Prefix the block with an ordered marker.
pub fn toggle_ordered(block: &DisplayBlock) -> Command {
    if matches!(block.kind, DisplayKind::Ordered(_)) {
        if let Some(marker) = block.marker.clone() {
            return Command::Replace {
                start: marker.start,
                end: marker.end,
                text: String::new(),
            };
        }
    }
    Command::Replace {
        start: block.span.start,
        end: block.span.start,
        text: "1. ".into(),
    }
}

/// Prefix each line of the block with `> `.
pub fn toggle_quote(bytes: &str, block: &DisplayBlock) -> Command {
    let body = bytes.get(block.span.clone()).unwrap_or("");
    if block.quote > 0 {
        let mut out = String::new();
        for line in body.split_inclusive('\n') {
            out.push_str(strip_quote(line));
        }
        return Command::Replace {
            start: block.span.start,
            end: block.span.end,
            text: out,
        };
    }
    let mut out = String::new();
    for line in body.split_inclusive('\n') {
        if line.trim().is_empty() {
            out.push_str(line);
        } else {
            out.push_str("> ");
            out.push_str(line);
        }
    }
    Command::Replace {
        start: block.span.start,
        end: block.span.end,
        text: out,
    }
}

/// Wrap the block in a fenced code block.
pub fn set_code_fence(bytes: &str, block: &DisplayBlock) -> Command {
    let body = bytes
        .get(block.span.clone())
        .unwrap_or("")
        .trim_end_matches(['\n', '\r']);
    Command::Replace {
        start: block.span.start,
        end: block.span.end,
        text: format!("```\n{body}\n```\n"),
    }
}

/// Replace the block with a new Liquid fence. `fields` are `(key, value)` in order.
pub fn set_liquid(block: &DisplayBlock, kind: &str, fields: &[(&str, &str)]) -> Command {
    Command::Replace {
        start: block.span.start,
        end: block.span.end,
        text: print_liquid(kind, fields),
    }
}

fn delete_between(anchor: usize, head: usize) -> Command {
    Command::Delete {
        start: anchor.min(head),
        end: anchor.max(head),
    }
}

fn at_text_start(block: &DisplayBlock, source: usize) -> bool {
    block.source_to_label(source) <= block.chrome.len()
}

fn delete_break_before(bytes: &str, at: usize) -> Command {
    if at == 0 {
        return Command::Delete { start: 0, end: 0 };
    }
    let mut start = at;
    let b = bytes.as_bytes();
    if start > 0 && b[start - 1] == b'\n' {
        start -= 1;
        if start > 0 && b[start - 1] == b'\r' {
            start -= 1;
        }
    } else if start > 0 && b[start - 1] == b'\r' {
        start -= 1;
    }
    // A blank line between blocks is a second break. Take one extra so the
    // blocks join instead of leaving an empty paragraph.
    if start > 0 && (b[start - 1] == b'\n' || b[start - 1] == b'\r') {
        if b[start - 1] == b'\n' {
            start -= 1;
            if start > 0 && b[start - 1] == b'\r' {
                start -= 1;
            }
        } else {
            start -= 1;
        }
    }
    Command::Delete { start, end: at }
}

fn strip_quote(line: &str) -> &str {
    let trimmed = line.trim_start();
    if let Some(rest) = trimmed.strip_prefix('>') {
        let rest = rest.strip_prefix(' ').unwrap_or(rest);
        let offset = line.len() - trimmed.len();
        // The returned slice must be the tail of `line`.
        let start = offset + (trimmed.len() - rest.len());
        &line[start..]
    } else {
        line
    }
}

fn prev_boundary(text: &str, index: usize) -> usize {
    let index = index.min(text.len());
    text.grapheme_indices(true)
        .map(|(offset, _)| offset)
        .take_while(|offset| *offset < index)
        .last()
        .unwrap_or(0)
}

fn next_boundary(text: &str, index: usize) -> usize {
    let index = index.min(text.len());
    text.grapheme_indices(true)
        .map(|(offset, grapheme)| offset + grapheme.len())
        .find(|offset| *offset > index)
        .unwrap_or(text.len())
}

/// Move `source` by one visible character inside `block`.
pub fn move_by(block: &DisplayBlock, source: usize, forward: bool) -> usize {
    let label = block.source_to_label(source);
    let text = block.label_text();
    let next = if forward {
        next_boundary(&text, label)
    } else {
        prev_boundary(&text, label)
    };
    block.display_to_source(next)
}
