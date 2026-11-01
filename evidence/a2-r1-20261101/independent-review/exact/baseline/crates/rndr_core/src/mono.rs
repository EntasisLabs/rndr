//! Fixed-pitch cell grid.
//!
//! A grid is a viewport of cells, not a document. Each cell has a character,
//! a foreground, a background, and a small style. Layout builds glyph runs
//! only for rows whose generation changed, and blank rows do not call the
//! text engine. Paint reads that cache.

use crate::font::{PlacedGlyph, TextSpan};
use crate::style::Color;
use crate::text::{
    text_engine, FontStyle, TextEngine, TextStyle, FONT_WEIGHT_BOLD, FONT_WEIGHT_REGULAR,
};
use crate::world::{Component, World};

impl Component for MonoGrid {}
impl Component for MonoCache {}

/// How the bottom of a cell is stroked.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MonoUnderline {
    /// No underline.
    #[default]
    None,
    /// One stroke along the bottom of the cell.
    Single,
    /// Two strokes along the bottom of the cell.
    Double,
    /// A wave along the bottom of the cell.
    Curly,
    /// Short strokes with gaps.
    Dotted,
    /// Longer strokes with gaps.
    Dashed,
}

/// Style bits that change how a cell is drawn.
///
/// Bold and italic select the face. Underline and strike are extra strokes.
/// A spacer is the second column of a double-width character and is not drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MonoStyle {
    /// Shape this cell at weight 700.
    pub bold: bool,
    /// Shape this cell as italic.
    pub italic: bool,
    /// Stroke along the bottom of the cell. [`MonoUnderline::None`] draws nothing.
    pub underline: MonoUnderline,
    /// Stroke through the middle of the cell.
    pub strike: bool,
    /// Do not draw the character.
    pub hidden: bool,
    /// First column of a double-width character.
    pub wide: bool,
    /// Second column of a double-width character. No glyph is placed here.
    pub spacer: bool,
}

/// One cell of a [`MonoGrid`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MonoCell {
    /// Character in this column. A spacer keeps the wide character's pair.
    /// A tab is stored as a space so the font is not asked for a missing glyph.
    pub ch: char,
    /// Combining marks that follow [`Self::ch`], in order. Unused slots are `'\0'`.
    ///
    /// At most four marks are kept. A cluster longer than that drops the rest.
    pub marks: [char; 4],
    /// Glyph color. Already resolved, including inverse and dim.
    pub fg: Color,
    /// Cell background. Already resolved.
    pub bg: Color,
    /// Color of [`MonoStyle::underline`]. Equals [`Self::fg`] when the program
    /// did not set one.
    pub underline_color: Color,
    /// Face and decoration.
    pub style: MonoStyle,
}

impl MonoCell {
    /// A visible character with no decorations.
    pub fn new(ch: char, fg: Color, bg: Color) -> Self {
        Self {
            ch,
            marks: ['\0'; 4],
            fg,
            bg,
            underline_color: fg,
            style: MonoStyle::default(),
        }
    }

    /// A space in `fg` on `bg`.
    pub fn blank(fg: Color, bg: Color) -> Self {
        Self::new(' ', fg, bg)
    }
}

/// Block, beam, or underline cursor drawn on top of the grid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MonoCursorShape {
    /// Filled cell. The glyph is redrawn in [`MonoCursor::ink`].
    Block,
    /// Narrow bar on the left edge of the cell.
    Beam,
    /// Stroke along the bottom of the cell.
    Underline,
}

/// Cursor over one cell. `visible` is false while a blink hides it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MonoCursor {
    /// Column inside the viewport, starting at 0.
    pub col: u16,
    /// Row inside the viewport, starting at 0 at the top.
    pub row: u16,
    /// Which cursor geometry to draw.
    pub shape: MonoCursorShape,
    /// Cursor fill.
    pub color: Color,
    /// Glyph color used when [`MonoCursorShape::Block`] covers the cell.
    pub ink: Color,
    /// When false, paint skips the cursor. Blink toggles this.
    pub visible: bool,
}

/// One horizontal run of cells on a single row, in grid-local points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MonoBand {
    /// Viewport row, starting at 0 at the top.
    pub row: u16,
    /// First column, inclusive.
    pub start: u16,
    /// Column past the last selected cell.
    pub end: u16,
}

/// Fixed-pitch viewport. Coordinates inside the cache are relative to the
/// entity rect's top-left, in logical points.
///
/// `row_gen[row]` changes when that row's cells change. Unchanged generations
/// keep their shaped runs.
#[derive(Clone, Debug)]
pub struct MonoGrid {
    /// Columns in the viewport.
    pub cols: u16,
    /// Rows in the viewport.
    pub rows: u16,
    /// Cell width in logical points.
    pub cell_w: f32,
    /// Cell height in logical points.
    pub cell_h: f32,
    /// Row-major cells, length `cols * rows`.
    pub cells: Vec<MonoCell>,
    /// Per-row generation. Length `rows`. A change marks that row for shaping.
    pub row_gen: Vec<u64>,
    /// Cursor, if one should be drawn.
    pub cursor: Option<MonoCursor>,
    /// Selection bands in viewport coordinates.
    pub selection: Vec<MonoBand>,
    /// Background that paint leaves to the entity's own fill.
    pub default_bg: Color,
    /// Face used when a row is shaped. Weight and slant come from each cell.
    pub font: TextStyle,
}

impl MonoGrid {
    /// `cols` by `rows` blank cells. Generations start at 1.
    ///
    /// `cell_w` and `cell_h` are the pitch in logical points. Non-positive
    /// pitches are raised to 1 so a later divide cannot see zero.
    pub fn blank(cols: u16, rows: u16, cell_w: f32, cell_h: f32) -> Self {
        let cell_w = positive(cell_w);
        let cell_h = positive(cell_h);
        let fg = Color::rgb(0.83, 0.83, 0.84);
        let bg = Color::rgb(0.05, 0.05, 0.06);
        let len = cols as usize * rows as usize;
        Self {
            cols,
            rows,
            cell_w,
            cell_h,
            cells: vec![MonoCell::blank(fg, bg); len],
            row_gen: vec![1; rows as usize],
            cursor: None,
            selection: Vec::new(),
            default_bg: bg,
            font: TextStyle {
                family: "bitmap".into(),
                size: 16.0,
                weight: FONT_WEIGHT_REGULAR,
                style: FontStyle::Normal,
                ..TextStyle::default()
            },
        }
    }

    /// Replace the viewport size, keeping the font and the default background.
    ///
    /// Every row generation bumps so the next layout rebuilds the cache.
    pub fn resize(&mut self, cols: u16, rows: u16) {
        if cols == self.cols && rows == self.rows {
            return;
        }
        let fg = self
            .cells
            .first()
            .map(|cell| cell.fg)
            .unwrap_or_else(|| Color::rgb(0.83, 0.83, 0.84));
        let bg = self.default_bg;
        self.cols = cols;
        self.rows = rows;
        self.cells = vec![MonoCell::blank(fg, bg); cols as usize * rows as usize];
        self.row_gen = vec![self.row_gen.iter().copied().max().unwrap_or(0) + 1; rows as usize];
        self.cursor = None;
        self.selection.clear();
    }
}

/// Shaped rows for one [`MonoGrid`]. Paint reads this and does not reshape.
#[derive(Clone, Debug, Default)]
pub struct MonoCache {
    /// One entry per viewport row.
    pub rows: Vec<MonoRow>,
    /// Rows written into the cache since it was created, including blanks.
    pub built: u64,
    /// Calls into the text engine. Blank rows do not add to this.
    pub shaped: u64,
}

/// Glyphs and solid runs for one viewport row.
#[derive(Clone, Debug, Default)]
pub struct MonoRow {
    /// [`MonoGrid::row_gen`] value these quads were built from.
    pub gen: u64,
    /// Glyphs in grid-local points.
    pub glyphs: Vec<PlacedGlyph>,
    /// Backgrounds that differ from [`MonoGrid::default_bg`].
    pub backgrounds: Vec<MonoRun>,
    /// Underlines and strikes.
    pub strokes: Vec<MonoRun>,
}

/// Axis-aligned solid in grid-local points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MonoRun {
    /// Left edge, relative to the grid.
    pub x: f32,
    /// Top edge, relative to the grid.
    pub y: f32,
    /// Width in points.
    pub w: f32,
    /// Height in points.
    pub h: f32,
    /// Fill color.
    pub color: Color,
}

fn positive(value: f32) -> f32 {
    if value.is_finite() && value > 0.0 {
        value
    } else {
        1.0
    }
}

/// Rebuild shaped rows whose generation changed.
///
/// Called at the end of [`crate::App::update`], after rects are final. A row
/// that still has the recorded generation is left as it was. A row of ordinary
/// spaces does not ask the text engine for glyphs.
pub fn apply_mono_grid(world: &mut World) {
    let engine = text_engine(world);
    let entities = world.entities_with::<MonoGrid>();
    for entity in entities {
        let Some(plan) = plan_rows(world, entity) else {
            continue;
        };
        let mut updates = Vec::with_capacity(plan.stale.len());
        for row in &plan.stale {
            let (built, shapes) = shape_row(&*engine, &plan, *row);
            updates.push((*row, built, shapes));
        }
        let mut cache = world.remove::<MonoCache>(entity).unwrap_or_default();
        if cache.rows.len() != plan.rows {
            cache.rows = vec![MonoRow::default(); plan.rows];
        }
        for (row, built, shapes) in updates {
            cache.rows[row] = built;
            cache.built = cache.built.saturating_add(1);
            cache.shaped = cache.shaped.saturating_add(shapes);
        }
        world.insert(entity, cache);
    }
}

struct RowPlan {
    rows: usize,
    cols: usize,
    cell_w: f32,
    cell_h: f32,
    default_bg: Color,
    font: TextStyle,
    cells: Vec<MonoCell>,
    gens: Vec<u64>,
    stale: Vec<usize>,
}

fn plan_rows(world: &World, entity: crate::entity::Entity) -> Option<RowPlan> {
    let (cols, rows, cell_w, cell_h, default_bg, font, gens) = {
        let grid = world.get::<MonoGrid>(entity)?;
        let cols = grid.cols as usize;
        let rows = grid.rows as usize;
        if cols == 0 || rows == 0 || grid.cells.len() != cols * rows || grid.row_gen.len() != rows {
            return None;
        }
        (
            cols,
            rows,
            positive(grid.cell_w),
            positive(grid.cell_h),
            grid.default_bg,
            grid.font.clone(),
            grid.row_gen.clone(),
        )
    };
    let stale: Vec<usize> = match world.get::<MonoCache>(entity) {
        Some(cache) if cache.rows.len() == rows => (0..rows)
            .filter(|row| cache.rows[*row].gen != gens[*row])
            .collect(),
        _ => (0..rows).collect(),
    };
    if stale.is_empty() {
        return None;
    }
    let cells = world.get::<MonoGrid>(entity)?.cells.clone();
    Some(RowPlan {
        rows,
        cols,
        cell_w,
        cell_h,
        default_bg,
        font,
        cells,
        gens,
        stale,
    })
}

fn shape_row(engine: &dyn TextEngine, plan: &RowPlan, row: usize) -> (MonoRow, u64) {
    let start = row * plan.cols;
    let cells = &plan.cells[start..start + plan.cols];
    let mut backgrounds = Vec::new();
    let mut strokes = Vec::new();
    let mut glyphs = Vec::new();
    let mut shaped = 0u64;

    coalesce(cells, plan, row, &mut backgrounds, &mut strokes);

    let mut col = 0usize;
    while col < cells.len() {
        if cells[col].style.spacer || block_box(cells[col].ch).is_some() {
            col += 1;
            continue;
        }
        let style = cells[col].style;
        let run_start = col;
        let mut text = String::new();
        let mut spans = Vec::new();
        let mut columns = Vec::new();
        while col < cells.len() {
            let cell = &cells[col];
            if cell.style.spacer || block_box(cell.ch).is_some() {
                break;
            }
            if cell.style.bold != style.bold || cell.style.italic != style.italic {
                break;
            }
            let byte = text.len();
            push_cluster(&mut text, cell);
            let end = text.len();
            if cell.fg
                != spans
                    .last()
                    .map(|span: &TextSpan| span.color)
                    .unwrap_or(cell.fg)
                || spans.is_empty()
            {
                spans.push(TextSpan::new(byte, end, cell.fg).weight(if style.bold {
                    FONT_WEIGHT_BOLD
                } else {
                    FONT_WEIGHT_REGULAR
                }));
            } else if let Some(span) = spans.last_mut() {
                span.end = end;
            }
            columns.push((byte, col));
            col += 1;
            if cell.style.wide && col < cells.len() && cells[col].style.spacer {
                col += 1;
            }
        }
        if !run_needs_glyphs(&cells[run_start..col]) {
            continue;
        }
        let mut font = plan.font.clone();
        font.weight = if style.bold {
            FONT_WEIGHT_BOLD
        } else {
            FONT_WEIGHT_REGULAR
        };
        font.style = if style.italic {
            FontStyle::Italic
        } else {
            FontStyle::Normal
        };
        let mut layout = engine.layout(&text, &font, &spans, f32::INFINITY);
        shaped += 1;
        let y_shift = row as f32 * plan.cell_h;
        let placed = std::mem::take(&mut layout.glyphs);
        for mut glyph in placed {
            let column = column_of(&columns, glyph.start);
            let pen = pen_at(&layout, glyph.start);
            let origin = glyph.x;
            glyph.x = column as f32 * plan.cell_w + (origin - pen);
            glyph.y += y_shift;
            glyphs.push(glyph);
        }
    }

    (
        MonoRow {
            gen: plan.gens[row],
            glyphs,
            backgrounds,
            strokes,
        },
        shaped,
    )
}

fn push_cluster(text: &mut String, cell: &MonoCell) {
    if cell.style.hidden || cell.ch == '\0' || cell.ch == '\t' || cell.ch == ' ' {
        text.push(' ');
        return;
    }
    text.push(cell.ch);
    for mark in cell.marks {
        if mark != '\0' {
            text.push(mark);
        }
    }
}

fn run_needs_glyphs(cells: &[MonoCell]) -> bool {
    cells.iter().any(|cell| {
        !cell.style.spacer
            && !cell.style.hidden
            && block_box(cell.ch).is_none()
            && cell.ch != ' '
            && cell.ch != '\0'
            && cell.ch != '\t'
    })
}

/// Axis-aligned coverage of a block glyph, as fractions of the cell.
///
/// These characters are painted as fills. A font glyph for U+2588 is inset
/// by the side bearings, which leaves a seam between neighboring cells.
fn block_box(ch: char) -> Option<(f32, f32, f32, f32)> {
    match ch {
        '\u{2588}' => Some((0.0, 0.0, 1.0, 1.0)),
        '\u{2580}' => Some((0.0, 0.0, 1.0, 0.5)),
        '\u{2584}' => Some((0.0, 0.5, 1.0, 0.5)),
        '\u{258C}' => Some((0.0, 0.0, 0.5, 1.0)),
        '\u{2590}' => Some((0.5, 0.0, 0.5, 1.0)),
        _ => None,
    }
}

fn column_of(columns: &[(usize, usize)], byte: usize) -> usize {
    let mut column = 0;
    for (start, col) in columns {
        if *start > byte {
            break;
        }
        column = *col;
    }
    column
}

fn pen_at(layout: &crate::font::TextLayout, byte: usize) -> f32 {
    let Some(line) = layout.lines.first() else {
        return 0.0;
    };
    let mut pen = 0.0;
    for (start, x) in &line.edges {
        if *start > byte {
            break;
        }
        pen = *x;
    }
    pen
}

struct Band<'a> {
    plan: &'a RowPlan,
    row: usize,
    y_off: f32,
    h: f32,
}

fn coalesce(
    cells: &[MonoCell],
    plan: &RowPlan,
    row: usize,
    backgrounds: &mut Vec<MonoRun>,
    strokes: &mut Vec<MonoRun>,
) {
    let mut bg: Option<(usize, Color)> = None;
    let mut under: Option<(usize, Color)> = None;
    let mut strike: Option<(usize, Color)> = None;
    let mut blocks = Vec::new();
    for (col, cell) in cells.iter().enumerate() {
        if cell.style.spacer {
            continue;
        }
        let solid = block_box(cell.ch).filter(|_| !cell.style.hidden);
        bg = step_run(
            bg,
            col,
            solid.is_none() && cell.bg != plan.default_bg,
            cell.bg,
            backgrounds,
            Band {
                plan,
                row,
                y_off: 0.0,
                h: plan.cell_h,
            },
        );
        if let Some((fx, fy, fw, fh)) = solid {
            blocks.push(MonoRun {
                x: (col as f32 + fx) * plan.cell_w,
                y: row as f32 * plan.cell_h + fy * plan.cell_h,
                w: fw * plan.cell_w,
                h: fh * plan.cell_h,
                color: cell.fg,
            });
        }
        let under_h = plan.cell_h.min(2.0);
        let single = cell.style.underline == MonoUnderline::Single;
        under = step_run(
            under,
            col,
            single,
            cell.underline_color,
            strokes,
            Band {
                plan,
                row,
                y_off: plan.cell_h - under_h,
                h: under_h,
            },
        );
        if cell.style.underline != MonoUnderline::None && !single {
            emit_decor(
                strokes,
                plan,
                row,
                col,
                cell.style.underline,
                cell.underline_color,
            );
        }
        strike = step_run(
            strike,
            col,
            cell.style.strike,
            cell.fg,
            strokes,
            Band {
                plan,
                row,
                y_off: plan.cell_h * 0.5,
                h: 1.5,
            },
        );
    }
    let under_h = plan.cell_h.min(2.0);
    flush_run(
        bg,
        cells.len(),
        backgrounds,
        Band {
            plan,
            row,
            y_off: 0.0,
            h: plan.cell_h,
        },
    );
    flush_run(
        under,
        cells.len(),
        strokes,
        Band {
            plan,
            row,
            y_off: plan.cell_h - under_h,
            h: under_h,
        },
    );
    flush_run(
        strike,
        cells.len(),
        strokes,
        Band {
            plan,
            row,
            y_off: plan.cell_h * 0.5,
            h: 1.5,
        },
    );
    backgrounds.extend(blocks);
}

fn emit_decor(
    out: &mut Vec<MonoRun>,
    plan: &RowPlan,
    row: usize,
    col: usize,
    kind: MonoUnderline,
    color: Color,
) {
    let x = col as f32 * plan.cell_w;
    let bottom = (row + 1) as f32 * plan.cell_h;
    match kind {
        MonoUnderline::None | MonoUnderline::Single => {}
        MonoUnderline::Double => {
            out.push(bar(x, bottom - 3.6, plan.cell_w, 1.2, color));
            out.push(bar(x, bottom - 1.5, plan.cell_w, 1.2, color));
        }
        MonoUnderline::Dotted => {
            let mut t = 0.0;
            while t < plan.cell_w {
                let w = 1.6_f32.min(plan.cell_w - t);
                out.push(bar(x + t, bottom - 1.8, w, 1.4, color));
                t += 3.4;
            }
        }
        MonoUnderline::Dashed => {
            let mut t = 0.0;
            while t < plan.cell_w {
                let w = 4.0_f32.min(plan.cell_w - t);
                out.push(bar(x + t, bottom - 1.8, w, 1.4, color));
                t += 6.5;
            }
        }
        MonoUnderline::Curly => {
            let amp = 1.15;
            let steps = ((plan.cell_w / 2.0).round() as usize).max(3);
            for i in 0..steps {
                let frac = i as f32 / steps as f32;
                let y = bottom - 2.4 - amp + (frac * 2.0 * std::f32::consts::TAU).sin() * amp;
                let w = plan.cell_w / steps as f32 + 0.8;
                out.push(bar(x + frac * plan.cell_w, y, w, 1.4, color));
            }
        }
    }
}

fn bar(x: f32, y: f32, w: f32, h: f32, color: Color) -> MonoRun {
    MonoRun { x, y, w, h, color }
}

fn step_run(
    current: Option<(usize, Color)>,
    col: usize,
    on: bool,
    color: Color,
    out: &mut Vec<MonoRun>,
    band: Band<'_>,
) -> Option<(usize, Color)> {
    match (current, on) {
        (Some((start, prev)), true) if prev == color => Some((start, prev)),
        (Some((start, prev)), true) => {
            push_run(out, band, start, col, prev);
            Some((col, color))
        }
        (Some((start, prev)), false) => {
            push_run(out, band, start, col, prev);
            None
        }
        (None, true) => Some((col, color)),
        (None, false) => None,
    }
}

fn flush_run(current: Option<(usize, Color)>, end: usize, out: &mut Vec<MonoRun>, band: Band<'_>) {
    if let Some((start, color)) = current {
        push_run(out, band, start, end, color);
    }
}

fn push_run(out: &mut Vec<MonoRun>, band: Band<'_>, start: usize, end: usize, color: Color) {
    if end <= start || band.h <= 0.0 {
        return;
    }
    out.push(MonoRun {
        x: start as f32 * band.plan.cell_w,
        y: band.row as f32 * band.plan.cell_h + band.y_off,
        w: (end - start) as f32 * band.plan.cell_w,
        h: band.h,
        color,
    });
}
