//! Copy the visible terminal grid into a [`MonoGrid`].
//!
//! Only cells that differ bump that row's generation, so unchanged lines are
//! not shaped again. The cursor and the selection are overlays and do not
//! reshape a row.

use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Point};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::{Term, TermMode};
use alacritty_terminal::vte::ansi::{Color as AnsiColor, CursorShape, NamedColor, Rgb};
use rndr_core::{Color, MonoBand, MonoCell, MonoCursor, MonoCursorShape, MonoGrid, MonoStyle};

use crate::session::Listener;

const ANSI16: [[u8; 3]; 16] = [
    [0, 0, 0],
    [205, 0, 0],
    [0, 205, 0],
    [205, 205, 0],
    [0, 0, 238],
    [205, 0, 205],
    [0, 205, 205],
    [229, 229, 229],
    [127, 127, 127],
    [255, 0, 0],
    [0, 255, 0],
    [255, 255, 0],
    [92, 92, 255],
    [255, 0, 255],
    [0, 255, 255],
    [255, 255, 255],
];

/// Copy `term`'s viewport into `grid`.
///
/// Row generations change only where the cells changed. Cursor and selection
/// are replaced every call.
pub fn publish(term: &mut Term<Listener>, grid: &mut MonoGrid) {
    let view = snapshot(term);
    if grid.cols as usize != view.cols || grid.rows as usize != view.rows {
        grid.resize(view.cols as u16, view.rows as u16);
    }
    let cols = view.cols;
    for row in 0..view.rows {
        let start = row * cols;
        let end = start + cols;
        if grid.cells[start..end] != view.cells[start..end] {
            grid.cells[start..end].copy_from_slice(&view.cells[start..end]);
            grid.row_gen[row] = grid.row_gen[row].wrapping_add(1);
        }
    }
    grid.cursor = view.cursor;
    grid.selection = view.selection;
    let _ = term.damage();
    term.reset_damage();
}

struct View {
    cols: usize,
    rows: usize,
    cells: Vec<MonoCell>,
    cursor: Option<MonoCursor>,
    selection: Vec<MonoBand>,
}

fn snapshot(term: &Term<Listener>) -> View {
    let content = term.renderable_content();
    let cols = term.columns();
    let rows = term.screen_lines();
    let cursor = content.cursor;
    let offset = content.display_offset;
    let mut cells = Vec::with_capacity(cols * rows);
    for indexed in content.display_iter {
        cells.push(convert_cell(indexed.cell, content.colors));
    }
    if cells.len() != cols * rows {
        cells.resize(cols * rows, MonoCell::blank(default_fg(), default_bg()));
    }
    let cursor = cursor_of(term, cursor, offset, &cells, cols);
    let selection = selection_of(term, cols, rows);
    View {
        cols,
        rows,
        cells,
        cursor,
        selection,
    }
}

fn convert_cell(
    cell: &alacritty_terminal::term::cell::Cell,
    colors: &alacritty_terminal::term::color::Colors,
) -> MonoCell {
    let mut fg = resolve(cell.fg, colors, true);
    let mut bg = resolve(cell.bg, colors, false);
    if cell.flags.contains(Flags::DIM) {
        fg = scale(fg, 0.67);
    }
    if cell.flags.contains(Flags::INVERSE) {
        std::mem::swap(&mut fg, &mut bg);
    }
    let spacer = cell
        .flags
        .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER);
    let hidden = cell.flags.contains(Flags::HIDDEN) || spacer;
    MonoCell {
        ch: if spacer { ' ' } else { cell.c },
        fg,
        bg,
        style: MonoStyle {
            bold: cell.flags.contains(Flags::BOLD),
            italic: cell.flags.contains(Flags::ITALIC),
            underline: cell.flags.intersects(Flags::ALL_UNDERLINES),
            strike: cell.flags.contains(Flags::STRIKEOUT),
            hidden,
            wide: cell.flags.contains(Flags::WIDE_CHAR),
            spacer,
        },
    }
}

fn cursor_of(
    term: &Term<Listener>,
    cursor: alacritty_terminal::term::RenderableCursor,
    offset: usize,
    cells: &[MonoCell],
    cols: usize,
) -> Option<MonoCursor> {
    if !term.mode().contains(TermMode::SHOW_CURSOR) {
        return None;
    }
    let shape = match cursor.shape {
        CursorShape::Hidden => return None,
        CursorShape::Block | CursorShape::HollowBlock => MonoCursorShape::Block,
        CursorShape::Underline => MonoCursorShape::Underline,
        CursorShape::Beam => MonoCursorShape::Beam,
    };
    let point = alacritty_terminal::term::point_to_viewport(offset, cursor.point)?;
    let row = point.line;
    let col = point.column.0;
    if row >= term.screen_lines() || col >= cols {
        return None;
    }
    let cell = cells.get(row * cols + col);
    let (color, ink) = match shape {
        MonoCursorShape::Block => {
            let fg = cell.map(|cell| cell.fg).unwrap_or_else(default_fg);
            let bg = cell.map(|cell| cell.bg).unwrap_or_else(default_bg);
            (fg, bg)
        }
        _ => (
            cell.map(|cell| cell.fg).unwrap_or_else(default_fg),
            default_fg(),
        ),
    };
    Some(MonoCursor {
        col: col as u16,
        row: row as u16,
        shape,
        color,
        ink,
        visible: true,
    })
}

fn selection_of(term: &Term<Listener>, cols: usize, rows: usize) -> Vec<MonoBand> {
    let Some(selection) = term.selection.as_ref() else {
        return Vec::new();
    };
    let Some(range) = selection.to_range(term) else {
        return Vec::new();
    };
    let offset = term.grid().display_offset();
    let Some(start) = viewport_cell(offset, range.start) else {
        return Vec::new();
    };
    let Some(end) = viewport_cell(offset, range.end) else {
        return Vec::new();
    };
    let (start, end) = if (start.0, start.1) <= (end.0, end.1) {
        (start, end)
    } else {
        (end, start)
    };
    let mut bands = Vec::new();
    if range.is_block {
        let left = start.1.min(end.1);
        let right = start.1.max(end.1).saturating_add(1).min(cols);
        for row in start.0..=end.0 {
            if row < rows && left < right {
                bands.push(MonoBand {
                    row: row as u16,
                    start: left as u16,
                    end: right as u16,
                });
            }
        }
        return bands;
    }
    if start.0 == end.0 {
        bands.push(MonoBand {
            row: start.0 as u16,
            start: start.1 as u16,
            end: end.1.saturating_add(1).min(cols) as u16,
        });
        return bands;
    }
    bands.push(MonoBand {
        row: start.0 as u16,
        start: start.1 as u16,
        end: cols as u16,
    });
    for row in start.0.saturating_add(1)..end.0 {
        if row < rows {
            bands.push(MonoBand {
                row: row as u16,
                start: 0,
                end: cols as u16,
            });
        }
    }
    if end.0 < rows {
        bands.push(MonoBand {
            row: end.0 as u16,
            start: 0,
            end: end.1.saturating_add(1).min(cols) as u16,
        });
    }
    bands
}

fn viewport_cell(offset: usize, point: Point) -> Option<(usize, usize)> {
    let view = alacritty_terminal::term::point_to_viewport(offset, point)?;
    Some((view.line, view.column.0))
}

pub fn cell_to_point(term: &Term<Listener>, col: usize, row: usize) -> Point {
    let offset = term.grid().display_offset();
    alacritty_terminal::term::viewport_to_point(offset, Point::new(row, Column(col)))
}

/// Indexed color, when the palette has that slot.
pub fn color_slot(colors: &alacritty_terminal::term::color::Colors, index: usize) -> Option<Rgb> {
    const SLOTS: usize = 269;
    if index >= SLOTS {
        return None;
    }
    colors[index]
}

fn resolve(
    color: AnsiColor,
    colors: &alacritty_terminal::term::color::Colors,
    foreground: bool,
) -> Color {
    match color {
        AnsiColor::Spec(rgb) => from_rgb(rgb),
        AnsiColor::Indexed(index) => {
            if let Some(rgb) = color_slot(colors, index as usize) {
                from_rgb(rgb)
            } else {
                from_channels(indexed(index))
            }
        }
        AnsiColor::Named(named) => {
            if let Some(rgb) = colors[named] {
                return from_rgb(rgb);
            }
            match named {
                NamedColor::Foreground => default_fg(),
                NamedColor::Background => default_bg(),
                NamedColor::Cursor => default_fg(),
                NamedColor::Black => from_channels(ANSI16[0]),
                NamedColor::Red => from_channels(ANSI16[1]),
                NamedColor::Green => from_channels(ANSI16[2]),
                NamedColor::Yellow => from_channels(ANSI16[3]),
                NamedColor::Blue => from_channels(ANSI16[4]),
                NamedColor::Magenta => from_channels(ANSI16[5]),
                NamedColor::Cyan => from_channels(ANSI16[6]),
                NamedColor::White => from_channels(ANSI16[7]),
                NamedColor::BrightBlack => from_channels(ANSI16[8]),
                NamedColor::BrightRed => from_channels(ANSI16[9]),
                NamedColor::BrightGreen => from_channels(ANSI16[10]),
                NamedColor::BrightYellow => from_channels(ANSI16[11]),
                NamedColor::BrightBlue => from_channels(ANSI16[12]),
                NamedColor::BrightMagenta => from_channels(ANSI16[13]),
                NamedColor::BrightCyan => from_channels(ANSI16[14]),
                NamedColor::BrightWhite => from_channels(ANSI16[15]),
                _ => {
                    if foreground {
                        default_fg()
                    } else {
                        default_bg()
                    }
                }
            }
        }
    }
}

fn indexed(index: u8) -> [u8; 3] {
    if index < 16 {
        return ANSI16[index as usize];
    }
    if index < 232 {
        let cube = index - 16;
        let r = cube / 36;
        let g = (cube % 36) / 6;
        let b = cube % 6;
        let level = |n: u8| if n == 0 { 0 } else { 55 + 40 * n };
        return [level(r), level(g), level(b)];
    }
    let gray = 8 + 10 * (index - 232);
    [gray, gray, gray]
}

fn from_rgb(rgb: Rgb) -> Color {
    from_channels([rgb.r, rgb.g, rgb.b])
}

fn from_channels(channels: [u8; 3]) -> Color {
    Color::rgb(
        channels[0] as f32 / 255.0,
        channels[1] as f32 / 255.0,
        channels[2] as f32 / 255.0,
    )
}

fn scale(color: Color, factor: f32) -> Color {
    Color::rgba(
        color.r * factor,
        color.g * factor,
        color.b * factor,
        color.a,
    )
}

pub fn default_fg() -> Color {
    Color::rgb(0.83, 0.83, 0.84)
}

pub fn default_bg() -> Color {
    Color::rgb(0.05, 0.05, 0.06)
}
