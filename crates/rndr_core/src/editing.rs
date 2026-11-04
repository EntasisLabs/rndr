//! Experimental single-view source document sessions.
//!
//! Documents outlive their retained views. All managed mutations use the same
//! snapshot history as native typing; a Buffer is only a layout/input projection.
//! Raw Buffer writes on a managed entity are not a supported editing API.
//! Batches use ordered, nonoverlapping UTF-8 byte ranges in one base revision.
//! Duplicate starts (including adjacent insertion/replacement ambiguity) refuse.
//! This additive API is intentionally not a multi-view or collaborative editor.
use std::collections::{HashMap, VecDeque};
use std::ops::Range;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use crate::typing::{apply_text_event, EditHistory, EditSurface, StepResult};
use crate::{Buffer, Component, Entity, Focus, Input, KeyEvent, Platform, World};

/// Process-unique document identity, never reused across stores or reloads.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DocumentId(u64);
/// Monotonic revision within one document, including undo and reload.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Revision(pub u64);
/// One directed selection in UTF-8 bytes. Interactive endpoints snap to graphemes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SourceSelection {
    /// Fixed endpoint.
    pub anchor: usize,
    /// Moving endpoint.
    pub head: usize,
}
/// Mutation provenance; host tokens are echoed, never interpreted as authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditOrigin {
    /// Native keyboard or clipboard command.
    Keyboard,
    /// One native composition commit.
    Ime,
    /// Application command correlation token.
    Host(u64),
    /// Undo creates a new revision.
    Undo,
    /// Redo creates a new revision.
    Redo,
    /// Explicit clean external replacement; clears local history.
    Reload,
}
/// A splice in the batch's immutable base coordinates.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceChange {
    /// Half-open scalar-boundary byte range.
    pub range: Range<usize>,
    /// Exact source bytes, including newlines/control characters if supplied by host.
    pub replacement: String,
}
/// An atomic source operation. Validation occurs before any side effect.
#[derive(Clone, Debug)]
pub struct SourceBatch {
    /// Target identity.
    pub document: DocumentId,
    /// Expected current revision.
    pub base: Revision,
    /// Correlation/provenance.
    pub origin: EditOrigin,
    /// Ordered nonoverlapping splices in base coordinates.
    pub changes: Vec<SourceChange>,
}
/// Which side of inserted/replaced text an endpoint follows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Affinity {
    /// Before replacement bytes.
    Before,
    /// After replacement bytes (interactive default).
    After,
}
/// Immutable scalar position map. Callers must supply a valid base byte offset.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceMap {
    changes: Vec<SourceChange>,
}
impl SourceMap {
    /// Map a base endpoint; deleted interiors collapse according to affinity.
    pub fn map(&self, at: usize, affinity: Affinity) -> usize {
        let mut removed = 0;
        let mut inserted = 0;
        for change in &self.changes {
            let r = &change.range;
            if at < r.start {
                break;
            }
            if at < r.end || (at == r.start && r.is_empty()) {
                return r.start - removed
                    + inserted
                    + if affinity == Affinity::After {
                        change.replacement.len()
                    } else {
                        0
                    };
            }
            removed += r.len();
            inserted += change.replacement.len();
        }
        at - removed + inserted
    }
}
/// Ordered notification, with exact forward/inverse operations and maps.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChangeEvent {
    /// Document identity.
    pub document: DocumentId,
    /// Revision before commit.
    pub before: Revision,
    /// Revision after commit.
    pub after: Revision,
    /// Mutation correlation.
    pub origin: EditOrigin,
    /// Normalized (no identical splices) base-coordinate operations.
    pub changes: Vec<SourceChange>,
    /// Operations in after coordinates that restore before bytes.
    pub inverse: Vec<SourceChange>,
    /// Before-to-after mapping.
    pub forward_map: SourceMap,
    /// After-to-before mapping.
    pub inverse_map: SourceMap,
}
/// Captured immutable host/save payload; acknowledging it never writes the session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceSnapshot {
    /// Document identity.
    pub document: DocumentId,
    /// Captured revision.
    pub revision: Revision,
    /// Captured content, not a reference to future edits.
    pub text: String,
}
/// Admission refusal. No refused operation changes content, history or events.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditError {
    /// Wrong or absent document.
    MissingDocument,
    /// Revision no longer current.
    StaleRevision,
    /// Reversed/out-of-bounds/split-codepoint range or selection.
    InvalidRange,
    /// Unordered/overlapping ranges or duplicate start coordinate.
    Overlap,
    /// Document or inherited view policy is readonly.
    Readonly,
    /// Composition owns its captured range; retry after native commit/cancel.
    Composing,
    /// E1 supports only one attached view per document.
    AlreadyAttached,
    /// Stale or malformed view.
    InvalidView,
    /// Event consumer must drain notifications before more mutations.
    EventBackpressure,
    /// Monotonic revision space exhausted.
    RevisionExhausted,
}
impl std::fmt::Display for EditError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for EditError {}

/// Last native input refusal on a managed view. Hosts may remove after reporting.
/// Backpressure never silently mutates or discards the ordered event stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceInputError(pub EditError);
impl Component for SourceInputError {}

/// Document-owned history memory ceiling (snapshots only); oldest steps evict.
pub const HISTORY_BYTES: usize = 16 * 1024 * 1024;
/// Undrained event count ceiling. Backpressure refuses edits rather than losing events.
pub const EVENT_LIMIT: usize = 256;

/// Durable plain-source state. One selection is retained across E1 detach/reattach.
/// Each accepted keyboard mutation emits an event even when typing shares undo.
/// The host must drain events; mutations stop at [`EVENT_LIMIT`].
pub struct SourceSession {
    id: DocumentId,
    revision: Revision,
    text: String,
    selection: SourceSelection,
    history: EditHistory,
    readonly: bool,
    composition: Option<SourceSelection>,
    events: VecDeque<ChangeEvent>,
}
impl SourceSession {
    /// Exact current source.
    pub fn text(&self) -> &str {
        &self.text
    }
    /// Current revision.
    pub fn revision(&self) -> Revision {
        self.revision
    }
    /// Current single-view selection.
    pub fn selection(&self) -> SourceSelection {
        self.selection
    }
    /// True when document-owned admission is readonly.
    pub fn is_readonly(&self) -> bool {
        self.readonly
    }
    /// True while a native preedit owns a captured selection.
    pub fn is_composing(&self) -> bool {
        self.composition.is_some()
    }
    /// Set document policy. Native composition must end before a transition.
    pub fn set_readonly(&mut self, readonly: bool) -> Result<(), EditError> {
        if self.composition.is_some() && readonly != self.readonly {
            return Err(EditError::Composing);
        }
        self.readonly = readonly;
        Ok(())
    }
    /// Set one directed scalar-boundary selection, snapping interactive graphemes.
    pub fn set_selection(&mut self, selection: SourceSelection) -> Result<(), EditError> {
        if self.composition.is_some() {
            return Err(EditError::Composing);
        }
        if !self.text.is_char_boundary(selection.anchor)
            || !self.text.is_char_boundary(selection.head)
        {
            return Err(EditError::InvalidRange);
        }
        self.select(selection);
        self.history.break_run();
        Ok(())
    }
    fn select(&mut self, s: SourceSelection) {
        self.selection = SourceSelection {
            anchor: crate::edit::floor_grapheme(&self.text, s.anchor.min(self.text.len())),
            head: crate::edit::floor_grapheme(&self.text, s.head.min(self.text.len())),
        };
    }
    /// Capture a host/save snapshot, without changing dirty state or history.
    pub fn snapshot(&self) -> SourceSnapshot {
        SourceSnapshot {
            document: self.id,
            revision: self.revision,
            text: self.text.clone(),
        }
    }
    /// Drain ordered changes. No callback is invoked inside a commit.
    pub fn drain_events(&mut self) -> impl Iterator<Item = ChangeEvent> + '_ {
        self.events.drain(..)
    }
    fn admit(&self) -> Result<(), EditError> {
        if self.readonly {
            return Err(EditError::Readonly);
        }
        if self.composition.is_some() {
            return Err(EditError::Composing);
        }
        if self.events.len() >= EVENT_LIMIT {
            return Err(EditError::EventBackpressure);
        }
        if self.revision.0 == u64::MAX {
            return Err(EditError::RevisionExhausted);
        }
        Ok(())
    }
    /// Validate/commit one batch. Identical edits preserve selection and redo.
    pub fn commit(&mut self, batch: SourceBatch) -> Result<Option<ChangeEvent>, EditError> {
        if batch.document != self.id {
            return Err(EditError::MissingDocument);
        }
        if batch.base != self.revision {
            return Err(EditError::StaleRevision);
        }
        self.admit()?;
        let mut previous: Option<&Range<usize>> = None;
        for c in &batch.changes {
            let r = &c.range;
            if r.start > r.end
                || !self.text.is_char_boundary(r.start)
                || !self.text.is_char_boundary(r.end)
            {
                return Err(EditError::InvalidRange);
            }
            if previous.is_some_and(|p| r.start < p.end || r.start <= p.start) {
                return Err(EditError::Overlap);
            }
            previous = Some(r);
        }
        let changes: Vec<_> = batch
            .changes
            .into_iter()
            .filter(|c| self.text[c.range.clone()] != c.replacement)
            .collect();
        if changes.is_empty() {
            return Ok(None);
        }
        let mut next = self.text.clone();
        for c in changes.iter().rev() {
            next.replace_range(c.range.clone(), &c.replacement);
        }
        if next == self.text {
            return Ok(None);
        }
        self.history.record_source_edit(
            self.text.clone(),
            self.selection.head,
            self.selection.anchor,
        );
        let event = self.apply(changes, batch.origin);
        self.history.trim_bytes(HISTORY_BYTES);
        Ok(Some(event))
    }
    fn apply(&mut self, changes: Vec<SourceChange>, origin: EditOrigin) -> ChangeEvent {
        let mut inverse = Vec::new();
        let mut removed = 0;
        let mut inserted = 0;
        for c in &changes {
            let start = c.range.start - removed + inserted;
            inverse.push(SourceChange {
                range: start..start + c.replacement.len(),
                replacement: self.text[c.range.clone()].to_string(),
            });
            removed += c.range.len();
            inserted += c.replacement.len();
        }
        // Adjacent deletions can produce identical inverse insertion offsets.
        // Coalesce touching inverse ranges so every emitted inverse is itself a
        // valid base-coordinate batch under the same duplicate-start rule.
        let mut merged: Vec<SourceChange> = Vec::new();
        for c in inverse {
            if let Some(last) = merged
                .last_mut()
                .filter(|last| last.range.end == c.range.start)
            {
                last.range.end = c.range.end;
                last.replacement.push_str(&c.replacement);
            } else {
                merged.push(c);
            }
        }
        let inverse = merged;
        let map = SourceMap {
            changes: changes.clone(),
        };
        for c in changes.iter().rev() {
            self.text.replace_range(c.range.clone(), &c.replacement);
        }
        self.select(SourceSelection {
            anchor: map.map(self.selection.anchor, Affinity::After),
            head: map.map(self.selection.head, Affinity::After),
        });
        let before = self.revision;
        self.revision.0 += 1;
        let event = ChangeEvent {
            document: self.id,
            before,
            after: self.revision,
            origin,
            changes,
            inverse_map: SourceMap {
                changes: inverse.clone(),
            },
            inverse,
            forward_map: map,
        };
        self.events.push_back(event.clone());
        event
    }
    /// Undo via the same document-owned history as native typing and host batches.
    pub fn undo(&mut self) -> Result<Option<ChangeEvent>, EditError> {
        self.restore(true)
    }
    /// Redo creates a new revision, never revives an old revision number.
    pub fn redo(&mut self) -> Result<Option<ChangeEvent>, EditError> {
        self.restore(false)
    }
    fn restore(&mut self, undo: bool) -> Result<Option<ChangeEvent>, EditError> {
        self.admit()?;
        let old = self.text.clone();
        crate::typing::restore(
            &mut self.text,
            &mut self.selection.head,
            &mut self.selection.anchor,
            &mut self.history,
            undo,
        );
        let next = std::mem::replace(&mut self.text, old);
        if self.text == next {
            return Ok(None);
        }
        let selection = self.selection;
        let change = minimal_change(&self.text, &next);
        let event = self.apply(
            vec![change],
            if undo {
                EditOrigin::Undo
            } else {
                EditOrigin::Redo
            },
        );
        self.select(selection);
        self.history.trim_bytes(HISTORY_BYTES);
        Ok(Some(event))
    }
    /// Explicit externally authorized reload, not a save acknowledgement.
    /// Requires a current base and no composition. A changed reload clears
    /// history; identical bytes preserve all state and emit no event.
    pub fn reload(
        &mut self,
        base: Revision,
        text: impl Into<String>,
    ) -> Result<Option<ChangeEvent>, EditError> {
        if base != self.revision {
            return Err(EditError::StaleRevision);
        }
        self.admit()?;
        let next = text.into();
        if self.text == next {
            return Ok(None);
        }
        self.history = EditHistory::default();
        let c = SourceChange {
            range: 0..self.text.len(),
            replacement: next,
        };
        Ok(Some(self.apply(vec![c], EditOrigin::Reload)))
    }
    pub(crate) fn native_key(
        &mut self,
        event: &KeyEvent,
        platform: Platform,
        now: Duration,
        clip: &str,
        origin: EditOrigin,
        view_readonly: bool,
    ) -> Result<StepResult, EditError> {
        let action = crate::keys::standard_action(platform, event);
        // Navigation/copy is allowed in readonly, but never clipboard cut/paste.
        if (self.readonly || view_readonly) && !crate::interaction::readonly_field_key(event) {
            return Ok(StepResult::default());
        }
        if self.composition.is_some() {
            return Ok(StepResult::default());
        }
        if action == Some(crate::keys::EditAction::Undo) {
            self.undo()?;
            return Ok(StepResult::default());
        }
        if action == Some(crate::keys::EditAction::Redo) {
            self.redo()?;
            return Ok(StepResult::default());
        }
        use crate::keys::EditAction;
        let passive = matches!(
            action,
            Some(
                EditAction::Copy
                    | EditAction::SelectAll
                    | EditAction::WordLeft
                    | EditAction::WordRight
                    | EditAction::LineStart
                    | EditAction::LineEnd
                    | EditAction::DocStart
                    | EditAction::DocEnd
            )
        ) || (action.is_none() && crate::interaction::readonly_field_key(event));
        // Selection/copy need no event slot. Clear malformed text on navigation
        // events as well, so this exception cannot become a mutation bypass.
        if !passive {
            if self.events.len() >= EVENT_LIMIT {
                return Err(EditError::EventBackpressure);
            }
            if self.revision.0 == u64::MAX {
                return Err(EditError::RevisionExhausted);
            }
        }
        let mut event = event.clone();
        if self.readonly || view_readonly || passive {
            event.text = None;
        }
        let old = self.text.clone();
        if origin == EditOrigin::Ime {
            self.history.break_run();
        }
        let step = apply_text_event(
            &mut EditSurface {
                text: &mut self.text,
                index: &mut self.selection.head,
                anchor: &mut self.selection.anchor,
            },
            &event,
            platform,
            true,
            now,
            &mut self.history,
            clip,
        );
        if origin == EditOrigin::Ime {
            self.history.break_run();
        }
        let next = std::mem::replace(&mut self.text, old);
        let selection = self.selection;
        if self.text != next {
            let change = minimal_change(&self.text, &next);
            self.apply(vec![change], origin);
        }
        self.select(selection);
        self.history.trim_bytes(HISTORY_BYTES);
        Ok(step)
    }
}

fn minimal_change(before: &str, after: &str) -> SourceChange {
    let mut start = before
        .bytes()
        .zip(after.bytes())
        .take_while(|(a, b)| a == b)
        .count();
    while !before.is_char_boundary(start) || !after.is_char_boundary(start) {
        start -= 1;
    }
    let mut suffix = before[start..]
        .bytes()
        .rev()
        .zip(after[start..].bytes().rev())
        .take_while(|(a, b)| a == b)
        .count();
    while !before.is_char_boundary(before.len() - suffix)
        || !after.is_char_boundary(after.len() - suffix)
    {
        suffix -= 1;
    }
    SourceChange {
        range: start..before.len() - suffix,
        replacement: after[start..after.len() - suffix].to_string(),
    }
}

/// Resource holding source documents independently of view entities.
#[derive(Default)]
pub struct SourceDocuments {
    documents: HashMap<DocumentId, SourceSession>,
}
impl SourceDocuments {
    /// Open exact source bytes with revision zero and empty history.
    pub fn open(&mut self, text: impl Into<String>) -> DocumentId {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let id = DocumentId(
            NEXT.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
                .expect("document identity exhausted"),
        );
        let text = text.into();
        let selection = SourceSelection {
            anchor: text.len(),
            head: text.len(),
        };
        self.documents.insert(
            id,
            SourceSession {
                id,
                revision: Revision(0),
                text,
                selection,
                history: EditHistory::default(),
                readonly: false,
                composition: None,
                events: VecDeque::new(),
            },
        );
        id
    }
    /// Inspect a document.
    pub fn get(&self, id: DocumentId) -> Option<&SourceSession> {
        self.documents.get(&id)
    }
    /// Mutate through session admission, never through the projected Buffer.
    pub fn get_mut(&mut self, id: DocumentId) -> Option<&mut SourceSession> {
        self.documents.get_mut(&id)
    }
}

#[derive(Clone, Copy)]
pub(crate) struct SourceBinding {
    pub document: DocumentId,
    pub revision: Revision,
    selection: SourceSelection,
}
impl Component for SourceBinding {}

/// Read the document attached to a managed view, without granting raw mutation.
pub fn attached_source(world: &World, entity: Entity) -> Option<DocumentId> {
    world.get::<SourceBinding>(entity).map(|b| b.document)
}

/// Attach one existing Buffer/Editor entity. A second view explicitly refuses.
/// Documents/history survive detach and entity teardown. Install Chrome normally.
pub fn attach_source(
    world: &mut World,
    entity: Entity,
    document: DocumentId,
) -> Result<(), EditError> {
    if !world.has::<Buffer>(entity) || !world.has::<crate::Editor>(entity) {
        return Err(EditError::InvalidView);
    }
    if let Some(b) = world.get::<SourceBinding>(entity) {
        return if b.document == document {
            Ok(())
        } else {
            Err(EditError::AlreadyAttached)
        };
    }
    if world.entities_with::<SourceBinding>().iter().any(|&e| {
        world
            .get::<SourceBinding>(e)
            .is_some_and(|b| b.document == document)
    }) {
        return Err(EditError::AlreadyAttached);
    }
    let doc = world
        .resource_mut::<SourceDocuments>()
        .and_then(|d| d.get_mut(document))
        .ok_or(EditError::MissingDocument)?;
    // A view destroyed outside detach is a cancellation of its transient preedit.
    doc.composition = None;
    let revision = doc.revision;
    let selection = doc.selection;
    world.remove::<EditHistory>(entity);
    world.insert(
        entity,
        SourceBinding {
            document,
            revision,
            selection,
        },
    );
    project_source(world, entity);
    Ok(())
}
/// Detach without deleting the document/history. Active composition must end first.
pub fn detach_source(world: &mut World, entity: Entity) -> Result<(), EditError> {
    let binding = *world
        .get::<SourceBinding>(entity)
        .ok_or(EditError::InvalidView)?;
    if world
        .resource::<SourceDocuments>()
        .and_then(|d| d.get(binding.document))
        .is_some_and(SourceSession::is_composing)
    {
        return Err(EditError::Composing);
    }
    sync_selection(world, entity);
    world.remove::<SourceBinding>(entity);
    world.remove::<crate::ImePreview>(entity);
    if let Some(focus) = world.resource_mut::<Focus>() {
        if focus.entity == Some(entity) {
            focus.entity = None;
        }
    }
    Ok(())
}
pub(crate) fn sync_selection(world: &mut World, entity: Entity) {
    let Some(b) = world.get::<SourceBinding>(entity).copied() else {
        return;
    };
    let Some(buffer) = world.get::<Buffer>(entity) else {
        return;
    };
    let selection = SourceSelection {
        head: buffer.caret,
        anchor: buffer.anchor,
    };
    if let Some(doc) = world
        .resource_mut::<SourceDocuments>()
        .and_then(|d| d.get_mut(b.document))
    {
        if doc.revision == b.revision && doc.composition.is_none() && b.selection != selection {
            doc.select(selection);
            doc.history.break_run();
        }
    }
}
pub(crate) fn project_source(world: &mut World, entity: Entity) {
    let Some(b) = world.get::<SourceBinding>(entity).copied() else {
        return;
    };
    let Some(doc) = world
        .resource::<SourceDocuments>()
        .and_then(|d| d.get(b.document))
    else {
        return;
    };
    let (text, s, revision, readonly) =
        (doc.text.clone(), doc.selection, doc.revision, doc.readonly);
    if let Some(semantics) = world.get_mut::<crate::Semantics>(entity) {
        semantics.readonly = readonly;
    }
    if let Some(buffer) = world.get_mut::<Buffer>(entity) {
        crate::editor::replace_buffer(buffer, &text, s.head, s.anchor);
    }
    if let Some(binding) = world.get_mut::<SourceBinding>(entity) {
        binding.revision = revision;
        binding.selection = s;
    }
}
/// Synchronize native composition ownership before host Update systems run.
/// Chrome calls this in Input. Standalone apply_editor also calls it.
pub fn prepare_source_input(world: &mut World) {
    let focused = world.resource::<Focus>().and_then(|f| f.entity);
    let (preedit, mut commit) = world
        .resource::<Input>()
        .map(|i| (!i.ime_preedit.is_empty(), !i.ime_commit.is_empty()))
        .unwrap_or_default();
    // Composition remains owned by its original view until native commit/cancel.
    // A focus move never transfers its captured range to another document. An
    // orphan commit is discarded, not inserted into the new focus target.
    let orphan = world.entities_with::<SourceBinding>().into_iter().any(|e| {
        focused != Some(e)
            && world
                .get::<SourceBinding>(e)
                .and_then(|b| world.resource::<SourceDocuments>()?.get(b.document))
                .is_some_and(SourceSession::is_composing)
    });
    if orphan && commit {
        if let Some(input) = world.resource_mut::<Input>() {
            input.ime_commit.clear();
        }
        commit = false;
    }
    for entity in world.entities_with::<SourceBinding>() {
        sync_selection(world, entity);
        let b = *world.get::<SourceBinding>(entity).unwrap();
        if let Some(doc) = world
            .resource_mut::<SourceDocuments>()
            .and_then(|d| d.get_mut(b.document))
        {
            if !preedit && !commit {
                doc.composition = None;
            } else if !orphan
                && focused == Some(entity)
                && preedit
                && !doc.readonly
                && doc.composition.is_none()
            {
                doc.composition = Some(doc.selection);
                doc.history.break_run();
            }
        }
        project_source(world, entity);
    }
}
pub(crate) fn finish_composition(world: &mut World, entity: Entity) {
    let Some(b) = world.get::<SourceBinding>(entity).copied() else {
        return;
    };
    if let Some(doc) = world
        .resource_mut::<SourceDocuments>()
        .and_then(|d| d.get_mut(b.document))
    {
        if let Some(selection) = doc.composition.take() {
            doc.selection = selection;
        }
    }
}
