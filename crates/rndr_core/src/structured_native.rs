//! Experimental native adapter for the separate structured tree.
//!
//! A `Buffer` is a disposable display/input projection, never the document or
//! its history. One attached view is supported per document in this slice.
//! Native geometry, grapheme motion and clipboard interpretation are shared;
//! tree transactions and undo are not routed through a source session.
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::editing::{Affinity, EditOrigin, Revision};
use crate::structured::{
    BlockId, BlockKind, Operation, Position, StructuredDocument, StructuredError,
    StructuredSelection, TextSelection, Transaction,
};
use crate::*;

/// Process-unique retained document handle, independent of ECS entity lifetime.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct StructuredHandle(u64);
/// Application-owned structured documents and their optional native views.
#[derive(Default)]
pub struct StructuredDocuments {
    documents: HashMap<StructuredHandle, StructuredDocument>,
    views: HashMap<StructuredHandle, Entity>,
}
impl StructuredDocuments {
    /// Retain an already validated tree without converting it to source text.
    pub fn insert(&mut self, document: StructuredDocument) -> StructuredHandle {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let handle = StructuredHandle(NEXT.fetch_add(1, Ordering::Relaxed));
        self.documents.insert(handle, document);
        handle
    }
    /// Read authoritative structure and history state.
    pub fn get(&self, handle: StructuredHandle) -> Option<&StructuredDocument> {
        self.documents.get(&handle)
    }
    /// Mutate through the document's typed transaction API. A concurrent host
    /// revision change invalidates any captured native composition.
    pub fn get_mut(&mut self, handle: StructuredHandle) -> Option<&mut StructuredDocument> {
        self.documents.get_mut(&handle)
    }
    /// Close only an unattached document. Detach its view first.
    pub fn remove(&mut self, handle: StructuredHandle) -> Option<StructuredDocument> {
        if self.views.contains_key(&handle) {
            return None;
        }
        self.documents.remove(&handle)
    }
}
/// Native view ownership and exact projected revision. Construct via attachment.
#[derive(Clone, Copy, Debug)]
pub struct StructuredBinding {
    /// Retained tree, not a source session identity.
    pub document: StructuredHandle,
    /// Revision last projected to the disposable buffer.
    pub revision: Revision,
    identity: crate::structured::StructuredDocumentId,
    attachment_epoch: u64,
}
impl Component for StructuredBinding {}
#[derive(Clone, Copy)]
struct ImeCapture {
    revision: Revision,
}
impl Component for ImeCapture {}
/// Refusal at the retained native boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StructuredViewError {
    /// Missing/stale entity, malformed view, missing document, or duplicate view.
    Unavailable,
    /// Toolbar/selection/detach cannot mutate a live composition.
    Composing,
    /// A preedit was captured against a different tree revision.
    StaleComposition,
    /// The typed model refused the operation atomically.
    Model(StructuredError),
}
impl Component for StructuredViewError {}
impl From<StructuredError> for StructuredViewError {
    fn from(error: StructuredError) -> Self {
        Self::Model(error)
    }
}
/// Attach an editor projection. The document and history outlive this entity.
pub fn attach_structured(
    world: &mut World,
    entity: Entity,
    document: StructuredHandle,
) -> Result<(), StructuredViewError> {
    if !world.has::<Editor>(entity)
        || !world.has::<Buffer>(entity)
        || world.has::<StructuredBinding>(entity)
        || world.has::<crate::editing::SourceBinding>(entity)
    {
        return Err(StructuredViewError::Unavailable);
    }
    let docs = world
        .resource_mut::<StructuredDocuments>()
        .ok_or(StructuredViewError::Unavailable)?;
    if docs
        .views
        .get(&document)
        .is_some_and(|view| *view != entity)
    {
        return Err(StructuredViewError::Unavailable);
    }
    let doc = docs.get(document).ok_or(StructuredViewError::Unavailable)?;
    let revision = doc.revision();
    let identity = doc.id();
    docs.views.insert(document, entity);
    world.remove::<crate::typing::EditHistory>(entity);
    world.insert(
        entity,
        StructuredBinding {
            document,
            revision,
            identity,
            attachment_epoch: {
                static NEXT_ATTACHMENT: AtomicU64 = AtomicU64::new(1);
                NEXT_ATTACHMENT.fetch_add(1, Ordering::Relaxed)
            },
        },
    );
    project(world, entity);
    Ok(())
}
/// Detach without deleting document/history. A live preedit must end first.
pub fn detach_structured(world: &mut World, entity: Entity) -> Result<(), StructuredViewError> {
    if is_composing(world, entity) {
        return Err(StructuredViewError::Composing);
    }
    let binding = world
        .remove::<StructuredBinding>(entity)
        .ok_or(StructuredViewError::Unavailable)?;
    if let Some(docs) = world.resource_mut::<StructuredDocuments>() {
        docs.views.remove(&binding.document);
    }
    world.remove::<crate::ImePreview>(entity);
    world.remove::<ImeCapture>(entity);
    Ok(())
}
/// Whether a native preedit owns the view's saved typed selection.
pub fn is_composing(world: &World, entity: Entity) -> bool {
    attached_structured(world, entity)
        .and_then(|id| world.resource::<StructuredDocuments>()?.get(id))
        .is_some_and(StructuredDocument::is_composing)
}
/// Monotonic attachment incarnation; detach/remount never recreates it.
pub fn structured_attachment_epoch(world: &World, entity: Entity) -> Option<u64> {
    attached_structured(world, entity)?;
    Some(world.get::<StructuredBinding>(entity)?.attachment_epoch)
}
/// Return the authoritative document attached to a native editor.
pub fn attached_structured(world: &World, entity: Entity) -> Option<StructuredHandle> {
    let binding = world.get::<StructuredBinding>(entity)?;
    let doc = world
        .resource::<StructuredDocuments>()?
        .get(binding.document)?;
    (doc.id() == binding.identity).then_some(binding.document)
}
/// Refuse formatting while an IME range is captured, including before the
/// editor pass has had an opportunity to capture a newly delivered preedit.
pub fn command_ready(
    world: &World,
    entity: Entity,
) -> Result<StructuredHandle, StructuredViewError> {
    let document = attached_structured(world, entity).ok_or(StructuredViewError::Unavailable)?;
    if is_composing(world, entity)
        || (world.resource::<Focus>().and_then(|f| f.entity) == Some(entity)
            && world
                .resource::<Input>()
                .is_some_and(|input| !input.ime_preedit.is_empty()))
    {
        return Err(StructuredViewError::Composing);
    }
    Ok(document)
}

fn document(world: &World, entity: Entity) -> Option<&StructuredDocument> {
    world
        .resource::<StructuredDocuments>()?
        .get(attached_structured(world, entity)?)
}
fn document_mut(world: &mut World, entity: Entity) -> Option<&mut StructuredDocument> {
    let id = attached_structured(world, entity)?;
    world.resource_mut::<StructuredDocuments>()?.get_mut(id)
}
/// Revision-fenced display coordinates of one actual tree block.
#[derive(Clone, Debug)]
pub struct StructuredBlockDisplay {
    /// Stable structural identity; not an ECS handle or byte index.
    pub block: BlockId,
    /// Text leaf or nontext horizontal-rule semantics.
    pub kind: BlockKind,
    /// Structural parent, never an editable depth-prefix string.
    pub parent: Option<crate::structured::ContainerId>,
    /// Canonical inert metadata; interpretation belongs to the host.
    pub attributes: crate::structured::Attributes,
    /// Half-open display range, excluding the block separator.
    pub range: std::ops::Range<usize>,
}
/// Native/AT tree-to-display contract. Display text is not a serialization codec.
#[derive(Clone, Debug)]
pub struct StructuredDisplay {
    /// Retained authority owning the tree.
    pub document: StructuredHandle,
    /// Fence required when interpreting display offsets.
    pub revision: Revision,
    /// Directed typed selection, retaining whole-node selection where applicable.
    pub selection: StructuredSelection,
    /// Plain display text with one newline per block boundary.
    pub text: String,
    /// Ordered structural boundaries for block navigation and accessible adapters.
    pub blocks: Vec<StructuredBlockDisplay>,
    /// Identity-bearing quote/list/item structure for native and AT adapters.
    pub containers: Vec<crate::structured::Container>,
}
/// Explicitly reveal a typed selection after a toolbar action. Pointer task
/// toggles must not call this: they preserve the user's current viewport.
pub fn reveal_selection(world: &mut World, entity: Entity) {
    if attached_structured(world, entity).is_some() {
        project(world, entity);
        crate::editor::reveal_selection(world, entity);
    }
}
/// Read the current disposable view projection.
pub fn structured_display(world: &World, entity: Entity) -> Option<StructuredDisplay> {
    let binding = world.get::<StructuredBinding>(entity)?;
    let doc = document(world, entity)?;
    if doc.revision() != binding.revision {
        return None;
    }
    let text = doc.plain_text();
    let buffer = world.get::<Buffer>(entity)?;
    if buffer.text() != text || display_selection(doc).ok() != Some((buffer.anchor, buffer.caret)) {
        return None;
    }
    let mut start = 0;
    let blocks = doc
        .blocks()
        .iter()
        .map(|b| {
            let range = start..start + b.len();
            start += b.len() + 1;
            StructuredBlockDisplay {
                block: b.id,
                kind: b.kind,
                parent: b.parent,
                attributes: b.attributes.clone(),
                range,
            }
        })
        .collect();
    Some(StructuredDisplay {
        document: binding.document,
        revision: doc.revision(),
        selection: doc.selection(),
        text,
        blocks,
        containers: doc.containers().to_vec(),
    })
}
fn display_selection(doc: &StructuredDocument) -> Result<(usize, usize), StructuredError> {
    match doc.selection() {
        StructuredSelection::Text(s) => {
            Ok((doc.display_offset(s.anchor)?, doc.display_offset(s.head)?))
        }
        StructuredSelection::Cells { head, .. } => {
            let offset = |cell| -> Result<usize, StructuredError> {
                let id = doc.cell_first_block(cell)?;
                Ok(doc
                    .blocks()
                    .iter()
                    .take_while(|b| b.id != id)
                    .map(|b| b.len() + 1)
                    .sum())
            };
            // Rectangle chrome owns structural selection. Publishing a scalar
            // range here would falsely highlight intervening, unselected cells.
            let head = offset(head)?;
            Ok((head, head))
        }
        StructuredSelection::Node(id) => {
            let block = doc
                .blocks()
                .iter()
                .find(|b| b.id == id)
                .ok_or(StructuredError::InvalidPosition)?;
            let at = doc
                .blocks()
                .iter()
                .take_while(|b| b.id != id)
                .map(|b| b.len() + 1)
                .sum::<usize>();
            Ok((at, at + block.len()))
        }
    }
}
/// Translate exact-revision display endpoints to typed block positions. Used by
/// pointer/AT adapters only; hosts should prefer the model's typed selections.
pub fn set_display_selection(
    world: &mut World,
    entity: Entity,
    revision: Revision,
    anchor: usize,
    head: usize,
) -> Result<(), StructuredViewError> {
    command_ready(world, entity)?;
    let doc = document_mut(world, entity).ok_or(StructuredViewError::Unavailable)?;
    if doc.revision() != revision {
        return Err(StructuredError::StaleRevision.into());
    }
    let text = doc.plain_text();
    if !text.is_char_boundary(anchor) || !text.is_char_boundary(head) {
        return Err(StructuredError::InvalidPosition.into());
    }
    let anchor = crate::edit::floor_grapheme(&text, anchor);
    let head = crate::edit::floor_grapheme(&text, head);
    let selection = display_to_selection(doc, anchor, head)?;
    doc.set_selection(selection)?;
    project(world, entity);
    Ok(())
}
// A horizontal rule has a display row but no legal text position. Pointer/AT
// and key motion select the actual node at that seam, not an empty paragraph.
fn display_to_selection(
    doc: &StructuredDocument,
    anchor: usize,
    head: usize,
) -> Result<StructuredSelection, StructuredError> {
    let mut start = 0;
    for block in doc.blocks() {
        if block.is_atomic() && anchor == start && head == start {
            return Ok(StructuredSelection::Node(block.id));
        }
        start += block.len() + 1;
    }
    let endpoint = |at: usize, forward: bool| -> Result<Position, StructuredError> {
        if let Ok(p) = doc.position_at(at, Affinity::After) {
            return Ok(p);
        }
        let mut start = 0;
        let candidates = doc
            .blocks()
            .iter()
            .filter_map(|block| {
                let offset = start;
                start += block.len() + 1;
                (!block.is_atomic()).then_some((block, offset))
            })
            .collect::<Vec<_>>();
        let (block, offset) = if forward {
            candidates
                .iter()
                .copied()
                .find(|(_, offset)| *offset > at)
                .or_else(|| candidates.last().copied())
        } else {
            candidates
                .iter()
                .copied()
                .rev()
                .find(|(_, offset)| *offset < at)
                .or_else(|| candidates.first().copied())
        }
        .ok_or(StructuredError::InvalidPosition)?;
        Ok(Position {
            block: block.id,
            offset: if offset < at { block.len() } else { 0 },
            affinity: Affinity::After,
        })
    };
    let a = endpoint(anchor, anchor > head)?;
    let h = endpoint(head, head >= anchor)?;
    if let (Some(a), Some(h)) = (doc.cell_address(a.block)?, doc.cell_address(h.block)?) {
        if a.cell != h.cell && a.table == h.table {
            return Ok(StructuredSelection::Cells {
                anchor: a.cell,
                head: h.cell,
            });
        }
    }
    Ok(StructuredSelection::Text(TextSelection {
        anchor: a,
        head: h,
    }))
}
/// Rebuild disposable text and rich spans from the canonical tree.
pub fn project(world: &mut World, entity: Entity) {
    let Some(doc) = document(world, entity) else {
        return;
    };
    let text = doc.plain_text();
    let Ok((anchor, head)) = display_selection(doc) else {
        return;
    };
    let revision = doc.revision();
    // `Color` fills the entity; glyph foreground belongs to `Label::color`.
    let color = world
        .get::<Label>(entity)
        .and_then(|label| label.color)
        .unwrap_or_else(|| theme_of(world).text);
    let mut spans = Vec::new();
    let mut offset = 0;
    for block in doc.blocks() {
        let header = doc.ancestors(block.id).unwrap_or_default().iter().any(|c| {
            matches!(
                c.kind,
                crate::structured::ContainerKind::Cell { header: true }
            )
        });
        for run in &block.runs {
            let mut span = TextSpan::new(
                offset,
                offset + run.text.len(),
                if let Some([r, g, b]) = run.marks.text_style.color {
                    Color::rgb(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0)
                } else if run.marks.link.is_some() {
                    Color::rgb(0.35, 0.65, 1.0)
                } else {
                    color
                },
            );
            span.weight = Some(
                if run.marks.strong || matches!(block.kind, BlockKind::Heading(_)) {
                    700
                } else if header {
                    600
                } else {
                    400
                },
            );
            span.family = run.marks.text_style.family.clone();
            span.italic = run.marks.emphasis;
            span.underline = run.marks.link.is_some();
            spans.push(span);
            offset += run.text.len();
        }
        offset += 1;
    }
    if let Some(buffer) = world.get_mut::<Buffer>(entity) {
        crate::editor::replace_buffer(buffer, &text, head, anchor);
    }
    world.insert(entity, Spans { runs: spans });
    if let Some(binding) = world.get_mut::<StructuredBinding>(entity) {
        binding.revision = revision;
    }
    // Readonly admission reads tree policy without overwriting view metadata.
    world.remove::<crate::typing::EditHistory>(entity);
}
/// Internal pointer-selection synchronization; no tree mutation or history step.
pub(crate) fn sync_selection(world: &mut World, entity: Entity) {
    if is_composing(world, entity) {
        return;
    }
    let Some(binding) = world.get::<StructuredBinding>(entity).copied() else {
        return;
    };
    let Some(buffer) = world.get::<Buffer>(entity) else {
        return;
    };
    let (anchor, head) = (buffer.anchor, buffer.caret);
    let Some(doc) = document(world, entity) else {
        return;
    };
    if display_selection(doc).ok() == Some((anchor, head)) {
        return;
    }
    let _ = set_display_selection(world, entity, binding.revision, anchor, head);
}
/// Route only explicit, freshly shaped structural hit targets. Gutter clicks
/// never become synthetic display bytes or retarget the saved text selection.
pub(crate) fn place_from_pointer(world: &mut World) -> Option<(Entity, bool)> {
    use crate::structured_chrome::{structured_marks, StructuredChromeTarget, StructuredMarkKind};
    let pointer = world.resource::<Input>()?.pointer;
    if !pointer.pressed
        || pointer.button != PointerButton::Left
        || crate::events::<crate::window::WindowFocus>(world)
            .contains(&crate::window::WindowFocus::Blurred)
    {
        return None;
    }
    let [x, y] = pointer.press_at.unwrap_or(pointer.position);
    if !x.is_finite() || !y.is_finite() {
        return None;
    }
    let hit = hit_test(world, x, y)?;
    let entity = if let Some(target) = world.get::<StructuredChromeTarget>(hit) {
        let entity = target.0;
        // A stale/reparented gutter cannot act on another retained surface.
        if world.get::<Parent>(hit).map(|p| p.entity)
            != world.get::<Parent>(entity).map(|p| p.entity)
            || !world.has::<Parent>(entity)
        {
            return None;
        }
        entity
    } else {
        ancestor_with::<Editor>(world, hit)?
    };
    if !crate::interaction::policy(world, entity).0 || is_composing(world, entity) {
        return None;
    }
    let mark = structured_marks(world, entity)
        .into_iter()
        .rev()
        .find(|m| {
            matches!(
                m.kind,
                StructuredMarkKind::Task(_)
                    | StructuredMarkKind::HorizontalRule
                    | StructuredMarkKind::Atomic
                    | StructuredMarkKind::TableCell { .. }
            ) && m.rect.contains(x, y)
                && m.clip.contains(x, y)
                && (!matches!(m.kind, StructuredMarkKind::TableCell { .. })
                    || x < m.rect.x + 5.0
                    || y < m.rect.y + 5.0
                    || x >= m.rect.x + m.rect.w - 5.0
                    || y >= m.rect.y + m.rect.h - 5.0)
        })?;
    if !crate::interaction::request_focus(world, entity) {
        return None;
    }
    let readonly = crate::interaction::readonly(world, entity);
    let shift = world.resource::<Input>().is_some_and(|i| i.shift);
    let result = (|| -> Result<(), StructuredViewError> {
        let doc = document_mut(world, entity).ok_or(StructuredViewError::Unavailable)?;
        if doc.revision() != mark.revision {
            return Err(StructuredViewError::Unavailable);
        }
        match mark.kind {
            StructuredMarkKind::Task(_) => {
                if readonly {
                    return Err(StructuredError::Readonly.into());
                }
                doc.apply(Transaction {
                    document: doc.id(),
                    base: mark.revision,
                    origin: EditOrigin::Host(6),
                    group: None,
                    operations: vec![Operation::ToggleTask { block: mark.block }],
                })?;
            }
            StructuredMarkKind::HorizontalRule | StructuredMarkKind::Atomic => {
                doc.set_selection(StructuredSelection::Node(mark.block))?
            }
            StructuredMarkKind::TableCell { .. } => {
                let address = doc
                    .cell_address(mark.block)?
                    .ok_or(StructuredError::InvalidPosition)?;
                let anchor = if shift {
                    match doc.selection() {
                        StructuredSelection::Cells { anchor, .. } => anchor,
                        StructuredSelection::Text(s) => doc
                            .cell_address(s.anchor.block)?
                            .map_or(address.cell, |a| a.cell),
                        _ => address.cell,
                    }
                } else {
                    address.cell
                };
                doc.set_selection(StructuredSelection::Cells {
                    anchor,
                    head: address.cell,
                })?;
            }
            _ => unreachable!(),
        }
        Ok(())
    })();
    if let Err(error) = result {
        world.insert(entity, error);
    }
    project(world, entity);
    Some((entity, mark.kind == StructuredMarkKind::HorizontalRule))
}
/// Structural Tab belongs to the tree interpreter, not global focus traversal.
/// Inspect without mutating: the canonical editor stage executes the command.
pub(crate) fn reserves_key(world: &World, event: &KeyEvent) -> bool {
    if event.logical != LogicalKey::Named(NamedKey::Tab)
        || event.modifiers.ctrl
        || event.modifiers.super_key
        || event.modifiers.alt
    {
        return false;
    }
    let Some(entity) = world.resource::<Focus>().and_then(|f| f.entity) else {
        return false;
    };
    let Some(doc) = document(world, entity) else {
        return false;
    };
    if is_composing(world, entity)
        || matches!(
            doc.selection(),
            StructuredSelection::Node(_) | StructuredSelection::Cells { .. }
        )
    {
        return true;
    }
    if doc
        .text_selection()
        .is_some_and(|s| doc.cell_address(s.head.block).ok().flatten().is_some())
    {
        return true;
    }
    doc.text_selection().is_some_and(|selection| {
        doc.ancestors(selection.head.block).is_ok_and(|ancestors| {
            ancestors
                .iter()
                .any(|c| matches!(c.kind, crate::structured::ContainerKind::Item { .. }))
        })
    })
}
/// Capture/cancel native preedit against a typed selection before any key input.
pub(crate) fn prepare_input(world: &mut World) {
    let focus = world.resource::<Focus>().and_then(|f| f.entity);
    let (preedit, commit) = world
        .resource::<Input>()
        .map(|i| (!i.ime_preedit.is_empty(), !i.ime_commit.is_empty()))
        .unwrap_or_default();
    let blurred = crate::events::<crate::window::WindowFocus>(world)
        .contains(&crate::window::WindowFocus::Blurred);
    let views = world.entities_with::<StructuredBinding>();
    for entity in views {
        let active = focus == Some(entity) && !blurred;
        let captured = world.has::<ImeCapture>(entity);
        let mut begun = None;
        if let Some(doc) = document_mut(world, entity) {
            if blurred || (!preedit && !commit) {
                if doc.is_composing() {
                    doc.cancel_composition();
                }
            } else if active
                && preedit
                && !doc.is_composing()
                && !captured
                && doc.begin_composition().is_ok()
            {
                begun = Some(doc.revision());
            }
        }
        if blurred || (!preedit && !commit) {
            world.remove::<ImeCapture>(entity);
        }
        if let Some(revision) = begun {
            world.insert(entity, ImeCapture { revision });
        }
    }
    // Despawning an entity directly cannot keep a document permanently attached.
    let stale = world
        .resource::<StructuredDocuments>()
        .map(|docs| {
            docs.views
                .iter()
                .filter(|(_, e)| !world.has::<StructuredBinding>(**e))
                .map(|(id, _)| *id)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if let Some(docs) = world.resource_mut::<StructuredDocuments>() {
        for id in stale {
            docs.views.remove(&id);
            if let Some(doc) = docs.get_mut(id) {
                doc.cancel_composition();
            }
        }
    }
}
fn native_keys(frame: &crate::ime::TextFrame, platform: Platform) -> Vec<KeyEvent> {
    if !frame.keys.is_empty() {
        return frame.keys.clone();
    }
    let mut keys = Vec::new();
    let mut mods = Modifiers::new();
    mods.shift = frame.shift;
    let named = match frame.legacy_key {
        Some(Key::Up) => Some(NamedKey::ArrowUp),
        Some(Key::Down) => Some(NamedKey::ArrowDown),
        Some(Key::Left) => Some(NamedKey::ArrowLeft),
        Some(Key::Right) => Some(NamedKey::ArrowRight),
        Some(Key::Backspace) => Some(NamedKey::Backspace),
        Some(Key::Delete) => Some(NamedKey::Delete),
        Some(Key::Enter) => Some(NamedKey::Enter),
        Some(Key::Tab) => Some(NamedKey::Tab),
        _ => None,
    };
    if let Some(named) = named {
        keys.push(KeyEvent::named(named, mods));
    }
    if let Some(key @ (Key::Copy | Key::Cut | Key::Paste)) = frame.legacy_key {
        if platform == Platform::Mac {
            mods.super_key = true;
        } else {
            mods.ctrl = true;
        }
        keys.push(KeyEvent::character(
            match key {
                Key::Copy => 'c',
                Key::Cut => 'x',
                _ => 'v',
            },
            mods,
        ));
    }
    if !frame.chars.is_empty() {
        let mut event = KeyEvent::character(' ', Modifiers::new());
        event.logical = LogicalKey::Unidentified;
        event.text = Some(frame.chars.clone());
        keys.push(event);
    }
    keys
}
#[path = "structured_table_input.rs"]
mod table_input;
use table_input::apply_table_key;

fn apply_key(
    world: &mut World,
    entity: Entity,
    event: &KeyEvent,
    clip: &str,
) -> Result<crate::typing::StepResult, StructuredViewError> {
    use crate::typing::{apply_text_event, EditHistory, EditSurface};
    if event.released || event.synthetic || is_composing(world, entity) {
        return Ok(Default::default());
    }
    let platform = crate::keys::platform_of(world);
    let readonly = crate::interaction::readonly(world, entity)
        || document(world, entity).is_some_and(StructuredDocument::is_readonly);
    if let Some(action @ (EditAction::Undo | EditAction::Redo)) =
        crate::keys::standard_action(platform, event)
    {
        if readonly {
            return Err(StructuredError::Readonly.into());
        }
        let doc = document_mut(world, entity).ok_or(StructuredViewError::Unavailable)?;
        if action == EditAction::Redo {
            doc.redo()?;
        } else {
            doc.undo()?;
        }
        return Ok(Default::default());
    }
    if let Some(result) = apply_table_key(world, entity, event, clip)? {
        return Ok(result);
    }
    let doc = document(world, entity).ok_or(StructuredViewError::Unavailable)?;
    if let StructuredSelection::Node(block) = doc.selection() {
        let atomic = doc
            .blocks()
            .iter()
            .position(|b| b.id == block)
            .filter(|i| doc.blocks()[*i].is_atomic());
        if let Some(i) = atomic {
            let action = crate::keys::standard_action(platform, event);
            if action == Some(EditAction::Copy) {
                return Ok(crate::typing::StepResult {
                    copied: Some(if doc.blocks()[i].kind == BlockKind::HorizontalRule {
                        "---".into()
                    } else {
                        doc.blocks()[i].copy_fallback()
                    }),
                    clipboard_action: true,
                });
            }
            let forward = match event.logical {
                LogicalKey::Named(NamedKey::ArrowRight | NamedKey::ArrowDown | NamedKey::Enter) => {
                    Some(true)
                }
                LogicalKey::Named(NamedKey::Tab) => Some(!event.modifiers.shift),
                LogicalKey::Named(NamedKey::ArrowLeft | NamedKey::ArrowUp) => Some(false),
                _ => None,
            };
            if let Some(forward) = forward {
                let next = if forward {
                    doc.blocks()[i + 1..].iter().find(|b| !b.is_atomic())
                } else {
                    doc.blocks()[..i].iter().rev().find(|b| !b.is_atomic())
                }
                .or_else(|| doc.blocks().iter().find(|b| !b.is_atomic()))
                .ok_or(StructuredError::InvalidPosition)?;
                let p = Position {
                    block: next.id,
                    offset: if forward { 0 } else { next.len() },
                    affinity: Affinity::After,
                };
                document_mut(world, entity)
                    .ok_or(StructuredViewError::Unavailable)?
                    .set_selection(StructuredSelection::Text(TextSelection {
                        anchor: p,
                        head: p,
                    }))?;
                return Ok(Default::default());
            }
            // Plain typing/paste/IME may not silently turn a nontext rule into
            // an editable paragraph. Arrow/Enter/Tab explicitly leave the node.
            if !matches!(
                event.logical,
                LogicalKey::Named(NamedKey::Backspace | NamedKey::Delete)
            ) && action != Some(EditAction::Cut)
            {
                return Ok(Default::default());
            }
        }
        let action = crate::keys::standard_action(platform, event);
        if matches!(
            event.logical,
            LogicalKey::Named(NamedKey::Backspace | NamedKey::Delete)
        ) || action == Some(EditAction::Cut)
        {
            if readonly {
                return Err(StructuredError::Readonly.into());
            }
            let copied = if action == Some(EditAction::Cut) {
                doc.blocks().iter().find(|b| b.id == block).map(|b| {
                    if b.kind == BlockKind::HorizontalRule {
                        "---".into()
                    } else {
                        b.copy_fallback()
                    }
                })
            } else {
                None
            };
            let doc = document_mut(world, entity).ok_or(StructuredViewError::Unavailable)?;
            doc.apply(Transaction {
                document: doc.id(),
                base: doc.revision(),
                origin: EditOrigin::Keyboard,
                group: None,
                operations: vec![Operation::DeleteNode { block }],
            })?;
            return Ok(crate::typing::StepResult {
                clipboard_action: copied.is_some(),
                copied,
            });
        }
    }
    let doc = document(world, entity).ok_or(StructuredViewError::Unavailable)?;
    if let Some(selection) = doc.text_selection() {
        let collapsed = selection.anchor.block == selection.head.block
            && selection.anchor.offset == selection.head.offset;
        let head = selection.head;
        let index = doc
            .blocks()
            .iter()
            .position(|b| b.id == head.block)
            .ok_or(StructuredError::InvalidPosition)?;
        let bare = !event.modifiers.ctrl && !event.modifiers.super_key && !event.modifiers.alt;
        if collapsed && bare {
            let neighbor = match event.logical {
                LogicalKey::Named(NamedKey::Backspace) if head.offset == 0 && index > 0 => {
                    Some(index - 1)
                }
                LogicalKey::Named(NamedKey::Delete)
                    if head.offset == doc.blocks()[index].len()
                        && index + 1 < doc.blocks().len() =>
                {
                    Some(index + 1)
                }
                _ => None,
            };
            if let Some(i) = neighbor.filter(|i| doc.blocks()[*i].is_atomic()) {
                let id = doc.blocks()[i].id;
                document_mut(world, entity)
                    .ok_or(StructuredViewError::Unavailable)?
                    .set_selection(StructuredSelection::Node(id))?;
                return Ok(Default::default());
            }
        }
        let structural = bare
            && (event.logical == LogicalKey::Named(NamedKey::Enter)
                || (collapsed
                    && head.offset == 0
                    && event.logical == LogicalKey::Named(NamedKey::Backspace))
                || (event.logical == LogicalKey::Named(NamedKey::Tab)
                    && doc
                        .ancestors(head.block)?
                        .iter()
                        .any(|c| matches!(c.kind, crate::structured::ContainerKind::Item { .. }))));
        if structural {
            if readonly {
                return Err(StructuredError::Readonly.into());
            }
            let doc = document_mut(world, entity).ok_or(StructuredViewError::Unavailable)?;
            match event.logical {
                LogicalKey::Named(NamedKey::Enter) => {
                    doc.enter(EditOrigin::Keyboard)?;
                }
                LogicalKey::Named(NamedKey::Backspace) => {
                    doc.backspace(EditOrigin::Keyboard)?;
                }
                LogicalKey::Named(NamedKey::Tab) => {
                    let operation = if event.modifiers.shift {
                        Operation::Outdent { block: head.block }
                    } else {
                        Operation::Indent { block: head.block }
                    };
                    doc.apply(Transaction {
                        document: doc.id(),
                        base: doc.revision(),
                        origin: EditOrigin::Keyboard,
                        group: None,
                        operations: vec![operation],
                    })?;
                }
                _ => unreachable!(),
            }
            return Ok(Default::default());
        }
    }
    let doc = document(world, entity).ok_or(StructuredViewError::Unavailable)?;
    let mut text = doc.plain_text();
    let (mut anchor, mut index) = display_selection(doc)?;
    let base = doc.revision();
    if let Some(delta) = crate::editor::vertical_delta(world, entity, &text, index, event) {
        (index, anchor) = crate::editor::move_vertical(
            world,
            entity,
            &text,
            index,
            anchor,
            delta,
            event.modifiers.shift,
        );
        set_display_selection(world, entity, base, anchor, index)?;
        return Ok(Default::default());
    }
    // Scratch interpreter only: its temporary string/history is discarded on
    // every key. Exactly one typed operation is published to the tree authority.
    let mut event = event.clone();
    event.text = event
        .text
        .as_deref()
        .map(crate::structured::normalize_plain_input);
    let clip = crate::structured::normalize_plain_input(clip);
    let event = &event;
    let clip = clip.as_str();
    let mut scratch = EditHistory::default();
    let step = apply_text_event(
        &mut EditSurface {
            text: &mut text,
            index: &mut index,
            anchor: &mut anchor,
        },
        event,
        platform,
        true,
        crate::typing::frame_now(world),
        &mut scratch,
        clip,
    );
    let change = scratch.take_native_change();
    let doc = document_mut(world, entity).ok_or(StructuredViewError::Unavailable)?;
    if let Some(change) = change {
        if readonly {
            return Err(StructuredError::Readonly.into());
        }
        let a = doc.position_at(change.range.start, Affinity::After)?;
        let h = doc.position_at(change.range.end, Affinity::After)?;
        doc.apply(Transaction {
            document: doc.id(),
            base,
            origin: EditOrigin::Keyboard,
            group: None,
            operations: vec![Operation::Replace {
                range: TextSelection { anchor: a, head: h },
                text: change.replacement,
            }],
        })?;
    }
    let selection = display_to_selection(doc, anchor, index)?;
    if doc.selection() != selection {
        doc.set_selection(selection)?;
    }
    Ok(step)
}
pub(crate) fn edit_focused(world: &mut World, entity: Entity) {
    let Some(input) = world.resource::<Input>() else {
        return;
    };
    let frame = crate::ime::text_frame(input);
    let platform = crate::keys::platform_of(world);
    let keys = native_keys(&frame, platform);
    let mut clip = world
        .resource::<Clipboard>()
        .map(|c| c.text.clone())
        .unwrap_or_default();
    let mut copied = None;
    let mut clipboard_action = false;
    for event in keys {
        match apply_key(world, entity, &event, &clip) {
            Ok(step) => {
                if let Some(text) = step.copied {
                    clip = text.clone();
                    copied = Some(text);
                }
                clipboard_action |= step.clipboard_action;
            }
            Err(error) => {
                world.insert(entity, error);
            }
        }
        project(world, entity);
    }
    if !frame.commit.is_empty() {
        let readonly = crate::interaction::readonly(world, entity);
        let capture = world.get::<ImeCapture>(entity).copied();
        if let Some(doc) = document_mut(world, entity) {
            let result =
                if capture.is_some_and(|c| c.revision != doc.revision() || !doc.is_composing()) {
                    Err(StructuredError::StaleRevision)
                } else if readonly {
                    Err(StructuredError::Readonly)
                } else if doc.is_composing() {
                    doc.commit_composition(&frame.commit)
                } else {
                    doc.replace_selection(&frame.commit, EditOrigin::Ime)
                };
            if let Err(error) = result {
                world.insert(entity, StructuredViewError::Model(error));
            }
        }
    }
    if !frame.commit.is_empty() {
        world.remove::<ImeCapture>(entity);
    }
    project(world, entity);
    if clipboard_action {
        crate::clipboard::note_clipboard(world, copied.as_deref());
    }
    if let Some(input) = world.resource_mut::<Input>() {
        input.keys.clear();
        input.chars.clear();
        input.ime_commit.clear();
        input.key = None;
    }
}

#[cfg(test)]
#[path = "structured_native_tests.rs"]
mod tests;
