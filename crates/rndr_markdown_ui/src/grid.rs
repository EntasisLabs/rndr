//! Grid paint and editing for a GFM table.
//!
//! The concrete tree stays the source of the bytes. A cell change, a row or
//! column change, and an alignment change are the table commands, each of
//! which compiles to a splice of the cells it touches.

use std::ops::Range;

use rndr_core::*;
use rndr_markdown::{Alignment, Command, Session, TypedView};
use rndr_ui::menu::{MenuAt, MenuEntry, MenuItem, MenuPicked};
use rndr_ui::{Space, Token, UiTheme};

use crate::intent::{next_boundary, prev_boundary};
use crate::project::{GridCell, GridRow, Palette, Segment, TableGrid};

const COL_MIN: f32 = 64.0;
const COL_MAX: f32 = 280.0;

/// Colors and spacing taken from the theme tokens.
pub(crate) struct Chrome {
    pub border: Color,
    pub header: Color,
    pub body: Color,
    pub text: Color,
    pub muted: Color,
    pub link: Color,
    pub code: Color,
    /// Low-emphasis column and row handle fill. Not the header fill.
    pub grip: Color,
    pub pad_x: f32,
    pub pad_y: f32,
    /// Slim handle strip. It is not a second header row.
    pub handle: f32,
    pub border_w: f32,
}

impl Chrome {
    pub(crate) fn from_theme(theme: &UiTheme) -> Self {
        let mut code = theme.color(Token::ControlHover);
        code.a = 0.55;
        Self {
            border: theme.color(Token::StrongStroke),
            header: theme.color(Token::Raised),
            body: theme.color(Token::Content),
            text: theme.color(Token::Text),
            muted: theme.color(Token::Secondary),
            link: theme.color(Token::InfoText),
            code,
            grip: theme.color(Token::ControlHover),
            pad_x: theme.space(Space::X8),
            pad_y: theme.space(Space::X4),
            handle: theme.space(Space::X4).max(6.0),
            border_w: 1.0,
        }
    }
}

pub(crate) fn palette_of(chrome: &Chrome) -> Palette {
    Palette {
        text: chrome.text,
        muted: chrome.muted,
        link: chrome.link,
        code: chrome.code,
    }
}

pub(crate) fn measure_height(
    engine: &dyn TextEngine,
    grid: &TableGrid,
    width: f32,
    style: &TextStyle,
) -> f32 {
    let chrome = Chrome::from_theme(&UiTheme::default());
    height_for(engine, grid, width, style, &chrome, 12.0)
}

pub(crate) fn outer_pad(block: &crate::project::DisplayBlock) -> f32 {
    12.0 + f32::from(block.quote) * 14.0 + f32::from(block.depth.saturating_sub(1)) * 18.0
}

pub(crate) fn height_for(
    engine: &dyn TextEngine,
    grid: &TableGrid,
    width: f32,
    style: &TextStyle,
    chrome: &Chrome,
    pad: f32,
) -> f32 {
    let metrics = layout_of(engine, grid, (width - pad * 2.0).max(1.0), style, chrome);
    // Children sit inside the padding. The outer box has to contain them.
    metrics.height + pad * 2.0
}

struct GridMetrics {
    columns: Vec<f32>,
    rows: Vec<f32>,
    height: f32,
    delimiter: f32,
}

fn layout_of(
    engine: &dyn TextEngine,
    grid: &TableGrid,
    width: f32,
    style: &TextStyle,
    chrome: &Chrome,
) -> GridMetrics {
    let count = column_count(grid);
    let mut columns = vec![COL_MIN; count];
    let header_style = header_style(style);
    for (index, column) in columns.iter_mut().enumerate() {
        let mut natural = 0.0f32;
        for row in &grid.rows {
            let Some(cell) = row.cells.get(index) else {
                continue;
            };
            let style = if row.header { &header_style } else { style };
            let measured = engine
                .measure(&cell.text, style, &cell.spans, 100_000.0)
                .width;
            natural = natural.max(measured + chrome.pad_x * 2.0);
        }
        *column = natural.clamp(COL_MIN, COL_MAX);
    }
    let budget = (width - chrome.handle).max(1.0);
    fit_columns(&mut columns, budget);
    let mut rows = Vec::with_capacity(grid.rows.len());
    for row in &grid.rows {
        // A cell that holds the caret paints with the text origin, which is
        // taller than the theme's cell padding. Size every row for that.
        let inset_y = chrome.pad_y.max(TEXT_ORIGIN_Y);
        let height = if row.raw.is_some() {
            engine.line_height(style) + inset_y * 2.0
        } else {
            let mut height =
                engine.line_height(if row.header { &header_style } else { style }) + inset_y * 2.0;
            for (index, cell) in row.cells.iter().enumerate() {
                let inner =
                    (columns.get(index).copied().unwrap_or(COL_MIN) - chrome.pad_x * 2.0).max(1.0);
                let style = if row.header { &header_style } else { style };
                let measured = engine.measure(&cell.text, style, &cell.spans, inner);
                height = height.max(measured.height + inset_y * 2.0);
            }
            height
        };
        rows.push(height);
    }
    let delimiter = if grid.delimiter_raw.is_some() {
        engine.line_height(style) + chrome.pad_y * 2.0
    } else {
        0.0
    };
    let height = chrome.handle + rows.iter().sum::<f32>() + delimiter;
    GridMetrics {
        columns,
        rows,
        height,
        delimiter,
    }
}

fn header_style(style: &TextStyle) -> TextStyle {
    let mut style = style.clone();
    style.weight = FONT_WEIGHT_BOLD;
    style
}

fn column_count(grid: &TableGrid) -> usize {
    grid.rows
        .iter()
        .map(|row| row.cells.len())
        .max()
        .unwrap_or(0)
        .max(grid.alignments.len())
}

fn fit_columns(columns: &mut [f32], budget: f32) {
    if columns.is_empty() {
        return;
    }
    let sum: f32 = columns.iter().sum();
    if sum <= budget {
        return;
    }
    let floor = COL_MIN.min(budget / columns.len() as f32).max(1.0);
    let flexible: f32 = columns.iter().map(|width| (*width - floor).max(0.0)).sum();
    let overflow = sum - budget;
    if flexible <= 0.0 {
        for width in columns.iter_mut() {
            *width = floor;
        }
        return;
    }
    for width in columns.iter_mut() {
        let slack = (*width - floor).max(0.0);
        *width -= overflow * (slack / flexible);
        *width = width.max(floor);
    }
}

/// Where an arrow key should put the caret. `Leave` steps out of the table.
pub(crate) enum Motion {
    /// Caret and anchor, absolute.
    Inside(usize, usize),
    /// The caret leaves through the top or the bottom of the table.
    Leave {
        /// Toward the following block.
        forward: bool,
    },
}

pub(crate) fn move_key(
    block: &crate::project::DisplayBlock,
    head: usize,
    anchor: usize,
    key: NamedKey,
    shift: bool,
) -> Option<Motion> {
    let grid = block.grid.as_ref()?;
    let base = block.span.start;
    let place = cell_at(grid, head);
    let motion = match key {
        NamedKey::ArrowLeft => step_horizontal(grid, base, head, anchor, place, false, shift),
        NamedKey::ArrowRight => step_horizontal(grid, base, head, anchor, place, true, shift),
        NamedKey::ArrowUp => step_vertical(grid, base, head, anchor, place, false, shift),
        NamedKey::ArrowDown => step_vertical(grid, base, head, anchor, place, true, shift),
        NamedKey::Home => home_end(grid, head, anchor, place, shift, true),
        NamedKey::End => home_end(grid, head, anchor, place, shift, false),
        _ => return None,
    };
    Some(motion)
}

fn home_end(
    grid: &TableGrid,
    head: usize,
    anchor: usize,
    place: Option<(usize, usize)>,
    shift: bool,
    home: bool,
) -> Motion {
    let Some((row, col)) = place else {
        return held(head, anchor, head, shift);
    };
    let Some(cell) = grid.rows.get(row).and_then(|row| row.cells.get(col)) else {
        return held(head, anchor, head, shift);
    };
    let next = if home {
        cell.content.start
    } else {
        cell.content.end
    };
    held(head, anchor, next, shift)
}

fn held(_head: usize, anchor: usize, next: usize, shift: bool) -> Motion {
    if shift {
        Motion::Inside(anchor, next)
    } else {
        Motion::Inside(next, next)
    }
}

fn step_horizontal(
    grid: &TableGrid,
    base: usize,
    head: usize,
    anchor: usize,
    place: Option<(usize, usize)>,
    forward: bool,
    shift: bool,
) -> Motion {
    if let Some((row, col)) = place {
        if let Some(cell) = grid.rows.get(row).and_then(|row| row.cells.get(col)) {
            if let Some(next) = step_display(cell, base, head, forward) {
                return held(head, anchor, next, shift);
            }
            if shift {
                return held(head, anchor, head, shift);
            }
            if let Some(next) = adjacent(grid, row, col, forward) {
                let cell = &grid.rows[next.0].cells[next.1];
                let at = if forward {
                    cell.content.start
                } else {
                    cell.content.end
                };
                return Motion::Inside(at, at);
            }
            return Motion::Leave { forward };
        }
    }
    if shift {
        return held(head, anchor, head, shift);
    }
    if let Some((row, col)) = nearest(grid, head, forward) {
        let cell = &grid.rows[row].cells[col];
        let at = if forward {
            cell.content.start
        } else {
            cell.content.end
        };
        Motion::Inside(at, at)
    } else {
        Motion::Leave { forward }
    }
}

fn step_vertical(
    grid: &TableGrid,
    base: usize,
    head: usize,
    anchor: usize,
    place: Option<(usize, usize)>,
    forward: bool,
    shift: bool,
) -> Motion {
    let Some((row, col)) = place.or_else(|| nearest(grid, head, forward)) else {
        return Motion::Leave { forward };
    };
    let next_row = if forward {
        row + 1
    } else {
        row.wrapping_sub(1)
    };
    if !forward && row == 0 || forward && next_row >= grid.rows.len() {
        if shift {
            return held(head, anchor, head, shift);
        }
        return Motion::Leave { forward };
    }
    if shift {
        // A selection stays inside the cell. Crossing rows is an unshifted move.
        return held(head, anchor, head, shift);
    }
    let Some(target_col) = clamp_col(grid, next_row, col) else {
        return Motion::Leave { forward };
    };
    let from = grid.rows[row].cells.get(col);
    let display = from
        .map(|cell| cell.source_to_display(head, base))
        .unwrap_or(0);
    let cell = &grid.rows[next_row].cells[target_col];
    let at = cell.display_to_source(display.min(cell.text.len()), base);
    Motion::Inside(at, at)
}

fn step_display(cell: &GridCell, base: usize, head: usize, forward: bool) -> Option<usize> {
    let display = cell.source_to_display(head, base);
    let next = if forward {
        next_boundary(&cell.text, display)
    } else {
        prev_boundary(&cell.text, display)
    };
    if next == display {
        None
    } else {
        Some(cell.display_to_source(next, base))
    }
}

fn adjacent(grid: &TableGrid, row: usize, col: usize, forward: bool) -> Option<(usize, usize)> {
    if forward {
        if col + 1 < grid.rows.get(row)?.cells.len() {
            return Some((row, col + 1));
        }
        let next = row + 1;
        if next < grid.rows.len() && !grid.rows[next].cells.is_empty() {
            return Some((next, 0));
        }
        None
    } else if col > 0 {
        Some((row, col - 1))
    } else if row > 0 {
        let prev = row - 1;
        let last = grid.rows.get(prev)?.cells.len().checked_sub(1)?;
        Some((prev, last))
    } else {
        None
    }
}

fn nearest(grid: &TableGrid, source: usize, forward: bool) -> Option<(usize, usize)> {
    let mut best: Option<(usize, usize)> = None;
    for (row, grow) in grid.rows.iter().enumerate() {
        for (col, cell) in grow.cells.iter().enumerate() {
            let at = if forward {
                cell.content.start
            } else {
                cell.content.end
            };
            let ok = if forward { at >= source } else { at <= source };
            if ok {
                best = Some((row, col));
                if forward {
                    return best;
                }
            }
        }
    }
    best
}

fn clamp_col(grid: &TableGrid, row: usize, col: usize) -> Option<usize> {
    let len = grid.rows.get(row)?.cells.len();
    if len == 0 {
        None
    } else {
        Some(col.min(len - 1))
    }
}

/// Tab stays inside the grid. The last cell asks for a new body row.
pub(crate) enum Tab {
    Move(usize),
    Append { at: usize, index: usize },
}

pub(crate) fn tab(block: &crate::project::DisplayBlock, head: usize, shift: bool) -> Option<Tab> {
    let grid = block.grid.as_ref()?;
    let place = cell_at(grid, head).or_else(|| nearest(grid, head, !shift));
    let Some((row, col)) = place else {
        return Some(Tab::Move(head));
    };
    if let Some((nrow, ncol)) = adjacent(grid, row, col, !shift) {
        let cell = &grid.rows[nrow].cells[ncol];
        let at = if shift {
            cell.content.end
        } else {
            cell.content.start
        };
        return Some(Tab::Move(at));
    }
    if shift {
        return Some(Tab::Move(head));
    }
    let at = grid
        .rows
        .first()
        .map(|row| row.range.start)
        .unwrap_or(block.span.start);
    Some(Tab::Append {
        at,
        index: grid.rows.len().saturating_sub(1),
    })
}

/// Enter moves to the cell below, and stays put on the last row.
pub(crate) fn enter(block: &crate::project::DisplayBlock, head: usize) -> Option<usize> {
    let grid = block.grid.as_ref()?;
    let place = cell_at(grid, head).or_else(|| nearest(grid, head, true));
    let Some((row, col)) = place else {
        return Some(head);
    };
    if row + 1 >= grid.rows.len() {
        return Some(head);
    }
    let Some(target) = clamp_col(grid, row + 1, col) else {
        return Some(head);
    };
    Some(grid.rows[row + 1].cells[target].content.start)
}

/// One cell replacement, already ordered so later ranges come first.
pub(crate) struct Planned {
    pub command: Command,
    pub caret: usize,
}

pub(crate) fn plan_type(
    block: &crate::project::DisplayBlock,
    bytes: &str,
    anchor: usize,
    head: usize,
    text: &str,
) -> Option<Vec<Planned>> {
    let grid = block.grid.as_ref()?;
    let text = first_line(text);
    if text.is_empty() && anchor == head {
        return Some(Vec::new());
    }
    Some(plan_replace(
        grid,
        block.span.start,
        bytes,
        anchor,
        head,
        &text,
    ))
}

pub(crate) fn plan_delete(
    block: &crate::project::DisplayBlock,
    bytes: &str,
    anchor: usize,
    head: usize,
    back: bool,
) -> Option<Vec<Planned>> {
    let grid = block.grid.as_ref()?;
    let base = block.span.start;
    if anchor != head {
        return Some(plan_replace(grid, base, bytes, anchor, head, ""));
    }
    let Some((row, col)) = cell_at(grid, head) else {
        return Some(Vec::new());
    };
    let cell = &grid.rows[row].cells[col];
    let display = cell.source_to_display(head, base);
    let next = if back {
        prev_boundary(&cell.text, display)
    } else {
        next_boundary(&cell.text, display)
    };
    if next == display {
        return Some(Vec::new());
    }
    let other = cell.display_to_source(next, base);
    Some(plan_replace(grid, base, bytes, head, other, ""))
}

fn plan_replace(
    grid: &TableGrid,
    base: usize,
    bytes: &str,
    anchor: usize,
    head: usize,
    insert: &str,
) -> Vec<Planned> {
    let start = anchor.min(head);
    let end = anchor.max(head);
    let mut touched = Vec::new();
    for (row, grow) in grid.rows.iter().enumerate() {
        for (col, cell) in grow.cells.iter().enumerate() {
            if cell_overlaps(cell, start, end) || (start == end && cell_holds(cell, head)) {
                touched.push((row, col));
            }
        }
    }
    if touched.is_empty() {
        if let Some((row, col)) = nearest(grid, head, true).or_else(|| nearest(grid, head, false)) {
            touched.push((row, col));
        }
    }
    if start != end {
        touched.retain(|(row, col)| {
            let cell = &grid.rows[*row].cells[*col];
            cell.content.end > start && cell.content.start < end
        });
    } else if touched.len() > 1 {
        touched.retain(|(row, col)| cell_holds(&grid.rows[*row].cells[*col], head));
    }
    let mut plans = Vec::new();
    for (row, col) in touched {
        let cell = &grid.rows[row].cells[col];
        let raw = bytes.get(cell.content.clone()).unwrap_or("");
        let (lo, hi) = if start == end {
            let at = snap_source(cell, base, head);
            (at, at)
        } else {
            let lo = snap_source(cell, base, start.max(cell.content.start));
            let hi = snap_source(cell, base, end.min(cell.content.end));
            (lo.min(hi), lo.max(hi))
        };
        let lo = lo.clamp(cell.content.start, cell.content.end);
        let hi = hi.clamp(cell.content.start, cell.content.end);
        let rel_lo = lo - cell.content.start;
        let rel_hi = hi - cell.content.start;
        if !raw.is_char_boundary(rel_lo) || !raw.is_char_boundary(rel_hi) {
            continue;
        }
        let piece = if start == end || cell_holds(cell, head) {
            insert
        } else {
            ""
        };
        let mut next = String::new();
        next.push_str(&raw[..rel_lo]);
        next.push_str(piece);
        next.push_str(&raw[rel_hi..]);
        if next == raw {
            continue;
        }
        let caret_in = rel_lo + piece.len();
        let caret = cell.content.start + escape_cell(&next[..caret_in]).len();
        plans.push(Planned {
            command: Command::SetTableCell {
                start: cell.content.start,
                end: cell.content.end,
                text: next,
            },
            caret,
        });
    }
    plans.sort_by_key(|plan| std::cmp::Reverse(plan_start(&plan.command)));
    plans
}

fn plan_start(command: &Command) -> usize {
    match command {
        Command::SetTableCell { start, .. } => *start,
        _ => 0,
    }
}

fn cell_overlaps(cell: &GridCell, start: usize, end: usize) -> bool {
    cell.content.end > start && cell.content.start < end
}

fn snap_source(cell: &GridCell, base: usize, source: usize) -> usize {
    let display = cell.source_to_display(source, base);
    cell.display_to_source(display, base)
}

fn first_line(text: &str) -> String {
    text.lines().next().unwrap_or("").to_string()
}

pub(crate) fn apply_plans(session: &mut Session, plans: &[Planned], now: u64) -> bool {
    if plans.is_empty() {
        return true;
    }
    let mut caret = plans.last().map(|plan| plan.caret).unwrap_or(0);
    let mut changed = false;
    for plan in plans {
        let revision = session.revision();
        if session.edit(plan.command.clone(), now).is_err() {
            return changed;
        }
        if session.revision() != revision {
            changed = true;
            caret = plan.caret.min(session.bytes().len());
        }
    }
    if changed {
        // Plans were applied from the later offsets backward, so the earliest
        // plan's caret is still the right byte.
        let caret = plans.last().map(|plan| plan.caret).unwrap_or(caret);
        let caret = caret.min(session.bytes().len());
        session.note_caret(caret, caret);
    }
    changed
}

pub(crate) fn caret_after_append(session: &Session, at: usize, index: usize) -> Option<usize> {
    let view = TypedView::new(session.bytes(), session.tree());
    let table = view
        .tables()
        .into_iter()
        .find(|table| table.range.start <= at && at < table.range.end)?;
    let row = table.body.get(index)?;
    Some(
        row.cells
            .first()
            .map(|cell| cell.content.start)
            .unwrap_or(row.range.start),
    )
}

fn cell_at(grid: &TableGrid, source: usize) -> Option<(usize, usize)> {
    for (row, grow) in grid.rows.iter().enumerate() {
        for (col, cell) in grow.cells.iter().enumerate() {
            if cell_holds(cell, source) {
                return Some((row, col));
            }
        }
    }
    None
}

fn cell_holds(cell: &GridCell, source: usize) -> bool {
    if cell.range.start <= source && source < cell.range.end {
        return true;
    }
    if source == cell.content.end && source == cell.range.end {
        return true;
    }
    cell.content.start == cell.content.end
        && source == cell.content.start
        && cell.range.start <= source
        && source <= cell.range.end
}

/// Clamp a raw-byte word motion so it stops at the cell content.
pub(crate) fn clamp_word(block: &crate::project::DisplayBlock, head: usize, next: usize) -> usize {
    let Some(grid) = block.grid.as_ref() else {
        return next;
    };
    let Some((row, col)) = cell_at(grid, head) else {
        return next;
    };
    let Some(cell) = grid.rows.get(row).and_then(|row| row.cells.get(col)) else {
        return next;
    };
    next.clamp(cell.content.start, cell.content.end)
}

#[derive(Clone)]
struct Hit {
    block_start: usize,
    row: usize,
    col: usize,
    content_start: usize,
    content_end: usize,
    text: String,
    segments: Vec<Segment>,
    inset_x: f32,
    inset_y: f32,
}

impl Component for Hit {}

/// The table entity. Handle visibility is decided against this span.
#[derive(Clone, Copy)]
pub(crate) struct GridFrame {
    /// Source start of the table block.
    pub start: usize,
    /// Source end of the table block.
    pub end: usize,
}

impl Component for GridFrame {}

#[derive(Clone)]
struct Handle {
    row: Option<usize>,
    col: Option<usize>,
    block_start: usize,
}

impl Component for Handle {}

struct MenuState {
    menu: Entity,
    document: crate::document::MarkdownId,
    table_at: usize,
    row: usize,
    col: usize,
}

impl Component for MenuState {}

pub(crate) struct Paint<'a> {
    pub block: &'a crate::project::DisplayBlock,
    pub bytes: &'a str,
    pub caret: usize,
    pub anchor: usize,
    pub preedit: &'a str,
    pub width: f32,
    pub chrome: &'a Chrome,
}

struct Frame<'a> {
    block: &'a crate::project::DisplayBlock,
    bytes: &'a str,
    caret: usize,
    anchor: usize,
    preedit: &'a str,
    chrome: &'a Chrome,
    style: &'a TextStyle,
    metrics: &'a GridMetrics,
}

struct CellGeom {
    row: usize,
    col: usize,
    header: bool,
    width: f32,
    height: f32,
}

pub(crate) fn spawn(world: &mut World, parent: Entity, paint: Paint<'_>) -> Entity {
    let block = paint.block;
    let grid = block.grid.as_ref();
    let pad = outer_pad(block);
    let style = TextStyle {
        size: 16.0,
        ..TextStyle::default()
    };
    let metrics = grid.map(|grid| {
        layout_of(
            text_engine(world).as_ref(),
            grid,
            (paint.width - pad * 2.0).max(1.0),
            &style,
            paint.chrome,
        )
    });
    let height = metrics
        .as_ref()
        .map(|metrics| metrics.height + pad * 2.0)
        .unwrap_or(style.size);
    let entity = world.spawn((Layout {
        width: Size::Fill,
        height: Size::Px(height + 8.0),
        padding: pad,
        ..Layout::column()
    },));
    set_parent(world, entity, parent);
    world.insert(
        entity,
        GridFrame {
            start: block.span.start,
            end: block.span.end,
        },
    );
    let Some(grid) = grid else {
        return entity;
    };
    let Some(metrics) = metrics else {
        return entity;
    };
    let frame = Frame {
        block,
        bytes: paint.bytes,
        caret: paint.caret,
        anchor: paint.anchor,
        preedit: paint.preedit,
        chrome: paint.chrome,
        style: &style,
        metrics: &metrics,
    };
    spawn_column_handles(world, entity, &frame, grid);
    for (index, row) in grid.rows.iter().enumerate() {
        spawn_row(world, entity, &frame, row, index);
        if index == 0 {
            if let Some(raw) = grid.delimiter_raw.clone() {
                spawn_raw_line(
                    world,
                    entity,
                    &frame,
                    raw,
                    0,
                    metrics.columns.iter().sum::<f32>().max(1.0),
                    metrics.delimiter,
                );
            }
        }
    }
    entity
}

fn spawn_column_handles(world: &mut World, parent: Entity, frame: &Frame<'_>, grid: &TableGrid) {
    let block = frame.block;
    let metrics = frame.metrics;
    let chrome = frame.chrome;
    let row = world.spawn((Layout {
        width: Size::Fill,
        height: Size::Px(chrome.handle),
        ..Layout::row()
    },));
    set_parent(world, row, parent);
    let corner = world.spawn((Layout {
        width: Size::Px(chrome.handle),
        height: Size::Px(chrome.handle),
        ..Layout::column()
    },));
    set_parent(world, corner, row);
    let count = column_count(grid).max(metrics.columns.len());
    for col in 0..count {
        let width = metrics.columns.get(col).copied().unwrap_or(COL_MIN);
        let handle = world.spawn((
            Layout {
                width: Size::Px(width),
                height: Size::Px(chrome.handle),
                ..Layout::column()
            },
            Color::rgba(0.0, 0.0, 0.0, 0.0),
            ContextTarget,
            Handle {
                row: None,
                col: Some(col),
                block_start: block.span.start,
            },
        ));
        set_parent(world, handle, row);
    }
}

fn spawn_row(world: &mut World, parent: Entity, frame: &Frame<'_>, row: &GridRow, index: usize) {
    let block = frame.block;
    let metrics = frame.metrics;
    let chrome = frame.chrome;
    let style = frame.style;
    let height = metrics.rows.get(index).copied().unwrap_or(style.size);
    let line = world.spawn((Layout {
        width: Size::Fill,
        height: Size::Px(height),
        ..Layout::row()
    },));
    set_parent(world, line, parent);
    let handle = world.spawn((
        Layout {
            width: Size::Px(chrome.handle),
            height: Size::Px(height),
            ..Layout::column()
        },
        Color::rgba(0.0, 0.0, 0.0, 0.0),
        ContextTarget,
        Handle {
            row: Some(index),
            col: None,
            block_start: block.span.start,
        },
    ));
    set_parent(world, handle, line);
    if let Some(raw) = row.raw.clone() {
        let width = metrics.columns.iter().sum::<f32>().max(1.0);
        spawn_raw_line(world, line, frame, raw, index, width, height);
        return;
    }
    let columns = metrics.columns.len();
    let missing = columns.saturating_sub(row.cells.len());
    for (col, cell) in row.cells.iter().enumerate() {
        let width = metrics.columns.get(col).copied().unwrap_or(COL_MIN);
        spawn_cell(
            world,
            line,
            frame,
            cell,
            CellGeom {
                row: index,
                col,
                header: row.header,
                width,
                height,
            },
        );
    }
    for extra in 0..missing {
        let width = metrics
            .columns
            .get(row.cells.len() + extra)
            .copied()
            .unwrap_or(COL_MIN);
        let filler = world.spawn((
            Layout {
                width: Size::Px(width),
                height: Size::Px(height),
                ..Layout::column()
            },
            if row.header {
                chrome.header
            } else {
                chrome.body
            },
            Border {
                color: chrome.border,
                width: chrome.border_w,
            },
        ));
        set_parent(world, filler, line);
    }
}

fn spawn_cell(
    world: &mut World,
    parent: Entity,
    frame: &Frame<'_>,
    cell: &GridCell,
    geom: CellGeom,
) {
    let block = frame.block;
    let chrome = frame.chrome;
    let caret = frame.caret;
    let anchor = frame.anchor;
    let preedit = frame.preedit;
    let style = frame.style;
    let CellGeom {
        row,
        col,
        header,
        width,
        height,
    } = geom;
    let align = block
        .grid
        .as_ref()
        .and_then(|grid| grid.alignments.get(col).copied())
        .map(align_of)
        .unwrap_or(TextAlign::Start);
    let mut style = style.clone();
    style.align = align;
    if header {
        style.weight = FONT_WEIGHT_BOLD;
    }
    let mut label = Label::new(&cell.text).with_color(chrome.text);
    label.overflow = TextOverflow::Wrap;
    let hit = Hit {
        block_start: block.span.start,
        row,
        col,
        content_start: cell.content.start,
        content_end: cell.content.end,
        text: cell.text.clone(),
        segments: cell.segments.clone(),
        inset_x: chrome.pad_x,
        inset_y: chrome.pad_y,
    };
    let entity = world.spawn((
        Layout {
            width: Size::Px(width),
            height: Size::Px(height),
            ..Layout::column()
        },
        label,
        Spans {
            runs: cell.spans.clone(),
        },
        style,
        LabelBox {
            inset_x: chrome.pad_x,
            inset_y: chrome.pad_y,
            min_height: 0.0,
        },
        if header { chrome.header } else { chrome.body },
    ));
    world.insert(
        entity,
        Border {
            color: chrome.border,
            width: chrome.border_w,
        },
    );
    world.insert(entity, ContextTarget);
    world.insert(entity, hit.clone());
    paint_caret(
        world,
        entity,
        &Hit {
            block_start: block.span.start,
            row,
            col,
            content_start: cell.content.start,
            content_end: cell.content.end,
            text: cell.text.clone(),
            segments: cell.segments.clone(),
            inset_x: chrome.pad_x,
            inset_y: chrome.pad_y,
        },
        caret,
        anchor,
        preedit,
    );
    set_parent(world, entity, parent);
}

fn spawn_raw_line(
    world: &mut World,
    parent: Entity,
    frame: &Frame<'_>,
    raw: Range<usize>,
    row: usize,
    width: f32,
    height: f32,
) {
    let block = frame.block;
    let bytes = frame.bytes;
    let chrome = frame.chrome;
    let caret = frame.caret;
    let anchor = frame.anchor;
    let text = bytes.get(raw.clone()).unwrap_or("").to_string();
    let segments = vec![Segment {
        display_start: 0,
        display_end: text.len(),
        source_start: raw.start.saturating_sub(block.span.start),
        source_end: raw.end.saturating_sub(block.span.start),
    }];
    let mut style = TextStyle {
        family: "monospace".into(),
        size: 14.0,
        ..TextStyle::default()
    };
    style.align = TextAlign::Start;
    let mut label = Label::new(&text).with_color(chrome.muted);
    label.overflow = TextOverflow::Wrap;
    let hit = Hit {
        block_start: block.span.start,
        row,
        col: 0,
        content_start: raw.start,
        content_end: raw.end,
        text: text.clone(),
        segments: segments.clone(),
        inset_x: chrome.pad_x,
        inset_y: chrome.pad_y,
    };
    let entity = world.spawn((
        Layout {
            width: Size::Px(width),
            height: Size::Px(height.max(1.0)),
            ..Layout::column()
        },
        label,
        style,
        LabelBox {
            inset_x: chrome.pad_x,
            inset_y: chrome.pad_y,
            min_height: 0.0,
        },
        chrome.body,
        Border {
            color: chrome.border,
            width: chrome.border_w,
        },
    ));
    world.insert(entity, ContextTarget);
    world.insert(entity, hit.clone());
    paint_caret(world, entity, &hit, caret, anchor, "");
    set_parent(world, entity, parent);
}

fn align_of(alignment: Alignment) -> TextAlign {
    match alignment {
        Alignment::Center => TextAlign::Center,
        Alignment::Right => TextAlign::End,
        Alignment::Left | Alignment::None => TextAlign::Start,
    }
}

fn paint_caret(
    world: &mut World,
    entity: Entity,
    hit: &Hit,
    caret: usize,
    anchor: usize,
    preedit: &str,
) {
    let holds = caret == hit.content_end
        || (hit.content_start == hit.content_end && caret == hit.content_start)
        || (hit.content_start <= caret && caret < hit.content_end)
        || (caret == hit.content_end);
    if holds && caret >= hit.content_start && caret <= hit.content_end.max(hit.content_start) {
        let index = display_index(hit, caret);
        world.insert(
            entity,
            Caret {
                index,
                anchor: display_index(
                    hit,
                    anchor.clamp(hit.content_start, hit.content_end.max(hit.content_start)),
                ),
            },
        );
        if !preedit.is_empty() {
            world.insert(
                entity,
                ImePreview {
                    text: preedit.to_string(),
                    cursor: Some((preedit.len(), preedit.len())),
                },
            );
        }
    }
    if anchor != caret {
        let start = anchor.min(caret);
        let end = anchor.max(caret);
        if end > hit.content_start && start < hit.content_end.max(hit.content_start) {
            let lo = start.max(hit.content_start);
            let hi = end.min(hit.content_end.max(hit.content_start));
            world.insert(
                entity,
                TextSelection {
                    anchor: display_index(hit, lo),
                    head: display_index(hit, hi),
                },
            );
        }
    }
}

fn display_index(hit: &Hit, source: usize) -> usize {
    if hit.text.is_empty() {
        return 0;
    }
    let rel = source.saturating_sub(hit.block_start);
    let end = hit.content_end.saturating_sub(hit.block_start);
    if rel >= end {
        return hit.text.len();
    }
    for seg in &hit.segments {
        if rel < seg.source_start {
            return seg.display_start;
        }
        if rel <= seg.source_end {
            let shown = seg.display_end - seg.display_start;
            let src = seg.source_end - seg.source_start;
            if shown == src {
                return seg.display_start + (rel - seg.source_start);
            }
            if rel == seg.source_end {
                return seg.display_end;
            }
            return seg.display_start;
        }
    }
    hit.text.len()
}

fn source_index(hit: &Hit, display: usize) -> usize {
    if hit.text.is_empty() {
        return hit.content_start;
    }
    let display = display.min(hit.text.len());
    let fallback = hit.content_end.saturating_sub(hit.block_start);
    for seg in &hit.segments {
        if display < seg.display_start {
            return hit.block_start + seg.source_start;
        }
        if display <= seg.display_end {
            let shown = seg.display_end - seg.display_start;
            let src = seg.source_end - seg.source_start;
            if shown == src {
                return hit.block_start + seg.source_start + (display - seg.display_start);
            }
            if display == seg.display_end {
                return hit.block_start + seg.source_end;
            }
            return hit.block_start + seg.source_start;
        }
    }
    hit.block_start + fallback
}

pub(crate) fn source_at(world: &World, hit: Entity, at: [f32; 2]) -> Option<usize> {
    let entity = find_hit(world, hit)?;
    let cell = world.get::<Hit>(entity)?;
    let rect = world.get::<Rect>(entity).copied()?;
    let style = world
        .get::<TextStyle>(entity)
        .cloned()
        .unwrap_or_else(TextStyle::default);
    let label = world.get::<Label>(entity)?;
    let spans = world
        .get::<Spans>(entity)
        .map(|spans| spans.runs.clone())
        .unwrap_or_default();
    let inner = (rect.w - cell.inset_x * 2.0).max(1.0);
    let layout = text_engine(world).layout(&label.text, &style, &spans, inner);
    let index = layout.index_at(at[0] - rect.x - cell.inset_x, at[1] - rect.y - cell.inset_y);
    Some(source_index(cell, index))
}

fn find_hit(world: &World, mut entity: Entity) -> Option<Entity> {
    for _ in 0..48 {
        if world.has::<Hit>(entity) {
            return Some(entity);
        }
        entity = world.get::<Parent>(entity)?.entity;
    }
    None
}

pub(crate) fn is_handle(world: &World, hit: Entity) -> bool {
    let mut entity = hit;
    for _ in 0..48 {
        if world.has::<Handle>(entity) {
            return true;
        }
        let Some(parent) = world.get::<Parent>(entity) else {
            return false;
        };
        entity = parent.entity;
    }
    false
}

fn handle_at(world: &World, hit: Entity) -> Option<(Handle, Entity)> {
    let mut entity = hit;
    for _ in 0..48 {
        if let Some(handle) = world.get::<Handle>(entity).cloned() {
            return Some((handle, entity));
        }
        entity = world.get::<Parent>(entity)?.entity;
    }
    None
}

pub(crate) fn open_for(
    world: &mut World,
    root: Entity,
    document: crate::document::MarkdownId,
    handle_or_hit: Entity,
    at: [f32; 2],
) -> bool {
    if let Some((handle, _)) = handle_at(world, handle_or_hit) {
        let row = handle.row.unwrap_or(0);
        let col = handle.col.unwrap_or(0);
        return open_menu_at(world, root, document, handle.block_start, row, col, at);
    }
    let Some(entity) = find_hit(world, handle_or_hit) else {
        return false;
    };
    let Some(hit) = world.get::<Hit>(entity).cloned() else {
        return false;
    };
    open_menu_at(world, root, document, hit.block_start, hit.row, hit.col, at)
}

fn open_menu_at(
    world: &mut World,
    root: Entity,
    document: crate::document::MarkdownId,
    table_at: usize,
    row: usize,
    col: usize,
    at: [f32; 2],
) -> bool {
    let alignment = current_alignment(world, document, table_at, col);
    let entries = menu_entries(row, alignment);
    let Ok(menu) = rndr_ui::menu::open_menu(world, root, MenuAt::Point(at), &entries) else {
        return false;
    };
    world.insert(
        root,
        MenuState {
            menu: menu.root,
            document,
            table_at,
            row,
            col,
        },
    );
    true
}

fn current_alignment(
    world: &World,
    document: crate::document::MarkdownId,
    table_at: usize,
    col: usize,
) -> Option<Alignment> {
    let session = world
        .resource::<crate::document::MarkdownDocuments>()?
        .get(document)?;
    let view = TypedView::new(session.bytes(), session.tree());
    view.tables()
        .into_iter()
        .find(|table| table.range.start <= table_at && table_at < table.range.end)
        .and_then(|table| table.alignments.get(col).copied())
}

fn menu_entries(row: usize, alignment: Option<Alignment>) -> Vec<MenuEntry> {
    let header = row == 0;
    vec![
        MenuEntry::Group("Row".into()),
        MenuEntry::Item(MenuItem::new(1, "Insert row above").enabled(!header)),
        MenuEntry::Item(MenuItem::new(2, "Insert row below")),
        MenuEntry::Item(MenuItem::new(3, "Delete row").enabled(!header).danger()),
        MenuEntry::Separator,
        MenuEntry::Group("Column".into()),
        MenuEntry::Item(MenuItem::new(4, "Insert column left")),
        MenuEntry::Item(MenuItem::new(5, "Insert column right")),
        MenuEntry::Item(MenuItem::new(6, "Delete column").danger()),
        MenuEntry::Separator,
        MenuEntry::Group("Alignment".into()),
        MenuEntry::Item(MenuItem::new(7, "Align left").checked(alignment == Some(Alignment::Left))),
        MenuEntry::Item(
            MenuItem::new(8, "Align center").checked(alignment == Some(Alignment::Center)),
        ),
        MenuEntry::Item(
            MenuItem::new(9, "Align right").checked(alignment == Some(Alignment::Right)),
        ),
        MenuEntry::Item(
            MenuItem::new(10, "No alignment").checked(alignment == Some(Alignment::None)),
        ),
    ]
}

pub(crate) fn take_menu(world: &mut World, root: Entity, now: u64) {
    let picks: Vec<MenuPicked> = events::<MenuPicked>(world).to_vec();
    let Some(state) = world.get::<MenuState>(root).map(|state| MenuState {
        menu: state.menu,
        document: state.document,
        table_at: state.table_at,
        row: state.row,
        col: state.col,
    }) else {
        return;
    };
    let Some(pick) = picks.into_iter().find(|pick| pick.menu == state.menu) else {
        return;
    };
    world.remove::<MenuState>(root);
    let Some(session) = world
        .resource_mut::<crate::document::MarkdownDocuments>()
        .and_then(|docs| docs.get_mut(state.document))
    else {
        return;
    };
    if let Some(caret) = run_menu(
        session,
        state.table_at,
        state.row,
        state.col,
        pick.item,
        now,
    ) {
        session.note_caret(caret, caret);
    }
}

fn run_menu(
    session: &mut Session,
    table_at: usize,
    row: usize,
    col: usize,
    item: u64,
    now: u64,
) -> Option<usize> {
    let at = table_at;
    let command = match item {
        1 if row > 0 => Command::InsertTableRow { at, index: row - 1 },
        2 => Command::InsertTableRow { at, index: row },
        3 if row > 0 => Command::DeleteTableRow { at, index: row - 1 },
        4 => Command::InsertTableColumn { at, index: col },
        5 => Command::InsertTableColumn { at, index: col + 1 },
        6 => Command::DeleteTableColumn { at, index: col },
        7 => Command::SetTableAlignment {
            at,
            index: col,
            alignment: Alignment::Left,
        },
        8 => Command::SetTableAlignment {
            at,
            index: col,
            alignment: Alignment::Center,
        },
        9 => Command::SetTableAlignment {
            at,
            index: col,
            alignment: Alignment::Right,
        },
        10 => Command::SetTableAlignment {
            at,
            index: col,
            alignment: Alignment::None,
        },
        _ => return None,
    };
    let revision = session.revision();
    session.edit(command, now).ok()?;
    if session.revision() == revision {
        return Some(session.caret());
    }
    let view = TypedView::new(session.bytes(), session.tree());
    let Some(table) = view
        .tables()
        .into_iter()
        .find(|table| table.range.start <= at && at < table.range.end.max(at + 1))
    else {
        return Some(session.caret().min(session.bytes().len()));
    };
    let caret = match item {
        1 | 2 => {
            let index = if item == 1 { row - 1 } else { row };
            table
                .body
                .get(index)
                .and_then(|body| body.cells.first())
                .map(|cell| cell.content.start)
        }
        3 => {
            let body = row.saturating_sub(1);
            table
                .body
                .get(body)
                .or_else(|| table.body.last())
                .and_then(|body| body.cells.get(col).or_else(|| body.cells.first()))
                .map(|cell| cell.content.start)
                .or_else(|| table.header.cells.get(col).map(|cell| cell.content.start))
        }
        4 | 5 => {
            let index = if item == 4 { col } else { col + 1 };
            let cells = if row == 0 {
                &table.header.cells
            } else {
                table
                    .body
                    .get(row - 1)
                    .map(|body| &body.cells)
                    .unwrap_or(&table.header.cells)
            };
            cells.get(index).map(|cell| cell.content.start)
        }
        6 => {
            let cells = if row == 0 {
                &table.header.cells
            } else {
                table
                    .body
                    .get(row - 1)
                    .map(|body| &body.cells)
                    .unwrap_or(&table.header.cells)
            };
            let index = col.min(cells.len().saturating_sub(1));
            cells.get(index).map(|cell| cell.content.start)
        }
        _ => Some(session.caret()),
    };
    caret.or(Some(table.range.start))
}

/// `|` becomes `\|`. An existing escaped punctuation mark is left as written.
fn escape_cell(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = String::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 1 < bytes.len() && bytes[i + 1].is_ascii_punctuation() {
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
        let width = text[i..].chars().next().map(char::len_utf8).unwrap_or(1);
        out.push_str(&text[i..i + width]);
        i += width;
    }
    out
}

pub(crate) fn on_update(
    world: &mut World,
    root: Entity,
    document: crate::document::MarkdownId,
    now: u64,
) {
    let presses: Vec<SecondaryPress> = events::<SecondaryPress>(world).to_vec();
    for press in presses {
        if owns(world, press.entity, root) {
            let _ = open_for(world, root, document, press.entity, press.at);
        }
    }
    take_menu(world, root, now);
}

/// Column and row handles are a slim strip. They take their grip color only
/// while the pointer is over the table, focus is inside it, or the caret is.
pub(crate) fn sync_handles(world: &mut World, root: Entity, caret: usize) {
    let theme = world.resource::<UiTheme>().cloned().unwrap_or_default();
    let chrome = Chrome::from_theme(&theme);
    let hover = world
        .resource::<Input>()
        .map(|input| input.pointer.position)
        .and_then(|point| hit_test(world, point[0], point[1]));
    let focus = world.resource::<Focus>().and_then(|focus| focus.entity);
    let handles = world.entities_with::<Handle>();
    for entity in handles {
        if !owns(world, entity, root) {
            continue;
        }
        let Some(handle) = world.get::<Handle>(entity).cloned() else {
            continue;
        };
        let table = ancestor_frame(world, entity);
        let range = table.and_then(|table| world.get::<GridFrame>(table).copied());
        let in_caret = range.is_some_and(|frame| {
            frame.start == handle.block_start && frame.start <= caret && caret <= frame.end
        });
        let hovered = table.is_some_and(|table| hover.is_some_and(|hit| owns(world, hit, table)));
        let focused = table
            .is_some_and(|table| focus.is_some_and(|hit| hit == table || owns(world, hit, table)));
        let color = if in_caret || hovered || focused {
            chrome.grip
        } else {
            Color::rgba(0.0, 0.0, 0.0, 0.0)
        };
        world.insert(entity, color);
    }
}

fn ancestor_frame(world: &World, mut entity: Entity) -> Option<Entity> {
    for _ in 0..48 {
        if world.has::<GridFrame>(entity) {
            return Some(entity);
        }
        entity = world.get::<Parent>(entity)?.entity;
    }
    None
}

fn owns(world: &World, mut entity: Entity, root: Entity) -> bool {
    for _ in 0..48 {
        if entity == root {
            return true;
        }
        let Some(parent) = world.get::<Parent>(entity) else {
            return false;
        };
        entity = parent.entity;
    }
    false
}
