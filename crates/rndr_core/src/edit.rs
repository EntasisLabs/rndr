use crate::input::Key;

/// Insert, delete, or move a caret in `text`. Indices are byte offsets on
/// scalar boundaries. `newline` inserts a line break on Enter. Left and Right
/// move by one scalar.
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
    let byte = |i: usize| text.as_bytes()[i];
    if matches!(key, Some(Key::Left | Key::Right)) {
        return move_caret(text, index, anchor, key, shift);
    }
    let editing =
        !chars.is_empty() || key == Some(Key::Backspace) || (newline && key == Some(Key::Enter));
    let mut replaced = false;
    if editing && anchor != index {
        let (start, end) = selection_span(len, anchor, index, &byte);
        if start < end {
            text.replace_range(start..end, "");
        }
        index = start;
        anchor = start;
        replaced = true;
    }
    if key == Some(Key::Backspace) && !replaced {
        if let Some((start, end)) = delete_prev_span(text.len(), index, &|i| text.as_bytes()[i]) {
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
    let byte = |i: usize| text.as_bytes()[i];
    let next = match key {
        Some(Key::Left) => prev_boundary(len, index, &byte),
        Some(Key::Right) => next_boundary(len, index, &byte),
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
        (
            floor_boundary(len, edge, &byte),
            floor_boundary(len, edge, &byte),
        )
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

/// Selection expanded so both ends sit on scalar boundaries.
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
