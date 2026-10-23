//! Multi-line prompt. Plain Enter submits. Shift+Enter inserts a newline.
//!
//! The widget edits the entity's [`Label`] with the same caret, undo stack,
//! clipboard, and IME preview as a [`crate::TextField`]. Soft wrap follows the
//! row width. The row grows with the wrapped text until [`Prompt::max_lines`],
//! then the text scrolls so the caret stays visible.
//!
//! A keymap binding for the same key is taken before the prompt sees it. Drop
//! list navigation while the prompt is the editor if Up, Down, Home, End, and
//! Enter should edit instead of moving a list.
//!
//! On X11 and Wayland, releasing a mouse selection writes the primary
//! selection, and so does a keyboard selection.

use crate::caret::Caret;
use crate::clipboard::{note_clipboard, note_primary, primary_selection, Clipboard};
use crate::entity::Entity;
use crate::event::send;
use crate::field::{self, ImePreview, Submitted, TextChanged};
use crate::font::{field_text_width, line_col, TextLayout};
use crate::input::{Focus, Input, Key, PointerButton};
use crate::keys::{platform_of, KeyEvent, LogicalKey, Modifiers, NamedKey, Platform};
use crate::layout::{hit_test, Rect};
use crate::style::{Color, Label, TEXT_ORIGIN_X, TEXT_ORIGIN_Y};
use crate::text::{style_of, text_engine};
use crate::theme::theme_of;
use crate::typing::{self, steer, EditHistory, EditSurface, Steer};
use crate::world::{Component, World};

impl Component for Prompt {}
impl Component for Placeholder {}
impl Component for PromptHistory {}

/// Multi-line editor on this entity's [`Label`].
///
/// Plain Enter sends [`Submitted`] when [`Self::submit_on_enter`] is set, and
/// does not insert a newline. Shift+Enter always inserts a newline. Up and
/// Down move one visual line. On the first line, plain Up recalls the previous
/// submission. On the last line, plain Down walks back toward the draft.
/// Home and End move to the hard-line edges. Word and line chords follow the
/// platform: Option or Ctrl for a word, Command or Home/End for a line.
///
/// The label's overflow is set to [`crate::TextOverflow::Wrap`] while this component
/// is present. Copy, cut, and paste use the in-memory [`Clipboard`] the desktop
/// runner mirrors. On X11 and Wayland, releasing a mouse selection also writes
/// the primary selection, and middle-click pastes it. Undo and redo use the
/// same stack as a text field: a typed run is one step.
///
/// # Example
///
/// ```
/// use rndr_core::{App, Chrome, Focus, Label, Layout, Prompt, Size};
///
/// let mut app = App::new();
/// app.add_plugin(Chrome);
/// let prompt = app.world_mut().spawn((
///     Layout {
///         width: Size::Px(200.0),
///         ..Layout::column()
///     },
///     Label::new(""),
///     Prompt::new(),
/// ));
/// app.world_mut().insert_resource(Focus {
///     entity: Some(prompt),
/// });
/// app.update();
/// assert_eq!(app.world().get::<Prompt>(prompt).unwrap().max_lines, 8);
/// ```
#[derive(Clone, Debug)]
pub struct Prompt {
    /// Plain Enter submits instead of inserting a newline.
    pub submit_on_enter: bool,
    /// Wrapped rows kept on screen. Further rows scroll. At least 1.
    pub max_lines: u32,
    /// How far the text has moved up, in points. Zero shows the first line.
    /// The layout pass writes this so the caret stays inside the row.
    pub scroll: f32,
}

impl Default for Prompt {
    fn default() -> Self {
        Self {
            submit_on_enter: true,
            max_lines: 8,
            scroll: 0.0,
        }
    }
}

impl Prompt {
    /// Submit on plain Enter, grow up to 8 lines, scroll at the top.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set [`Self::submit_on_enter`].
    pub fn submit_on_enter(mut self, submit: bool) -> Self {
        self.submit_on_enter = submit;
        self
    }

    /// Set [`Self::max_lines`]. Zero is kept as one line.
    pub fn max_lines(mut self, lines: u32) -> Self {
        self.max_lines = lines.max(1);
        self
    }
}

/// Hint drawn while the label is empty and no IME preedit is active.
///
/// The string is not part of the label, not submitted, and not copied.
#[derive(Clone, Debug, Default)]
pub struct Placeholder {
    /// Hint text. An empty string draws nothing.
    pub text: String,
}

impl Placeholder {
    /// Store `text` as the hint.
    pub fn new(text: impl Into<String>) -> Self {
        Self { text: text.into() }
    }
}

/// Submissions a [`Prompt`] can recall, oldest first.
///
/// The prompt records a non-empty submission when plain Enter submits. Plain
/// Up on the first visual line walks toward older entries. Plain Down on the
/// last visual line walks back. Typing, deleting, or a newline leaves the
/// recalled entry and keeps the buffer as the new draft.
#[derive(Clone, Debug, Default)]
pub struct PromptHistory {
    entries: Vec<String>,
    /// Index into [`Self::entries`] while browsing. `None` is the live draft.
    cursor: Option<usize>,
    draft: String,
}

impl PromptHistory {
    /// Recorded submissions, oldest first.
    pub fn entries(&self) -> &[String] {
        &self.entries
    }

    /// Append `text` and stop browsing. Empty and whitespace-only strings are
    /// dropped. A repeat of the newest entry is dropped. The list keeps 100.
    pub fn push(&mut self, text: impl Into<String>) {
        let text = text.into();
        self.cursor = None;
        self.draft.clear();
        if text.trim().is_empty() {
            return;
        }
        if self.entries.last().is_some_and(|last| last == &text) {
            return;
        }
        self.entries.push(text);
        if self.entries.len() > 100 {
            self.entries.remove(0);
        }
    }

    fn abandon(&mut self) {
        self.cursor = None;
        self.draft.clear();
    }

    fn older(&mut self, current: &str) -> Option<String> {
        if self.entries.is_empty() {
            return None;
        }
        match self.cursor {
            None => {
                self.draft = current.to_string();
                let index = self.entries.len() - 1;
                self.cursor = Some(index);
                Some(self.entries[index].clone())
            }
            Some(0) => None,
            Some(index) => {
                let index = index - 1;
                self.cursor = Some(index);
                Some(self.entries[index].clone())
            }
        }
    }

    fn newer(&mut self) -> Option<String> {
        let index = self.cursor?;
        if index + 1 >= self.entries.len() {
            self.cursor = None;
            Some(std::mem::take(&mut self.draft))
        } else {
            let index = index + 1;
            self.cursor = Some(index);
            Some(self.entries[index].clone())
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct PromptDrag {
    entity: Option<Entity>,
}

/// Hint to draw, when the label is empty and composition is idle.
pub(crate) fn placeholder_text(world: &World, entity: Entity) -> Option<String> {
    let label = world.get::<Label>(entity)?;
    if !label.text.is_empty() {
        return None;
    }
    if world
        .get::<ImePreview>(entity)
        .is_some_and(|preview| !preview.text.is_empty())
    {
        return None;
    }
    let text = world.get::<Placeholder>(entity)?.text.clone();
    (!text.is_empty()).then_some(text)
}

/// Dim color for a placeholder run. The hint is not the label color at full strength.
pub(crate) fn placeholder_color(world: &World) -> Color {
    let color = theme_of(world).text;
    Color::rgba(color.r, color.g, color.b, (color.a * 0.45).clamp(0.0, 1.0))
}

/// Cap used by hug measurement. At least one line.
pub(crate) fn line_cap(prompt: &Prompt) -> usize {
    prompt.max_lines.max(1) as usize
}

pub(crate) fn edit_focused(world: &mut World, entity: Entity) {
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
    clamp_caret(world, entity);
    place_pointer(world, entity, existed);
    edit_text(world, entity);
    field::sync_preview(world, entity);
}

fn clamp_caret(world: &mut World, entity: Entity) {
    let len = world
        .get::<Label>(entity)
        .map(|label| label.text.len())
        .unwrap_or(0);
    if let Some(caret) = world.get_mut::<Caret>(entity) {
        caret.index = caret.index.min(len);
        caret.anchor = caret.anchor.min(len);
    }
}

fn place_pointer(world: &mut World, entity: Entity, existed: bool) {
    let Some(input) = world.resource::<Input>().cloned() else {
        return;
    };
    let hit = hit_test(world, input.pointer.position[0], input.pointer.position[1])
        .and_then(|hit| field::ancestor_editor(world, hit));
    if input.pointer.button == PointerButton::Middle && input.pointer.pressed {
        if hit == Some(entity) {
            let index = index_at_pointer(world, entity, input.pointer.position);
            if let Some(caret) = world.get_mut::<Caret>(entity) {
                caret.index = index;
                caret.anchor = index;
            }
            field::break_typing_at(world, entity);
        }
        return;
    }
    if input.pointer.pressed && input.pointer.button == PointerButton::Left {
        if hit != Some(entity) {
            clear_drag(world, entity);
        } else {
            world.insert_resource(PromptDrag {
                entity: Some(entity),
            });
            let index = index_at_pointer(world, entity, input.pointer.position);
            if let Some(caret) = world.get_mut::<Caret>(entity) {
                caret.index = index;
                if !(input.shift && existed) {
                    caret.anchor = index;
                }
            }
            field::break_typing_at(world, entity);
        }
    } else if input.pointer.down && input.pointer.button == PointerButton::Left {
        let dragging = world
            .resource::<PromptDrag>()
            .is_some_and(|drag| drag.entity == Some(entity));
        if dragging {
            let index = index_at_pointer(world, entity, input.pointer.position);
            if let Some(caret) = world.get_mut::<Caret>(entity) {
                caret.index = index;
            }
            field::break_typing_at(world, entity);
        }
    }
    if input.pointer.released && input.pointer.button == PointerButton::Left {
        let dragging = world
            .resource::<PromptDrag>()
            .is_some_and(|drag| drag.entity == Some(entity));
        if (dragging || hit == Some(entity)) && primary_selection(platform_of(world)) {
            let selected = field::label_selection(world, entity);
            note_primary(world, &selected);
        }
        clear_drag(world, entity);
    } else if !input.pointer.down {
        clear_drag(world, entity);
    }
}

fn clear_drag(world: &mut World, entity: Entity) {
    if let Some(drag) = world.resource_mut::<PromptDrag>() {
        if drag.entity == Some(entity) {
            drag.entity = None;
        }
    }
}

fn index_at_pointer(world: &World, entity: Entity, position: [f32; 2]) -> usize {
    let text = world
        .get::<Label>(entity)
        .map(|label| label.text.clone())
        .unwrap_or_default();
    if text.is_empty() {
        return 0;
    }
    let Some(rect) = world.get::<Rect>(entity).copied() else {
        return 0;
    };
    let scroll = world
        .get::<Prompt>(entity)
        .map(|prompt| prompt.scroll)
        .unwrap_or(0.0);
    let layout = layout_for(world, entity, &text);
    layout.index_at(
        position[0] - rect.x - TEXT_ORIGIN_X,
        position[1] - rect.y - TEXT_ORIGIN_Y + scroll,
    )
}

fn edit_text(world: &mut World, entity: Entity) {
    let frame = match world.resource::<Input>() {
        Some(input) => crate::ime::text_frame(input),
        None => return,
    };
    let key = frame.legacy_key;
    let shift = frame.shift;
    let chars = frame.chars;
    let commit = frame.commit;
    let preedit = frame.preedit;
    let keys = frame.keys;
    let submit_on_enter = world
        .get::<Prompt>(entity)
        .is_none_or(|prompt| prompt.submit_on_enter);
    let platform = platform_of(world);
    let composing = !preedit.is_empty() || !commit.is_empty();
    let events = if keys.is_empty() {
        legacy_events(key, shift, &chars)
    } else {
        keys
    };
    let before = world
        .get::<Label>(entity)
        .map(|label| label.text.clone())
        .unwrap_or_default();
    let mut text = before.clone();
    let (mut index, mut anchor) = world
        .get::<Caret>(entity)
        .map(|caret| (caret.index.min(text.len()), caret.anchor.min(text.len())))
        .unwrap_or((text.len(), text.len()));
    let (before_index, before_anchor) = (index, anchor);
    let mut history = world.remove::<EditHistory>(entity).unwrap_or_default();
    let mut recalls = world.remove::<PromptHistory>(entity).unwrap_or_default();
    if let Some(paste) = field::middle_paste_text(world, entity) {
        anchor = index;
        typing::insert_plain(
            &mut EditSurface {
                text: &mut text,
                index: &mut index,
                anchor: &mut anchor,
            },
            &mut history,
            &paste,
        );
    }
    let mut clip = world
        .resource::<Clipboard>()
        .map(|clipboard| clipboard.text.clone())
        .unwrap_or_default();
    let mut copied = None;
    let mut clipboard_action = false;
    let mut submitted = false;
    let mut break_recall = false;
    let now = typing::frame_now(world);
    for event in &events {
        if event.released || event.synthetic {
            continue;
        }
        let before_step = text.clone();
        let mut from_recall = false;
        match steer(event, platform) {
            Steer::Enter if !composing => {
                let chord =
                    event.modifiers.ctrl || event.modifiers.alt || event.modifiers.super_key;
                if chord {
                    continue;
                }
                if event.modifiers.shift || !submit_on_enter {
                    let step = apply(
                        &mut text,
                        &mut index,
                        &mut anchor,
                        event,
                        platform,
                        true,
                        now,
                        &mut history,
                        &clip,
                    );
                    copied = step.copied.or(copied);
                    clipboard_action |= step.clipboard_action;
                } else if !event.repeat {
                    submitted = true;
                }
            }
            Steer::Up | Steer::Down => {
                history.break_run();
                let delta = if matches!(event.logical, LogicalKey::Named(NamedKey::ArrowUp)) {
                    -1
                } else {
                    1
                };
                let (line, count) = visual_line(world, entity, &text, index);
                let edge = (delta < 0 && line == 0) || (delta > 0 && line + 1 >= count);
                if !event.modifiers.shift && index == anchor && edge {
                    let next = if delta < 0 {
                        recalls.older(&text)
                    } else {
                        recalls.newer()
                    };
                    if let Some(next) = next {
                        typing::rewrite(
                            &mut EditSurface {
                                text: &mut text,
                                index: &mut index,
                                anchor: &mut anchor,
                            },
                            &mut history,
                            &next,
                        );
                        from_recall = true;
                    }
                } else {
                    let next = move_visual(world, entity, &text, index, delta);
                    if event.modifiers.shift {
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
                }
            }
            _ => {
                let step = apply(
                    &mut text,
                    &mut index,
                    &mut anchor,
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
        }
        if from_recall {
            break_recall = false;
        } else if text != before_step {
            break_recall = true;
        }
    }
    if !commit.is_empty() {
        let before_commit = text.clone();
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
        let _ = apply(
            &mut text,
            &mut index,
            &mut anchor,
            &commit_event,
            platform,
            false,
            now,
            &mut history,
            &clip,
        );
        if text != before_commit {
            break_recall = true;
        }
    }
    if break_recall {
        recalls.abandon();
    }
    if submitted {
        recalls.push(text.clone());
    }
    if let Some(label) = world.get_mut::<Label>(entity) {
        label.text = text.clone();
    }
    if let Some(caret) = world.get_mut::<Caret>(entity) {
        caret.index = index;
        caret.anchor = anchor;
    }
    field::publish_keyboard_primary(world, entity, before_index, before_anchor);
    world.insert(entity, history);
    world.insert(entity, recalls);
    if clipboard_action {
        note_clipboard(world, copied.as_deref());
    }
    if before != text {
        send(
            world,
            TextChanged {
                entity,
                text: text.clone(),
            },
        );
    }
    if submitted {
        send(world, Submitted { entity, text });
    }
    if let Some(input) = world.resource_mut::<Input>() {
        input.keys.clear();
        input.chars.clear();
        input.ime_commit.clear();
        input.key = None;
    }
}

fn legacy_events(key: Option<Key>, shift: bool, chars: &str) -> Vec<KeyEvent> {
    let mut events = Vec::new();
    let mut modifiers = Modifiers::new();
    modifiers.shift = shift;
    if let Some(named) = key.and_then(named_of) {
        events.push(KeyEvent::named(named, modifiers));
    }
    if !chars.is_empty() {
        events.push(KeyEvent {
            logical: LogicalKey::Unidentified,
            text: Some(chars.to_string()),
            code: None,
            modifiers: Modifiers::new(),
            repeat: false,
            released: false,
            synthetic: false,
            order: KeyEvent::UNORDERED,
        });
    }
    events
}

fn named_of(key: Key) -> Option<NamedKey> {
    Some(match key {
        Key::Up => NamedKey::ArrowUp,
        Key::Down => NamedKey::ArrowDown,
        Key::Left => NamedKey::ArrowLeft,
        Key::Right => NamedKey::ArrowRight,
        Key::Enter => NamedKey::Enter,
        Key::Backspace => NamedKey::Backspace,
        Key::Delete => NamedKey::Delete,
        Key::Tab => NamedKey::Tab,
        _ => return None,
    })
}

// One edit step needs the buffer, caret, event, platform, newline flag, undo stack, and clipboard.
#[allow(clippy::too_many_arguments)]
fn apply(
    text: &mut String,
    index: &mut usize,
    anchor: &mut usize,
    event: &KeyEvent,
    platform: Platform,
    newline: bool,
    now: std::time::Duration,
    history: &mut EditHistory,
    clip: &str,
) -> typing::StepResult {
    typing::apply_text_event(
        &mut EditSurface {
            text,
            index,
            anchor,
        },
        event,
        platform,
        newline,
        now,
        history,
        clip,
    )
}

fn layout_for(world: &World, entity: Entity, text: &str) -> TextLayout {
    let style = style_of(world);
    let engine = text_engine(world);
    let rect_w = world.get::<Rect>(entity).map(|rect| rect.w).unwrap_or(0.0);
    let max_width = if rect_w > 1.0 {
        field_text_width(rect_w, engine.unit_advance(&style))
    } else {
        f32::INFINITY
    };
    let spans = world
        .get::<crate::font::Spans>(entity)
        .map(|spans| spans.runs.clone())
        .unwrap_or_default();
    engine.layout(text, &style, &spans, max_width)
}

fn visual_line(world: &World, entity: Entity, text: &str, index: usize) -> (usize, usize) {
    let layout = layout_for(world, entity, text);
    if layout.lines.is_empty() {
        return (0, 1);
    }
    let ranges: Vec<(usize, usize)> = layout
        .lines
        .iter()
        .map(|line| (line.start, line.end))
        .collect();
    let (line, _) = line_col(&ranges, index);
    (line, layout.lines.len())
}

fn move_visual(world: &World, entity: Entity, text: &str, index: usize, delta: isize) -> usize {
    let layout = layout_for(world, entity, text);
    if layout.lines.is_empty() {
        return index;
    }
    let ranges: Vec<(usize, usize)> = layout
        .lines
        .iter()
        .map(|line| (line.start, line.end))
        .collect();
    let (line, _) = line_col(&ranges, index);
    let (x, _) = layout.caret_xy(index);
    let last = layout.lines.len().saturating_sub(1) as isize;
    let next = (line as isize + delta).clamp(0, last) as usize;
    layout.index_on_line(next, x)
}

/// Keep each focused prompt's caret inside the visible rows.
pub(crate) fn reveal_prompts(world: &mut World) {
    let focused = world.resource::<Focus>().and_then(|focus| focus.entity);
    for entity in world.entities_with::<Prompt>() {
        let Some(rect) = world.get::<Rect>(entity).copied() else {
            continue;
        };
        if rect.h <= 0.0 {
            continue;
        }
        let Some(layout) = world.get::<TextLayout>(entity).cloned() else {
            continue;
        };
        let line_h = layout
            .lines
            .first()
            .map(|line| line.h)
            .filter(|h| *h > 0.0)
            .unwrap_or(16.0);
        let view = (rect.h - 2.0 * TEXT_ORIGIN_Y).max(line_h);
        let content = layout.height.max(line_h);
        let max_scroll = (content - view).max(0.0);
        let mut scroll = world
            .get::<Prompt>(entity)
            .map(|prompt| prompt.scroll)
            .unwrap_or(0.0);
        if focused == Some(entity) {
            if let Some(caret) = world.get::<Caret>(entity).copied() {
                let shaped = layout
                    .ime_cursor
                    .unwrap_or(caret.index)
                    .min(layout_end(&layout));
                let y = layout.caret_xy(shaped).1;
                if y < scroll {
                    scroll = y;
                } else if y + line_h > scroll + view {
                    scroll = y + line_h - view;
                }
            }
        }
        scroll = scroll.clamp(0.0, max_scroll);
        if let Some(prompt) = world.get_mut::<Prompt>(entity) {
            prompt.scroll = scroll;
        }
    }
}

fn layout_end(layout: &TextLayout) -> usize {
    layout.lines.last().map(|line| line.end).unwrap_or(0)
}
