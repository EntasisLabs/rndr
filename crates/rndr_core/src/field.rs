use crate::caret::Caret;
use crate::clipboard::{
    note_clipboard, note_primary, paste_selection, primary_selection, Clipboard,
};
use crate::edit::{edit_string, grapheme_selection};
use crate::entity::Entity;
use crate::event::{send, Event};
use crate::hierarchy::Parent;
use crate::input::{Focus, Input, Key, PointerButton};
use crate::keys::{platform_of, KeyEvent, LogicalKey, Modifiers, NamedKey};
use crate::layout::{hit_test, Rect};
use crate::style::{Label, TextOverflow, TEXT_ORIGIN_X, TEXT_ORIGIN_Y};
use crate::typing::{apply_text_event, insert_plain, EditHistory, EditSurface};
use crate::world::{Component, World};
use crate::wrap::with_text_layout;

impl Component for TextField {}
impl Component for CapturesKeys {}
impl Component for ImePreview {}

/// Uncommitted composition shown at the caret. The label stays unchanged.
#[derive(Clone, Debug, Default)]
pub struct ImePreview {
    /// Composition text. [`Self::cursor`] is a byte range inside this string.
    pub text: String,
    /// Cursor range inside [`Self::text`], in bytes. `None` draws the caret at
    /// the end of the preedit.
    pub cursor: Option<(usize, usize)>,
}

/// Single-line editor on this entity's [`Label`]. No file binding.
///
/// A [`crate::Prompt`] on the same entity replaces this editing behavior:
/// Shift+Enter inserts a newline, the row grows and then scrolls, and plain
/// Enter still submits.
///
/// A focused field applies the key queue during Input, then clears it. Update
/// on that same frame reads [`TextChanged`] and [`Submitted`]. Watching the
/// label from PostUpdate is not required.
///
/// On X11 and Wayland, releasing the mouse over a selection writes
/// [`crate::Clipboard::primary`], and so does a keyboard selection
/// (Primary+A, Shift+arrows, and the other selection motions). Middle-click
/// pastes that selection, or the clipboard when the selection is empty.
#[derive(Clone, Copy, Debug, Default)]
pub struct TextField;

/// The focused field's label changed while it applied this frame's keys.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextChanged {
    /// The [`TextField`] whose label changed.
    pub entity: Entity,
    /// Label contents after this frame's edit, not a diff.
    pub text: String,
}

impl Event for TextChanged {}

/// Plain Enter on a focused field or prompt. The label is unchanged by the key
/// itself. A [`crate::Prompt`] inserts a newline on Shift+Enter instead, and
/// skips this event while an IME preedit or commit is in progress. When
/// [`crate::Prompt::submit_on_enter`] is false, plain Enter inserts a newline
/// and does not send this event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Submitted {
    /// The [`TextField`] that received plain Enter.
    pub entity: Entity,
    /// Label contents at submit time. Enter does not insert a newline.
    pub text: String,
}

impl Event for Submitted {}

/// Focused entity keeps key edges for itself. Tree navigation does not take them.
///
/// Marker with no data. [`crate::apply_focus`] leaves [`crate::Input::key`]
/// alone when the focused entity has this component.
#[derive(Clone, Copy, Debug, Default)]
pub struct CapturesKeys;

pub fn apply_fields(world: &mut World) {
    // A click focuses the field before the caret is placed, so this does not
    // depend on Shell's focus system running earlier in the frame.
    for entity in world.entities_with::<crate::prompt::Prompt>() {
        if let Some(label) = world.get_mut::<Label>(entity) {
            label.overflow = TextOverflow::Wrap;
        }
    }
    focus_pressed_field(world);
    let focus = world.resource::<Focus>().and_then(|focus| focus.entity);
    let mut fields = world.entities_with::<TextField>();
    for entity in world.entities_with::<crate::prompt::Prompt>() {
        if !fields.contains(&entity) {
            fields.push(entity);
        }
    }
    for entity in fields {
        if focus != Some(entity) {
            world.remove::<Caret>(entity);
            continue;
        }
        if world.has::<crate::prompt::Prompt>(entity) {
            crate::prompt::edit_focused(world, entity);
            continue;
        }
        let existed = world.has::<Caret>(entity);
        if !existed {
            let len = world
                .get::<Label>(entity)
                .map(|label| label.text.len())
                .unwrap_or(0);
            world.insert(
                entity,
                Caret {
                    index: len,
                    anchor: len,
                },
            );
        }
        place_from_pointer(world, entity, existed);
        publish_field_primary(world, entity);
        edit_label(world, entity);
    }
}

/// Nearest [`TextField`] or [`crate::Prompt`], including `entity` itself.
pub(crate) fn ancestor_editor(world: &World, mut entity: Entity) -> Option<Entity> {
    loop {
        if !world.is_alive(entity) {
            return None;
        }
        if world.has::<TextField>(entity) || world.has::<crate::prompt::Prompt>(entity) {
            return Some(entity);
        }
        entity = world.get::<Parent>(entity)?.entity;
    }
}

fn place_from_pointer(world: &mut World, entity: crate::entity::Entity, existed: bool) {
    let Some(input) = world.resource::<Input>().cloned() else {
        return;
    };
    if !input.pointer.pressed {
        return;
    }
    let Some(hit) = hit_test(world, input.pointer.position[0], input.pointer.position[1]) else {
        return;
    };
    if ancestor_editor(world, hit) != Some(entity) {
        return;
    }
    let Some(rect) = world.get::<Rect>(entity).copied() else {
        return;
    };
    let text = world
        .get::<Label>(entity)
        .map(|label| label.text.clone())
        .unwrap_or_default();
    let index = with_text_layout(world, entity, &text, |layout| {
        layout.index_at(
            input.pointer.position[0] - rect.x - TEXT_ORIGIN_X,
            input.pointer.position[1] - rect.y - TEXT_ORIGIN_Y,
        )
    });
    if let Some(caret) = world.get_mut::<Caret>(entity) {
        caret.index = index;
        if input.pointer.button == PointerButton::Middle || !(input.shift && existed) {
            caret.anchor = index;
        }
    }
    break_typing_at(world, entity);
}

fn publish_field_primary(world: &mut World, entity: Entity) {
    if !primary_selection(platform_of(world)) {
        return;
    }
    let Some(input) = world.resource::<Input>() else {
        return;
    };
    if input.pointer.button != PointerButton::Left || !input.pointer.released {
        return;
    }
    let position = input.pointer.position;
    let Some(hit) = hit_test(world, position[0], position[1]) else {
        return;
    };
    if ancestor_editor(world, hit) != Some(entity) {
        return;
    }
    let selected = label_selection(world, entity);
    note_primary(world, &selected);
}

/// Text to insert for a middle-click on `entity`, when this platform has a
/// primary selection and the press landed in the field.
pub(crate) fn middle_paste_text(world: &World, entity: Entity) -> Option<String> {
    if !primary_selection(platform_of(world)) {
        return None;
    }
    let input = world.resource::<Input>()?;
    if input.pointer.button != PointerButton::Middle || !input.pointer.pressed {
        return None;
    }
    let hit = hit_test(world, input.pointer.position[0], input.pointer.position[1])?;
    if ancestor_editor(world, hit) != Some(entity) {
        return None;
    }
    let text = paste_selection(world);
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

/// Publish the field's selection when a key this frame changed the range.
///
/// Mouse release has its own write. An empty range leaves the primary
/// selection untouched, matching [`note_primary`].
pub(crate) fn publish_keyboard_primary(
    world: &mut World,
    entity: Entity,
    before_index: usize,
    before_anchor: usize,
) {
    if !primary_selection(platform_of(world)) {
        return;
    }
    let (index, anchor) = world
        .get::<Caret>(entity)
        .map(|caret| (caret.index, caret.anchor))
        .unwrap_or((before_index, before_anchor));
    if index == before_index && anchor == before_anchor {
        return;
    }
    let selected = label_selection(world, entity);
    note_primary(world, &selected);
}

pub(crate) fn label_selection(world: &World, entity: Entity) -> String {
    let text = world
        .get::<Label>(entity)
        .map(|label| label.text.clone())
        .unwrap_or_default();
    let (index, anchor) = world
        .get::<Caret>(entity)
        .map(|caret| (caret.index, caret.anchor))
        .unwrap_or((text.len(), text.len()));
    let (start, end) = grapheme_selection(&text, anchor, index);
    text[start..end].to_string()
}

fn paste_field_primary(world: &mut World, entity: Entity) {
    let Some(paste) = middle_paste_text(world, entity) else {
        return;
    };
    let mut text = world
        .get::<Label>(entity)
        .map(|label| label.text.clone())
        .unwrap_or_default();
    let mut index = world
        .get::<Caret>(entity)
        .map(|caret| caret.index.min(text.len()))
        .unwrap_or(text.len());
    let mut anchor = index;
    let mut history = world.remove::<EditHistory>(entity).unwrap_or_default();
    insert_plain(
        &mut EditSurface {
            text: &mut text,
            index: &mut index,
            anchor: &mut anchor,
        },
        &mut history,
        &paste,
    );
    if let Some(label) = world.get_mut::<Label>(entity) {
        label.text = text;
    }
    if let Some(caret) = world.get_mut::<Caret>(entity) {
        caret.index = index;
        caret.anchor = anchor;
    }
    world.insert(entity, history);
}

pub(crate) fn break_typing_at(world: &mut World, entity: Entity) {
    if let Some(history) = world.get_mut::<EditHistory>(entity) {
        history.break_run();
    }
}

fn focus_pressed_field(world: &mut World) {
    let Some(input) = world.resource::<Input>() else {
        return;
    };
    if !input.pointer.pressed || world.resource::<Focus>().is_none() {
        return;
    }
    let position = input.pointer.position;
    let Some(hit) = hit_test(world, position[0], position[1]) else {
        return;
    };
    let Some(field) = ancestor_editor(world, hit) else {
        return;
    };
    if let Some(focus) = world.resource_mut::<Focus>() {
        focus.entity = Some(field);
    }
}

fn edit_label(world: &mut World, entity: crate::entity::Entity) {
    let frame = match world.resource::<Input>() {
        Some(input) => crate::ime::text_frame(input),
        None => return,
    };
    let key = frame.legacy_key;
    let shift = frame.shift;
    let chars = frame.chars;
    let commit = frame.commit;
    let keys = frame.keys;
    let before = world
        .get::<Label>(entity)
        .map(|label| label.text.clone())
        .unwrap_or_default();
    paste_field_primary(world, entity);
    let submitted = wants_submit(key, &keys);
    if keys.is_empty() {
        edit_label_legacy(world, entity, key, shift, &chars, &commit);
    } else {
        edit_label_queue(world, entity, &keys, &commit);
    }
    let after = world
        .get::<Label>(entity)
        .map(|label| label.text.clone())
        .unwrap_or_default();
    if before != after {
        send(
            world,
            TextChanged {
                entity,
                text: after.clone(),
            },
        );
    }
    if submitted {
        send(
            world,
            Submitted {
                entity,
                text: after,
            },
        );
    }
    sync_preview(world, entity);
}

fn wants_submit(key: Option<Key>, keys: &[KeyEvent]) -> bool {
    if keys.is_empty() {
        return key == Some(Key::Enter);
    }
    keys.iter().any(|event| {
        event.logical == LogicalKey::Named(NamedKey::Enter) && !event.modifiers.command()
    })
}

fn edit_label_legacy(
    world: &mut World,
    entity: crate::entity::Entity,
    key: Option<Key>,
    shift: bool,
    chars: &str,
    commit: &str,
) {
    let mut typed = chars.to_string();
    typed.push_str(commit);
    let mut text = world
        .get::<Label>(entity)
        .map(|label| label.text.clone())
        .unwrap_or_default();
    let (before_index, before_anchor) = world
        .get::<Caret>(entity)
        .map(|caret| (caret.index, caret.anchor))
        .unwrap_or((text.len(), text.len()));
    let (index, anchor) = edit_string(
        &mut text,
        before_index,
        before_anchor,
        key,
        shift,
        &typed,
        false,
    );
    if let Some(label) = world.get_mut::<Label>(entity) {
        label.text = text;
    }
    if let Some(caret) = world.get_mut::<Caret>(entity) {
        caret.index = index;
        caret.anchor = anchor;
    }
    publish_keyboard_primary(world, entity, before_index, before_anchor);
    if let Some(input) = world.resource_mut::<Input>() {
        input.chars.clear();
        input.ime_commit.clear();
        if matches!(input.key, Some(Key::Left | Key::Right | Key::Backspace)) {
            input.key = None;
        }
    }
}

fn edit_label_queue(
    world: &mut World,
    entity: crate::entity::Entity,
    keys: &[KeyEvent],
    commit: &str,
) {
    let platform = platform_of(world);
    let mut text = world
        .get::<Label>(entity)
        .map(|label| label.text.clone())
        .unwrap_or_default();
    let (mut index, mut anchor) = world
        .get::<Caret>(entity)
        .map(|caret| (caret.index, caret.anchor))
        .unwrap_or((text.len(), text.len()));
    let (before_index, before_anchor) = (index, anchor);
    let mut history = world.remove::<EditHistory>(entity).unwrap_or_default();
    let mut clip = world
        .resource::<Clipboard>()
        .map(|clipboard| clipboard.text.clone())
        .unwrap_or_default();
    let mut copied = None;
    let mut clipboard_action = false;
    let now = crate::typing::frame_now(world);
    for event in keys {
        let step = apply_text_event(
            &mut EditSurface {
                text: &mut text,
                index: &mut index,
                anchor: &mut anchor,
            },
            event,
            platform,
            false,
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
            false,
            now,
            &mut history,
            &clip,
        );
    }
    if let Some(label) = world.get_mut::<Label>(entity) {
        label.text = text;
    }
    if let Some(caret) = world.get_mut::<Caret>(entity) {
        caret.index = index;
        caret.anchor = anchor;
    }
    publish_keyboard_primary(world, entity, before_index, before_anchor);
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

pub(crate) fn sync_preview(world: &mut World, entity: crate::entity::Entity) {
    let (preedit, cursor) = world
        .resource::<Input>()
        .map(|input| (input.ime_preedit.clone(), input.ime_cursor))
        .unwrap_or_default();
    if preedit.is_empty() {
        world.remove::<ImePreview>(entity);
    } else {
        world.insert(
            entity,
            ImePreview {
                text: preedit,
                cursor,
            },
        );
    }
}
