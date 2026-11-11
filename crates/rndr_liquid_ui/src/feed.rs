//! Async feed loads for `feed` blocks and dashboard tiles.
//!
//! The host supplies bytes through [`LiquidHost::load_feed`]. A later failure
//! keeps the last ready body. The record is the component state for that
//! source, so it survives host-slot recycling. Interval polls run only while
//! the block is mounted.
use crate::host::{FeedBody, FeedDatatype, FeedEntry, FeedOutcome, FeedRequest, LiquidEvent};
use crate::lifecycle::{self, Key};
use crate::render;
use std::collections::{HashMap, VecDeque};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

/// Identity and liveness of one local feed observation, never backend authority.
#[derive(Clone, Debug)]
pub struct LiquidFeedContext {
    key: Key,
    request: u64,
    path: Vec<u64>,
    retired: Arc<AtomicBool>,
    acknowledged: Arc<AtomicBool>,
}
impl PartialEq for LiquidFeedContext {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key && self.request == other.request
    }
}
impl Eq for LiquidFeedContext {}
impl LiquidFeedContext {
    #[cfg(test)]
    pub(crate) fn request_id(&self) -> u64 {
        self.request
    }
    /// Whether this observation has been retired. A worker must stop UI delivery.
    pub fn is_retired(&self) -> bool {
        self.retired.load(Ordering::Acquire)
    }
    /// Confirm release of local observation resources, not durable job cancellation.
    pub fn acknowledge_retirement(&self) {
        if self.is_retired() {
            self.acknowledged.store(true, Ordering::Release);
        }
    }
}
struct Observation {
    context: LiquidFeedContext,
    pending: bool,
}
#[derive(Default)]
struct Observations {
    next: u64,
    active: HashMap<(Key, String), Observation>,
    unacknowledged: Vec<LiquidFeedContext>,
    retiring: VecDeque<Observation>,
}
fn observation(
    world: &mut World,
    key: Key,
    instance: &str,
    fresh: bool,
) -> Option<LiquidFeedContext> {
    if !world.has_resource::<Observations>() {
        world.insert_resource(Observations::default());
    }
    let max_scopes = lifecycle::limits(world).scopes;
    let max_lanes = lifecycle::limits(world).lanes;
    {
        let r = world.resource_mut::<Observations>()?;
        r.unacknowledged
            .retain(|c| !c.acknowledged.load(Ordering::Acquire));
        if !fresh {
            if let Some(old) = r.active.get(&(key, instance.to_string())) {
                return Some(old.context.clone());
            }
        }
        let replacing = r.active.contains_key(&(key, instance.to_string()));
        if r.next == u64::MAX
            || r.active.len() + r.unacknowledged.len() + r.retiring.len() >= max_scopes
            || (!replacing
                && r.active.keys().filter(|(owner, _)| *owner == key).count() >= max_lanes)
        {
            return None;
        }
    }
    // All refusal gates precede cancellation of the old valid request.
    let old = world
        .resource_mut::<Observations>()?
        .active
        .remove(&(key, instance.to_string()));
    if let Some(old) = old {
        retire(world, old);
    }
    let path = lifecycle::retirement_path(world, key.0);
    let r = world.resource_mut::<Observations>()?;
    let next = r.next + 1;
    r.next = next;
    let context = LiquidFeedContext {
        key,
        request: next,
        path,
        retired: Arc::new(AtomicBool::new(false)),
        acknowledged: Arc::new(AtomicBool::new(false)),
    };
    r.active.insert(
        (key, instance.to_string()),
        Observation {
            context: context.clone(),
            pending: false,
        },
    );
    Some(context)
}

fn retire(world: &mut World, old: Observation) {
    old.context.retired.store(true, Ordering::Release);
    if let Some(r) = world.resource_mut::<Observations>() {
        r.retiring.push_back(old);
    }
}
fn drain_notifications(world: &mut World) {
    if !view::host_available(world) {
        return;
    }
    loop {
        if world
            .resource::<Observations>()
            .is_none_or(|r| r.retiring.is_empty())
        {
            break;
        }
        if !state::take_callback_budget(world) {
            break;
        }
        let old = world
            .resource_mut::<Observations>()
            .unwrap()
            .retiring
            .pop_front()
            .unwrap();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            view::with_host(world, |host, world| {
                host.retire_feed_observation(world, &old.context)
            })
        }));
        if result.is_err() {
            lifecycle::panicked(world);
        }
        if old.pending && !old.context.acknowledged.load(Ordering::Acquire) {
            if let Some(r) = world.resource_mut::<Observations>() {
                r.unacknowledged.push(old.context);
            }
        }
    }
}
/// Apply a deferred result only to this exact live request and state revision.
/// Legacy positional puts cannot establish request-generation safety.
pub fn put_liquid_feed_result(
    world: &mut World,
    context: &LiquidFeedContext,
    expected_revision: u64,
    outcome: FeedOutcome,
) -> Result<(), crate::LiquidUpdateError> {
    if context.is_retired() || !lifecycle::block_live(world, context.key) {
        return Err(crate::LiquidUpdateError::StaleOwner);
    }
    let instance = world
        .resource::<Observations>()
        .and_then(|r| {
            r.active
                .iter()
                .find(|(_, o)| o.context == *context)
                .map(|((_, i), _)| i.clone())
        })
        .ok_or(crate::LiquidUpdateError::StaleOwner)?;
    let (view, source, block) =
        lifecycle::location(world, context.key).ok_or(crate::LiquidUpdateError::StaleOwner)?;
    let record = state::liquid_component_state(world, view, source)
        .ok_or(crate::LiquidUpdateError::StaleOwner)?;
    if record.revision == u64::MAX {
        return Err(crate::LiquidUpdateError::Exhausted);
    }
    if record.revision != expected_revision {
        return Err(crate::LiquidUpdateError::StaleOwner);
    }
    if !state::can_emit(world) {
        return Err(crate::LiquidUpdateError::Capacity);
    }
    let Some(slot) = stored(world, view, source, &instance) else {
        return Err(crate::LiquidUpdateError::StaleOwner);
    };
    let spec = Spec {
        instance,
        feed_id: slot.feed_id.clone(),
        datatype: FeedDatatype::parse(&slot.datatype).unwrap_or(FeedDatatype::Text),
        field: None,
        autoload: true,
        interval_ms: None,
    };
    commit_outcome(world, view, source, block, &spec, &outcome);
    if let Some(r) = world.resource_mut::<Observations>() {
        if let Some(o) = r.active.get_mut(&(context.key, spec.instance.clone())) {
            o.pending = matches!(outcome, FeedOutcome::Pending);
        }
    }
    let roots: Vec<_> = world
        .entities_with::<BlockRoot>()
        .into_iter()
        .filter(|root| {
            place(world, *root).is_some_and(|(owner, at, _)| owner == view && at == source)
        })
        .collect();
    for root in roots {
        render::rebuild(world, root);
    }
    Ok(())
}

fn retirement_cmp(a: &LiquidFeedContext, b: &LiquidFeedContext) -> std::cmp::Ordering {
    for (a, b) in a.path.iter().zip(&b.path) {
        match a.cmp(b) {
            std::cmp::Ordering::Equal => {}
            order => return order,
        }
    }
    b.path
        .len()
        .cmp(&a.path.len())
        .then_with(|| b.request.cmp(&a.request))
}

pub(crate) fn retire_dead(world: &mut World) {
    let dead: Vec<_> = world
        .resource::<Observations>()
        .map(|r| {
            r.active
                .keys()
                .filter(|(key, _)| !lifecycle::block_live(world, *key))
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    let mut removed: Vec<_> = if let Some(r) = world.resource_mut::<Observations>() {
        dead.into_iter()
            .filter_map(|key| r.active.remove(&key))
            .collect()
    } else {
        Vec::new()
    };
    removed.sort_by(|a, b| retirement_cmp(&a.context, &b.context));
    // Invalidate the entire batch before invoking any host callback.
    for old in &removed {
        old.context.retired.store(true, Ordering::Release);
    }
    for old in removed {
        retire(world, old);
    }
    if let Some(r) = world.resource_mut::<Observations>() {
        r.unacknowledged
            .retain(|c| !c.acknowledged.load(Ordering::Acquire));
    }
    // Source transactions may temporarily extract LiquidView. Defer host
    // notifications until that whole publication has committed.
    if !crate::lifecycle::transaction_active(world) {
        drain_notifications(world);
    }
    let dead: Vec<_> = world
        .entities_with::<FeedWatch>()
        .into_iter()
        .filter(|root| place(world, *root).is_none())
        .collect();
    for root in dead {
        world.remove::<FeedWatch>(root);
    }
}

use crate::state::{
    self, FeedPhase, FeedSlot, LiquidComponentState, LiquidDisposition, LiquidInteraction,
    LiquidInteractionEnvelope,
};
use crate::view::{self, BlockRoot};
use rndr_core::{Entity, Parent, Time, World};
use rndr_liquid::DashboardTile;

/// Default tile poll, matching Medousa's feed-tail interval.
pub(crate) const DEFAULT_TILE_INTERVAL_MS: u64 = 15_000;

#[derive(Clone, Debug)]
pub(crate) struct Spec {
    pub instance: String,
    pub feed_id: String,
    pub datatype: FeedDatatype,
    pub field: Option<String>,
    pub autoload: bool,
    pub interval_ms: Option<u64>,
}

#[derive(Clone, Debug)]
struct Tracked {
    spec: Spec,
    probed: bool,
    async_feed: bool,
    last_poll_ms: u64,
}

pub(crate) struct FeedWatch {
    view: Entity,
    source: usize,
    block: usize,
    slots: Vec<Tracked>,
}

impl rndr_core::Component for FeedWatch {}

pub(crate) enum Shown {
    Sync(Vec<FeedEntry>),
    Live(FeedSlot),
}

pub(crate) fn tile_manual(tile: &DashboardTile) -> bool {
    tile.refresh.as_deref() != Some("interval")
}

pub(crate) fn tile_interval(tile: &DashboardTile) -> Option<u64> {
    if tile.refresh.as_deref() == Some("manual") {
        return None;
    }
    Some(tile.interval_ms.unwrap_or(DEFAULT_TILE_INTERVAL_MS))
}

/// Paint model for one feed or tile. Probes the host the first time.
pub(crate) fn ensure(world: &mut World, root: Entity, spec: Spec) -> Shown {
    let Some((view, source, block)) = place(world, root) else {
        return Shown::Sync(Vec::new());
    };
    remember(world, root, view, source, block, spec.clone());
    let tracked = world
        .get::<FeedWatch>(root)
        .and_then(|watch| {
            watch
                .slots
                .iter()
                .find(|slot| slot.spec.instance == spec.instance)
        })
        .cloned();
    let Some(tracked) = tracked else {
        return Shown::Sync(Vec::new());
    };
    if tracked.probed && !tracked.async_feed {
        return Shown::Sync(sync_rows(world, &spec));
    }
    if let Some(slot) = stored(world, view, source, &spec.instance) {
        if slot.inflight {
            poll(world, root, &spec, false);
        }
        return Shown::Live(stored(world, view, source, &spec.instance).unwrap_or(slot));
    }
    if !spec.autoload {
        return Shown::Live(empty_slot(&spec));
    }
    match poll(world, root, &spec, false) {
        FeedOutcome::Defer => Shown::Sync(sync_rows(world, &spec)),
        _ => Shown::Live(
            stored(world, view, source, &spec.instance).unwrap_or_else(|| empty_slot(&spec)),
        ),
    }
}

/// Manual refresh of one instance. Sync feeds only report the existing event.
pub(crate) fn refresh(world: &mut World, root: Entity, instance: &str) {
    let Some(spec) = world.get::<FeedWatch>(root).and_then(|watch| {
        watch
            .slots
            .iter()
            .find(|slot| slot.spec.instance == instance)
            .map(|slot| slot.spec.clone())
    }) else {
        return;
    };
    if world.get::<FeedWatch>(root).is_some_and(|watch| {
        watch
            .slots
            .iter()
            .any(|slot| slot.spec.instance == instance && slot.probed && !slot.async_feed)
    }) {
        view::emit(
            world,
            LiquidEvent::RefreshFeed {
                feed_id: spec.feed_id,
            },
        );
        return;
    }
    poll(world, root, &spec, true);
    render::rebuild(world, root);
}

/// Continue in-flight loads and fire due intervals.
pub(crate) fn reconcile(world: &mut World) {
    let now = now_ms(world);
    let roots = world.entities_with::<FeedWatch>();
    for root in roots {
        let specs: Vec<(Spec, bool, u64)> = world
            .get::<FeedWatch>(root)
            .map(|watch| {
                watch
                    .slots
                    .iter()
                    .filter(|slot| slot.async_feed)
                    .map(|slot| (slot.spec.clone(), slot.async_feed, slot.last_poll_ms))
                    .collect()
            })
            .unwrap_or_default();
        let Some((view, source, _)) = place(world, root) else {
            continue;
        };
        for (spec, _, last) in specs {
            let slot = stored(world, view, source, &spec.instance);
            let inflight = slot.as_ref().is_some_and(|slot| slot.inflight);
            let due = spec.interval_ms.is_some_and(|interval| {
                !inflight && now.saturating_sub(last) >= interval && slot.is_some()
            });
            if inflight || due {
                poll(world, root, &spec, false);
                render::rebuild(world, root);
            }
        }
    }
}

fn poll(world: &mut World, root: Entity, spec: &Spec, manual: bool) -> FeedOutcome {
    let Some((view, source, block)) = place(world, root) else {
        return FeedOutcome::Defer;
    };
    let request = FeedRequest {
        feed_id: spec.feed_id.clone(),
        datatype: spec.datatype,
        field: spec.field.clone(),
        instance: spec.instance.clone(),
    };
    if !state::can_emit(world) {
        return FeedOutcome::Pending;
    }
    let Some(key) = lifecycle::key(world, view, source) else {
        return FeedOutcome::Defer;
    };
    let previous = stored(world, view, source, &spec.instance);
    let fresh = manual || previous.as_ref().is_some_and(|s| !s.inflight);
    let Some(context) = observation(world, key, &spec.instance, fresh) else {
        return FeedOutcome::Pending;
    };
    let outcome = view::with_host(world, |host, world| {
        host.load_feed_for(world, &context, &request)
    });
    if !lifecycle::block_live(world, key) || context.is_retired() {
        return FeedOutcome::Defer;
    }
    if let Some(r) = world.resource_mut::<Observations>() {
        if let Some(o) = r.active.get_mut(&(key, spec.instance.clone())) {
            o.pending = matches!(outcome, FeedOutcome::Pending);
        }
    }
    if matches!(outcome, FeedOutcome::Defer) {
        mark(world, root, spec, false);
        return FeedOutcome::Defer;
    }
    mark(world, root, spec, true);
    let record = commit_outcome(world, view, source, block, spec, &outcome);
    if manual {
        view::emit(
            world,
            LiquidEvent::RefreshFeed {
                feed_id: spec.feed_id.clone(),
            },
        );
    }
    state::emit(
        world,
        LiquidInteractionEnvelope {
            version: 1,
            view,
            source,
            node_id: "feed".into(),
            instance_id: spec.instance.clone(),
            event: LiquidInteraction::FeedRefresh,
            disposition: LiquidDisposition::LocalState,
            revision: record.revision,
        },
    );
    outcome
}

fn upsert(
    world: &mut World,
    view: Entity,
    source: usize,
    block: usize,
    slot: FeedSlot,
) -> state::LiquidComponentRecord {
    let mut slots = match state::liquid_component_state(world, view, source) {
        Some(record) => match record.state {
            LiquidComponentState::Feeds { slots } => slots,
            _ => Vec::new(),
        },
        None => Vec::new(),
    };
    if let Some(existing) = slots.iter_mut().find(|item| item.instance == slot.instance) {
        *existing = slot;
    } else {
        slots.push(slot);
    }
    state::store_snapshot(
        world,
        view,
        source,
        block,
        "feed",
        LiquidComponentState::Feeds { slots },
    )
}

fn stored(world: &World, view: Entity, source: usize, instance: &str) -> Option<FeedSlot> {
    let record = state::liquid_component_state(world, view, source)?;
    match record.state {
        LiquidComponentState::Feeds { slots } => {
            slots.into_iter().find(|slot| slot.instance == instance)
        }
        _ => None,
    }
}

fn sync_rows(world: &mut World, spec: &Spec) -> Vec<FeedEntry> {
    let id = spec.feed_id.clone();
    let kind = spec.datatype.as_str().to_string();
    view::with_host(world, |host, world| host.feed(world, &id, &kind))
}

fn remember(
    world: &mut World,
    root: Entity,
    view: Entity,
    source: usize,
    block: usize,
    spec: Spec,
) {
    let now = now_ms(world);
    if world.get::<FeedWatch>(root).is_none() {
        world.insert(
            root,
            FeedWatch {
                view,
                source,
                block,
                slots: Vec::new(),
            },
        );
    }
    let watch = world.get_mut::<FeedWatch>(root).unwrap();
    watch.view = view;
    watch.source = source;
    watch.block = block;
    if let Some(existing) = watch
        .slots
        .iter_mut()
        .find(|slot| slot.spec.instance == spec.instance)
    {
        existing.spec = spec;
    } else {
        watch.slots.push(Tracked {
            spec,
            probed: false,
            async_feed: false,
            last_poll_ms: now,
        });
    }
}

fn mark(world: &mut World, root: Entity, spec: &Spec, async_feed: bool) {
    let now = now_ms(world);
    let Some(watch) = world.get_mut::<FeedWatch>(root) else {
        return;
    };
    if let Some(slot) = watch
        .slots
        .iter_mut()
        .find(|slot| slot.spec.instance == spec.instance)
    {
        slot.probed = true;
        slot.async_feed = async_feed;
        if async_feed {
            slot.last_poll_ms = now;
        }
    }
}

fn place(world: &World, root: Entity) -> Option<(Entity, usize, usize)> {
    let info = world.get::<BlockRoot>(root)?;
    let view = world.get::<Parent>(root)?.entity;
    lifecycle::key(world, view, info.source)?;
    Some((view, info.source, info.index))
}

fn empty_slot(spec: &Spec) -> FeedSlot {
    FeedSlot {
        instance: spec.instance.clone(),
        feed_id: spec.feed_id.clone(),
        datatype: spec.datatype.as_str().to_string(),
        phase: FeedPhase::Ready,
        body: None,
        error: None,
        inflight: false,
    }
}

fn now_ms(world: &World) -> u64 {
    world
        .resource::<Time>()
        .map(|time| u64::try_from(time.now.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

fn commit_outcome(
    world: &mut World,
    view: Entity,
    source: usize,
    block: usize,
    spec: &Spec,
    outcome: &FeedOutcome,
) -> state::LiquidComponentRecord {
    let previous = stored(world, view, source, &spec.instance);
    let mut slot = previous.clone().unwrap_or_else(|| empty_slot(spec));
    slot.datatype = spec.datatype.as_str().to_string();
    match outcome {
        FeedOutcome::Defer => {}
        FeedOutcome::Pending => {
            slot.inflight = true;
            slot.phase = if slot.body.as_ref().is_some_and(|body| !body.is_empty()) {
                FeedPhase::Stale
            } else {
                FeedPhase::Loading
            };
        }
        FeedOutcome::Ready(FeedBody { datatype, body }) => {
            slot.inflight = false;
            slot.error = None;
            slot.datatype = datatype.as_str().to_string();
            slot.body = Some(body.clone());
            slot.phase = FeedPhase::Ready;
        }
        FeedOutcome::Failed { message } => {
            slot.inflight = false;
            slot.error = Some(message.clone());
            slot.phase = if slot.body.as_ref().is_some_and(|body| !body.is_empty()) {
                FeedPhase::Stale
            } else {
                FeedPhase::Error
            };
        }
    }
    upsert(world, view, source, block, slot)
}

#[cfg(test)]
pub(crate) fn counts(world: &World) -> (usize, usize, usize, usize) {
    (
        world.entities_with::<FeedWatch>().len(),
        world
            .resource::<Observations>()
            .map_or(0, |r| r.active.len()),
        world
            .resource::<Observations>()
            .map_or(0, |r| r.retiring.len()),
        world
            .resource::<Observations>()
            .map_or(0, |r| r.unacknowledged.len()),
    )
}

#[cfg(test)]
mod retirement_tests {
    use super::*;
    use crate::{liquid_view, set_liquid_source, LiquidHost, NoHost};
    const DOC: &str = "```feed\nid: news\ndatatype: text\n```\n";
    fn setup() -> (World, Entity, Key, usize) {
        let mut w = World::new();
        w.insert_resource(crate::view::HostSlot(Some(Box::new(NoHost))));
        let view = liquid_view(&mut w, DOC);
        let key = lifecycle::key_at(&w, view, 0).unwrap();
        let source = lifecycle::location(&w, key).unwrap().1;
        (w, view, key, source)
    }
    fn spec() -> Spec {
        Spec {
            instance: "feed".into(),
            feed_id: "news".into(),
            datatype: FeedDatatype::Text,
            field: None,
            autoload: true,
            interval_ms: None,
        }
    }
    #[test]
    fn retirement_budget_progresses_in_exact_finite_pumps() {
        let (mut w, view, key, _) = setup();
        w.insert_resource(crate::LiquidLimits {
            callbacks: 2,
            ..crate::LiquidLimits::default()
        });
        for i in 0..5 {
            observation(&mut w, key, &i.to_string(), false).unwrap();
        }
        state::begin_pump(&mut w);
        w.despawn_entity(view);
        retire_dead(&mut w);
        assert_eq!(counts(&w), (0, 0, 3, 0));
        // Repeated drains in one pump do not reset the shared budget.
        retire_dead(&mut w);
        assert_eq!(counts(&w), (0, 0, 3, 0));
        state::begin_pump(&mut w);
        retire_dead(&mut w);
        assert_eq!(counts(&w), (0, 0, 1, 0));
        state::begin_pump(&mut w);
        retire_dead(&mut w);
        assert_eq!(counts(&w), (0, 0, 0, 0));
    }

    #[test]
    fn retirement_callbacks_are_tree_postorder_then_reverse_registration() {
        struct Ordered(std::sync::Arc<std::sync::Mutex<Vec<u64>>>);
        impl LiquidHost for Ordered {
            fn retire_feed_observation(&mut self, _: &mut World, context: &LiquidFeedContext) {
                self.0.lock().unwrap().push(context.request_id());
                context.acknowledge_retirement();
            }
        }
        let (mut w, view, key, _) = setup();
        let base = observation(&mut w, key, "base", false).unwrap();
        w.insert_resource(Observations::default());
        let order = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        w.insert_resource(crate::view::HostSlot(Some(Box::new(Ordered(
            order.clone(),
        )))));
        let mut observations = Observations::default();
        // Explicit parent, A, A-child, B, B-child creation paths. Feed request
        // ids in each scope must retire in reverse registration, not ascending.
        for (request, path) in [
            (1, vec![1]),
            (2, vec![1, 2]),
            (3, vec![1, 2, 3]),
            (4, vec![1, 4]),
            (5, vec![1, 4, 5]),
            (6, vec![1, 2]),
        ] {
            let context = LiquidFeedContext {
                request,
                path,
                retired: Arc::new(AtomicBool::new(false)),
                acknowledged: Arc::new(AtomicBool::new(false)),
                ..base.clone()
            };
            observations.active.insert(
                (key, request.to_string()),
                Observation {
                    context,
                    pending: true,
                },
            );
        }
        w.insert_resource(observations);
        w.despawn_entity(view);
        retire_dead(&mut w);
        assert_eq!(*order.lock().unwrap(), vec![3, 6, 2, 5, 4, 1]);
        assert_eq!(counts(&w), (0, 0, 0, 0));
        retire_dead(&mut w);
        assert_eq!(*order.lock().unwrap(), vec![3, 6, 2, 5, 4, 1]);
    }

    #[test]
    fn publication_before_retirement_refresh_generation_and_foreign_world_reject() {
        let (mut w, view, key, source) = setup();
        let old = observation(&mut w, key, "feed", false).unwrap();
        let record = commit_outcome(&mut w, view, source, 0, &spec(), &FeedOutcome::Pending);
        put_liquid_feed_result(
            &mut w,
            &old,
            record.revision,
            FeedOutcome::Ready(FeedBody {
                datatype: FeedDatatype::Text,
                body: "last good".into(),
            }),
        )
        .unwrap();
        let before = state::liquid_component_state(&w, view, source).unwrap();
        let fresh = observation(&mut w, key, "feed", true).unwrap();
        assert!(old.is_retired());
        assert_ne!(old, fresh);
        assert!(put_liquid_feed_result(
            &mut w,
            &old,
            before.revision,
            FeedOutcome::Failed {
                message: "stale".into()
            }
        )
        .is_err());
        let (mut foreign, _, _, _) = setup();
        assert!(put_liquid_feed_result(
            &mut foreign,
            &fresh,
            before.revision,
            FeedOutcome::Pending
        )
        .is_err());
        set_liquid_source(&mut w, view, "");
        assert!(!lifecycle::block_live(&w, key));
        assert!(fresh.is_retired());
        assert!(
            put_liquid_feed_result(&mut w, &fresh, before.revision, FeedOutcome::Pending).is_err()
        );
        retire_dead(&mut w);
        assert_eq!(counts(&w), (0, 0, 0, 0));
    }
    #[test]
    fn exhausted_replacement_preserves_current_lane_and_legacy_pending_is_not_quiescent() {
        let (mut w, view, key, _) = setup();
        let old = observation(&mut w, key, "feed", false).unwrap();
        let r = w.resource_mut::<Observations>().unwrap();
        r.active.get_mut(&(key, "feed".into())).unwrap().pending = true;
        r.next = u64::MAX;
        assert!(observation(&mut w, key, "feed", true).is_none());
        assert!(!old.is_retired());
        assert_eq!(counts(&w).1, 1);
        w.remove::<crate::LiquidView>(view);
        retire_dead(&mut w);
        assert!(old.is_retired());
        assert_eq!(counts(&w), (0, 0, 0, 1)); // default legacy retirement cannot promise a stopped worker
        old.acknowledge_retirement();
        retire_dead(&mut w);
        assert_eq!(counts(&w), (0, 0, 0, 0));
    }
    #[test]
    fn reentrant_retirement_and_panicking_host_do_not_lose_later_notifications() {
        struct Host {
            seen: std::sync::Arc<std::sync::atomic::AtomicUsize>,
        }
        impl LiquidHost for Host {
            fn retire_feed_observation(&mut self, _: &mut World, c: &LiquidFeedContext) {
                let count = self.seen.fetch_add(1, Ordering::SeqCst);
                c.acknowledge_retirement();
                if count == 0 {
                    panic!("retirement panic");
                }
            }
        }
        let (mut w, view, key, _) = setup();
        let seen = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        w.insert_resource(crate::view::HostSlot(Some(Box::new(Host {
            seen: seen.clone(),
        }))));
        let one = observation(&mut w, key, "one", false).unwrap();
        let two = observation(&mut w, key, "two", false).unwrap();
        w.remove::<crate::LiquidView>(view);
        crate::view::with_host(&mut w, |_, w| {
            retire_dead(w);
            assert!(one.is_retired() && two.is_retired());
            assert_eq!(seen.load(Ordering::SeqCst), 0);
        });
        retire_dead(&mut w);
        assert_eq!(seen.load(Ordering::SeqCst), 2);
        assert_eq!(counts(&w), (0, 0, 0, 0));
    }
}
