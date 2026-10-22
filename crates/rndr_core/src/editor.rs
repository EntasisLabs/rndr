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
use crate::text::{style_of, text_engine};
use crate::typing::{apply_text_event, EditHistory, EditSurface};
use crate::world::{Component, World};
use crate::wrap::with_text_layout;

impl Component for Buffer {}
impl Component for Editor {}
impl Component for Gutter {}

/// Gap buffer plus a line-start index. Indices are byte offsets.
#[derive(Clone, Debug)]
pub struct Buffer {
    data: Vec<u8>,
    gap_start: usize,
    gap_end: usize,
    lines: Vec<usize>,
    pub caret: usize,
    pub anchor: usize,
}

impl Buffer {
    /// Infallible document constructor. This is not `FromStr`: a gap buffer
    /// accepts any `&str`, and a `Result` would only change every call site.
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

    pub fn len(&self) -> usize {
        self.data.len() - (self.gap_end - self.gap_start)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn line_count(&self) -> u32 {
        self.lines.len() as u32
    }

    pub fn line_start(&self, line: u32) -> usize {
        self.lines.get(line as usize).copied().unwrap_or(self.len())
    }

    pub fn text(&self) -> String {
        self.slice(0, self.len())
    }

    pub fn slice(&self, start: usize, end: usize) -> String {
        let end = end.min(self.len());
        let start = start.min(end);
        let mut bytes = Vec::with_capacity(end - start);
        for index in start..end {
            bytes.push(self.byte(index));
        }
        String::from_utf8_lossy(&bytes).into_owned()
    }

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

/// Scroll offset, in points, for a [`Buffer`] on the same entity.
#[derive(Clone, Copy, Debug)]
pub struct Editor {
    pub offset: f32,
}

impl Default for Editor {
    fn default() -> Self {
        Self { offset: 0.0 }
    }
}

/// Sibling column that shows line numbers for the visible window.
#[derive(Clone, Copy, Debug, Default)]
pub struct Gutter;

pub fn apply_editor(world: &mut World) {
    let editors = world.entities_with::<Editor>();
    for entity in editors {
        if !world.has::<Buffer>(entity) {
            continue;
        }
        let focused = world.resource::<Focus>().and_then(|focus| focus.entity) == Some(entity);
        if focused {
            edit_focused(world, entity);
            place_from_pointer(world, entity);
        }
        publish_window(world, entity);
        if focused {
            sync_ime_preview(world, entity);
        } else {
            world.remove::<crate::field::ImePreview>(entity);
        }
    }
}

fn edit_focused(world: &mut World, entity: crate::entity::Entity) {
    let queued = world
        .resource::<Input>()
        .is_some_and(|input| !input.keys.is_empty());
    if queued {
        edit_queue(world, entity);
        return;
    }
    let (key, shift, chars, commit) = match world.resource::<Input>() {
        Some(input) => (
            input.key,
            input.shift,
            input.chars.clone(),
            input.ime_commit.clone(),
        ),
        None => return,
    };
    let Some(buffer) = world.get_mut::<Buffer>(entity) else {
        return;
    };
    let snapshot = buffer.text();
    let len = buffer.len();
    let mut index = buffer.caret.min(len);
    let mut anchor = buffer.anchor.min(len);
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

fn edit_queue(world: &mut World, entity: crate::entity::Entity) {
    let platform = platform_of(world);
    let (keys, commit) = match world.resource::<Input>() {
        Some(input) => (input.keys.clone(), input.ime_commit.clone()),
        None => return,
    };
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
    for event in &keys {
        let step = apply_text_event(
            &mut EditSurface {
                text: &mut text,
                index: &mut index,
                anchor: &mut anchor,
            },
            event,
            platform,
            true,
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
        let commit_event = KeyEvent {
            logical: LogicalKey::Unidentified,
            text: Some(commit),
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
            &mut history,
            &clip,
        );
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

fn replace_buffer(buffer: &mut Buffer, text: &str, caret: usize, anchor: usize) {
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

fn place_from_pointer(world: &mut World, entity: crate::entity::Entity) {
    let Some(input) = world.resource::<Input>().cloned() else {
        return;
    };
    if !input.pointer.pressed {
        return;
    }
    let Some(hit) = hit_test(world, input.pointer.position[0], input.pointer.position[1]) else {
        return;
    };
    if ancestor_with::<Editor>(world, hit) != Some(entity) {
        return;
    }
    let Some(rect) = world.get::<Rect>(entity).copied() else {
        return;
    };
    let text = world
        .get::<Label>(entity)
        .map(|label| label.text.clone())
        .unwrap_or_default();
    let base = window_base(world, entity);
    let index = with_text_layout(world, entity, &text, |layout| {
        layout.index_at(
            input.pointer.position[0] - rect.x - TEXT_ORIGIN_X,
            input.pointer.position[1] - rect.y - TEXT_ORIGIN_Y,
        )
    }) + base;
    if let Some(buffer) = world.get_mut::<Buffer>(entity) {
        let index = index.min(buffer.len());
        buffer.caret = index;
        if !input.shift {
            buffer.anchor = index;
        }
    }
}

fn publish_window(world: &mut World, entity: crate::entity::Entity) {
    let height = world.get::<Rect>(entity).map(|rect| rect.h).unwrap_or(0.0);
    let offset = world
        .get::<Editor>(entity)
        .map(|editor| editor.offset)
        .unwrap_or(0.0);
    let line_h = text_engine(world).line_height(&style_of(world)).max(1.0);
    let Some(buffer) = world.get::<Buffer>(entity) else {
        return;
    };
    let count = buffer.line_count();
    let (first, last, base, end) = if count == 0 || height <= 0.0 {
        (0, 0, 0, 0)
    } else {
        let first = ((offset.max(0.0) / line_h).floor() as u32).min(count - 1);
        let rows = ((height / line_h).ceil() as u32).max(1);
        let last = first.saturating_add(rows).min(count);
        let base = buffer.line_start(first);
        let end = if last >= count {
            buffer.len()
        } else {
            buffer.line_start(last)
        };
        (first, last, base, end)
    };
    let window = buffer.slice(base, end);
    let caret = buffer.caret;
    let anchor = buffer.anchor;
    if let Some(label) = world.get_mut::<Label>(entity) {
        label.text = window;
    }
    if height > 0.0 && caret >= base && caret <= end {
        world.insert(
            entity,
            Caret {
                index: caret - base,
                anchor: anchor.clamp(base, end) - base,
            },
        );
    } else {
        world.remove::<Caret>(entity);
    }
    if let Some(gutter) = gutter_for(world, entity) {
        let mut numbers = String::new();
        for line in first..last {
            if !numbers.is_empty() {
                numbers.push('\n');
            }
            numbers.push_str(&(line + 1).to_string());
        }
        if let Some(label) = world.get_mut::<Label>(gutter) {
            label.text = numbers;
        }
    }
}

pub(crate) fn window_base(world: &World, entity: crate::entity::Entity) -> usize {
    let height = world.get::<Rect>(entity).map(|rect| rect.h).unwrap_or(0.0);
    let offset = world
        .get::<Editor>(entity)
        .map(|editor| editor.offset)
        .unwrap_or(0.0);
    let line_h = text_engine(world).line_height(&style_of(world)).max(1.0);
    let Some(buffer) = world.get::<Buffer>(entity) else {
        return 0;
    };
    let count = buffer.line_count();
    if count == 0 || height <= 0.0 {
        return 0;
    }
    let first = ((offset.max(0.0) / line_h).floor() as u32).min(count - 1);
    buffer.line_start(first)
}

fn gutter_for(world: &World, entity: crate::entity::Entity) -> Option<crate::entity::Entity> {
    let parent = world.get::<Parent>(entity)?.entity;
    children_of(world, parent)
        .into_iter()
        .find(|child| world.has::<Gutter>(*child))
}

impl Component for Decorations {}
impl Component for DecorationMarks {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecorationKind {
    Find,
    Diagnostic,
}

#[derive(Clone, Copy, Debug)]
pub struct Decoration {
    pub start: usize,
    pub end: usize,
    pub kind: DecorationKind,
}

#[derive(Clone, Debug, Default)]
pub struct Decorations {
    pub list: Vec<Decoration>,
}

/// Quads relative to the text origin. Empty when the range is offscreen.
#[derive(Clone, Debug, Default)]
pub struct DecorationMarks {
    pub marks: Vec<DecorationMark>,
}

#[derive(Clone, Copy, Debug)]
pub struct DecorationMark {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
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
