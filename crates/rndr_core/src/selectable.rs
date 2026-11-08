//! Pointer selection and copy for read-only labels.
//!
//! [`Selectable`] on a display [`Label`] makes its text selectable once
//! [`SelectablePlugin`] is added. A press places the selection, a drag
//! extends it, a double press selects a word and a triple press selects the
//! paragraph; dragging after a double or triple press extends by whole words
//! or paragraphs. Shift+press extends the current selection. The last label
//! pressed owns Select All and Copy until a press lands elsewhere or another
//! entity takes focus. On platforms with a primary selection the selected
//! text is published there when the drag ends.
//!
//! Selection is paint-only state: it leaves the label's box metrics, shaping
//! and layout unchanged. A press on a selectable label releases focus, as a
//! press on markdown text does, so a selectable label inside a button takes
//! the press from the button.

use std::time::Duration;

use unicode_segmentation::UnicodeSegmentation;

use crate::app::{App, Plugin, Schedule};
use crate::clipboard::{note_clipboard, note_primary, primary_selection};
use crate::entity::Entity;
use crate::input::{CursorStyle, Focus, Input, MotionWatch, PointerButton};
use crate::keys::{platform_of, standard_action, EditAction, Key};
use crate::layout::{hit_test, Rect};
use crate::style::{label_box, Label};
use crate::world::{Component, World};

/// Presses closer together than this, at most [`MULTI_PRESS_SLOP`] apart,
/// count as a double or triple press.
pub const MULTI_PRESS_INTERVAL: Duration = Duration::from_millis(500);
/// Largest pointer travel, in points, between presses of one multi-press.
pub const MULTI_PRESS_SLOP: f32 = 4.0;

/// Selection state of a read-only selectable [`Label`], in label bytes.
///
/// Insert `Selectable::default()` to opt a label in. `anchor` is where the
/// selection started and `head` where it ends; equal values select nothing.
/// Both are kept on UTF-8 boundaries of the label text.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Selectable {
    /// Byte where the selection started.
    pub anchor: usize,
    /// Byte where the selection ends; may be before `anchor`.
    pub head: usize,
}
impl Component for Selectable {}

impl Selectable {
    /// Selected byte range, start first.
    pub fn range(&self) -> (usize, usize) {
        (self.anchor.min(self.head), self.anchor.max(self.head))
    }

    /// True when nothing is selected.
    pub fn is_empty(&self) -> bool {
        self.anchor == self.head
    }

    /// Selected slice of `text`, or `""` when the range is not on boundaries.
    pub fn selected<'a>(&self, text: &'a str) -> &'a str {
        let (start, end) = self.range();
        text.get(start..end.min(text.len())).unwrap_or("")
    }
}

/// Adds pointer selection, Select All and Copy for [`Selectable`] labels.
pub struct SelectablePlugin;

impl Plugin for SelectablePlugin {
    fn build(self, app: &mut App) {
        app.add_system(Schedule::Input, selectable_input);
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Unit {
    #[default]
    Char,
    Word,
    Paragraph,
}

#[derive(Clone, Copy, Debug)]
struct LastPress {
    entity: Entity,
    at: Duration,
    position: [f32; 2],
    count: u8,
}

#[derive(Clone, Copy, Debug, Default)]
struct SelectState {
    drag: Option<Entity>,
    engaged: Option<Entity>,
    unit: Unit,
    origin: (usize, usize),
    last: Option<LastPress>,
    text_cursor: bool,
}

/// Input system installed by [`SelectablePlugin`].
pub fn selectable_input(world: &mut World) {
    let Some(input) = world.resource::<Input>().cloned() else {
        return;
    };
    let mut state = world.resource::<SelectState>().copied().unwrap_or_default();
    if state
        .engaged
        .is_some_and(|entity| !world.has::<Selectable>(entity))
    {
        state.engaged = None;
        state.drag = None;
    }
    let [x, y] = input.pointer.position;
    if input.pointer.pressed && input.pointer.button == PointerButton::Left {
        press(world, &mut state, &input, x, y);
    } else if input.pointer.down || input.pointer.released {
        if let Some(entity) = state.drag {
            if let Some(byte) = byte_at(world, entity, x, y) {
                drag(world, &state, entity, byte);
            }
        }
    }
    if input.pointer.released {
        if let Some(entity) = state.drag.take() {
            let text = selected_text(world, entity);
            if !text.is_empty() && primary_selection(platform_of(world)) {
                note_primary(world, &text);
            }
        }
    }
    let focused = world.resource::<Focus>().and_then(|focus| focus.entity);
    if focused.is_some() {
        state.engaged = None;
    }
    if let Some(entity) = state.engaged {
        apply_keys(world, entity, &input);
    }
    update_cursor(world, &mut state, x, y);
    world.insert_resource(state);
}

fn press(world: &mut World, state: &mut SelectState, input: &Input, x: f32, y: f32) {
    let target = hit_test(world, x, y)
        .filter(|hit| world.has::<Selectable>(*hit) && world.has::<Label>(*hit));
    for entity in world.entities_with::<Selectable>() {
        if Some(entity) == target {
            continue;
        }
        if let Some(selection) = world.get_mut::<Selectable>(entity) {
            if !selection.is_empty() {
                *selection = Selectable::default();
            }
        }
    }
    let Some(entity) = target else {
        state.engaged = None;
        state.drag = None;
        return;
    };
    let Some(byte) = byte_at(world, entity, x, y) else {
        return;
    };
    crate::interaction::release_focus(world);
    let now = world
        .resource::<crate::clock::Time>()
        .map(|time| time.now)
        .unwrap_or_default();
    let count = match state.last {
        Some(last)
            if last.entity == entity
                && now.saturating_sub(last.at) <= MULTI_PRESS_INTERVAL
                && (last.position[0] - x).hypot(last.position[1] - y) <= MULTI_PRESS_SLOP =>
        {
            last.count % 3 + 1
        }
        _ => 1,
    };
    state.last = Some(LastPress {
        entity,
        at: now,
        position: [x, y],
        count,
    });
    let text = world
        .get::<Label>(entity)
        .map(|label| label.text.clone())
        .unwrap_or_default();
    let extend = input.shift && count == 1 && state.engaged == Some(entity);
    state.unit = match count {
        2 => Unit::Word,
        3 => Unit::Paragraph,
        _ => Unit::Char,
    };
    let (start, end) = unit_range(&text, byte, state.unit);
    if let Some(selection) = world.get_mut::<Selectable>(entity) {
        if extend {
            selection.head = byte;
            state.origin = (selection.anchor, selection.anchor);
        } else {
            *selection = Selectable {
                anchor: start,
                head: end,
            };
            state.origin = (start, end);
        }
    }
    state.engaged = Some(entity);
    state.drag = Some(entity);
}

fn drag(world: &mut World, state: &SelectState, entity: Entity, byte: usize) {
    let text = world
        .get::<Label>(entity)
        .map(|label| label.text.clone())
        .unwrap_or_default();
    let (start, end) = unit_range(&text, byte, state.unit);
    let (origin_start, origin_end) = state.origin;
    let next = if byte < origin_start {
        Selectable {
            anchor: origin_end,
            head: start,
        }
    } else {
        Selectable {
            anchor: origin_start,
            head: end.max(origin_end),
        }
    };
    if let Some(selection) = world.get_mut::<Selectable>(entity) {
        if *selection != next {
            *selection = next;
        }
    }
}

fn apply_keys(world: &mut World, entity: Entity, input: &Input) {
    let platform = platform_of(world);
    let mut select_all = false;
    let mut copy = input.key == Some(Key::Copy);
    for event in &input.keys {
        match standard_action(platform, event) {
            Some(EditAction::SelectAll) => select_all = true,
            Some(EditAction::Copy) => copy = true,
            _ => {}
        }
    }
    if !select_all && !copy {
        return;
    }
    let len = world
        .get::<Label>(entity)
        .map_or(0, |label| label.text.len());
    if select_all {
        if let Some(selection) = world.get_mut::<Selectable>(entity) {
            *selection = Selectable {
                anchor: 0,
                head: len,
            };
        }
        let text = selected_text(world, entity);
        if !text.is_empty() && primary_selection(platform) {
            note_primary(world, &text);
        }
    }
    let mut acted = select_all;
    if copy {
        let text = selected_text(world, entity);
        if !text.is_empty() {
            note_clipboard(world, Some(&text));
            acted = true;
        }
    }
    if acted {
        if let Some(input) = world.resource_mut::<Input>() {
            if input.key == Some(Key::Copy) {
                input.key = None;
            }
            input.keys.retain(|event| {
                !matches!(
                    standard_action(platform, event),
                    Some(EditAction::Copy | EditAction::SelectAll)
                )
            });
        }
    }
}

fn update_cursor(world: &mut World, state: &mut SelectState, x: f32, y: f32) {
    if world.entities_with::<Selectable>().is_empty() {
        return;
    }
    world.insert_resource(MotionWatch { active: true });
    let over = state.drag.is_some()
        || hit_test(world, x, y)
            .is_some_and(|hit| world.has::<Selectable>(hit) && world.has::<Label>(hit));
    if over {
        world.insert_resource(CursorStyle::Text);
        state.text_cursor = true;
    } else if state.text_cursor {
        if world.resource::<CursorStyle>().copied() == Some(CursorStyle::Text) {
            world.insert_resource(CursorStyle::Arrow);
        }
        state.text_cursor = false;
    }
}

fn selected_text(world: &World, entity: Entity) -> String {
    match (world.get::<Selectable>(entity), world.get::<Label>(entity)) {
        (Some(selection), Some(label)) => selection.selected(&label.text).to_string(),
        _ => String::new(),
    }
}

fn byte_at(world: &World, entity: Entity, x: f32, y: f32) -> Option<usize> {
    let rect = world.get::<Rect>(entity).copied()?;
    let layout = world.get::<crate::font::TextLayout>(entity)?;
    let text = &world.get::<Label>(entity)?.text;
    let metrics = label_box(world, entity);
    let mut byte = layout
        .index_at(x - rect.x - metrics.inset_x, y - rect.y - metrics.inset_y)
        .min(text.len());
    while !text.is_char_boundary(byte) {
        byte -= 1;
    }
    Some(byte)
}

fn unit_range(text: &str, byte: usize, unit: Unit) -> (usize, usize) {
    match unit {
        Unit::Char => (byte, byte),
        Unit::Word => word_range(text, byte),
        Unit::Paragraph => {
            let start = text[..byte].rfind('\n').map_or(0, |at| at + 1);
            let end = text[byte..].find('\n').map_or(text.len(), |at| byte + at);
            (start, end)
        }
    }
}

fn word_range(text: &str, byte: usize) -> (usize, usize) {
    let mut fallback = (byte, byte);
    for (start, word) in text.split_word_bound_indices() {
        let end = start + word.len();
        if byte >= start && byte < end {
            return (start, end);
        }
        if byte == end {
            fallback = (start, end);
        }
    }
    fallback
}
