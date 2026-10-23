//! Apply [`KeyEvent`]s to a string caret. Left, Right, Backspace, and Delete
//! step by grapheme cluster.

use std::time::Duration;

use crate::edit::{edit_string, floor_boundary, prev_boundary};
use crate::input::Key;
use crate::keys::{standard_action, EditAction, KeyEvent, LogicalKey, NamedKey, Platform};
use crate::world::Component;

impl Component for EditHistory {}

#[derive(Clone, Debug)]
struct Snap {
    text: String,
    index: usize,
    anchor: usize,
}

/// How long a typed run may pause before the next character is a new undo step.
const TYPING_PAUSE: Duration = Duration::from_millis(600);

#[derive(Clone, Debug)]
struct TypingRun {
    end: usize,
    at: Duration,
}

/// Undo and redo snapshots for one text surface. Capped at 100 edits.
///
/// Characters typed in a row share one snapshot. The run ends at whitespace,
/// when the caret moves, and after a pause of 600 milliseconds. A delete,
/// paste, or newline is its own snapshot.
#[derive(Clone, Debug, Default)]
pub struct EditHistory {
    undo: Vec<Snap>,
    redo: Vec<Snap>,
    run: Option<TypingRun>,
}

impl EditHistory {
    fn push_undo(&mut self, snap: Snap) {
        self.undo.push(snap);
        if self.undo.len() > 100 {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    fn can_extend(&self, index: usize, anchor: usize, now: Duration, typed: &str) -> bool {
        if !is_typed_run(typed) {
            return false;
        }
        let Some(run) = &self.run else {
            return false;
        };
        index == anchor && index == run.end && now.saturating_sub(run.at) < TYPING_PAUSE
    }

    fn note_typed(&mut self, end: usize, at: Duration) {
        self.run = Some(TypingRun { end, at });
    }

    /// The next edit starts a new undo step.
    pub(crate) fn break_run(&mut self) {
        self.run = None;
    }
}

fn is_typed_run(text: &str) -> bool {
    let mut any = false;
    for ch in text.chars() {
        if ch.is_whitespace() {
            return false;
        }
        any = true;
    }
    any
}

/// Result of one key. `copied` replaces the clipboard when set.
#[derive(Clone, Debug, Default)]
pub(crate) struct StepResult {
    pub copied: Option<String>,
    pub clipboard_action: bool,
}

/// Keys the caret steals before they edit the buffer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Steer {
    Edit,
    Tab,
    Enter,
    Up,
    Down,
}

/// Plain Tab, Enter, and arrows. A standard chord such as Cmd+Up stays [`Steer::Edit`].
pub(crate) fn steer(event: &KeyEvent, platform: Platform) -> Steer {
    if standard_action(platform, event).is_some() {
        return Steer::Edit;
    }
    if event.modifiers.ctrl || event.modifiers.super_key || event.modifiers.alt {
        return Steer::Edit;
    }
    match event.logical {
        LogicalKey::Named(NamedKey::Tab) => Steer::Tab,
        LogicalKey::Named(NamedKey::Enter) => Steer::Enter,
        LogicalKey::Named(NamedKey::ArrowUp) => Steer::Up,
        LogicalKey::Named(NamedKey::ArrowDown) => Steer::Down,
        _ => Steer::Edit,
    }
}

/// Document text and the caret, borrowed together so one edit stays in sync.
pub(crate) struct EditSurface<'a> {
    pub text: &'a mut String,
    pub index: &'a mut usize,
    pub anchor: &'a mut usize,
}

/// [`crate::Time::now`] for this frame, or zero when the resource is absent.
pub(crate) fn frame_now(world: &crate::world::World) -> Duration {
    world
        .resource::<crate::clock::Time>()
        .map(|time| time.now)
        .unwrap_or_default()
}

pub(crate) fn apply_text_event(
    surface: &mut EditSurface<'_>,
    event: &KeyEvent,
    platform: Platform,
    newline: bool,
    now: Duration,
    history: &mut EditHistory,
    clip: &str,
) -> StepResult {
    // A named Space with no text inserts a character, so a focus-in synthetic
    // has to be rejected here and not only by dropping its reported text.
    if event.released || event.synthetic {
        return StepResult::default();
    }
    let text = &mut *surface.text;
    let index = &mut *surface.index;
    let anchor = &mut *surface.anchor;
    let shift = event.modifiers.shift;
    if let Some(action) = standard_action(platform, event) {
        return apply_action(text, index, anchor, action, shift, now, history, clip);
    }
    match event.logical {
        LogicalKey::Named(NamedKey::ArrowLeft) => {
            history.break_run();
            step_key(text, index, anchor, Some(Key::Left), shift, "", false);
        }
        LogicalKey::Named(NamedKey::ArrowRight) => {
            history.break_run();
            step_key(text, index, anchor, Some(Key::Right), shift, "", false);
        }
        LogicalKey::Named(NamedKey::Backspace) => {
            change(
                text,
                index,
                anchor,
                history,
                now,
                None,
                |text, index, anchor| {
                    step_key(text, index, anchor, Some(Key::Backspace), false, "", false);
                },
            );
        }
        LogicalKey::Named(NamedKey::Delete) => {
            change(
                text,
                index,
                anchor,
                history,
                now,
                None,
                |text, index, anchor| {
                    delete_forward(text, index, anchor);
                },
            );
        }
        LogicalKey::Named(NamedKey::Enter) if newline && !event.modifiers.command() => {
            change(
                text,
                index,
                anchor,
                history,
                now,
                None,
                |text, index, anchor| {
                    step_key(text, index, anchor, Some(Key::Enter), false, "", true);
                },
            );
        }
        LogicalKey::Named(NamedKey::Home) => {
            history.break_run();
            let dest = line_start(text, *index);
            place(index, anchor, dest, shift);
        }
        LogicalKey::Named(NamedKey::End) => {
            history.break_run();
            let dest = line_end(text, *index);
            place(index, anchor, dest, shift);
        }
        LogicalKey::Named(NamedKey::Space)
            if event.text.is_none()
                && !event.modifiers.ctrl
                && !event.modifiers.alt
                && !event.modifiers.super_key =>
        {
            change(
                text,
                index,
                anchor,
                history,
                now,
                Some(" "),
                |text, index, anchor| {
                    step_key(text, index, anchor, None, false, " ", false);
                },
            );
        }
        _ => {
            if let Some(typed) = event.insertable_text() {
                let typed_for_run = typed.clone();
                change(
                    text,
                    index,
                    anchor,
                    history,
                    now,
                    Some(&typed_for_run),
                    |text, index, anchor| {
                        step_key(text, index, anchor, None, false, &typed, false);
                    },
                );
            }
        }
    }
    StepResult::default()
}

// Motions, undo, and the clipboard share the buffer, caret, clock, and history.
#[allow(clippy::too_many_arguments)]
fn apply_action(
    text: &mut String,
    index: &mut usize,
    anchor: &mut usize,
    action: EditAction,
    shift: bool,
    now: Duration,
    history: &mut EditHistory,
    clip: &str,
) -> StepResult {
    match action {
        EditAction::SelectAll => {
            history.break_run();
            *anchor = 0;
            *index = text.len();
            StepResult::default()
        }
        EditAction::Undo => {
            restore(text, index, anchor, history, true);
            StepResult::default()
        }
        EditAction::Redo => {
            restore(text, index, anchor, history, false);
            StepResult::default()
        }
        EditAction::Copy => StepResult {
            copied: Some(selected(text, *index, *anchor)),
            clipboard_action: true,
        },
        EditAction::Cut => {
            let copied = selected(text, *index, *anchor);
            change(
                text,
                index,
                anchor,
                history,
                now,
                None,
                |text, index, anchor| {
                    let (start, end) = span(text, *index, *anchor);
                    text.replace_range(start..end, "");
                    *index = start;
                    *anchor = start;
                },
            );
            StepResult {
                copied: Some(copied),
                clipboard_action: true,
            }
        }
        EditAction::Paste => {
            let clip = clip.to_string();
            change(
                text,
                index,
                anchor,
                history,
                now,
                None,
                |text, index, anchor| {
                    let (start, end) = span(text, *index, *anchor);
                    text.replace_range(start..end, &clip);
                    let next = start + clip.len();
                    *index = next;
                    *anchor = next;
                },
            );
            StepResult {
                copied: None,
                clipboard_action: true,
            }
        }
        EditAction::WordLeft => {
            history.break_run();
            let dest = word_left(text, *index);
            place(index, anchor, dest, shift);
            StepResult::default()
        }
        EditAction::WordRight => {
            history.break_run();
            let dest = word_right(text, *index);
            place(index, anchor, dest, shift);
            StepResult::default()
        }
        EditAction::LineStart => {
            history.break_run();
            let dest = line_start(text, *index);
            place(index, anchor, dest, shift);
            StepResult::default()
        }
        EditAction::LineEnd => {
            history.break_run();
            let dest = line_end(text, *index);
            place(index, anchor, dest, shift);
            StepResult::default()
        }
        EditAction::DocStart => {
            history.break_run();
            place(index, anchor, 0, shift)
        }
        EditAction::DocEnd => {
            history.break_run();
            place(index, anchor, text.len(), shift)
        }
        EditAction::DeleteWordBackward => {
            change(
                text,
                index,
                anchor,
                history,
                now,
                None,
                |text, index, anchor| {
                    if *index != *anchor {
                        step_key(text, index, anchor, Some(Key::Backspace), false, "", false);
                        return;
                    }
                    let start = word_left(text, *index);
                    if start < *index {
                        text.replace_range(start..*index, "");
                        *index = start;
                        *anchor = start;
                    }
                },
            );
            StepResult::default()
        }
    }
}

fn restore(
    text: &mut String,
    index: &mut usize,
    anchor: &mut usize,
    history: &mut EditHistory,
    undo: bool,
) {
    let prev = if undo {
        history.undo.pop()
    } else {
        history.redo.pop()
    };
    let Some(prev) = prev else {
        return;
    };
    let current = Snap {
        text: text.clone(),
        index: *index,
        anchor: *anchor,
    };
    if undo {
        history.redo.push(current);
    } else {
        history.undo.push(current);
    }
    history.run = None;
    *text = prev.text;
    *index = prev.index.min(text.len());
    *anchor = prev.anchor.min(text.len());
}

/// Replace the whole buffer and record one undo step when the text changes.
/// The caret moves to the end.
pub(crate) fn rewrite(surface: &mut EditSurface<'_>, history: &mut EditHistory, next: &str) {
    let next = next.to_string();
    let text = &mut *surface.text;
    let index = &mut *surface.index;
    let anchor = &mut *surface.anchor;
    change(
        text,
        index,
        anchor,
        history,
        Duration::ZERO,
        None,
        |text, index, anchor| {
            text.clear();
            text.push_str(&next);
            *index = text.len();
            *anchor = *index;
        },
    );
}

/// Insert `text` at the caret, replacing a selection, as its own undo step.
pub(crate) fn insert_plain(surface: &mut EditSurface<'_>, history: &mut EditHistory, text: &str) {
    let pasted = text.to_string();
    *surface.anchor = *surface.index;
    change(
        surface.text,
        surface.index,
        surface.anchor,
        history,
        Duration::ZERO,
        None,
        |text, index, anchor| {
            step_key(text, index, anchor, None, false, &pasted, false);
        },
    );
}

fn change(
    text: &mut String,
    index: &mut usize,
    anchor: &mut usize,
    history: &mut EditHistory,
    now: Duration,
    typed: Option<&str>,
    mutate: impl FnOnce(&mut String, &mut usize, &mut usize),
) {
    let extend = typed.is_some_and(|typed| history.can_extend(*index, *anchor, now, typed));
    let before = Snap {
        text: text.clone(),
        index: *index,
        anchor: *anchor,
    };
    mutate(text, index, anchor);
    if *text == before.text {
        return;
    }
    if extend {
        history.note_typed(*index, now);
        return;
    }
    history.push_undo(before);
    if typed.is_some_and(is_typed_run) && *index == *anchor {
        history.note_typed(*index, now);
    } else {
        history.run = None;
    }
}

fn step_key(
    text: &mut String,
    index: &mut usize,
    anchor: &mut usize,
    key: Option<Key>,
    shift: bool,
    chars: &str,
    newline: bool,
) {
    let (next, next_anchor) = edit_string(text, *index, *anchor, key, shift, chars, newline);
    *index = next;
    *anchor = next_anchor;
}

fn place(index: &mut usize, anchor: &mut usize, dest: usize, shift: bool) -> StepResult {
    *index = dest;
    if !shift {
        *anchor = dest;
    }
    StepResult::default()
}

fn selected(text: &str, index: usize, anchor: usize) -> String {
    let (start, end) = span(text, index, anchor);
    text[start..end].to_string()
}

fn span(text: &str, index: usize, anchor: usize) -> (usize, usize) {
    crate::edit::grapheme_selection(text, anchor, index)
}

fn delete_forward(text: &mut String, index: &mut usize, anchor: &mut usize) {
    if *index != *anchor {
        let (start, end) = span(text, *index, *anchor);
        text.replace_range(start..end, "");
        *index = start;
        *anchor = start;
        return;
    }
    let start = crate::edit::floor_grapheme(text, *index);
    let end = crate::edit::next_grapheme(text, start);
    if start < end {
        text.replace_range(start..end, "");
        *index = start;
        *anchor = start;
    }
}

fn line_start(text: &str, index: usize) -> usize {
    let index = index.min(text.len());
    text[..index].rfind('\n').map(|at| at + 1).unwrap_or(0)
}

fn line_end(text: &str, index: usize) -> usize {
    let index = index.min(text.len());
    text[index..]
        .find('\n')
        .map(|at| index + at)
        .unwrap_or(text.len())
}

fn is_word_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

fn prev_char(text: &str, index: usize) -> Option<(usize, char)> {
    let len = text.len();
    let byte = |i: usize| text.as_bytes()[i];
    let index = floor_boundary(len, index, &byte);
    if index == 0 {
        return None;
    }
    let start = prev_boundary(len, index, &byte);
    let ch = text[start..index].chars().next()?;
    Some((start, ch))
}

fn char_at(text: &str, index: usize) -> Option<(usize, char)> {
    let len = text.len();
    let byte = |i: usize| text.as_bytes()[i];
    let index = floor_boundary(len, index, &byte);
    if index >= len {
        return None;
    }
    let ch = text[index..].chars().next()?;
    Some((index + ch.len_utf8(), ch))
}

/// Start of the word before `index`. Whitespace is skipped, then one run of
/// word characters or punctuation.
pub(crate) fn word_left(text: &str, index: usize) -> usize {
    let mut index = floor_boundary(text.len(), index, &|i| text.as_bytes()[i]);
    while let Some((start, ch)) = prev_char(text, index) {
        if ch.is_whitespace() {
            index = start;
        } else {
            break;
        }
    }
    let word = prev_char(text, index).is_some_and(|(_, ch)| is_word_char(ch));
    while let Some((start, ch)) = prev_char(text, index) {
        if ch.is_whitespace() || is_word_char(ch) != word {
            break;
        }
        index = start;
    }
    index
}

/// Start of the next word after `index`.
pub(crate) fn word_right(text: &str, index: usize) -> usize {
    let mut index = floor_boundary(text.len(), index, &|i| text.as_bytes()[i]);
    if let Some((_, ch)) = char_at(text, index) {
        if !ch.is_whitespace() {
            let word = is_word_char(ch);
            while let Some((end, ch)) = char_at(text, index) {
                if ch.is_whitespace() || is_word_char(ch) != word {
                    break;
                }
                index = end;
            }
        }
    }
    while let Some((end, ch)) = char_at(text, index) {
        if !ch.is_whitespace() {
            break;
        }
        index = end;
    }
    index
}
