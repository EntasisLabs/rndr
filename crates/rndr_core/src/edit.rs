use unicode_segmentation::UnicodeSegmentation;

use crate::input::Key;

/// Insert, delete, or move a caret in `text`. Indices are byte offsets.
/// Left, Right, and Backspace step by grapheme cluster, so a flag emoji or
/// `e` plus a combining mark is one caret stop. `newline` inserts a line break
/// on Enter.
pub fn edit_string(
    text: &mut String,
    index: usize,
    anchor: usize,
    key: Option<Key>,
    shift: bool,
    chars: &str,
    newline: bool,
) -> (usize, usize) {
    let len = text.len();
    let mut index = index.min(len);
    let mut anchor = anchor.min(len);
    if matches!(key, Some(Key::Left | Key::Right)) {
        return move_caret(text, index, anchor, key, shift);
    }
    let editing =
        !chars.is_empty() || key == Some(Key::Backspace) || (newline && key == Some(Key::Enter));
    let mut replaced = false;
    if editing && anchor != index {
        let (start, end) = grapheme_selection(text, anchor, index);
        if start < end {
            text.replace_range(start..end, "");
        }
        index = start;
        anchor = start;
        replaced = true;
    }
    if key == Some(Key::Backspace) && !replaced {
        if let Some((start, end)) = delete_prev_grapheme(text, index) {
            text.replace_range(start..end, "");
            index = start;
            anchor = start;
        }
    }
    index = floor_boundary(text.len(), index, &|i| text.as_bytes()[i]);
    for ch in chars.chars() {
        if is_insertable(ch) {
            text.insert(index, ch);
            index += ch.len_utf8();
            anchor = index;
        }
    }
    if newline && key == Some(Key::Enter) {
        text.insert(index, '\n');
        index += 1;
        anchor = index;
    }
    (index, anchor)
}

fn move_caret(
    text: &str,
    index: usize,
    anchor: usize,
    key: Option<Key>,
    shift: bool,
) -> (usize, usize) {
    let len = text.len();
    let index = index.min(len);
    let anchor = anchor.min(len);
    let next = match key {
        Some(Key::Left) => prev_grapheme(text, index),
        Some(Key::Right) => next_grapheme(text, index),
        _ => index,
    };
    if shift {
        (next, anchor)
    } else if anchor != index {
        let edge = match key {
            Some(Key::Left) => anchor.min(index),
            Some(Key::Right) => anchor.max(index),
            _ => next,
        };
        let edge = floor_grapheme(text, edge);
        (edge, edge)
    } else {
        (next, next)
    }
}

fn is_insertable(ch: char) -> bool {
    !ch.is_control()
}

fn is_cont(byte: u8) -> bool {
    byte & 0b1100_0000 == 0b1000_0000
}

pub(crate) fn floor_boundary(len: usize, index: usize, byte: &impl Fn(usize) -> u8) -> usize {
    let mut i = index.min(len);
    while i > 0 && i < len && is_cont(byte(i)) {
        i -= 1;
    }
    i
}

#[allow(dead_code)]
pub(crate) fn ceil_boundary(len: usize, index: usize, byte: &impl Fn(usize) -> u8) -> usize {
    let mut i = index.min(len);
    if i == len || !is_cont(byte(i)) {
        return i;
    }
    while i < len && is_cont(byte(i)) {
        i += 1;
    }
    i
}

pub(crate) fn is_boundary(len: usize, index: usize, byte: &impl Fn(usize) -> u8) -> bool {
    floor_boundary(len, index.min(len), byte) == index.min(len)
}

/// Previous caret stop. From the middle of a scalar this is that scalar's start.
pub(crate) fn prev_boundary(len: usize, index: usize, byte: &impl Fn(usize) -> u8) -> usize {
    let index = index.min(len);
    if !is_boundary(len, index, byte) {
        return floor_boundary(len, index, byte);
    }
    if index == 0 {
        return 0;
    }
    floor_boundary(len, index - 1, byte)
}

/// Next caret stop. From the middle of a scalar this is that scalar's end.
#[allow(dead_code)]
pub(crate) fn next_boundary(len: usize, index: usize, byte: &impl Fn(usize) -> u8) -> usize {
    let index = index.min(len);
    if !is_boundary(len, index, byte) {
        return ceil_boundary(len, index, byte);
    }
    if index >= len {
        return len;
    }
    let mut j = index + 1;
    while j < len && is_cont(byte(j)) {
        j += 1;
    }
    j
}

/// Bytes removed by Backspace. A caret in the middle of a scalar deletes that scalar.
#[allow(dead_code)]
pub(crate) fn delete_prev_span(
    len: usize,
    index: usize,
    byte: &impl Fn(usize) -> u8,
) -> Option<(usize, usize)> {
    let index = index.min(len);
    if index == 0 && is_boundary(len, index, byte) {
        return None;
    }
    let (start, end) = if is_boundary(len, index, byte) {
        (prev_boundary(len, index, byte), index)
    } else {
        (
            floor_boundary(len, index, byte),
            ceil_boundary(len, index, byte),
        )
    };
    (start < end).then_some((start, end))
}

fn grapheme_edges(text: &str) -> Vec<usize> {
    let mut edges: Vec<usize> = text
        .grapheme_indices(true)
        .map(|(index, _)| index)
        .collect();
    edges.push(text.len());
    edges
}

fn on_grapheme_boundary(text: &str, index: usize) -> bool {
    let len = text.len();
    let byte = |i: usize| text.as_bytes()[i];
    let index = index.min(len);
    if !is_boundary(len, index, &byte) {
        return false;
    }
    grapheme_edges(text).contains(&index)
}

/// Start of the grapheme that contains `index`, or `index` when it is already a boundary.
pub(crate) fn floor_grapheme(text: &str, index: usize) -> usize {
    let len = text.len();
    let index = floor_boundary(len, index.min(len), &|i| text.as_bytes()[i]);
    let mut start = 0;
    for edge in grapheme_edges(text) {
        if edge > index {
            return start;
        }
        start = edge;
        if edge == index {
            return edge;
        }
    }
    start
}

/// Previous grapheme boundary. A caret inside a cluster snaps to that cluster's start.
pub(crate) fn prev_grapheme(text: &str, index: usize) -> usize {
    let len = text.len();
    let index = floor_boundary(len, index.min(len), &|i| text.as_bytes()[i]);
    if !on_grapheme_boundary(text, index) {
        return floor_grapheme(text, index);
    }
    let mut prev = 0;
    for edge in grapheme_edges(text) {
        if edge >= index {
            return prev;
        }
        prev = edge;
    }
    0
}

/// Next grapheme boundary. A caret inside a cluster snaps to that cluster's end.
pub(crate) fn next_grapheme(text: &str, index: usize) -> usize {
    let len = text.len();
    let index = floor_boundary(len, index.min(len), &|i| text.as_bytes()[i]);
    for edge in grapheme_edges(text) {
        if edge > index {
            return edge;
        }
    }
    len
}

/// Bytes removed by Backspace. A caret inside a cluster deletes that whole cluster.
pub(crate) fn delete_prev_grapheme(text: &str, index: usize) -> Option<(usize, usize)> {
    let len = text.len();
    let index = index.min(len);
    if index == 0 && on_grapheme_boundary(text, index) {
        return None;
    }
    let (start, end) = if on_grapheme_boundary(text, index) {
        (prev_grapheme(text, index), index)
    } else {
        (floor_grapheme(text, index), next_grapheme(text, index))
    };
    (start < end).then_some((start, end))
}

/// Selection expanded so both ends sit on grapheme boundaries.
pub(crate) fn grapheme_selection(text: &str, anchor: usize, index: usize) -> (usize, usize) {
    let start = anchor.min(index).min(text.len());
    let end = anchor.max(index).min(text.len());
    (floor_grapheme(text, start), next_or_same(text, end))
}

fn next_or_same(text: &str, index: usize) -> usize {
    if on_grapheme_boundary(text, index) {
        index.min(text.len())
    } else {
        next_grapheme(text, index)
    }
}

/// Selection expanded so both ends sit on scalar boundaries.
#[allow(dead_code)]
pub(crate) fn selection_span(
    len: usize,
    anchor: usize,
    index: usize,
    byte: &impl Fn(usize) -> u8,
) -> (usize, usize) {
    let start = anchor.min(index).min(len);
    let end = anchor.max(index).min(len);
    (
        floor_boundary(len, start, byte),
        ceil_boundary(len, end, byte),
    )
}
