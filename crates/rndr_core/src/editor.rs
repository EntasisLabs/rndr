use crate::caret::Caret;
use crate::clipboard::{note_clipboard, Clipboard};
use crate::edit::{
    delete_prev_grapheme, floor_boundary, floor_grapheme, grapheme_selection, next_grapheme,
    prev_grapheme,
};
use crate::hierarchy::{children_of, Parent};
use crate::input::{Focus, Input, Key};
use crate::keys::{platform_of, KeyEvent, LogicalKey, Modifiers};
use crate::layout::{ancestor_with, hit_test, Rect};
use crate::style::{Label, TEXT_ORIGIN_X, TEXT_ORIGIN_Y};
use crate::typing::{apply_text_event, steer, EditHistory, EditSurface, Steer};
use crate::world::{Component, World};

impl Component for Buffer {}
impl Component for Editor {}
impl Component for Gutter {}

/// Gap buffer plus a line-start index. Indices are byte offsets.
///
/// Focused editors accept Up/Down on both the legacy key and queued native
/// key paths. Movement follows visual rows and preserves the preferred x
/// across short rows. Shift extends selection; an unshifted vertical key
/// collapses a selection toward that direction. Carets and anchors are
/// clamped to document grapheme boundaries by the editor pass. [`crate::Spans`]
/// on an editor use document byte offsets; publication clips and rebases them
/// for the visible label without changing the source spans.
///
/// Put this on the same entity as [`Editor`]. The editor pass copies the
/// visible line window into that entity's [`crate::Label`] and publishes a
/// window-relative [`crate::Caret`]. That caret component is removed when the
/// document caret sits outside the window. [`Buffer::caret`] and
/// [`Buffer::anchor`] stay document offsets. [`Buffer::from_str`] places both
/// at the end.
#[derive(Clone, Debug)]
pub struct Buffer {
    data: Vec<u8>,
    gap_start: usize,
    gap_end: usize,
    lines: Vec<usize>,
    // Immutable decoded snapshots are shared by geometry/input until bytes change.
    snapshot: std::sync::OnceLock<std::sync::Arc<str>>,
    /// Document caret, in bytes. Not a char index, and not the window-relative
    /// [`crate::Caret`] published for drawing.
    pub caret: usize,
    /// Selection anchor, in bytes. Equal to [`Self::caret`] when the range is
    /// empty. The selected span is the half-open range between the two, in
    /// either order.
    pub anchor: usize,
}

impl Buffer {
    /// Infallible document constructor. This is not `FromStr`: a gap buffer
    /// accepts any `&str`, and a `Result` would only change every call site.
    ///
    /// [`Buffer::caret`] and [`Buffer::anchor`] start at the end of `text`,
    /// as a byte length.
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(text: &str) -> Self {
        let mut data = text.as_bytes().to_vec();
        let gap_start = data.len();
        data.resize(gap_start + 32, 0);
        let mut buffer = Self {
            data,
            gap_start,
            gap_end: gap_start + 32,
            lines: vec![0],
            snapshot: std::sync::OnceLock::new(),
            caret: text.len(),
            anchor: text.len(),
        };
        buffer.rebuild_lines();
        buffer
    }

    /// Logical length in bytes. The gap is not counted. This is not a char
    /// or grapheme count.
    pub fn len(&self) -> usize {
        self.data.len() - (self.gap_end - self.gap_start)
    }

    /// `true` when [`Self::len`] is 0. An empty buffer still reports one line.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Number of line starts. Always at least 1, including an empty buffer.
    ///
    /// A newline starts another line only when a byte follows it, so a
    /// trailing `\n` does not add an empty row.
    pub fn line_count(&self) -> u32 {
        self.lines.len() as u32
    }

    /// Byte offset where 0-based `line` starts.
    ///
    /// Returns [`Self::len`] when `line` is past the last line. Does not panic.
    pub fn line_start(&self, line: u32) -> usize {
        self.lines.get(line as usize).copied().unwrap_or(self.len())
    }

    /// The whole document as a `String`.
    ///
    /// Valid UTF-8 round-trips. Bytes left invalid by an edit that splits a
    /// codepoint are substituted, so the result is not a raw dump of the gap
    /// buffer. Other methods still count those stored bytes.
    pub fn text(&self) -> String {
        self.snapshot().to_string()
    }

    pub(crate) fn snapshot(&self) -> std::sync::Arc<str> {
        std::sync::Arc::clone(
            self.snapshot
                .get_or_init(|| self.slice(0, self.len()).into()),
        )
    }

    /// Bytes in `[start, end)`, clamped to the document.
    ///
    /// `start` and `end` are byte offsets. An end past [`Self::len`] shrinks,
    /// and a reversed pair becomes empty. A split codepoint is substituted, so
    /// the string's length can differ from the byte span. Does not panic.
    pub fn slice(&self, start: usize, end: usize) -> String {
        let end = end.min(self.len());
        let start = start.min(end);
        let mut bytes = Vec::with_capacity(end - start);
        let left_end = end.min(self.gap_start);
        if start < left_end {
            bytes.extend_from_slice(&self.data[start..left_end]);
        }
        let right_start = start.max(self.gap_start);
        if right_start < end {
            let gap = self.gap_end - self.gap_start;
            bytes.extend_from_slice(&self.data[right_start + gap..end + gap]);
        }
        String::from_utf8_lossy(&bytes).into_owned()
    }

    /// Insert `text` at byte `index`, clamped to [`Self::len`].
    ///
    /// An empty `text` returns without moving the gap or rebuilding lines.
    /// Does not move [`Self::caret`] or [`Self::anchor`]. A mid-codepoint
    /// index splits that scalar. The line index is updated. This is history-free;
    /// use [`edit_buffer`] for undoable app-owned source operations.
    pub fn insert_str(&mut self, index: usize, text: &str) {
        if text.is_empty() {
            return;
        }
        let index = index.min(self.len());
        self.snapshot.take();
        self.move_gap_to(index);
        let bytes = text.as_bytes();
        self.ensure_gap(bytes.len());
        self.data[self.gap_start..self.gap_start + bytes.len()].copy_from_slice(bytes);
        self.gap_start += bytes.len();
        self.update_lines(index, index, bytes);
    }

    /// Delete bytes in `[start, end)`, clamped into order inside the document.
    ///
    /// An empty span returns immediately. Does not move [`Self::caret`] or
    /// [`Self::anchor`], so those indices can sit past the new end until the
    /// caller updates them. Not snapped to a char boundary. This is history-free;
    /// use [`edit_buffer`] for undoable app-owned source operations.
    pub fn delete_range(&mut self, start: usize, end: usize) {
        let start = start.min(self.len());
        let end = end.min(self.len()).max(start);
        if start == end {
            return;
        }
        self.snapshot.take();
        self.move_gap_to(start);
        self.gap_end += end - start;
        self.update_lines(start, end, &[]);
    }

    fn byte(&self, logical: usize) -> u8 {
        let physical = if logical < self.gap_start {
            logical
        } else {
            logical + (self.gap_end - self.gap_start)
        };
        self.data[physical]
    }

    fn ensure_gap(&mut self, needed: usize) {
        let gap = self.gap_end - self.gap_start;
        if gap >= needed {
            return;
        }
        let extra = needed + 32 - gap;
        self.data
            .splice(self.gap_end..self.gap_end, std::iter::repeat_n(0, extra));
        self.gap_end += extra;
    }

    fn move_gap_to(&mut self, index: usize) {
        let index = index.min(self.len());
        if index == self.gap_start {
            return;
        }
        // Moving a gap is a memmove, not a document serialization. In
        // particular, do not run lossy UTF-8 decoding on raw byte edits.
        if index < self.gap_start {
            let count = self.gap_start - index;
            self.data
                .copy_within(index..self.gap_start, self.gap_end - count);
            self.gap_end -= count;
        } else {
            let count = index - self.gap_start;
            self.data
                .copy_within(self.gap_end..self.gap_end + count, self.gap_start);
            self.gap_end += count;
        }
        self.gap_start = index;
    }

    // Keep unaffected line starts and shift the suffix; scan only inserted
    // bytes. A final newline is deliberately not an extra empty legacy row.
    fn update_lines(&mut self, start: usize, end: usize, inserted: &[u8]) {
        let len = self.len();
        let first = self.lines.partition_point(|&at| at < start || at == 0);
        let last = self.lines.partition_point(|&at| at <= end);
        let mut next = Vec::new();
        if start > 0 && start < len && self.byte(start - 1) == b'\n' {
            next.push(start);
        }
        for (offset, &byte) in inserted.iter().enumerate() {
            let at = start + offset + 1;
            if byte == b'\n' && at < len {
                next.push(at);
            }
        }
        for at in &mut self.lines[last..] {
            *at = *at - (end - start) + inserted.len();
        }
        self.lines.splice(first..last, next);
        // Deletion can expose a formerly internal newline at EOF.
        if len > 0 && self.lines.last() == Some(&len) {
            self.lines.pop();
        }
    }

    fn rebuild_lines(&mut self) {
        self.lines.clear();
        self.lines.push(0);
        let len = self.len();
        for index in 0..len {
            if self.byte(index) == b'\n' && index + 1 < len {
                self.lines.push(index + 1);
            }
        }
    }
}

/// A refused [`edit_buffer`] operation. Refusal leaves text, selection, and
/// history unchanged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BufferEditError {
    /// The entity has no [`Buffer`].
    MissingBuffer,
    /// The range is reversed, out of bounds, or splits a UTF-8 scalar.
    InvalidRange,
    /// The target has an active IME preedit. Retry after composition ends.
    Composing,
    /// A managed source view must use its revisioned SourceSession instead.
    ManagedDocument,
}

impl std::fmt::Display for BufferEditError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::MissingBuffer => "entity has no Buffer",
            Self::InvalidRange => "range is not an in-bounds UTF-8 byte range",
            Self::Composing => "buffer has an active IME composition",
            Self::ManagedDocument => "managed document requires a revisioned source batch",
        })
    }
}

impl std::error::Error for BufferEditError {}

/// Replace a current-source byte range as one keyboard undo/redo step.
///
/// For app-owned source operations (for example a task toggle), use this
/// instead of the history-free [`Buffer::delete_range`]/[`Buffer::insert_str`].
/// The range must be ordered, in bounds, and on UTF-8 scalar boundaries; it is
/// never silently clamped. `Ok(false)` means identical bytes (no history or
/// selection change). A changed edit clears redo and ends typing coalescence.
///
/// Selection endpoints before the range stay put, endpoints at/after its end
/// move by the byte delta, and endpoints inside it move to the replacement end.
/// Insertion is right-affine: endpoints at the insertion move after it. The
/// resulting endpoints snap backward to grapheme boundaries. Undo restores
/// the original text and selection; redo restores the remapped selection.
///
/// Does not steal focus, consume queued input, modify clipboard, or cancel
/// composition. Active preedit on the target refuses the operation. Call from
/// an app system on the UI thread, using ranges derived from the current source
/// (not stale parser results). Chrome publishes the visible label/caret on its
/// next editor pass. Derived spans/decorations/projections remain app-owned and
/// must be rebuilt after text changes, including undo/redo. This is not a
/// hidden-markup display projection or a parser/serializer.
///
/// ```
/// use rndr_core::{edit_buffer, App, Buffer, Chrome, Editor, Label};
/// let mut app = App::new();
/// app.add_plugin(Chrome);
/// let note = app.world_mut().spawn((
///     Buffer::from_str("- [ ] task"), Editor::default(), Label::new(""),
/// ));
/// // The app locates the exact current-source range; rndr interprets no markup.
/// assert_eq!(edit_buffer(app.world_mut(), note, 3..4, "x"), Ok(true));
/// assert_eq!(app.world().get::<Buffer>(note).unwrap().text(), "- [x] task");
/// // Standard focused-editor Undo will restore the unchecked bytes.
/// ```
pub fn edit_buffer(
    world: &mut World,
    entity: crate::Entity,
    range: std::ops::Range<usize>,
    replacement: &str,
) -> Result<bool, BufferEditError> {
    if world.has::<crate::editing::SourceBinding>(entity)
        || world.has::<crate::structured_native::StructuredBinding>(entity)
    {
        return Err(BufferEditError::ManagedDocument);
    }
    let buffer = world
        .get::<Buffer>(entity)
        .ok_or(BufferEditError::MissingBuffer)?;
    let text = buffer.text();
    if range.start > range.end
        || range.end > text.len()
        || !text.is_char_boundary(range.start)
        || !text.is_char_boundary(range.end)
    {
        return Err(BufferEditError::InvalidRange);
    }
    let focused = world.resource::<Focus>().and_then(|focus| focus.entity) == Some(entity);
    if world
        .get::<crate::ImePreview>(entity)
        .is_some_and(|ime| !ime.text.is_empty())
        || (focused
            && world
                .resource::<Input>()
                .is_some_and(|input| !input.ime_preedit.is_empty()))
    {
        return Err(BufferEditError::Composing);
    }
    if &text[range.clone()] == replacement {
        return Ok(false);
    }
    let caret = buffer.caret;
    let anchor = buffer.anchor;
    let mut next = text.clone();
    next.replace_range(range.clone(), replacement);
    let remap = |at: usize| {
        let at = at.min(text.len());
        let at = if at < range.start {
            at
        } else if at >= range.end {
            at - (range.end - range.start) + replacement.len()
        } else {
            range.start + replacement.len()
        };
        floor_grapheme(&next, at)
    };
    let (next_caret, next_anchor) = (remap(caret), remap(anchor));
    let mut history = world.remove::<EditHistory>(entity).unwrap_or_default();
    history.record_source_edit(text, caret, anchor);
    replace_buffer(
        world.get_mut::<Buffer>(entity).expect("validated buffer"),
        &next,
        next_caret,
        next_anchor,
    );
    world.insert(entity, history);
    world.remove::<VerticalGoal>(entity);
    world.remove::<EditorDrag>(entity);
    Ok(true)
}

/// Vertical scroll, in logical points, for a [`Buffer`] on the same entity.
/// Editors soft-wrap at their rectangle's inner text width, using the same
/// text engine and entity style as painting and hit testing.
///
/// The offset chooses the first visible visual row (soft wraps included).
/// Editing or navigating the focused buffer reveals its caret by updating
/// this offset. An idle editor preserves explicit scrolling. Offsets snap to
/// row tops; negative and non-finite values are treated as zero.
///
/// Primary-button presses start a selection (Shift preserves the old anchor).
/// Held and release frames extend it, even outside the editor rectangle, while
/// focus and pointer identity remain unchanged. Vertical off-editor dragging
/// scrolls at most one visual row per delivered update, not per elapsed time.
/// Release, lost focus, a changed contact/button, or a missing held state ends
/// this logical capture. Native capture outside the window and stationary-drag
/// frame scheduling remain the platform runner's responsibility.
#[derive(Clone, Copy, Debug)]
pub struct Editor {
    /// Distance from the top of the document, in logical points. Negative
    /// values count as zero when the first visible line is chosen.
    pub offset: f32,
}

impl Default for Editor {
    fn default() -> Self {
        Self { offset: 0.0 }
    }
}

/// Sibling column that shows line numbers for the visible window.
///
/// Marker with no data. The editor pass finds a [`Gutter`] under the same
/// parent as the [`Editor`] and writes 1-based line numbers for the visible
/// rows into that sibling's [`crate::Label`].
#[derive(Clone, Copy, Debug, Default)]
pub struct Gutter;

#[derive(Clone, Debug, Default)]
struct EditorWindow {
    base: usize,
    end: usize,
    hidden: Vec<std::ops::Range<usize>>,
}
impl Component for EditorWindow {}

// Logical selection capture. Native backends provide implicit button capture;
// the timer drives stationary off-viewport motion without confining the cursor.
struct EditorDrag {
    id: u64,
    anchor: usize,
    direction: i8,
    next_step: std::time::Duration,
    // Dropping capture (release, blur, destruction) cancels its pending wake.
    _wake: Option<crate::TimerHandle>,
}

const DRAG_ROW_PERIOD: std::time::Duration = std::time::Duration::from_millis(50);
impl Component for EditorDrag {}

#[path = "editor_geometry.rs"]
mod geometry;
#[path = "editor_index.rs"]
mod index;

/// Deterministic per-view geometry costs, separate from visible glyph publication.
/// Counters accumulate until the view is destroyed. Retained byte counts include
/// row/edge vector capacities and paragraph payloads, not allocator overhead,
/// source/history, engine caches or the visible label's paint layout.
#[derive(Clone, Copy, Debug, Default)]
pub struct EditorLayoutStats {
    /// Paragraph layout requests since attachment (engine caches may also hit).
    pub paragraphs_shaped: u64,
    /// UTF-8 bytes submitted to paragraph layout.
    pub bytes_shaped: u64,
    /// Paragraphs reused without a layout request.
    pub paragraphs_reused: u64,
    /// Full row assemblies; idle scroll does not increase this counter.
    pub assemblies: u64,
    /// Current retained paragraph payload, capped at 8 MiB per view.
    pub paragraph_cache_bytes: usize,
    /// Caret-edge geometry this view retains. Plain text keeps the visible
    /// window plus a small overscan, so the count does not grow with the
    /// document. Tables still retain every row.
    pub row_geometry_bytes: usize,
}
impl Component for EditorLayoutStats {}

// Only geometry is retained here; glyphs belong to the visible Label.
struct DocumentRows {
    /// Style, spans, font, and engine. Width and folds are separate.
    context: u64,
    width_bits: u32,
    fold_key: u64,
    text: std::sync::Arc<str>,
    cache: geometry::Cache,
    // Keep engine allocation identity alive, so pointer-derived keys cannot alias.
    _engine: std::sync::Arc<dyn crate::TextEngine>,
    /// Paragraph heights for plain text. Empty when `table` is set.
    index: std::sync::Arc<index::RowIndex>,
    /// Table layouts keep every row. Plain text keeps the visible window's edges.
    layout: std::sync::Arc<crate::TextLayout>,
    table: bool,
}
impl Component for DocumentRows {}

struct MappedScrollAnchor(usize);
impl Component for MappedScrollAnchor {}
pub(crate) fn sync_source_scroll_anchor(world: &mut World, entity: crate::Entity) {
    let Some(binding) = world.get::<crate::editing::SourceBinding>(entity).copied() else {
        return;
    };
    let Some(rows) = world.get::<DocumentRows>(entity) else {
        return;
    };
    let Some(editor) = world.get::<Editor>(entity) else {
        return;
    };
    let at = if rows.table {
        let row = rows
            .layout
            .lines
            .partition_point(|l| l.y <= editor.offset)
            .saturating_sub(1);
        rows.layout.lines.get(row).map(|l| l.start)
    } else if rows.index.line_count() == 0 {
        None
    } else {
        Some(
            rows.index
                .line(rows.index.row_at_y(editor.offset.max(0.0)))
                .start,
        )
    };
    let Some(at) = at else {
        return;
    };
    if let Some(doc) = world
        .resource_mut::<crate::editing::SourceDocuments>()
        .and_then(|d| d.get_mut(binding.document))
    {
        // Never overwrite an eagerly mapped anchor with old-revision geometry.
        if doc.revision() == binding.revision {
            doc.set_view_scroll_anchor(entity, at);
        }
    }
}

pub(crate) fn project_scroll_anchor(world: &mut World, entity: crate::Entity, byte: usize) {
    if world.has::<DocumentRows>(entity) {
        world.insert(entity, MappedScrollAnchor(byte));
    }
}

fn fold_key(world: &World, entity: crate::Entity) -> u64 {
    use std::hash::{Hash, Hasher};
    let Some(folds) = crate::source_folding::current(world, entity) else {
        return 0;
    };
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    folds.ranges.hash(&mut hash);
    hash.finish()
}

fn fold_ranges(world: &World, entity: crate::Entity) -> Vec<std::ops::Range<usize>> {
    crate::source_folding::current(world, entity)
        .map(|folds| folds.ranges.clone())
        .unwrap_or_default()
}

fn shape_context(
    world: &World,
    entity: crate::Entity,
    font: &dyn crate::Font,
    style: &crate::TextStyle,
    engine: &std::sync::Arc<dyn crate::TextEngine>,
    spans: &[crate::TextSpan],
) -> u64 {
    let mut context = crate::font::layout_cache_key(font, "", 0.0, spans, "", 0);
    context ^= crate::text::style_key(style);
    context ^= crate::structured_chrome::layout_key(world, entity);
    context ^= (std::sync::Arc::as_ptr(engine) as *const () as usize as u64).rotate_left(17);
    context ^= engine.cache_epoch().wrapping_mul(0x9E37_79B9_7F4A_7C15);
    context
}

fn anchor_from_rows(rows: &DocumentRows, offset: f32) -> Option<usize> {
    if rows.table {
        let row = rows
            .layout
            .lines
            .partition_point(|line| line.y <= offset)
            .saturating_sub(1);
        rows.layout.lines.get(row).map(|line| line.start)
    } else if rows.index.line_count() == 0 {
        None
    } else {
        Some(rows.index.line(rows.index.row_at_y(offset)).start)
    }
}

fn place_index_anchor(
    world: &mut World,
    entity: crate::Entity,
    index: &index::RowIndex,
    byte: usize,
) {
    if index.line_count() == 0 {
        return;
    }
    let line = index.line_for_byte(byte);
    place_anchored_row(world, entity, line.y, line.h);
}

#[allow(clippy::too_many_arguments)]
fn window_for(
    world: &mut World,
    entity: crate::Entity,
    text: &str,
    index: &index::RowIndex,
    cache: &mut geometry::Cache,
    work: &mut geometry::Work,
    engine: &dyn crate::TextEngine,
    font: &dyn crate::Font,
    style: &crate::TextStyle,
    width: f32,
    spans: &[crate::TextSpan],
) -> crate::TextLayout {
    let offset = world
        .get::<Editor>(entity)
        .map(|editor| editor.offset)
        .filter(|offset| offset.is_finite())
        .unwrap_or(0.0)
        .max(0.0);
    let height = world
        .get::<Rect>(entity)
        .map(|rect| rect.h)
        .unwrap_or(400.0);
    let available = (height - 2.0 * TEXT_ORIGIN_Y).max(1.0);
    if index.line_count() == 0 {
        return crate::TextLayout::shaped(Vec::new(), Vec::new(), 0.0, 0.0);
    }
    let first = index.row_at_y(offset);
    let last = index
        .row_at_y(offset + available)
        .saturating_add(4)
        .min(index.line_count().saturating_sub(1));
    index::window_layout(
        index, engine, font, style, width, text, spans, first, last, None, cache, work,
    )
}

#[allow(clippy::too_many_arguments)]
fn finish_rows(
    world: &mut World,
    entity: crate::Entity,
    text: &str,
    context: u64,
    width_bits: u32,
    fold_key: u64,
    engine: std::sync::Arc<dyn crate::TextEngine>,
    built: index::Built,
    window: crate::TextLayout,
    table: bool,
    anchor: Option<usize>,
) -> std::sync::Arc<crate::TextLayout> {
    let mut stats = world
        .get::<EditorLayoutStats>(entity)
        .copied()
        .unwrap_or_default();
    stats.paragraphs_shaped += built.work.paragraphs;
    stats.bytes_shaped += built.work.bytes;
    stats.paragraphs_reused += built.work.reused;
    stats.assemblies += 1;
    stats.paragraph_cache_bytes = built.cache.bytes;
    stats.row_geometry_bytes = geometry::geometry_bytes(&window);
    world.insert(entity, stats);
    if let Some(byte) = anchor {
        if table {
            let row = window
                .lines
                .partition_point(|line| line.start <= byte)
                .saturating_sub(1);
            if let Some(line) = window.lines.get(row) {
                let (y, h) = (line.y, line.h);
                place_anchored_row(world, entity, y, h);
            }
        } else {
            place_index_anchor(world, entity, &built.index, byte);
        }
    }
    world.remove::<VerticalGoal>(entity);
    let layout = std::sync::Arc::new(window);
    let snapshot = world
        .get::<Buffer>(entity)
        .map(Buffer::snapshot)
        .filter(|snapshot| std::ptr::eq(snapshot.as_ref(), text))
        .unwrap_or_else(|| std::sync::Arc::from(text));
    world.insert(
        entity,
        DocumentRows {
            context,
            width_bits,
            fold_key,
            text: snapshot,
            cache: built.cache,
            _engine: engine,
            index: std::sync::Arc::new(built.index),
            layout: std::sync::Arc::clone(&layout),
            table,
        },
    );
    layout
}

fn owned_index(index: std::sync::Arc<index::RowIndex>) -> index::RowIndex {
    std::sync::Arc::try_unwrap(index).unwrap_or_else(|shared| (*shared).clone())
}

fn document_rows(
    world: &mut World,
    entity: crate::entity::Entity,
    text: &str,
) -> std::sync::Arc<crate::TextLayout> {
    crate::source_folding::prepare(world, entity);
    let mapped_anchor = world.remove::<MappedScrollAnchor>(entity).map(|a| a.0);
    let style = crate::text::style_for(world, entity);
    let engine = crate::text::text_engine(world);
    let width = world.get::<Rect>(entity).map_or(f32::INFINITY, |rect| {
        crate::font::field_text_width(rect.w, engine.unit_advance(&style))
    });
    let spans = world
        .get::<crate::Spans>(entity)
        .map(|spans| spans.runs.clone())
        .unwrap_or_default();
    let font = crate::text_face(world);
    let context = shape_context(world, entity, font.as_ref(), &style, &engine, &spans);
    let folds_key = fold_key(world, entity);
    let width_bits = width.to_bits();
    let same_text =
        |rows: &DocumentRows| std::ptr::eq(rows.text.as_ref(), text) || rows.text.as_ref() == text;
    if let Some(rows) = world.get::<DocumentRows>(entity).filter(|rows| {
        rows.context == context
            && rows.width_bits == width_bits
            && rows.fold_key == folds_key
            && same_text(rows)
    }) {
        let layout = std::sync::Arc::clone(&rows.layout);
        if let Some(byte) = mapped_anchor {
            if rows.table {
                let row = layout
                    .lines
                    .partition_point(|line| line.start <= byte)
                    .saturating_sub(1);
                if let Some(line) = layout.lines.get(row) {
                    let (y, h) = (line.y, line.h);
                    place_anchored_row(world, entity, y, h);
                }
            } else {
                let index = std::sync::Arc::clone(&rows.index);
                place_index_anchor(world, entity, &index, byte);
            }
        }
        return layout;
    }
    let previous = world.remove::<DocumentRows>(entity);
    let old_anchor = mapped_anchor.or_else(|| {
        let rows = previous.as_ref()?;
        let offset = world.get::<Editor>(entity)?.offset;
        let offset = if offset.is_finite() {
            offset.max(0.0)
        } else {
            0.0
        };
        let at = anchor_from_rows(rows, offset)?;
        Some(map_scroll_anchor(&rows.text, text, at))
    });
    if crate::structured_chrome::has_tables(world, entity) {
        let (mut layout, cache, work) = if let Some(mut layout) =
            crate::structured_chrome::text_layout(world, entity, &spans, text, width, (0, 0, 0))
        {
            layout.glyphs = Vec::new();
            layout.highlights = Vec::new();
            (
                layout,
                geometry::Cache::default(),
                geometry::Work {
                    paragraphs: text.split('\n').count() as u64,
                    bytes: text.len() as u64,
                    reused: 0,
                },
            )
        } else {
            geometry::build(
                engine.as_ref(),
                font.as_ref(),
                &style,
                width,
                text,
                &spans,
                previous
                    .as_ref()
                    .filter(|rows| rows.context == context)
                    .map(|rows| &rows.cache),
            )
        };
        crate::source_folding::project_fold_layout(world, entity, &mut layout);
        let dummy = index::build(
            engine.as_ref(),
            font.as_ref(),
            &style,
            width,
            "",
            &[],
            &[],
            None,
        );
        return finish_rows(
            world,
            entity,
            text,
            context,
            width_bits,
            folds_key,
            engine,
            index::Built {
                index: dummy.index,
                cache,
                work,
            },
            layout,
            true,
            old_anchor,
        );
    }
    let folds = fold_ranges(world, entity);
    let built = match previous {
        Some(rows) if !rows.table && rows.context == context && rows.width_bits == width_bits => {
            if same_text(&rows) {
                index::Built {
                    index: index::refold(owned_index(rows.index), &folds),
                    cache: rows.cache,
                    work: geometry::Work::default(),
                }
            } else {
                let old = rows.text.clone();
                index::update_text(
                    owned_index(rows.index),
                    rows.cache,
                    engine.as_ref(),
                    font.as_ref(),
                    &style,
                    width,
                    &old,
                    text,
                    &spans,
                    &folds,
                )
            }
        }
        Some(rows) if !rows.table && rows.context == context && same_text(&rows) => {
            index::refit_width(
                owned_index(rows.index),
                rows.cache,
                engine.as_ref(),
                font.as_ref(),
                &style,
                width,
                text,
                &spans,
                &folds,
            )
        }
        previous => index::build(
            engine.as_ref(),
            font.as_ref(),
            &style,
            width,
            text,
            &spans,
            &folds,
            previous.as_ref().map(|rows| &rows.cache),
        ),
    };
    if let Some(byte) = old_anchor {
        place_index_anchor(world, entity, &built.index, byte);
    }
    let mut cache = built.cache;
    let mut work = built.work;
    let window = window_for(
        world,
        entity,
        text,
        &built.index,
        &mut cache,
        &mut work,
        engine.as_ref(),
        font.as_ref(),
        &style,
        width,
        &spans,
    );
    finish_rows(
        world,
        entity,
        text,
        context,
        width_bits,
        folds_key,
        engine,
        index::Built {
            index: built.index,
            cache,
            work,
        },
        window,
        false,
        None,
    )
}

// Conservative single-replacement map for legacy buffers and style reflow.
// Managed batches supply an exact eagerly mapped per-view anchor instead. No stale map is
// retained: deleted anchors collapse to the start of the changed region.
fn map_scroll_anchor(before: &str, after: &str, at: usize) -> usize {
    if before == after {
        return at.min(after.len());
    }
    let mut prefix = before
        .bytes()
        .zip(after.bytes())
        .take_while(|(a, b)| a == b)
        .count();
    while !before.is_char_boundary(prefix) || !after.is_char_boundary(prefix) {
        prefix -= 1;
    }
    let mut suffix = before[prefix..]
        .bytes()
        .rev()
        .zip(after[prefix..].bytes().rev())
        .take_while(|(a, b)| a == b)
        .count();
    while !before.is_char_boundary(before.len() - suffix)
        || !after.is_char_boundary(after.len() - suffix)
    {
        suffix -= 1;
    }
    if at < prefix {
        at
    } else if at >= before.len() - suffix {
        after.len() - (before.len() - at)
    } else {
        prefix
    }
}

#[derive(Clone, Copy, Debug)]
struct VerticalGoal {
    x: f32,
    index: usize,
    row: usize,
}
impl Component for VerticalGoal {}

fn index_holds(index: &index::RowIndex, row: usize, byte: usize) -> bool {
    if index.line_count() == 0 || row >= index.line_count() {
        return false;
    }
    let line = index.line(row);
    byte >= line.start && byte <= line.end
}

fn shaped_row(world: &mut World, entity: crate::Entity, text: &str, row: usize) -> crate::TextLine {
    let style = crate::text::style_for(world, entity);
    let engine = crate::text::text_engine(world);
    let font = crate::text_face(world);
    let width = world.get::<Rect>(entity).map_or(f32::INFINITY, |rect| {
        crate::font::field_text_width(rect.w, engine.unit_advance(&style))
    });
    let spans = world
        .get::<crate::Spans>(entity)
        .map(|spans| spans.runs.clone())
        .unwrap_or_default();
    let mut rows = world.remove::<DocumentRows>(entity).expect("document rows");
    let mut work = geometry::Work::default();
    let layout = index::window_layout(
        &rows.index,
        engine.as_ref(),
        font.as_ref(),
        &style,
        width,
        text,
        &spans,
        row,
        row,
        None,
        &mut rows.cache,
        &mut work,
    );
    if let Some(stats) = world.get_mut::<EditorLayoutStats>(entity) {
        stats.paragraphs_shaped += work.paragraphs;
        stats.bytes_shaped += work.bytes;
        stats.paragraphs_reused += work.reused;
        stats.paragraph_cache_bytes = rows.cache.bytes;
    }
    let line = layout.lines.into_iter().next().unwrap_or(crate::TextLine {
        start: 0,
        end: 0,
        y: 0.0,
        h: 1.0,
        baseline: 0.8,
        underline_thickness: 1.0,
        underline_offset: 1.0,
        width: 0.0,
        edges: Vec::new(),
    });
    world.insert(entity, rows);
    line
}

fn row_contains(layout: &crate::TextLayout, row: usize, index: usize) -> bool {
    layout
        .lines
        .get(row)
        .is_some_and(|line| index >= line.start && index <= line.end)
}

fn vertical_key(event: &KeyEvent, platform: crate::Platform) -> Option<(isize, bool)> {
    match steer(event, platform) {
        Steer::Up => return Some((-1, false)),
        Steer::Down => return Some((1, false)),
        _ => {}
    }
    if event.modifiers.ctrl || event.modifiers.alt || event.modifiers.super_key {
        return None;
    }
    match event.logical {
        LogicalKey::Named(crate::NamedKey::PageUp) => Some((-1, true)),
        LogicalKey::Named(crate::NamedKey::PageDown) => Some((1, true)),
        _ => None,
    }
}

// Page motion uses shaped row geometry and leaves one row of overlap. Like
// Up/Down it retains preferred x and the wrap-row hint; it never invents byte
// columns (which would split Unicode or drift across short rows).
pub(crate) fn vertical_delta(
    world: &mut World,
    entity: crate::Entity,
    text: &str,
    index: usize,
    event: &KeyEvent,
) -> Option<isize> {
    let (direction, page) = vertical_key(event, platform_of(world))?;
    if !page {
        return Some(direction);
    }
    document_rows(world, entity, text);
    if !crate::structured_chrome::has_tables(world, entity) {
        let rows = std::sync::Arc::clone(&world.get::<DocumentRows>(entity).unwrap().index);
        if rows.line_count() == 0 {
            return Some(direction);
        }
        let row = world
            .get::<VerticalGoal>(entity)
            .filter(|goal| goal.index == index && index_holds(&rows, goal.row, index))
            .map(|goal| goal.row)
            .unwrap_or_else(|| rows.row_at_byte(index));
        let line = rows.line(row);
        let height = world.get::<Rect>(entity).map_or(line.h, |rect| rect.h);
        let distance = (height - 2.0 * TEXT_ORIGIN_Y - line.h).max(line.h);
        let target_y = line.y + direction as f32 * distance;
        let target = rows.row_at_y(target_y);
        let delta = target as isize - row as isize;
        return Some(if delta == 0 { direction } else { delta });
    }
    let layout = world.get::<DocumentRows>(entity).unwrap().layout.clone();
    let (_, y) = layout.caret_xy(index);
    let row = world
        .get::<VerticalGoal>(entity)
        .filter(|goal| goal.index == index && row_contains(&layout, goal.row, index))
        .map_or_else(
            || {
                layout
                    .lines
                    .iter()
                    .position(|line| line.y == y)
                    .unwrap_or(0)
            },
            |goal| goal.row,
        );
    let Some(line) = layout.lines.get(row) else {
        return Some(direction);
    };
    let height = world.get::<Rect>(entity).map_or(line.h, |rect| rect.h);
    let distance = (height - 2.0 * TEXT_ORIGIN_Y - line.h).max(line.h);
    let target_y = line.y + direction as f32 * distance;
    let target = layout
        .lines
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| (a.y - target_y).abs().total_cmp(&(b.y - target_y).abs()))
        .map_or(row, |(i, _)| i);
    // A tiny viewport still advances one row. move_vertical clamps at the
    // document boundary without clearing the preferred horizontal position.
    let delta = target as isize - row as isize;
    Some(if delta == 0 { direction } else { delta })
}

pub(crate) fn move_indexed(
    world: &mut World,
    entity: crate::Entity,
    text: &str,
    index: usize,
    anchor: usize,
    delta: isize,
    shift: bool,
) -> (usize, usize) {
    let rows = std::sync::Arc::clone(&world.get::<DocumentRows>(entity).expect("rows").index);
    if rows.line_count() == 0 {
        return (index, if shift { anchor } else { index });
    }
    let previous = world.get::<VerticalGoal>(entity).copied();
    let row = previous
        .filter(|goal| goal.index == index && index_holds(&rows, goal.row, index))
        .map(|goal| goal.row)
        .unwrap_or_else(|| rows.row_at_byte(index));
    let current = shaped_row(world, entity, text, row);
    let here = crate::TextLayout::shaped(Vec::new(), vec![current], 0.0, 0.0);
    let x = previous.map_or_else(|| here.caret_xy_on_line(index, 0).0, |goal| goal.x);
    let next_row =
        (row as isize + delta).clamp(0, rows.line_count().saturating_sub(1) as isize) as usize;
    let next = if next_row == row {
        index
    } else {
        let line = shaped_row(world, entity, text, next_row);
        let laid = crate::TextLayout::shaped(Vec::new(), vec![line], 0.0, 0.0);
        laid.index_on_line(0, x)
    };
    let mut next = floor_grapheme(text, next.min(text.len()));
    let next_start = rows.line(next_row).start;
    if delta > 0 && next < next_start {
        next = next_grapheme(text, next);
    }
    let next_row = if index_holds(&rows, next_row, next) {
        next_row
    } else {
        rows.row_at_byte(next)
    };
    world.insert(
        entity,
        VerticalGoal {
            x,
            index: next,
            row: next_row,
        },
    );
    (next, if shift { anchor } else { next })
}

pub(crate) fn move_vertical(
    world: &mut World,
    entity: crate::entity::Entity,
    text: &str,
    index: usize,
    anchor: usize,
    delta: isize,
    shift: bool,
) -> (usize, usize) {
    let index = floor_grapheme(text, index.min(text.len()));
    let anchor = floor_grapheme(text, anchor.min(text.len()));
    if !shift && anchor != index {
        let edge = if delta < 0 {
            anchor.min(index)
        } else {
            anchor.max(index)
        };
        world.remove::<VerticalGoal>(entity);
        return (edge, edge);
    }
    document_rows(world, entity, text);
    if !crate::structured_chrome::has_tables(world, entity) {
        return move_indexed(world, entity, text, index, anchor, delta, shift);
    }
    let layout = std::sync::Arc::clone(&world.get::<DocumentRows>(entity).unwrap().layout);
    let previous = world.get::<VerticalGoal>(entity).copied();
    let (next, x, next_row) = {
        let (x, y) = layout.caret_xy(index);
        let previous =
            previous.filter(|goal| goal.index == index && row_contains(&layout, goal.row, index));
        let x = previous.map_or(x, |goal| goal.x);
        let row = previous.map_or_else(
            || {
                layout
                    .lines
                    .iter()
                    .position(|line| line.y == y)
                    .unwrap_or(0)
            },
            |goal| goal.row,
        );
        let next_row = if crate::structured_chrome::has_tables(world, entity) {
            let current_y = layout.lines[row].y;
            let target_y = if delta.abs() > 1 {
                layout.lines[(row as isize + delta)
                    .clamp(0, layout.lines.len().saturating_sub(1) as isize)
                    as usize]
                    .y
            } else {
                current_y
            };
            layout
                .lines
                .iter()
                .enumerate()
                .filter(|(_, l)| {
                    if delta < 0 {
                        l.y < current_y - 0.01
                    } else {
                        l.y > current_y + 0.01
                    }
                })
                .min_by(|(_, a), (_, b)| {
                    let dy = |l: &crate::font::TextLine| (l.y - target_y).abs();
                    let dx = |l: &crate::font::TextLine| {
                        l.edges
                            .iter()
                            .map(|e| (e.1 - x).abs())
                            .reduce(f32::min)
                            .unwrap_or(f32::MAX)
                    };
                    dy(a).total_cmp(&dy(b)).then(dx(a).total_cmp(&dx(b)))
                })
                .map_or(row, |(i, _)| i)
        } else {
            (row as isize + delta).clamp(0, layout.lines.len().saturating_sub(1) as isize) as usize
        };
        let next = if row == next_row {
            index
        } else {
            layout.index_on_line(next_row, x)
        };
        let mut next = floor_grapheme(text, next.min(text.len()));
        // Some engines wrap between scalars of one grapheme. Never park a
        // caret inside that cluster or attach its preceding stop to a row
        // that does not contain it. Down skips to the next whole cluster.
        if delta > 0
            && layout
                .lines
                .get(next_row)
                .is_some_and(|line| next < line.start)
        {
            next = next_grapheme(text, next);
        }
        let next_row = if row_contains(&layout, next_row, next) {
            next_row
        } else {
            document_row_at(&layout, next)
        };
        (next, x, next_row)
    };
    world.insert(
        entity,
        VerticalGoal {
            x,
            index: next,
            row: next_row,
        },
    );
    (next, if shift { anchor } else { next })
}

pub fn apply_editor(world: &mut World) {
    crate::editing::prepare_editor_source_input(world);
    crate::structured_native::prepare_input(world);
    let structural_pointer = crate::structured_native::place_from_pointer(world);
    let editors = world.entities_with::<Editor>();
    let mut publication = Vec::new();
    for entity in editors {
        if !world.has::<Buffer>(entity) {
            world.remove::<EditorDrag>(entity);
            continue;
        }
        let managed = world.has::<crate::editing::SourceBinding>(entity);
        if managed {
            crate::editing::project_source(world, entity);
        }
        crate::structured_native::project(world, entity);
        let focused = world.resource::<Focus>().and_then(|focus| focus.entity) == Some(entity);
        let mut reveal = focused
            && world.resource::<Input>().is_some_and(|input| {
                let frame = crate::ime::text_frame(input);
                !frame.commit.is_empty()
                    || if frame.keys.is_empty() {
                        !frame.chars.is_empty()
                            || matches!(
                                frame.legacy_key,
                                Some(
                                    Key::Up
                                        | Key::Down
                                        | Key::Left
                                        | Key::Right
                                        | Key::Enter
                                        | Key::Backspace
                                )
                            )
                    } else {
                        frame.keys.iter().any(|event| {
                            !event.released
                                && !event.synthetic
                                && (event.text.as_ref().is_some_and(|text| !text.is_empty())
                                    || !matches!(steer(event, platform_of(world)), Steer::Edit)
                                    || crate::keys::standard_action(platform_of(world), event)
                                        .is_some()
                                    || matches!(
                                        event.logical,
                                        LogicalKey::Named(
                                            crate::NamedKey::PageUp
                                                | crate::NamedKey::PageDown
                                                | crate::NamedKey::Home
                                                | crate::NamedKey::End
                                                | crate::NamedKey::ArrowLeft
                                                | crate::NamedKey::ArrowRight
                                                | crate::NamedKey::Backspace
                                                | crate::NamedKey::Delete
                                        )
                                    ))
                        })
                    }
            });
        if focused {
            edit_focused(world, entity);
            let composing = world
                .get::<crate::editing::SourceBinding>(entity)
                .and_then(|b| {
                    world
                        .resource::<crate::editing::SourceDocuments>()?
                        .get(b.document)
                })
                .is_some_and(crate::editing::SourceSession::is_composing);
            if !composing && !crate::structured_native::is_composing(world, entity) {
                if let Some((_, pointer_reveal)) = structural_pointer.filter(|(e, _)| *e == entity)
                {
                    // Task toggles retain the saved selection, which may be
                    // offscreen. Rule clicks explicitly select/reveal a node.
                    reveal |= pointer_reveal;
                    world.remove::<EditorDrag>(entity);
                } else {
                    reveal |= place_from_pointer(world, entity);
                }
            } else {
                world.remove::<EditorDrag>(entity);
            }
            if managed {
                crate::editing::sync_selection(world, entity);
            }
            crate::structured_native::sync_selection(world, entity);
        } else {
            world.remove::<VerticalGoal>(entity);
            world.remove::<EditorDrag>(entity);
        }
        publication.push((entity, focused, reveal));
    }
    // Publish after every edit, so peer views never lag by ECS iteration order.
    for (entity, focused, reveal) in publication {
        if world.has::<crate::editing::SourceBinding>(entity) {
            crate::editing::project_source(world, entity);
        }
        crate::structured_native::project(world, entity);
        publish_window(world, entity, reveal);
        if focused {
            sync_ime_preview(world, entity);
        } else {
            world.remove::<crate::field::ImePreview>(entity);
        }
    }
}

fn edit_focused(world: &mut World, entity: crate::entity::Entity) {
    if world.has::<crate::structured_native::StructuredBinding>(entity) {
        crate::structured_native::edit_focused(world, entity);
        return;
    }
    if world.has::<crate::editing::SourceBinding>(entity) {
        edit_managed(world, entity);
        return;
    }
    let frame = match world.resource::<Input>() {
        Some(input) => crate::ime::text_frame(input),
        None => return,
    };
    // A native IME may commit without a key event. Route that standalone
    // commit through the same selection replacement and history as queued
    // input; leave explicitly mixed legacy key/chars frames on their path.
    if !frame.keys.is_empty()
        || (!frame.commit.is_empty() && frame.legacy_key.is_none() && frame.chars.is_empty())
    {
        edit_queue(world, entity, &frame.keys, &frame.commit);
        return;
    }
    let key = frame.legacy_key;
    let shift = frame.shift;
    let chars = frame.chars;
    let commit = frame.commit;
    if matches!(key, Some(Key::Up | Key::Down)) {
        if let Some(history) = world.get_mut::<EditHistory>(entity) {
            history.break_run();
        }
        let Some(buffer) = world.get::<Buffer>(entity) else {
            return;
        };
        let (index, anchor) = move_vertical(
            world,
            entity,
            &buffer.text(),
            buffer.caret,
            buffer.anchor,
            if key == Some(Key::Up) { -1 } else { 1 },
            shift,
        );
        if let Some(buffer) = world.get_mut::<Buffer>(entity) {
            buffer.caret = index;
            buffer.anchor = anchor;
        }
        if let Some(input) = world.resource_mut::<Input>() {
            input.key = None;
        }
        if chars.is_empty() && commit.is_empty() {
            return;
        }
    }
    if key.is_some() || !chars.is_empty() || !commit.is_empty() {
        world.remove::<VerticalGoal>(entity);
    }
    let Some(buffer) = world.get_mut::<Buffer>(entity) else {
        return;
    };
    let snapshot = buffer.snapshot();
    let len = buffer.len();
    let mut index = floor_grapheme(&snapshot, buffer.caret.min(len));
    let mut anchor = floor_grapheme(&snapshot, buffer.anchor.min(len));
    if matches!(key, Some(Key::Left | Key::Right)) {
        let next = match key {
            Some(Key::Left) => prev_grapheme(&snapshot, index),
            Some(Key::Right) => next_grapheme(&snapshot, index),
            _ => index,
        };
        if shift {
            index = next;
        } else if anchor != index {
            index = match key {
                Some(Key::Left) => anchor.min(index),
                Some(Key::Right) => anchor.max(index),
                _ => next,
            };
            index = floor_grapheme(&snapshot, index);
            anchor = index;
        } else {
            index = next;
            anchor = next;
        }
        buffer.caret = index;
        buffer.anchor = anchor;
    } else {
        let typing = !chars.is_empty() || key == Some(Key::Backspace) || key == Some(Key::Enter);
        if typing && anchor != index {
            let (start, end) = grapheme_selection(&snapshot, anchor, index);
            buffer.delete_range(start, end);
            index = start;
            anchor = start;
        } else if key == Some(Key::Backspace) {
            if let Some((start, end)) = delete_prev_grapheme(&snapshot, index) {
                buffer.delete_range(start, end);
                index = start;
                anchor = start;
            }
        }
        index = floor_boundary(buffer.len(), index, &|i| buffer.byte(i));
        if !chars.is_empty() {
            let mut insert = String::new();
            for ch in chars.chars() {
                if !ch.is_control() {
                    insert.push(ch);
                }
            }
            if !insert.is_empty() {
                buffer.insert_str(index, &insert);
                index += insert.len();
                anchor = index;
            }
        }
        if key == Some(Key::Enter) {
            buffer.insert_str(index, "\n");
            index += 1;
            anchor = index;
        }
        if !commit.is_empty() {
            let commit: String = commit.chars().filter(|ch| !ch.is_control()).collect();
            if !commit.is_empty() {
                buffer.insert_str(index, &commit);
                index += commit.len();
                anchor = index;
            }
        }
        buffer.caret = index;
        buffer.anchor = anchor;
    }
    if let Some(input) = world.resource_mut::<Input>() {
        input.chars.clear();
        input.ime_commit.clear();
        if matches!(
            input.key,
            Some(Key::Left | Key::Right | Key::Backspace | Key::Enter)
        ) {
            input.key = None;
        }
    }
}

// Managed input reuses key semantics and shaping, but history and mutation live
// in SourceSession. The entity never receives a competing EditHistory.
fn edit_managed(world: &mut World, entity: crate::Entity) {
    crate::editing::activate_source_view(world, entity);
    use crate::editing::{EditOrigin, SourceBinding, SourceDocuments, SourceSelection};
    let Some(input) = world.resource::<Input>() else {
        return;
    };
    let frame = crate::ime::text_frame(input);
    let binding = *world.get::<SourceBinding>(entity).expect("managed view");
    let platform = platform_of(world);
    let now = crate::typing::frame_now(world);
    let readonly = crate::interaction::readonly(world, entity);
    let mut clip = world
        .resource::<Clipboard>()
        .map(|c| c.text.clone())
        .unwrap_or_default();
    let mut keys = frame.keys;
    if keys.is_empty() {
        let mut mods = Modifiers::new();
        mods.shift = frame.shift;
        let named = match frame.legacy_key {
            Some(Key::Up) => Some(crate::NamedKey::ArrowUp),
            Some(Key::Down) => Some(crate::NamedKey::ArrowDown),
            Some(Key::Left) => Some(crate::NamedKey::ArrowLeft),
            Some(Key::Right) => Some(crate::NamedKey::ArrowRight),
            Some(Key::Backspace) => Some(crate::NamedKey::Backspace),
            Some(Key::Delete) => Some(crate::NamedKey::Delete),
            Some(Key::Enter) => Some(crate::NamedKey::Enter),
            Some(Key::Tab) => Some(crate::NamedKey::Tab),
            Some(Key::Escape) => Some(crate::NamedKey::Escape),
            _ => None,
        };
        if let Some(named) = named {
            keys.push(KeyEvent::named(named, mods));
        }
        if matches!(frame.legacy_key, Some(Key::Copy | Key::Cut | Key::Paste)) {
            let ch = match frame.legacy_key {
                Some(Key::Copy) => 'c',
                Some(Key::Cut) => 'x',
                _ => 'v',
            };
            if platform == crate::Platform::Mac {
                mods.super_key = true;
            } else {
                mods.ctrl = true;
            }
            keys.push(KeyEvent::character(ch, mods));
        }
        if !frame.chars.is_empty() {
            keys.push(KeyEvent {
                logical: LogicalKey::Unidentified,
                text: Some(frame.chars),
                code: None,
                modifiers: Modifiers::new(),
                repeat: false,
                released: false,
                synthetic: false,
                order: KeyEvent::UNORDERED,
            });
        }
    }
    let mut copied = None;
    let mut clipboard_action = false;
    for event in keys {
        if event.released || event.synthetic {
            continue;
        }
        let composing = world
            .resource::<SourceDocuments>()
            .and_then(|d| d.get(binding.document))
            .is_some_and(crate::editing::SourceSession::is_composing);
        if composing {
            continue;
        }
        if crate::source_extensions::native_completion_key(world, entity, &event) {
            continue;
        }
        if vertical_key(&event, platform).is_some() {
            let Some(doc) = world
                .resource::<SourceDocuments>()
                .and_then(|d| d.get(binding.document))
            else {
                break;
            };
            let text = doc.text().to_string();
            let s = doc.selection();
            let delta = vertical_delta(world, entity, &text, s.head, &event)
                .expect("vertical key already admitted");
            let (head, anchor) = move_vertical(
                world,
                entity,
                &text,
                s.head,
                s.anchor,
                delta,
                event.modifiers.shift,
            );
            let _ = world
                .resource_mut::<SourceDocuments>()
                .unwrap()
                .get_mut(binding.document)
                .unwrap()
                .set_selection(SourceSelection { head, anchor });
            continue;
        }
        world.remove::<VerticalGoal>(entity);
        let result = world
            .resource_mut::<SourceDocuments>()
            .and_then(|d| d.get_mut(binding.document))
            .map(|d| d.native_key(&event, platform, now, &clip, EditOrigin::Keyboard, readonly));
        match result {
            Some(Ok(step)) => {
                if let Some(text) = step.copied {
                    clip = text.clone();
                    copied = Some(text);
                }
                clipboard_action |= step.clipboard_action;
            }
            Some(Err(error)) => {
                world.insert(entity, crate::editing::SourceInputError(error));
            }
            None => {}
        }
    }
    if !frame.commit.is_empty() {
        crate::editing::finish_composition(world, entity);
        let event = KeyEvent {
            logical: LogicalKey::Unidentified,
            text: Some(frame.commit),
            code: None,
            modifiers: Modifiers::new(),
            repeat: false,
            released: false,
            synthetic: false,
            order: KeyEvent::UNORDERED,
        };
        if let Some(Err(error)) = world
            .resource_mut::<SourceDocuments>()
            .and_then(|d| d.get_mut(binding.document))
            .map(|d| d.native_key(&event, platform, now, &clip, EditOrigin::Ime, readonly))
        {
            world.insert(entity, crate::editing::SourceInputError(error));
        }
    }
    crate::editing::project_source(world, entity);
    if clipboard_action {
        note_clipboard(world, copied.as_deref());
    }
    if let Some(input) = world.resource_mut::<Input>() {
        input.keys.clear();
        input.chars.clear();
        input.ime_commit.clear();
        input.key = None;
    }
}

fn edit_queue(world: &mut World, entity: crate::entity::Entity, keys: &[KeyEvent], commit: &str) {
    let platform = platform_of(world);
    let Some(buffer) = world.get::<Buffer>(entity) else {
        return;
    };
    let mut text = buffer.text();
    let mut index = buffer.caret.min(text.len());
    let mut anchor = buffer.anchor.min(text.len());
    let mut history = world.remove::<EditHistory>(entity).unwrap_or_default();
    let mut clip = world
        .resource::<Clipboard>()
        .map(|clipboard| clipboard.text.clone())
        .unwrap_or_default();
    let mut copied = None;
    let mut clipboard_action = false;
    let now = crate::typing::frame_now(world);
    for event in keys {
        if !event.released && !event.synthetic {
            if let Some(delta) = vertical_delta(world, entity, &text, index, event) {
                history.break_run();
                (index, anchor) = move_vertical(
                    world,
                    entity,
                    &text,
                    index,
                    anchor,
                    delta,
                    event.modifiers.shift,
                );
                continue;
            }
            world.remove::<VerticalGoal>(entity);
        }
        let step = apply_text_event(
            &mut EditSurface {
                text: &mut text,
                index: &mut index,
                anchor: &mut anchor,
            },
            event,
            platform,
            true,
            now,
            &mut history,
            &clip,
        );
        if let Some(text) = step.copied.clone() {
            clip = text.clone();
            copied = Some(text);
        }
        clipboard_action |= step.clipboard_action;
    }
    if !commit.is_empty() {
        history.break_run();
        let commit_event = KeyEvent {
            logical: LogicalKey::Unidentified,
            text: Some(commit.to_string()),
            code: None,
            modifiers: Modifiers::new(),
            repeat: false,
            released: false,
            synthetic: false,
            order: KeyEvent::UNORDERED,
        };
        let _ = apply_text_event(
            &mut EditSurface {
                text: &mut text,
                index: &mut index,
                anchor: &mut anchor,
            },
            &commit_event,
            platform,
            true,
            now,
            &mut history,
            &clip,
        );
        history.break_run();
    }
    if let Some(buffer) = world.get_mut::<Buffer>(entity) {
        replace_buffer(buffer, &text, index, anchor);
    }
    world.insert(entity, history);
    if clipboard_action {
        note_clipboard(world, copied.as_deref());
    }
    if let Some(input) = world.resource_mut::<Input>() {
        input.keys.clear();
        input.chars.clear();
        input.ime_commit.clear();
        input.key = None;
    }
}

pub(crate) fn replace_buffer(buffer: &mut Buffer, text: &str, caret: usize, anchor: usize) {
    if buffer.snapshot().as_ref() != text {
        let len = buffer.len();
        buffer.delete_range(0, len);
        buffer.insert_str(0, text);
    }
    let len = buffer.len();
    buffer.caret = caret.min(len);
    buffer.anchor = anchor.min(len);
}

fn sync_ime_preview(world: &mut World, entity: crate::entity::Entity) {
    if world.has::<crate::structured_native::StructuredBinding>(entity)
        && !crate::structured_native::is_composing(world, entity)
    {
        world.remove::<crate::ImePreview>(entity);
        return;
    }
    if let Some(binding) = world.get::<crate::editing::SourceBinding>(entity) {
        let composing = world
            .resource::<crate::editing::SourceDocuments>()
            .and_then(|docs| docs.get(binding.document))
            .is_some_and(|doc| doc.is_composing() && !doc.is_readonly());
        if !composing {
            world.remove::<crate::ImePreview>(entity);
            return;
        }
    }
    let preedit = world
        .resource::<Input>()
        .map(|input| input.ime_preedit.clone())
        .unwrap_or_default();
    if preedit.is_empty() {
        world.remove::<crate::field::ImePreview>(entity);
    } else {
        let cursor = world.resource::<Input>().and_then(|input| input.ime_cursor);
        world.insert(
            entity,
            crate::field::ImePreview {
                text: preedit,
                cursor,
            },
        );
    }
}

// Returns whether a successful press needs the usual caret reveal (including
// grapheme repair at scalar-wrap edges). Held/release frames use bounded scroll.
fn place_from_pointer(world: &mut World, entity: crate::entity::Entity) -> bool {
    let Some(input) = world.resource::<Input>() else {
        world.remove::<EditorDrag>(entity);
        return false;
    };
    let pointer = input.pointer;
    let shift = input.shift;
    let mut previous = world.remove::<EditorDrag>(entity);
    if crate::events::<crate::window::WindowFocus>(world)
        .contains(&crate::window::WindowFocus::Blurred)
    {
        return false;
    }
    let now = world
        .resource::<crate::Time>()
        .map_or(std::time::Duration::ZERO, |time| time.now);
    if pointer.button != crate::PointerButton::Left
        || !pointer.position.iter().all(|v| v.is_finite())
    {
        return false;
    }
    let Some(rect) = world.get::<Rect>(entity).copied().filter(|r| r.h > 0.0) else {
        return false;
    };
    let start = pointer.press_at.unwrap_or(pointer.position);
    let starting = pointer.pressed
        && start.iter().all(|v| v.is_finite())
        && hit_test(world, start[0], start[1])
            .is_some_and(|hit| ancestor_with::<Editor>(world, hit) == Some(entity));
    let continuing = !pointer.pressed
        && previous.as_ref().is_some_and(|drag| drag.id == pointer.id)
        && (pointer.down || pointer.released);
    if !starting && !continuing {
        return false;
    }
    let Some(buffer) = world.get::<Buffer>(entity) else {
        return false;
    };
    let text = buffer.snapshot();
    let old_anchor = buffer.anchor;
    let mut layout = document_rows(world, entity, &text);
    // Structured paragraphs are shaped at their indented width and the caret
    // edges are shifted. Hit testing has to use that layout, not the plain
    // source paragraph, or a click on the painted indent lands too far right.
    let mut indented = false;
    if world.has::<crate::structured_chrome::StructuredChromeGutter>(entity) {
        let style = crate::text::style_for(world, entity);
        let engine = crate::text::text_engine(world);
        let width = world.get::<Rect>(entity).map_or(f32::INFINITY, |rect| {
            crate::font::field_text_width(rect.w, engine.unit_advance(&style))
        });
        let spans = world
            .get::<crate::Spans>(entity)
            .map(|spans| spans.runs.clone())
            .unwrap_or_default();
        if let Some(mut canonical) =
            crate::structured_chrome::text_layout(world, entity, &spans, &text, width, (0, 0, 0))
        {
            canonical.glyphs.clear();
            canonical.highlights.clear();
            layout = std::sync::Arc::new(canonical);
            indented = true;
        }
    }
    if layout.lines.is_empty() {
        return false;
    }
    world.remove::<VerticalGoal>(entity);
    if let Some(history) = world.get_mut::<EditHistory>(entity) {
        history.break_run();
    }
    let offset = world
        .get::<Editor>(entity)
        .map_or(0.0, |editor| editor.offset);
    let offset = if offset.is_finite() {
        offset.max(0.0)
    } else {
        0.0
    };
    let plain = std::sync::Arc::clone(&world.get::<DocumentRows>(entity).expect("rows").index);
    let grid = crate::structured_chrome::has_tables(world, entity);
    let layout_hit = grid || indented;
    let mut first = if layout_hit {
        layout
            .lines
            .iter()
            .enumerate()
            .filter(|(_, line)| line.y <= offset)
            .max_by(|(_, a), (_, b)| a.y.total_cmp(&b.y))
            .map_or(0, |(i, _)| i)
    } else {
        plain.row_at_y(offset)
    };
    let initial_first = first;
    let available = (rect.h - 2.0 * TEXT_ORIGIN_Y).max(1.0);
    let map = |world: &mut World, position: [f32; 2], first: usize| {
        let x = position[0] - rect.x - TEXT_ORIGIN_X;
        let local = (position[1] - rect.y - TEXT_ORIGIN_Y).clamp(0.0, (available - 0.001).max(0.0));
        if layout_hit {
            let y = local
                + if grid && first == initial_first {
                    offset
                } else {
                    layout.lines[first].y
                };
            let row = layout.line_at_point(x, y);
            let index = floor_grapheme(&text, layout.index_at(x, y).min(text.len()));
            // Preserve B001's preferred-column contract: subsequent Up/Down
            // starts at the actual caret stop, not a distant off-text pointer x.
            return (index, row, layout.caret_xy_on_line(index, row).0);
        }
        let y = local + plain.line(first).y;
        let row = plain.row_at_y(y);
        let line = shaped_row(world, entity, &text, row);
        let laid = crate::TextLayout::shaped(Vec::new(), vec![line], 0.0, 0.0);
        let index = floor_grapheme(&text, laid.index_on_line(0, x).min(text.len()));
        (index, row, laid.caret_xy_on_line(index, 0).0)
    };
    let anchor = if starting {
        if shift {
            floor_grapheme(&text, old_anchor.min(text.len()))
        } else {
            map(world, start, first).0
        }
    } else {
        floor_grapheme(
            &text,
            previous
                .as_ref()
                .expect("continuing drag")
                .anchor
                .min(text.len()),
        )
    };
    // Fixed elapsed-time rate, independent of delivered pointer/redraw count.
    // Entering/reversing the edge starts a fresh period; release maps its final
    // coordinate without scrolling. Limit catch-up after a stalled UI to 5 rows.
    let direction = if pointer.position[1] < rect.y {
        -1
    } else if pointer.position[1] >= rect.y + rect.h {
        1
    } else {
        0
    };
    let mut next_step = previous
        .as_ref()
        .filter(|drag| continuing && drag.direction == direction && direction != 0)
        .map_or(now.saturating_add(DRAG_ROW_PERIOD), |drag| drag.next_step);
    if continuing && pointer.down && !pointer.released && direction != 0 && now >= next_step {
        let elapsed = now.saturating_sub(next_step);
        let due = 1 + elapsed.as_nanos() / DRAG_ROW_PERIOD.as_nanos();
        let steps = due.min(5) as usize;
        first = if grid {
            let mut ys: Vec<_> = layout.lines.iter().map(|l| l.y).collect();
            ys.sort_by(f32::total_cmp);
            ys.dedup();
            let current = ys.iter().rposition(|y| *y <= offset).unwrap_or(0);
            let target = if direction < 0 {
                current.saturating_sub(steps)
            } else {
                (current + steps).min(ys.len() - 1)
            };
            layout
                .lines
                .iter()
                .position(|l| l.y == ys[target])
                .unwrap_or(first)
        } else if direction < 0 {
            first.saturating_sub(steps)
        } else {
            let limit = if layout_hit {
                layout.lines.len()
            } else {
                plain.line_count()
            };
            first.saturating_add(steps).min(limit.saturating_sub(1))
        };
        next_step = if due > 5 {
            now.saturating_add(DRAG_ROW_PERIOD)
        } else {
            next_step.saturating_add(DRAG_ROW_PERIOD * steps as u32)
        };
    }
    let limit = if layout_hit {
        layout.lines.len()
    } else {
        plain.line_count()
    };
    let can_scroll = (direction < 0 && first > 0) || (direction > 0 && first + 1 < limit);
    if let Some(editor) = world.get_mut::<Editor>(entity) {
        editor.offset = if grid && first == initial_first {
            offset
        } else if layout_hit {
            layout.lines[first].y
        } else {
            plain.line(first).y
        };
    }
    let (index, row, x) = map(world, pointer.position, first);
    if let Some(buffer) = world.get_mut::<Buffer>(entity) {
        buffer.caret = index;
        buffer.anchor = anchor;
    }
    world.insert(entity, VerticalGoal { x, index, row });
    if pointer.down && !pointer.released {
        // Arming a timer wakes the native loop. Reuse the existing future
        // timer on unrelated redraws, or each wake would rearm another wake
        // forever even though no elapsed-time scroll step was due.
        let wake = if can_scroll {
            previous
                .as_mut()
                .filter(|drag| drag.direction == direction && drag.next_step == next_step)
                .and_then(|drag| drag._wake.take())
                .or_else(|| {
                    Some(crate::runtime::call_after(
                        world,
                        next_step.saturating_sub(now),
                        |_| {},
                    ))
                })
        } else {
            None
        };
        world.insert(
            entity,
            EditorDrag {
                id: pointer.id,
                anchor,
                direction,
                next_step,
                _wake: wake,
            },
        );
    }
    starting
}

/// Milli-points in one logical point. Matches the virtual-list scroll identity.
const MILLI_PER_POINT: i64 = 1000;

/// Scroll identity beside the source-byte anchor: distance into that row.
/// `published` is the last offset this view wrote. A different [`Editor::offset`]
/// is paint input and clears the remainder.
struct ScrollRemainder {
    published: f32,
    remainder_mp: i64,
}
impl crate::Component for ScrollRemainder {}

fn points_to_mp(points: f32) -> i64 {
    if !points.is_finite() || points <= 0.0 {
        return 0;
    }
    let milli = f64::from(points) * MILLI_PER_POINT as f64;
    if !milli.is_finite() {
        return 0;
    }
    milli.round().clamp(0.0, i64::MAX as f64) as i64
}

fn mp_to_points(mp: i64) -> f32 {
    (mp.max(0) as f64 / MILLI_PER_POINT as f64) as f32
}

/// One milli-point short of the row height, so the remainder stays on this row.
fn remainder_limit(row_h: f32) -> i64 {
    points_to_mp(row_h).saturating_sub(1).max(0)
}

fn place_anchored_row(world: &mut World, entity: crate::Entity, y: f32, h: f32) {
    let remainder_mp = world
        .get::<ScrollRemainder>(entity)
        .map(|state| state.remainder_mp)
        .unwrap_or(0)
        .clamp(0, remainder_limit(h));
    if let Some(editor) = world.get_mut::<Editor>(entity) {
        editor.offset = y;
    }
    world.insert(
        entity,
        ScrollRemainder {
            published: y,
            remainder_mp,
        },
    );
}

fn finish_scroll_publication(
    world: &mut World,
    entity: crate::Entity,
    top: f32,
    row_h: f32,
    anchored_top: f32,
) {
    let incoming = world.get::<Editor>(entity).map(|editor| editor.offset);
    let keep = world.get::<ScrollRemainder>(entity).is_some_and(|state| {
        incoming.is_some_and(|offset| state.published.to_bits() == offset.to_bits())
            && top.to_bits() == anchored_top.to_bits()
    });
    let remainder_mp = if keep {
        world
            .get::<ScrollRemainder>(entity)
            .map(|state| state.remainder_mp)
            .unwrap_or(0)
    } else {
        0
    }
    .clamp(0, remainder_limit(row_h));
    if let Some(editor) = world.get_mut::<Editor>(entity) {
        editor.offset = top;
    }
    world.insert(
        entity,
        ScrollRemainder {
            published: top,
            remainder_mp,
        },
    );
}

pub(crate) fn reset_editor_wheel(world: &mut World, entity: crate::Entity) {
    world.remove::<ScrollRemainder>(entity);
}

// Return the bounded signed consumption for ancestor wheel chaining. The
// published offset snaps to a row top. The milli-point remainder is the
// position inside that row and survives a later reflow.
pub(crate) fn scroll_editor_wheel(world: &mut World, entity: crate::Entity, delta: f32) -> f32 {
    if !delta.is_finite() || delta == 0.0 {
        return 0.0;
    }
    let Some((content, page)) = editor_scroll_metrics(world, entity) else {
        return 0.0;
    };
    let Some(editor) = world.get::<Editor>(entity) else {
        return 0.0;
    };
    let max = (content - page).max(0.0);
    let offset = editor.offset.clamp(0.0, max);
    let carried = world.get::<ScrollRemainder>(entity).and_then(|state| {
        (state.published.to_bits() == offset.to_bits()).then_some(state.remainder_mp)
    });
    let before = match carried {
        Some(mp) => (offset + mp_to_points(mp)).clamp(0.0, max),
        None => offset,
    };
    let requested = (before - delta).clamp(0.0, max);
    // Plain rows are stacked in increasing y. Table cells are not, so those
    // views keep the spatial maximum.
    let monotonic = !crate::structured_chrome::has_tables(world, entity);
    let (published, row_h) = if monotonic {
        let index = std::sync::Arc::clone(&world.get::<DocumentRows>(entity).unwrap().index);
        let published = index.top_at_most(requested);
        let row_h = index.line(index.row_at_y(published)).h;
        (published, row_h)
    } else {
        let rows = &world.get::<DocumentRows>(entity).unwrap().layout.lines;
        let published = row_top_at_most(rows, requested, false);
        let row_h = rows
            .iter()
            .find(|row| row.y.to_bits() == published.to_bits())
            .map(|row| row.h)
            .unwrap_or(0.0);
        (published, row_h)
    };
    let remainder_mp =
        points_to_mp((requested - published).max(0.0)).clamp(0, remainder_limit(row_h));
    world.get_mut::<Editor>(entity).unwrap().offset = published;
    world.insert(
        entity,
        ScrollRemainder {
            published,
            remainder_mp,
        },
    );
    if requested != before {
        crate::focus::pause_focus_reveal(world);
    }
    before - requested
}

/// Cached canonical row range for editor scrollbar controls. The editor permits
/// the final row at the viewport top (also used by wheel/selection dragging),
/// so the virtual extent includes the trailing viewport, not just ink height.
/// Returns None until layout has published the document rows.
pub(crate) fn editor_scroll_metrics(world: &World, entity: crate::Entity) -> Option<(f32, f32)> {
    let monotonic = !crate::structured_chrome::has_tables(world, entity);
    let rows = world.get::<DocumentRows>(entity)?;
    let rect = world.get::<Rect>(entity)?;
    let page = (rect.h - 2.0 * TEXT_ORIGIN_Y).max(1.0);
    let maximum = if monotonic {
        (rows.index.line_count() > 0).then(|| rows.index.max_top)
    } else {
        max_row_y(&rows.layout.lines, false)
    }?;
    Some((maximum + page, page))
}

/// Largest row top that is still `<= requested`. No such row yields `0`.
///
/// `monotonic` is the plain-text stack: row tops never decrease, including
/// after fold projection. The search is then the last row with
/// `y <= requested`. Spatial layouts pass `false` and keep a full scan.
fn row_top_at_most(lines: &[crate::TextLine], requested: f32, monotonic: bool) -> f32 {
    if !monotonic {
        return lines
            .iter()
            .filter(|row| row.y <= requested)
            .map(|row| row.y)
            .reduce(f32::max)
            .unwrap_or(0.0);
    }
    let index = lines.partition_point(|line| line.y <= requested);
    if index == 0 {
        0.0
    } else {
        lines[index - 1].y
    }
}

/// Bottom of the scroll extent is the greatest row top. On a plain stack that
/// is the last row. Empty geometry returns None, matching the previous scan.
fn max_row_y(lines: &[crate::TextLine], monotonic: bool) -> Option<f32> {
    if monotonic {
        lines.last().map(|line| line.y)
    } else {
        lines.iter().map(|line| line.y).reduce(f32::max)
    }
}

pub(crate) fn access_caret_row(
    world: &World,
    entity: crate::Entity,
    layout: &crate::TextLayout,
    index: usize,
) -> usize {
    if let Some(goal) = world
        .get::<VerticalGoal>(entity)
        .filter(|goal| goal.index == index && row_contains(layout, goal.row, index))
    {
        return goal.row;
    }
    document_row_at(layout, index)
}

pub(crate) fn reveal_access_selection(world: &mut World, entity: crate::Entity) {
    world.remove::<VerticalGoal>(entity);
    world.remove::<EditorDrag>(entity);
    publish_window(world, entity, true);
}

// Same downstream shared-edge affinity as font::line_col, without allocating
// a full-document range vector or scanning rows before each idle publication.
fn document_row_at(layout: &crate::TextLayout, index: usize) -> usize {
    let mut low = 0;
    let mut high = layout.lines.len();
    while low < high {
        let mid = low + (high - low) / 2;
        let line = &layout.lines[mid];
        let before = index >= line.start
            && (line.end < index
                || (line.end == index
                    && layout
                        .lines
                        .get(mid + 1)
                        .is_some_and(|next| next.start == index)));
        if before {
            low = mid + 1;
        } else {
            high = mid;
        }
    }
    low.min(layout.lines.len().saturating_sub(1))
}

// Grid cells are source-ordered but not y-ordered. Keep their disposable text
// whole and clip canonical geometry rather than slicing through a visual row.
fn publish_grid_window(
    world: &mut World,
    entity: crate::Entity,
    text: &str,
    layout: &crate::TextLayout,
    reveal: bool,
) {
    let height = world.get::<Rect>(entity).map_or(0.0, |r| r.h);
    let available = (height - 2.0 * TEXT_ORIGIN_Y).max(1.0);
    let Some(buffer) = world.get::<Buffer>(entity) else {
        return;
    };
    let caret = buffer.caret.min(text.len());
    let anchor = buffer.anchor.min(text.len());
    let row = document_row_at(layout, caret);
    let Some(line) = layout.lines.get(row) else {
        return;
    };
    let maximum = layout.lines.iter().map(|l| l.y).fold(0.0, f32::max);
    let mut offset = world.get::<Editor>(entity).map_or(0.0, |e| e.offset);
    if !offset.is_finite() {
        offset = 0.0;
    }
    if reveal {
        if line.y < offset {
            offset = line.y;
        }
        if line.y + line.h > offset + available {
            offset = line.y + line.h - available;
        }
    }
    offset = offset.clamp(0.0, maximum);
    if let Some(editor) = world.get_mut::<Editor>(entity) {
        editor.offset = offset;
    }
    world.insert(
        entity,
        ScrollRemainder {
            published: offset,
            remainder_mp: 0,
        },
    );
    world.insert(
        entity,
        EditorWindow {
            base: 0,
            end: text.len(),
            hidden: vec![],
        },
    );
    if let Some(label) = world.get_mut::<Label>(entity) {
        label.text = text.into();
    }
    if height > 0.0 && line.y + line.h > offset && line.y < offset + available {
        world.insert(entity, crate::CaretRow { index: caret, row });
        world.insert(
            entity,
            Caret {
                index: caret,
                anchor,
            },
        );
    } else {
        world.remove::<Caret>(entity);
        world.remove::<crate::CaretRow>(entity);
    }
}

pub(crate) fn reveal_selection(world: &mut World, entity: crate::Entity) {
    publish_window(world, entity, true);
}

fn publish_window(world: &mut World, entity: crate::entity::Entity, reveal: bool) {
    let height = world.get::<Rect>(entity).map(|rect| rect.h).unwrap_or(0.0);
    let Some(buffer) = world.get::<Buffer>(entity) else {
        return;
    };
    let text = buffer.snapshot();
    let layout = document_rows(world, entity, &text);
    if crate::structured_chrome::has_tables(world, entity) {
        publish_grid_window(world, entity, &text, &layout, reveal);
        return;
    }
    // Reflow/edit invalidation may have restored the source-byte scroll anchor.
    let offset = world
        .get::<Editor>(entity)
        .map_or(0.0, |editor| editor.offset);
    let buffer = world.get::<Buffer>(entity).expect("buffer exists");
    let caret = floor_grapheme(&text, buffer.caret.min(text.len()));
    let anchor = floor_grapheme(&text, buffer.anchor.min(text.len()));
    let available = (height - 2.0 * TEXT_ORIGIN_Y).max(1.0);
    let rows_index = std::sync::Arc::clone(&world.get::<DocumentRows>(entity).expect("rows").index);
    let affinity = world
        .get::<VerticalGoal>(entity)
        .filter(|goal| goal.index == caret && index_holds(&rows_index, goal.row, caret));
    let (base, end, top, visible, numbers, first, caret_row) = {
        let offset = if offset.is_finite() {
            offset.max(0.0)
        } else {
            0.0
        };
        let mut first = rows_index.row_at_y(offset);
        let caret_row = affinity.map_or_else(|| rows_index.row_at_byte(caret), |goal| goal.row);
        if reveal && height > 0.0 && rows_index.line_count() > 0 {
            if caret_row < first {
                first = caret_row;
            }
            let caret_line = rows_index.line(caret_row);
            let needed = caret_line.y + caret_line.h - available;
            // First visual row whose top is at or below the caret only as far
            // as the viewport still contains the caret line (y >= needed).
            let reveal_row = if needed <= 0.0 {
                0
            } else {
                let containing = rows_index.row_at_y(needed);
                let top = rows_index.line(containing).y;
                if top < needed {
                    containing.saturating_add(1)
                } else {
                    containing
                }
            };
            first = first.max(reveal_row.min(caret_row));
        }
        let top = rows_index.line(first).y;
        let last = rows_index
            .row_at_y((top + available - 0.001).max(top))
            .max(first);
        let base = rows_index.line(first).start;
        let end = if last + 1 < rows_index.line_count() {
            rows_index.line(last + 1).start
        } else {
            text.len()
        };
        let visible = height > 0.0
            && caret_row >= first
            && caret_row <= last
            && caret >= base
            && caret <= end;
        let mut numbers = String::new();
        for row in first..=last {
            if row > first {
                numbers.push('\n');
            }
            if rows_index.line_count() > row {
                let line = rows_index.line(row);
                let logical = buffer
                    .lines
                    .partition_point(|start| *start <= line.start)
                    .max(1)
                    + usize::from(line.start == text.len() && text.ends_with('\n'));
                if row == 0 || text.as_bytes().get(line.start.wrapping_sub(1)) == Some(&b'\n') {
                    numbers.push_str(&logical.to_string());
                }
            }
        }
        (base, end, top, visible, numbers, first, caret_row)
    };
    let hidden: Vec<_> = crate::source_folding::current(world, entity)
        .map(|folds| {
            folds
                .ranges
                .iter()
                .filter_map(|r| {
                    let start = r.start.max(base);
                    let stop = r.end.min(end);
                    (start < stop).then_some(start..stop)
                })
                .collect()
        })
        .unwrap_or_default();
    let mut window = String::with_capacity(end - base);
    let mut cursor = base;
    for range in &hidden {
        window.push_str(&text[cursor..range.start]);
        cursor = range.end;
    }
    window.push_str(&text[cursor..end]);
    let row_h = if rows_index.line_count() > first {
        rows_index.line(first).h
    } else {
        0.0
    };
    let sane = if offset.is_finite() {
        offset.max(0.0)
    } else {
        0.0
    };
    let anchored_top = if rows_index.line_count() == 0 {
        0.0
    } else {
        rows_index.line(rows_index.row_at_y(sane)).y
    };
    finish_scroll_publication(world, entity, top, row_h, anchored_top);
    if let Some(buffer) = world.get_mut::<Buffer>(entity) {
        buffer.caret = caret;
        buffer.anchor = anchor;
    }
    if let Some(binding) = world.get::<crate::editing::SourceBinding>(entity).copied() {
        if let Some(doc) = world
            .resource_mut::<crate::editing::SourceDocuments>()
            .and_then(|d| d.get_mut(binding.document))
        {
            if doc.revision() == binding.revision {
                doc.set_view_scroll_anchor(entity, base);
            }
        }
    }
    world.insert(entity, EditorWindow { base, end, hidden });
    let paint_caret = window_offset(world, entity, caret);
    let paint_anchor = window_offset(world, entity, anchor);
    if let Some(label) = world.get_mut::<Label>(entity) {
        label.text = window;
    }
    if visible {
        world.insert(
            entity,
            crate::CaretRow {
                index: paint_caret,
                row: caret_row - first,
            },
        );
        world.insert(
            entity,
            Caret {
                index: paint_caret,
                anchor: paint_anchor,
            },
        );
    } else {
        world.remove::<Caret>(entity);
        world.remove::<crate::CaretRow>(entity);
    }
    if let Some(gutter) = gutter_for(world, entity) {
        if let Some(label) = world.get_mut::<Label>(gutter) {
            label.text = numbers;
        }
    }
}

pub(crate) fn window_base(world: &World, entity: crate::entity::Entity) -> usize {
    world
        .get::<EditorWindow>(entity)
        .map_or(0, |window| window.base)
}

// Convert document source positions into the non-contiguous visible Label.
// Hidden interiors collapse to their seam; this maps cross-fold selections
// without painting the hidden bytes. Input hit testing uses document rows.
pub(crate) fn window_offset(world: &World, entity: crate::Entity, byte: usize) -> usize {
    let Some(window) = world.get::<EditorWindow>(entity) else {
        return byte;
    };
    let byte = byte.clamp(window.base, window.end);
    let removed: usize = window
        .hidden
        .iter()
        .map(|r| byte.min(r.end).saturating_sub(r.start))
        .sum();
    byte - window.base - removed
}

/// Visible selection fragments for document-byte source positions. Folded
/// interiors and offscreen ranges produce no fragments. Coordinates are
/// relative to the visible Label text origin, not the viewport rectangle.
pub fn source_range_boxes(
    world: &World,
    entity: crate::Entity,
    start: usize,
    end: usize,
) -> Vec<crate::font::SelectionBox> {
    let Some(layout) = world.get::<crate::TextLayout>(entity) else {
        return Vec::new();
    };
    layout.selection_boxes(
        window_offset(world, entity, start),
        window_offset(world, entity, end),
    )
}

/// Visible caret rectangle for one document-byte source position. Hidden and
/// offscreen positions return None rather than attaching to a fold seam.
pub fn source_position_box(
    world: &World,
    entity: crate::Entity,
    at: usize,
) -> Option<crate::font::SelectionBox> {
    let window = world.get::<EditorWindow>(entity)?;
    if at < window.base || at > window.end || window.hidden.iter().any(|r| r.contains(&at)) {
        return None;
    }
    let layout = world.get::<crate::TextLayout>(entity)?;
    let at = window_offset(world, entity, at);
    let row = document_row_at(layout, at);
    let line = layout.lines.get(row)?;
    let (x, y) = layout.caret_xy_on_line(at, row);
    Some(crate::font::SelectionBox {
        x,
        y,
        w: 1.0,
        h: line.h,
    })
}

/// Copy the entity's styled spans for shaping or painting `text`.
///
/// On a [`Buffer`] + [`Editor`] entity, spans use document byte offsets. When
/// `text` is the published [`Label`] window this clips and rebases those spans
/// into label-relative offsets. For full document text or a non-editor entity
/// it returns the source spans unchanged. Source components are never mutated.
pub fn spans_for_text(
    world: &World,
    entity: crate::entity::Entity,
    text: &str,
) -> Vec<crate::TextSpan> {
    let spans = world
        .get::<crate::Spans>(entity)
        .map(|spans| spans.runs.as_slice())
        .unwrap_or(&[]);
    if !world.has::<Editor>(entity)
        || !world.has::<Buffer>(entity)
        || !world
            .get::<Label>(entity)
            .is_some_and(|label| label.text == text)
    {
        return spans.to_vec();
    }
    let base = window_base(world, entity);
    let end = world
        .get::<EditorWindow>(entity)
        .map_or(base.saturating_add(text.len()), |w| w.end);
    spans
        .iter()
        .filter_map(|span| {
            if span.start >= end || span.end <= base {
                return None;
            }
            let mut span = span.clone();
            span.start = window_offset(world, entity, span.start.max(base));
            span.end = window_offset(world, entity, span.end.min(end));
            (span.start < span.end).then_some(span)
        })
        .collect()
}

fn gutter_for(world: &World, entity: crate::entity::Entity) -> Option<crate::entity::Entity> {
    let parent = world.get::<Parent>(entity)?.entity;
    children_of(world, parent)
        .into_iter()
        .find(|child| world.has::<Gutter>(*child))
}

impl Component for Decorations {}
impl Component for DecorationMarks {}

/// How a [`Decoration`] is painted over the visible text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecorationKind {
    /// Quad covering each selection box of the span.
    Find,
    /// Two-point bar along the bottom of the span.
    Diagnostic,
}

/// Byte range to mark on an entity that also has [`Decorations`].
///
/// With a [`Buffer`], `start` and `end` are document offsets. Without one,
/// they are offsets into the [`crate::Label`]. The span is clipped to the
/// visible window, and a range wholly outside it produces no
/// [`DecorationMark`]. Either order paints; `start == end` paints nothing.
#[derive(Clone, Copy, Debug)]
pub struct Decoration {
    /// One end of the span, in bytes.
    pub start: usize,
    /// The other end of the span, in bytes. Exclusive after the ends are
    /// ordered.
    pub end: usize,
    /// Paint style for this range.
    pub kind: DecorationKind,
}

/// Ranges turned into [`DecorationMarks`] after text layout.
///
/// [`crate::App::update`] rebuilds the marks for every entity that has this
/// component. An entity with no text layout gets an empty mark list.
#[derive(Clone, Debug, Default)]
pub struct Decorations {
    /// Ranges in paint order. One range can yield several marks, one per
    /// visible fragment.
    pub list: Vec<Decoration>,
}

/// Quads relative to the text origin. Empty when the range is offscreen.
///
/// Coordinates are logical points from the label's text origin, not from the
/// entity rectangle. A mix of ranges keeps only the fragments that intersect
/// the visible window.
#[derive(Clone, Debug, Default)]
pub struct DecorationMarks {
    /// One quad per visible fragment, in [`Decorations::list`] order.
    pub marks: Vec<DecorationMark>,
}

/// One painted fragment of a [`Decoration`], in logical points.
#[derive(Clone, Copy, Debug)]
pub struct DecorationMark {
    /// Left edge, relative to the text origin.
    pub x: f32,
    /// Top edge, relative to the text origin.
    pub y: f32,
    /// Width. At least 1 point.
    pub w: f32,
    /// Height of the fragment. A [`DecorationKind::Diagnostic`] is 2 points.
    pub h: f32,
    /// Which decoration produced this quad.
    pub kind: DecorationKind,
}

pub fn apply_decorations(world: &mut World) {
    let entities = world.entities_with::<Decorations>();
    for entity in entities {
        let marks = decoration_marks(world, entity);
        world.insert(entity, DecorationMarks { marks });
    }
}

fn decoration_marks(world: &World, entity: crate::entity::Entity) -> Vec<DecorationMark> {
    let Some(layout) = world.get::<crate::font::TextLayout>(entity) else {
        return Vec::new();
    };
    let window_len = world
        .get::<Label>(entity)
        .map(|label| label.text.len())
        .unwrap_or(0);
    let base = if world.has::<Buffer>(entity) {
        window_base(world, entity)
    } else {
        0
    };
    let list = world
        .get::<Decorations>(entity)
        .map(|decorations| decorations.list.as_slice())
        .unwrap_or(&[]);
    let mut marks = Vec::new();
    for decoration in list {
        let document_end = world
            .get::<EditorWindow>(entity)
            .map_or(base + window_len, |w| w.end);
        if decoration.end <= base || decoration.start >= document_end {
            continue;
        }
        let (start, end) = if world.has::<Buffer>(entity) {
            (
                window_offset(world, entity, decoration.start),
                window_offset(world, entity, decoration.end),
            )
        } else {
            (
                decoration.start.min(window_len),
                decoration.end.min(window_len),
            )
        };
        if start >= end {
            continue;
        }
        for span in layout.selection_boxes(start, end) {
            let (y, h) = match decoration.kind {
                DecorationKind::Find => (span.y, span.h),
                DecorationKind::Diagnostic => (span.y + span.h - 2.0, 2.0),
            };
            marks.push(DecorationMark {
                x: span.x,
                y,
                w: span.w.max(1.0),
                h,
                kind: decoration.kind,
            });
        }
    }
    marks
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn buffer_snapshots_reuse_until_bytes_change_and_clones_detach() {
        let mut buffer = Buffer::from_str("é\nsource");
        let first = buffer.snapshot();
        assert!(Arc::ptr_eq(&first, &buffer.snapshot()));
        buffer.caret = 0;
        buffer.insert_str(0, "");
        buffer.delete_range(1, 1);
        assert!(Arc::ptr_eq(&first, &buffer.snapshot()));
        let mut clone = buffer.clone();
        clone.insert_str(2, "!");
        assert_eq!(clone.snapshot().as_ref(), "é!\nsource");
        assert_eq!(first.as_ref(), buffer.text());
        assert!(!Arc::ptr_eq(&first, &clone.snapshot()));
        buffer.delete_range(0, 1); // Preserve lossy legacy mid-scalar decoding.
        assert_eq!(buffer.snapshot().as_ref(), buffer.slice(0, buffer.len()));
        assert!(!Arc::ptr_eq(&first, &buffer.snapshot()));
    }

    #[test]
    fn binary_document_row_matches_public_line_col_at_all_bytes() {
        let world = World::new();
        let engine = crate::text::text_engine(&world);
        let style = crate::TextStyle::default();
        for text in [
            "",
            "\n",
            "a\n\nβ\r\n",
            "é 👩‍💻 العربية\nend",
            "long long long line\nnext",
        ] {
            for width in [20.0, 200.0, f32::INFINITY] {
                let layout = engine.layout(text, &style, &[], width);
                let ranges: Vec<_> = layout.lines.iter().map(|l| (l.start, l.end)).collect();
                for index in 0..=text.len() + 1 {
                    assert_eq!(
                        document_row_at(&layout, index),
                        crate::font::line_col(&ranges, index).0,
                        "{text:?} {width} {index}"
                    );
                }
            }
        }
    }

    #[test]
    fn document_rows_cache_geometry_and_invalidate_for_text_width_style_and_engine() {
        let mut world = World::new();
        crate::text::ensure_text_engine(&mut world);
        let entity = world.spawn((
            Editor::default(),
            Label::new(""),
            Rect {
                x: 0.0,
                y: 0.0,
                w: 280.0,
                h: 80.0,
            },
        ));
        let text = "a".repeat(100);
        let original = document_rows(&mut world, entity, &text);
        assert!(original.glyphs.is_empty());
        assert!(original.lines.len() > 4);
        for _ in 0..3 {
            crate::text::text_engine(&world).begin_frame();
            let cached = document_rows(&mut world, entity, &text);
            assert!(Arc::ptr_eq(&original, &cached));
        }
        let changed = document_rows(&mut world, entity, &(text.clone() + "x"));
        assert!(!Arc::ptr_eq(&original, &changed));
        world.get_mut::<Rect>(entity).unwrap().w = 100.0;
        let narrow = document_rows(&mut world, entity, &text);
        assert!(narrow.lines.len() > original.lines.len());
        world.insert(
            entity,
            crate::TextStyle {
                size: 8.0,
                ..crate::TextStyle::default()
            },
        );
        let styled = document_rows(&mut world, entity, &text);
        assert!(!Arc::ptr_eq(&narrow, &styled));
        assert_ne!(narrow.lines[0].h, styled.lines[0].h);
        let replacement: Arc<dyn crate::TextEngine> = Arc::new(crate::BuiltinText::new());
        world.insert_resource(replacement);
        let replaced = document_rows(&mut world, entity, &text);
        assert!(!Arc::ptr_eq(&styled, &replaced));
    }

    fn line_at(y: f32) -> crate::TextLine {
        crate::TextLine {
            start: 0,
            end: 0,
            y,
            h: 16.0,
            baseline: 12.0,
            underline_thickness: 1.0,
            underline_offset: 1.0,
            width: 10.0,
            edges: Vec::new(),
        }
    }

    fn linear_top(lines: &[crate::TextLine], requested: f32) -> f32 {
        lines
            .iter()
            .filter(|row| row.y <= requested)
            .map(|row| row.y)
            .reduce(f32::max)
            .unwrap_or(0.0)
    }

    #[test]
    fn plain_row_query_matches_a_full_scan_and_spatial_rows_keep_the_scan() {
        let stacked: Vec<_> = [0.0, 10.0, 10.0, 24.5, 40.0]
            .into_iter()
            .map(line_at)
            .collect();
        for requested in [-1.0, 0.0, 0.5, 10.0, 11.0, 24.5, 100.0] {
            assert_eq!(
                row_top_at_most(&stacked, requested, true),
                linear_top(&stacked, requested),
                "{requested}"
            );
        }
        assert_eq!(row_top_at_most(&[], 4.0, true), 0.0);
        assert_eq!(max_row_y(&stacked, true), Some(40.0));
        assert_eq!(max_row_y(&[], true), None);

        let spatial = vec![line_at(0.0), line_at(80.0), line_at(12.0)];
        assert_eq!(row_top_at_most(&spatial, 20.0, false), 12.0);
        assert_eq!(max_row_y(&spatial, false), Some(80.0));
        assert_ne!(max_row_y(&spatial, true), Some(80.0));
    }

    #[test]
    fn plain_editor_wheel_snaps_to_the_same_row_as_a_linear_scan() {
        let mut world = World::new();
        crate::text::ensure_text_engine(&mut world);
        let text: String = (0..120)
            .map(|i| format!("row {i} distinct text\n"))
            .collect();
        let entity = world.spawn((
            Editor::default(),
            Label::new(""),
            Rect {
                x: 0.0,
                y: 0.0,
                w: 280.0,
                h: 90.0,
            },
        ));
        let _layout = document_rows(&mut world, entity, &text);
        let index = std::sync::Arc::clone(&world.get::<DocumentRows>(entity).unwrap().index);
        let lines: Vec<_> = (0..index.line_count()).map(|row| index.line(row)).collect();
        assert!(lines.len() > 40);
        assert!(
            lines.windows(2).all(|pair| pair[0].y <= pair[1].y),
            "plain rows must stay y-ordered"
        );
        let (content, page) = editor_scroll_metrics(&world, entity).unwrap();
        let scanned = lines.iter().map(|line| line.y).reduce(f32::max).unwrap();
        assert_eq!(scanned, lines.last().unwrap().y);
        assert_eq!(content, scanned + page);

        let anchor = lines[8].y;
        for delta in [-0.25, -1.0, -17.0, -80.0, 6.0, -10_000.0] {
            reset_editor_wheel(&mut world, entity);
            world.get_mut::<Editor>(entity).unwrap().offset = anchor;
            scroll_editor_wheel(&mut world, entity, delta);
            let offset = world.get::<Editor>(entity).unwrap().offset;
            let max = (content - page).max(0.0);
            let requested = (anchor - delta).clamp(0.0, max);
            let expected = lines
                .iter()
                .filter(|line| line.y <= requested)
                .map(|line| line.y)
                .reduce(f32::max)
                .unwrap_or(0.0);
            assert_eq!(offset, expected, "{delta}");
            assert!(lines.iter().any(|line| line.y == offset) || offset == 0.0);
        }
    }

    #[test]
    fn sub_row_remainder_survives_a_width_change_and_the_next_wheel_crosses() {
        let mut world = World::new();
        crate::text::ensure_text_engine(&mut world);
        let text: String = (0..40).map(|i| format!("row {i:02}\n")).collect();
        let entity = world.spawn((
            Editor::default(),
            Label::new(""),
            Rect {
                x: 0.0,
                y: 0.0,
                w: 280.0,
                h: 90.0,
            },
        ));
        let layout = document_rows(&mut world, entity, &text);
        let row_h = layout.lines[0].h;
        assert!(row_h > 4.0);
        scroll_editor_wheel(&mut world, entity, -(row_h - 2.0));
        assert_eq!(
            world.get::<Editor>(entity).unwrap().offset,
            layout.lines[0].y
        );
        world.get_mut::<Rect>(entity).unwrap().w = 120.0;
        let layout = document_rows(&mut world, entity, &text);
        assert_eq!(
            world.get::<Editor>(entity).unwrap().offset,
            layout.lines[0].y
        );
        scroll_editor_wheel(&mut world, entity, -4.0);
        assert_eq!(
            world.get::<Editor>(entity).unwrap().offset,
            layout.lines[1].y
        );
    }

    #[test]
    fn height_index_matches_paragraph_layout_for_wraps_and_folds() {
        let mut world = World::new();
        crate::text::ensure_text_engine(&mut world);
        let engine = crate::text::text_engine(&world);
        let font = crate::text_face(&world);
        let style = crate::TextStyle::default();
        let text = format!("{}folded line\nvisible tail\n", "word ".repeat(30));
        let fold_at = text.find("folded line\n").unwrap();
        let fold = fold_at..fold_at + "folded line\n".len();
        let folds = std::slice::from_ref(&fold);
        let width = 96.0;
        let (mut full, _, _) = geometry::build(
            engine.as_ref(),
            font.as_ref(),
            &style,
            width,
            &text,
            &[],
            None,
        );
        assert!(
            full.lines.len() > 4,
            "the fixture must wrap, got {} lines",
            full.lines.len()
        );
        let built = index::build(
            engine.as_ref(),
            font.as_ref(),
            &style,
            width,
            &text,
            &[],
            &[],
            None,
        );
        assert_eq!(built.index.line_count(), full.lines.len());
        for row in 0..full.lines.len() {
            let indexed = built.index.line(row);
            let line = &full.lines[row];
            assert_eq!(indexed.start, line.start, "row {row} start");
            assert_eq!(indexed.end, line.end, "row {row} end");
            assert_eq!(indexed.y.to_bits(), line.y.to_bits(), "row {row} y");
            assert_eq!(indexed.h.to_bits(), line.h.to_bits(), "row {row} h");
        }
        // Same projection as `project_fold_layout`: drop hidden line starts and
        // pack the rest with no paragraph gap.
        full.lines.retain(|line| {
            let next = folds.partition_point(|range| range.end <= line.start);
            !folds
                .get(next)
                .is_some_and(|range| range.contains(&line.start))
        });
        let mut y = 0.0;
        for line in &mut full.lines {
            line.y = y;
            y += line.h;
        }
        let folded = index::build(
            engine.as_ref(),
            font.as_ref(),
            &style,
            width,
            &text,
            &[],
            folds,
            None,
        );
        assert_eq!(folded.index.line_count(), full.lines.len());
        assert!(folded.index.line_count() < built.index.line_count());
        for row in 0..full.lines.len() {
            let indexed = folded.index.line(row);
            let line = &full.lines[row];
            assert_eq!(indexed.start, line.start, "folded row {row} start");
            assert_eq!(indexed.y.to_bits(), line.y.to_bits(), "folded row {row} y");
            assert_eq!(indexed.h.to_bits(), line.h.to_bits(), "folded row {row} h");
        }
    }
}
