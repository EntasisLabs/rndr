//! GFM pipe tables.
//!
//! A table is a header row, a delimiter row with the same cell count, and zero
//! or more body rows. Cells split on `|` that is not escaped by a backslash.
//! An escaped backslash does not escape the pipe after it (`\\|` splits,
//! `\|` and `\\\|` do not). Alignment is the delimiter cell's colons.
//! Body rows may be shorter or longer than the header; the extra cells stay
//! in the tree so the bytes round-trip. Padding spaces stay inside the cell.

use std::ops::Range;

use crate::green::Green;
use crate::kind::SyntaxKind;

/// Column alignment taken from a delimiter cell.
///
/// `---` is [`Alignment::None`], distinct from a left colon. GFM renders the
/// unmarked column with no `align` attribute.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Alignment {
    /// Hyphens only: `---`.
    None,
    /// Leading colon: `:---`.
    Left,
    /// A colon on both ends: `:---:`.
    Center,
    /// Trailing colon: `---:`.
    Right,
}

/// One planned replacement. Offsets are bytes into the document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TextEdit {
    pub start: usize,
    pub end: usize,
    pub replacement: String,
}

/// Why a table edit was not planned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TableFault {
    /// An offset splits a scalar or falls outside the buffer.
    Range,
    /// The edit does not name a table cell, row, or column.
    Refused,
}

/// A table mutation the editor can ask for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TableEdit {
    /// Insert a body row. `index == body_len` appends. `at` is inside the table.
    InsertRow { at: usize, index: usize },
    /// Delete body row `index`.
    DeleteRow { at: usize, index: usize },
    /// Insert a column. `index == columns` appends.
    InsertColumn { at: usize, index: usize },
    /// Delete column `index`.
    DeleteColumn { at: usize, index: usize },
    /// Set the delimiter marker of column `index`.
    SetAlignment {
        at: usize,
        index: usize,
        alignment: Alignment,
    },
    /// Replace a cell's text. `start..end` is the cell, or its trimmed content.
    SetCell {
        start: usize,
        end: usize,
        text: String,
    },
}

struct RowMap {
    indent: Range<usize>,
    leading_pipe: Option<usize>,
    cells: Vec<Range<usize>>,
    trailing_pipe: Option<usize>,
    trailing: Range<usize>,
}

fn map_row(text: &str) -> RowMap {
    let pipes = pipe_positions(text);
    let bytes = text.as_bytes();
    let mut k = 0;
    while k < bytes.len() && is_ws(bytes[k]) {
        k += 1;
    }
    let leading = pipes.first().copied() == Some(k) && k < text.len();
    let indent = if leading { 0..k } else { 0..0 };
    let leading_pipe = leading.then_some(k);

    let mut cells = Vec::new();
    let mut trailing_pipe = None;
    let mut trailing = text.len()..text.len();
    let mut cell_start = if leading { k + 1 } else { 0 };
    let mut pipe_at = usize::from(leading);
    while pipe_at < pipes.len() {
        let pipe = pipes[pipe_at];
        let after = pipe + 1;
        let tail_is_ws = text[after..].bytes().all(is_ws);
        let last_pipe = pipe_at + 1 == pipes.len();
        if tail_is_ws && last_pipe {
            cells.push(cell_start..pipe);
            trailing_pipe = Some(pipe);
            trailing = after..text.len();
            cell_start = text.len();
            break;
        }
        cells.push(cell_start..pipe);
        cell_start = after;
        pipe_at += 1;
    }
    if cell_start < text.len() {
        let only_ws = text[cell_start..].bytes().all(is_ws);
        let follows_pipe = leading_pipe.is_some_and(|pipe| pipe + 1 == cell_start)
            || pipes.iter().any(|pipe| pipe + 1 == cell_start);
        if only_ws && follows_pipe && trailing_pipe.is_none() {
            trailing = cell_start..text.len();
        } else if trailing_pipe.is_none() {
            cells.push(cell_start..text.len());
        }
    }
    RowMap {
        indent,
        leading_pipe,
        cells,
        trailing_pipe,
        trailing,
    }
}

fn pipe_positions(text: &str) -> Vec<usize> {
    let bytes = text.as_bytes();
    let mut i = 0;
    let mut pipes = Vec::new();
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 1 < bytes.len() && is_ascii_punct(bytes[i + 1]) {
            i += 2;
            continue;
        }
        if bytes[i] == b'|' {
            pipes.push(i);
            i += 1;
            continue;
        }
        i += utf8_width(text, i);
    }
    pipes
}

/// How many cells `text` splits into. Zero when the line is not a row.
pub(crate) fn cell_count(text: &str) -> usize {
    map_row(text).cells.len()
}

/// Cell count of a delimiter row, when every cell is a marker.
pub(crate) fn delimiter_columns(text: &str) -> Option<usize> {
    let row = map_row(text);
    if row.cells.is_empty() {
        return None;
    }
    row.cells
        .iter()
        .all(|cell| is_marker(&text[cell.start..cell.end]))
        .then_some(row.cells.len())
}

pub(crate) fn row_nodes(text: &str) -> Vec<Green> {
    let row = map_row(text);
    let mut nodes = Vec::new();
    if !row.indent.is_empty() {
        nodes.push(Green::leaf(
            SyntaxKind::Text,
            &text[row.indent.start..row.indent.end],
        ));
    }
    if row.leading_pipe.is_some() {
        nodes.push(Green::leaf(SyntaxKind::Pipe, "|"));
    }
    for (index, cell) in row.cells.iter().enumerate() {
        if index > 0 {
            nodes.push(Green::leaf(SyntaxKind::Pipe, "|"));
        }
        nodes.push(cell_node(&text[cell.start..cell.end]));
    }
    if row.trailing_pipe.is_some() {
        nodes.push(Green::leaf(SyntaxKind::Pipe, "|"));
    }
    if !row.trailing.is_empty() {
        nodes.push(Green::leaf(
            SyntaxKind::Text,
            &text[row.trailing.start..row.trailing.end],
        ));
    }
    nodes
}

fn cell_node(text: &str) -> Green {
    if text.is_empty() {
        Green::branch(SyntaxKind::TableCell, Vec::new())
    } else {
        Green::branch(
            SyntaxKind::TableCell,
            vec![Green::leaf(SyntaxKind::Text, text)],
        )
    }
}

pub(crate) fn alignment_of(cell: &str) -> Alignment {
    let trimmed = cell.trim_matches([' ', '\t']);
    let left = trimmed.as_bytes().first() == Some(&b':');
    let right = trimmed.as_bytes().last() == Some(&b':');
    match (left, right) {
        (true, true) => Alignment::Center,
        (true, false) => Alignment::Left,
        (false, true) => Alignment::Right,
        (false, false) => Alignment::None,
    }
}

fn is_marker(cell: &str) -> bool {
    let trimmed = cell.trim_matches([' ', '\t']);
    let bytes = trimmed.as_bytes();
    if bytes.is_empty() {
        return false;
    }
    let mut i = usize::from(bytes[0] == b':');
    let dashes = i;
    while i < bytes.len() && bytes[i] == b'-' {
        i += 1;
    }
    if i == dashes {
        return false;
    }
    if i < bytes.len() && bytes[i] == b':' {
        i += 1;
    }
    i == bytes.len()
}

fn is_ws(byte: u8) -> bool {
    byte == b' ' || byte == b'\t'
}

fn is_ascii_punct(byte: u8) -> bool {
    byte.is_ascii_punctuation()
}

fn utf8_width(text: &str, index: usize) -> usize {
    text[index..]
        .chars()
        .next()
        .map(char::len_utf8)
        .unwrap_or(1)
}

struct RowSpan {
    range: Range<usize>,
    /// Row bytes excluding the line ending.
    text: Range<usize>,
    ending: Range<usize>,
    indent: Range<usize>,
    cells: Vec<Range<usize>>,
    pipes: Vec<Range<usize>>,
}

struct Roles {
    leading: Option<Range<usize>>,
    between: Vec<Range<usize>>,
    trailing: Option<Range<usize>>,
}

struct Located {
    rows: Vec<RowSpan>,
}

fn locate(tree: &Green, at: usize) -> Option<Located> {
    fn walk(node: &Green, start: usize, at: usize) -> Option<(Range<usize>, Vec<RowSpan>)> {
        let end = start + node.width();
        if at < start || at >= end {
            return None;
        }
        if node.kind() == SyntaxKind::Table {
            let rows = read_rows(node, start);
            if rows.len() >= 2 {
                return Some((start..end, rows));
            }
        }
        let mut cursor = start;
        for child in node.children() {
            if let Some(found) = walk(child, cursor, at) {
                return Some(found);
            }
            cursor += child.width();
        }
        None
    }
    walk(tree, 0, at).map(|(_, rows)| Located { rows })
}

fn read_rows(table: &Green, at: usize) -> Vec<RowSpan> {
    let mut cursor = at;
    let mut rows = Vec::new();
    for child in table.children() {
        let end = cursor + child.width();
        if child.kind() == SyntaxKind::TableRow {
            rows.push(read_row(child, cursor));
        }
        cursor = end;
    }
    rows
}

fn read_row(row: &Green, at: usize) -> RowSpan {
    let mut cursor = at;
    let mut cells = Vec::new();
    let mut pipes = Vec::new();
    let mut indent = at..at;
    let mut seen_structural = false;
    let mut ending = at..at;
    for child in row.children() {
        let end = cursor + child.width();
        match child.kind() {
            SyntaxKind::Pipe => {
                seen_structural = true;
                pipes.push(cursor..end);
            }
            SyntaxKind::TableCell => {
                seen_structural = true;
                cells.push(cursor..end);
            }
            SyntaxKind::LineEnding => ending = cursor..end,
            SyntaxKind::Text if !seen_structural => indent = cursor..end,
            _ => {}
        }
        cursor = end;
    }
    let text_end = if ending.is_empty() {
        cursor
    } else {
        ending.start
    };
    RowSpan {
        range: at..cursor,
        text: at..text_end,
        ending,
        indent,
        cells,
        pipes,
    }
}

fn roles(row: &RowSpan) -> Roles {
    let first = row.cells.first().map(|cell| cell.start);
    let last_end = row.cells.last().map(|cell| cell.end);
    let mut leading = None;
    let mut between = Vec::new();
    let mut trailing = None;
    for pipe in &row.pipes {
        if first.is_none_or(|start| pipe.end <= start) {
            if leading.is_none() {
                leading = Some(pipe.clone());
            } else {
                between.push(pipe.clone());
            }
        } else if last_end.is_some_and(|end| pipe.start >= end) {
            if let Some(previous) = trailing.replace(pipe.clone()) {
                between.push(previous);
            }
        } else {
            between.push(pipe.clone());
        }
    }
    debug_assert!(
        row.cells.is_empty() || between.len() + 1 == row.cells.len(),
        "table row has {} cells and {} interior pipes",
        row.cells.len(),
        between.len()
    );
    Roles {
        leading,
        between,
        trailing,
    }
}

fn columns(located: &Located) -> usize {
    located.rows.get(1).map(|row| row.cells.len()).unwrap_or(0)
}

fn body(located: &Located) -> &[RowSpan] {
    located.rows.get(2..).unwrap_or(&[])
}

pub(crate) fn plan(
    source: &str,
    tree: &Green,
    edit: &TableEdit,
) -> Result<Vec<TextEdit>, TableFault> {
    match edit {
        TableEdit::InsertRow { at, index } => {
            boundary(source, *at)?;
            let located = locate(tree, *at).ok_or(TableFault::Refused)?;
            plan_insert_row(source, &located, *index)
        }
        TableEdit::DeleteRow { at, index } => {
            boundary(source, *at)?;
            let located = locate(tree, *at).ok_or(TableFault::Refused)?;
            let rows = body(&located);
            let row = rows.get(*index).ok_or(TableFault::Refused)?;
            Ok(vec![TextEdit {
                start: row.range.start,
                end: row.range.end,
                replacement: String::new(),
            }])
        }
        TableEdit::InsertColumn { at, index } => {
            boundary(source, *at)?;
            let located = locate(tree, *at).ok_or(TableFault::Refused)?;
            plan_insert_column(source, &located, *index)
        }
        TableEdit::DeleteColumn { at, index } => {
            boundary(source, *at)?;
            let located = locate(tree, *at).ok_or(TableFault::Refused)?;
            plan_delete_column(&located, *index)
        }
        TableEdit::SetAlignment {
            at,
            index,
            alignment,
        } => {
            boundary(source, *at)?;
            let located = locate(tree, *at).ok_or(TableFault::Refused)?;
            plan_alignment(source, &located, *index, *alignment)
        }
        TableEdit::SetCell { start, end, text } => {
            boundary(source, *start)?;
            boundary(source, *end)?;
            if start > end {
                return Err(TableFault::Range);
            }
            plan_set_cell(source, tree, *start, *end, text)
        }
    }
}

fn plan_insert_row(
    source: &str,
    located: &Located,
    index: usize,
) -> Result<Vec<TextEdit>, TableFault> {
    let rows = body(located);
    if index > rows.len() {
        return Err(TableFault::Refused);
    }
    let template = template_row(located, index);
    let blank = blank_like(source, template);
    let newline = preferred_newline(source, &located.rows);
    let (start, replacement) = if index < rows.len() {
        (rows[index].range.start, format!("{blank}{newline}"))
    } else {
        let previous = rows.last().unwrap_or(&located.rows[1]);
        if previous.ending.is_empty() {
            (previous.range.end, format!("{newline}{blank}"))
        } else {
            (previous.range.end, format!("{blank}{newline}"))
        }
    };
    Ok(vec![TextEdit {
        start,
        end: start,
        replacement,
    }])
}

fn template_row(located: &Located, index: usize) -> &RowSpan {
    let header = &located.rows[0];
    let width = header.cells.len();
    let rows = body(located);
    if index > 0 {
        if let Some(previous) = rows.get(index - 1) {
            if previous.cells.len() == width {
                return previous;
            }
        }
    }
    if let Some(next) = rows.get(index) {
        if next.cells.len() == width {
            return next;
        }
    }
    header
}

fn blank_like(source: &str, row: &RowSpan) -> String {
    let roles = roles(row);
    let cells: Vec<String> = row
        .cells
        .iter()
        .map(|cell| empty_like(&source[cell.start..cell.end]))
        .collect();
    let mut out = String::new();
    out.push_str(&source[row.indent.start..row.indent.end]);
    // An empty cell only survives between pipes. A tight row with no outer
    // pipe gains the pipe it needs so the new line keeps the column count.
    let leading = roles.leading.is_some() || cells.iter().any(String::is_empty);
    let trailing =
        roles.trailing.is_some() || cells.last().is_some_and(|cell| cell.trim().is_empty());
    if leading {
        out.push('|');
    }
    for (index, cell) in cells.iter().enumerate() {
        if index > 0 {
            out.push('|');
        }
        out.push_str(cell);
    }
    if trailing {
        out.push('|');
    }
    out
}

fn plan_insert_column(
    source: &str,
    located: &Located,
    index: usize,
) -> Result<Vec<TextEdit>, TableFault> {
    let width = columns(located);
    if index > width {
        return Err(TableFault::Refused);
    }
    let mut edits = Vec::new();
    for (row_index, row) in located.rows.iter().enumerate() {
        let cell = if row_index == 1 {
            delimiter_cell(source, row, index)
        } else {
            content_cell(source, row, index)
        };
        if let Some(edit) = insert_cell(row, index, &cell) {
            edits.push(edit);
        }
    }
    Ok(edits)
}

fn content_cell(source: &str, row: &RowSpan, index: usize) -> String {
    let neighbor = if index < row.cells.len() {
        &source[row.cells[index].start..row.cells[index].end]
    } else {
        row.cells
            .last()
            .map(|cell| &source[cell.start..cell.end])
            .unwrap_or("")
    };
    empty_like(neighbor)
}

fn delimiter_cell(source: &str, row: &RowSpan, index: usize) -> String {
    let neighbor = if index < row.cells.len() {
        &source[row.cells[index].start..row.cells[index].end]
    } else {
        row.cells
            .last()
            .map(|cell| &source[cell.start..cell.end])
            .unwrap_or("---")
    };
    let (lead, trail) = padding(neighbor);
    let dashes = "-".repeat(dash_count(neighbor).max(1));
    format!("{lead}{dashes}{trail}")
}

fn insert_cell(row: &RowSpan, index: usize, cell: &str) -> Option<TextEdit> {
    if index < row.cells.len() {
        let at = row.cells[index].start;
        let roles = roles(row);
        let replacement = if index == 0 && roles.leading.is_none() && cell.is_empty() {
            format!("|{cell}|")
        } else {
            format!("{cell}|")
        };
        return Some(TextEdit {
            start: at,
            end: at,
            replacement,
        });
    }
    let missing = index - row.cells.len();
    let mut pieces = Vec::new();
    for _ in 0..missing {
        pieces.push(cell.to_string());
    }
    pieces.push(cell.to_string());
    Some(append_cells(row, &pieces))
}

fn append_cells(row: &RowSpan, cells: &[String]) -> TextEdit {
    let roles = roles(row);
    let mut replacement = String::new();
    for cell in cells {
        replacement.push('|');
        replacement.push_str(cell);
    }
    let closes =
        roles.trailing.is_none() && cells.last().is_some_and(|cell| cell.trim().is_empty());
    if closes {
        replacement.push('|');
    }
    let at = roles
        .trailing
        .as_ref()
        .map(|pipe| pipe.start)
        .unwrap_or(row.text.end);
    TextEdit {
        start: at,
        end: at,
        replacement,
    }
}

fn plan_delete_column(located: &Located, index: usize) -> Result<Vec<TextEdit>, TableFault> {
    let width = columns(located);
    if index >= width {
        return Err(TableFault::Refused);
    }
    let mut edits = Vec::new();
    for row in &located.rows {
        if let Some(edit) = delete_cell(row, index) {
            edits.push(edit);
        }
    }
    Ok(edits)
}

fn delete_cell(row: &RowSpan, index: usize) -> Option<TextEdit> {
    let cell = row.cells.get(index)?.clone();
    let roles = roles(row);
    let last = index + 1 == row.cells.len();
    let (start, end) = if !last {
        let pipe = roles.between.get(index)?.clone();
        (cell.start, pipe.end)
    } else if index > 0 {
        let pipe = roles.between.get(index - 1)?.clone();
        (pipe.start, cell.end)
    } else if let Some(leading) = roles.leading {
        (leading.start, cell.end)
    } else if let Some(trailing) = roles.trailing {
        (cell.start, trailing.end)
    } else {
        (cell.start, cell.end)
    };
    Some(TextEdit {
        start,
        end,
        replacement: String::new(),
    })
}

fn plan_alignment(
    source: &str,
    located: &Located,
    index: usize,
    alignment: Alignment,
) -> Result<Vec<TextEdit>, TableFault> {
    let delimiter = located.rows.get(1).ok_or(TableFault::Refused)?;
    let cell = delimiter.cells.get(index).ok_or(TableFault::Refused)?;
    let text = &source[cell.start..cell.end];
    let (inner_start, inner_end, marker) = rewrite_marker(text, alignment);
    Ok(vec![TextEdit {
        start: cell.start + inner_start,
        end: cell.start + inner_end,
        replacement: marker,
    }])
}

fn rewrite_marker(cell: &str, alignment: Alignment) -> (usize, usize, String) {
    let trimmed = cell.trim_matches([' ', '\t']);
    if trimmed.is_empty() {
        return (0, cell.len(), "---".to_string());
    }
    let start = cell.find(trimmed).unwrap_or(0);
    let end = start + trimmed.len();
    let dashes = "-".repeat(dash_count(cell).max(1));
    let marker = match alignment {
        Alignment::None => dashes,
        Alignment::Left => format!(":{dashes}"),
        Alignment::Right => format!("{dashes}:"),
        Alignment::Center => format!(":{dashes}:"),
    };
    (start, end, marker)
}

fn plan_set_cell(
    source: &str,
    tree: &Green,
    start: usize,
    end: usize,
    text: &str,
) -> Result<Vec<TextEdit>, TableFault> {
    if text.contains('\n') || text.contains('\r') {
        return Err(TableFault::Refused);
    }
    let cell = find_cell(tree, 0, start, end).ok_or(TableFault::Refused)?;
    let full = cell.start == start && cell.end == end;
    let raw = &source[cell.start..cell.end];
    let (pad_start, pad_end) = padding_bounds(raw);
    let content = (cell.start + pad_start)..(cell.start + pad_end);
    if !full && !(start == content.start && end == content.end) {
        return Err(TableFault::Refused);
    }
    let replacement = escape_cell(text);
    Ok(vec![TextEdit {
        start: content.start,
        end: content.end,
        replacement,
    }])
}

fn find_cell(node: &Green, at: usize, start: usize, end: usize) -> Option<Range<usize>> {
    let node_end = at + node.width();
    if node.kind() == SyntaxKind::TableCell && at <= start && end <= node_end {
        // Prefer the cell itself over a nested inline that happens to cover
        // the same span. Cells do not contain cells.
        return Some(at..node_end);
    }
    if start < at || end > node_end {
        return None;
    }
    let mut cursor = at;
    for child in node.children() {
        if let Some(found) = find_cell(child, cursor, start, end) {
            return Some(found);
        }
        cursor += child.width();
    }
    None
}

fn escape_cell(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = String::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 1 < bytes.len() && is_ascii_punct(bytes[i + 1]) {
            out.push('\\');
            out.push(bytes[i + 1] as char);
            i += 2;
            continue;
        }
        if bytes[i] == b'|' {
            out.push('\\');
            out.push('|');
            i += 1;
            continue;
        }
        let width = utf8_width(text, i);
        out.push_str(&text[i..i + width]);
        i += width;
    }
    out
}

fn empty_like(cell: &str) -> String {
    if cell.trim_matches([' ', '\t']).is_empty() {
        return cell.to_string();
    }
    let (lead, trail) = padding(cell);
    format!("{lead}{trail}")
}

fn padding(cell: &str) -> (String, String) {
    let (start, end) = padding_bounds(cell);
    (cell[..start].to_string(), cell[end..].to_string())
}

fn padding_bounds(cell: &str) -> (usize, usize) {
    let bytes = cell.as_bytes();
    let mut start = 0;
    while start < bytes.len() && is_ws(bytes[start]) {
        start += 1;
    }
    let mut end = bytes.len();
    while end > start && is_ws(bytes[end - 1]) {
        end -= 1;
    }
    if start == end {
        let mid = cell.len() / 2;
        return (mid, mid);
    }
    (start, end)
}

fn dash_count(cell: &str) -> usize {
    cell.trim_matches([' ', '\t'])
        .bytes()
        .filter(|byte| *byte == b'-')
        .count()
}

fn preferred_newline<'a>(source: &'a str, rows: &[RowSpan]) -> &'a str {
    rows.iter()
        .find(|row| !row.ending.is_empty())
        .map(|row| &source[row.ending.start..row.ending.end])
        .unwrap_or("\n")
}

fn boundary(bytes: &str, at: usize) -> Result<(), TableFault> {
    if at <= bytes.len() && bytes.is_char_boundary(at) {
        Ok(())
    } else {
        Err(TableFault::Range)
    }
}

/// Trimmed content span inside a cell's text, relative to `base`.
pub(crate) fn content_span(cell: &str, base: usize) -> Range<usize> {
    let (start, end) = padding_bounds(cell);
    (base + start)..(base + end)
}
