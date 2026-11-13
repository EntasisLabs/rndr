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

/// Opt-in synchronous transaction boundary on a single-line TextField.
/// The canonical generic loop applies each real queued key to the existing
/// field/history engine before delivering this callback and the next key.
/// Tab remains canonical focus traversal; Escape cancels; repeat Enter is ignored.
/// Hosts own validation, committed models and cancellation on model reconciliation.
#[derive(Clone, Copy)]
pub struct FieldTransactions(pub fn(&mut World, Entity, FieldTransaction));
impl Component for FieldTransactions {}

/// A draft edit or explicit transaction request, never committed model mutation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FieldTransaction {
    /// Current draft after one chronological edit.
    Changed(String),
    /// Non-repeat plain Enter snapshot at exactly this input boundary.
    Submit(String),
    /// Plain Escape request. The owner decides how to restore its draft.
    Cancel,
    /// Focus leaves this field. None denotes window blur or loss of focus.
    FocusLost(Option<Entity>),
}
fn transaction_notify(world: &mut World, entity: Entity, signal: FieldTransaction) {
    if let Some(handler) = world.get::<FieldTransactions>(entity).copied() {
        (handler.0)(world, entity, signal);
    }
}
pub(crate) fn transaction_focus_lost(world: &mut World, next: Option<Entity>) {
    let old = world.resource::<Focus>().and_then(|f| f.entity);
    notify_focus_lost(world, old, next);
}
/// Opt-in synchronous notification when focus leaves this entity's subtree.
/// Called before the next queued input, including window blur (`next = None`).
/// Moving between descendants does not notify. This does not change focus policy.
#[derive(Clone, Copy)]
pub struct FocusBoundary(pub fn(&mut World, Entity, Option<Entity>));
impl Component for FocusBoundary {}
fn focus_ancestors(world: &World, mut entity: Option<Entity>) -> Vec<Entity> {
    let mut result = Vec::new();
    for _ in 0..256 {
        let Some(e) = entity else {
            break;
        };
        if result.contains(&e) {
            break;
        }
        result.push(e);
        entity = world
            .get::<crate::FocusOwner>(e)
            .map(|p| p.0)
            .or_else(|| world.get::<Parent>(e).map(|p| p.entity));
    }
    result
}
pub(crate) fn notify_focus_lost(world: &mut World, old: Option<Entity>, next: Option<Entity>) {
    let destinations = focus_ancestors(world, next);
    let departures = focus_ancestors(world, old);
    for e in departures {
        if !destinations.contains(&e) {
            if let Some(handler) = world.get::<FocusBoundary>(e).copied() {
                (handler.0)(world, e, next);
            }
        }
    }

    if let Some(e) =
        old.filter(|e| world.has::<TextField>(*e) && world.has::<FieldTransactions>(*e))
    {
        break_typing_at(world, e);
        world.remove::<ImePreview>(e);
        transaction_notify(world, e, FieldTransaction::FocusLost(next));
    }
}
fn ensure_field_caret(world: &mut World, entity: Entity) {
    if !world.has::<Caret>(entity) {
        let len = world
            .get::<Label>(entity)
            .map(|l| l.text.len())
            .unwrap_or(0);
        world.insert(
            entity,
            Caret {
                index: len,
                anchor: len,
            },
        );
    }
}
pub(crate) fn prepare_transaction_field(world: &mut World) {
    let Some(entity) = world
        .resource::<Focus>()
        .and_then(|f| f.entity)
        .filter(|e| world.has::<TextField>(*e) && world.has::<FieldTransactions>(*e))
    else {
        return;
    };
    let existed = world.has::<Caret>(entity);
    ensure_field_caret(world, entity);
    place_from_pointer(world, entity, existed);
    publish_field_primary(world, entity);
    // The aggregate legacy shim has no per-character ordering. Its text
    // precedes Tab, just as rich queued text does; apply it at the current
    // field boundary before canonical traversal can move focus. Rich queues
    // remain authoritative and are never replayed through this shim.
    let legacy_text_before_tab = world.resource::<Input>().is_some_and(|input| {
        input.keys.is_empty()
            && input.key == Some(Key::Tab)
            && !input.chars.is_empty()
            && input.ime_preedit.is_empty()
            && input.ime_commit.is_empty()
    });
    if legacy_text_before_tab {
        crate::interaction::sanitize_readonly_input(world);
        let frame = crate::ime::text_frame(world.resource::<Input>().unwrap());
        let before = world.get::<Label>(entity).map(|label| label.text.clone());
        if !frame.chars.is_empty() {
            edit_label_queue(world, entity, &[], &frame.chars, true);
        }
        let after = world.get::<Label>(entity).map(|label| label.text.clone());
        if before != after {
            if let Some(text) = after {
                send(
                    world,
                    TextChanged {
                        entity,
                        text: text.clone(),
                    },
                );
                transaction_notify(world, entity, FieldTransaction::Changed(text));
            }
        }
        sync_preview(world, entity);
    }
}
pub(crate) fn transaction_key(world: &mut World, entity: Entity, event: &KeyEvent) -> bool {
    if !world.has::<TextField>(entity) || !world.has::<FieldTransactions>(entity) {
        return false;
    }
    if event.released || event.synthetic {
        return true;
    }
    if !crate::interaction::access_details(world, entity).focus {
        return true;
    }
    // Use the canonical IME filter, without replaying the Input resource. The
    // aggregate commit is inserted once at the field stage, never as a submit.
    let mut one = Input {
        keys: vec![event.clone()],
        ..Input::default()
    };
    if let Some(input) = world.resource::<Input>() {
        one.ime_enabled = input.ime_enabled;
        one.ime_preedit = input.ime_preedit.clone();
        one.ime_commit = input.ime_commit.clone();
    }
    let filtered = crate::ime::text_frame(&one);
    let Some(event) = filtered.keys.first() else {
        return true;
    };
    let plain = !event.modifiers.command() && !event.modifiers.alt;
    if plain && event.logical == LogicalKey::Named(crate::NamedKey::Tab) {
        // Keep composition and aggregate commits on their owning field. Tab
        // may be an input-method candidate action, not a focus request.
        if world
            .resource::<Input>()
            .is_some_and(|input| !input.ime_preedit.is_empty() || !input.ime_commit.is_empty())
        {
            return true;
        }
        break_typing_at(world, entity);
        return false;
    }
    ensure_field_caret(world, entity);
    if plain
        && matches!(
            event.logical,
            LogicalKey::Named(crate::NamedKey::Enter | crate::NamedKey::Escape)
        )
    {
        break_typing_at(world, entity);
        if !event.repeat && !crate::interaction::readonly(world, entity) {
            if event.logical == LogicalKey::Named(crate::NamedKey::Escape) {
                transaction_notify(world, entity, FieldTransaction::Cancel);
            } else if world
                .resource::<Input>()
                .is_none_or(|i| i.ime_preedit.is_empty() && i.ime_commit.is_empty())
            {
                let text = world
                    .get::<Label>(entity)
                    .map(|l| l.text.clone())
                    .unwrap_or_default();
                send(
                    world,
                    Submitted {
                        entity,
                        text: text.clone(),
                    },
                );
                transaction_notify(world, entity, FieldTransaction::Submit(text));
            }
        }
        return true;
    }
    let before = world
        .get::<Label>(entity)
        .map(|l| l.text.clone())
        .unwrap_or_default();
    // The exact native event is borrowed, not synthesized/replayed. Readonly
    // filtering uses the same canonical sanitizer/selection-only key policy.
    let mut readonly_event = event.clone();
    let event = if crate::interaction::readonly(world, entity) {
        if !crate::interaction::readonly_field_key(event) {
            return true;
        }
        readonly_event.text = None;
        &readonly_event
    } else {
        event
    };
    edit_label_queue(world, entity, std::slice::from_ref(event), "", false);
    let after = world
        .get::<Label>(entity)
        .map(|l| l.text.clone())
        .unwrap_or_default();
    if before != after {
        send(
            world,
            TextChanged {
                entity,
                text: after.clone(),
            },
        );
        transaction_notify(world, entity, FieldTransaction::Changed(after));
    }
    true
}

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
    crate::interaction::sanitize_readonly_input(world);
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
        if world.has::<crate::prompt::Prompt>(entity)
            && !crate::interaction::readonly(world, entity)
        {
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
        if world.has::<FieldTransactions>(entity) {
            // Queued events were already consumed chronologically. Legacy chars
            // and IME commits still use the existing engine at the frame boundary.
            let before = world
                .get::<Label>(entity)
                .map(|l| l.text.clone())
                .unwrap_or_default();
            paste_field_primary(world, entity);
            let commit = world
                .resource::<Input>()
                .map(|i| {
                    let f = crate::ime::text_frame(i);
                    let mut text = f.chars;
                    text.push_str(&f.commit);
                    text
                })
                .unwrap_or_default();
            if !commit.is_empty() && !crate::interaction::readonly(world, entity) {
                edit_label_queue(world, entity, &[], &commit, true);
            }
            let after = world
                .get::<Label>(entity)
                .map(|l| l.text.clone())
                .unwrap_or_default();
            if before != after {
                send(
                    world,
                    TextChanged {
                        entity,
                        text: after.clone(),
                    },
                );
                transaction_notify(world, entity, FieldTransaction::Changed(after));
            }
            sync_preview(world, entity);
        } else {
            place_from_pointer(world, entity, existed);
            publish_field_primary(world, entity);
            edit_label(world, entity);
        }
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
    if crate::interaction::readonly(world, entity) {
        return None;
    }
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
    // A press can be followed by motion/release before this frame. Focus
    // belongs to its origin, matching the editor's logical drag capture.
    let position = input.pointer.press_at.unwrap_or(input.pointer.position);
    let Some(hit) = hit_test(world, position[0], position[1]) else {
        return;
    };
    let target = ancestor_editor(world, hit).or_else(|| {
        if input.pointer.button != PointerButton::Left {
            return None;
        }
        crate::layout::ancestor_with::<crate::Editor>(world, hit)
            .filter(|entity| world.has::<crate::Buffer>(*entity))
    });
    let Some(target) = target else {
        return;
    };
    // GenericInteraction already focused/placed opted-in fields before keys;
    // refocusing the press origin here would undo a later chronological Tab.
    if world.has::<FieldTransactions>(target) {
        return;
    }
    if let Some(focus) = world.resource_mut::<Focus>() {
        focus.entity = Some(target);
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
        edit_label_queue(world, entity, &keys, &commit, true);
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
    consume_input: bool,
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
    if let Some(input) = world.resource_mut::<Input>().filter(|_| consume_input) {
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

/// Replace a text field's label and end its current typing run.
///
/// The caret moves to the end of `text` when one is already present. The next
/// typed character starts a new history snapshot. Returns false when `entity`
/// is not a live [`TextField`]. Emits no typing events.
pub fn replace_field_text(world: &mut World, entity: Entity, text: &str) -> bool {
    if !world.is_alive(entity) || !world.has::<TextField>(entity) || !world.has::<Label>(entity) {
        return false;
    }
    world.get_mut::<Label>(entity).unwrap().text = text.into();
    let len = text.len();
    if let Some(caret) = world.get_mut::<Caret>(entity) {
        caret.index = len;
        caret.anchor = len;
    }
    world.remove::<EditHistory>(entity);
    world.remove::<ImePreview>(entity);
    true
}

/// Owner-driven draft replacement, not a user edit or property model transaction.
/// Only opted-in TextFields are accepted. Discards local caret/typing history so
/// undo cannot resurrect an old object/property draft. Emits no typing events.
pub fn reset_field_draft(world: &mut World, entity: Entity, text: &str) -> bool {
    if !world.has::<TextField>(entity)
        || !world.has::<FieldTransactions>(entity)
        || !world.has::<Label>(entity)
    {
        return false;
    }
    world.get_mut::<Label>(entity).unwrap().text = text.into();
    world.remove::<Caret>(entity);
    world.remove::<EditHistory>(entity);
    world.remove::<ImePreview>(entity);
    true
}
