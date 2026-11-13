//! Logical ownership, independent of a recycled presentation entity.
use crate::view::LiquidView;
use rndr_core::{Component, Entity, MarkdownView, World};
use rndr_liquid::LiquidBlock;
use std::collections::{HashMap, HashSet};
use std::sync::{
    atomic::{AtomicU64, AtomicU8, Ordering},
    Arc,
};

static SERIAL: AtomicU64 = AtomicU64::new(1);
fn allocate(counter: &AtomicU64) -> Option<u64> {
    counter
        .try_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
        .ok()
}
fn serial() -> Option<u64> {
    allocate(&SERIAL)
}

/// Process-local identity of a live logical Liquid surface, not permission.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LiquidSurfaceHandle(pub(crate) u64);
/// A refused Liquid operation. No document payload is retained in this error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LiquidUpdateError {
    /// Surface is foreign, retired, or has lost its authority components.
    StaleOwner,
    /// A configured admission bound was reached.
    Capacity,
    /// A nonwrapping identity or revision counter was exhausted.
    Exhausted,
}

/// Positive finite renderer admission budgets. These bound logical live work,
/// not allocator capacity or externally uncontrolled legacy workers.
#[derive(Clone, Debug)]
pub struct LiquidLimits {
    /// Logical surfaces plus block leases per World (default 4096).
    pub scopes: usize,
    /// Concurrent feed binding lanes per block (default 32).
    pub lanes: usize,
    /// Owned interaction listeners per owner (default 256).
    pub listeners: usize,
    /// Queued interaction envelopes per World (default 1024).
    pub envelopes: usize,
    /// Listener and retirement callbacks per pump (default 256).
    pub callbacks: usize,
    /// Payload-free diagnostic ring capacity (default 512).
    pub diagnostics: usize,
}
impl Default for LiquidLimits {
    fn default() -> Self {
        Self {
            scopes: 4096,
            lanes: 32,
            listeners: 256,
            envelopes: 1024,
            callbacks: 256,
            diagnostics: 512,
        }
    }
}
impl LiquidLimits {
    pub(crate) fn valid(&self) -> bool {
        [
            self.scopes,
            self.lanes,
            self.listeners,
            self.envelopes,
            self.callbacks,
            self.diagnostics,
        ]
        .iter()
        .all(|n| *n > 0)
    }
}
pub(crate) fn limits(world: &World) -> LiquidLimits {
    world
        .resource::<LiquidLimits>()
        .cloned()
        .unwrap_or_default()
}

pub(crate) type Key = (u64, u64);
struct Surface {
    last_view: Entity,
    view: Option<Entity>,
    parent: Option<Key>,
    path: String,
    blocks: std::collections::BTreeMap<usize, (u64, LiquidBlock)>,
    sources: HashMap<usize, usize>,
    all_blocks: Vec<LiquidBlock>,
    order: Vec<usize>,
    keys: Vec<Option<String>>,
    revision: u64,
    transaction: Arc<AtomicU8>,
    projections: HashSet<Entity>,
    parked: Option<LiquidView>,
}
#[derive(Clone, Copy)]
enum Diagnostic {
    StaleOwner,
    Capacity,
    Exhausted,
    Panicked,
}
#[derive(Default)]
pub(crate) struct Owners {
    surfaces: HashMap<u64, Surface>,
    views: HashMap<Entity, u64>,
    refusals: std::collections::VecDeque<Diagnostic>,
    dropped_diagnostics: u64,
}
#[derive(Clone)]
struct Lease(u64);
impl Component for Lease {}

/// Resolve the logical surface while its authority components are live.
pub fn liquid_surface_handle(
    world: &World,
    view: Entity,
) -> Result<LiquidSurfaceHandle, LiquidUpdateError> {
    let id = world
        .resource::<Owners>()
        .and_then(|r| r.views.get(&view))
        .copied()
        .ok_or(LiquidUpdateError::StaleOwner)?;
    live(world, id)
        .then_some(LiquidSurfaceHandle(id))
        .ok_or(LiquidUpdateError::StaleOwner)
}
pub(crate) fn live(world: &World, id: u64) -> bool {
    let Some(surface) = world.resource::<Owners>().and_then(|r| r.surfaces.get(&id)) else {
        return false;
    };
    if surface.transaction.load(Ordering::Acquire) >= 2 {
        return false;
    }
    if let Some(parent) = surface.parent {
        if !block_live(world, parent) {
            return false;
        }
    }
    match surface.view {
        Some(view) => {
            world.is_alive(view)
                && world.get::<Lease>(view).is_some_and(|l| l.0 == id)
                && (surface.transaction.load(Ordering::Acquire) == 1
                    || (world.has::<LiquidView>(view) && world.has::<MarkdownView>(view)))
        }
        None => surface.parent.is_some(),
    }
}
pub(crate) fn block_live(world: &World, key: Key) -> bool {
    live(world, key.0)
        && world
            .resource::<Owners>()
            .and_then(|r| r.surfaces.get(&key.0))
            .is_some_and(|s| s.blocks.values().any(|(id, _)| *id == key.1))
}
pub(crate) fn key(world: &World, view: Entity, source: usize) -> Option<Key> {
    let id = liquid_surface_handle(world, view).ok()?.0;
    let s = world.resource::<Owners>()?.surfaces.get(&id)?;
    let index = s.sources.get(&source)?;
    Some((id, s.blocks.get(index)?.0))
}
pub(crate) fn key_at(world: &World, view: Entity, index: usize) -> Option<Key> {
    let id = world.resource::<Owners>()?.views.get(&view)?;
    Some((
        *id,
        world
            .resource::<Owners>()?
            .surfaces
            .get(id)?
            .blocks
            .get(&index)?
            .0,
    ))
}
pub(crate) fn logical_location(world: &World, key: Key) -> Option<(Option<Entity>, usize, usize)> {
    if !block_live(world, key) {
        return None;
    }
    let s = world.resource::<Owners>()?.surfaces.get(&key.0)?;
    let index = *s.blocks.iter().find(|(_, (id, _))| *id == key.1)?.0;
    let source = *s.sources.iter().find(|(_, i)| **i == index)?.0;
    Some((s.view, source, index))
}
pub(crate) fn envelope_location(world: &World, key: Key) -> Option<(Entity, usize, usize)> {
    let (_, source, index) = logical_location(world, key)?;
    let s = world.resource::<Owners>()?.surfaces.get(&key.0)?;
    Some((s.view.unwrap_or(s.last_view), source, index))
}
pub(crate) fn parsed(world: &World, key: Key) -> Option<&LiquidBlock> {
    if !block_live(world, key) {
        return None;
    }
    let s = world.resource::<Owners>()?.surfaces.get(&key.0)?;
    let index = *s.blocks.iter().find(|(_, (id, _))| *id == key.1)?.0;
    s.all_blocks.get(index)
}
pub(crate) fn location(world: &World, key: Key) -> Option<(Entity, usize, usize)> {
    let (view, source, index) = logical_location(world, key)?;
    Some((view?, source, index))
}
pub(crate) fn bind_sources(world: &mut World, view: Entity, sources: HashMap<usize, usize>) {
    if let Some(r) = world.resource_mut::<Owners>() {
        if let Some(s) = r.views.get(&view).and_then(|id| r.surfaces.get_mut(id)) {
            s.sources = sources;
        }
    }
}
pub(crate) fn register(
    world: &mut World,
    view: Entity,
    blocks: &[LiquidBlock],
    markdown: &str,
    parent: Option<Key>,
    path: String,
) -> Result<u64, LiquidUpdateError> {
    if !world.has_resource::<Owners>() {
        world.insert_resource(Owners::default());
    }
    if world
        .resource::<Owners>()
        .unwrap()
        .surfaces
        .values()
        .map(|s| 1 + s.blocks.len())
        .sum::<usize>()
        .checked_add(
            1 + rndr_liquid::segments(markdown)
                .iter()
                .filter(|s| matches!(s, rndr_liquid::Segment::Block(_)))
                .count(),
        )
        .is_none_or(|n| n > limits(world).scopes)
    {
        return Err(LiquidUpdateError::Capacity);
    }
    let order = crate::identity::indices(markdown);
    crate::identity::validate_projection(markdown, blocks)
        .map_err(|_| LiquidUpdateError::StaleOwner)?;
    let id = reserve_serials(order.len() as u64 + 1).ok_or(LiquidUpdateError::Exhausted)?;
    let blocks_input = blocks.to_vec();
    let blocks = order
        .iter()
        .enumerate()
        .map(|(offset, i)| (*i, (id + 1 + offset as u64, blocks[*i].clone())))
        .collect();
    let r = world.resource_mut::<Owners>().unwrap();
    r.surfaces.insert(
        id,
        Surface {
            view: Some(view),
            last_view: view,
            parent,
            path,
            blocks,
            sources: HashMap::new(),
            all_blocks: blocks_input,
            keys: vec![None; order.len()],
            order,
            revision: 0,
            transaction: Arc::new(AtomicU8::new(0)),
            projections: HashSet::new(),
            parked: None,
        },
    );
    r.views.insert(view, id);
    world.insert(view, Lease(id));
    Ok(id)
}
// Missing component in a supported internal extraction is not disposal. An
// unwound transaction is poisoned and retired at the next drain instead.
pub(crate) struct Transaction {
    flag: Arc<AtomicU8>,
    committed: bool,
}
impl Transaction {
    pub(crate) fn commit(mut self) {
        // A callback may poison the owner while publication is extracted.
        // Committing that frame must never revive terminal authority.
        let _ = self
            .flag
            .compare_exchange(1, 0, Ordering::AcqRel, Ordering::Acquire);
        self.committed = true;
    }
}
impl Drop for Transaction {
    fn drop(&mut self) {
        if !self.committed {
            let _ = self
                .flag
                .compare_exchange(1, 3, Ordering::AcqRel, Ordering::Acquire);
        }
    }
}
pub(crate) fn transaction(world: &World, view: Entity) -> Option<Transaction> {
    let r = world.resource::<Owners>()?;
    transaction_id(world, *r.views.get(&view)?)
}
fn transaction_id(world: &World, id: u64) -> Option<Transaction> {
    let s = world.resource::<Owners>()?.surfaces.get(&id)?;
    s.transaction
        .compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire)
        .ok()?;
    Some(Transaction {
        flag: s.transaction.clone(),
        committed: false,
    })
}
/// Preflight the complete provisional equality plan, then publish membership.
/// Saved and mounted blocks use precisely the same correspondence.
pub(crate) fn replace(
    world: &mut World,
    view: Entity,
    new: &[LiquidBlock],
    markdown: &str,
    append: bool,
) -> Result<(), LiquidUpdateError> {
    replace_detailed(world, view, new, markdown, append, None, true)
        .map(|_| ())
        .map_err(|e| match e {
            crate::LiquidF2Error::Capacity => LiquidUpdateError::Capacity,
            crate::LiquidF2Error::Exhausted => LiquidUpdateError::Exhausted,
            _ => LiquidUpdateError::StaleOwner,
        })
}
pub(crate) fn replace_detailed(
    world: &mut World,
    view: Entity,
    new: &[LiquidBlock],
    markdown: &str,
    append: bool,
    keys: Option<&crate::LiquidKeyMap>,
    source_changed: bool,
) -> Result<Vec<Option<usize>>, crate::LiquidF2Error> {
    use crate::LiquidF2Error as E;
    let count = world.resource::<Owners>().map_or(0, |r| {
        r.surfaces
            .values()
            .map(|s| 1 + s.blocks.len())
            .sum::<usize>()
    });
    let previous = world
        .resource::<Owners>()
        .and_then(|r| r.views.get(&view).and_then(|id| r.surfaces.get(id)))
        .map_or(0, |s| s.blocks.len());
    if count
        .saturating_sub(previous)
        .checked_add(
            rndr_liquid::segments(markdown)
                .iter()
                .filter(|s| matches!(s, rndr_liquid::Segment::Block(_)))
                .count(),
        )
        .is_none_or(|n| n > limits(world).scopes)
    {
        return Err(E::Capacity);
    }
    let r = world.resource::<Owners>().ok_or(E::StaleOwner)?;
    let id = *r.views.get(&view).ok_or(E::StaleOwner)?;
    let surface = &r.surfaces[&id];
    let old = &surface.blocks;
    let order = crate::identity::indices(markdown);
    let keys = keys.cloned().unwrap_or_else(|| {
        let mut keys = vec![None; order.len()];
        if append {
            for (k, prior) in keys.iter_mut().zip(&surface.keys) {
                *k = prior.clone();
            }
        }
        crate::LiquidKeyMap { keys }
    });
    crate::identity::validate(&keys, order.len())?;
    crate::identity::validate_projection(markdown, new)?;
    let changed = source_changed || keys.keys != surface.keys;
    let revision = if changed {
        surface.revision.checked_add(1).ok_or(E::Exhausted)?
    } else {
        surface.revision
    };
    let correspondence = crate::identity::correspondence(
        &surface.all_blocks,
        &surface.order,
        &surface.keys,
        new,
        &order,
        &keys.keys,
        append,
    );
    let mut used = HashSet::new();
    let mut resets = Vec::new();
    for (index, found) in order.iter().zip(&correspondence) {
        if let Some(prior) = found {
            if !crate::identity::compatible(&surface.all_blocks, *prior, new, *index) {
                let key = (id, old[prior].0);
                crate::state::preflight_reset(world, key)?;
                resets.push((key, *index, surface.all_blocks[*prior].clone()));
            }
        }
    }
    // Reserve the entire serial range in one checked atomic operation: a
    // refusal cannot consume a prefix of the requested IDs.
    let needed = correspondence.iter().filter(|i| i.is_none()).count() as u64;
    let mut next = reserve_serials(needed).ok_or(E::Exhausted)?;
    let mut plan = std::collections::BTreeMap::new();
    let mut carry = vec![None; new.len()];
    for ((index, found), _) in order.iter().zip(&correspondence).zip(&keys.keys) {
        let block = new.get(*index).ok_or(E::InvalidKeys)?;
        let block_id = if let Some(i) = found {
            used.insert(*i);
            if !resets.iter().any(|(_, n, _)| n == index) {
                carry[*index] = Some(*i);
            }
            old[i].0
        } else {
            let id = next;
            next += 1;
            id
        };
        plan.insert(*index, (block_id, block.clone()));
    }
    let retired: Vec<Key> = old
        .iter()
        .filter(|(i, _)| !used.contains(*i))
        .map(|(_, (b, _))| (id, *b))
        .collect();
    let surface = world
        .resource_mut::<Owners>()
        .unwrap()
        .surfaces
        .get_mut(&id)
        .unwrap();
    surface.blocks = plan;
    surface.order = order;
    surface.keys = keys.keys;
    surface.all_blocks = new.to_vec();
    surface.revision = revision;
    for (key, index, prior) in resets {
        retire_children(world, key);
        if matches!(
            &new[index],
            LiquidBlock::Feed(_) | LiquidBlock::Dashboard(_)
        ) {
            crate::feed::rebind(world, key, &prior, &new[index]);
        } else {
            crate::state::reset_record(world, key, &new[index]);
            crate::timer::reset_key(world, key);
            crate::feed::reset_key(world, key);
        }
    }
    for key in retired {
        retire_children(world, key);
    }
    crate::state::retain_live(world);
    crate::timer::retain_live(world);
    crate::feed::retire_dead(world);
    Ok(carry)
}

fn reserve_serials(count: u64) -> Option<u64> {
    reserve_from(&SERIAL, count)
}
fn reserve_from(counter: &AtomicU64, count: u64) -> Option<u64> {
    counter
        .try_update(Ordering::Relaxed, Ordering::Relaxed, |n| {
            n.checked_add(count)
        })
        .ok()
}
pub(crate) fn key_mutation_idle(world: &World, key: Key) -> bool {
    let Some(r) = world.resource::<Owners>() else {
        return false;
    };
    let mut id = key.0;
    loop {
        let Some(s) = r.surfaces.get(&id) else {
            return false;
        };
        if s.transaction.load(Ordering::Acquire) != 0 {
            return false;
        }
        if let Some(parent) = s.parent {
            id = parent.0;
        } else {
            return true;
        }
    }
}
pub(crate) fn mutation_idle(world: &World, view: Entity) -> bool {
    world
        .resource::<Owners>()
        .and_then(|r| r.views.get(&view))
        .is_some_and(|id| key_mutation_idle(world, (*id, 0)))
}
pub(crate) fn same_keys(world: &World, view: Entity, keys: Option<&crate::LiquidKeyMap>) -> bool {
    let Some(r) = world.resource::<Owners>() else {
        return false;
    };
    let Some(s) = r.views.get(&view).and_then(|id| r.surfaces.get(id)) else {
        return false;
    };
    keys.is_none_or(|k| k.keys == s.keys)
}
pub(crate) fn source_revision(
    world: &World,
    surface: LiquidSurfaceHandle,
) -> Result<crate::LiquidSourceRevision, crate::LiquidF2Error> {
    if !live(world, surface.0) {
        return Err(crate::LiquidF2Error::StaleOwner);
    }
    Ok(crate::LiquidSourceRevision(
        world.resource::<Owners>().unwrap().surfaces[&surface.0].revision,
    ))
}
pub(crate) fn component_handle(
    world: &World,
    surface: LiquidSurfaceHandle,
    occurrence: usize,
) -> Result<crate::LiquidBlockHandle, crate::LiquidF2Error> {
    source_revision(world, surface)?;
    let s = &world.resource::<Owners>().unwrap().surfaces[&surface.0];
    let index = s
        .order
        .get(occurrence)
        .ok_or(crate::LiquidF2Error::StaleBlock)?;
    Ok(crate::LiquidBlockHandle((surface.0, s.blocks[index].0)))
}
pub(crate) fn view_for(
    world: &World,
    surface: LiquidSurfaceHandle,
) -> Result<Entity, crate::LiquidF2Error> {
    source_revision(world, surface)?;
    world.resource::<Owners>().unwrap().surfaces[&surface.0]
        .view
        .ok_or(crate::LiquidF2Error::StaleOwner)
}
pub(crate) fn reset_block(
    world: &mut World,
    block: crate::LiquidBlockHandle,
    expected: u64,
) -> Result<crate::LiquidBlockHandle, crate::LiquidF2Error> {
    use crate::LiquidF2Error as E;
    crate::identity::check(world, block)?;
    if !key_mutation_idle(world, block.0) {
        return Err(E::StaleOwner);
    }
    let (view, _, index) = logical_location(world, block.0).ok_or(E::StaleBlock)?;
    let actual = crate::state::read_key(world, block.0).map_or(0, |r| r.revision);
    if actual != expected {
        return Err(E::Revision { expected, actual });
    }
    let rev = source_revision(world, LiquidSurfaceHandle(block.0 .0))?
        .0
        .checked_add(1)
        .ok_or(E::Exhausted)?;
    let tx = transaction_id(world, block.0 .0).ok_or(E::StaleOwner)?;
    let Some(id) = serial() else {
        tx.commit();
        return Err(E::Exhausted);
    };
    let s = world
        .resource_mut::<Owners>()
        .unwrap()
        .surfaces
        .get_mut(&block.0 .0)
        .unwrap();
    s.blocks.get_mut(&index).unwrap().0 = id;
    s.revision = rev;
    if let Some(parked) = s.parked.as_mut() {
        parked.forget(Some(index));
    }
    retire_children(world, block.0);
    crate::state::retain_live(world);
    crate::timer::retain_live(world);
    crate::feed::retire_dead(world);
    if let Some(view) = view {
        crate::view::discard_presentation(world, view, Some(index));
    }
    tx.commit();
    Ok(crate::LiquidBlockHandle((block.0 .0, id)))
}
pub(crate) fn reset_surface(
    world: &mut World,
    handle: LiquidSurfaceHandle,
    expected: crate::LiquidSourceRevision,
) -> Result<LiquidSurfaceHandle, crate::LiquidF2Error> {
    use crate::LiquidF2Error as E;
    let actual = source_revision(world, handle)?;
    if !key_mutation_idle(world, (handle.0, 0)) {
        return Err(E::StaleOwner);
    }
    if actual != expected {
        return Err(E::Revision {
            expected: expected.0,
            actual: actual.0,
        });
    }
    let view = world.resource::<Owners>().unwrap().surfaces[&handle.0].view;
    let count = world.resource::<Owners>().unwrap().surfaces[&handle.0]
        .blocks
        .len() as u64;
    let tx = transaction_id(world, handle.0).ok_or(E::StaleOwner)?;
    let Some(id) = reserve_serials(count + 1) else {
        tx.commit();
        return Err(E::Exhausted);
    };
    let old_keys: Vec<_> = world.resource::<Owners>().unwrap().surfaces[&handle.0]
        .blocks
        .values()
        .map(|(b, _)| (handle.0, *b))
        .collect();
    for key in old_keys {
        retire_children(world, key);
    }
    let r = world.resource_mut::<Owners>().unwrap();
    let mut s = r.surfaces.remove(&handle.0).unwrap();
    for (offset, (b, _)) in s.blocks.values_mut().enumerate() {
        *b = id + 1 + offset as u64;
    }
    s.revision = 0;
    if let Some(parked) = s.parked.as_mut() {
        parked.forget(None);
    }
    // Retain the active transaction fence across the incarnation swap.
    r.surfaces.insert(id, s);
    if let Some(view) = view {
        r.views.insert(view, id);
        world.insert(view, Lease(id));
    }
    crate::state::retain_live(world);
    crate::timer::retain_live(world);
    crate::feed::retire_dead(world);
    if let Some(view) = view {
        crate::view::discard_presentation(world, view, None);
    }
    tx.commit();
    Ok(LiquidSurfaceHandle(id))
}
#[derive(Clone, Copy)]
struct OwnedProjection;
impl Component for OwnedProjection {}
pub(crate) fn own_projection(world: &mut World, view: Entity, entity: Entity) {
    if world.is_alive(entity) {
        world.insert(entity, OwnedProjection);
    }
    if let Some(r) = world.resource_mut::<Owners>() {
        if let Some(s) = r.views.get(&view).and_then(|id| r.surfaces.get_mut(id)) {
            s.projections.insert(entity);
        }
    }
}
fn retire_children(world: &mut World, parent: Key) {
    let mut children: Vec<u64> = world
        .resource::<Owners>()
        .map(|r| {
            r.surfaces
                .iter()
                .filter(|(_, s)| s.parent == Some(parent))
                .map(|(id, _)| *id)
                .collect()
        })
        .unwrap_or_default();
    children.sort_unstable();
    for id in &children {
        mark_tree_dead(world, *id);
    }
    for id in children {
        retire_surface(world, id);
    }
}
fn mark_tree_dead(world: &World, id: u64) {
    let Some(r) = world.resource::<Owners>() else {
        return;
    };
    if let Some(s) = r.surfaces.get(&id) {
        s.transaction.store(2, Ordering::Release);
    }
    let mut children: Vec<_> = r
        .surfaces
        .iter()
        .filter(|(_, s)| s.parent.is_some_and(|p| p.0 == id))
        .map(|(id, _)| *id)
        .collect();
    children.sort_unstable();
    for child in children {
        mark_tree_dead(world, child);
    }
}
pub(crate) fn dispose_view(world: &mut World, view: Entity) {
    if let Some(id) = world
        .resource::<Owners>()
        .and_then(|r| r.views.get(&view))
        .copied()
    {
        mark_tree_dead(world, id);
        retire_surface(world, id);
    }
}
fn retire_surface(world: &mut World, id: u64) {
    mark_tree_dead(world, id);
    let blocks: Vec<Key> = world
        .resource::<Owners>()
        .and_then(|r| r.surfaces.get(&id))
        .map(|s| s.blocks.values().map(|(b, _)| (id, *b)).collect())
        .unwrap_or_default();
    for key in blocks {
        retire_children(world, key);
    }
    let removed = world
        .resource_mut::<Owners>()
        .and_then(|r| r.surfaces.remove(&id));
    let Some(s) = removed else {
        return;
    };
    if let Some(view) = s.view {
        world.resource_mut::<Owners>().unwrap().views.remove(&view);
        let host = world
            .resource_mut::<rndr_core::MarkdownSlotHosts>()
            .and_then(|h| h.remove(view));
        drop(host);
        world.remove::<LiquidView>(view);
        // Both are surface authority. Leaving MarkdownView on component-loss
        // would allow its next sync to recreate retired native projections.
        world.remove::<MarkdownView>(view);
        world.remove::<Lease>(view);
        remove_projections(
            world,
            rndr_core::MarkdownSlotHosts::native_projections(world, view),
        );
    }
    // Only entities actually constructed by Liquid are reclaimed. In particular
    // raw owner despawn must not cascade through unrelated application children.
    remove_projections(world, s.projections.into_iter().collect());
    drop(s.parked);
}
pub(crate) fn drain(world: &mut World) {
    let mut dead: Vec<u64> = world
        .resource::<Owners>()
        .map(|r| {
            r.surfaces
                .keys()
                .copied()
                .filter(|id| !live(world, *id))
                .collect()
        })
        .unwrap_or_default();
    dead.sort_unstable();
    for id in &dead {
        if world
            .resource::<Owners>()
            .and_then(|r| r.surfaces.get(id))
            .is_some_and(|s| s.transaction.load(Ordering::Acquire) == 3)
        {
            panicked(world);
        }
        mark_tree_dead(world, *id);
    }
    for id in dead {
        retire_surface(world, id);
    }
    // Rebuild/recycle may remove physical nodes many times within one logical
    // lifetime. Retain only live exact tags, never an Entity-id history.
    let live_projections: HashSet<_> = world
        .entities_with::<OwnedProjection>()
        .into_iter()
        .collect();
    if let Some(owners) = world.resource_mut::<Owners>() {
        for surface in owners.surfaces.values_mut() {
            surface
                .projections
                .retain(|entity| live_projections.contains(entity));
        }
    }
    crate::state::retain_live(world);
    crate::timer::retain_live(world);
    crate::feed::retire_dead(world);
    crate::state::drain_listeners(world);
    crate::chart::reclaim(world);
    let dead_projections: Vec<_> = world
        .resource::<Owners>()
        .map(|r| {
            r.surfaces
                .iter()
                .flat_map(|(id, s)| {
                    s.projections
                        .iter()
                        .filter(|e| !world.is_alive(**e))
                        .map(|e| (*id, *e))
                })
                .collect()
        })
        .unwrap_or_default();
    if let Some(r) = world.resource_mut::<Owners>() {
        for (id, e) in dead_projections {
            if let Some(s) = r.surfaces.get_mut(&id) {
                s.projections.remove(&e);
            }
        }
    }
}

pub(crate) fn ancestor_key(world: &World, mut entity: Entity) -> Option<Key> {
    let mut seen = HashSet::new();
    while seen.insert(entity) {
        if let Some(root) = world.get::<crate::view::BlockRoot>(entity) {
            return root.lease;
        }
        entity = world.get::<rndr_core::Parent>(entity)?.entity;
    }
    None
}

// Physical ancestry is presentation only. Follow registered parent keys to
// decide whether a surface belongs to the block being recycled.
fn belongs_under(world: &World, view: Entity, owner: Key) -> bool {
    let Some(r) = world.resource::<Owners>() else {
        return false;
    };
    let mut id = r.views.get(&view).copied();
    while let Some(surface) = id.and_then(|id| r.surfaces.get(&id)) {
        let Some(parent) = surface.parent else {
            return false;
        };
        if parent == owner {
            return true;
        }
        id = Some(parent.0);
    }
    false
}

/// Park logical descendants before their presentation subtree is destroyed.
pub(crate) fn park_under(world: &mut World, root: Entity) {
    let Some(owner) = ancestor_key(world, root) else {
        return;
    };
    // Select by registered semantic ancestry, never by physical traversal.
    // A child can have been moved behind an independent surface or entirely
    // outside this presentation tree. Park deepest first so each saved view
    // observes its own mounted state before exact projection reclamation.
    let mut views: Vec<_> = world
        .resource::<Owners>()
        .map(|r| r.views.keys().copied().collect())
        .unwrap_or_default();
    views.retain(|view| belongs_under(world, *view, owner));
    views.sort_by_key(|view| {
        let id = world.resource::<Owners>().unwrap().views[view];
        (
            std::cmp::Reverse(retirement_path(world, id).len()),
            std::cmp::Reverse(id),
        )
    });
    for view in views {
        let id = world
            .resource::<Owners>()
            .and_then(|r| r.views.get(&view))
            .copied();
        let Some(id) = id else {
            continue;
        };
        let Some(mut liquid) = world.remove::<LiquidView>(view) else {
            continue;
        };
        liquid.park(world);
        let host = world
            .resource_mut::<rndr_core::MarkdownSlotHosts>()
            .and_then(|r| r.remove(view));
        drop(host);
        if let Some(r) = world.resource_mut::<Owners>() {
            r.views.remove(&view);
            if let Some(s) = r.surfaces.get_mut(&id) {
                s.view = None;
                s.parked = Some(liquid);
                s.transaction.store(0, Ordering::Release);
            }
        }
        // Reclaim only this logical surface's exact presentation set, even
        // when relocated. Do not traverse or dispose independent attachments.
        world.remove::<MarkdownView>(view);
        let mut projections = rndr_core::MarkdownSlotHosts::native_projections(world, view);
        if let Some(surface) = world
            .resource_mut::<Owners>()
            .and_then(|r| r.surfaces.get_mut(&id))
        {
            projections.extend(surface.projections.drain());
        }
        projections.push(view);
        remove_projections(world, projections);
        world.remove::<Lease>(view);
    }
}
pub(crate) fn reattach(
    world: &mut World,
    view: Entity,
    parent: Key,
    path: &str,
) -> Option<LiquidView> {
    let id = world
        .resource::<Owners>()?
        .surfaces
        .iter()
        .find(|(_, s)| s.parent == Some(parent) && s.path == path && s.view.is_none())
        .map(|(id, _)| *id)?;
    let r = world.resource_mut::<Owners>()?;
    let s = r.surfaces.get_mut(&id)?;
    let liquid = s.parked.take()?;
    s.view = Some(view);
    s.last_view = view;
    r.views.insert(view, id);
    world.insert(view, Lease(id));
    Some(liquid)
}
/// Tag only a just-constructed, callback-free renderer/UI tree. Never call on
/// an existing subtree: application attachments are not renderer ownership.
pub(crate) fn own_created(world: &mut World, parent: Entity, root: Entity) {
    let Some((view, _, _)) = ancestor_key(world, parent).and_then(|k| location(world, k)) else {
        return;
    };
    let mut pending = vec![root];
    while let Some(entity) = pending.pop() {
        pending.extend(rndr_core::children_of(world, entity));
        own_projection(world, view, entity);
    }
}
/// Raw-remove an exact admitted ownership set. Surviving application children
/// become roots; neither ancestry nor a reparented application's nodes confer
/// ownership. Keep hierarchy links tidy without invoking cascade semantics.
fn remove_projections(world: &mut World, entities: Vec<Entity>) {
    let dying: HashSet<_> = entities.iter().copied().collect();
    for &entity in &entities {
        for child in rndr_core::children_of(world, entity) {
            if !dying.contains(&child) {
                world.remove::<rndr_core::Parent>(child);
            }
        }
        if let Some(parent) = world.get::<rndr_core::Parent>(entity).map(|p| p.entity) {
            if let Some(children) = world.get_mut::<rndr_core::Children>(parent) {
                children.list.retain(|e| *e != entity);
            }
        }
        crate::chart::release(world, entity);
        world.despawn_entity(entity);
    }
}
/// Release the known renderer nodes under a physical block, preserving all
/// application attachments. Nested logical owners must already be parked.
pub(crate) fn clear_presentation(world: &mut World, root: Entity) {
    let owner = ancestor_key(world, root);
    let mut pending = rndr_core::children_of(world, root);
    let mut nodes = Vec::new();
    let mut native = HashSet::new();
    while let Some(entity) = pending.pop() {
        if world.has::<LiquidView>(entity)
            && owner.is_none_or(|key| !belongs_under(world, entity, key))
        {
            continue;
        }
        pending.extend(rndr_core::children_of(world, entity));
        if world.has::<OwnedProjection>(entity) && world.has::<MarkdownView>(entity) {
            native.extend(rndr_core::MarkdownSlotHosts::native_projections(
                world, entity,
            ));
        }
        nodes.push(entity);
    }
    let owned = nodes
        .into_iter()
        .filter(|e| world.has::<OwnedProjection>(*e) || native.contains(e))
        .collect();
    remove_projections(world, owned);
    for child in rndr_core::children_of(world, root) {
        world.remove::<rndr_core::Parent>(child);
    }
    world.remove::<rndr_core::Children>(root);
}

#[cfg(test)]
pub(crate) fn diagnostic_counts(world: &World) -> (usize, u64) {
    world
        .resource::<Owners>()
        .map_or((0, 0), |r| (r.refusals.len(), r.dropped_diagnostics))
}
pub(crate) fn refused(world: &mut World, reason: LiquidUpdateError) {
    diagnostic(
        world,
        match reason {
            LiquidUpdateError::StaleOwner => Diagnostic::StaleOwner,
            LiquidUpdateError::Capacity => Diagnostic::Capacity,
            LiquidUpdateError::Exhausted => Diagnostic::Exhausted,
        },
    );
}
pub(crate) fn panicked(world: &mut World) {
    diagnostic(world, Diagnostic::Panicked);
}
fn diagnostic(world: &mut World, event: Diagnostic) {
    let capacity = limits(world).diagnostics;
    if let Some(r) = world.resource_mut::<Owners>() {
        if r.refusals.len() == capacity {
            r.refusals.pop_front();
            r.dropped_diagnostics = r.dropped_diagnostics.saturating_add(1);
        }
        if capacity > 0 {
            r.refusals.push_back(event);
        }
    }
}
// LiquidStream emits a token as its own paragraph. Source metadata is published
// with membership, before Markdown's next physical sync, never at a stale byte.
pub(crate) fn bind_markdown(world: &mut World, view: Entity, markdown: &str) {
    let mut cursor = 0;
    let mut sources = HashMap::new();
    for segment in rndr_liquid::segments(markdown) {
        match segment {
            rndr_liquid::Segment::Markdown(text) => cursor += text.len(),
            rndr_liquid::Segment::Block(index) => {
                sources.insert(cursor, index);
                cursor += rndr_liquid::block_token(index).len();
            }
        }
    }
    bind_sources(world, view, sources);
}

// Lexicographic creation paths support deterministic tree postorder. A longer
// prefix sorts before its parent; siblings retain their creation order.
pub(crate) fn retirement_path(world: &World, mut id: u64) -> Vec<u64> {
    let mut path = vec![id];
    while let Some(parent) = world
        .resource::<Owners>()
        .and_then(|r| r.surfaces.get(&id))
        .and_then(|s| s.parent)
    {
        id = parent.0;
        path.push(id);
    }
    path.reverse();
    path
}
pub(crate) fn transaction_active(world: &World) -> bool {
    world.resource::<Owners>().is_some_and(|r| {
        r.surfaces
            .values()
            .any(|s| s.transaction.load(Ordering::Acquire) == 1)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::*;
    use rndr_core::*;
    use rndr_ui::{apply_theme_bridge, UiPlugin, UiTheme};
    use std::sync::{atomic::AtomicUsize, Mutex, Weak};
    use std::time::Instant;

    const ACC: &str = "```accordion\ntitle: FAQ\n\n---\nlabel: One\nbody: First body\n---\nlabel: Two\nbody: Second body\n```\n\n";
    const CONTROLS: &str = "```tabs\n---\nlabel: A\nbody: Alpha\n---\nlabel: B\nbody: Beta\n```\n\n```steps\n---\nlabel: Task\nbody: Work\n```\n\n```carousel\n---\ntitle: First\n---\ntitle: Second\n```\n\n```recipe\ntitle: Soup\n\n---\nlabel: Simmer\nbody: Stir\nduration: 5s\n```\n\n```feed\nid: news\ndatatype: text\n```\n\n```chart\ntype: bar\ntitle: Users\n| Month | Users |\n|---|---|\n| Jan | 3 |\n| Feb | 4 |\n```\n\n";
    const NESTED: &str = "```report\ntitle: Nested\n\n```accordion\ntitle: Inner\n\n---\nlabel: Child\nbody: Child body\n```\n\n```\n\n";
    #[derive(Default)]
    struct Model {
        local: HashMap<u64, Arc<()>>,
        contexts: Vec<LiquidFeedContext>,
        weak: Vec<Weak<()>>,
        cancelled_backend: usize,
        retire_order: Vec<u64>,
    }
    struct Cooperative(Arc<Mutex<Model>>);
    impl LiquidHost for Cooperative {
        fn load_feed_for(
            &mut self,
            _: &mut World,
            context: &LiquidFeedContext,
            _: &FeedRequest,
        ) -> FeedOutcome {
            let mut model = self.0.lock().unwrap();
            if !model.local.contains_key(&context.request_id()) {
                let capture = Arc::new(());
                model.weak.push(Arc::downgrade(&capture));
                model.local.insert(context.request_id(), capture);
                model.contexts.push(context.clone());
            }
            FeedOutcome::Pending
        }
        fn retire_feed_observation(&mut self, _: &mut World, context: &LiquidFeedContext) {
            assert!(context.is_retired());
            let mut model = self.0.lock().unwrap();
            model.local.remove(&context.request_id());
            model.retire_order.push(context.request_id());
            context.acknowledge_retirement();
        }
    }
    struct Capture {
        value: Arc<()>,
        drops: Arc<AtomicUsize>,
    }
    impl Drop for Capture {
        fn drop(&mut self) {
            self.drops.fetch_add(1, Ordering::SeqCst);
        }
    }
    impl LiquidInteractionHandler for Capture {
        fn on_interaction(&mut self, _: &mut World, _: &LiquidInteractionEnvelope) {
            let _ = &self.value;
        }
    }
    fn boot(seed: u64, model: Arc<Mutex<Model>>) -> Sim {
        let mut sim = Sim::with_seed(seed);
        sim.app.add_plugin(UiPlugin);
        sim.app.add_plugin(MarkdownPlugin);
        sim.app.add_plugin(LiquidUiPlugin::new(Cooperative(model)));
        sim.world().insert_resource(Viewport::new(960.0, 6000.0));
        apply_theme_bridge(sim.world(), UiTheme::default());
        frame(&mut sim);
        frame(&mut sim);
        sim
    }
    fn frame(sim: &mut Sim) {
        rndr_paint::frame(&mut sim.app);
        sim.run_until_parked();
    }
    fn pump(sim: &mut Sim) {
        frame(sim);
        frame(sim);
    }
    fn nodes(world: &World, at: Entity, out: &mut Vec<Entity>) {
        out.push(at);
        for child in children_of(world, at) {
            nodes(world, child, out);
        }
    }
    fn press(sim: &mut Sim, root: Entity, label: &str) {
        let mut all = Vec::new();
        nodes(sim.app.world(), root, &mut all);
        let mut entity = all
            .into_iter()
            .find(|e| {
                sim.app
                    .world()
                    .get::<Label>(*e)
                    .is_some_and(|l| l.text == label)
            })
            .unwrap_or_else(|| panic!("no label {label}"));
        while try_activate(sim.world(), entity) != ActivationResult::Accepted {
            entity = sim
                .app
                .world()
                .get::<Parent>(entity)
                .expect("action parent")
                .entity;
        }
        for request in take_action_intents(sim.world()) {
            assert!(dispatch(sim.world(), &request));
        }
        frame(sim);
    }
    fn snapshot(world: &World, model: &Model) -> Vec<usize> {
        let owners = world.resource::<Owners>();
        let (records, owned, global, queued) = crate::state::counts(world);
        let (saved, carry, mounted) = crate::view::retention_counts(world);
        let (watches, observations, retiring, unack) = crate::feed::counts(world);
        let mut values = vec![
            records,
            crate::timer::count(world),
            owners.map_or(0, |r| r.surfaces.len()),
            owners.map_or(0, |r| r.surfaces.values().map(|s| s.blocks.len()).sum()),
            owners.map_or(0, |r| {
                r.surfaces.values().filter(|s| s.parent.is_some()).count()
            }),
            saved,
            carry,
            mounted,
            crate::chart::count(world),
            watches,
            observations,
            owned,
            global,
            queued,
            retiring,
            model.local.len(),
            unack,
            world.entities_with::<OwnedProjection>().len(),
            world.entities_with::<Layout>().len(),
        ];
        // Parked snapshots are no longer physical LiquidView components.
        if let Some(r) = owners {
            for s in r.surfaces.values() {
                if let Some(v) = &s.parked {
                    let (saved, carry) = v.logical_retention_counts();
                    values[5] += saved;
                    values[6] += carry;
                }
            }
        }
        values
    }
    #[test]
    fn full_same_world_100_cycle_resource_oracle_seeds_1_7_42() {
        let mut failures = Vec::new();
        for seed in [1, 7, 42] {
            let started = Instant::now();
            let model = Arc::new(Mutex::new(Model::default()));
            let mut sim = boot(seed, model.clone());
            let global = Arc::new(());
            let global_weak = Arc::downgrade(&global);
            let global_drops = Arc::new(AtomicUsize::new(0));
            subscribe_liquid_interactions(
                sim.world(),
                Capture {
                    value: global,
                    drops: global_drops.clone(),
                },
            );
            let warm = liquid_view(sim.world(), ACC);
            pump(&mut sim);
            press(&mut sim, warm, "+ One");
            despawn(sim.world(), warm);
            pump(&mut sim);
            let baseline = snapshot(sim.app.world(), &model.lock().unwrap());
            assert_eq!(
                baseline
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| *i != 12)
                    .map(|(_, v)| *v)
                    .sum::<usize>(),
                0
            );
            let mut high = baseline.clone();
            let drops = Arc::new(AtomicUsize::new(0));
            let filler = (0..120)
                .map(|i| format!("Filler {i} alpha beta gamma delta.\n\n"))
                .collect::<String>();
            for cycle in 0..100 {
                eprintln!("oracle seed={seed} cycle={cycle}");
                let source = format!("{ACC}{CONTROLS}{NESTED}{filler}");
                let view = liquid_view(sim.world(), &source);
                sim.world()
                    .get_mut::<MarkdownView>(view)
                    .unwrap()
                    .set_follow(false);
                pump(&mut sim);
                let handle = liquid_surface_handle(sim.app.world(), view).unwrap();
                let capture = Arc::new(());
                let weak = Arc::downgrade(&capture);
                let token = subscribe_liquid_interactions_owned(
                    sim.world(),
                    LiquidListenerOwner::Surface(handle),
                    Capture {
                        value: capture,
                        drops: drops.clone(),
                    },
                )
                .unwrap_or_else(|_| panic!("subscription"));
                press(&mut sim, view, "+ One");
                press(&mut sim, view, "B");
                press(&mut sim, view, "[ ]");
                press(&mut sim, view, "Next");
                press(&mut sim, view, "Start");
                press(&mut sim, view, "+ Child");
                let timer = liquid_timers(sim.app.world(), view).remove(0);
                let record = liquid_component_states(sim.app.world(), view)
                    .into_iter()
                    .find(|r| r.component_type == "accordion")
                    .unwrap();
                let semantic = key(sim.app.world(), view, record.source).unwrap();
                // Last-good plus an active Pending context are distinct resources.
                let context = model.lock().unwrap().contexts.last().unwrap().clone();
                let feed_record = liquid_component_states(sim.app.world(), view)
                    .into_iter()
                    .find(|r| r.component_type == "feed")
                    .unwrap();
                put_liquid_feed_result(
                    sim.world(),
                    &context,
                    feed_record.revision,
                    FeedOutcome::Ready(FeedBody {
                        datatype: FeedDatatype::Text,
                        body: "last good".into(),
                    }),
                )
                .unwrap();
                sim.world().get_mut::<Scroll>(view).unwrap().offset[1] = 100000.0;
                pump(&mut sim);
                assert_eq!(
                    liquid_surface_handle(sim.app.world(), view).unwrap(),
                    handle
                );
                assert_eq!(key(sim.app.world(), view, record.source), Some(semantic));
                assert_eq!(
                    liquid_timers(sim.app.world(), view)[0].deadline_ms,
                    timer.deadline_ms
                );
                sim.world().get_mut::<Scroll>(view).unwrap().offset[1] = 0.0;
                pump(&mut sim);
                assert_eq!(
                    liquid_timers(sim.app.world(), view)[0].deadline_ms,
                    timer.deadline_ms
                );
                push_liquid_str(
                    sim.world(),
                    view,
                    "\n```callout\nbody: Settled\n```\n\n```accordion\nPartial",
                );
                pump(&mut sim);
                assert_eq!(key(sim.app.world(), view, record.source), Some(semantic));
                // F2's shared semantic resolver: unique prefix remaps; changed
                // same-byte content and remove/reinsert retire.
                let prefixed = format!("Prefix\n\n{source}");
                set_liquid_source(sim.world(), view, &prefixed);
                pump(&mut sim);
                let moved = liquid_component_states(sim.app.world(), view)
                    .into_iter()
                    .find(|r| r.component_type == "accordion")
                    .unwrap();
                assert_eq!(key(sim.app.world(), view, moved.source), Some(semantic));
                // Interleave keyed identity/reset/refusal with the full F1
                // resources in this SAME World on EVERY cycle, not a detached
                // microbenchmark. Initial keyed rebind intentionally resets all
                // unkeyed resources; contexts remain charged until host ack.
                let occurrences = crate::inspect_liquid_occurrences(&prefixed).unwrap();
                let keys = crate::LiquidKeyMap {
                    keys: (0..occurrences.len())
                        .map(|i| Some(format!("component-{i}")))
                        .collect(),
                };
                let rev = crate::liquid_source_revision(sim.app.world(), handle).unwrap();
                crate::set_liquid_source_keyed(sim.world(), handle, rev, &prefixed, &keys).unwrap();
                assert!(!block_live(sim.app.world(), semantic));
                assert!(context.is_retired());
                let block = crate::liquid_component_handle(sim.app.world(), handle, 0).unwrap();
                crate::put_liquid_component_state_for(
                    sim.world(),
                    block,
                    0,
                    LiquidComponentState::Accordion { open: vec![0] },
                )
                .unwrap();
                let cosmetic = prefixed.replace("title: FAQ", "title: F2 title");
                let rev = crate::liquid_source_revision(sim.app.world(), handle).unwrap();
                crate::set_liquid_source_keyed(sim.world(), handle, rev, &cosmetic, &keys).unwrap();
                assert_eq!(
                    crate::liquid_component_handle(sim.app.world(), handle, 0).unwrap(),
                    block
                );
                assert_eq!(
                    crate::liquid_component_state_for(sim.app.world(), block)
                        .unwrap()
                        .unwrap()
                        .revision,
                    1
                );
                let edited = cosmetic.replace("First body", "F2 child schema");
                let rev = crate::liquid_source_revision(sim.app.world(), handle).unwrap();
                crate::set_liquid_source_keyed(sim.world(), handle, rev, &edited, &keys).unwrap();
                let reset = crate::liquid_component_state_for(sim.app.world(), block)
                    .unwrap()
                    .unwrap();
                assert_eq!(reset.revision, 2);
                assert_eq!(
                    reset.state,
                    LiquidComponentState::Accordion { open: vec![] }
                );
                assert!(crate::put_liquid_component_state_for(
                    sim.world(),
                    block,
                    1,
                    LiquidComponentState::Accordion { open: vec![0] }
                )
                .is_err());
                assert_eq!(
                    crate::liquid_component_state_for(sim.app.world(), block).unwrap(),
                    Some(reset)
                );
                let fresh = crate::reset_liquid_block(sim.world(), block, 2).unwrap();
                assert_ne!(fresh, block);
                assert!(crate::liquid_component_state_for(sim.app.world(), block).is_err());
                if cycle % 3 == 0 {
                    let rev = crate::liquid_source_revision(sim.app.world(), handle).unwrap();
                    let new_surface =
                        crate::reset_liquid_surface(sim.world(), handle, rev).unwrap();
                    assert_ne!(new_surface, handle);
                    assert!(crate::liquid_source_revision(sim.app.world(), handle).is_err());
                    assert!(crate::liquid_component_state_for(sim.app.world(), fresh).is_err());
                }
                pump(&mut sim);
                if cycle % 2 == 0 {
                    set_liquid_source(sim.world(), view, "");
                    assert!(!block_live(sim.app.world(), semantic));
                    assert!(liquid_timers(sim.app.world(), view).is_empty());
                }
                let used = snapshot(sim.app.world(), &model.lock().unwrap());
                for (h, v) in high.iter_mut().zip(used) {
                    *h = (*h).max(v);
                }
                match (cycle / 2) % 4 {
                    0 => despawn(sim.world(), view),
                    1 => {
                        sim.world().despawn_entity(view);
                    }
                    2 => {
                        sim.world().remove::<LiquidView>(view);
                    }
                    _ => {
                        sim.world().remove::<MarkdownView>(view);
                    }
                }
                assert!(liquid_surface_handle(sim.app.world(), view).is_err());
                assert!(put_liquid_component_state(
                    sim.world(),
                    view,
                    moved.source,
                    moved.revision,
                    LiquidComponentState::Accordion { open: vec![] }
                )
                .is_err());
                assert!(put_liquid_feed_result(
                    sim.world(),
                    &context,
                    feed_record.revision,
                    FeedOutcome::Pending
                )
                .is_err());
                // Fixed trace admits <256 callbacks (one owned and one global
                // per interaction, <=16 feed retirements). Two normal frames
                // plus executor parked suffice; bound = 2+ceil(work/256)+2=5.
                pump(&mut sim);
                // Component-loss leaves a caller-owned shell. Remove only that
                // entity (not its children), so the physical oracle cannot be
                // made green by a test-side cascade sweeping the residue.
                if sim.app.world().is_alive(view) {
                    sim.world().despawn_entity(view);
                }
                let after = snapshot(sim.app.world(), &model.lock().unwrap());
                eprintln!("F1/F2 teardown seed={seed} cycle={cycle} vector={after:?} baseline={baseline:?} external_ack_pending={}", model.lock().unwrap().local.len());
                if after != baseline {
                    assert!(weak.upgrade().is_none());
                    assert_eq!(drops.load(Ordering::SeqCst), cycle + 1);
                    failures.push(format!(
                        "seed{seed} cycle{cycle}: {after:?} != {baseline:?}"
                    ));
                    break;
                }
                assert!(weak.upgrade().is_none());
                assert_eq!(drops.load(Ordering::SeqCst), cycle + 1);
                assert!(model
                    .lock()
                    .unwrap()
                    .weak
                    .iter()
                    .all(|w| w.upgrade().is_none()));
                assert_eq!(model.lock().unwrap().cancelled_backend, 0);
                assert!(global_weak.upgrade().is_some());
                assert!(put_liquid_feed_result(
                    sim.world(),
                    &context,
                    feed_record.revision,
                    FeedOutcome::Pending
                )
                .is_err());
                drop(token);
                let mut m = model.lock().unwrap();
                m.contexts.clear();
                m.weak.clear();
                m.retire_order.clear();
            }
            eprintln!(
                "F1 attempted-prefix seed={seed} baseline={baseline:?} high_water={high:?} elapsed={:?}",
                started.elapsed()
            );
            drop(sim);
            assert!(global_weak.upgrade().is_none());
            assert_eq!(global_drops.load(Ordering::SeqCst), 1);
        }
        assert!(
            failures.is_empty(),
            "full F1 oracle incomplete: {failures:?}"
        );
    }
    #[test]
    fn transaction_reentry_poison_unwind_and_source_refusal_are_atomic() {
        let mut world = World::new();
        let view = liquid_view(&mut world, ACC);
        let handle = liquid_surface_handle(&world, view).unwrap();
        let blocks = world.get::<LiquidView>(view).unwrap().blocks().to_vec();
        let tx = transaction(&world, view).unwrap();
        assert!(transaction(&world, view).is_none());
        set_liquid_source(&mut world, view, "replacement");
        push_liquid_str(&mut world, view, "appended");
        assert_eq!(
            world.get::<LiquidView>(view).unwrap().blocks(),
            blocks.as_slice()
        );
        mark_tree_dead(&world, handle.0);
        tx.commit();
        assert!(!live(&world, handle.0), "commit revived poisoned owner");
        drain(&mut world);
        assert!(liquid_surface_handle(&world, view).is_err());
        let view = liquid_view(&mut world, ACC);
        let handle = liquid_surface_handle(&world, view).unwrap();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _tx = transaction(&world, view).unwrap();
            panic!("unwind extracted publication");
        }));
        assert!(result.is_err());
        assert!(!live(&world, handle.0));
        drain(&mut world);
        assert!(world.get::<LiquidView>(view).is_none());
        assert_eq!(
            diagnostic_counts(&world).0,
            1,
            "aborted publication was not observable"
        );
    }

    #[test]
    fn keyed_source_refusals_and_exhaustion_leave_all_membership_untouched() {
        use crate::*;
        let mut w = World::new();
        let src = "```accordion\ntitle: FAQ\n---\nlabel: One\nbody: Body\n```\n";
        let view = liquid_view(&mut w, src);
        let s = liquid_surface_handle(&w, view).unwrap();
        let keys = LiquidKeyMap {
            keys: vec![Some("a".into())],
        };
        let rev =
            set_liquid_source_keyed(&mut w, s, LiquidSourceRevision::ZERO, src, &keys).unwrap();
        let h = liquid_component_handle(&w, s, 0).unwrap();
        let record = put_liquid_component_state_for(
            &mut w,
            h,
            0,
            LiquidComponentState::Accordion { open: vec![0] },
        )
        .unwrap();
        let tx = transaction(&w, view).unwrap();
        assert_eq!(
            set_liquid_source_keyed(&mut w, s, rev, src, &keys),
            Err(LiquidF2Error::StaleOwner)
        );
        assert_eq!(
            put_liquid_component_state_for(&mut w, h, 1, record.state.clone()),
            Err(LiquidF2Error::StaleOwner)
        );
        assert_eq!(
            reset_liquid_block(&mut w, h, 1),
            Err(LiquidF2Error::StaleOwner)
        );
        assert_eq!(
            reset_liquid_surface(&mut w, s, rev),
            Err(LiquidF2Error::StaleOwner)
        );
        tx.commit();
        assert_eq!(
            liquid_component_state_for(&w, h).unwrap(),
            Some(record.clone())
        );
        let invalid = format!("{}\n{src}", rndr_liquid::block_token(999));
        let invalid_keys = LiquidKeyMap {
            keys: vec![None, None],
        };
        assert_eq!(
            set_liquid_source_keyed(&mut w, s, rev, &invalid, &invalid_keys),
            Err(LiquidF2Error::InvalidKeys)
        );
        w.insert_resource(LiquidLimits {
            scopes: 2,
            ..Default::default()
        });
        let doubled = format!("{src}\n{src}");
        assert_eq!(
            set_liquid_source_keyed(
                &mut w,
                s,
                rev,
                &doubled,
                &LiquidKeyMap {
                    keys: vec![Some("a".into()), Some("b".into())]
                }
            ),
            Err(LiquidF2Error::Capacity)
        );
        assert_eq!(liquid_source_revision(&w, s).unwrap(), rev);
        assert_eq!(liquid_component_handle(&w, s, 0).unwrap(), h);
        assert_eq!(
            liquid_component_state_for(&w, h).unwrap(),
            Some(record.clone())
        );
        w.resource_mut::<Owners>()
            .unwrap()
            .surfaces
            .get_mut(&s.0)
            .unwrap()
            .revision = u64::MAX;
        assert_eq!(
            set_liquid_source_keyed(
                &mut w,
                s,
                LiquidSourceRevision(u64::MAX),
                &src.replace("FAQ", "Title"),
                &keys
            ),
            Err(LiquidF2Error::Exhausted)
        );
        assert_eq!(
            reset_liquid_block(&mut w, h, 1),
            Err(LiquidF2Error::Exhausted)
        );
        assert_eq!(liquid_component_state_for(&w, h).unwrap(), Some(record));
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _tx = transaction(&w, view).unwrap();
            panic!("poison F2 owner");
        }));
        assert_eq!(
            liquid_component_state_for(&w, h),
            Err(LiquidF2Error::StaleOwner)
        );
        assert_eq!(
            set_liquid_source_keyed(&mut w, s, LiquidSourceRevision(u64::MAX), src, &keys),
            Err(LiquidF2Error::StaleOwner)
        );
    }
    #[test]
    fn complete_serial_range_reservation_cannot_consume_a_partial_prefix() {
        let counter = AtomicU64::new(u64::MAX - 2);
        assert_eq!(reserve_from(&counter, 3), None);
        assert_eq!(counter.load(Ordering::Relaxed), u64::MAX - 2);
        assert_eq!(reserve_from(&counter, 2), Some(u64::MAX - 2));
        assert_eq!(reserve_from(&counter, 1), None);
        assert_eq!(counter.load(Ordering::Relaxed), u64::MAX);
    }
    #[test]
    fn checked_serial_allocator_never_aliases_on_exhaustion() {
        let counter = AtomicU64::new(u64::MAX - 1);
        assert_eq!(allocate(&counter), Some(u64::MAX - 1));
        assert_eq!(allocate(&counter), None);
        assert_eq!(counter.load(Ordering::Relaxed), u64::MAX);
    }
}
