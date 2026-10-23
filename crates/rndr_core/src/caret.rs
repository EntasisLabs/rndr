use crate::clipboard::{note_clipboard, Clipboard};
use crate::edit::edit_string;
use crate::entity::Entity;
use crate::fs::{Dirty, FsPath};
use crate::hierarchy::children_of;
use crate::input::{Input, Key};
use crate::keys::{platform_of, KeyEvent};
use crate::layout::{ancestor_with, hit_test, Clip, Rect, Scroll};
use crate::pane::Pane;
use crate::style::{Body, Label};
use crate::typing::{apply_text_event, steer, EditHistory, EditSurface, Steer};
use crate::world::{Component, World};
use crate::wrap::{
    visual_pos, visual_selection_spans, with_text_layout, wrap_lines, GLYPH_PX, TEXT_INSET_X,
    TEXT_INSET_Y,
};

impl Component for Caret {}
impl Component for TextSelection {}
impl Component for Field {}

/// Which pane field the caret is bound to.
///
/// Tab in [`apply_caret`] swaps the two. Enter on the title moves to the body.
/// The body takes newlines and vertical motion. The title does not.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EditField {
    /// Pane title. [`set_editing`] selects this when you pass a node.
    #[default]
    Title,
    /// Pane body. Enter inserts a newline. Up and Down move one visual line.
    Body,
}

/// Marks a pane row as the title or body field.
///
/// [`apply_caret`] searches rows spawned by a [`crate::Pane`] for this marker.
#[derive(Clone, Copy, Debug)]
pub struct Field {
    /// Title or body slot this row edits.
    pub kind: EditField,
}

/// Tree node whose pane title or body is being edited.
///
/// `source` is that node. `None` means the pane is idle. [`apply_caret`]
/// treats a dead `source` the same way and clears it.
#[derive(Clone, Copy, Debug, Default)]
pub struct Editing {
    /// Node that owns the pane text. `None` leaves the pane idle.
    pub source: Option<Entity>,
    /// Title or body currently receiving keys.
    pub field: EditField,
}

/// Insertion point on a pane field [`Label`]. Indices are byte offsets.
/// `anchor` equals `index` when the range is empty.
#[derive(Clone, Copy, Debug, Default)]
pub struct Caret {
    /// Byte offset of the insertion point. [`apply_caret`] clamps it to the label.
    pub index: usize,
    /// Byte offset where the selection started. Equals [`Caret::index`] when
    /// the range is empty.
    pub anchor: usize,
}

/// Highlighted byte range with no caret and no focus chrome.
///
/// Paint draws the range after inline highlight bars. It does not lighten
/// the entity fill and it does not draw a caret. [`Caret`] remains the
/// insertion point for a field.
#[derive(Clone, Copy, Debug, Default)]
pub struct TextSelection {
    /// Byte offset where the range started.
    pub anchor: usize,
    /// Byte offset of the other end. Equals [`Self::anchor`] when the range
    /// is empty.
    pub head: usize,
}

/// Point [`Editing`] at `source` and select the title.
///
/// Pass `None` to leave the pane idle. The field is left unchanged in that
/// case. Returns without writing when the [`Editing`] resource is missing. A
/// dead `source` is kept until [`apply_caret`] drops it.
pub fn set_editing(world: &mut World, source: Option<Entity>) {
    if let Some(editing) = world.resource_mut::<Editing>() {
        editing.source = source;
        if source.is_some() {
            editing.field = EditField::Title;
        }
    }
}

/// Point [`Editing`] at `source` and `field`.
///
/// Returns without writing when the [`Editing`] resource is missing. A dead
/// `source` is kept until [`apply_caret`] drops it.
pub fn set_editing_field(world: &mut World, source: Entity, field: EditField) {
    if let Some(editing) = world.resource_mut::<Editing>() {
        editing.source = Some(source);
        editing.field = field;
    }
}

pub fn is_editing(world: &World) -> bool {
    world
        .resource::<Editing>()
        .is_some_and(|editing| editing.source.is_some())
}

/// Bind or move the pane-field caret from [`Editing`] and [`Input`].
///
/// [`crate::Shell`] runs this on Update, after the pane has spawned its rows.
/// A missing [`Editing`] resource, a `None` source, or a dead source takes
/// the idle path: carets are removed except on a [`crate::CapturesKeys`]
/// entity, and `source` is cleared when the resource exists.
///
/// With a live source, a non-empty [`Input::keys`] queue is applied in order
/// and [`Input::chars`] is not read. Tab, Enter, Up, and Down steer the field
/// unless Ctrl, Alt, Super, or an [`crate::EditAction`] chord is held.
/// Tab swaps title and body. Enter on the title moves to the body. Up and
/// Down move one visual line in the body. Other events edit the row,
/// including [`crate::EditAction`] chords. Changing field parks the caret
/// at the end of that label. The pointer is placed before those events.
///
/// When the queue is empty, the legacy [`Input::key`] is used. Tab and Enter
/// on the title behave as above and return before the pointer is read. Up and
/// Down move one visual line in the body. The title ignores vertical motion.
/// Other keys go through [`edit_string`], with newlines enabled on the body.
/// If the pane has no row for that field, the key stays and this returns.
///
/// Shift on a vertical move keeps the anchor. Without Shift, a selection
/// collapses to the edge in that direction, and a caret moves with its anchor.
/// A pointer press that hits the active field sets [`Caret::index`] from the
/// glyph under the cursor. Shift keeps that anchor only when the caret already
/// existed this frame.
///
/// An edit of the title is written to the source's first labeled child, or to
/// the source when that entity itself has the [`Label`]. An edit of the body
/// is written to [`Body`]. When the text changes on a node that has
/// [`crate::FsPath`], [`crate::Dirty`] is inserted. A caret is scrolled into
/// its nearest [`crate::Scroll`] ancestor so its line and row stay inside
/// the clip.
///
/// Handling a live source clears [`Input::chars`] and [`Input::keys`], except
/// the legacy path with no field row. The idle path clears them unless focus
/// has [`crate::CapturesKeys`]. Whenever input is cleared, the legacy
/// [`Input::key`] is cleared only for arrows, Enter, Tab, and Backspace.
pub fn apply_caret(world: &mut World) {
    let (source, field) = match world.resource::<Editing>().copied() {
        Some(editing) => (editing.source, editing.field),
        None => (None, EditField::Title),
    };
    let source = source.filter(|entity| world.is_alive(*entity));
    let Some(source) = source else {
        clear_carets(world);
        if let Some(editing) = world.resource_mut::<Editing>() {
            editing.source = None;
        }
        if !focus_captures_keys(world) {
            clear_text_input(world);
        }
        return;
    };

    let uses_queue = world
        .resource::<Input>()
        .is_some_and(|input| !input.keys.is_empty());
    if uses_queue {
        apply_queue(world, source, field);
        return;
    }

    let key = world.resource::<Input>().and_then(|input| input.key);

    let mut field = field;
    if key == Some(Key::Tab) {
        field = toggle_field(field);
        if let Some(editing) = world.resource_mut::<Editing>() {
            editing.field = field;
        }
        bind_caret(world, field);
        clear_text_input(world);
        ensure_caret_visible(world);
        return;
    }
    if key == Some(Key::Enter) && field == EditField::Title {
        field = EditField::Body;
        if let Some(editing) = world.resource_mut::<Editing>() {
            editing.field = field;
        }
        bind_caret(world, field);
        clear_text_input(world);
        ensure_caret_visible(world);
        return;
    }

    let Some(row) = field_row(world, field) else {
        return;
    };
    let existed = world.has::<Caret>(row);
    if !existed {
        bind_caret(world, field);
    }
    let Some(row) = field_row(world, field) else {
        return;
    };
    place_caret_from_pointer(world, row, existed);

    let chars = world
        .resource::<Input>()
        .map(|input| input.chars.clone())
        .unwrap_or_default();
    let shift = world.resource::<Input>().is_some_and(|input| input.shift);

    let mut text = world
        .get::<Label>(row)
        .map(|label| label.text.clone())
        .unwrap_or_default();
    let original = text.clone();
    let (mut index, mut anchor) = world
        .get::<Caret>(row)
        .map(|caret| (caret.index.min(text.len()), caret.anchor.min(text.len())))
        .unwrap_or((text.len(), text.len()));

    if matches!(key, Some(Key::Up | Key::Down)) && field == EditField::Body {
        let delta = if key == Some(Key::Up) { -1 } else { 1 };
        let next = move_visual_line(world, row, &text, index, delta);
        if shift {
            index = next;
        } else if anchor != index {
            let edge = if delta < 0 {
                anchor.min(index)
            } else {
                anchor.max(index)
            };
            index = edge;
            anchor = edge;
        } else {
            index = next;
            anchor = next;
        }
    } else {
        (index, anchor) = edit_string(
            &mut text,
            index,
            anchor,
            key,
            shift,
            &chars,
            field == EditField::Body,
        );
    }

    if let Some(label) = world.get_mut::<Label>(row) {
        label.text.clone_from(&text);
    }
    if let Some(caret) = world.get_mut::<Caret>(row) {
        caret.index = index;
        caret.anchor = anchor;
    }
    match field {
        EditField::Title => {
            if let Some(header) = header_label_entity(world, source) {
                if let Some(label) = world.get_mut::<Label>(header) {
                    label.text.clone_from(&text);
                }
            }
        }
        EditField::Body => {
            if let Some(body) = world.get_mut::<Body>(source) {
                body.text.clone_from(&text);
            }
        }
    }
    if text != original && world.has::<FsPath>(source) {
        world.insert(source, Dirty);
    }

    clear_text_input(world);
    ensure_caret_visible(world);
}

fn apply_queue(world: &mut World, source: Entity, mut field: EditField) {
    let platform = platform_of(world);
    let events = world
        .resource::<Input>()
        .map(|input| input.keys.clone())
        .unwrap_or_default();
    let existed = field_row(world, field).is_some_and(|row| world.has::<Caret>(row));
    if !existed {
        bind_caret(world, field);
    }
    if let Some(row) = field_row(world, field) {
        place_caret_from_pointer(world, row, existed);
    }
    let mut clip = world
        .resource::<Clipboard>()
        .map(|clipboard| clipboard.text.clone())
        .unwrap_or_default();
    let mut copied = None;
    let mut clipboard_action = false;
    for event in &events {
        match steer(event, platform) {
            Steer::Tab => {
                field = toggle_field(field);
                if let Some(editing) = world.resource_mut::<Editing>() {
                    editing.field = field;
                }
                bind_caret(world, field);
            }
            Steer::Enter if field == EditField::Title => {
                field = EditField::Body;
                if let Some(editing) = world.resource_mut::<Editing>() {
                    editing.field = field;
                }
                bind_caret(world, field);
            }
            Steer::Up if field == EditField::Body => {
                nudge(world, source, field, -1, event.modifiers.shift)
            }
            Steer::Down if field == EditField::Body => {
                nudge(world, source, field, 1, event.modifiers.shift)
            }
            _ => {
                let step = edit_row(world, source, field, event, platform, &clip);
                if let Some(text) = step.copied.clone() {
                    clip = text.clone();
                    copied = Some(text);
                }
                clipboard_action |= step.clipboard_action;
            }
        }
    }
    if clipboard_action {
        note_clipboard(world, copied.as_deref());
    }
    clear_text_input(world);
    ensure_caret_visible(world);
}

fn edit_row(
    world: &mut World,
    source: Entity,
    field: EditField,
    event: &KeyEvent,
    platform: crate::keys::Platform,
    clip: &str,
) -> crate::typing::StepResult {
    let Some(row) = field_row(world, field) else {
        return crate::typing::StepResult::default();
    };
    let mut text = world
        .get::<Label>(row)
        .map(|label| label.text.clone())
        .unwrap_or_default();
    let (mut index, mut anchor) = world
        .get::<Caret>(row)
        .map(|caret| (caret.index.min(text.len()), caret.anchor.min(text.len())))
        .unwrap_or((text.len(), text.len()));
    let original = text.clone();
    let mut history = world.remove::<EditHistory>(row).unwrap_or_default();
    let step = apply_text_event(
        &mut EditSurface {
            text: &mut text,
            index: &mut index,
            anchor: &mut anchor,
        },
        event,
        platform,
        field == EditField::Body,
        crate::typing::frame_now(world),
        &mut history,
        clip,
    );
    world.insert(row, history);
    store_row(
        world,
        source,
        field,
        row,
        Written {
            text: &text,
            index,
            anchor,
            original: &original,
        },
    );
    step
}

fn nudge(world: &mut World, source: Entity, field: EditField, delta: isize, shift: bool) {
    let Some(row) = field_row(world, field) else {
        return;
    };
    let text = world
        .get::<Label>(row)
        .map(|label| label.text.clone())
        .unwrap_or_default();
    let (index, anchor) = world
        .get::<Caret>(row)
        .map(|caret| (caret.index.min(text.len()), caret.anchor.min(text.len())))
        .unwrap_or((text.len(), text.len()));
    let next = move_visual_line(world, row, &text, index, delta);
    let (index, anchor) = if shift {
        (next, anchor)
    } else if anchor != index {
        let edge = if delta < 0 {
            anchor.min(index)
        } else {
            anchor.max(index)
        };
        (edge, edge)
    } else {
        (next, next)
    };
    store_row(
        world,
        source,
        field,
        row,
        Written {
            text: &text,
            index,
            anchor,
            original: &text,
        },
    );
}

struct Written<'a> {
    text: &'a str,
    index: usize,
    anchor: usize,
    original: &'a str,
}

fn store_row(
    world: &mut World,
    source: Entity,
    field: EditField,
    row: Entity,
    written: Written<'_>,
) {
    if let Some(label) = world.get_mut::<Label>(row) {
        label.text = written.text.to_string();
    }
    if let Some(caret) = world.get_mut::<Caret>(row) {
        caret.index = written.index;
        caret.anchor = written.anchor;
    }
    match field {
        EditField::Title => {
            if let Some(header) = header_label_entity(world, source) {
                if let Some(label) = world.get_mut::<Label>(header) {
                    label.text = written.text.to_string();
                }
            }
        }
        EditField::Body => {
            if let Some(body) = world.get_mut::<Body>(source) {
                body.text = written.text.to_string();
            }
        }
    }
    if written.text != written.original && world.has::<FsPath>(source) {
        world.insert(source, Dirty);
    }
}

fn toggle_field(field: EditField) -> EditField {
    match field {
        EditField::Title => EditField::Body,
        EditField::Body => EditField::Title,
    }
}

fn bind_caret(world: &mut World, field: EditField) {
    let Some(row) = field_row(world, field) else {
        return;
    };
    let len = world
        .get::<Label>(row)
        .map(|label| label.text.len())
        .unwrap_or(0);
    clear_carets(world);
    world.insert(
        row,
        Caret {
            index: len,
            anchor: len,
        },
    );
}

fn place_caret_from_pointer(world: &mut World, row: Entity, existed: bool) {
    let Some(input) = world.resource::<Input>() else {
        return;
    };
    if !input.pointer.pressed {
        return;
    }
    let shift = input.shift;
    let x = input.pointer.position[0];
    let y = input.pointer.position[1];
    let Some(hit) = hit_test(world, x, y) else {
        return;
    };
    if ancestor_with::<Field>(world, hit) != Some(row) {
        return;
    }
    let Some(rect) = world.get::<Rect>(row).copied() else {
        return;
    };
    let text = world
        .get::<Label>(row)
        .map(|label| label.text.clone())
        .unwrap_or_default();
    let index = with_text_layout(world, row, &text, |layout| {
        layout.index_at(x - rect.x - TEXT_INSET_X, y - rect.y - TEXT_INSET_Y)
    });
    if let Some(caret) = world.get_mut::<Caret>(row) {
        caret.index = index;
        if !(shift && existed) {
            caret.anchor = index;
        }
    }
}

/// Line and column of a byte `index`. Column is a byte offset on that line.
///
/// Only hard newlines break lines. An index past the end sits on the last
/// line, at that line's end. Empty text returns `(0, 0)`.
pub fn caret_line_col(text: &str, index: usize) -> (usize, usize) {
    visual_pos(&wrap_lines(text, usize::MAX), index)
}

/// Selected runs as `(line, start_col, end_col)` in paint order. No soft wrap.
///
/// Columns are byte offsets from the start of that hard line. An empty range
/// returns an empty vec. A blank line inside the range is `(line, 0, 0)`.
pub fn selection_spans(text: &str, anchor: usize, index: usize) -> Vec<(usize, usize, usize)> {
    visual_selection_spans(&wrap_lines(text, usize::MAX), anchor, index)
}

fn field_row(world: &World, kind: EditField) -> Option<Entity> {
    let pane = world.entities_with::<Pane>().into_iter().next()?;
    let spawned = world.get::<Pane>(pane)?.spawned.clone();
    spawned.into_iter().find(|&entity| {
        world
            .get::<Field>(entity)
            .is_some_and(|field| field.kind == kind)
    })
}

fn header_label_entity(world: &World, entity: Entity) -> Option<Entity> {
    for child in children_of(world, entity) {
        if world.has::<Label>(child) {
            return Some(child);
        }
    }
    world.has::<Label>(entity).then_some(entity)
}

fn focus_captures_keys(world: &World) -> bool {
    world
        .resource::<crate::input::Focus>()
        .and_then(|focus| focus.entity)
        .is_some_and(|entity| world.has::<crate::field::CapturesKeys>(entity))
}

fn clear_carets(world: &mut World) {
    for entity in world.entities_with::<Caret>() {
        if world.has::<crate::field::CapturesKeys>(entity) {
            continue;
        }
        world.remove::<Caret>(entity);
    }
}

fn clear_text_input(world: &mut World) {
    if let Some(input) = world.resource_mut::<Input>() {
        input.chars.clear();
        input.keys.clear();
        if matches!(
            input.key,
            Some(
                Key::Left
                    | Key::Right
                    | Key::Backspace
                    | Key::Enter
                    | Key::Tab
                    | Key::Up
                    | Key::Down
            )
        ) {
            input.key = None;
        }
    }
}

fn move_visual_line(world: &World, row: Entity, text: &str, index: usize, delta: isize) -> usize {
    with_text_layout(world, row, text, |layout| {
        if layout.lines.is_empty() {
            return index;
        }
        let (line, _) = layout_line_col(layout, index);
        let (x, _) = layout.caret_xy(index);
        let next = (line as isize + delta).clamp(0, layout.lines.len().saturating_sub(1) as isize);
        layout.index_on_line(next as usize, x)
    })
}

fn layout_line_col(layout: &crate::font::TextLayout, index: usize) -> (usize, usize) {
    crate::font::line_col(
        &layout
            .lines
            .iter()
            .map(|line| (line.start, line.end))
            .collect::<Vec<_>>(),
        index,
    )
}

fn ensure_caret_visible(world: &mut World) {
    let Some(entity) = world.entities_with::<Caret>().into_iter().next() else {
        return;
    };
    let Some(rect) = world.get::<Rect>(entity).copied() else {
        return;
    };
    let Some(scroll_entity) = ancestor_with::<Scroll>(world, entity) else {
        return;
    };
    let Some(port) = world.get::<Clip>(scroll_entity).copied().or_else(|| {
        world
            .get::<Rect>(scroll_entity)
            .copied()
            .map(Clip::from_rect)
    }) else {
        return;
    };
    let (caret_y, caret_h) = caret_line_rect(world, entity, rect);
    let mut dy = 0.0f32;
    if caret_y < port.y {
        dy = port.y - caret_y;
    } else if caret_y + caret_h > port.y + port.h {
        dy = (port.y + port.h) - (caret_y + caret_h);
    }
    let mut dx = 0.0f32;
    if rect.x < port.x {
        dx = port.x - rect.x;
    } else if rect.x + rect.w > port.x + port.w {
        dx = (port.x + port.w) - (rect.x + rect.w);
    }
    if dx == 0.0 && dy == 0.0 {
        return;
    }
    if let Some(scroll) = world.get_mut::<Scroll>(scroll_entity) {
        scroll.offset[0] += dx;
        scroll.offset[1] += dy;
    }
}

fn caret_line_rect(world: &World, entity: Entity, rect: Rect) -> (f32, f32) {
    let text = world
        .get::<Label>(entity)
        .map(|label| label.text.clone())
        .unwrap_or_default();
    let index = world
        .get::<Caret>(entity)
        .map(|caret| caret.index)
        .unwrap_or(0);
    let (y, h) = with_text_layout(world, entity, &text, |layout| {
        let (_x, y) = layout.caret_xy(index);
        let h = layout.lines.first().map(|line| line.h).unwrap_or(GLYPH_PX);
        (y, h)
    });
    (rect.y + TEXT_INSET_Y + y, h)
}
