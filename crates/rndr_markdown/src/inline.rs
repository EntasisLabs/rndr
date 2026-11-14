//! Inline grammar. The concatenation of the result equals the input.
//!
//! Code spans, autolinks, HTML, entities, escapes, and icon shortcodes are
//! recognized before emphasis. Emphasis uses the CommonMark flanking and
//! multiple-of-three rules. A link is a link only when its destination is
//! written inline or a reference definition resolves it; the bytes either way
//! are the bytes in the file.

use unicode_general_category::{get_general_category, GeneralCategory};

use crate::green::Green;
use crate::kind::SyntaxKind;

/// First-wins reference destinations. The key is a normalized label.
#[derive(Clone, Debug, Default)]
pub struct Defs {
    order: Vec<(String, String)>,
}

impl Defs {
    /// Insert `label` if it has not been defined yet.
    pub fn insert_first(&mut self, label: &str, destination: &str) {
        let key = normalize_label(label);
        if key.is_empty() || self.get(&key).is_some() {
            return;
        }
        self.order.push((key, destination.to_string()));
    }

    fn get(&self, normalized: &str) -> Option<&str> {
        self.order
            .iter()
            .find(|(k, _)| k == normalized)
            .map(|(_, v)| v.as_str())
    }

    /// Whether the normalized label resolves.
    pub fn contains(&self, label: &str) -> bool {
        self.get(&normalize_label(label)).is_some()
    }

    /// Definitions in source order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.order.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }
}

/// Trim, case-fold, and collapse whitespace the way CommonMark labels do.
pub fn normalize_label(label: &str) -> String {
    let mut out = String::new();
    let mut space = false;
    for c in label.trim().chars() {
        if c.is_whitespace() {
            if !out.is_empty() {
                space = true;
            }
            continue;
        }
        if space {
            out.push(' ');
            space = false;
        }
        for lower in c.to_lowercase() {
            out.push(lower);
        }
    }
    out
}

/// Parse `src` into inline nodes that concatenate back to `src`.
pub fn parse_inlines(src: &str, defs: &Defs) -> Vec<Green> {
    let pieces = scan(src);
    let pieces = resolve_links(pieces, src, defs);
    let pieces = resolve_emphasis(pieces);
    let nodes = pieces_to_nodes(pieces);
    debug_assert_eq!(concat_nodes(&nodes), src);
    nodes
}

fn concat_nodes(nodes: &[Green]) -> String {
    let mut out = String::new();
    for node in nodes {
        out.push_str(&node.concat());
    }
    out
}

#[derive(Clone, Debug)]
enum Piece {
    Node(Green),
    Delim {
        start: usize,
        end: usize,
        ch: char,
        len: usize,
        can_open: bool,
        can_close: bool,
    },
    Bracket {
        start: usize,
        end: usize,
        image: bool,
        open: bool,
    },
}

impl Piece {
    fn range(&self) -> Option<(usize, usize)> {
        match self {
            Self::Node(_) => None,
            Self::Delim { start, end, .. } | Self::Bracket { start, end, .. } => {
                Some((*start, *end))
            }
        }
    }
}

fn scan(src: &str) -> Vec<Piece> {
    let bytes = src.as_bytes();
    let mut i = 0;
    let mut out = Vec::new();
    let mut text_start = 0;
    let flush = |out: &mut Vec<Piece>, src: &str, text_start: &mut usize, end: usize| {
        if *text_start < end {
            out.push(Piece::Node(Green::leaf(
                SyntaxKind::Text,
                &src[*text_start..end],
            )));
        }
        *text_start = end;
    };
    while i < bytes.len() {
        if bytes[i] == b'`' {
            if let Some(end) = code_span_end(src, i) {
                flush(&mut out, src, &mut text_start, i);
                out.push(Piece::Node(code_span(src, i, end)));
                i = end;
                text_start = i;
                continue;
            }
        }
        if bytes[i] == b'\\' {
            let rest = &src[i..];
            let mut chars = rest.chars();
            let _slash = chars.next();
            if let Some(next) = chars.next() {
                let next_len = next.len_utf8();
                if next == '\n' || next == '\r' {
                    flush(&mut out, src, &mut text_start, i);
                    let nl = newline_len(bytes, i + 1);
                    out.push(Piece::Node(Green::branch(
                        SyntaxKind::HardBreak,
                        vec![
                            Green::leaf(SyntaxKind::Delimiter, "\\"),
                            Green::leaf(SyntaxKind::LineEnding, &src[i + 1..i + 1 + nl]),
                        ],
                    )));
                    i = i + 1 + nl;
                    text_start = i;
                    continue;
                }
                if is_ascii_punct(next) {
                    flush(&mut out, src, &mut text_start, i);
                    let end = i + 1 + next_len;
                    out.push(Piece::Node(Green::leaf(SyntaxKind::Escape, &src[i..end])));
                    i = end;
                    text_start = i;
                    continue;
                }
            }
        }
        if bytes[i] == b'&' {
            if let Some(end) = entity_end(src, i) {
                flush(&mut out, src, &mut text_start, i);
                out.push(Piece::Node(Green::leaf(SyntaxKind::Entity, &src[i..end])));
                i = end;
                text_start = i;
                continue;
            }
        }
        if bytes[i] == b'<' {
            if let Some(end) = autolink_or_html(src, i) {
                flush(&mut out, src, &mut text_start, i);
                let kind = if looks_like_autolink(&src[i..end]) {
                    SyntaxKind::Autolink
                } else {
                    SyntaxKind::HtmlInline
                };
                out.push(Piece::Node(Green::leaf(kind, &src[i..end])));
                i = end;
                text_start = i;
                continue;
            }
        }
        if src[i..].starts_with("{{icon:") {
            if let Some(end) = icon_end(src, i) {
                flush(&mut out, src, &mut text_start, i);
                out.push(Piece::Node(Green::leaf(
                    SyntaxKind::IconShortcode,
                    &src[i..end],
                )));
                i = end;
                text_start = i;
                continue;
            }
        }
        if bytes[i] == b'\n' || bytes[i] == b'\r' {
            flush(&mut out, src, &mut text_start, i);
            let nl = newline_len(bytes, i);
            classify_break(&mut out, src, i, nl);
            i += nl;
            text_start = i;
            continue;
        }
        if bytes[i] == b'*' || bytes[i] == b'_' || bytes[i] == b'~' {
            let ch = bytes[i] as char;
            let mut end = i + 1;
            while end < bytes.len() && bytes[end] == bytes[i] {
                end += 1;
            }
            let len = end - i;
            if ch == '~' && len != 2 {
                i += 1;
                continue;
            }
            if ch == '~' {
                // only an exact run of two
            }
            flush(&mut out, src, &mut text_start, i);
            let (can_open, can_close) = flanking(src, i, end, ch);
            if ch == '~' {
                out.push(Piece::Delim {
                    start: i,
                    end,
                    ch,
                    len: 2,
                    can_open: can_open || !can_close,
                    can_close: can_close || !can_open,
                });
            } else {
                out.push(Piece::Delim {
                    start: i,
                    end,
                    ch,
                    len,
                    can_open,
                    can_close,
                });
            }
            i = end;
            text_start = i;
            continue;
        }
        if bytes[i] == b'!' && i + 1 < bytes.len() && bytes[i + 1] == b'[' {
            flush(&mut out, src, &mut text_start, i);
            out.push(Piece::Bracket {
                start: i,
                end: i + 2,
                image: true,
                open: true,
            });
            i += 2;
            text_start = i;
            continue;
        }
        if bytes[i] == b'[' || bytes[i] == b']' {
            flush(&mut out, src, &mut text_start, i);
            out.push(Piece::Bracket {
                start: i,
                end: i + 1,
                image: false,
                open: bytes[i] == b'[',
            });
            i += 1;
            text_start = i;
            continue;
        }
        i += utf8_width(bytes[i]);
    }
    flush(&mut out, src, &mut text_start, src.len());
    out
}

fn utf8_width(lead: u8) -> usize {
    if lead < 0x80 {
        1
    } else if lead < 0xE0 {
        2
    } else if lead < 0xF0 {
        3
    } else {
        4
    }
}

fn looks_like_autolink(span: &str) -> bool {
    let inner = span.trim_start_matches('<').trim_end_matches('>');
    inner.contains(':') || inner.contains('@')
}

fn classify_break(out: &mut Vec<Piece>, src: &str, nl_at: usize, nl_len: usize) {
    let nl = &src[nl_at..nl_at + nl_len];
    if let Some(Piece::Node(node)) = out.last_mut() {
        if node.kind() == SyntaxKind::Text {
            let text = node.text();
            if text.ends_with("  ") {
                let keep = text.len() - 2;
                let spaces = "  ".to_string();
                if keep == 0 {
                    *node = Green::branch(
                        SyntaxKind::HardBreak,
                        vec![
                            Green::leaf(SyntaxKind::Delimiter, spaces),
                            Green::leaf(SyntaxKind::LineEnding, nl),
                        ],
                    );
                } else {
                    let kept = text[..keep].to_string();
                    *node = Green::leaf(SyntaxKind::Text, kept);
                    out.push(Piece::Node(Green::branch(
                        SyntaxKind::HardBreak,
                        vec![
                            Green::leaf(SyntaxKind::Delimiter, spaces),
                            Green::leaf(SyntaxKind::LineEnding, nl),
                        ],
                    )));
                }
                return;
            }
        }
    }
    out.push(Piece::Node(Green::leaf(SyntaxKind::SoftBreak, nl)));
}

fn newline_len(bytes: &[u8], i: usize) -> usize {
    if bytes[i] == b'\r' && i + 1 < bytes.len() && bytes[i + 1] == b'\n' {
        2
    } else {
        1
    }
}

fn code_span_end(src: &str, start: usize) -> Option<usize> {
    let bytes = src.as_bytes();
    let mut len = 0;
    while start + len < bytes.len() && bytes[start + len] == b'`' {
        len += 1;
    }
    if len == 0 {
        return None;
    }
    let mut i = start + len;
    while i < bytes.len() {
        if bytes[i] == b'`' {
            let mut run = 0;
            while i + run < bytes.len() && bytes[i + run] == b'`' {
                run += 1;
            }
            if run == len {
                return Some(i + run);
            }
            i += run;
            continue;
        }
        i += 1;
    }
    None
}

fn code_span(src: &str, start: usize, end: usize) -> Green {
    let bytes = src.as_bytes();
    let mut open = 0;
    while start + open < end && bytes[start + open] == b'`' {
        open += 1;
    }
    let close = open;
    let content_end = end - close;
    Green::branch(
        SyntaxKind::InlineCode,
        vec![
            Green::leaf(SyntaxKind::Delimiter, &src[start..start + open]),
            Green::leaf(SyntaxKind::Text, &src[start + open..content_end]),
            Green::leaf(SyntaxKind::Delimiter, &src[content_end..end]),
        ],
    )
}

fn entity_end(src: &str, start: usize) -> Option<usize> {
    let rest = &src[start + 1..];
    if let Some(stripped) = rest.strip_prefix('#') {
        let hex = stripped.starts_with(['x', 'X']);
        let digits = if hex { &stripped[1..] } else { stripped };
        let mut n = 0;
        for (i, c) in digits.char_indices() {
            let ok = if hex {
                c.is_ascii_hexdigit()
            } else {
                c.is_ascii_digit()
            };
            if !ok {
                if i > 0 && c == ';' {
                    return Some(start + 1 + 1 + usize::from(hex) + i + 1);
                }
                return None;
            }
            n += 1;
            if n > 8 {
                return None;
            }
        }
        return None;
    }
    let mut name_len = 0;
    for c in rest.chars() {
        if name_len == 0 && c.is_ascii_alphabetic() || name_len > 0 && c.is_ascii_alphanumeric() {
            name_len += 1;
            continue;
        }
        if c == ';' && is_known_entity(&rest[..name_len]) {
            return Some(start + 1 + name_len + 1);
        }
        return None;
    }
    None
}

fn is_known_entity(name: &str) -> bool {
    matches!(
        name,
        "amp" | "lt" | "gt" | "quot" | "apos" | "nbsp" | "AMP" | "LT" | "GT" | "QUOT" | "Apos"
    )
}

fn autolink_or_html(src: &str, start: usize) -> Option<usize> {
    let rest = &src[start..];
    if let Some(end) = rest.find('>') {
        let span = &rest[..=end];
        if span.contains('\n') || span.contains('\r') {
            return None;
        }
        if looks_like_autolink(span) || looks_like_html_tag(span) {
            return Some(start + end + 1);
        }
    }
    if rest.starts_with("<!--") {
        if let Some(rel) = rest.find("-->") {
            let span = &rest[..=rel + 2];
            if !span.contains('\n') {
                return Some(start + rel + 3);
            }
        }
    }
    None
}

fn looks_like_html_tag(span: &str) -> bool {
    let inner = span.trim_start_matches('<').trim_end_matches('>').trim();
    let inner = inner.strip_prefix('/').unwrap_or(inner);
    inner.starts_with(|c: char| c.is_ascii_alphabetic())
}

fn icon_end(src: &str, start: usize) -> Option<usize> {
    let rest = &src[start + "{{icon:".len()..];
    let mut len = 0;
    for c in rest.chars() {
        if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
            len += c.len_utf8();
            continue;
        }
        break;
    }
    if len == 0 {
        return None;
    }
    let after = &rest[len..];
    after
        .starts_with("}}")
        .then_some(start + "{{icon:".len() + len + 2)
}

fn flanking(src: &str, start: usize, end: usize, ch: char) -> (bool, bool) {
    let before = prev_char(src, start);
    let after = src[end..].chars().next();
    let left = is_left_flanking(before, after);
    let right = is_right_flanking(before, after);
    if ch == '_' {
        let open = left && (!right || before.is_some_and(is_punct));
        let close = right && (!left || after.is_some_and(is_punct));
        (open, close)
    } else {
        (left, right)
    }
}

fn is_left_flanking(before: Option<char>, after: Option<char>) -> bool {
    let Some(after) = after else {
        return false;
    };
    if is_unicode_ws(after) {
        return false;
    }
    !is_punct(after) || before.is_none_or(|c| is_unicode_ws(c) || is_punct(c))
}

fn is_right_flanking(before: Option<char>, after: Option<char>) -> bool {
    let Some(before) = before else {
        return false;
    };
    if is_unicode_ws(before) {
        return false;
    }
    !is_punct(before) || after.is_none_or(|c| is_unicode_ws(c) || is_punct(c))
}

fn prev_char(src: &str, index: usize) -> Option<char> {
    src[..index].chars().next_back()
}

fn is_unicode_ws(c: char) -> bool {
    c.is_whitespace()
}

fn is_punct(c: char) -> bool {
    matches!(
        get_general_category(c),
        GeneralCategory::ConnectorPunctuation
            | GeneralCategory::DashPunctuation
            | GeneralCategory::OpenPunctuation
            | GeneralCategory::ClosePunctuation
            | GeneralCategory::InitialPunctuation
            | GeneralCategory::FinalPunctuation
            | GeneralCategory::OtherPunctuation
            | GeneralCategory::MathSymbol
            | GeneralCategory::CurrencySymbol
            | GeneralCategory::ModifierSymbol
            | GeneralCategory::OtherSymbol
    )
}

fn is_ascii_punct(c: char) -> bool {
    c.is_ascii_punctuation()
}

fn resolve_links(mut pieces: Vec<Piece>, src: &str, defs: &Defs) -> Vec<Piece> {
    let mut i = 0;
    while i < pieces.len() {
        let is_close = matches!(pieces[i], Piece::Bracket { open: false, .. });
        if !is_close {
            i += 1;
            continue;
        }
        let close_end = match pieces[i] {
            Piece::Bracket { end, .. } => end,
            _ => unreachable!(),
        };
        let Some(open_at) = find_open_bracket(&pieces, i) else {
            i += 1;
            continue;
        };
        if let Some(dest_end) = inline_destination(src, close_end) {
            let interior = pieces[open_at + 1..i].to_vec();
            let image = matches!(pieces[open_at], Piece::Bracket { image: true, .. });
            let open = piece_text(&pieces[open_at], src);
            let mut children = vec![Green::leaf(SyntaxKind::Delimiter, open)];
            children.extend(pieces_to_nodes(interior));
            children.push(Green::leaf(SyntaxKind::Delimiter, "]"));
            children.push(Green::leaf(SyntaxKind::Text, &src[close_end..dest_end]));
            let node = Green::branch(
                if image {
                    SyntaxKind::Image
                } else {
                    SyntaxKind::Link
                },
                children,
            );
            // Drop pieces covered by the destination. They were scanned as text
            // after the bracket and must not be emitted twice.
            let mut after = i + 1;
            while after < pieces.len() {
                if let Some((s, _)) = pieces[after].range() {
                    if s >= dest_end {
                        break;
                    }
                } else if let Piece::Node(node) = &pieces[after] {
                    // Node pieces have no stored range. They are in order, so
                    // count their width against the source cursor.
                    let _ = node;
                }
                after += 1;
            }
            // Re-scan is safer: rebuild the tail from dest_end.
            let tail = scan(&src[dest_end..]);
            let tail = shift_pieces(tail, dest_end);
            let mut next = pieces[..open_at].to_vec();
            next.push(Piece::Node(node));
            next.extend(tail);
            pieces = resolve_links(next, src, defs);
            return pieces;
        }
        if let Some(label_span) = reference_label(src, close_end) {
            let label = &src[label_span.0..label_span.1];
            let inner_label = label.trim_start_matches('[').trim_end_matches(']').trim();
            let use_label = if inner_label.is_empty() {
                piece_interior_text(&pieces[open_at + 1..i], src)
            } else {
                inner_label.to_string()
            };
            if defs.contains(&use_label) {
                let image = matches!(pieces[open_at], Piece::Bracket { image: true, .. });
                let interior = pieces[open_at + 1..i].to_vec();
                let mut children = vec![Green::leaf(
                    SyntaxKind::Delimiter,
                    piece_text(&pieces[open_at], src),
                )];
                children.extend(pieces_to_nodes(interior));
                children.push(Green::leaf(SyntaxKind::Delimiter, "]"));
                children.push(Green::leaf(SyntaxKind::Text, label));
                let node = Green::branch(
                    if image {
                        SyntaxKind::Image
                    } else {
                        SyntaxKind::Link
                    },
                    children,
                );
                let tail = shift_pieces(scan(&src[label_span.1..]), label_span.1);
                let mut next = pieces[..open_at].to_vec();
                next.push(Piece::Node(node));
                next.extend(tail);
                return resolve_links(next, src, defs);
            }
        }
        // Shortcut reference: the bracket text itself is the label.
        let shortcut = piece_interior_text(&pieces[open_at + 1..i], src);
        if defs.contains(&shortcut)
            && !shortcut.is_empty()
            && !matches!(pieces.get(i + 1), Some(Piece::Bracket { open: true, .. }))
        {
            let image = matches!(pieces[open_at], Piece::Bracket { image: true, .. });
            let interior = pieces[open_at + 1..i].to_vec();
            let mut children = vec![Green::leaf(
                SyntaxKind::Delimiter,
                piece_text(&pieces[open_at], src),
            )];
            children.extend(pieces_to_nodes(interior));
            children.push(Green::leaf(SyntaxKind::Delimiter, "]"));
            let node = Green::branch(
                if image {
                    SyntaxKind::Image
                } else {
                    SyntaxKind::Link
                },
                children,
            );
            let mut next = pieces[..open_at].to_vec();
            next.push(Piece::Node(node));
            next.extend(pieces[i + 1..].iter().cloned());
            return resolve_links(next, src, defs);
        }
        i += 1;
    }
    pieces
}

fn shift_pieces(pieces: Vec<Piece>, by: usize) -> Vec<Piece> {
    pieces
        .into_iter()
        .map(|piece| match piece {
            Piece::Node(node) => Piece::Node(node),
            Piece::Delim {
                start,
                end,
                ch,
                len,
                can_open,
                can_close,
            } => Piece::Delim {
                start: start + by,
                end: end + by,
                ch,
                len,
                can_open,
                can_close,
            },
            Piece::Bracket {
                start,
                end,
                image,
                open,
            } => Piece::Bracket {
                start: start + by,
                end: end + by,
                image,
                open,
            },
        })
        .collect()
}

fn find_open_bracket(pieces: &[Piece], close: usize) -> Option<usize> {
    let mut i = close;
    while i > 0 {
        i -= 1;
        if matches!(pieces[i], Piece::Bracket { open: true, .. }) {
            return Some(i);
        }
        if matches!(pieces[i], Piece::Node(ref n) if n.kind() == SyntaxKind::Link || n.kind() == SyntaxKind::Image)
        {
            return None;
        }
    }
    None
}

fn inline_destination(src: &str, at: usize) -> Option<usize> {
    let rest = &src[at..];
    if !rest.starts_with('(') {
        return None;
    }
    let bytes = rest.as_bytes();
    let mut i = 1;
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    if i < bytes.len() && bytes[i] == b'<' {
        let close = rest[i + 1..].find('>')?;
        i = i + 1 + close + 1;
    } else {
        let mut depth = 0;
        let start = i;
        while i < bytes.len() {
            let c = bytes[i];
            if c == b'\\' && i + 1 < bytes.len() {
                i += 2;
                continue;
            }
            if c == b'(' {
                depth += 1;
            } else if c == b')' && depth == 0 {
                break;
            } else if c == b')' {
                depth -= 1;
            } else if c.is_ascii_whitespace() && depth == 0 {
                break;
            }
            i += 1;
        }
        if i == start && i < bytes.len() && bytes[i] == b')' {
            // empty destination
        }
    }
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        if bytes[i] == b'\n' || bytes[i] == b'\r' {
            break;
        }
        // title
        if bytes[i] == b'"' || bytes[i] == b'\'' || bytes[i] == b'(' {
            let quote = bytes[i];
            let end_quote = if quote == b'(' { b')' } else { quote };
            i += 1;
            while i < bytes.len() && bytes[i] != end_quote {
                if bytes[i] == b'\\' && i + 1 < bytes.len() {
                    i += 2;
                    continue;
                }
                i += 1;
            }
            if i >= bytes.len() {
                return None;
            }
            i += 1;
            while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
                i += 1;
            }
            break;
        }
        i += 1;
    }
    if i < bytes.len() && bytes[i] == b')' {
        Some(at + i + 1)
    } else {
        None
    }
}

fn reference_label(src: &str, at: usize) -> Option<(usize, usize)> {
    let rest = &src[at..];
    if !rest.starts_with('[') {
        return None;
    }
    let bytes = rest.as_bytes();
    let mut i = 1;
    while i < bytes.len() && bytes[i] != b']' && bytes[i] != b'[' && bytes[i] != b'\n' {
        i += 1;
    }
    if i < bytes.len() && bytes[i] == b']' {
        Some((at, at + i + 1))
    } else {
        None
    }
}

fn piece_text(piece: &Piece, src: &str) -> String {
    match piece {
        Piece::Bracket { start, end, .. } | Piece::Delim { start, end, .. } => {
            src[*start..*end].to_string()
        }
        Piece::Node(node) => node.concat(),
    }
}

fn piece_interior_text(pieces: &[Piece], src: &str) -> String {
    let mut out = String::new();
    for piece in pieces {
        out.push_str(&piece_text(piece, src));
    }
    out
}

fn resolve_emphasis(mut pieces: Vec<Piece>) -> Vec<Piece> {
    let mut closer = 0;
    while closer < pieces.len() {
        let Some((ch, close_len, can_close)) = delim_info(&pieces[closer]) else {
            closer += 1;
            continue;
        };
        if !can_close {
            closer += 1;
            continue;
        }
        let mut opener = None;
        let mut look = closer;
        while look > 0 {
            look -= 1;
            let Some((och, open_len, _)) = delim_info(&pieces[look]) else {
                continue;
            };
            let can_open = matches!(pieces[look], Piece::Delim { can_open: true, .. });
            if !can_open || och != ch {
                continue;
            }
            if !mod3_allows(open_len, close_len) {
                continue;
            }
            opener = Some((look, open_len));
            break;
        }
        let Some((open_at, open_len)) = opener else {
            closer += 1;
            continue;
        };
        let use_len = if ch == '~' || (open_len >= 2 && close_len >= 2) {
            2
        } else {
            1
        };
        if ch == '~' && (open_len != 2 || close_len != 2) {
            closer += 1;
            continue;
        }
        let kind = if ch == '~' {
            SyntaxKind::Strikethrough
        } else if use_len == 2 {
            SyntaxKind::Strong
        } else {
            SyntaxKind::Emphasis
        };
        let (open_start, open_end, open_can_open, open_can_close) = match pieces[open_at] {
            Piece::Delim {
                start,
                end,
                can_open,
                can_close,
                ..
            } => (start, end, can_open, can_close),
            _ => unreachable!(),
        };
        let (close_start, close_end, close_can_open, close_can_close) = match pieces[closer] {
            Piece::Delim {
                start,
                end,
                can_open,
                can_close,
                ..
            } => (start, end, can_open, can_close),
            _ => unreachable!(),
        };
        let _ = (open_end, close_end);
        let used_open: String = std::iter::repeat_n(ch, use_len).collect();
        let used_close = used_open.clone();
        let interior = pieces[open_at + 1..closer].to_vec();
        let mut children = vec![Green::leaf(SyntaxKind::Delimiter, used_open.clone())];
        children.extend(pieces_to_nodes(resolve_emphasis(interior)));
        children.push(Green::leaf(SyntaxKind::Delimiter, used_close));
        let wrapped = Piece::Node(Green::branch(kind, children));
        let mut next = Vec::new();
        next.extend(pieces[..open_at].iter().cloned());
        if open_len > use_len {
            let remain = open_len - use_len;
            let remain_end = open_start + remain * ch.len_utf8();
            next.push(Piece::Delim {
                start: open_start,
                end: remain_end,
                ch,
                len: remain,
                can_open: open_can_open,
                can_close: open_can_close,
            });
        }
        next.push(wrapped);
        if close_len > use_len {
            let remain = close_len - use_len;
            let consumed = use_len * ch.len_utf8();
            next.push(Piece::Delim {
                start: close_start + consumed,
                end: close_end,
                ch,
                len: remain,
                can_open: close_can_open,
                can_close: close_can_close,
            });
        }
        next.extend(pieces[closer + 1..].iter().cloned());
        pieces = next;
        closer = open_at;
    }
    pieces
}

fn delim_info(piece: &Piece) -> Option<(char, usize, bool)> {
    match piece {
        Piece::Delim {
            ch, len, can_close, ..
        } => Some((*ch, *len, *can_close)),
        _ => None,
    }
}

fn mod3_allows(open_len: usize, close_len: usize) -> bool {
    !((open_len + close_len).is_multiple_of(3)
        && (!open_len.is_multiple_of(3) || !close_len.is_multiple_of(3)))
}

fn pieces_to_nodes(pieces: Vec<Piece>) -> Vec<Green> {
    let mut out = Vec::new();
    for piece in pieces {
        match piece {
            Piece::Node(node) => out.push(node),
            Piece::Delim { ch, len, .. } => {
                let text: String = std::iter::repeat_n(ch, len).collect();
                out.push(Green::leaf(SyntaxKind::Text, text));
            }
            Piece::Bracket { image, open, .. } => {
                let text = if image && open {
                    "!["
                } else if open {
                    "["
                } else {
                    "]"
                };
                out.push(Green::leaf(SyntaxKind::Text, text));
            }
        }
    }
    out
}

/// Read a reference definition starting at `src[start..end]`.
/// Returns the normalized label, the exact destination bytes, and the end
/// offset of the definition inside `src` (not past `end`).
pub fn parse_reference_def(src: &str, start: usize, end: usize) -> Option<(String, String, usize)> {
    let region = &src[start..end];
    let bytes = region.as_bytes();
    if bytes.first() != Some(&b'[') {
        return None;
    }
    let mut i = 1;
    while i < bytes.len() && bytes[i] != b']' {
        if bytes[i] == b'[' || bytes[i] == b'\n' {
            return None;
        }
        i += 1;
    }
    if i >= bytes.len() || i == 1 {
        return None;
    }
    let label = region[1..i].to_string();
    i += 1;
    if i >= bytes.len() || bytes[i] != b':' {
        return None;
    }
    i += 1;
    while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
        i += 1;
    }
    if i < bytes.len() && (bytes[i] == b'\n' || bytes[i] == b'\r') {
        let nl = newline_len(bytes, i);
        i += nl;
        while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
            i += 1;
        }
    }
    if i >= bytes.len() {
        return None;
    }
    let dest_start = i;
    if bytes[i] == b'<' {
        let close = region[i + 1..].find('>')?;
        i = i + 1 + close + 1;
    } else {
        while i < bytes.len()
            && !bytes[i].is_ascii_whitespace()
            && bytes[i] != b'"'
            && bytes[i] != b'\''
        {
            if bytes[i] == b'\\' && i + 1 < bytes.len() {
                i += 2;
                continue;
            }
            i += 1;
        }
    }
    if i == dest_start {
        return None;
    }
    let destination = region[dest_start..i].to_string();
    while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
        i += 1;
    }
    if i < bytes.len() && (bytes[i] == b'"' || bytes[i] == b'\'' || bytes[i] == b'(') {
        let quote = bytes[i];
        let end_quote = if quote == b'(' { b')' } else { quote };
        i += 1;
        while i < bytes.len() && bytes[i] != end_quote {
            i += 1;
        }
        if i < bytes.len() && bytes[i] == end_quote {
            i += 1;
        }
    }
    while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
        i += 1;
    }
    if i < bytes.len() && (bytes[i] == b'\n' || bytes[i] == b'\r') {
        i += newline_len(bytes, i);
    }
    Some((label, destination, start + i))
}
