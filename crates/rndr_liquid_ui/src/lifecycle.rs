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
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
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
    view: Option<Entity>,
    parent: Option<Key>,
    path: String,
    blocks: std::collections::BTreeMap<usize, (u64, LiquidBlock)>,
    sources: HashMap<usize, usize>,
    transaction: Arc<AtomicU8>,
    projections: HashSet<Entity>,
    parked: Option<LiquidView>,
}
#[derive(Default)]
pub(crate) struct Owners {
    surfaces: HashMap<u64, Surface>,
    views: HashMap<Entity, u64>,
    refusals: std::collections::VecDeque<LiquidUpdateError>,
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
    if surface.transaction.load(Ordering::Acquire) == 2 {
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
    Some((id, s.blocks.get(&index)?.0))
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
pub(crate) fn location(world: &World, key: Key) -> Option<(Entity, usize, usize)> {
    if !block_live(world, key) {
        return None;
    }
    let s = world.resource::<Owners>()?.surfaces.get(&key.0)?;
    let index = *s.blocks.iter().find(|(_, (id, _))| *id == key.1)?.0;
    let source = *s.sources.iter().find(|(_, i)| **i == index)?.0;
    Some((s.view?, source, index))
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
    let id = serial().ok_or(LiquidUpdateError::Exhausted)?;
    let ids: Option<std::collections::BTreeMap<_, _>> = rndr_liquid::segments(markdown)
        .into_iter()
        .filter_map(|s| {
            if let rndr_liquid::Segment::Block(index) = s {
                Some(index)
            } else {
                None
            }
        })
        .map(|i| Some((i, (serial()?, blocks.get(i)?.clone()))))
        .collect();
    let blocks = ids.ok_or(LiquidUpdateError::Exhausted)?;
    let r = world.resource_mut::<Owners>().unwrap();
    r.surfaces.insert(
        id,
        Surface {
            view: Some(view),
            parent,
            path,
            blocks,
            sources: HashMap::new(),
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
        self.flag.store(0, Ordering::Release);
        self.committed = true;
    }
}
impl Drop for Transaction {
    fn drop(&mut self) {
        if !self.committed {
            self.flag.store(2, Ordering::Release);
        }
    }
}
pub(crate) fn transaction(world: &World, view: Entity) -> Option<Transaction> {
    let r = world.resource::<Owners>()?;
    let s = r.surfaces.get(r.views.get(&view)?)?;
    s.transaction.store(1, Ordering::Release);
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
        return Err(LiquidUpdateError::Capacity);
    }
    let r = world
        .resource::<Owners>()
        .ok_or(LiquidUpdateError::StaleOwner)?;
    let id = *r.views.get(&view).ok_or(LiquidUpdateError::StaleOwner)?;
    let old = &r.surfaces[&id].blocks;
    let mut used = HashSet::new();
    let mut plan = std::collections::BTreeMap::new();
    for index in rndr_liquid::segments(markdown).into_iter().filter_map(|s| {
        if let rndr_liquid::Segment::Block(i) = s {
            Some(i)
        } else {
            None
        }
    }) {
        let block = new.get(index).ok_or(LiquidUpdateError::StaleOwner)?;
        let found = if append {
            old.get(&index).filter(|(_, b)| b == block).map(|_| index)
        } else {
            old.iter()
                .find(|(i, (_, b))| !used.contains(*i) && b == block)
                .map(|(i, _)| *i)
        };
        let block_id = if let Some(i) = found {
            used.insert(i);
            old[&i].0
        } else {
            serial().ok_or(LiquidUpdateError::Exhausted)?
        };
        plan.insert(index, (block_id, block.clone()));
    }
    let retired: Vec<Key> = old
        .iter()
        .filter(|(i, _)| !used.contains(*i))
        .map(|(_, (b, _))| (id, *b))
        .collect();
    world
        .resource_mut::<Owners>()
        .unwrap()
        .surfaces
        .get_mut(&id)
        .unwrap()
        .blocks = plan;
    for key in retired {
        retire_children(world, key);
    }
    crate::state::retain_live(world);
    crate::timer::retain_live(world);
    crate::feed::retire_dead(world);
    Ok(())
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
        world.remove::<Lease>(view);
    }
    // Only entities actually constructed by Liquid are reclaimed. In particular
    // raw owner despawn must not cascade through unrelated application children.
    for entity in s.projections {
        crate::chart::release(world, entity);
        world.despawn_entity(entity);
    }
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
        mark_tree_dead(world, *id);
    }
    for id in dead {
        retire_surface(world, id);
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
            let view = world.get::<rndr_core::Parent>(entity)?.entity;
            return key_at(world, view, root.index);
        }
        entity = world.get::<rndr_core::Parent>(entity)?.entity;
    }
    None
}

/// Park logical descendants before their presentation subtree is destroyed.
pub(crate) fn park_under(world: &mut World, root: Entity) {
    fn walk(world: &World, at: Entity, out: &mut Vec<Entity>) {
        for child in rndr_core::children_of(world, at) {
            walk(world, child, out);
        }
        if world.has::<LiquidView>(at) {
            out.push(at);
        }
    }
    let mut views = Vec::new();
    walk(world, root, &mut views);
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
    r.views.insert(view, id);
    world.insert(view, Lease(id));
    Some(liquid)
}
pub(crate) fn track_tree(world: &mut World, root: Entity) {
    let Some(view) = world.get::<rndr_core::Parent>(root).map(|p| p.entity) else {
        return;
    };
    fn walk(world: &World, root: Entity, out: &mut Vec<Entity>) {
        out.push(root);
        for child in rndr_core::children_of(world, root) {
            walk(world, child, out);
        }
    }
    let mut nodes = Vec::new();
    walk(world, root, &mut nodes);
    for node in nodes {
        own_projection(world, view, node);
    }
}

pub(crate) fn refused(world: &mut World, reason: LiquidUpdateError) {
    let capacity = limits(world).diagnostics;
    if let Some(r) = world.resource_mut::<Owners>() {
        if r.refusals.len() == capacity {
            r.refusals.pop_front();
            r.dropped_diagnostics = r.dropped_diagnostics.saturating_add(1);
        }
        r.refusals.push_back(reason);
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

pub(crate) fn depth(world: &World, mut id: u64) -> usize {
    let mut depth = 0;
    while let Some(parent) = world
        .resource::<Owners>()
        .and_then(|r| r.surfaces.get(&id))
        .and_then(|s| s.parent)
    {
        depth += 1;
        id = parent.0;
    }
    depth
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
                // F1's deliberately provisional equality resolver: unique prefix
                // remaps; changed same-byte content and remove/reinsert retire.
                let prefixed = format!("Prefix\n\n{source}");
                set_liquid_source(sim.world(), view, &prefixed);
                pump(&mut sim);
                let moved = liquid_component_states(sim.app.world(), view)
                    .into_iter()
                    .find(|r| r.component_type == "accordion")
                    .unwrap();
                assert_eq!(key(sim.app.world(), view, moved.source), Some(semantic));
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
                // Component-removal paths leave the caller's Markdown shell;
                // it is not Liquid-owned and is explicitly removed by the test.
                if sim.app.world().is_alive(view) {
                    despawn(sim.world(), view);
                }
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
    fn checked_serial_allocator_never_aliases_on_exhaustion() {
        let counter = AtomicU64::new(u64::MAX - 1);
        assert_eq!(allocate(&counter), Some(u64::MAX - 1));
        assert_eq!(allocate(&counter), None);
        assert_eq!(counter.load(Ordering::Relaxed), u64::MAX);
    }
}
