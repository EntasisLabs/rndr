//! Block grammar. Every byte of the source is owned by exactly one leaf.
//!
//! Paragraphs are stored as a single raw text leaf until [`finish`] resolves
//! inlines against the document's reference definitions. Quote prefixes and
//! list continuation indents are inserted around that raw text first, so the
//! inline pass sees the logical line and the bytes still concatenate.

use crate::green::Green;
use crate::inline::{self, parse_reference_def, Defs};
use crate::kind::SyntaxKind;
use rndr_liquid::FENCE_LANGS;

/// Parse `src` into a document whose concatenation equals `src`.
pub fn parse(src: &str) -> Green {
    let mut doc = parse_raw(src);
    let defs = collect_defs(&doc);
    finish(&mut doc, &defs);
    debug_assert_eq!(doc.concat(), src, "parse dropped or invented bytes");
    doc
}

/// Parse blocks, leaving paragraph text raw, and resolve inlines with `defs`
/// plus any definitions inside `src`. Definitions in `defs_before` win over
/// later ones; `defs_after` apply only when the document does not define them.
pub fn parse_with(src: &str, defs_before: &Defs, defs_after: &Defs) -> Green {
    let mut doc = parse_raw(src);
    let mut defs = Defs::default();
    for (label, dest) in defs_before.iter() {
        defs.insert_first(label, dest);
    }
    let internal = collect_defs(&doc);
    for (label, dest) in internal.iter() {
        defs.insert_first(label, dest);
    }
    for (label, dest) in defs_after.iter() {
        defs.insert_first(label, dest);
    }
    finish(&mut doc, &defs);
    debug_assert_eq!(doc.concat(), src);
    doc
}

fn parse_raw(src: &str) -> Green {
    let mut children = Vec::new();
    let mut rest = src;
    let mut offset = 0;
    if let Some(stripped) = src.strip_prefix('\u{feff}') {
        children.push(Green::leaf(SyntaxKind::Bom, "\u{feff}"));
        rest = stripped;
        offset = '\u{feff}'.len_utf8();
    }
    let lines = split_lines(rest);
    let (blocks, consumed) = parse_seq(rest, &lines, 0, lines.len());
    debug_assert_eq!(consumed, lines.len());
    children.extend(blocks);
    let _ = offset;
    Green::branch(SyntaxKind::Document, children)
}

struct Line {
    start: usize,
    text_end: usize,
    end: usize,
}

fn split_lines(src: &str) -> Vec<Line> {
    let bytes = src.as_bytes();
    let mut lines = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let start = i;
        while i < bytes.len() && bytes[i] != b'\n' && bytes[i] != b'\r' {
            i += 1;
        }
        let text_end = i;
        if i < bytes.len() {
            if bytes[i] == b'\r' && i + 1 < bytes.len() && bytes[i + 1] == b'\n' {
                i += 2;
            } else {
                i += 1;
            }
        }
        lines.push(Line {
            start,
            text_end,
            end: i,
        });
    }
    lines
}

fn parse_seq(src: &str, lines: &[Line], mut i: usize, end: usize) -> (Vec<Green>, usize) {
    let mut out = Vec::new();
    while i < end {
        if is_blank(src, &lines[i]) {
            out.push(Green::leaf(
                SyntaxKind::BlockGap,
                &src[lines[i].start..lines[i].end],
            ));
            i += 1;
            continue;
        }
        if let Some((node, next)) = try_html(src, lines, i, end) {
            out.push(node);
            i = next;
            continue;
        }
        if let Some((node, next)) = try_fence(src, lines, i, end) {
            out.push(node);
            i = next;
            continue;
        }
        if let Some((node, next)) = try_opaque(src, lines, i, end) {
            out.push(node);
            i = next;
            continue;
        }
        if let Some((node, next)) = try_quote(src, lines, i, end) {
            out.push(node);
            i = next;
            continue;
        }
        if let Some((node, next)) = try_atx(src, lines, i) {
            out.push(node);
            i = next;
            continue;
        }
        if let Some((node, next)) = try_table(src, lines, i, end) {
            out.push(node);
            i = next;
            continue;
        }
        if thematic(src, &lines[i]).is_some() && !setext_continue(src, lines, i) {
            out.push(Green::leaf(
                SyntaxKind::ThematicBreak,
                &src[lines[i].start..lines[i].end],
            ));
            i += 1;
            continue;
        }
        if let Some((node, next)) = try_list(src, lines, i, end) {
            out.push(node);
            i = next;
            continue;
        }
        if let Some((node, next)) = try_ref(src, lines, i, end) {
            out.push(node);
            i = next;
            continue;
        }
        if let Some((node, next)) = try_indented_code(src, lines, i, end) {
            out.push(node);
            i = next;
            continue;
        }
        let (node, next) = take_paragraph(src, lines, i, end);
        out.push(node);
        i = next;
    }
    (out, i)
}

fn is_blank(src: &str, line: &Line) -> bool {
    src[line.start..line.text_end]
        .chars()
        .all(|c| c == ' ' || c == '\t')
}

fn indent_of(text: &str) -> (usize, usize) {
    let mut cols = 0;
    let mut bytes = 0;
    for c in text.chars() {
        if c == ' ' {
            cols += 1;
            bytes += 1;
        } else if c == '\t' {
            cols += 4 - (cols % 4);
            bytes += 1;
        } else {
            break;
        }
    }
    (cols, bytes)
}

fn try_atx(src: &str, lines: &[Line], i: usize) -> Option<(Green, usize)> {
    let line = &lines[i];
    let text = &src[line.start..line.text_end];
    let (cols, indent_bytes) = indent_of(text);
    if cols > 3 {
        return None;
    }
    let rest = &text[indent_bytes..];
    let hashes = rest.bytes().take_while(|b| *b == b'#').count();
    if !(1..=6).contains(&hashes) {
        return None;
    }
    if rest.len() > hashes {
        let after = rest.as_bytes()[hashes];
        if after != b' ' && after != b'\t' {
            return None;
        }
    }
    let mut content_start = indent_bytes + hashes;
    if content_start < text.len() && text.as_bytes()[content_start] == b' ' {
        content_start += 1;
    }
    let mut content_end = text.len();
    let bytes = text.as_bytes();
    let mut close = 0;
    while content_end > content_start && bytes[content_end - 1] == b' ' {
        content_end -= 1;
    }
    while content_end > content_start && bytes[content_end - 1] == b'#' {
        close += 1;
        content_end -= 1;
    }
    if close > 0 && content_end > content_start && bytes[content_end - 1] == b' ' {
        content_end -= 1;
    } else if close > 0 && content_end != content_start {
        // Closing hashes must be preceded by a space, or be the whole content.
        content_end += close;
        close = 0;
    }
    let mut children = vec![Green::leaf(
        SyntaxKind::AtxMarker,
        &src[line.start..line.start + content_start],
    )];
    if content_start < content_end {
        children.push(Green::leaf(
            SyntaxKind::Text,
            &src[line.start + content_start..line.start + content_end],
        ));
    }
    if close > 0 {
        let marker_from = line.start + content_end;
        children.push(Green::leaf(
            SyntaxKind::AtxMarker,
            &src[marker_from..line.text_end],
        ));
    } else if content_end < text.len() {
        children.push(Green::leaf(
            SyntaxKind::Text,
            &src[line.start + content_end..line.text_end],
        ));
    }
    if line.end > line.text_end {
        children.push(Green::leaf(
            SyntaxKind::LineEnding,
            &src[line.text_end..line.end],
        ));
    }
    Some((Green::branch(SyntaxKind::Heading, children), i + 1))
}

fn setext_underline(text: &str) -> bool {
    let (cols, indent) = indent_of(text);
    if cols > 3 {
        return false;
    }
    let rest = text[indent..].trim_end_matches([' ', '\t']);
    if rest.is_empty() {
        return false;
    }
    let c = rest.as_bytes()[0];
    (c == b'=' || c == b'-') && rest.bytes().all(|b| b == c)
}

fn setext_continue(src: &str, lines: &[Line], i: usize) -> bool {
    i > 0
        && setext_underline(&src[lines[i].start..lines[i].text_end])
        && !is_blank(src, &lines[i - 1])
}

fn thematic(src: &str, line: &Line) -> Option<()> {
    let text = &src[line.start..line.text_end];
    let (cols, indent) = indent_of(text);
    if cols > 3 {
        return None;
    }
    let rest = &text.as_bytes()[indent..];
    if rest.is_empty() {
        return None;
    }
    let c = rest[0];
    if c != b'-' && c != b'_' && c != b'*' {
        return None;
    }
    let mut count = 0;
    for b in rest {
        if *b == c {
            count += 1;
        } else if *b == b' ' || *b == b'\t' {
            continue;
        } else {
            return None;
        }
    }
    (count >= 3).then_some(())
}

fn try_fence(src: &str, lines: &[Line], i: usize, end: usize) -> Option<(Green, usize)> {
    let open = fence_open(src, &lines[i])?;
    let mut j = i + 1;
    while j < end {
        if fence_close(src, &lines[j], open.ch, open.len) {
            j += 1;
            break;
        }
        j += 1;
    }
    let stop = if j == i + 1 && j >= end {
        lines[end - 1].end
    } else {
        lines[j - 1].end
    };
    // `j` is the line after the closer, or `end` if unclosed. The node covers
    // [start, stop).
    let _ = stop;
    let node_end = lines[j.min(end) - 1].end;
    // When unclosed, j == end, node_end is the last consumed line. When the
    // loop breaks on a closer, j was incremented past it, so j-1 is the closer.
    let mut children = vec![Green::leaf(
        SyntaxKind::FenceLine,
        &src[lines[i].start..lines[i].end],
    )];
    let body_start = lines[i].end;
    let closed = j <= end && j > i + 1 && fence_close(src, &lines[j - 1], open.ch, open.len);
    let body_end = if closed { lines[j - 1].start } else { node_end };
    if body_end > body_start {
        if open.liquid {
            children.extend(parse_liquid_body(
                &src[body_start..body_end],
                body_start,
                &open.info,
            ));
        } else {
            children.push(Green::leaf(
                SyntaxKind::OpaqueInline,
                &src[body_start..body_end],
            ));
        }
    }
    if closed {
        children.push(Green::leaf(
            SyntaxKind::FenceLine,
            &src[lines[j - 1].start..lines[j - 1].end],
        ));
    }
    let kind = if open.liquid {
        SyntaxKind::LiquidFence
    } else {
        SyntaxKind::FencedCode
    };
    Some((Green::branch(kind, children), if closed { j } else { end }))
}

struct FenceOpen {
    ch: u8,
    len: usize,
    info: String,
    liquid: bool,
}

fn fence_open(src: &str, line: &Line) -> Option<FenceOpen> {
    let text = &src[line.start..line.text_end];
    let (cols, indent) = indent_of(text);
    if cols > 3 {
        return None;
    }
    let rest = &text[indent..];
    let bytes = rest.as_bytes();
    if bytes.is_empty() || (bytes[0] != b'`' && bytes[0] != b'~') {
        return None;
    }
    let ch = bytes[0];
    let len = bytes.iter().take_while(|b| **b == ch).count();
    if len < 3 {
        return None;
    }
    let info = rest[len..].trim();
    if ch == b'`' && info.contains('`') {
        return None;
    }
    let word = info.split_whitespace().next().unwrap_or("").to_lowercase();
    let liquid = FENCE_LANGS.iter().any(|lang| *lang == word);
    Some(FenceOpen {
        ch,
        len,
        info: word,
        liquid,
    })
}

fn fence_close(src: &str, line: &Line, ch: u8, len: usize) -> bool {
    let text = &src[line.start..line.text_end];
    let (cols, indent) = indent_of(text);
    if cols > 3 {
        return false;
    }
    let rest = text[indent..].trim_end_matches([' ', '\t']);
    let bytes = rest.as_bytes();
    bytes.len() >= len && bytes.iter().all(|b| *b == ch)
}

fn parse_liquid_body(body: &str, _abs: usize, info: &str) -> Vec<Green> {
    let lines = split_lines(body);
    if lines.is_empty() {
        return vec![Green::leaf(SyntaxKind::OpaqueInline, body)];
    }
    let trailing_markdown = matches!(info, "report" | "section" | "block");
    let mut out = Vec::new();
    let mut i = 0;
    let mut saw_field = false;
    while i < lines.len() {
        let line = &lines[i];
        let text = &body[line.start..line.text_end];
        let trimmed = text.trim();
        if trimmed.is_empty() {
            if trailing_markdown && saw_field {
                break;
            }
            out.push(Green::leaf(
                SyntaxKind::BlockGap,
                &body[line.start..line.end],
            ));
            i += 1;
            continue;
        }
        if trimmed.starts_with('#') {
            out.push(Green::leaf(
                SyntaxKind::FieldComment,
                &body[line.start..line.end],
            ));
            i += 1;
            continue;
        }
        if trimmed.bytes().all(|b| b == b'-') && trimmed.len() >= 3 {
            out.push(Green::leaf(
                SyntaxKind::FieldSeparator,
                &body[line.start..line.end],
            ));
            i += 1;
            continue;
        }
        if is_field_line(trimmed) || is_pipe_row(trimmed) {
            out.push(field_node(body, line));
            saw_field = true;
            i += 1;
            continue;
        }
        if trailing_markdown && saw_field {
            break;
        }
        out.push(Green::leaf(
            SyntaxKind::OpaqueInline,
            &body[line.start..line.end],
        ));
        i += 1;
    }
    if i < lines.len() {
        let start = lines[i].start;
        let end = lines.last().map(|l| l.end).unwrap_or(start);
        if end > start {
            let nested = parse_raw(&body[start..end]);
            // parse_raw returns a document. Hoist its children, which already
            // concatenate to the trailing bytes.
            if nested.kind() == SyntaxKind::Document {
                out.extend(nested.children().to_vec());
            } else {
                out.push(nested);
            }
        }
    }
    out
}

fn is_field_line(trimmed: &str) -> bool {
    let bytes = trimmed.as_bytes();
    if bytes.is_empty() || !bytes[0].is_ascii_alphabetic() {
        return false;
    }
    let mut i = 1;
    while i < bytes.len()
        && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_' || bytes[i] == b'-')
    {
        i += 1;
    }
    while i < bytes.len() && bytes[i] == b' ' {
        i += 1;
    }
    i < bytes.len() && bytes[i] == b':'
}

fn is_pipe_row(trimmed: &str) -> bool {
    trimmed.contains('|') && trimmed.contains(':')
}

fn field_node(body: &str, line: &Line) -> Green {
    let text = &body[line.start..line.text_end];
    let nl = &body[line.text_end..line.end];
    if is_pipe_row(text.trim()) && !is_field_line(text.trim()) {
        let mut children = vec![Green::leaf(SyntaxKind::FieldValue, text)];
        if !nl.is_empty() {
            children.push(Green::leaf(SyntaxKind::LineEnding, nl));
        }
        return Green::branch(SyntaxKind::PipeRow, children);
    }
    let Some(colon) = text.find(':') else {
        return raw_line(body, line, SyntaxKind::OpaqueInline);
    };
    let key = &text[..colon];
    let after = &text[colon + 1..];
    let value_lead = after.len() - after.trim_start().len();
    let sep = &text[colon..colon + 1 + value_lead];
    let value = &text[colon + 1 + value_lead..];
    let mut children = vec![
        Green::leaf(SyntaxKind::FieldKey, key),
        Green::leaf(SyntaxKind::FieldSep, sep),
    ];
    if let Some(scalar) = block_scalar_indicator(value) {
        children.push(Green::leaf(SyntaxKind::BlockScalar, scalar));
        let rest = &value[scalar.len()..];
        if !rest.is_empty() {
            children.push(Green::leaf(SyntaxKind::FieldValue, rest));
        }
    } else if let Some((quote, inner)) = quoted_value(value) {
        children.push(Green::leaf(SyntaxKind::Delimiter, quote));
        children.push(Green::leaf(SyntaxKind::FieldValue, inner));
        children.push(Green::leaf(SyntaxKind::Delimiter, quote));
        let used = quote.len() + inner.len() + quote.len();
        if used < value.len() {
            children.push(Green::leaf(SyntaxKind::Text, &value[used..]));
        }
    } else {
        children.push(Green::leaf(SyntaxKind::FieldValue, value));
    }
    if !nl.is_empty() {
        children.push(Green::leaf(SyntaxKind::LineEnding, nl));
    }
    Green::branch(SyntaxKind::Field, children)
}

fn block_scalar_indicator(value: &str) -> Option<&str> {
    let v = value.trim();
    matches!(v, "|" | "|-" | "|+" | ">" | ">-" | ">+").then_some(v)
}

fn quoted_value(value: &str) -> Option<(&str, &str)> {
    let bytes = value.as_bytes();
    if bytes.len() >= 2
        && (bytes[0] == b'"' || bytes[0] == b'\'')
        && bytes[bytes.len() - 1] == bytes[0]
    {
        let q = &value[..1];
        Some((q, &value[1..value.len() - 1]))
    } else {
        None
    }
}

fn raw_line(src: &str, line: &Line, kind: SyntaxKind) -> Green {
    Green::leaf(kind, &src[line.start..line.end])
}

fn try_quote(src: &str, lines: &[Line], i: usize, end: usize) -> Option<(Green, usize)> {
    quote_prefix_len(src, &lines[i])?;
    let mut j = i;
    let mut prefixes = Vec::new();
    let mut inner = String::new();
    while j < end {
        if is_blank(src, &lines[j]) {
            break;
        }
        if let Some(pref) = quote_prefix_len(src, &lines[j]) {
            let text_end = lines[j].text_end;
            let start = lines[j].start;
            prefixes.push(src[start..start + pref].to_string());
            inner.push_str(&src[start + pref..text_end]);
            inner.push_str(&src[text_end..lines[j].end]);
            j += 1;
            continue;
        }
        // Lazy continuation of the quote paragraph.
        if j > i && !interrupts(src, &lines[j], true) {
            prefixes.push(String::new());
            inner.push_str(&src[lines[j].start..lines[j].end]);
            j += 1;
            continue;
        }
        break;
    }
    let inner_doc = parse_raw(&inner);
    let injected = inject_prefixes(inner_doc.children(), &prefixes);
    let node = Green::branch(SyntaxKind::BlockQuote, injected);
    debug_assert_eq!(node.concat(), &src[lines[i].start..lines[j - 1].end]);
    Some((node, j))
}

fn quote_prefix_len(src: &str, line: &Line) -> Option<usize> {
    let text = &src[line.start..line.text_end];
    let bytes = text.as_bytes();
    let mut i = 0;
    let mut spaces = 0;
    while i < bytes.len() && bytes[i] == b' ' && spaces < 3 {
        i += 1;
        spaces += 1;
    }
    if i < bytes.len() && bytes[i] == b'>' {
        i += 1;
        if i < bytes.len() && bytes[i] == b' ' {
            i += 1;
        }
        Some(i)
    } else {
        None
    }
}

fn inject_prefixes(nodes: &[Green], prefixes: &[String]) -> Vec<Green> {
    let mut st = Inject {
        line: 0,
        at_bol: true,
        prefixes,
    };
    map_seq(nodes, &mut st)
}

struct Inject<'a> {
    line: usize,
    at_bol: bool,
    prefixes: &'a [String],
}

fn map_seq(nodes: &[Green], st: &mut Inject<'_>) -> Vec<Green> {
    let mut out = Vec::new();
    for node in nodes {
        if node.children().is_empty() {
            expand_leaf(node, st, &mut out);
        } else {
            out.push(Green::branch(node.kind(), map_seq(node.children(), st)));
        }
    }
    out
}

fn expand_leaf(node: &Green, st: &mut Inject<'_>, out: &mut Vec<Green>) {
    let text = node.text();
    if text.is_empty() {
        return;
    }
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if st.at_bol {
            push_prefix(out, st);
        }
        if bytes[i] == b'\n' || bytes[i] == b'\r' {
            let nl = if bytes[i] == b'\r' && i + 1 < bytes.len() && bytes[i + 1] == b'\n' {
                2
            } else {
                1
            };
            out.push(Green::leaf(node.kind(), &text[i..i + nl]));
            i += nl;
            st.at_bol = true;
            st.line += 1;
        } else {
            let start = i;
            while i < bytes.len() && bytes[i] != b'\n' && bytes[i] != b'\r' {
                i += 1;
            }
            out.push(Green::leaf(node.kind(), &text[start..i]));
            st.at_bol = false;
        }
    }
}

fn push_prefix(out: &mut Vec<Green>, st: &mut Inject<'_>) {
    if let Some(prefix) = st.prefixes.get(st.line) {
        if !prefix.is_empty() {
            out.push(Green::leaf(SyntaxKind::LinePrefix, prefix.clone()));
        }
    }
    st.at_bol = false;
}

fn try_list(src: &str, lines: &[Line], i: usize, end: usize) -> Option<(Green, usize)> {
    let marker = list_marker(src, &lines[i])?;
    if thematic(src, &lines[i]).is_some() {
        return None;
    }
    let mut items = Vec::new();
    let mut j = i;
    while j < end {
        if is_blank(src, &lines[j]) {
            let mut k = j;
            while k < end && is_blank(src, &lines[k]) {
                k += 1;
            }
            if k < end && same_item(src, &lines[k], &marker) {
                while j < k {
                    items.push(Green::leaf(
                        SyntaxKind::BlockGap,
                        &src[lines[j].start..lines[j].end],
                    ));
                    j += 1;
                }
                continue;
            }
            break;
        }
        if !same_item(src, &lines[j], &marker) {
            break;
        }
        let (item, next) = take_item(src, lines, j, end, &marker);
        items.push(item);
        j = next;
    }
    if items.is_empty() {
        return None;
    }
    Some((Green::branch(SyntaxKind::List, items), j))
}

struct Marker {
    indent_cols: usize,
    bullet: bool,
    paren: bool,
    task: bool,
}

fn list_marker(src: &str, line: &Line) -> Option<Marker> {
    let text = &src[line.start..line.text_end];
    let (cols, indent) = indent_of(text);
    if cols > 3 {
        return None;
    }
    let rest = &text[indent..];
    let bytes = rest.as_bytes();
    if bytes.is_empty() {
        return None;
    }
    if matches!(bytes[0], b'-' | b'+' | b'*') {
        if bytes.len() > 1 && bytes[1] != b' ' && bytes[1] != b'\t' {
            return None;
        }
        let task = bytes.len() >= 5
            && bytes[1] == b' '
            && bytes[2] == b'['
            && (bytes[3] == b' ' || bytes[3] == b'x' || bytes[3] == b'X')
            && bytes[4] == b']';
        return Some(Marker {
            indent_cols: cols,
            bullet: true,
            paren: false,
            task,
        });
    }
    let mut n = 0;
    while n < bytes.len() && bytes[n].is_ascii_digit() && n < 9 {
        n += 1;
    }
    if n == 0 || n >= bytes.len() {
        return None;
    }
    if bytes[n] != b'.' && bytes[n] != b')' {
        return None;
    }
    if n + 1 < bytes.len() && bytes[n + 1] != b' ' && bytes[n + 1] != b'\t' {
        return None;
    }
    Some(Marker {
        indent_cols: cols,
        bullet: false,
        paren: bytes[n] == b')',
        task: false,
    })
}

fn same_item(src: &str, line: &Line, marker: &Marker) -> bool {
    let Some(next) = list_marker(src, line) else {
        return false;
    };
    if thematic(src, line).is_some() {
        return false;
    }
    next.indent_cols == marker.indent_cols
        && next.bullet == marker.bullet
        && next.paren == marker.paren
        && next.task == marker.task
}

fn marker_bytes(src: &str, line: &Line) -> usize {
    let text = &src[line.start..line.text_end];
    let (_cols, indent) = indent_of(text);
    let rest = &text[indent..];
    let bytes = rest.as_bytes();
    let mut i = 0;
    if matches!(bytes.first(), Some(b'-' | b'+' | b'*')) {
        i = 1;
    } else {
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i < bytes.len() && (bytes[i] == b'.' || bytes[i] == b')') {
            i += 1;
        }
    }
    if i < bytes.len() && bytes[i] == b' ' {
        i += 1;
        if rest[i..].starts_with('[') {
            if let Some(rel) = rest[i..].find(']') {
                let after = i + rel + 1;
                if after < rest.len() && rest.as_bytes()[after] == b' ' {
                    i = after + 1;
                }
            }
        }
    }
    // Up to three more spaces stay with the marker when content follows.
    let mut extra = 0;
    while i + extra < bytes.len() && bytes[i + extra] == b' ' && extra < 3 {
        extra += 1;
    }
    if i + extra < bytes.len() {
        i += extra;
    }
    indent + i
}

fn take_item(src: &str, lines: &[Line], i: usize, end: usize, marker: &Marker) -> (Green, usize) {
    let marker_len = marker_bytes(src, &lines[i]);
    let content_col = column_width(&src[lines[i].start..lines[i].start + marker_len]);
    let mut j = i + 1;
    while j < end {
        if is_blank(src, &lines[j]) {
            let mut k = j;
            while k < end && is_blank(src, &lines[k]) {
                k += 1;
            }
            if k < end && indented_continuation(src, &lines[k], content_col, marker) {
                j += 1;
                continue;
            }
            break;
        }
        if same_item(src, &lines[j], marker) {
            break;
        }
        if indented_continuation(src, &lines[j], content_col, marker)
            || lazy_continuation(src, &lines[j])
        {
            j += 1;
            continue;
        }
        break;
    }
    let (children, _) = build_item(src, lines, i, j, marker_len);
    (Green::branch(SyntaxKind::ListItem, children), j)
}

fn column_width(s: &str) -> usize {
    let mut cols = 0;
    for c in s.chars() {
        if c == '\t' {
            cols += 4 - (cols % 4);
        } else {
            cols += 1;
        }
    }
    cols
}

fn indented_continuation(src: &str, line: &Line, content_col: usize, marker: &Marker) -> bool {
    if same_item(src, line, marker) || is_blank(src, line) {
        return false;
    }
    let text = &src[line.start..line.text_end];
    indent_of(text).0 >= content_col
}

fn lazy_continuation(src: &str, line: &Line) -> bool {
    !is_blank(src, line) && !interrupts(src, line, true)
}

fn build_item(
    src: &str,
    lines: &[Line],
    start: usize,
    end: usize,
    marker_len: usize,
) -> (Vec<Green>, ()) {
    let mut prefixes = Vec::new();
    let mut inner = String::new();
    let first = &lines[start];
    let marker = &src[first.start..first.start + marker_len];
    prefixes.push(String::new());
    inner.push_str(&src[first.start + marker_len..first.end]);
    let content_col = column_width(&src[first.start..first.start + marker_len]);
    for line in lines.iter().take(end).skip(start + 1) {
        if is_blank(src, line) {
            prefixes.push(String::new());
            inner.push_str(&src[line.start..line.end]);
            continue;
        }
        let text = &src[line.start..line.text_end];
        let (cols, indent_bytes) = indent_of(text);
        if cols >= content_col {
            let skip = skip_columns(text, content_col);
            prefixes.push(src[line.start..line.start + skip].to_string());
            inner.push_str(&src[line.start + skip..line.end]);
        } else {
            prefixes.push(String::new());
            inner.push_str(&src[line.start..line.end]);
        }
        let _ = indent_bytes;
    }
    let inner_doc = parse_raw(&inner);
    let mut children = vec![Green::leaf(SyntaxKind::ListMarker, marker)];
    children.extend(inject_prefixes(inner_doc.children(), &prefixes));
    debug_assert_eq!(
        Green::branch(SyntaxKind::ListItem, children.clone()).concat(),
        &src[lines[start].start..lines[end - 1].end]
    );
    (children, ())
}

fn skip_columns(text: &str, cols: usize) -> usize {
    let mut have = 0;
    for (i, c) in text.char_indices() {
        if have >= cols {
            return i;
        }
        if c == '\t' {
            let w = 4 - (have % 4);
            if have + w > cols {
                return i;
            }
            have += w;
        } else {
            have += 1;
        }
    }
    text.len()
}

fn interrupts(src: &str, line: &Line, in_paragraph: bool) -> bool {
    if fence_open(src, line).is_some()
        || quote_prefix_len(src, line).is_some()
        || try_atx(src, std::slice::from_ref(line), 0).is_some()
        || html_start(src, line).is_some()
        || thematic(src, line).is_some()
    {
        return true;
    }
    if let Some(marker) = list_marker(src, line) {
        if thematic(src, line).is_some() {
            return false;
        }
        if !in_paragraph {
            return true;
        }
        if marker.bullet {
            return true;
        }
        let text = &src[line.start..line.text_end];
        let (_c, indent) = indent_of(text);
        let rest = &text[indent..];
        return rest.starts_with('1')
            && (rest[1..].starts_with(". ") || rest[1..].starts_with(") "));
    }
    false
}

fn take_paragraph(src: &str, lines: &[Line], i: usize, end: usize) -> (Green, usize) {
    let mut j = i + 1;
    while j < end {
        if is_blank(src, &lines[j])
            || interrupts(src, &lines[j], true)
            || setext_underline(&src[lines[j].start..lines[j].text_end])
        {
            break;
        }
        j += 1;
    }
    if j < end && setext_underline(&src[lines[j].start..lines[j].text_end]) {
        let mut children = vec![Green::leaf(
            SyntaxKind::Text,
            &src[lines[i].start..lines[j].start],
        )];
        children.push(Green::leaf(
            SyntaxKind::SetextUnderline,
            &src[lines[j].start..lines[j].end],
        ));
        return (Green::branch(SyntaxKind::Heading, children), j + 1);
    }
    let node = Green::branch(
        SyntaxKind::Paragraph,
        vec![Green::leaf(
            SyntaxKind::Text,
            &src[lines[i].start..lines[j - 1].end],
        )],
    );
    (node, j)
}

fn try_indented_code(src: &str, lines: &[Line], i: usize, end: usize) -> Option<(Green, usize)> {
    let text = &src[lines[i].start..lines[i].text_end];
    let (cols, _) = indent_of(text);
    if cols < 4 || is_blank(src, &lines[i]) {
        return None;
    }
    let mut j = i + 1;
    while j < end && !is_blank(src, &lines[j]) {
        let (c, _) = indent_of(&src[lines[j].start..lines[j].text_end]);
        if c < 4 {
            break;
        }
        j += 1;
    }
    Some((
        Green::branch(
            SyntaxKind::IndentedCode,
            vec![Green::leaf(
                SyntaxKind::OpaqueInline,
                &src[lines[i].start..lines[j - 1].end],
            )],
        ),
        j,
    ))
}

fn try_ref(src: &str, lines: &[Line], i: usize, end: usize) -> Option<(Green, usize)> {
    let start = lines[i].start;
    let limit = lines[end - 1].end;
    let (label, dest, abs_end) = parse_reference_def(src, start, limit)?;
    if label.is_empty() || dest.is_empty() {
        return None;
    }
    // The definition must end on a line boundary we actually scanned.
    let mut j = i;
    while j < end && lines[j].end < abs_end {
        j += 1;
    }
    if j >= end || lines[j].end != abs_end && lines[j].text_end != abs_end && lines[j].end < abs_end
    {
        // Accept a definition that ends at the line end.
    }
    if abs_end > lines[j.min(end - 1)].end {
        return None;
    }
    let mut stop = i;
    while stop < end && lines[stop].end < abs_end {
        stop += 1;
    }
    if stop < end && lines[stop].end >= abs_end {
        // If the def ends mid-line, don't take it; reference defs are line based.
        if lines[stop].end != abs_end && abs_end != lines[stop].text_end {
            return None;
        }
        return Some((
            Green::leaf(SyntaxKind::LinkRefDef, &src[start..lines[stop].end]),
            stop + 1,
        ));
    }
    None
}

fn try_table(src: &str, lines: &[Line], i: usize, end: usize) -> Option<(Green, usize)> {
    if i + 1 >= end {
        return None;
    }
    let header = &src[lines[i].start..lines[i].text_end];
    if !header.contains('|') {
        return None;
    }
    let delim = &src[lines[i + 1].start..lines[i + 1].text_end];
    if !is_delim_row(delim) {
        return None;
    }
    let mut j = i + 2;
    while j < end && !is_blank(src, &lines[j]) {
        let text = &src[lines[j].start..lines[j].text_end];
        if !text.contains('|') || interrupts(src, &lines[j], true) {
            break;
        }
        j += 1;
    }
    let mut rows = Vec::new();
    for line in lines.iter().take(j).skip(i) {
        rows.push(table_row(src, line));
    }
    Some((Green::branch(SyntaxKind::Table, rows), j))
}

fn is_delim_row(text: &str) -> bool {
    let mut cells = 0;
    for cell in split_row(text) {
        let t = cell.trim().trim_matches('|').trim();
        if t.is_empty() {
            continue;
        }
        let bytes = t.as_bytes();
        let mut i = 0;
        if bytes.first() == Some(&b':') {
            i += 1;
        }
        let dashes = bytes[i..].iter().take_while(|b| **b == b'-').count();
        if dashes < 3 {
            return false;
        }
        i += dashes;
        if i < bytes.len() && bytes[i] == b':' {
            i += 1;
        }
        if i != bytes.len() {
            return false;
        }
        cells += 1;
    }
    cells >= 1
}

fn split_row(text: &str) -> Vec<&str> {
    let trimmed = text.trim();
    let trimmed = trimmed.strip_prefix('|').unwrap_or(trimmed);
    let trimmed = trimmed.strip_suffix('|').unwrap_or(trimmed);
    trimmed.split('|').collect()
}

fn table_row(src: &str, line: &Line) -> Green {
    let text = &src[line.start..line.text_end];
    let mut children = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'|' {
            children.push(Green::leaf(SyntaxKind::Pipe, "|"));
            i += 1;
            continue;
        }
        if bytes[i] == b'\\' && i + 1 < bytes.len() && bytes[i + 1] == b'|' {
            children.push(Green::leaf(SyntaxKind::Text, &text[i..i + 2]));
            i += 2;
            continue;
        }
        let start = i;
        while i < bytes.len() && bytes[i] != b'|' {
            if bytes[i] == b'\\' && i + 1 < bytes.len() && bytes[i + 1] == b'|' {
                break;
            }
            i += 1;
        }
        children.push(Green::branch(
            SyntaxKind::TableCell,
            vec![Green::leaf(SyntaxKind::Text, &text[start..i])],
        ));
    }
    if line.end > line.text_end {
        children.push(Green::leaf(
            SyntaxKind::LineEnding,
            &src[line.text_end..line.end],
        ));
    }
    Green::branch(SyntaxKind::TableRow, children)
}

fn try_opaque(src: &str, lines: &[Line], i: usize, end: usize) -> Option<(Green, usize)> {
    let text = src[lines[i].start..lines[i].text_end].trim();
    if text != "$$" && text != ":::" && !text.starts_with("$$") && !text.starts_with(":::") {
        return None;
    }
    let closer = if text.starts_with("$$") { "$$" } else { ":::" };
    if text != closer && !text.starts_with(closer) {
        return None;
    }
    // A single opener line is already an opaque block. A matching closer ends it.
    let mut j = i + 1;
    if text == closer {
        while j < end {
            let t = src[lines[j].start..lines[j].text_end].trim();
            j += 1;
            if t == closer {
                break;
            }
        }
    }
    Some((
        Green::branch(
            SyntaxKind::OpaqueBlock,
            vec![Green::leaf(
                SyntaxKind::OpaqueInline,
                &src[lines[i].start..lines[j - 1].end],
            )],
        ),
        j,
    ))
}

fn try_html(src: &str, lines: &[Line], i: usize, end: usize) -> Option<(Green, usize)> {
    let kind = html_start(src, &lines[i])?;
    let mut j = i + 1;
    match kind {
        HtmlKind::Script(tag) | HtmlKind::Until(tag) => {
            while j < end {
                let text = &src[lines[j].start..lines[j].text_end];
                let hit = text
                    .to_ascii_lowercase()
                    .contains(&tag.to_ascii_lowercase());
                j += 1;
                if hit {
                    break;
                }
            }
        }
        HtmlKind::Blank => {
            while j < end && !is_blank(src, &lines[j]) {
                j += 1;
            }
        }
    }
    Some((
        Green::branch(
            SyntaxKind::HtmlBlock,
            vec![Green::leaf(
                SyntaxKind::OpaqueInline,
                &src[lines[i].start..lines[j - 1].end],
            )],
        ),
        j,
    ))
}

enum HtmlKind {
    Script(&'static str),
    Until(&'static str),
    Blank,
}

fn html_start(src: &str, line: &Line) -> Option<HtmlKind> {
    let text = &src[line.start..line.text_end];
    let (cols, indent) = indent_of(text);
    if cols > 3 {
        return None;
    }
    let rest = text[indent..].trim_start();
    let lower = rest.to_ascii_lowercase();
    if !lower.starts_with('<') {
        return None;
    }
    for (open, close) in [
        ("<script", "</script>"),
        ("<pre", "</pre>"),
        ("<style", "</style>"),
        ("<textarea", "</textarea>"),
    ] {
        if lower.starts_with(open)
            && lower
                .as_bytes()
                .get(open.len())
                .is_none_or(|b| b.is_ascii_whitespace() || *b == b'>')
        {
            return Some(HtmlKind::Script(close));
        }
    }
    if lower.starts_with("<!--") {
        return Some(HtmlKind::Until("-->"));
    }
    if lower.starts_with("<?") {
        return Some(HtmlKind::Until("?>"));
    }
    if lower.starts_with("<![cdata[") {
        return Some(HtmlKind::Until("]]>"));
    }
    if lower.starts_with("<!")
        && lower
            .as_bytes()
            .get(2)
            .is_some_and(|b| b.is_ascii_uppercase())
    {
        return Some(HtmlKind::Until(">"));
    }
    if block_tag(&lower) {
        return Some(HtmlKind::Blank);
    }
    None
}

fn block_tag(lower: &str) -> bool {
    const TAGS: &[&str] = &[
        "address",
        "article",
        "aside",
        "base",
        "basefont",
        "blockquote",
        "body",
        "caption",
        "center",
        "col",
        "colgroup",
        "dd",
        "details",
        "dialog",
        "dir",
        "div",
        "dl",
        "dt",
        "fieldset",
        "figcaption",
        "figure",
        "footer",
        "form",
        "frame",
        "frameset",
        "h1",
        "h2",
        "h3",
        "h4",
        "h5",
        "h6",
        "head",
        "header",
        "hr",
        "html",
        "iframe",
        "legend",
        "li",
        "link",
        "main",
        "menu",
        "menuitem",
        "nav",
        "noframes",
        "ol",
        "optgroup",
        "option",
        "p",
        "param",
        "search",
        "section",
        "source",
        "summary",
        "table",
        "tbody",
        "td",
        "tfoot",
        "th",
        "thead",
        "title",
        "tr",
        "track",
        "ul",
    ];
    let rest = lower.trim_start_matches('<').trim_start_matches('/');
    let name = rest
        .split(|c: char| c.is_ascii_whitespace() || c == '>' || c == '/')
        .next()
        .unwrap_or("");
    TAGS.contains(&name)
}

fn collect_defs(doc: &Green) -> Defs {
    let mut defs = Defs::default();
    walk_defs(doc, &mut defs);
    defs
}

fn walk_defs(node: &Green, defs: &mut Defs) {
    if node.kind() == SyntaxKind::LinkRefDef {
        let text = node.concat();
        if let Some((label, dest, _)) = parse_reference_def(&text, 0, text.len()) {
            defs.insert_first(&label, &dest);
        }
        return;
    }
    for child in node.children() {
        walk_defs(child, defs);
    }
}

fn finish(node: &mut Green, defs: &Defs) {
    let done = finish_node(node, defs);
    *node = done;
}

fn finish_node(node: &Green, defs: &Defs) -> Green {
    let children: Vec<Green> = node
        .children()
        .iter()
        .map(|child| finish_node(child, defs))
        .collect();
    match node.kind() {
        SyntaxKind::Paragraph if is_raw_paragraph(&children) => refill_paragraph(&children, defs),
        SyntaxKind::Heading => refill_heading(&children, defs),
        SyntaxKind::TableCell if is_raw_paragraph(&children) => {
            let text = concat_children(&children);
            Green::branch(SyntaxKind::TableCell, inline::parse_inlines(&text, defs))
        }
        _ => {
            if children.is_empty() {
                node.clone()
            } else {
                Green::branch(node.kind(), children)
            }
        }
    }
}

fn is_raw_paragraph(children: &[Green]) -> bool {
    !children.is_empty()
        && children.iter().all(|c| {
            matches!(
                c.kind(),
                SyntaxKind::Text | SyntaxKind::LinePrefix | SyntaxKind::LineEnding
            )
        })
}

fn concat_children(children: &[Green]) -> String {
    let mut out = String::new();
    for child in children {
        out.push_str(&child.concat());
    }
    out
}

fn refill_paragraph(children: &[Green], defs: &Defs) -> Green {
    if children.iter().any(|c| c.kind() == SyntaxKind::LinePrefix) {
        let (logical, prefixes) = logical_source(children);
        let inlines = inline::parse_inlines(&logical, defs);
        return Green::branch(SyntaxKind::Paragraph, inject_prefixes(&inlines, &prefixes));
    }
    let text = concat_children(children);
    Green::branch(SyntaxKind::Paragraph, inline::parse_inlines(&text, defs))
}

fn logical_source(children: &[Green]) -> (String, Vec<String>) {
    let mut logical = String::new();
    let mut prefixes = Vec::new();
    let mut at_bol = true;
    let mut line = 0;
    for child in children {
        if child.kind() == SyntaxKind::LinePrefix {
            while prefixes.len() <= line {
                prefixes.push(String::new());
            }
            prefixes[line] = child.text().to_string();
            continue;
        }
        let text = child.concat();
        for ch in text.chars() {
            if at_bol {
                while prefixes.len() <= line {
                    prefixes.push(String::new());
                }
                at_bol = false;
            }
            logical.push(ch);
            if ch == '\n' {
                line += 1;
                at_bol = true;
            }
        }
    }
    (logical, prefixes)
}

fn refill_heading(children: &[Green], defs: &Defs) -> Green {
    if let Some(pos) = children
        .iter()
        .position(|c| c.kind() == SyntaxKind::SetextUnderline)
    {
        let text = concat_children(&children[..pos]);
        let mut out = inline::parse_inlines(&text, defs);
        out.extend(children[pos..].iter().cloned());
        return Green::branch(SyntaxKind::Heading, out);
    }
    let mut out = Vec::new();
    let mut i = 0;
    if children
        .first()
        .is_some_and(|c| c.kind() == SyntaxKind::AtxMarker)
    {
        out.push(children[0].clone());
        i = 1;
    }
    let end = children
        .iter()
        .rposition(|c| c.kind() == SyntaxKind::AtxMarker || c.kind() == SyntaxKind::LineEnding)
        .unwrap_or(children.len().saturating_sub(1));
    let content_end = if children
        .get(end)
        .is_some_and(|c| c.kind() == SyntaxKind::AtxMarker || c.kind() == SyntaxKind::LineEnding)
        && end >= i
    {
        end
    } else {
        children.len()
    };
    if i < content_end {
        let text = concat_children(&children[i..content_end]);
        out.extend(inline::parse_inlines(&text, defs));
    }
    if content_end < children.len() {
        out.extend(children[content_end..].iter().cloned());
    }
    Green::branch(SyntaxKind::Heading, out)
}

/// Definitions in `node`, in walk order.
pub fn definitions_in(node: &Green) -> Defs {
    collect_defs(node)
}

/// Re-resolve inlines from the bytes already stored in `tree`.
///
/// Block boundaries stay put. Paragraphs, headings, and table cells are parsed
/// again with `defs`, so a changed reference definition updates link structure
/// without reprinting the file.
pub(crate) fn reresolve(tree: &Green, defs: &Defs) -> Green {
    reresolve_node(tree, defs)
}

fn reresolve_node(node: &Green, defs: &Defs) -> Green {
    match node.kind() {
        SyntaxKind::Paragraph => refill_paragraph_from_finished(node, defs),
        SyntaxKind::Heading => refill_heading(node.children(), defs),
        SyntaxKind::TableCell => Green::branch(
            SyntaxKind::TableCell,
            inline::parse_inlines(&node.concat(), defs),
        ),
        _ if node.children().is_empty() => node.clone(),
        _ => Green::branch(
            node.kind(),
            node.children()
                .iter()
                .map(|child| reresolve_node(child, defs))
                .collect(),
        ),
    }
}

fn refill_paragraph_from_finished(node: &Green, defs: &Defs) -> Green {
    if node
        .children()
        .iter()
        .any(|c| c.kind() == SyntaxKind::LinePrefix)
    {
        let (logical, prefixes) = logical_source(node.children());
        let inlines = inline::parse_inlines(&logical, defs);
        return Green::branch(SyntaxKind::Paragraph, inject_prefixes(&inlines, &prefixes));
    }
    Green::branch(
        SyntaxKind::Paragraph,
        inline::parse_inlines(&node.concat(), defs),
    )
}
