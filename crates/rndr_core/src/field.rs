use crate::caret::Caret;
use crate::clipboard::{note_clipboard, Clipboard};
use crate::edit::edit_string;
use crate::entity::Entity;
use crate::event::{send, Event};
use crate::input::{Focus, Input, Key};
use crate::keys::{platform_of, KeyEvent, LogicalKey, Modifiers, NamedKey};
use crate::layout::{ancestor_with, hit_test, Rect};
use crate::style::{Label, TEXT_ORIGIN_X, TEXT_ORIGIN_Y};
use crate::typing::{apply_text_event, EditHistory, EditSurface};
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
/// A focused field applies the key queue during Input, then clears it. Update
/// on that same frame reads [`TextChanged`] and [`Submitted`]. Watching the
/// label from PostUpdate is not required.
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

/// Plain Enter on a focused field. The label is unchanged by the key itself.
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
    focus_pressed_field(world);
    let focus = world.resource::<Focus>().and_then(|focus| focus.entity);
    let fields = world.entities_with::<TextField>();
    for entity in fields {
        if focus != Some(entity) {
            world.remove::<Caret>(entity);
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
        edit_label(world, entity);
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
    if ancestor_with::<TextField>(world, hit) != Some(entity) {
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
        if !(input.shift && existed) {
            caret.anchor = index;
        }
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
    let Some(field) = ancestor_with::<TextField>(world, hit) else {
        return;
    };
    if let Some(focus) = world.resource_mut::<Focus>() {
        focus.entity = Some(field);
    }
}

fn edit_label(world: &mut World, entity: crate::entity::Entity) {
    let (key, shift, chars, commit, keys) = match world.resource::<Input>() {
        Some(input) => (
            input.key,
            input.shift,
            input.chars.clone(),
            input.ime_commit.clone(),
            input.keys.clone(),
        ),
        None => return,
    };
    let before = world
        .get::<Label>(entity)
        .map(|label| label.text.clone())
        .unwrap_or_default();
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
    let (index, anchor) = world
        .get::<Caret>(entity)
        .map(|caret| (caret.index, caret.anchor))
        .unwrap_or((text.len(), text.len()));
    let (index, anchor) = edit_string(&mut text, index, anchor, key, shift, &typed, false);
    if let Some(label) = world.get_mut::<Label>(entity) {
        label.text = text;
    }
    if let Some(caret) = world.get_mut::<Caret>(entity) {
        caret.index = index;
        caret.anchor = anchor;
    }
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
    let mut history = world.remove::<EditHistory>(entity).unwrap_or_default();
    let mut clip = world
        .resource::<Clipboard>()
        .map(|clipboard| clipboard.text.clone())
        .unwrap_or_default();
    let mut copied = None;
    let mut clipboard_action = false;
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

fn sync_preview(world: &mut World, entity: crate::entity::Entity) {
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
