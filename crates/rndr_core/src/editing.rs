//! Experimental shared source document sessions.
//!
//! Documents outlive their retained views. All managed mutations use the same
//! snapshot history as native typing; a Buffer is only a layout/input projection.
//! Raw Buffer writes on a managed entity are not a supported editing API.
//! Batches use ordered, nonoverlapping UTF-8 byte ranges in one base revision.
//! Duplicate starts (including adjacent insertion/replacement ambiguity) refuse.
//! Views own selections and viewport state; documents own content and history.
//! This additive API is not a collaborative editor.
use std::collections::{HashMap, VecDeque};
use std::ops::Range;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use crate::source_selection::SourceSelections;
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

/// A fold hides whole lines: it starts after a newline and ends at a newline or EOF.
pub(crate) fn is_whole_line_fold(text: &str, start: usize, end: usize) -> bool {
    start > 0
        && start < end
        && end <= text.len()
        && text.is_char_boundary(start)
        && text.is_char_boundary(end)
        && text.as_bytes().get(start - 1) == Some(&b'\n')
        && (end == text.len() || text.as_bytes().get(end - 1) == Some(&b'\n'))
}

/// Map fold ranges through one commit. The exclusive end stays before an insertion
/// at that boundary. A range that no longer covers whole lines is dropped.
fn map_fold_ranges(text: &str, ranges: &[Range<usize>], map: &SourceMap) -> Vec<Range<usize>> {
    let mut mapped = Vec::new();
    for range in ranges {
        let start = map.map(range.start, Affinity::After);
        let end = map.map(range.end, Affinity::Before);
        if is_whole_line_fold(text, start, end) {
            mapped.push(start..end);
        }
    }
    mapped.sort_by_key(|range| range.start);
    let mut kept = Vec::new();
    let mut covered = 0usize;
    for range in mapped {
        if range.start >= covered {
            covered = range.end;
            kept.push(range);
        }
    }
    kept
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
    /// Entity is already attached to a different document.
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

/// Document-owned history memory ceiling (snapshots and splice metadata); oldest steps evict.
pub const HISTORY_BYTES: usize = 16 * 1024 * 1024;
/// Undrained event count ceiling. Backpressure refuses edits rather than losing events.
pub const EVENT_LIMIT: usize = 256;

/// Durable plain-source state. The last invoking selection survives detach/reattach.
/// Attached views retain independent directed selections. Undo restores the history
/// selection in the invoking view, maps other views, and never changes focus.
/// Each accepted keyboard mutation emits an event even when typing shares undo.
/// The host must drain events; mutations stop at [`EVENT_LIMIT`].
pub struct SourceSession {
    id: DocumentId,
    revision: Revision,
    text: std::sync::Arc<String>,
    selection: SourceSelection,
    history: EditHistory,
    views: HashMap<Entity, SourceSelection>,
    selections: SourceSelections,
    view_selections: HashMap<Entity, SourceSelections>,
    scroll_anchors: HashMap<Entity, usize>,
    /// Per-view whole-line folds in the current revision. Mapped with each commit.
    fold_ranges: HashMap<Entity, Vec<Range<usize>>>,
    /// Bumps on every accepted text change. Views use it instead of a second document copy.
    generation: u64,
    /// Splices newer than a view's last projection, oldest first. Each entry's
    /// generation is the document generation after that splice.
    edits: Vec<(u64, Vec<SourceChange>)>,
    active_view: Option<Entity>,
    readonly: bool,
    freeze: crate::freeze::FreezeState,
    pub(crate) policy_epoch: u64,
    composition: Option<SourceSelection>,
    events: VecDeque<ChangeEvent>,
}
impl SourceSession {
    /// Exact current source.
    pub fn text(&self) -> &str {
        &self.text
    }
    /// Bytes retained by undo and redo steps, excluding the live document.
    pub fn history_retained_bytes(&self) -> usize {
        self.history.retained_bytes()
    }
    pub(crate) fn generation(&self) -> u64 {
        self.generation
    }
    /// Forward splices applied after `generation`, in order. `None` when a step
    /// was discarded and the view must reload the whole document.
    pub(crate) fn edits_after(&self, generation: u64) -> Option<&[(u64, Vec<SourceChange>)]> {
        if generation >= self.generation {
            return Some(&[]);
        }
        let first = self.edits.first()?;
        if first.0 > generation + 1 {
            return None;
        }
        let start = self.edits.partition_point(|(gen, _)| *gen <= generation);
        Some(&self.edits[start..])
    }
    /// Map a byte from `generation` through later splices. `None` if a step was dropped.
    pub(crate) fn map_since(&self, generation: u64, at: usize) -> Option<usize> {
        let edits = self.edits_after(generation)?;
        let mut at = at;
        for (_, changes) in edits {
            at = SourceMap {
                changes: changes.clone(),
            }
            .map(at, Affinity::After);
        }
        Some(at)
    }
    /// Tight changed span of the single splice since `generation`, as
    /// `(old_start, old_end, new_end)`. `None` when more than one step is pending.
    pub(crate) fn span_since(
        &self,
        generation: u64,
        new_len: usize,
    ) -> Option<(usize, usize, usize)> {
        let edits = self.edits_after(generation)?;
        if edits.is_empty() {
            return Some((0, new_len, new_len));
        }
        if edits.len() != 1 {
            return None;
        }
        let changes = &edits[0].1;
        if changes.is_empty() {
            return Some((0, new_len, new_len));
        }
        let old_lo = changes[0].range.start;
        let old_hi = changes.last().unwrap().range.end;
        let removed: usize = changes.iter().map(|change| change.range.len()).sum();
        let inserted: usize = changes.iter().map(|change| change.replacement.len()).sum();
        let old_len = new_len + removed - inserted;
        let suffix = old_len.saturating_sub(old_hi);
        Some((old_lo, old_hi, new_len.saturating_sub(suffix)))
    }
    pub(crate) fn share_text(&self) -> std::sync::Arc<String> {
        std::sync::Arc::clone(&self.text)
    }
    /// Current revision.
    pub fn revision(&self) -> Revision {
        self.revision
    }
    /// Last invoking view selection (also the legacy host-command selection).
    pub fn selection(&self) -> SourceSelection {
        self.selection
    }
    /// Full normalized selections of the last invoking view.
    pub fn selections(&self) -> &SourceSelections {
        &self.selections
    }
    /// Full normalized selections for an attached view.
    pub fn view_selections(&self, view: Entity) -> Option<&SourceSelections> {
        self.view_selections.get(&view)
    }
    /// Set multiple directed selections. Revalidates against this document,
    /// since a set constructed for another snapshot is not authority.
    pub fn set_selections(&mut self, selections: SourceSelections) -> Result<(), EditError> {
        if self.composition.is_some() {
            return Err(EditError::Composing);
        }
        let selections = SourceSelections::new(
            &self.text,
            selections.ranges().to_vec(),
            selections.primary_index(),
        )?;
        self.select_many(selections);
        self.history.break_run();
        Ok(())
    }
    /// Set one view's full selection set against a current revision.
    pub fn set_view_selections(
        &mut self,
        view: Entity,
        base: Revision,
        selections: SourceSelections,
    ) -> Result<(), EditError> {
        if base != self.revision {
            return Err(EditError::StaleRevision);
        }
        if !self.views.contains_key(&view) {
            return Err(EditError::InvalidView);
        }
        if self.composition.is_some() {
            return Err(EditError::Composing);
        }
        let selections = SourceSelections::new(
            &self.text,
            selections.ranges().to_vec(),
            selections.primary_index(),
        )?;
        self.views.insert(view, selections.primary());
        self.view_selections.insert(view, selections.clone());
        if self.active_view == Some(view) {
            self.select_many(selections);
        }
        self.history.break_run();
        Ok(())
    }
    fn select_many(&mut self, selections: SourceSelections) {
        self.selection = selections.primary();
        self.selections = selections;
        if let Some(view) = self.active_view {
            self.views.insert(view, self.selection);
            self.view_selections.insert(view, self.selections.clone());
        }
    }
    /// Current directed selection for an attached view, in the current revision.
    pub fn view_selection(&self, view: Entity) -> Option<SourceSelection> {
        self.views.get(&view).copied()
    }
    /// Canonical viewport byte offset, eagerly mapped into the current revision.
    pub(crate) fn view_scroll_anchor(&self, view: Entity) -> Option<usize> {
        self.scroll_anchors.get(&view).copied()
    }
    /// Record a published viewport only for a currently attached view.
    pub(crate) fn set_view_scroll_anchor(&mut self, view: Entity, at: usize) {
        if self.views.contains_key(&view) && self.text.is_char_boundary(at) {
            self.scroll_anchors.insert(view, at);
        }
    }
    /// Whole-line folds last mapped into this revision. Empty after a drop.
    pub(crate) fn view_folds(&self, view: Entity) -> Option<&[Range<usize>]> {
        self.fold_ranges.get(&view).map(Vec::as_slice)
    }
    /// Replace one attached view's folds. The caller has already checked line bounds.
    pub(crate) fn set_view_folds(&mut self, view: Entity, ranges: Vec<Range<usize>>) {
        if !self.views.contains_key(&view) {
            return;
        }
        if ranges.is_empty() {
            self.fold_ranges.remove(&view);
        } else {
            self.fold_ranges.insert(view, ranges);
        }
    }
    pub(crate) fn clear_view_folds(&mut self, view: Entity) {
        self.fold_ranges.remove(&view);
    }
    /// Set one attached view's selection against an explicit current revision.
    /// Stale callers and composition refuse without changing history or focus.
    pub fn set_view_selection(
        &mut self,
        view: Entity,
        base: Revision,
        selection: SourceSelection,
    ) -> Result<(), EditError> {
        if base != self.revision {
            return Err(EditError::StaleRevision);
        }
        if !self.views.contains_key(&view) {
            return Err(EditError::InvalidView);
        }
        if self.composition.is_some() {
            return Err(EditError::Composing);
        }
        if !self.text.is_char_boundary(selection.anchor)
            || !self.text.is_char_boundary(selection.head)
        {
            return Err(EditError::InvalidRange);
        }
        let s = snapped_selection(&self.text, selection);
        self.views.insert(view, s);
        self.view_selections
            .insert(view, SourceSelections::single(s));
        if self.active_view == Some(view) {
            self.selection = s;
            self.selections = SourceSelections::single(s);
        }
        self.history.break_run();
        Ok(())
    }
    fn activate_view(&mut self, view: Entity) {
        if self.active_view != Some(view) {
            if let Some(s) = self.views.get(&view).copied() {
                self.selection = s;
                self.selections = self
                    .view_selections
                    .get(&view)
                    .cloned()
                    .unwrap_or_else(|| SourceSelections::single(s));
                self.active_view = Some(view);
                self.history.break_run();
            }
        }
    }
    /// True when document-owned admission is readonly.
    pub fn is_readonly(&self) -> bool {
        self.readonly || self.freeze.active()
    }
    /// Freeze content/history at the writer without cancelling live IME.
    /// Resolve composition through its editor owner before retrying.
    pub fn freeze(&self) -> Result<crate::DocumentFreeze, EditError> {
        if self.is_composing() {
            return Err(EditError::Composing);
        }
        self.freeze.acquire().ok_or(EditError::RevisionExhausted)
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
        if self.readonly != readonly {
            self.policy_epoch = self
                .policy_epoch
                .checked_add(1)
                .expect("source policy exhausted");
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
        self.select_many(SourceSelections::single(snapped_selection(
            &self.text, selection,
        )));
        self.history.break_run();
        Ok(())
    }
    fn select(&mut self, s: SourceSelection) {
        self.select_many(SourceSelections::single(snapped_selection(&self.text, s)));
    }
    /// Capture a host/save snapshot, without changing dirty state or history.
    pub fn snapshot(&self) -> SourceSnapshot {
        SourceSnapshot {
            document: self.id,
            revision: self.revision,
            text: (*self.text).clone(),
        }
    }
    /// Drain ordered changes. No callback is invoked inside a commit.
    pub fn drain_events(&mut self) -> impl Iterator<Item = ChangeEvent> + '_ {
        self.events.drain(..)
    }
    fn admit(&self) -> Result<(), EditError> {
        if self.is_readonly() {
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
        if changes.is_empty() || net_noop(self.text.as_str(), &changes) {
            return Ok(None);
        }
        self.history.source_selections = Some(self.selections.clone());
        self.history.record_source_changes(
            &self.text,
            self.selection.head,
            self.selection.anchor,
            changes.clone(),
        );
        let event = self.apply(changes, batch.origin);
        self.history.trim_bytes(HISTORY_BYTES);
        Ok(Some(event))
    }
    fn apply(&mut self, changes: Vec<SourceChange>, origin: EditOrigin) -> ChangeEvent {
        let inverse = inverse_changes(&self.text, &changes);
        let map = SourceMap {
            changes: changes.clone(),
        };
        {
            let text = std::sync::Arc::make_mut(&mut self.text);
            for c in changes.iter().rev() {
                text.replace_range(c.range.clone(), &c.replacement);
            }
        }
        for at in self.scroll_anchors.values_mut() {
            *at = map.map(*at, Affinity::After);
        }
        for ranges in self.fold_ranges.values_mut() {
            *ranges = map_fold_ranges(&self.text, ranges, &map);
        }
        self.fold_ranges.retain(|_, ranges| !ranges.is_empty());
        self.generation = self.generation.saturating_add(1);
        self.edits.push((self.generation, changes.clone()));
        if self.edits.len() > 64 {
            self.edits.remove(0);
        }
        for s in self.views.values_mut() {
            *s = snapped_selection(
                &self.text,
                SourceSelection {
                    anchor: map.map(s.anchor, Affinity::After),
                    head: map.map(s.head, Affinity::After),
                },
            );
        }
        for (view, selections) in &mut self.view_selections {
            *selections = selections.mapped(&self.text, &map);
            self.views.insert(*view, selections.primary());
        }
        self.select_many(self.selections.mapped(&self.text, &map));
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
        self.history.source_selections = Some(self.selections.clone());
        let Some(plan) = crate::typing::plan_session_restore(
            &mut self.history,
            &self.text,
            self.selection.head,
            self.selection.anchor,
            undo,
        ) else {
            return Ok(None);
        };
        if plan
            .changes
            .iter()
            .all(|change| self.text.get(change.range.clone()) == Some(change.replacement.as_str()))
        {
            return Ok(None);
        }
        // apply maps the selections that were current before this undo, then
        // the history selection replaces them.
        self.selection = self.selections.primary();
        let event = self.apply(
            plan.changes,
            if undo {
                EditOrigin::Undo
            } else {
                EditOrigin::Redo
            },
        );
        self.select_many(plan.selections);
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
        if self.text.as_str() == next {
            return Ok(None);
        }
        self.history = EditHistory::default();
        let c = SourceChange {
            range: 0..self.text.len(),
            replacement: next,
        };
        Ok(Some(self.apply(vec![c], EditOrigin::Reload)))
    }
    /// Copy disjoint selections in source order, separated by a newline.
    /// Empty carets contribute no clipboard bytes.
    pub fn copy_selections(&self) -> String {
        self.selections
            .ranges()
            .iter()
            .filter(|s| s.anchor != s.head)
            .map(|s| &self.text[s.anchor.min(s.head)..s.anchor.max(s.head)])
            .collect::<Vec<_>>()
            .join("\n")
    }
    /// Replace every selected range with the same exact source text in one
    /// history step. Native paste follows the same exact clipboard-byte policy.
    /// A stale base refuses atomically.
    pub fn replace_selections(
        &mut self,
        base: Revision,
        replacement: &str,
        origin: EditOrigin,
    ) -> Result<Option<ChangeEvent>, EditError> {
        if base != self.revision {
            return Err(EditError::StaleRevision);
        }
        let before = self.selections.clone();
        let changes = before
            .ranges()
            .iter()
            .map(|s| SourceChange {
                range: s.anchor.min(s.head)..s.anchor.max(s.head),
                replacement: replacement.to_string(),
            })
            .collect();
        let event = self.commit(SourceBatch {
            document: self.id,
            base,
            origin,
            changes,
        })?;
        if let Some(event) = &event {
            let ranges = before
                .ranges()
                .iter()
                .map(|s| {
                    let at = event
                        .forward_map
                        .map(s.anchor.min(s.head), Affinity::Before)
                        + replacement.len();
                    SourceSelection {
                        anchor: at,
                        head: at,
                    }
                })
                .collect();
            self.select_many(SourceSelections::new(
                &self.text,
                ranges,
                before.primary_index(),
            )?);
        }
        Ok(event)
    }
    fn native_multi_key(
        &mut self,
        event: &KeyEvent,
        platform: Platform,
        now: Duration,
        clip: &str,
        origin: EditOrigin,
    ) -> Result<StepResult, EditError> {
        use crate::keys::EditAction;
        let action = crate::keys::standard_action(platform, event);
        let before = self.selections.clone();
        let copied = matches!(action, Some(EditAction::Copy | EditAction::Cut))
            .then(|| self.copy_selections());
        let mut changes = Vec::new();
        let mut results = Vec::new();
        for selection in before.ranges() {
            // Reuse native grapheme/word/chord semantics in the immutable base.
            // No per-caret history or mutation escapes this planning phase.
            let mut text = (*self.text).clone();
            let mut s = *selection;
            let mut history = EditHistory::default();
            apply_text_event(
                &mut EditSurface {
                    text: &mut text,
                    index: &mut s.head,
                    anchor: &mut s.anchor,
                },
                event,
                platform,
                true,
                now,
                &mut history,
                clip,
            );
            let change = history.take_native_change();
            results.push((s, change.clone()));
            if let Some(change) = change {
                changes.push(change);
            }
        }
        changes.sort_by_key(|c| (c.range.start, c.range.end));
        // Word/grapheme deletes may reach into another caret's range. Refuse
        // conflicting plans rather than applying duplicated text/deletions.
        let event = if changes.is_empty() {
            None
        } else {
            self.commit(SourceBatch {
                document: self.id,
                base: self.revision,
                origin,
                changes,
            })?
        };
        let ranges = results
            .into_iter()
            .map(|(s, own)| {
                let map = |at| {
                    let Some(event) = &event else {
                        return at;
                    };
                    if let Some(own) = &own {
                        // s already includes its own splice; only earlier splices
                        // shift it into final coordinates.
                        let (removed, inserted) = event
                            .changes
                            .iter()
                            .filter(|c| c.range.start < own.range.start)
                            .fold((0, 0), |(r, i), c| {
                                (r + c.range.len(), i + c.replacement.len())
                            });
                        at - removed + inserted
                    } else {
                        event.forward_map.map(at, Affinity::After)
                    }
                };
                SourceSelection {
                    anchor: map(s.anchor),
                    head: map(s.head),
                }
            })
            .collect();
        self.select_many(SourceSelections::new(
            &self.text,
            ranges,
            before.primary_index(),
        )?);
        self.history.break_run(); // each native multi-caret event is one history step
        Ok(StepResult {
            copied,
            clipboard_action: matches!(
                action,
                Some(EditAction::Copy | EditAction::Cut | EditAction::Paste)
            ),
        })
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
        if event.released || event.synthetic {
            return Ok(StepResult::default());
        }
        let action = crate::keys::standard_action(platform, event);
        // Navigation/copy is allowed in readonly, but never clipboard cut/paste.
        if (self.is_readonly() || view_readonly) && !crate::interaction::readonly_field_key(event) {
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
        if self.is_readonly() || view_readonly || passive {
            event.text = None;
        }
        if self.selections.ranges().len() > 1 && origin != EditOrigin::Ime {
            return self.native_multi_key(&event, platform, now, clip, origin);
        }
        self.history.source_selections = Some(self.selections.clone());
        if origin == EditOrigin::Ime {
            self.history.break_run();
        }
        let step = apply_text_event(
            &mut EditSurface {
                text: std::sync::Arc::make_mut(&mut self.text),
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
        let selection = self.selection;
        if let Some((change, deleted)) = self.history.take_native_edit() {
            let (inverse, _) =
                crate::typing::invert_changes(std::slice::from_ref(&change), &[deleted]);
            {
                let text = std::sync::Arc::make_mut(&mut self.text);
                for change in inverse.iter().rev() {
                    text.replace_range(change.range.clone(), &change.replacement);
                }
            }
            self.apply(vec![change], origin);
        }
        self.select(selection);
        self.history.trim_bytes(HISTORY_BYTES);
        Ok(step)
    }
}

#[cfg(test)]
mod multi_selection_tests {
    use super::*;
    fn setup(text: &str) -> (SourceDocuments, DocumentId) {
        let mut docs = SourceDocuments::default();
        let id = docs.open(text);
        (docs, id)
    }
    fn set(doc: &mut SourceSession, ranges: &[(usize, usize)], primary: usize) {
        let selections = SourceSelections::new(
            doc.text(),
            ranges
                .iter()
                .map(|&(anchor, head)| SourceSelection { anchor, head })
                .collect(),
            primary,
        )
        .unwrap();
        doc.set_selections(selections).unwrap();
    }
    fn key(doc: &mut SourceSession, event: KeyEvent, clip: &str) -> Result<StepResult, EditError> {
        doc.native_key(
            &event,
            Platform::Windows,
            Duration::ZERO,
            clip,
            EditOrigin::Keyboard,
            false,
        )
    }
    fn command(ch: char) -> KeyEvent {
        let mut modifiers = crate::Modifiers::new();
        modifiers.ctrl = true;
        KeyEvent::character(ch, modifiers)
    }
    #[test]
    fn native_paste_copy_cut_restore_every_selection_one_step() {
        let (mut docs, id) = setup("one two three");
        let doc = docs.get_mut(id).unwrap();
        set(doc, &[(3, 0), (8, 13)], 1);
        let before = doc.selections().clone();
        assert_eq!(
            key(doc, command('c'), "").unwrap().copied.as_deref(),
            Some("one\nthree")
        );
        assert_eq!(doc.revision(), Revision(0));
        key(doc, command('v'), "😀\r\n").unwrap();
        assert_eq!(doc.text(), "😀\r\n two 😀\r\n");
        let after = doc.selections().clone();
        assert_eq!(
            after.ranges(),
            &[
                SourceSelection { anchor: 6, head: 6 },
                SourceSelection {
                    anchor: 17,
                    head: 17
                }
            ]
        );
        let events: Vec<_> = doc.drain_events().collect();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].changes.len(), 2);
        doc.undo().unwrap();
        assert_eq!(doc.text(), "one two three");
        assert_eq!(doc.selections(), &before);
        doc.redo().unwrap();
        assert_eq!(doc.selections(), &after);
        doc.undo().unwrap();
        key(doc, command('x'), "").unwrap();
        assert_eq!(doc.text(), " two ");
        doc.undo().unwrap();
        assert_eq!(doc.text(), "one two three");
        assert_eq!(doc.selections(), &before);
    }
    #[test]
    fn native_typing_and_grapheme_delete_are_batches() {
        let (mut docs, id) = setup("e\u{301} x😀");
        let doc = docs.get_mut(id).unwrap();
        set(doc, &[(3, 3), (9, 9)], 0);
        let before = doc.selections().clone();
        key(
            doc,
            KeyEvent::named(crate::NamedKey::Backspace, crate::Modifiers::new()),
            "",
        )
        .unwrap();
        assert_eq!(doc.text(), " x");
        doc.undo().unwrap();
        assert_eq!(doc.selections(), &before);
        key(doc, KeyEvent::character('z', crate::Modifiers::new()), "").unwrap();
        assert_eq!(doc.text(), "e\u{301}z x😀z");
        doc.undo().unwrap();
        assert_eq!(doc.text(), "e\u{301} x😀");
        assert_eq!(doc.selections(), &before);
    }
    #[test]
    fn overlap_readonly_stale_composition_and_backpressure_are_atomic() {
        let (mut docs, id) = setup("abcdef");
        let doc = docs.get_mut(id).unwrap();
        set(doc, &[(2, 2), (4, 4)], 0);
        let before = doc.selections().clone();
        let mut ctrl = crate::Modifiers::new();
        ctrl.ctrl = true;
        assert_eq!(
            key(doc, KeyEvent::named(crate::NamedKey::Backspace, ctrl), "").unwrap_err(),
            EditError::Overlap
        );
        assert_eq!(doc.text(), "abcdef");
        assert_eq!(doc.selections(), &before);
        assert_eq!(doc.revision(), Revision(0));
        doc.readonly = true;
        key(doc, command('v'), "x").unwrap();
        assert_eq!(doc.text(), "abcdef");
        assert_eq!(
            doc.replace_selections(Revision(0), "x", EditOrigin::Host(0))
                .unwrap_err(),
            EditError::Readonly
        );
        doc.readonly = false;
        doc.composition = Some(doc.selection);
        assert_eq!(
            doc.set_selections(before.clone()).unwrap_err(),
            EditError::Composing
        );
        doc.composition = None;
        assert_eq!(
            doc.replace_selections(Revision(1), "x", EditOrigin::Host(0))
                .unwrap_err(),
            EditError::StaleRevision
        );
        for _ in 0..EVENT_LIMIT {
            doc.replace_selections(doc.revision(), "x", EditOrigin::Host(0))
                .unwrap();
        }
        let snapshot = doc.snapshot();
        assert_eq!(
            key(doc, command('v'), "y").unwrap_err(),
            EditError::EventBackpressure
        );
        assert_eq!(doc.snapshot(), snapshot);
        key(doc, command('c'), "").unwrap(); // passive commands still admitted
    }
    #[test]
    fn host_batch_and_native_history_interleave_without_losing_sets() {
        let (mut docs, id) = setup("abcd");
        let doc = docs.get_mut(id).unwrap();
        set(doc, &[(0, 0), (4, 4)], 1);
        let original = doc.selections().clone();
        key(doc, KeyEvent::character('x', crate::Modifiers::new()), "").unwrap();
        let typed = doc.selections().clone();
        doc.commit(SourceBatch {
            document: id,
            base: doc.revision(),
            origin: EditOrigin::Host(5),
            changes: vec![SourceChange {
                range: 2..3,
                replacement: "hello".into(),
            }],
        })
        .unwrap();
        let hosted = doc.selections().clone();
        doc.undo().unwrap();
        assert_eq!(doc.selections(), &typed);
        doc.undo().unwrap();
        assert_eq!(doc.selections(), &original);
        doc.redo().unwrap();
        assert_eq!(doc.selections(), &typed);
        doc.redo().unwrap();
        assert_eq!(doc.selections(), &hosted);
    }
    #[test]
    fn views_keep_independent_sets_and_map_without_focus_transfer() {
        let mut world = World::new();
        let (docs, id) = setup("abcdef");
        world.insert_resource(docs);
        let a = world.spawn((Buffer::from_str(""), crate::Editor::default()));
        let b = world.spawn((Buffer::from_str(""), crate::Editor::default()));
        attach_source(&mut world, a, id).unwrap();
        attach_source(&mut world, b, id).unwrap();
        let doc = world
            .resource_mut::<SourceDocuments>()
            .unwrap()
            .get_mut(id)
            .unwrap();
        let peer = SourceSelections::new(
            doc.text(),
            vec![
                SourceSelection { anchor: 1, head: 1 },
                SourceSelection { anchor: 5, head: 5 },
            ],
            1,
        )
        .unwrap();
        doc.set_view_selections(b, doc.revision(), peer.clone())
            .unwrap();
        set(doc, &[(0, 0), (6, 6)], 0);
        key(doc, command('v'), "x").unwrap();
        assert_eq!(
            doc.view_selections(b).unwrap().ranges(),
            &[
                SourceSelection { anchor: 2, head: 2 },
                SourceSelection { anchor: 6, head: 6 }
            ]
        );
        doc.undo().unwrap();
        assert_eq!(doc.view_selections(b), Some(&peer));
        doc.activate_view(b);
        assert_eq!(doc.selections(), &peer);
        detach_source(&mut world, b).unwrap();
        let doc = world
            .resource::<SourceDocuments>()
            .unwrap()
            .get(id)
            .unwrap();
        assert!(doc.view_selections(b).is_none());
        assert_eq!(doc.selections(), &peer);
    }
}

/// True when applying `changes` reproduces `text`. A single same-length splice
/// compares only its own bytes. Several splices are compared when their net
/// length is unchanged, which is how cancelling batches preserve redo.
fn net_noop(text: &str, changes: &[SourceChange]) -> bool {
    let removed: usize = changes.iter().map(|change| change.range.len()).sum();
    let inserted: usize = changes.iter().map(|change| change.replacement.len()).sum();
    if removed != inserted {
        return false;
    }
    if changes.len() == 1 {
        return text[changes[0].range.clone()] == changes[0].replacement;
    }
    let bytes = text.as_bytes();
    let mut at = 0usize;
    let mut src = 0usize;
    for change in changes {
        let gap = change.range.start - src;
        if at + gap > bytes.len() || bytes[at..at + gap] != bytes[src..change.range.start] {
            return false;
        }
        at += gap;
        let replacement = change.replacement.as_bytes();
        if at + replacement.len() > bytes.len() || bytes[at..at + replacement.len()] != *replacement
        {
            return false;
        }
        at += replacement.len();
        src = change.range.end;
    }
    let tail = &bytes[src..];
    at + tail.len() == bytes.len() && bytes[at..] == *tail
}

/// Invert exact base-coordinate splices, retaining unchanged interior gaps.
pub(crate) fn inverse_changes(text: &str, changes: &[SourceChange]) -> Vec<SourceChange> {
    let mut inverse: Vec<SourceChange> = Vec::new();
    let mut removed = 0;
    let mut inserted = 0;
    for c in changes {
        let start = c.range.start - removed + inserted;
        let next = SourceChange {
            range: start..start + c.replacement.len(),
            replacement: text[c.range.clone()].to_string(),
        };
        // Touching inverse ranges must coalesce: adjacent deletions otherwise
        // create duplicate insertion offsets, which source batches prohibit.
        if let Some(last) = inverse.last_mut().filter(|last| last.range.end == start) {
            last.range.end = next.range.end;
            last.replacement.push_str(&next.replacement);
        } else {
            inverse.push(next);
        }
        removed += c.range.len();
        inserted += c.replacement.len();
    }
    inverse
}

fn snapped_selection(text: &str, s: SourceSelection) -> SourceSelection {
    SourceSelection {
        anchor: crate::edit::floor_grapheme(text, s.anchor.min(text.len())),
        head: crate::edit::floor_grapheme(text, s.head.min(text.len())),
    }
}

/// Resource holding source documents independently of view entities.
#[derive(Default)]
pub struct SourceDocuments {
    documents: HashMap<DocumentId, SourceSession>,
}
impl SourceDocuments {
    /// Exact retained native identities for host protection coverage checks.
    pub fn ids(&self) -> impl Iterator<Item = DocumentId> + '_ {
        self.documents.keys().copied()
    }
    /// Open exact source bytes with revision zero and empty history.
    pub fn open(&mut self, text: impl Into<String>) -> DocumentId {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let id = DocumentId(
            NEXT.try_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
                .expect("document identity exhausted"),
        );
        let text = std::sync::Arc::new(text.into());
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
                selections: SourceSelections::single(selection),
                views: HashMap::new(),
                view_selections: HashMap::new(),
                scroll_anchors: HashMap::new(),
                fold_ranges: HashMap::new(),
                generation: 1,
                edits: Vec::new(),
                active_view: None,
                readonly: false,
                freeze: Default::default(),
                policy_epoch: 0,
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
    attachment_epoch: u64,
    pub document: DocumentId,
    pub revision: Revision,
    selection: SourceSelection,
    view_readonly: bool,
    published_readonly: bool,
}
impl Component for SourceBinding {}

/// Read the document attached to a managed view, without granting raw mutation.
pub fn attached_source(world: &World, entity: Entity) -> Option<DocumentId> {
    world.get::<SourceBinding>(entity).map(|b| b.document)
}

/// Process-unique identity of the current attachment. Changes on detach/rebind,
/// even when the entity and document are unchanged; idempotent attach preserves it.
pub fn source_attachment_epoch(world: &World, entity: Entity) -> Option<u64> {
    world
        .get::<SourceBinding>(entity)
        .map(|b| b.attachment_epoch)
}

/// Attach an existing Buffer/Editor entity as an independent view.
/// New views start at the last invoking selection; their Editor scroll is local.
/// Documents/history survive detach and entity teardown. Install Chrome normally.
/// A structured authority on the same entity refuses before any side effect.
/// Attaching replaces the legacy Buffer and removes its legacy history; it does
/// not import either. For explicit adoption, see [`crate::editing_recipes`].
pub fn attach_source(
    world: &mut World,
    entity: Entity,
    document: DocumentId,
) -> Result<(), EditError> {
    if !world.has::<Buffer>(entity) || !world.has::<crate::Editor>(entity) {
        return Err(EditError::InvalidView);
    }
    if world.has::<crate::structured_native::StructuredBinding>(entity) {
        return Err(EditError::AlreadyAttached);
    }
    if let Some(b) = world.get::<SourceBinding>(entity) {
        if b.document != document {
            return Err(EditError::AlreadyAttached);
        }
        if world
            .resource::<SourceDocuments>()
            .and_then(|d| d.get(document))
            .is_some_and(|d| d.view_selection(entity).is_some())
        {
            return Ok(());
        }
        // A malformed view may have been retired during input preparation.
        // Repaired components can explicitly reattach to the durable session.
    }
    // Destroying the owner cancels its range, but retain the IME tombstone so
    // its eventual native commit cannot target this newly attached view.
    let dead_owner = world
        .resource::<SourceImeOwner>()
        .and_then(|o| o.0)
        .is_some_and(|(view, id)| {
            id == document
                && (attached_source(world, view) != Some(id)
                    || !world.has::<crate::Editor>(view)
                    || !world.has::<Buffer>(view))
        });
    if dead_owner {
        clear_source_composition(world, document);
    }
    let doc = world
        .resource_mut::<SourceDocuments>()
        .and_then(|d| d.get_mut(document))
        .ok_or(EditError::MissingDocument)?;
    if doc.is_composing() {
        return Err(EditError::Composing);
    }
    if doc.views.is_empty() {
        doc.active_view = Some(entity);
    }
    doc.views.insert(entity, doc.selection);
    doc.view_selections.insert(entity, doc.selections.clone());
    let revision = doc.revision;
    let selection = doc.selection;
    crate::source_folding::clear_source_folds(world, entity);
    world.remove::<EditHistory>(entity);
    world.insert(
        entity,
        SourceBinding {
            attachment_epoch: {
                static NEXT: AtomicU64 = AtomicU64::new(1);
                NEXT.try_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
                    .expect("source attachment identity exhausted")
            },
            document,
            revision,
            selection,
            view_readonly: world
                .get::<crate::Semantics>(entity)
                .is_some_and(|s| s.readonly),
            published_readonly: world
                .get::<crate::Semantics>(entity)
                .is_some_and(|s| s.readonly),
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
    if let Some(doc) = world
        .resource_mut::<SourceDocuments>()
        .and_then(|d| d.get_mut(binding.document))
    {
        doc.views.remove(&entity);
        doc.view_selections.remove(&entity);
        doc.scroll_anchors.remove(&entity);
        doc.fold_ranges.remove(&entity);
        if doc.active_view == Some(entity) {
            doc.active_view = None;
            doc.history.break_run();
        }
    }
    world.remove::<SourceBinding>(entity);
    crate::editor::forget_document_rows(world, entity);
    crate::source_folding::clear_source_folds(world, entity);
    world.remove::<crate::ImePreview>(entity);
    if world.resource::<Focus>().and_then(|f| f.entity) == Some(entity) {
        world.resource_mut::<Focus>().unwrap().entity = None;
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
            let s = snapped_selection(&doc.text, selection);
            doc.views.insert(entity, s);
            doc.view_selections
                .insert(entity, SourceSelections::single(s));
            if doc.active_view == Some(entity) {
                doc.selection = s;
                doc.selections = SourceSelections::single(s);
            }
            doc.history.break_run();
        }
    }
}
struct SourceProjection {
    document: DocumentId,
    revision: Revision,
    generation: u64,
}
impl crate::Component for SourceProjection {}

/// Refresh one managed view's Buffer/selection projection after a host command.
/// This does not mutate source, create history, activate the view or transfer focus.
/// Missing or detached entities are ignored. Ordinary input refreshes automatically.
pub fn project_source(world: &mut World, entity: Entity) {
    let Some(b) = world.get::<SourceBinding>(entity).copied() else {
        return;
    };
    let Some(doc) = world
        .resource::<SourceDocuments>()
        .and_then(|d| d.get(b.document))
    else {
        return;
    };
    let Some(s) = doc.view_selection(entity) else {
        return;
    };
    let projected_generation = world
        .get::<SourceProjection>(entity)
        .filter(|projection| projection.document == b.document)
        .map(|projection| projection.generation)
        .unwrap_or(0);
    let coherent = world
        .get::<Buffer>(entity)
        .is_some_and(|buffer| buffer.is_coherent());
    let unchanged =
        projected_generation == doc.generation() && b.revision == doc.revision && coherent;
    let edits = (!unchanged).then(|| {
        doc.edits_after(projected_generation).map(|edits| {
            edits
                .iter()
                .map(|(generation, changes)| (*generation, changes.clone()))
                .collect::<Vec<_>>()
        })
    });
    let text = (!unchanged).then(|| doc.share_text());
    let revision = doc.revision;
    let readonly = doc.is_readonly();
    let generation = doc.generation();
    let mapped_anchor = world
        .get::<SourceProjection>(entity)
        .filter(|p| p.document == b.document && p.revision != doc.revision)
        .and_then(|_| doc.view_scroll_anchor(entity));
    if let Some(byte) = mapped_anchor {
        crate::editor::project_scroll_anchor(world, entity, byte);
    }
    // Preserve view-owned readonly policy separately from document-owned projection.
    // A host policy change is detected before publishing the combined value; idle
    // reprojection must neither erase it nor retire valid delayed provider tickets.
    let observed = world
        .get::<crate::Semantics>(entity)
        .is_some_and(|s| s.readonly);
    let view_readonly = if observed != b.published_readonly {
        observed
    } else {
        b.view_readonly
    };
    let effective_readonly = readonly || view_readonly;
    if observed != effective_readonly {
        if let Some(semantics) = world.get_mut::<crate::Semantics>(entity) {
            semantics.readonly = effective_readonly;
        }
    }
    if let Some(binding) = world.get_mut::<SourceBinding>(entity) {
        binding.view_readonly = view_readonly;
        binding.published_readonly = effective_readonly;
    }
    if let Some(buffer) = world.get_mut::<Buffer>(entity) {
        if unchanged {
            buffer.caret = s.head;
            buffer.anchor = s.anchor;
        } else if let Some(Some(edits)) = edits.as_ref() {
            let final_len = text.as_ref().map(|text| text.len()).unwrap_or(0);
            let mut len = final_len;
            for (_, changes) in edits.iter().rev() {
                let inserted: usize = changes.iter().map(|change| change.replacement.len()).sum();
                let removed: usize = changes.iter().map(|change| change.range.len()).sum();
                len = len + removed - inserted;
            }
            if !edits.is_empty() && buffer.len() == len {
                for (_, changes) in edits {
                    buffer.apply_changes(changes);
                }
                let len = buffer.len();
                buffer.caret = s.head.min(len);
                buffer.anchor = s.anchor.min(len);
            } else if let Some(text) = &text {
                crate::editor::replace_buffer(buffer, text.as_str(), s.head, s.anchor);
            }
            buffer.mark_coherent();
        } else if let Some(text) = &text {
            crate::editor::replace_buffer(buffer, text.as_str(), s.head, s.anchor);
            buffer.mark_coherent();
        }
    }
    world.insert(
        entity,
        SourceProjection {
            document: b.document,
            revision,
            generation,
        },
    );
    if let Some(binding) = world.get_mut::<SourceBinding>(entity) {
        binding.revision = revision;
        binding.selection = s;
    }
}
/// Activate an invoking pane without transferring keyboard focus.
pub(crate) fn activate_source_view(world: &mut World, entity: Entity) {
    let Some(b) = world.get::<SourceBinding>(entity).copied() else {
        return;
    };
    if let Some(doc) = world
        .resource_mut::<SourceDocuments>()
        .and_then(|d| d.get_mut(b.document))
    {
        doc.activate_view(entity);
    }
}
// A dead view remains a tombstone until a native commit/cancel boundary.
#[derive(Default)]
struct SourceImeOwner(Option<(Entity, DocumentId)>);
#[derive(Default)]
struct SourceInputPrepared(bool);

pub(crate) fn prepare_chrome_source_input(world: &mut World) {
    prepare_source_input(world);
    world.insert_resource(SourceInputPrepared(true));
}
pub(crate) fn prepare_editor_source_input(world: &mut World) {
    let prepared = world
        .resource_mut::<SourceInputPrepared>()
        .is_some_and(|p| {
            let prepared = p.0;
            p.0 = false;
            prepared
        });
    if !prepared {
        prepare_source_input(world);
    } else {
        // Update systems may destroy or move the owner after Input preparation.
        guard_source_owner(world);
    }
}
fn clear_source_composition(world: &mut World, document: DocumentId) {
    if let Some(doc) = world
        .resource_mut::<SourceDocuments>()
        .and_then(|d| d.get_mut(document))
    {
        doc.composition = None;
    }
}
pub(crate) fn guard_source_owner(world: &mut World) {
    let Some((view, document)) = world.resource::<SourceImeOwner>().and_then(|o| o.0) else {
        return;
    };
    let live = attached_source(world, view) == Some(document)
        && world.has::<crate::Editor>(view)
        && world.has::<crate::Buffer>(view);
    if !live {
        clear_source_composition(world, document);
    }
    let focused = world.resource::<Focus>().and_then(|f| f.entity) == Some(view);
    if let Some(input) = world.resource_mut::<Input>() {
        // Candidate keys (including Tab/Escape) belong to the native IME.
        // Suppress duplicate key text too, not just an orphan commit string.
        input.keys.clear();
        input.key = None;
        input.chars.clear();
        if !live || !focused {
            input.ime_commit.clear();
            input.ime_preedit.clear();
            input.ime_cursor = None;
        }
    }
}

/// Synchronize native composition ownership before host Update systems run.
/// Chrome snapshots before consumer filtering: sanitized widget text must never
/// be mistaken for native cancellation. Standalone apply_editor prepares too.
pub fn prepare_source_input(world: &mut World) {
    let live: HashMap<_, _> = world
        .entities_with::<SourceBinding>()
        .into_iter()
        .filter(|&e| world.has::<Buffer>(e) && world.has::<crate::Editor>(e))
        .filter_map(|e| attached_source(world, e).map(|d| (e, d)))
        .collect();
    if let Some(docs) = world.resource_mut::<SourceDocuments>() {
        for doc in docs.documents.values_mut() {
            doc.views.retain(|e, _| live.get(e) == Some(&doc.id));
            doc.view_selections.retain(|e, _| doc.views.contains_key(e));
            doc.scroll_anchors.retain(|e, _| doc.views.contains_key(e));
            doc.fold_ranges.retain(|e, _| doc.views.contains_key(e));
            if doc.active_view.is_some_and(|e| !doc.views.contains_key(&e)) {
                doc.active_view = None;
                doc.history.break_run();
            }
        }
    }
    let focused = world.resource::<Focus>().and_then(|f| f.entity);
    let (mut preedit, mut commit) = world
        .resource::<Input>()
        .map(|i| (!i.ime_preedit.is_empty(), !i.ime_commit.is_empty()))
        .unwrap_or_default();
    if crate::events::<crate::WindowFocus>(world).contains(&crate::WindowFocus::Blurred) {
        // Blur is a native cancellation boundary, not consumer text filtering.
        preedit = false;
        commit = false;
        if let Some(input) = world.resource_mut::<Input>() {
            input.ime_preedit.clear();
            input.ime_commit.clear();
            input.ime_cursor = None;
            input.chars.clear();
            input.keys.clear();
            input.key = None;
        }
    }
    let owner = world.resource::<SourceImeOwner>().and_then(|o| o.0);
    if let Some((view, document)) = owner {
        if !preedit && !commit {
            clear_source_composition(world, document);
            world.insert_resource(SourceImeOwner::default());
        } else {
            guard_source_owner(world);
            let orphan = focused != Some(view) || attached_source(world, view) != Some(document);
            if commit && orphan {
                clear_source_composition(world, document);
                world.insert_resource(SourceImeOwner::default());
                commit = false;
            }
        }
    }
    let occupied = world
        .resource::<SourceImeOwner>()
        .and_then(|o| o.0)
        .is_some();
    for entity in world.entities_with::<SourceBinding>() {
        if !live.contains_key(&entity) {
            continue;
        }
        crate::editor::sync_source_scroll_anchor(world, entity);
        sync_selection(world, entity);
        let b = *world.get::<SourceBinding>(entity).unwrap();
        if let Some(doc) = world
            .resource_mut::<SourceDocuments>()
            .and_then(|d| d.get_mut(b.document))
        {
            if focused == Some(entity) && doc.composition.is_none() {
                doc.activate_view(entity);
            }
            if !preedit && !commit {
                doc.composition = None;
            } else if !occupied
                && owner.is_none()
                && focused == Some(entity)
                && preedit
                && !doc.is_readonly()
                && doc.composition.is_none()
            {
                // Native composition has exactly one captured range. Collapse
                // secondary carets at preedit start; never broadcast an IME commit.
                doc.select(doc.selection);
                doc.policy_epoch = doc
                    .policy_epoch
                    .checked_add(1)
                    .expect("source policy exhausted");
                doc.composition = Some(doc.selection);
                doc.history.break_run();
                world.insert_resource(SourceImeOwner(Some((entity, b.document))));
            }
        }
        project_source(world, entity);
    }
    guard_source_owner(world);
}
pub(crate) fn finish_composition(world: &mut World, entity: Entity) {
    if world
        .resource::<SourceImeOwner>()
        .and_then(|o| o.0)
        .is_some_and(|(view, _)| view == entity)
    {
        world.insert_resource(SourceImeOwner::default());
    }
    let Some(b) = world.get::<SourceBinding>(entity).copied() else {
        return;
    };
    if let Some(doc) = world
        .resource_mut::<SourceDocuments>()
        .and_then(|d| d.get_mut(b.document))
    {
        if let Some(selection) = doc.composition.take() {
            doc.select(selection);
        }
    }
}

#[cfg(test)]
mod scroll_anchor_tests {
    use super::*;

    fn setup() -> (World, DocumentId, Entity) {
        let mut world = World::new();
        let mut docs = SourceDocuments::default();
        let id = docs.open("first\nmiddle\nlast");
        world.insert_resource(docs);
        let view = world.spawn((Buffer::from_str(""), crate::Editor::default()));
        attach_source(&mut world, view, id).unwrap();
        (world, id, view)
    }
    fn session(world: &mut World, id: DocumentId) -> &mut SourceSession {
        world
            .resource_mut::<SourceDocuments>()
            .unwrap()
            .get_mut(id)
            .unwrap()
    }

    #[test]
    fn repeated_newlines_map_canonical_anchor_at_actual_native_insertion() {
        let (mut world, id, view) = setup();
        let doc = session(&mut world, id);
        doc.reload(doc.revision(), "\n\n\n\n").unwrap();
        doc.drain_events().for_each(drop);
        doc.set_selection(SourceSelection { anchor: 0, head: 0 })
            .unwrap();
        doc.set_view_scroll_anchor(view, 2);
        let event = KeyEvent::named(crate::NamedKey::Enter, crate::Modifiers::new());
        for i in 0..2 {
            doc.native_key(
                &event,
                Platform::Windows,
                Duration::ZERO,
                "",
                EditOrigin::Keyboard,
                false,
            )
            .unwrap();
            assert_eq!(doc.view_scroll_anchor(view), Some(3 + i));
            let change = doc.drain_events().next().unwrap();
            assert_eq!(
                change.changes,
                vec![SourceChange {
                    range: i..i,
                    replacement: "\n".into()
                }]
            );
        }
        doc.undo().unwrap();
        assert_eq!(doc.view_scroll_anchor(view), Some(3));
        doc.undo().unwrap();
        assert_eq!(doc.view_scroll_anchor(view), Some(2));
        doc.redo().unwrap();
        doc.redo().unwrap();
        assert_eq!(doc.view_scroll_anchor(view), Some(4));
    }

    #[test]
    fn repeated_clipboard_and_word_delete_operations_keep_exact_ranges() {
        let (mut world, id, view) = setup();
        let doc = session(&mut world, id);
        doc.reload(doc.revision(), "aaaa aaaa aaaa").unwrap();
        doc.drain_events().for_each(drop);
        let mut command = crate::Modifiers::new();
        command.ctrl = true;
        for (event, selection, clip, range, replacement) in [
            (
                KeyEvent::character('v', command),
                SourceSelection { anchor: 0, head: 0 },
                "a",
                0..0,
                "a",
            ),
            (
                KeyEvent::character('x', command),
                SourceSelection { anchor: 1, head: 0 },
                "",
                0..1,
                "",
            ),
            (
                KeyEvent::named(crate::NamedKey::Backspace, command),
                SourceSelection { anchor: 4, head: 4 },
                "",
                0..4,
                "",
            ),
        ] {
            doc.set_selection(selection).unwrap();
            doc.set_view_scroll_anchor(view, 5);
            doc.native_key(
                &event,
                Platform::Windows,
                Duration::ZERO,
                clip,
                EditOrigin::Keyboard,
                false,
            )
            .unwrap();
            let event = doc.drain_events().next().unwrap();
            assert_eq!(
                event.changes,
                vec![SourceChange {
                    range: range.clone(),
                    replacement: replacement.into()
                }]
            );
            assert_eq!(
                doc.view_scroll_anchor(view),
                Some(5 - range.len() + replacement.len())
            );
            doc.undo().unwrap();
            assert_eq!(doc.text(), "aaaa aaaa aaaa");
            assert_eq!(doc.view_scroll_anchor(view), Some(5));
            doc.drain_events().for_each(drop);
        }
    }

    #[test]
    fn exact_anchors_compose_without_projection_or_event_journal() {
        let (mut world, id, view) = setup();
        let doc = session(&mut world, id);
        doc.set_view_scroll_anchor(view, 6);
        let len = doc.text.len();
        doc.commit(SourceBatch {
            document: id,
            base: doc.revision(),
            origin: EditOrigin::Host(1),
            changes: vec![
                SourceChange {
                    range: 0..0,
                    replacement: "prefix\n".into(),
                },
                SourceChange {
                    range: len..len,
                    replacement: "\nsuffix".into(),
                },
            ],
        })
        .unwrap();
        assert_eq!(doc.view_scroll_anchor(view), Some(13));
        doc.drain_events().for_each(drop);
        doc.commit(SourceBatch {
            document: id,
            base: doc.revision(),
            origin: EditOrigin::Host(2),
            changes: vec![SourceChange {
                range: 0..0,
                replacement: "x\n".into(),
            }],
        })
        .unwrap();
        assert_eq!(doc.view_scroll_anchor(view), Some(15));
        doc.undo().unwrap();
        assert_eq!(doc.view_scroll_anchor(view), Some(13));
        doc.undo().unwrap();
        assert_eq!(doc.view_scroll_anchor(view), Some(6));
        doc.redo().unwrap();
        assert_eq!(doc.view_scroll_anchor(view), Some(13));
        doc.redo().unwrap();
        assert_eq!(doc.view_scroll_anchor(view), Some(15));
    }

    #[test]
    fn anchors_follow_native_reload_and_retire_with_views() {
        let (mut world, id, view) = setup();
        let stray = world.spawn((Buffer::from_str(""),));
        let doc = session(&mut world, id);
        doc.set_view_scroll_anchor(stray, 0);
        assert_eq!(doc.view_scroll_anchor(stray), None);
        doc.set_view_scroll_anchor(view, 6);
        doc.select(SourceSelection { anchor: 0, head: 0 });
        doc.native_key(
            &KeyEvent::character('x', crate::Modifiers::new()),
            Platform::Windows,
            Duration::ZERO,
            "",
            EditOrigin::Keyboard,
            false,
        )
        .unwrap();
        assert_eq!(doc.view_scroll_anchor(view), Some(7));
        doc.reload(doc.revision(), "é").unwrap();
        assert_eq!(doc.view_scroll_anchor(view), Some(2));
        doc.set_view_scroll_anchor(view, 1); // Invalid UTF-8 boundary is ignored.
        assert_eq!(doc.view_scroll_anchor(view), Some(2));
        detach_source(&mut world, view).unwrap();
        assert_eq!(session(&mut world, id).view_scroll_anchor(view), None);
        attach_source(&mut world, view, id).unwrap();
        session(&mut world, id).set_view_scroll_anchor(view, 0);
        world.remove::<Buffer>(view);
        prepare_source_input(&mut world);
        assert_eq!(session(&mut world, id).view_scroll_anchor(view), None);
        assert!(session(&mut world, id).scroll_anchors.is_empty());
    }
}
