//! Experimental owned source providers; no executor, transport, or language dependency.
//!
//! The host runs [`crate::source_extensions::SourceExtensions::refresh`] every input/update boundary and
//! may call [`crate::source_extensions::SourceExtensions::cancel_view`] for explicit cancellation. Native
//! lifecycle mutation boundaries automatically fence pending results. Results are also checked against the live World on every read/write.
//! Completion acceptance is one existing document-history transaction. Providers
//! never write projected Buffers. Higher priority wins; equal priority uses the
//! earlier registration. IME owns all completion keys while composing.
use std::collections::HashMap;
use std::ops::Range;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc,
};

use crate::editing::{
    activate_source_view, attached_source, project_source, ChangeEvent, DocumentId, EditError,
    EditOrigin, Revision, SourceBatch, SourceChange, SourceDocuments, SourceSelection,
    SourceSnapshot,
};
use crate::{Buffer, Color, Entity, Focus, World};

/// Process-unique registration identity. Removing it permanently retires its work.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ProviderId(u64);
/// Independent request streams; a new request supersedes only its own stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RequestKind {
    /// Editable focused completion popup.
    Completion,
    /// Readonly-compatible diagnostics.
    Diagnostics,
    /// Readonly-compatible syntax/search/annotation marks.
    Decoration,
    /// Focused nonmutating source navigation.
    Navigation,
}
/// Thread-safe immutable request identity with cooperative cancellation.
#[derive(Clone, Debug)]
pub struct RequestTicket {
    provider: ProviderId,
    serial: u64,
    focus_epoch: u64,
    entity_epochs: Vec<(Entity, u64)>,
    attachment_epoch: u64,
    policy_epoch: u64,
    document: DocumentId,
    revision: Revision,
    view: Entity,
    kind: RequestKind,
    selection: SourceSelection,
    selections: Vec<SourceSelection>,
    cancelled: Arc<AtomicBool>,
}
impl RequestTicket {
    /// Provider registration that owns this work.
    pub fn provider(&self) -> ProviderId {
        self.provider
    }
    /// Target document, distinct even across reload into a new session.
    pub fn document(&self) -> DocumentId {
        self.document
    }
    /// Captured content revision.
    pub fn revision(&self) -> Revision {
        self.revision
    }
    /// Requesting generation-qualified view.
    pub fn view(&self) -> Entity {
        self.view
    }
    /// Stream type.
    pub fn kind(&self) -> RequestKind {
        self.kind
    }
    /// Captured directed selection.
    pub fn selection(&self) -> SourceSelection {
        self.selection
    }
    /// Worker should stop promptly; late replies will still be refused.
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
    /// Cancel all clones of this ticket. Safe from producer threads.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }
}
/// Source extension refusal. No refusal mutates document/history/focus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExtensionError {
    /// Registration was retired or ticket is not owned by this controller.
    MissingProvider,
    /// Cancelled, superseded, changed source, or changed selection.
    Stale,
    /// View is dead, malformed, detached or bound elsewhere.
    InvalidView,
    /// Completion/navigation requires the requesting view still focused.
    Unfocused,
    /// Native composition owns input/selection.
    Composing,
    /// Mutation request on document or view readonly policy.
    Readonly,
    /// Wrong result stream or malformed response.
    InvalidResult,
    /// UTF-16 position is out of bounds or inside a surrogate/CRLF.
    InvalidPosition,
    /// Existing atomic document admission refused the edit.
    Edit(EditError),
}
impl std::fmt::Display for ExtensionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for ExtensionError {}
impl From<EditError> for ExtensionError {
    fn from(e: EditError) -> Self {
        Self::Edit(e)
    }
}
/// Strict zero-based line/UTF-16 code-unit coordinate, excluding line endings.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Utf16Position {
    /// Zero-based logical line.
    pub line: usize,
    /// UTF-16 units before the position on that line.
    pub character: usize,
}
fn line_range(text: &str, line: usize) -> Option<Range<usize>> {
    let start = if line == 0 {
        0
    } else {
        text.match_indices('\n').nth(line - 1)?.0 + 1
    };
    let end = text[start..].find('\n').map_or(text.len(), |n| start + n);
    let end = if end > start
        && text.as_bytes()[end - 1] == b'\r'
        && text.as_bytes().get(end) == Some(&b'\n')
    {
        end - 1
    } else {
        end
    };
    Some(start..end)
}
/// Convert without clamping. CRLF is one line terminator; its interior refuses.
pub fn utf16_to_byte(text: &str, position: Utf16Position) -> Result<usize, ExtensionError> {
    let range = line_range(text, position.line).ok_or(ExtensionError::InvalidPosition)?;
    let mut units = 0;
    for (offset, ch) in text[range.clone()].char_indices() {
        if units == position.character {
            return Ok(range.start + offset);
        }
        units += ch.len_utf16();
        if units > position.character {
            return Err(ExtensionError::InvalidPosition);
        }
    }
    if units == position.character {
        Ok(range.end)
    } else {
        Err(ExtensionError::InvalidPosition)
    }
}
/// Convert a scalar boundary, refusing the interior of CRLF and all split UTF-8.
pub fn byte_to_utf16(text: &str, byte: usize) -> Result<Utf16Position, ExtensionError> {
    if !text.is_char_boundary(byte) {
        return Err(ExtensionError::InvalidPosition);
    }
    let line = text[..byte].bytes().filter(|b| *b == b'\n').count();
    let range = line_range(text, line).ok_or(ExtensionError::InvalidPosition)?;
    if byte > range.end {
        return Err(ExtensionError::InvalidPosition);
    }
    Ok(Utf16Position {
        line,
        character: text[range.start..byte].encode_utf16().count(),
    })
}
/// Convert one future language-service edit without normalizing source/newlines.
pub fn utf16_change(
    text: &str,
    start: Utf16Position,
    end: Utf16Position,
    replacement: impl Into<String>,
) -> Result<SourceChange, ExtensionError> {
    let start = utf16_to_byte(text, start)?;
    let end = utf16_to_byte(text, end)?;
    if end < start {
        return Err(ExtensionError::InvalidPosition);
    }
    Ok(SourceChange {
        range: start..end,
        replacement: replacement.into(),
    })
}
/// One popup row; edits are in the ticket's immutable base UTF-8 coordinates.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompletionItem {
    /// Visible accessible row label.
    pub label: String,
    /// One atomic, ordered nonoverlapping document-history operation.
    pub changes: Vec<SourceChange>,
}
/// Generic owned span. Priority composes by registration, not by result timing.
#[derive(Clone, Debug, PartialEq)]
pub struct SourceDecoration {
    /// Scalar-boundary source interval (empty intervals are allowed).
    pub range: Range<usize>,
    /// Generic rendering color; interpretation of class is host-defined.
    pub color: Color,
    /// Application-neutral semantic/style class.
    pub class: String,
}
/// Generic gutter annotation positioned at a scalar source boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GutterMarker {
    /// Source byte position; host resolves the current logical/visual row.
    pub at: usize,
    /// Accessible label/tooltip.
    pub label: String,
}
/// Language-neutral diagnostic severity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiagnosticSeverity {
    /// Blocking error.
    Error,
    /// Potential problem.
    Warning,
    /// Informational annotation.
    Information,
    /// Optional hint.
    Hint,
}
/// Revision-owned diagnostic with optional atomic quick-fix changes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    /// Affected source interval.
    pub range: Range<usize>,
    /// Human-readable description.
    pub message: String,
    /// Generic severity.
    pub severity: DiagnosticSeverity,
    /// Optional quick-fix, admitted by the same guards/history as completion.
    pub fix: Vec<SourceChange>,
}
struct Published<T> {
    ticket: RequestTicket,
    items: Vec<T>,
}
type Stream = (ProviderId, Entity, RequestKind);
/// Host-owned provider registry/controller. It can be a World resource or field.
/// Call refresh before rendering/input. Native lifecycle mutations fence tickets.
#[derive(Default)]
pub struct SourceExtensions {
    providers: HashMap<ProviderId, i32>,
    requests: HashMap<Stream, RequestTicket>,
    completions: HashMap<ProviderId, Vec<Published<CompletionItem>>>,
    diagnostics: HashMap<ProviderId, Vec<Published<Diagnostic>>>,
    decorations: HashMap<ProviderId, Vec<Published<SourceDecoration>>>,
    gutters: HashMap<ProviderId, Vec<Published<GutterMarker>>>,
    serial: u64,
    selected: HashMap<Entity, (ProviderId, usize)>,
}
impl Drop for SourceExtensions {
    fn drop(&mut self) {
        for t in self.requests.values() {
            t.cancel();
        }
    }
}
// Capture the canonical target/ancestor chain, including epochs so same-frame
// readonly/reparent/composition ABA cannot revive a ticket. A raw mutable policy
// borrow on this chain conservatively cancels; unrelated UI policy is ignored.
fn entity_epochs(world: &World, view: Entity) -> Vec<(Entity, u64)> {
    let mut out = Vec::new();
    let mut current = Some(view);
    while let Some(entity) = current {
        if out.iter().any(|(e, _)| *e == entity) {
            break;
        }
        out.push((entity, world.source_entity_epoch(entity)));
        current = world.get::<crate::Parent>(entity).map(|p| p.entity);
    }
    out
}
fn live<'a>(
    world: &'a World,
    ticket: &RequestTicket,
) -> Result<&'a crate::editing::SourceSession, ExtensionError> {
    if ticket.is_cancelled() {
        return Err(ExtensionError::Stale);
    }
    if !world.is_alive(ticket.view)
        || attached_source(world, ticket.view) != Some(ticket.document)
        || !world.has::<Buffer>(ticket.view)
        || !world.has::<crate::Editor>(ticket.view)
    {
        return Err(ExtensionError::InvalidView);
    }
    let doc = world
        .resource::<SourceDocuments>()
        .and_then(|d| d.get(ticket.document))
        .ok_or(ExtensionError::InvalidView)?;
    if doc.revision() != ticket.revision || doc.view_selection(ticket.view).is_none() {
        return Err(ExtensionError::Stale);
    }
    if doc.is_composing()
        || world.has::<crate::ImePreview>(ticket.view)
        || world
            .resource::<crate::Input>()
            .is_some_and(|i| !i.ime_preedit.is_empty())
    {
        return Err(ExtensionError::Composing);
    }
    if matches!(
        ticket.kind,
        RequestKind::Completion | RequestKind::Navigation
    ) {
        if world.resource::<Focus>().and_then(|f| f.entity) != Some(ticket.view) {
            return Err(ExtensionError::Unfocused);
        }
        let buffer = world
            .get::<Buffer>(ticket.view)
            .ok_or(ExtensionError::InvalidView)?;
        if doc.view_selection(ticket.view) != Some(ticket.selection)
            || doc.view_selections(ticket.view).map(|s| s.ranges())
                != Some(ticket.selections.as_slice())
            || buffer.caret != ticket.selection.head
            || buffer.anchor != ticket.selection.anchor
        {
            return Err(ExtensionError::Stale);
        }
    }
    if ticket.kind == RequestKind::Completion {
        writable(world, ticket, doc)?;
    }
    if (matches!(
        ticket.kind,
        RequestKind::Completion | RequestKind::Navigation
    ) && ticket.focus_epoch != world.source_focus_epoch)
        || ticket.policy_epoch != doc.policy_epoch
        || ticket.entity_epochs != entity_epochs(world, ticket.view)
        || crate::editing::source_attachment_epoch(world, ticket.view)
            != Some(ticket.attachment_epoch)
    {
        return Err(ExtensionError::Stale);
    }
    Ok(doc)
}
fn writable(
    world: &World,
    ticket: &RequestTicket,
    doc: &crate::editing::SourceSession,
) -> Result<(), ExtensionError> {
    if doc.is_readonly() || crate::interaction::readonly(world, ticket.view) {
        return Err(ExtensionError::Readonly);
    }
    Ok(())
}
fn valid_range(text: &str, range: &Range<usize>) -> bool {
    range.start <= range.end
        && text.is_char_boundary(range.start)
        && text.is_char_boundary(range.end)
}
fn valid_changes(text: &str, changes: &[SourceChange]) -> Result<(), ExtensionError> {
    let mut previous: Option<&Range<usize>> = None;
    for c in changes {
        if !valid_range(text, &c.range) {
            return Err(ExtensionError::Edit(EditError::InvalidRange));
        }
        if previous.is_some_and(|p| c.range.start < p.end || c.range.start <= p.start) {
            return Err(ExtensionError::Edit(EditError::Overlap));
        }
        previous = Some(&c.range);
    }
    Ok(())
}
fn replace<T>(
    map: &mut HashMap<ProviderId, Vec<Published<T>>>,
    ticket: &RequestTicket,
    items: Vec<T>,
) {
    let values = map.entry(ticket.provider).or_default();
    values.retain(|v| v.ticket.view != ticket.view);
    values.push(Published {
        ticket: ticket.clone(),
        items,
    });
}
impl SourceExtensions {
    /// Register a provider. Higher priority paints later and owns popup first.
    /// Equal priority is resolved by earlier process-unique registration.
    pub fn register(&mut self, priority: i32) -> ProviderId {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let id = ProviderId(
            NEXT.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
                .expect("provider identity exhausted"),
        );
        self.providers.insert(id, priority);
        id
    }
    /// Retire a provider and synchronously remove all owned popup/marks/requests.
    pub fn remove_provider(&mut self, provider: ProviderId) {
        self.providers.remove(&provider);
        self.requests.retain(|(p, _, _), t| {
            if *p == provider {
                t.cancel();
                false
            } else {
                true
            }
        });
        self.completions.remove(&provider);
        self.diagnostics.remove(&provider);
        self.decorations.remove(&provider);
        self.gutters.remove(&provider);
    }
    /// Capture a request and source snapshot; transport is entirely host-owned.
    /// Supersedes old work only after the new request passes live admission.
    pub fn request(
        &mut self,
        world: &World,
        provider: ProviderId,
        view: Entity,
        kind: RequestKind,
    ) -> Result<(RequestTicket, SourceSnapshot), ExtensionError> {
        if !self.providers.contains_key(&provider) {
            return Err(ExtensionError::MissingProvider);
        }
        let document = attached_source(world, view).ok_or(ExtensionError::InvalidView)?;
        let doc = world
            .resource::<SourceDocuments>()
            .and_then(|d| d.get(document))
            .ok_or(ExtensionError::InvalidView)?;
        let ticket = RequestTicket {
            focus_epoch: world.source_focus_epoch,
            entity_epochs: entity_epochs(world, view),
            attachment_epoch: crate::editing::source_attachment_epoch(world, view)
                .ok_or(ExtensionError::InvalidView)?,
            policy_epoch: doc.policy_epoch,
            provider,
            serial: self
                .serial
                .checked_add(1)
                .expect("request serial exhausted"),
            document,
            revision: doc.revision(),
            view,
            kind,
            selection: doc
                .view_selection(view)
                .ok_or(ExtensionError::InvalidView)?,
            selections: doc
                .view_selections(view)
                .ok_or(ExtensionError::InvalidView)?
                .ranges()
                .to_vec(),
            cancelled: Arc::new(AtomicBool::new(false)),
        };
        live(world, &ticket)?;
        let snapshot = doc.snapshot();
        self.serial = ticket.serial;
        if let Some(old) = self.requests.insert((provider, view, kind), ticket.clone()) {
            old.cancel();
        }
        Ok((ticket, snapshot))
    }
    fn validate<'a>(
        &self,
        world: &'a World,
        ticket: &RequestTicket,
        kind: RequestKind,
    ) -> Result<&'a crate::editing::SourceSession, ExtensionError> {
        if !self.providers.contains_key(&ticket.provider) {
            return Err(ExtensionError::MissingProvider);
        }
        if ticket.kind != kind {
            return Err(ExtensionError::InvalidResult);
        }
        if !self
            .requests
            .get(&(ticket.provider, ticket.view, ticket.kind))
            .is_some_and(|t| {
                t.serial == ticket.serial && Arc::ptr_eq(&t.cancelled, &ticket.cancelled)
            })
        {
            return Err(ExtensionError::Stale);
        }
        live(world, ticket)
    }
    /// Publish a full replacement popup. Every row's edits validate atomically.
    pub fn publish_completions(
        &mut self,
        world: &World,
        ticket: &RequestTicket,
        items: Vec<CompletionItem>,
    ) -> Result<(), ExtensionError> {
        let doc = self.validate(world, ticket, RequestKind::Completion)?;
        for item in &items {
            valid_changes(doc.text(), &item.changes)?;
        }
        self.selected.insert(ticket.view, (ticket.provider, 0));
        replace(&mut self.completions, ticket, items);
        Ok(())
    }
    /// Publish replacement diagnostics, refusing invalid ranges/fixes as a whole.
    pub fn publish_diagnostics(
        &mut self,
        world: &World,
        ticket: &RequestTicket,
        items: Vec<Diagnostic>,
    ) -> Result<(), ExtensionError> {
        let doc = self.validate(world, ticket, RequestKind::Diagnostics)?;
        for item in &items {
            if !valid_range(doc.text(), &item.range) {
                return Err(ExtensionError::InvalidResult);
            }
            valid_changes(doc.text(), &item.fix)?;
        }
        replace(&mut self.diagnostics, ticket, items);
        Ok(())
    }
    /// Publish owned replacement decorations and generic gutter markers.
    pub fn publish_decorations(
        &mut self,
        world: &World,
        ticket: &RequestTicket,
        items: Vec<SourceDecoration>,
        gutters: Vec<GutterMarker>,
    ) -> Result<(), ExtensionError> {
        let doc = self.validate(world, ticket, RequestKind::Decoration)?;
        if items.iter().any(|i| !valid_range(doc.text(), &i.range))
            || gutters.iter().any(|i| !doc.text().is_char_boundary(i.at))
        {
            return Err(ExtensionError::InvalidResult);
        }
        replace(&mut self.decorations, ticket, items);
        replace(&mut self.gutters, ticket, gutters);
        Ok(())
    }
    fn ordered<'a, T>(
        &'a self,
        world: &World,
        view: Entity,
        map: &'a HashMap<ProviderId, Vec<Published<T>>>,
    ) -> Vec<&'a Published<T>> {
        let mut out: Vec<_> = map
            .values()
            .flatten()
            .filter(|p| {
                p.ticket.view == view && self.validate(world, &p.ticket, p.ticket.kind).is_ok()
            })
            .collect();
        out.sort_by_key(|p| {
            (
                self.providers[&p.ticket.provider],
                std::cmp::Reverse(p.ticket.provider),
            )
        });
        out
    }
    /// Highest-priority nonempty live popup. Old results never reappear on read.
    pub fn completions(
        &self,
        world: &World,
        view: Entity,
    ) -> Option<(ProviderId, &[CompletionItem])> {
        self.ordered(world, view, &self.completions)
            .into_iter()
            .rev()
            .find(|p| !p.items.is_empty())
            .map(|p| (p.ticket.provider, p.items.as_slice()))
    }
    /// Selected popup row. Returns zero for a new winning provider.
    pub fn completion_index(&self, view: Entity) -> usize {
        self.selected.get(&view).map_or(0, |(_, index)| *index)
    }
    /// Handle one chronological native key, after IME admission and before editing.
    /// Released/synthetic/modified events never accept a popup.
    pub fn completion_key(
        &mut self,
        world: &mut World,
        view: Entity,
        event: &crate::KeyEvent,
    ) -> bool {
        if !completion_control(event) {
            return false;
        }
        let Some((provider, items)) = self.completions(world, view) else {
            return false;
        };
        let len = items.len();
        let index = self
            .selected
            .get(&view)
            .filter(|(p, _)| *p == provider)
            .map_or(0, |(_, i)| (*i).min(len - 1));
        use crate::{LogicalKey, NamedKey};
        match event.logical {
            LogicalKey::Named(NamedKey::ArrowUp) => {
                self.selected
                    .insert(view, (provider, (index + len - 1) % len));
            }
            LogicalKey::Named(NamedKey::ArrowDown) => {
                self.selected.insert(view, (provider, (index + 1) % len));
            }
            LogicalKey::Named(NamedKey::Escape) => self.cancel_view(view),
            _ => {
                let _ = self.accept_completion(world, view, index);
            }
        }
        true
    }
    /// Live diagnostics in low-to-high priority rendering order.
    pub fn diagnostics(&self, world: &World, view: Entity) -> Vec<(ProviderId, &Diagnostic)> {
        self.ordered(world, view, &self.diagnostics)
            .into_iter()
            .flat_map(|p| p.items.iter().map(move |i| (p.ticket.provider, i)))
            .collect()
    }
    /// Live decorations in low-to-high priority rendering order; overlaps coexist.
    pub fn decorations(&self, world: &World, view: Entity) -> Vec<(ProviderId, &SourceDecoration)> {
        self.ordered(world, view, &self.decorations)
            .into_iter()
            .flat_map(|p| p.items.iter().map(move |i| (p.ticket.provider, i)))
            .collect()
    }
    /// Generic live gutter annotations in deterministic priority order.
    pub fn gutter_markers(&self, world: &World, view: Entity) -> Vec<(ProviderId, &GutterMarker)> {
        self.ordered(world, view, &self.gutters)
            .into_iter()
            .flat_map(|p| p.items.iter().map(move |i| (p.ticket.provider, i)))
            .collect()
    }
    fn apply(
        &mut self,
        world: &mut World,
        ticket: &RequestTicket,
        changes: Vec<SourceChange>,
    ) -> Result<Option<ChangeEvent>, ExtensionError> {
        let doc = self.validate(world, ticket, ticket.kind)?;
        writable(world, ticket, doc)?;
        if world.resource::<Focus>().and_then(|f| f.entity) != Some(ticket.view) {
            return Err(ExtensionError::Unfocused);
        }
        valid_changes(doc.text(), &changes)?;
        activate_source_view(world, ticket.view);
        let event = world
            .resource_mut::<SourceDocuments>()
            .and_then(|d| d.get_mut(ticket.document))
            .ok_or(ExtensionError::InvalidView)?
            .commit(SourceBatch {
                document: ticket.document,
                base: ticket.revision,
                origin: EditOrigin::Host(ticket.serial),
                changes,
            })?;
        project_source(world, ticket.view);
        self.cancel_view(ticket.view);
        Ok(event)
    }
    /// Accept a row from the live winning popup through shared document history.
    pub fn accept_completion(
        &mut self,
        world: &mut World,
        view: Entity,
        index: usize,
    ) -> Result<Option<ChangeEvent>, ExtensionError> {
        let p = self
            .ordered(world, view, &self.completions)
            .into_iter()
            .rev()
            .find(|p| !p.items.is_empty())
            .ok_or(ExtensionError::Stale)?;
        let changes = p
            .items
            .get(index)
            .ok_or(ExtensionError::InvalidResult)?
            .changes
            .clone();
        let ticket = p.ticket.clone();
        self.apply(world, &ticket, changes)
    }
    /// Accept one current diagnostic fix; readonly/focus/IME and history still apply.
    pub fn apply_diagnostic_fix(
        &mut self,
        world: &mut World,
        provider: ProviderId,
        view: Entity,
        index: usize,
    ) -> Result<Option<ChangeEvent>, ExtensionError> {
        let p = self
            .diagnostics
            .get(&provider)
            .and_then(|v| v.iter().find(|p| p.ticket.view == view))
            .ok_or(ExtensionError::Stale)?;
        let ticket = p.ticket.clone();
        let changes = p
            .items
            .get(index)
            .ok_or(ExtensionError::InvalidResult)?
            .fix
            .clone();
        self.apply(world, &ticket, changes)
    }
    /// Navigate within the requested document, without changing focus or history.
    /// Cross-document targets are host intents, not implicit open/focus authority.
    pub fn navigate(
        &mut self,
        world: &mut World,
        ticket: &RequestTicket,
        selection: SourceSelection,
    ) -> Result<(), ExtensionError> {
        let doc = self.validate(world, ticket, RequestKind::Navigation)?;
        if !doc.text().is_char_boundary(selection.anchor)
            || !doc.text().is_char_boundary(selection.head)
        {
            return Err(ExtensionError::InvalidResult);
        }
        world
            .resource_mut::<SourceDocuments>()
            .and_then(|d| d.get_mut(ticket.document))
            .ok_or(ExtensionError::InvalidView)?
            .set_view_selection(ticket.view, ticket.revision, selection)?;
        project_source(world, ticket.view);
        self.cancel_view(ticket.view);
        Ok(())
    }
    /// Explicitly cancel pending and displayed work for one view.
    /// Native detach/rebind and policy boundaries also invalidate tickets automatically.
    pub fn cancel_view(&mut self, view: Entity) {
        self.selected.remove(&view);
        self.requests.retain(|(_, v, _), t| {
            if *v == view {
                t.cancel();
                false
            } else {
                true
            }
        });
        for values in self.completions.values_mut() {
            values.retain(|p| p.ticket.view != view);
        }
        for values in self.diagnostics.values_mut() {
            values.retain(|p| p.ticket.view != view);
        }
        for values in self.decorations.values_mut() {
            values.retain(|p| p.ticket.view != view);
        }
        for values in self.gutters.values_mut() {
            values.retain(|p| p.ticket.view != view);
        }
    }
    /// Prune invalidated tickets/results and notify cooperative producers.
    /// Run each input boundary, not just when async work happens to finish.
    pub fn refresh(&mut self, world: &World) {
        self.requests.retain(|_, t| {
            if live(world, t).is_err() {
                t.cancel();
                false
            } else {
                true
            }
        });
        for values in self.completions.values_mut() {
            values.retain(|p| !p.ticket.is_cancelled());
        }
        for values in self.diagnostics.values_mut() {
            values.retain(|p| !p.ticket.is_cancelled());
        }
        for values in self.decorations.values_mut() {
            values.retain(|p| !p.ticket.is_cancelled());
        }
        for values in self.gutters.values_mut() {
            values.retain(|p| !p.ticket.is_cancelled());
        }
    }
}
/// Deterministic delayed fake transport. Hosts/tests advance it explicitly; no sleep.
#[derive(Default)]
pub struct FakeDelayedProvider {
    pending: Vec<(u64, RequestTicket, Vec<CompletionItem>)>,
}
impl FakeDelayedProvider {
    /// Enqueue a result for an arbitrary logical deadline; preserves cancelled work
    /// so the receiver can prove late delivery refuses instead of trusting transport.
    pub fn enqueue(&mut self, deadline: u64, ticket: RequestTicket, items: Vec<CompletionItem>) {
        self.pending.push((deadline, ticket, items));
    }
    /// Drain due results, possibly out of request order, through normal admission.
    pub fn deliver(
        &mut self,
        now: u64,
        extensions: &mut SourceExtensions,
        world: &World,
    ) -> Vec<Result<(), ExtensionError>> {
        let mut due = Vec::new();
        self.pending.retain(|(at, ticket, items)| {
            if *at <= now {
                due.push((ticket.clone(), items.clone()));
                false
            } else {
                true
            }
        });
        due.into_iter()
            .map(|(ticket, items)| extensions.publish_completions(world, &ticket, items))
            .collect()
    }
}

// Reserve controls from generic focus/keymap policy, but do not execute them
// until the editor reaches that event. Earlier text must invalidate acceptance.
fn completion_control(event: &crate::KeyEvent) -> bool {
    !event.released
        && !event.synthetic
        && !event.modifiers.command()
        && !event.modifiers.alt
        && !event.modifiers.shift
        && matches!(
            event.logical,
            crate::LogicalKey::Named(
                crate::NamedKey::ArrowUp
                    | crate::NamedKey::ArrowDown
                    | crate::NamedKey::Enter
                    | crate::NamedKey::Tab
                    | crate::NamedKey::Escape
            )
        )
}
pub(crate) fn reserves_key(world: &World, event: &crate::KeyEvent) -> bool {
    let Some(view) = world.resource::<Focus>().and_then(|f| f.entity) else {
        return false;
    };
    completion_control(event)
        && (attached_source(world, view).is_some_and(|id| {
            world
                .resource::<SourceDocuments>()
                .and_then(|d| d.get(id))
                .is_some_and(|d| d.is_composing())
                || world
                    .resource::<crate::Input>()
                    .is_some_and(|i| !i.ime_preedit.is_empty() || !i.ime_commit.is_empty())
        }) || world
            .resource::<SourceExtensions>()
            .is_some_and(|ex| ex.completions(world, view).is_some()))
}
pub(crate) fn native_completion_key(
    world: &mut World,
    view: Entity,
    event: &crate::KeyEvent,
) -> bool {
    if !world.has_resource::<SourceExtensions>() {
        return false;
    }
    let mut ex = std::mem::take(world.resource_mut::<SourceExtensions>().unwrap());
    let handled = ex.completion_key(world, view, event);
    *world.resource_mut::<SourceExtensions>().unwrap() = ex;
    handled
}
