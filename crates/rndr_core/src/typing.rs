//! Apply [`KeyEvent`]s to a string caret. Left, Right, Backspace, and Delete
//! step by grapheme cluster.

use std::time::Duration;

use crate::edit::{edit_string, floor_boundary, prev_boundary};
use crate::editing::SourceChange;
use crate::input::Key;
use crate::keys::{standard_action, EditAction, KeyEvent, LogicalKey, NamedKey, Platform};
use crate::world::Component;

impl Component for EditHistory {}

#[derive(Clone, Debug)]
struct Snap {
    /// Full preimage for steps that have no splice (legacy whole-buffer edits).
    /// Delta steps leave this empty and retain only `deleted`.
    text: Option<String>,
    index: usize,
    anchor: usize,
    // Forward source operations from this snapshot to the state they produce.
    source_changes: Option<Vec<crate::editing::SourceChange>>,
    /// Bytes each forward range replaced. Parallel to `source_changes`.
    deleted: Option<Vec<String>>,
    selections: Option<crate::source_selection::SourceSelections>,
}

impl Snap {
    fn retained_bytes(&self) -> usize {
        self.text.as_ref().map_or(0, String::capacity)
            + std::mem::size_of::<Self>()
            + self.selections.as_ref().map_or(0, |s| s.retained_bytes())
            + self.source_changes.as_ref().map_or(0, |changes| {
                changes.capacity() * std::mem::size_of::<crate::editing::SourceChange>()
                    + changes
                        .iter()
                        .map(|c| c.replacement.capacity())
                        .sum::<usize>()
            })
            + self.deleted.as_ref().map_or(0, |deleted| {
                deleted.capacity() * std::mem::size_of::<String>()
                    + deleted.iter().map(String::capacity).sum::<usize>()
            })
    }
}

/// Invert forward splices using the bytes they replaced. The second vector is
/// what a later redo of this inverse must remove (the original replacements).
pub(crate) fn invert_changes(
    changes: &[crate::editing::SourceChange],
    deleted: &[String],
) -> (Vec<crate::editing::SourceChange>, Vec<String>) {
    let mut inverse: Vec<crate::editing::SourceChange> = Vec::new();
    let mut inverse_deleted: Vec<String> = Vec::new();
    let mut removed = 0usize;
    let mut inserted = 0usize;
    for (change, gone) in changes.iter().zip(deleted) {
        let start = change.range.start - removed + inserted;
        let next_end = start + change.replacement.len();
        if let Some(last) = inverse.last_mut().filter(|last| last.range.end == start) {
            last.range.end = next_end;
            last.replacement.push_str(gone);
            inverse_deleted
                .last_mut()
                .unwrap()
                .push_str(&change.replacement);
        } else {
            inverse.push(crate::editing::SourceChange {
                range: start..next_end,
                replacement: gone.clone(),
            });
            inverse_deleted.push(change.replacement.clone());
        }
        removed += change.range.len();
        inserted += change.replacement.len();
    }
    (inverse, inverse_deleted)
}

fn apply_splices(text: &mut String, changes: &[crate::editing::SourceChange]) {
    for change in changes.iter().rev() {
        text.replace_range(change.range.clone(), &change.replacement);
    }
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
    // Transient exact operation for the current native event, consumed by a session.
    native_change: Option<SourceChange>,
    native_deleted: Option<String>,
    pub(crate) source_selections: Option<crate::source_selection::SourceSelections>,
}

impl EditHistory {
    pub(crate) fn retained_bytes(&self) -> usize {
        self.undo
            .iter()
            .chain(&self.redo)
            .map(Snap::retained_bytes)
            .sum()
    }

    /// Enforce the managed-session snapshot budget, evicting oldest steps.
    pub(crate) fn trim_bytes(&mut self, limit: usize) {
        let mut bytes: usize = self
            .undo
            .iter()
            .chain(&self.redo)
            .map(Snap::retained_bytes)
            .sum();
        while bytes > limit {
            let snap = if !self.undo.is_empty() {
                self.undo.remove(0)
            } else if !self.redo.is_empty() {
                self.redo.remove(0)
            } else {
                break;
            };
            bytes = bytes.saturating_sub(snap.retained_bytes());
            self.break_run();
        }
    }

    /// Record a validated external source edit in the keyboard history.
    pub(crate) fn record_source_edit(&mut self, text: String, index: usize, anchor: usize) {
        self.push_undo(Snap {
            text: Some(text),
            index,
            anchor,
            source_changes: None,
            deleted: None,
            selections: None,
        });
        self.break_run();
    }

    /// Record forward splices. `text` is the preimage; only the replaced slices
    /// are retained, not the document.
    pub(crate) fn record_source_changes(
        &mut self,
        text: &str,
        index: usize,
        anchor: usize,
        changes: Vec<crate::editing::SourceChange>,
    ) {
        let deleted = changes
            .iter()
            .map(|change| text[change.range.clone()].to_string())
            .collect();
        self.push_undo(Snap {
            text: None,
            index,
            anchor,
            source_changes: Some(changes),
            deleted: Some(deleted),
            selections: None,
        });
        self.break_run();
    }

    /// Consume the operation captured from input semantics, never a text diff.
    pub(crate) fn take_native_change(&mut self) -> Option<SourceChange> {
        self.native_deleted = None;
        self.native_change.take()
    }

    pub(crate) fn take_native_edit(&mut self) -> Option<(SourceChange, String)> {
        Some((self.native_change.take()?, self.native_deleted.take()?))
    }

    fn push_undo(&mut self, mut snap: Snap) {
        snap.selections = self.source_selections.clone();
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
    history.native_change = None;
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
                    step_key(text, index, anchor, Some(Key::Backspace), false, "", false)
                },
            );
        }
        LogicalKey::Named(NamedKey::Delete) => {
            change(text, index, anchor, history, now, None, delete_forward);
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
                    step_key(text, index, anchor, Some(Key::Enter), false, "", true)
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
                |text, index, anchor| step_key(text, index, anchor, None, false, " ", false),
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
                    |text, index, anchor| step_key(text, index, anchor, None, false, &typed, false),
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
                    let deleted = text[start..end].to_string();
                    text.replace_range(start..end, "");
                    *index = start;
                    *anchor = start;
                    (
                        SourceChange {
                            range: start..end,
                            replacement: String::new(),
                        },
                        deleted,
                    )
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
                    let deleted = text[start..end].to_string();
                    text.replace_range(start..end, &clip);
                    let next = start + clip.len();
                    *index = next;
                    *anchor = next;
                    (
                        SourceChange {
                            range: start..end,
                            replacement: clip.clone(),
                        },
                        deleted,
                    )
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
                        return step_key(
                            text,
                            index,
                            anchor,
                            Some(Key::Backspace),
                            false,
                            "",
                            false,
                        );
                    }
                    let end = *index;
                    let start = word_left(text, end);
                    let deleted = text[start..end].to_string();
                    if start < end {
                        text.replace_range(start..end, "");
                        *index = start;
                        *anchor = start;
                    }
                    (
                        SourceChange {
                            range: start..end,
                            replacement: String::new(),
                        },
                        deleted,
                    )
                },
            );
            StepResult::default()
        }
    }
}

struct RestorePlan {
    changes: Vec<crate::editing::SourceChange>,
    index: usize,
    anchor: usize,
    selections: Option<crate::source_selection::SourceSelections>,
    /// Full preimage to install by moving the allocation. Delta steps leave this empty.
    install: Option<String>,
}

fn plan_restore(
    history: &mut EditHistory,
    _current: &str,
    index: usize,
    anchor: usize,
    undo: bool,
) -> Option<RestorePlan> {
    let prev = if undo {
        history.undo.pop()
    } else {
        history.redo.pop()
    }?;
    let saved_index = index;
    let saved_anchor = anchor;
    let selections = history.source_selections.clone();
    let (changes, deleted, install) =
        if let (Some(forward), Some(gone)) = (&prev.source_changes, &prev.deleted) {
            let (inverse, inverse_deleted) = invert_changes(forward, gone);
            (inverse, Some(inverse_deleted), None)
        } else if let Some(snapshot) = prev.text {
            let inverse = prev
                .source_changes
                .as_ref()
                .map(|changes| crate::editing::inverse_changes(&snapshot, changes));
            (inverse.unwrap_or_default(), None, Some(snapshot))
        } else {
            (Vec::new(), None, None)
        };
    let opposite = Snap {
        text: None,
        index: saved_index,
        anchor: saved_anchor,
        source_changes: (!changes.is_empty()).then(|| changes.clone()),
        deleted,
        selections,
    };
    history.source_selections = prev.selections.clone();
    if undo {
        history.redo.push(opposite);
    } else {
        history.undo.push(opposite);
    }
    history.run = None;
    Some(RestorePlan {
        changes,
        index: prev.index,
        anchor: prev.anchor,
        selections: prev.selections,
        install,
    })
}

pub(crate) struct SessionRestore {
    pub changes: Vec<crate::editing::SourceChange>,
    pub selections: crate::source_selection::SourceSelections,
}

pub(crate) fn plan_session_restore(
    history: &mut EditHistory,
    current: &str,
    index: usize,
    anchor: usize,
    undo: bool,
) -> Option<SessionRestore> {
    let plan = plan_restore(history, current, index, anchor, undo)?;
    let selections = plan.selections.clone().unwrap_or_else(|| {
        crate::source_selection::SourceSelections::single(crate::editing::SourceSelection {
            head: plan.index,
            anchor: plan.anchor,
        })
    });
    Some(SessionRestore {
        changes: plan.changes,
        selections,
    })
}

pub(crate) fn restore(
    text: &mut String,
    index: &mut usize,
    anchor: &mut usize,
    history: &mut EditHistory,
    undo: bool,
) -> Option<Vec<crate::editing::SourceChange>> {
    let plan = plan_restore(history, text, *index, *anchor, undo)?;
    if let Some(install) = plan.install {
        // Move the snapshot allocation into the live buffer and keep the
        // displaced buffer on the opposite stack. Delta steps never do this.
        let displaced = std::mem::replace(text, install);
        let stack = if undo {
            &mut history.redo
        } else {
            &mut history.undo
        };
        if let Some(snap) = stack.last_mut() {
            snap.text = Some(displaced);
        }
    } else {
        apply_splices(text, &plan.changes);
    }
    *index = plan.index.min(text.len());
    *anchor = plan.anchor.min(text.len());
    Some(plan.changes)
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
            let end = text.len();
            let deleted = std::mem::take(text);
            text.push_str(&next);
            *index = text.len();
            *anchor = *index;
            (
                SourceChange {
                    range: 0..end,
                    replacement: next.clone(),
                },
                deleted,
            )
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
        |text, index, anchor| step_key(text, index, anchor, None, false, &pasted, false),
    );
}

fn change(
    text: &mut String,
    index: &mut usize,
    anchor: &mut usize,
    history: &mut EditHistory,
    now: Duration,
    typed: Option<&str>,
    mutate: impl FnOnce(&mut String, &mut usize, &mut usize) -> (SourceChange, String),
) {
    let extend = typed.is_some_and(|typed| history.can_extend(*index, *anchor, now, typed));
    let saved_index = *index;
    let saved_anchor = *anchor;
    let (change, deleted) = mutate(text, index, anchor);
    if deleted == change.replacement {
        return;
    }
    history.native_change = Some(change.clone());
    history.native_deleted = Some(deleted.clone());
    if extend {
        // Only consecutive insertions at the previous run end can extend.
        // Keep the initial base range (possibly a replaced selection) and
        // append newly typed bytes in operation order, even on repeated text.
        let previous = history
            .undo
            .last_mut()
            .unwrap()
            .source_changes
            .as_mut()
            .unwrap()
            .last_mut()
            .unwrap();
        debug_assert!(change.range.is_empty());
        debug_assert_eq!(
            change.range.start,
            previous.range.start + previous.replacement.len()
        );
        previous.replacement.push_str(&change.replacement);
        history.redo.clear();
        history.note_typed(*index, now);
        return;
    }
    history.push_undo(Snap {
        text: None,
        index: saved_index,
        anchor: saved_anchor,
        source_changes: Some(vec![change]),
        deleted: Some(vec![deleted]),
        selections: None,
    });
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
) -> (SourceChange, String) {
    // Capture the same replacement range used by edit_string before mutation.
    // Selection replacement and backward grapheme deletion are deliberately
    // not reconstructed by matching equal prefix/suffix bytes afterwards.
    let caret = floor_boundary(text.len(), *index, &|i| text.as_bytes()[i]);
    let editing =
        !chars.is_empty() || key == Some(Key::Backspace) || (newline && key == Some(Key::Enter));
    let range = if editing && *index != *anchor {
        let (start, end) = span(text, *index, *anchor);
        start..end
    } else if key == Some(Key::Backspace) {
        crate::edit::delete_prev_grapheme(text, *index)
            .map_or(caret..caret, |(start, end)| start..end)
    } else {
        caret..caret
    };
    let deleted = text[range.clone()].to_string();
    let (next, next_anchor) = edit_string(text, *index, *anchor, key, shift, chars, newline);
    let replacement = if editing {
        text[range.start..next].to_string()
    } else {
        String::new()
    };
    *index = next;
    *anchor = next_anchor;
    (SourceChange { range, replacement }, deleted)
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

fn delete_forward(
    text: &mut String,
    index: &mut usize,
    anchor: &mut usize,
) -> (SourceChange, String) {
    if *index != *anchor {
        let (start, end) = span(text, *index, *anchor);
        let deleted = text[start..end].to_string();
        text.replace_range(start..end, "");
        *index = start;
        *anchor = start;
        return (
            SourceChange {
                range: start..end,
                replacement: String::new(),
            },
            deleted,
        );
    }
    let start = crate::edit::floor_grapheme(text, *index);
    let end = crate::edit::next_grapheme(text, start);
    let deleted = text[start..end].to_string();
    if start < end {
        text.replace_range(start..end, "");
        *index = start;
        *anchor = start;
    }
    (
        SourceChange {
            range: start..end,
            replacement: String::new(),
        },
        deleted,
    )
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

#[cfg(test)]
mod storage_tests {
    use super::*;

    #[test]
    fn source_metadata_evicts_with_snapshots_and_redo_invalidation() {
        use crate::editing::SourceChange;
        let mut history = EditHistory::default();
        history.record_source_changes(
            "abcd",
            2,
            2,
            vec![SourceChange {
                range: 0..0,
                replacement: "x".repeat(4096),
            }],
        );
        assert!(history.undo[0].retained_bytes() > 4096);
        // A budget large enough for the text alone must still evict metadata.
        history.trim_bytes(1024);
        assert!(history.undo.is_empty());
        history.record_source_changes(
            "abcd",
            2,
            2,
            vec![
                SourceChange {
                    range: 0..0,
                    replacement: "X".into(),
                },
                SourceChange {
                    range: 4..4,
                    replacement: "Y".into(),
                },
            ],
        );
        let mut text = "XabcdY".into();
        let (mut index, mut anchor) = (3, 3);
        let inverse = restore(&mut text, &mut index, &mut anchor, &mut history, true).unwrap();
        assert_eq!(inverse.len(), 2);
        assert!(history.redo[0].source_changes.is_some());
        history.record_source_edit(text, index, anchor);
        assert!(history.redo.is_empty());
        assert!(history.undo[0].source_changes.is_none());
    }

    #[test]
    fn restore_transfers_owned_allocations_and_directed_selection() {
        let before = String::from("é before");
        let before_ptr = before.as_ptr();
        let mut text = String::from("after 👩‍💻");
        let after_ptr = text.as_ptr();
        let mut history = EditHistory::default();
        history.record_source_edit(before, 2, 0);
        let (mut index, mut anchor) = (0, text.len());
        restore(&mut text, &mut index, &mut anchor, &mut history, true);
        assert_eq!(text.as_ptr(), before_ptr);
        assert_eq!(history.redo[0].text.as_ref().unwrap().as_ptr(), after_ptr);
        assert_eq!((index, anchor), (2, 0));
        restore(&mut text, &mut index, &mut anchor, &mut history, false);
        assert_eq!(text.as_ptr(), after_ptr);
        assert_eq!(history.undo[0].text.as_ref().unwrap().as_ptr(), before_ptr);
        assert_eq!((index, anchor), (0, text.len()));
    }

    #[test]
    #[ignore = "opt-in retained history capacity accounting, not RSS or allocator overhead"]
    fn snapshot_storage_accounting() {
        fn retained(history: &EditHistory) -> usize {
            history
                .undo
                .iter()
                .chain(&history.redo)
                .map(Snap::retained_bytes)
                .sum()
        }
        println!("document_bytes,snapshots_before_trim,retained_snapshot_bytes_before,retained_snapshot_bytes_after,vector_capacity_bytes_after");
        for bytes in [10_240, 1_048_576, 10_485_760] {
            let mut history = EditHistory::default();
            for _ in 0..8 {
                history.record_source_edit("x".repeat(bytes), bytes, bytes);
            }
            let before = retained(&history);
            history.trim_bytes(32 * 1024 * 1024);
            let after = retained(&history);
            assert!(after <= 32 * 1024 * 1024);
            println!(
                "{bytes},8,{before},{after},{}",
                (history.undo.capacity() + history.redo.capacity()) * std::mem::size_of::<Snap>()
            );
        }
    }
}
