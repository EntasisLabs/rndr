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
        self.slice(0, self.len())
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
        for index in start..end {
            bytes.push(self.byte(index));
        }
        String::from_utf8_lossy(&bytes).into_owned()
    }

    /// Insert `text` at byte `index`, clamped to [`Self::len`].
    ///
    /// An empty `text` returns without moving the gap or rebuilding lines.
    /// Does not move [`Self::caret`] or [`Self::anchor`]. A mid-codepoint
    /// index splits that scalar. The line index is rebuilt. This is history-free;
    /// use [`edit_buffer`] for undoable app-owned source operations.
    pub fn insert_str(&mut self, index: usize, text: &str) {
        if text.is_empty() {
            return;
        }
        self.move_gap_to(index.min(self.len()));
        let bytes = text.as_bytes();
        self.ensure_gap(bytes.len());
        self.data[self.gap_start..self.gap_start + bytes.len()].copy_from_slice(bytes);
        self.gap_start += bytes.len();
        self.rebuild_lines();
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
        self.move_gap_to(start);
        self.gap_end += end - start;
        self.rebuild_lines();
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
        let gap = self.gap_end - self.gap_start;
        let mut next = Vec::with_capacity(self.len() + gap);
        next.extend_from_slice(self.slice(0, index).as_bytes());
        let gap_at = next.len();
        next.resize(next.len() + gap, 0);
        next.extend_from_slice(self.slice(index, self.len()).as_bytes());
        self.data = next;
        self.gap_start = gap_at;
        self.gap_end = gap_at + gap;
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
    if world.has::<crate::editing::SourceBinding>(entity) {
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

#[derive(Clone, Copy, Debug, Default)]
struct EditorWindow {
    base: usize,
}
impl Component for EditorWindow {}

// Logical capture only: the desktop backend must still deliver held/release
// frames. No global/native window capture is requested here.
#[derive(Clone, Copy)]
struct EditorDrag {
    id: u64,
    anchor: usize,
}
impl Component for EditorDrag {}

// Keep row geometry, not document glyph quads. Only the visible Label is
// published for painting. An idle editor must not reshape the whole document.
struct DocumentRows {
    key: u64,
    layout: std::sync::Arc<crate::TextLayout>,
}
impl Component for DocumentRows {}

fn document_rows(
    world: &mut World,
    entity: crate::entity::Entity,
    text: &str,
) -> std::sync::Arc<crate::TextLayout> {
    let style = crate::text::style_for(world, entity);
    let engine = crate::text::text_engine(world);
    let width = world.get::<Rect>(entity).map_or(f32::INFINITY, |rect| {
        crate::font::field_text_width(rect.w, engine.unit_advance(&style))
    });
    let spans = world
        .get::<crate::Spans>(entity)
        .map(|spans| spans.runs.as_slice())
        .unwrap_or(&[]);
    let font = crate::font::text_face(world);
    let mut key = crate::font::layout_cache_key(font.as_ref(), text, width, spans, "", 0);
    key ^= crate::text::style_key(&style);
    key ^= (std::sync::Arc::as_ptr(&engine) as *const () as usize as u64).rotate_left(17);
    key ^= engine.cache_epoch().wrapping_mul(0x9E37_79B9_7F4A_7C15);
    if let Some(rows) = world
        .get::<DocumentRows>(entity)
        .filter(|rows| rows.key == key)
    {
        return std::sync::Arc::clone(&rows.layout);
    }
    let mut layout = engine.layout(text, &style, spans, width);
    world.remove::<VerticalGoal>(entity);
    layout.glyphs = Vec::new();
    layout.highlights = Vec::new();
    let layout = std::sync::Arc::new(layout);
    world.insert(
        entity,
        DocumentRows {
            key,
            layout: std::sync::Arc::clone(&layout),
        },
    );
    layout
}

#[derive(Clone, Copy, Debug)]
struct VerticalGoal {
    x: f32,
    index: usize,
    row: usize,
}
impl Component for VerticalGoal {}

fn row_contains(layout: &crate::TextLayout, row: usize, index: usize) -> bool {
    layout
        .lines
        .get(row)
        .is_some_and(|line| index >= line.start && index <= line.end)
}

fn move_vertical(
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
    let layout = document_rows(world, entity, text);
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
        let next_row =
            (row as isize + delta).clamp(0, layout.lines.len().saturating_sub(1) as isize) as usize;
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
            let (_, y) = layout.caret_xy(next);
            layout
                .lines
                .iter()
                .position(|line| line.y == y)
                .unwrap_or(0)
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
    crate::editing::prepare_source_input(world);
    let editors = world.entities_with::<Editor>();
    for entity in editors {
        if !world.has::<Buffer>(entity) {
            world.remove::<EditorDrag>(entity);
            continue;
        }
        let managed = world.has::<crate::editing::SourceBinding>(entity);
        if managed {
            crate::editing::project_source(world, entity);
        }
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
                                            crate::NamedKey::ArrowLeft
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
            if !composing {
                reveal |= place_from_pointer(world, entity);
            }
            if managed {
                crate::editing::sync_selection(world, entity);
            }
        } else {
            world.remove::<VerticalGoal>(entity);
            world.remove::<EditorDrag>(entity);
        }
        publish_window(world, entity, reveal);
        if focused {
            sync_ime_preview(world, entity);
        } else {
            world.remove::<crate::field::ImePreview>(entity);
        }
    }
}

fn edit_focused(world: &mut World, entity: crate::entity::Entity) {
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
    let snapshot = buffer.text();
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
        let delta = match steer(&event, platform) {
            Steer::Up => Some(-1),
            Steer::Down => Some(1),
            _ => None,
        };
        if let Some(delta) = delta {
            let Some(doc) = world
                .resource::<SourceDocuments>()
                .and_then(|d| d.get(binding.document))
            else {
                break;
            };
            let text = doc.text().to_string();
            let s = doc.selection();
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
            let delta = match steer(event, platform) {
                Steer::Up => Some(-1),
                Steer::Down => Some(1),
                _ => None,
            };
            if let Some(delta) = delta {
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
    if buffer.text() != text {
        let len = buffer.len();
        buffer.delete_range(0, len);
        buffer.insert_str(0, text);
    }
    let len = buffer.len();
    buffer.caret = caret.min(len);
    buffer.anchor = anchor.min(len);
}

fn sync_ime_preview(world: &mut World, entity: crate::entity::Entity) {
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
    let previous = world.remove::<EditorDrag>(entity);
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
        && previous.is_some_and(|drag| drag.id == pointer.id)
        && (pointer.down || pointer.released);
    if !starting && !continuing {
        return false;
    }
    let Some(buffer) = world.get::<Buffer>(entity) else {
        return false;
    };
    let text = buffer.text();
    let old_anchor = buffer.anchor;
    let layout = document_rows(world, entity, &text);
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
    let mut first = layout
        .lines
        .iter()
        .rposition(|line| line.y <= offset)
        .unwrap_or(0);
    let available = (rect.h - 2.0 * TEXT_ORIGIN_Y).max(1.0);
    let map = |position: [f32; 2], first: usize| {
        let x = position[0] - rect.x - TEXT_ORIGIN_X;
        let y = (position[1] - rect.y - TEXT_ORIGIN_Y).clamp(0.0, (available - 0.001).max(0.0))
            + layout.lines[first].y;
        let row = layout
            .lines
            .iter()
            .position(|line| y < line.y + line.h.max(0.001))
            .unwrap_or(layout.lines.len() - 1);
        let index = floor_grapheme(&text, layout.index_at(x, y).min(text.len()));
        // Preserve B001's preferred-column contract: subsequent Up/Down
        // starts at the actual caret stop, not a distant off-text pointer x.
        (index, row, layout.caret_xy_on_line(index, row).0)
    };
    let anchor = if starting {
        if shift {
            floor_grapheme(&text, old_anchor.min(text.len()))
        } else {
            map(start, first).0
        }
    } else {
        floor_grapheme(
            &text,
            previous.expect("continuing drag").anchor.min(text.len()),
        )
    };
    // Never let a distant coordinate jump to a distant document row. Release
    // applies its final position but does not add another autoscroll step.
    if continuing && pointer.down && !pointer.released {
        if pointer.position[1] < rect.y {
            first = first.saturating_sub(1);
        } else if pointer.position[1] >= rect.y + rect.h {
            first = (first + 1).min(layout.lines.len() - 1);
        }
    }
    if let Some(editor) = world.get_mut::<Editor>(entity) {
        editor.offset = layout.lines[first].y;
    }
    let (index, row, x) = map(pointer.position, first);
    if let Some(buffer) = world.get_mut::<Buffer>(entity) {
        buffer.caret = index;
        buffer.anchor = anchor;
    }
    world.insert(entity, VerticalGoal { x, index, row });
    if pointer.down && !pointer.released {
        world.insert(
            entity,
            EditorDrag {
                id: pointer.id,
                anchor,
            },
        );
    }
    starting
}

fn publish_window(world: &mut World, entity: crate::entity::Entity, reveal: bool) {
    let height = world.get::<Rect>(entity).map(|rect| rect.h).unwrap_or(0.0);
    let offset = world
        .get::<Editor>(entity)
        .map(|editor| editor.offset)
        .unwrap_or(0.0);
    let Some(buffer) = world.get::<Buffer>(entity) else {
        return;
    };
    let text = buffer.text();
    let layout = document_rows(world, entity, &text);
    let buffer = world.get::<Buffer>(entity).expect("buffer exists");
    let caret = floor_grapheme(&text, buffer.caret.min(text.len()));
    let anchor = floor_grapheme(&text, buffer.anchor.min(text.len()));
    let available = (height - 2.0 * TEXT_ORIGIN_Y).max(1.0);
    let affinity = world
        .get::<VerticalGoal>(entity)
        .filter(|goal| goal.index == caret && row_contains(&layout, goal.row, caret));
    let (base, end, top, visible, numbers, first, caret_row) = {
        let offset = if offset.is_finite() {
            offset.max(0.0)
        } else {
            0.0
        };
        let mut first = layout
            .lines
            .iter()
            .rposition(|line| line.y <= offset)
            .unwrap_or(0);
        let (_, caret_y) = layout.caret_xy(caret);
        let caret_row = affinity.map_or_else(
            || {
                layout
                    .lines
                    .iter()
                    .position(|line| line.y == caret_y)
                    .unwrap_or(0)
            },
            |goal| goal.row,
        );
        if reveal && height > 0.0 {
            if caret_row < first {
                first = caret_row;
            }
            while first < caret_row
                && layout.lines[caret_row].y + layout.lines[caret_row].h
                    > layout.lines[first].y + available
            {
                first += 1;
            }
        }
        let top = layout.lines.get(first).map_or(0.0, |line| line.y);
        let last = layout
            .lines
            .iter()
            .rposition(|line| line.y < top + available)
            .unwrap_or(first);
        let base = layout.lines.get(first).map_or(0, |line| line.start);
        let end = layout
            .lines
            .get(last + 1)
            .map_or(text.len(), |line| line.start);
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
            if let Some(line) = layout.lines.get(row) {
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
    let window = text[base..end].to_string();
    if let Some(editor) = world.get_mut::<Editor>(entity) {
        editor.offset = top;
    }
    if let Some(buffer) = world.get_mut::<Buffer>(entity) {
        buffer.caret = caret;
        buffer.anchor = anchor;
    }
    world.insert(entity, EditorWindow { base });
    if let Some(label) = world.get_mut::<Label>(entity) {
        label.text = window;
    }
    if visible {
        world.insert(
            entity,
            crate::CaretRow {
                index: caret - base,
                row: caret_row - first,
            },
        );
        world.insert(
            entity,
            Caret {
                index: caret - base,
                anchor: anchor.clamp(base, end) - base,
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
    let end = base.saturating_add(text.len());
    spans
        .iter()
        .filter_map(|span| {
            if span.start >= end || span.end <= base {
                return None;
            }
            let mut span = span.clone();
            span.start = span.start.max(base) - base;
            span.end = span.end.min(end) - base;
            Some(span)
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
        if decoration.end <= base || decoration.start >= base + window_len {
            continue;
        }
        let start = decoration.start.saturating_sub(base).min(window_len);
        let end = decoration.end.saturating_sub(base).min(window_len);
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
}
