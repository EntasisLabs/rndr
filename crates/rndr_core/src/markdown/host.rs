//! Explicit, programmatic host content. Markdown never executes or requests UI.
use super::*;

/// Caller-selected identity, scoped to one markdown view. Never reuse an ID
/// within that view's lifetime, even after removal (prevents stale-handle ABA).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MarkdownSlotId(pub u64);

/// Failure without partial mutation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkdownSlotError {
    /// View/block has not synced, source is pending, or expected block changed.
    StaleBlock,
    /// ID was used before, or this block already has a host.
    Duplicate,
    /// Removed/dead/replaced handle or subtree.
    StaleHandle,
    /// Height must be finite and positive.
    InvalidHeight,
}

/// Retained root owned by the markdown view. Add real child entities with
/// `set_parent`, or mount a semantic UI component under `root`. The host owns
/// descendant content/updates, not the root's layout or parent. All descendants
/// die on detach, block replacement/removal, or view despawn. No callbacks run.
/// Copying a handle does not extend lifetime; methods validate it each time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MarkdownSlot {
    /// View that owns this slot.
    pub view: Entity,
    /// Stable caller identity.
    pub id: MarkdownSlotId,
    /// Container, never a raster/text proxy. Do not reparent it.
    pub root: Entity,
}

pub(super) struct Binding {
    pub index: usize,
    pub root: Entity,
    id: MarkdownSlotId,
    block: MarkdownBlock,
    pub height: f32,
}
struct HostRoot;
impl Component for HostRoot {}

// Ignore theme-derived spans: a theme update is not document replacement.
fn same(a: &MarkdownBlock, b: &MarkdownBlock) -> bool {
    a.kind == b.kind
        && a.depth == b.depth
        && a.quote == b.quote
        && a.tight == b.tight
        && a.text == b.text
        && a.language == b.language
        && a.links == b.links
        && a.spans.len() == b.spans.len()
        && a.spans.iter().zip(&b.spans).all(|(a, b)| {
            a.start == b.start
                && a.end == b.end
                && a.face == b.face
                && a.weight == b.weight
                && a.italic == b.italic
                && a.family == b.family
                && a.underline == b.underline
                && a.highlight.is_some() == b.highlight.is_some()
        })
}

/// Replace the complete visual body of one last-synced block with a host root.
/// `expected` must match `MarkdownView::blocks()[index]`; pending source edits
/// are rejected. Binding is positional and content-checked, not a fuzzy text
/// search: insertion/replacement at that index retires it on the next sync.
/// Append/reparse and theme/width changes preserve its root and descendants
/// while that block's semantic content remains unchanged. Identical blocks at
/// distinct indexes are independent. No special markdown syntax is interpreted.
///
/// `height` reserves logical points; hosts must update it for their content.
/// Width follows the reading column. The ordinary view scroll clip applies.
/// Source/selection/copy remain markdown, not embedded control values. Pointer
/// selection and markdown copy shortcuts yield to focused embedded controls.
/// Only mount trusted application content; untrusted markdown cannot mount UI.
pub fn mount_markdown_slot(
    world: &mut World,
    view: Entity,
    index: usize,
    expected: &MarkdownBlock,
    id: MarkdownSlotId,
    height: f32,
) -> Result<MarkdownSlot, MarkdownSlotError> {
    if !height.is_finite() || height <= 0.0 {
        return Err(MarkdownSlotError::InvalidHeight);
    }
    let doc = world
        .get::<MarkdownView>(view)
        .ok_or(MarkdownSlotError::StaleBlock)?;
    if doc.dirty
        || !doc
            .blocks
            .get(index)
            .is_some_and(|block| same(block, expected))
    {
        return Err(MarkdownSlotError::StaleBlock);
    }
    if doc.used_host_ids.contains(&id) || doc.hosts.iter().any(|binding| binding.index == index) {
        return Err(MarkdownSlotError::Duplicate);
    }
    let root = world.spawn((
        Layout {
            width: Size::Fill,
            height: Size::Px(height),
            ..Layout::column()
        },
        HostRoot,
    ));
    set_parent(world, root, view);
    let doc = world.get_mut::<MarkdownView>(view).expect("validated view");
    doc.hosts.push(Binding {
        index,
        root,
        id,
        block: expected.clone(),
        height,
    });
    doc.used_host_ids.insert(id);
    doc.hosts_dirty = true;
    Ok(MarkdownSlot { view, id, root })
}

impl MarkdownSlot {
    /// Check whether the retained handle is still mounted. Pending source edits
    /// conservatively make it unavailable until sync has checked the binding.
    pub fn is_mounted(self, world: &World) -> bool {
        world.get::<MarkdownView>(self.view).is_some_and(|doc| {
            !doc.dirty
                && doc
                    .hosts
                    .iter()
                    .any(|b| b.id == self.id && b.root == self.root && valid(world, doc, b))
        })
    }
    /// Reserve a new content height without replacing descendants/state.
    pub fn set_height(self, world: &mut World, height: f32) -> Result<(), MarkdownSlotError> {
        if !height.is_finite() || height <= 0.0 {
            return Err(MarkdownSlotError::InvalidHeight);
        }
        if !self.is_mounted(world) {
            return Err(MarkdownSlotError::StaleHandle);
        }
        let doc = world.get_mut::<MarkdownView>(self.view).unwrap();
        doc.hosts
            .iter_mut()
            .find(|b| b.id == self.id)
            .unwrap()
            .height = height;
        doc.hosts_dirty = true;
        Ok(())
    }
    /// Destroy this subtree now; ordinary markdown returns on the next sync.
    /// Host-owned application records/resources are not modified or destroyed.
    pub fn detach(self, world: &mut World) -> Result<(), MarkdownSlotError> {
        if !world.is_alive(self.root) {
            return Err(MarkdownSlotError::StaleHandle);
        }
        let doc = world
            .get_mut::<MarkdownView>(self.view)
            .ok_or(MarkdownSlotError::StaleHandle)?;
        let index = doc
            .hosts
            .iter()
            .position(|b| b.id == self.id && b.root == self.root)
            .ok_or(MarkdownSlotError::StaleHandle)?;
        doc.hosts.remove(index);
        doc.hosts_dirty = true;
        despawn(world, self.root);
        Ok(())
    }
}
fn valid(world: &World, doc: &MarkdownView, binding: &Binding) -> bool {
    world.is_alive(binding.root)
        && doc
            .blocks
            .get(binding.index)
            .is_some_and(|block| same(block, &binding.block))
}
pub(super) fn stale(world: &World, doc: &MarkdownView) -> bool {
    doc.hosts.iter().any(|b| !valid(world, doc, b))
}
pub(super) fn retire_stale(world: &mut World, doc: &mut MarkdownView) {
    let mut keep = Vec::new();
    for binding in std::mem::take(&mut doc.hosts) {
        if valid(world, doc, &binding) {
            keep.push(binding);
        } else {
            despawn(world, binding.root);
        }
    }
    doc.hosts = keep;
}
pub(super) fn contains(world: &World, mut entity: Entity) -> bool {
    // Follow arbitrary valid host depth, stopping on malformed hierarchy cycles.
    let mut visited = std::collections::HashSet::new();
    while visited.insert(entity) {
        if world.has::<HostRoot>(entity) {
            return true;
        }
        let Some(parent) = world.get::<crate::Parent>(entity) else {
            break;
        };
        entity = parent.entity;
    }
    false
}
