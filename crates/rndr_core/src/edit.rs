use crate::input::Key;

/// Insert, delete, or move a caret in `text`. Indices are byte offsets.
/// `newline` inserts a line break on Enter. Left and Right move by one byte.
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
        let start = anchor.min(index);
        let end = anchor.max(index).min(text.len());
        text.replace_range(start..end, "");
        index = start;
        anchor = start;
        replaced = true;
    }
    if key == Some(Key::Backspace) && !replaced && index > 0 {
        index -= 1;
        text.remove(index);
        anchor = index;
    }
    for ch in chars.chars() {
        if is_printable_ascii(ch) {
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
        Some(Key::Left) => index.saturating_sub(1),
        Some(Key::Right) => (index + 1).min(len),
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
        (edge, edge)
    } else {
        (next, next)
    }
}

fn is_printable_ascii(ch: char) -> bool {
    matches!(ch, ' '..='~')
}
