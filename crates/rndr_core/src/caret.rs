use crate::entity::Entity;
use crate::fs::{Dirty, FsPath};
use crate::hierarchy::children_of;
use crate::input::{Input, Key};
use crate::layout::{ancestor_with, hit_test, Clip, Layout, Rect, Scroll, Size};
use crate::pane::{field_height, Pane};
use crate::style::{Body, Label, CELL_H, CELL_W, TEXT_ORIGIN_X, TEXT_ORIGIN_Y};
use crate::world::{Component, World};

impl Component for Caret {}
impl Component for Field {}

/// Which pane field the caret is bound to.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EditField {
    #[default]
    Title,
    Body,
}

/// Marks a pane row as the title or body field.
#[derive(Clone, Copy, Debug)]
pub struct Field {
    pub kind: EditField,
}

/// Tree node whose pane title or body is being edited.
#[derive(Clone, Copy, Debug, Default)]
pub struct Editing {
    pub source: Option<Entity>,
    pub field: EditField,
}

/// Insertion point on a pane field [`Label`]. Indices are byte offsets.
/// `anchor` equals `index` when the range is empty.
#[derive(Clone, Copy, Debug, Default)]
pub struct Caret {
    pub index: usize,
    pub anchor: usize,
}

pub fn set_editing(world: &mut World, source: Option<Entity>) {
    if let Some(editing) = world.resource_mut::<Editing>() {
        editing.source = source;
        if source.is_some() {
            editing.field = EditField::Title;
        }
    }
}

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
        clear_text_input(world);
        return;
    };

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

    let movement = match (key, field) {
        (Some(Key::Left | Key::Right), _) => true,
        (Some(Key::Up | Key::Down), EditField::Body) => true,
        _ => false,
    };
    if movement {
        (index, anchor) = move_caret(&text, index, anchor, key, shift);
    } else {
        let editing = !chars.is_empty()
            || key == Some(Key::Backspace)
            || (key == Some(Key::Enter) && field == EditField::Body);
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
        if key == Some(Key::Enter) && field == EditField::Body {
            text.insert(index, '\n');
            index += 1;
            anchor = index;
        }
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
            if let Some(layout) = world.get_mut::<Layout>(row) {
                layout.height = Size::Px(field_height(&text));
            }
        }
    }
    if text != original && world.has::<FsPath>(source) {
        world.insert(source, Dirty);
    }

    clear_text_input(world);
    ensure_caret_visible(world);
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
    if !input.pressed {
        return;
    }
    let shift = input.shift;
    let x = input.cursor[0];
    let y = input.cursor[1];
    let Some(hit) = hit_test(world, x, y) else {
        return;
    };
    if hit != row {
        return;
    }
    let Some(rect) = world.get::<Rect>(row).copied() else {
        return;
    };
    let text = world
        .get::<Label>(row)
        .map(|label| label.text.clone())
        .unwrap_or_default();
    let index = index_at_point(&text, rect, x, y);
    if let Some(caret) = world.get_mut::<Caret>(row) {
        caret.index = index;
        if !(shift && existed) {
            caret.anchor = index;
        }
    }
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
        Some(Key::Up) => move_line(text, index, -1),
        Some(Key::Down) => move_line(text, index, 1),
        _ => index,
    };
    if shift {
        (next, anchor)
    } else if anchor != index {
        let edge = match key {
            Some(Key::Left | Key::Up) => anchor.min(index),
            Some(Key::Right | Key::Down) => anchor.max(index),
            _ => next,
        };
        (edge, edge)
    } else {
        (next, next)
    }
}

/// Line and column of a byte `index`. Column is a byte offset on that line.
pub fn caret_line_col(text: &str, index: usize) -> (usize, usize) {
    line_col(text, index)
}

/// Selected runs as `(line, start_col, end_col)` in paint order.
pub fn selection_spans(text: &str, anchor: usize, index: usize) -> Vec<(usize, usize, usize)> {
    let start = anchor.min(index).min(text.len());
    let end = anchor.max(index).min(text.len());
    if start == end {
        return Vec::new();
    }
    let (start_line, start_col) = line_col(text, start);
    let (end_line, end_col) = line_col(text, end);
    let lines: Vec<&str> = text.split('\n').collect();
    if start_line == end_line {
        return vec![(start_line, start_col, end_col)];
    }
    let mut spans = Vec::new();
    let first_len = lines.get(start_line).map(|line| line.len()).unwrap_or(0);
    spans.push((start_line, start_col, first_len));
    for line in start_line + 1..end_line {
        let len = lines.get(line).map(|part| part.len()).unwrap_or(0);
        spans.push((line, 0, len));
    }
    spans.push((end_line, 0, end_col));
    spans
}

fn index_at_point(text: &str, rect: Rect, x: f32, y: f32) -> usize {
    let local_y = y - (rect.y + TEXT_ORIGIN_Y);
    let line = if local_y < 0.0 {
        0
    } else {
        (local_y / CELL_H).floor() as usize
    };
    let local_x = x - (rect.x + TEXT_ORIGIN_X);
    let col = if local_x < 0.0 {
        0
    } else {
        (local_x / CELL_W).floor() as usize
    };
    index_at(text, line, col)
}

fn field_row(world: &World, kind: EditField) -> Option<Entity> {
    let pane = world.entities_with::<Pane>().into_iter().next()?;
    for entity in world.get::<Pane>(pane)?.spawned.iter().copied() {
        if world
            .get::<Field>(entity)
            .is_some_and(|field| field.kind == kind)
        {
            return Some(entity);
        }
    }
    None
}

fn header_label_entity(world: &World, entity: Entity) -> Option<Entity> {
    for child in children_of(world, entity) {
        if world.has::<Label>(child) {
            return Some(child);
        }
    }
    world.has::<Label>(entity).then_some(entity)
}

fn clear_carets(world: &mut World) {
    for entity in world.entities_with::<Caret>() {
        world.remove::<Caret>(entity);
    }
}

fn clear_text_input(world: &mut World) {
    if let Some(input) = world.resource_mut::<Input>() {
        input.chars.clear();
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

fn is_printable_ascii(ch: char) -> bool {
    matches!(ch, ' '..='~')
}

fn move_line(text: &str, index: usize, delta: isize) -> usize {
    let (line, col) = line_col(text, index);
    let next = line.saturating_add_signed(delta);
    index_at(text, next, col)
}

fn line_col(text: &str, index: usize) -> (usize, usize) {
    let index = index.min(text.len());
    let before = &text[..index];
    let line = before.bytes().filter(|b| *b == b'\n').count();
    let col = match before.rfind('\n') {
        Some(i) => index - i - 1,
        None => index,
    };
    (line, col)
}

fn index_at(text: &str, line: usize, col: usize) -> usize {
    let mut start = 0usize;
    for (i, part) in text.split('\n').enumerate() {
        if i == line {
            return start + col.min(part.len());
        }
        start += part.len() + 1;
    }
    text.len()
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
    let mut dy = 0.0f32;
    if rect.y < port.y {
        dy = port.y - rect.y;
    } else if rect.y + rect.h > port.y + port.h {
        dy = (port.y + port.h) - (rect.y + rect.h);
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
