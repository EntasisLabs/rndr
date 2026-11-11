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
/// descendant content/updates, not the root's layout or parent. Descendants
/// still attached at removal die on detach, block replacement/removal, or view
/// despawn; explicitly reparented descendants follow their new owner.
/// [`MarkdownView::set_recycle_host_slots`] is the only path that releases a
/// root without [`MarkdownSlot::detach`], and it tells [`MarkdownSlotHost`]
/// first. `detach` itself does not.
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
    /// Source byte range after the last sync. `usize::MAX` means the anchor was lost.
    start: usize,
    end: usize,
    /// When true, [`measure_hosts`] replaces [`Self::height`] from the subtree.
    pub(super) measured: bool,
    /// The root was released because the block left the window. The id and
    /// measured height stay so the same slot can return.
    pub(super) released: bool,
    /// Skip the next measurement. A slot that just came back builds its
    /// subtree during this sync, so the laid-out height is still the empty one.
    pub(super) defer_measure: bool,
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
/// are rejected. The slot's identity is the block's source byte range. An
/// insert above that range, or a streaming append after it, moves the range
/// with the edit and the slot follows that block. An edit that deletes the
/// range retires the slot. Theme and width changes keep the root. Identical
/// blocks at distinct ranges stay independent. No special markdown syntax is
/// interpreted.
///
/// Height is measured from the mounted subtree after layout. Until that
/// measurement exists the root hugs its children. [`MarkdownSlot::set_height`]
/// opts out and reserves a constant. Width follows the reading column. The
/// ordinary view scroll clip applies.
/// Source/selection/copy remain markdown, not embedded control values. Pointer
/// selection and markdown copy shortcuts yield to focused embedded controls.
/// Only mount trusted application content; untrusted markdown cannot mount UI.
pub fn mount_markdown_slot(
    world: &mut World,
    view: Entity,
    index: usize,
    expected: &MarkdownBlock,
    id: MarkdownSlotId,
) -> Result<MarkdownSlot, MarkdownSlotError> {
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
    let (start, end) = {
        let doc = world
            .get::<MarkdownView>(view)
            .ok_or(MarkdownSlotError::StaleBlock)?;
        let start = *doc.starts.get(index).ok_or(MarkdownSlotError::StaleBlock)?;
        let end = *doc.ends.get(index).ok_or(MarkdownSlotError::StaleBlock)?;
        if end < start {
            return Err(MarkdownSlotError::StaleBlock);
        }
        (start, end)
    };
    let root = world.spawn((
        Layout {
            width: Size::Fill,
            height: Size::Hug,
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
        height: 0.0,
        start,
        end,
        measured: true,
        released: false,
        defer_measure: false,
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
                && doc.hosts.iter().any(|b| {
                    b.id == self.id && b.root == self.root && !b.released && valid(world, doc, b)
                })
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
        let binding = doc.hosts.iter_mut().find(|b| b.id == self.id).unwrap();
        binding.height = height;
        binding.measured = false;
        doc.hosts_dirty = true;
        Ok(())
    }
    /// Destroy this subtree now; ordinary markdown returns on the next sync.
    /// Host-owned application records/resources are not modified or destroyed.
    pub fn detach(self, world: &mut World) -> Result<(), MarkdownSlotError> {
        let (index, released) = {
            let doc = world
                .get::<MarkdownView>(self.view)
                .ok_or(MarkdownSlotError::StaleHandle)?;
            let index = doc
                .hosts
                .iter()
                .position(|b| b.id == self.id && b.root == self.root)
                .ok_or(MarkdownSlotError::StaleHandle)?;
            (index, doc.hosts[index].released)
        };
        if !released && !world.is_alive(self.root) {
            return Err(MarkdownSlotError::StaleHandle);
        }
        let doc = world.get_mut::<MarkdownView>(self.view).unwrap();
        doc.hosts.remove(index);
        doc.hosts_dirty = true;
        despawn(world, self.root);
        Ok(())
    }
}

/// Told when a recycled host slot leaves the window and when it returns.
///
/// The [`MarkdownSlotId`] is the same one the host passed to
/// [`mount_markdown_slot`]. The root is new on the way back in; descendants
/// are gone and the host rebuilds them. `source` is the block's source byte
/// start, so state can be stored by position rather than by a live entity.
pub trait MarkdownSlotHost: Send + Sync {
    /// The slot's block entered the visible window plus overscan.
    fn slot_mounted(&mut self, world: &mut World, slot: MarkdownSlot, index: usize, source: usize);
    /// The slot's block left that window. The root is still alive for this call.
    fn slot_unmounted(
        &mut self,
        world: &mut World,
        slot: MarkdownSlot,
        index: usize,
        source: usize,
    );
}

/// Per-view [`MarkdownSlotHost`] values. Installed by [`set_markdown_slot_host`].
#[derive(Default)]
pub struct MarkdownSlotHosts {
    hosts: std::collections::HashMap<Entity, Box<dyn MarkdownSlotHost>>,
    // Only active callback frames live here. A frame guard removes its flag even
    // on unwind; no per-view tombstones or monotonically growing ID table.
    dispatch: std::sync::Arc<
        std::sync::Mutex<
            std::collections::HashMap<Entity, Vec<std::sync::Arc<std::sync::atomic::AtomicBool>>>,
        >,
    >,
}

impl MarkdownSlotHosts {
    /// Remove this view's registration, without touching any other view.
    ///
    /// Also invalidates callbacks temporarily extracted by dispatch. Such a
    /// callback is dropped after it returns, never restored. Drop the returned
    /// value outside the World resource borrow; captures may run destructors.
    pub fn remove(&mut self, view: Entity) -> Option<Box<dyn MarkdownSlotHost>> {
        if let Some(frames) = self.dispatch.lock().unwrap().get(&view) {
            for frame in frames {
                frame.store(false, std::sync::atomic::Ordering::Release);
            }
        }
        self.hosts.remove(&view)
    }
}

type DispatchFrames = std::sync::Arc<
    std::sync::Mutex<
        std::collections::HashMap<Entity, Vec<std::sync::Arc<std::sync::atomic::AtomicBool>>>,
    >,
>;
struct DispatchFrame {
    frames: DispatchFrames,
    view: Entity,
    live: std::sync::Arc<std::sync::atomic::AtomicBool>,
}
impl Drop for DispatchFrame {
    fn drop(&mut self) {
        let mut frames = self.frames.lock().unwrap();
        if let Some(stack) = frames.get_mut(&self.view) {
            stack.retain(|flag| !std::sync::Arc::ptr_eq(flag, &self.live));
            if stack.is_empty() {
                frames.remove(&self.view);
            }
        }
    }
}

/// Remove every host binding on `view`, including slots released offscreen.
pub fn clear_markdown_slots(world: &mut World, view: Entity) {
    let hosts = {
        let Some(doc) = world.get_mut::<MarkdownView>(view) else {
            return;
        };
        doc.hosts_dirty = true;
        std::mem::take(&mut doc.hosts)
    };
    for binding in hosts {
        despawn(world, binding.root);
    }
}

/// Register `host` for `view`. A later call replaces the previous host.
pub fn set_markdown_slot_host(
    world: &mut World,
    view: Entity,
    host: impl MarkdownSlotHost + 'static,
) {
    if world.resource_mut::<MarkdownSlotHosts>().is_none() {
        world.insert_resource(MarkdownSlotHosts::default());
    }
    let old = {
        let hosts = world.resource_mut::<MarkdownSlotHosts>().unwrap();
        let old = hosts.remove(view);
        hosts.hosts.insert(view, Box::new(host));
        old
    };
    drop(old);
}

fn notify(
    world: &mut World,
    view: Entity,
    slot: MarkdownSlot,
    index: usize,
    source: usize,
    mount: bool,
) {
    let require_markdown = world.has::<MarkdownView>(view);
    let Some((mut host, frame)) = world.resource_mut::<MarkdownSlotHosts>().and_then(|hosts| {
        let host = hosts.hosts.remove(&view)?;
        let live = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        hosts
            .dispatch
            .lock()
            .unwrap()
            .entry(view)
            .or_default()
            .push(live.clone());
        Some((
            host,
            DispatchFrame {
                frames: hosts.dispatch.clone(),
                view,
                live,
            },
        ))
    }) else {
        return;
    };
    // Markdown sync legitimately extracts MarkdownView while invoking us. Do
    // not mistake that transaction for component disposal. Explicit remove
    // invalidates this frame; raw/cascade despawn is additionally checked here.
    if mount {
        host.slot_mounted(world, slot, index, source);
    } else {
        host.slot_unmounted(world, slot, index, source);
    }
    if frame.live.load(std::sync::atomic::Ordering::Acquire)
        && world.is_alive(view)
        && (!require_markdown || world.has::<MarkdownView>(view))
    {
        if let Some(hosts) = world.resource_mut::<MarkdownSlotHosts>() {
            // A replacement wins, including one installed by nested dispatch.
            if let std::collections::hash_map::Entry::Vacant(entry) = hosts.hosts.entry(view) {
                entry.insert(host);
                return;
            }
        }
    }
    // Fail closed on unwind: neither the host nor frame is restored.
    drop(host);
}
fn anchored(doc: &MarkdownView, binding: &Binding) -> bool {
    binding.start != usize::MAX
        && doc.starts.get(binding.index) == Some(&binding.start)
        && doc.ends.get(binding.index) == Some(&binding.end)
        && doc
            .blocks
            .get(binding.index)
            .is_some_and(|block| same(block, &binding.block))
}

fn valid(world: &World, doc: &MarkdownView, binding: &Binding) -> bool {
    if !anchored(doc, binding) {
        return false;
    }
    binding.released || world.is_alive(binding.root)
}

/// Move anchors through the edit since the last sync, before the new parse.
pub(super) fn shift_anchors(doc: &mut MarkdownView) {
    let (at, removed, inserted) = span_edit(&doc.synced_source, &doc.source);
    for binding in &mut doc.hosts {
        match remap(binding.start, binding.end, at, removed, inserted) {
            Some((start, end)) => {
                binding.start = start;
                binding.end = end;
            }
            None => binding.start = usize::MAX,
        }
    }
}

/// Point each shifted anchor at the block that now owns that source range.
pub(super) fn rebind(doc: &mut MarkdownView) {
    for binding in &mut doc.hosts {
        if binding.start == usize::MAX {
            continue;
        }
        let exact = doc.starts.iter().enumerate().find(|(index, start)| {
            **start == binding.start && doc.ends.get(*index) == Some(&binding.end)
        });
        let contained = doc.starts.iter().enumerate().filter(|(index, start)| {
            **start <= binding.start && doc.ends.get(*index).is_some_and(|end| *end >= binding.end)
        });
        let index = exact
            .map(|(index, _)| index)
            .or_else(|| only(contained))
            .or_else(|| {
                only(
                    doc.starts
                        .iter()
                        .enumerate()
                        .filter(|(_, start)| **start == binding.start),
                )
            });
        if let Some(index) = index {
            binding.index = index;
            binding.start = doc.starts[index];
            binding.end = doc.ends[index];
            binding.block = doc.blocks[index].clone();
        } else {
            binding.start = usize::MAX;
        }
    }
}

/// Replace measured slot heights from the previous layout of their children.
pub(super) fn measure_hosts(world: &World, doc: &mut MarkdownView) {
    for binding in &mut doc.hosts {
        if binding.released {
            continue;
        }
        if binding.defer_measure {
            binding.defer_measure = false;
            continue;
        }
        if !binding.measured {
            continue;
        }
        let Some(height) = content_height(world, binding.root) else {
            continue;
        };
        if height != binding.height {
            binding.height = height;
            doc.hosts_dirty = true;
        }
    }
}

fn content_height(world: &World, root: Entity) -> Option<f32> {
    let children = crate::children_of(world, root);
    if children.is_empty() {
        return None;
    }
    let gap = world
        .get::<Layout>(root)
        .map(|layout| layout.gap)
        .unwrap_or(0.0);
    let mut height = 0.0;
    let mut count = 0usize;
    for child in children {
        let rect = world.get::<Rect>(child)?;
        if !rect.h.is_finite() || rect.h <= 0.0 {
            return None;
        }
        height += rect.h;
        count += 1;
    }
    if count > 1 {
        height += gap * (count - 1) as f32;
    }
    height.is_finite().then_some(height)
}

/// One span replacement: `(at, removed, inserted)` in bytes.
fn span_edit(old: &str, new: &str) -> (usize, usize, usize) {
    let prefix: usize = old
        .chars()
        .zip(new.chars())
        .take_while(|(a, b)| a == b)
        .map(|(ch, _)| ch.len_utf8())
        .sum();
    let old_tail = &old[prefix..];
    let new_tail = &new[prefix..];
    let suffix: usize = old_tail
        .chars()
        .rev()
        .zip(new_tail.chars().rev())
        .take_while(|(a, b)| a == b)
        .map(|(ch, _)| ch.len_utf8())
        .sum();
    let removed = old_tail.len() - suffix;
    let inserted = new_tail.len() - suffix;
    (prefix, removed, inserted)
}

fn remap(
    start: usize,
    end: usize,
    at: usize,
    removed: usize,
    inserted: usize,
) -> Option<(usize, usize)> {
    if start == usize::MAX || end < start {
        return None;
    }
    let delta = inserted as isize - removed as isize;
    if end <= at {
        return Some((start, end));
    }
    if start >= at.saturating_add(removed) {
        return Some((shift(start, delta)?, shift(end, delta)?));
    }
    if start <= at && end >= at.saturating_add(removed) {
        return Some((start, shift(end, delta)?));
    }
    None
}

fn only<'a>(mut found: impl Iterator<Item = (usize, &'a usize)>) -> Option<usize> {
    let first = found.next().map(|(index, _)| index);
    if found.next().is_some() {
        None
    } else {
        first
    }
}

fn shift(value: usize, delta: isize) -> Option<usize> {
    let next = value as isize + delta;
    (next >= 0).then_some(next as usize)
}

pub(super) fn recycle(
    world: &mut World,
    view: Entity,
    doc: &mut MarkdownView,
    range: std::ops::Range<usize>,
) {
    if !doc.recycle_hosts {
        return;
    }
    let release: Vec<_> = doc
        .hosts
        .iter()
        .filter(|binding| !binding.released && !range.contains(&binding.index))
        .map(|binding| binding.id)
        .collect();
    for id in release {
        release_one(world, view, doc, id);
    }
    let realize: Vec<_> = doc
        .hosts
        .iter()
        .filter(|binding| binding.released && range.contains(&binding.index))
        .map(|binding| binding.id)
        .collect();
    for id in realize {
        realize_one(world, view, doc, id);
    }
}

fn release_one(world: &mut World, view: Entity, doc: &mut MarkdownView, id: MarkdownSlotId) {
    let Some(binding) = doc.hosts.iter_mut().find(|binding| binding.id == id) else {
        return;
    };
    if binding.height <= 0.0 {
        if let Some(height) = content_height(world, binding.root) {
            binding.height = height;
        }
    }
    let index = binding.index;
    let source = binding.start;
    let root = binding.root;
    let height = binding.height;
    if height > 0.0 {
        if let Some(slot) = doc.heights.get_mut(index) {
            *slot = height + super::trail_after(&doc.blocks, index);
        }
        if let Some(measured) = doc.measured.get_mut(index) {
            *measured = true;
        }
    }
    let slot = MarkdownSlot { view, id, root };
    notify(world, view, slot, index, source, false);
    despawn(world, root);
    if let Some(binding) = doc.hosts.iter_mut().find(|binding| binding.id == id) {
        binding.released = true;
    }
}

fn realize_one(world: &mut World, view: Entity, doc: &mut MarkdownView, id: MarkdownSlotId) {
    let Some(binding) = doc.hosts.iter().find(|binding| binding.id == id) else {
        return;
    };
    let index = binding.index;
    let source = binding.start;
    let height = binding.height;
    let root = world.spawn((
        Layout {
            width: Size::Fill,
            height: if height > 0.0 {
                Size::Px(height)
            } else {
                Size::Hug
            },
            ..Layout::column()
        },
        HostRoot,
    ));
    set_parent(world, root, view);
    if let Some(binding) = doc.hosts.iter_mut().find(|binding| binding.id == id) {
        binding.root = root;
        binding.released = false;
        binding.defer_measure = true;
    }
    notify(
        world,
        view,
        MarkdownSlot { view, id, root },
        index,
        source,
        true,
    );
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

#[cfg(test)]
mod shift_tests {
    use super::{remap, span_edit};

    #[test]
    fn insert_above_is_a_prefix_insertion() {
        let edit = span_edit("Keep\n\nTail", "Intro\n\nKeep\n\nTail");
        assert_eq!(edit, (0, 0, "Intro\n\n".len()), "{edit:?}");
        assert_eq!(remap(0, 4, 0, 0, "Intro\n\n".len()), Some((7, 11)));
    }

    #[test]
    fn append_does_not_move_an_earlier_range() {
        assert_eq!(
            span_edit("Keep\n\nTail", "Keep\n\nTail\n\nMore"),
            (10, 0, 6)
        );
        assert_eq!(remap(0, 4, 10, 0, 6), Some((0, 4)));
    }
}

#[cfg(test)]
mod registration_tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    struct Probe {
        drops: Arc<AtomicUsize>,
        action: u8,
    }
    impl Drop for Probe {
        fn drop(&mut self) {
            self.drops.fetch_add(1, Ordering::SeqCst);
        }
    }
    impl MarkdownSlotHost for Probe {
        fn slot_mounted(&mut self, world: &mut World, slot: MarkdownSlot, _: usize, _: usize) {
            match self.action {
                1 => {
                    let old = world
                        .resource_mut::<MarkdownSlotHosts>()
                        .unwrap()
                        .remove(slot.view);
                    drop(old);
                }
                2 => {
                    set_markdown_slot_host(
                        world,
                        slot.view,
                        Probe {
                            drops: self.drops.clone(),
                            action: 0,
                        },
                    );
                    notify(world, slot.view, slot, 0, 0, true);
                }
                3 => {
                    world.despawn_entity(slot.view);
                }
                4 => panic!("callback failure"),
                5 => {
                    world.remove::<MarkdownView>(slot.view);
                }
                6 => {
                    crate::despawn(world, slot.view);
                }
                _ => {}
            }
        }
        fn slot_unmounted(&mut self, world: &mut World, slot: MarkdownSlot, i: usize, s: usize) {
            self.slot_mounted(world, slot, i, s);
        }
    }
    #[test]
    fn callback_removal_replacement_raw_loss_and_unwind_never_resurrect() {
        for action in 1..=6 {
            let mut world = World::new();
            let drops = Arc::new(AtomicUsize::new(0));
            let other = world.spawn(Layout::column());
            set_markdown_slot_host(
                &mut world,
                other,
                Probe {
                    drops: drops.clone(),
                    action: 0,
                },
            );
            for _ in 0..100 {
                let view = crate::markdown_view(&mut world, "");
                set_markdown_slot_host(
                    &mut world,
                    view,
                    Probe {
                        drops: drops.clone(),
                        action,
                    },
                );
                let slot = MarkdownSlot {
                    view,
                    root: view,
                    id: MarkdownSlotId(1),
                };
                let before = drops.load(Ordering::SeqCst);
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    notify(&mut world, view, slot, 0, 0, true)
                }));
                assert_eq!(result.is_err(), action == 4);
                let hosts = world.resource::<MarkdownSlotHosts>().unwrap();
                assert!(hosts.dispatch.lock().unwrap().is_empty());
                assert!(hosts.hosts.contains_key(&other));
                assert_eq!(hosts.hosts.contains_key(&view), action == 2);
                assert_eq!(drops.load(Ordering::SeqCst), before + 1);
                let old = world
                    .resource_mut::<MarkdownSlotHosts>()
                    .unwrap()
                    .remove(view);
                drop(old);
                if world.is_alive(view) {
                    world.despawn_entity(view);
                }
            }
            assert_eq!(
                world.resource::<MarkdownSlotHosts>().unwrap().hosts.len(),
                1
            );
        }
    }
}
