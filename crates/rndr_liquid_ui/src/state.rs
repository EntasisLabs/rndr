//! Revisioned component state and the interaction envelope.
//!
//! Accordion, tabs, checklists, carousels and slides keep a record keyed by
//! the view and the block's source byte. Recycling and streaming appends leave
//! that key in place, so a host can read the state while the block is
//! offscreen and see it restored when the block returns. Each reader change
//! also publishes a [`LiquidInteractionEnvelope`].
use crate::lifecycle::{self, Key, LiquidSurfaceHandle};
use rndr_core::{Entity, World};
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

/// Where an interaction should be routed.
///
/// Component changes are [`LiquidDisposition::LocalState`]. The other variants match the
/// dispositions a host already has to tell apart, and are available for
/// events that leave the view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LiquidDisposition {
    /// Update the component record and do not start a turn.
    LocalState,
    /// Tell the host, without changing component state.
    ContextOnly,
    /// The reader submitted something the host may turn into work.
    SubmitTurn,
    /// The reader asked to move somewhere else.
    Navigation,
    /// An event the host must not treat as ordinary context.
    PrivilegedAction,
}

/// What the reader did to a stateful Liquid component.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LiquidInteraction {
    /// A tab, slide, carousel item, or checklist row was chosen.
    Select,
    /// An accordion section opened.
    Expand,
    /// An accordion section closed.
    Collapse,
    /// A checklist mark changed.
    Edit,
    /// A recipe timer started or resumed.
    TimerStart,
    /// A running recipe timer was paused.
    TimerPause,
    /// A recipe timer returned to idle.
    TimerReset,
    /// A recipe timer reached zero.
    TimerComplete,
    /// A feed or dashboard tile started or finished a reload.
    FeedRefresh,
}

impl LiquidInteraction {
    /// Stable name used in the envelope, matching the scene-event vocabulary.
    pub fn name(self) -> &'static str {
        match self {
            Self::Select => "select",
            Self::Expand => "expand",
            Self::Collapse => "collapse",
            Self::Edit => "edit",
            Self::TimerStart => "timer_start",
            Self::TimerPause => "timer_pause",
            Self::TimerReset => "timer_reset",
            Self::TimerComplete => "timer_complete",
            Self::FeedRefresh => "feed_refresh",
        }
    }
}

/// Reader state of one stateful block.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LiquidComponentState {
    /// Sections that are open. Indexes are into the block's items.
    Accordion {
        /// Open section indexes.
        open: Vec<usize>,
    },
    /// The visible tab.
    Tabs {
        /// Panel index.
        selected: usize,
    },
    /// Checked rows of a steps, recipe, or kanban block.
    Checklist {
        /// Checked row indexes.
        checked: Vec<usize>,
    },
    /// The visible carousel item or slide.
    Selection {
        /// Item index.
        selected: usize,
    },
    /// Async feed and dashboard-tile loads for one block.
    Feeds {
        /// One entry per feed id or tile id.
        slots: Vec<FeedSlot>,
    },
}

/// Where an async feed load is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FeedPhase {
    /// No last-good body yet, and a load is running.
    Loading,
    /// The latest load succeeded.
    Ready,
    /// The latest load failed and there is no body to keep.
    Error,
    /// A later load failed or is still running, and the previous body is kept.
    Stale,
}

/// One feed or tile inside [`LiquidComponentState::Feeds`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FeedSlot {
    /// Feed id, or dashboard tile id.
    pub instance: String,
    /// Feed id the host was asked to load.
    pub feed_id: String,
    /// `md`, `text`, `json`, `csv`, or `image`.
    pub datatype: String,
    /// Load phase.
    pub phase: FeedPhase,
    /// Last successful body.
    pub body: Option<String>,
    /// Latest failure, when [`Self::phase`] is [`FeedPhase::Error`] or [`FeedPhase::Stale`].
    pub error: Option<String>,
    /// A load is still running.
    pub inflight: bool,
}

/// One persisted component record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiquidComponentRecord {
    /// Schema of [`Self::state`]. This crate writes `1`.
    pub schema_version: u32,
    /// Source byte of the block's reference, stable across a streaming append.
    pub source: usize,
    /// Index into the view's parsed block list when the record was written.
    pub block: usize,
    /// `accordion`, `tabs`, `steps`, `recipe`, `kanban`, `carousel`, or `slides`.
    pub component_type: String,
    /// Monotonic revision. A put that names a different revision is rejected.
    pub revision: u64,
    /// The reader state.
    pub state: LiquidComponentState,
}

/// A component interaction a host subscribed to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiquidInteractionEnvelope {
    /// Envelope schema. This crate writes `1`.
    pub version: u8,
    /// View whose block emitted the event.
    pub view: Entity,
    /// Source byte of that block.
    pub source: usize,
    /// Component type, the same string as [`LiquidComponentRecord::component_type`].
    pub node_id: String,
    /// Child inside the block, such as an accordion section index.
    pub instance_id: String,
    /// What the reader did.
    pub event: LiquidInteraction,
    /// How the host should treat it.
    pub disposition: LiquidDisposition,
    /// Component revision after the change.
    pub revision: u64,
}

/// The stored revision is not the one the caller expected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LiquidRevisionError {
    /// Revision the caller sent.
    pub expected: u64,
    /// Revision currently stored. Zero means no record yet.
    pub actual: u64,
}

/// Receives [`LiquidInteractionEnvelope`] values.
pub trait LiquidInteractionHandler: Send + Sync {
    /// One interaction was committed.
    fn on_interaction(&mut self, world: &mut World, envelope: &LiquidInteractionEnvelope);
}

#[derive(Default)]
pub(crate) struct ComponentStore {
    records: HashMap<Key, LiquidComponentRecord>,
}

/// Owner of an additive removable interaction subscription.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LiquidListenerOwner {
    /// Deliberately application-global, until token removal or World drop.
    World,
    /// Retired with exactly this logical surface, including while offscreen.
    Surface(LiquidSurfaceHandle),
}
/// A removable subscription. Drop only signals; no World access or callback.
#[must_use]
#[derive(Debug)]
pub struct LiquidInteractionSubscription {
    cancelled: Arc<AtomicBool>,
}
impl Drop for LiquidInteractionSubscription {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
    }
}
/// Refused subscription with the untouched handler for caller cleanup.
pub struct LiquidSubscribeError<H> {
    /// Why admission was refused.
    pub reason: crate::LiquidUpdateError,
    /// Handler still owned by the caller.
    pub handler: H,
}
impl<H> std::fmt::Debug for LiquidSubscribeError<H> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LiquidSubscribeError")
            .field("reason", &self.reason)
            .finish_non_exhaustive()
    }
}
struct Listener {
    owner: LiquidListenerOwner,
    cancelled: Arc<AtomicBool>,
    handler: Option<Box<dyn LiquidInteractionHandler>>,
}
struct Delivery {
    envelope: LiquidInteractionEnvelope,
    key: Option<Key>,
    ids: Vec<u64>,
    cursor: usize,
}
struct Listeners {
    handlers: BTreeMap<u64, Listener>,
    next: u64,
    queue: VecDeque<Delivery>,
    dispatching: bool,
    remaining: usize,
    panicked: u64,
}
impl Default for Listeners {
    fn default() -> Self {
        Self {
            handlers: BTreeMap::new(),
            next: 1,
            queue: VecDeque::new(),
            dispatching: false,
            remaining: 256,
            panicked: 0,
        }
    }
}
fn listeners(world: &mut World) -> &mut Listeners {
    if !world.has_resource::<Listeners>() {
        let remaining = lifecycle::limits(world).callbacks;
        world.insert_resource(Listeners {
            remaining,
            ..Listeners::default()
        });
    }
    world.resource_mut::<Listeners>().unwrap()
}
/// Subscribe globally for the World lifetime. Prefer the owned API for UI work.
/// Callbacks added during dispatch start with the next envelope.
pub fn subscribe_liquid_interactions(
    world: &mut World,
    handler: impl LiquidInteractionHandler + 'static,
) {
    // This legacy API deliberately has no removable lifetime token.
    let slot = listeners(world);
    let Some(next) = slot.next.checked_add(1) else {
        return;
    };
    let id = slot.next;
    slot.next = next;
    slot.handlers.insert(
        id,
        Listener {
            owner: LiquidListenerOwner::World,
            cancelled: Arc::new(AtomicBool::new(false)),
            handler: Some(Box::new(handler)),
        },
    );
}
/// Register without running callbacks. Refusal returns the untouched handler.
pub fn subscribe_liquid_interactions_owned<H: LiquidInteractionHandler + 'static>(
    world: &mut World,
    owner: LiquidListenerOwner,
    handler: H,
) -> Result<LiquidInteractionSubscription, LiquidSubscribeError<H>> {
    let reason = match owner {
        LiquidListenerOwner::Surface(id) if !lifecycle::live(world, id.0) => {
            Some(crate::LiquidUpdateError::StaleOwner)
        }
        _ if world.resource::<Listeners>().map_or(0, |r| {
            r.handlers.values().filter(|l| l.owner == owner).count()
        }) >= lifecycle::limits(world).listeners =>
        {
            Some(crate::LiquidUpdateError::Capacity)
        }
        _ if listeners(world).next == u64::MAX => Some(crate::LiquidUpdateError::Exhausted),
        _ => None,
    };
    if let Some(reason) = reason {
        return Err(LiquidSubscribeError { reason, handler });
    }
    let cancelled = Arc::new(AtomicBool::new(false));
    let slot = listeners(world);
    let id = slot.next;
    slot.next += 1;
    slot.handlers.insert(
        id,
        Listener {
            owner,
            cancelled: cancelled.clone(),
            handler: Some(Box::new(handler)),
        },
    );
    Ok(LiquidInteractionSubscription { cancelled })
}
/// Signal and physically remove the subscription outside active resource borrows.
pub fn unsubscribe_liquid_interactions(world: &mut World, token: LiquidInteractionSubscription) {
    drop(token);
    drain_listeners(world);
}
fn listener_live(world: &World, listener: &Listener) -> bool {
    !listener.cancelled.load(Ordering::Acquire)
        && match listener.owner {
            LiquidListenerOwner::World => true,
            LiquidListenerOwner::Surface(id) => lifecycle::live(world, id.0),
        }
}
pub(crate) fn drain_listeners(world: &mut World) {
    let dead: Vec<u64> = world
        .resource::<Listeners>()
        .map(|r| {
            r.handlers
                .iter()
                .filter(|(_, l)| !listener_live(world, l))
                .map(|(id, _)| *id)
                .collect()
        })
        .unwrap_or_default();
    let removed: Vec<_> = if let Some(r) = world.resource_mut::<Listeners>() {
        dead.into_iter()
            .filter_map(|id| r.handlers.remove(&id))
            .collect()
    } else {
        Vec::new()
    };
    drop(removed);
}
pub(crate) fn begin_pump(world: &mut World) {
    listeners(world).remaining = lifecycle::limits(world).callbacks;
    drain_listeners(world);
    dispatch_pending(world);
}
pub(crate) fn can_emit(world: &World) -> bool {
    world
        .resource::<Listeners>()
        .is_none_or(|r| r.queue.len() < lifecycle::limits(world).envelopes)
}

/// Every component record stored for `view`, in source order.
pub fn liquid_component_states(world: &World, view: Entity) -> Vec<LiquidComponentRecord> {
    let mut records: Vec<_> = world
        .resource::<ComponentStore>()
        .map(|store| {
            store
                .records
                .iter()
                .filter_map(|(key, record)| {
                    let (owner, source, index) = lifecycle::location(world, *key)?;
                    if owner != view {
                        return None;
                    }
                    let mut record = record.clone();
                    record.source = source;
                    record.block = index;
                    Some(record)
                })
                .collect()
        })
        .unwrap_or_default();
    records.sort_by_key(|record| record.source);
    records
}

/// The record for `view` at `source`, if the reader has changed that block.
pub fn liquid_component_state(
    world: &World,
    view: Entity,
    source: usize,
) -> Option<LiquidComponentRecord> {
    let key = lifecycle::key(world, view, source)?;
    let mut record = world
        .resource::<ComponentStore>()?
        .records
        .get(&key)?
        .clone();
    let (_, _, index) = lifecycle::location(world, key)?;
    record.source = source;
    record.block = index;
    Some(record)
}

/// Replace the record at `source` when `expected` is its current revision.
///
/// A missing record has revision zero. On success the revision becomes
/// `expected + 1`. A mounted block is rebuilt from the new state.
pub fn put_liquid_component_state(
    world: &mut World,
    view: Entity,
    source: usize,
    expected: u64,
    state: LiquidComponentState,
) -> Result<LiquidComponentRecord, LiquidRevisionError> {
    if lifecycle::key(world, view, source).is_none() || expected == u64::MAX {
        return Err(LiquidRevisionError {
            expected,
            actual: 0,
        });
    }
    let actual = liquid_component_state(world, view, source)
        .map(|record| record.revision)
        .unwrap_or(0);
    if expected != actual {
        return Err(LiquidRevisionError { expected, actual });
    }
    let previous = liquid_component_state(world, view, source);
    let record = LiquidComponentRecord {
        schema_version: 1,
        source,
        block: previous.as_ref().map(|record| record.block).unwrap_or(0),
        component_type: previous
            .as_ref()
            .map(|record| record.component_type.clone())
            .unwrap_or_else(|| component_type_of(&state).to_string()),
        revision: actual + 1,
        state,
    };
    write(world, view, record.clone());
    crate::view::apply_stored_state(world, view, &record);
    Ok(record)
}

pub(crate) fn store_snapshot(
    world: &mut World,
    view: Entity,
    source: usize,
    block: usize,
    component_type: &str,
    state: LiquidComponentState,
) -> LiquidComponentRecord {
    if let Some(existing) = liquid_component_state(world, view, source) {
        if existing.state == state && existing.component_type == component_type {
            return existing;
        }
    }
    if let Some(record) = liquid_component_state(world, view, source) {
        if record.revision == u64::MAX {
            return record;
        }
    }
    let revision = liquid_component_state(world, view, source)
        .map(|record| record.revision + 1)
        .unwrap_or(1);
    let record = LiquidComponentRecord {
        schema_version: 1,
        source,
        block,
        component_type: component_type.to_string(),
        revision,
        state,
    };
    write(world, view, record.clone());
    record
}

pub(crate) fn emit(world: &mut World, envelope: LiquidInteractionEnvelope) {
    if !can_emit(world) {
        return;
    } // Callers reserve capacity before committing.
    let key = lifecycle::key(world, envelope.view, envelope.source);
    if key.is_none() {
        return;
    }
    let ids = listeners(world).handlers.keys().copied().collect();
    listeners(world).queue.push_back(Delivery {
        envelope,
        key,
        ids,
        cursor: 0,
    });
    dispatch_pending(world);
}
fn dispatch_pending(world: &mut World) {
    if listeners(world).dispatching {
        return;
    }
    listeners(world).dispatching = true;
    loop {
        let Some(mut delivery) = listeners(world).queue.pop_front() else {
            break;
        };
        while delivery.cursor < delivery.ids.len() {
            if delivery
                .key
                .is_some_and(|key| !lifecycle::block_live(world, key))
            {
                break;
            }
            if listeners(world).remaining == 0 {
                break;
            }
            let id = delivery.ids[delivery.cursor];
            delivery.cursor += 1;
            let live = world
                .resource::<Listeners>()
                .and_then(|r| r.handlers.get(&id))
                .is_some_and(|l| {
                    listener_live(world, l)
                        && match l.owner {
                            LiquidListenerOwner::World => true,
                            LiquidListenerOwner::Surface(owner) => {
                                delivery.key.is_some_and(|key| key.0 == owner.0)
                            }
                        }
                });
            if !live {
                continue;
            }
            let handler = listeners(world)
                .handlers
                .get_mut(&id)
                .and_then(|l| l.handler.take());
            let Some(mut handler) = handler else {
                continue;
            };
            listeners(world).remaining -= 1;
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                handler.on_interaction(world, &delivery.envelope)
            }));
            if result.is_err() {
                let r = listeners(world);
                r.panicked = r.panicked.saturating_add(1);
                if let Some(l) = r.handlers.get(&id) {
                    l.cancelled.store(true, Ordering::Release);
                }
            } else if let Some(l) = listeners(world).handlers.get_mut(&id) {
                l.handler = Some(handler);
            }
            // A detached handler is destroyed only after releasing the resource borrow.
            drain_listeners(world);
        }
        if delivery.cursor < delivery.ids.len()
            && delivery
                .key
                .is_some_and(|key| lifecycle::block_live(world, key))
        {
            listeners(world).queue.push_front(delivery);
            break;
        }
    }
    listeners(world).dispatching = false;
    drain_listeners(world);
}
fn write(world: &mut World, view: Entity, record: LiquidComponentRecord) {
    let Some(key) = lifecycle::key(world, view, record.source) else {
        return;
    };
    if !world.has_resource::<ComponentStore>() {
        world.insert_resource(ComponentStore::default());
    }
    world
        .resource_mut::<ComponentStore>()
        .unwrap()
        .records
        .insert(key, record);
}
pub(crate) fn retain_live(world: &mut World) {
    let dead: Vec<Key> = world
        .resource::<ComponentStore>()
        .map(|r| {
            r.records
                .keys()
                .copied()
                .filter(|key| !lifecycle::block_live(world, *key))
                .collect()
        })
        .unwrap_or_default();
    if let Some(r) = world.resource_mut::<ComponentStore>() {
        for key in dead {
            r.records.remove(&key);
        }
    }
}

fn component_type_of(state: &LiquidComponentState) -> &'static str {
    match state {
        LiquidComponentState::Accordion { .. } => "accordion",
        LiquidComponentState::Tabs { .. } => "tabs",
        LiquidComponentState::Checklist { .. } => "checklist",
        LiquidComponentState::Selection { .. } => "selection",
        LiquidComponentState::Feeds { .. } => "feed",
    }
}

pub(crate) fn take_callback_budget(world: &mut World) -> bool {
    let r = listeners(world);
    if r.remaining == 0 {
        return false;
    }
    r.remaining -= 1;
    true
}
#[cfg(test)]
pub(crate) fn counts(world: &World) -> (usize, usize, usize, usize) {
    (
        world
            .resource::<ComponentStore>()
            .map_or(0, |r| r.records.len()),
        world.resource::<Listeners>().map_or(0, |r| {
            r.handlers
                .values()
                .filter(|l| matches!(l.owner, LiquidListenerOwner::Surface(_)))
                .count()
        }),
        world.resource::<Listeners>().map_or(0, |r| {
            r.handlers
                .values()
                .filter(|l| matches!(l.owner, LiquidListenerOwner::World))
                .count()
        }),
        world.resource::<Listeners>().map_or(0, |r| r.queue.len()),
    )
}

#[cfg(test)]
mod lifecycle_tests {
    use super::*;
    use crate::{liquid_view, LiquidLimits};
    use std::sync::Mutex;
    const DOC: &str = "```accordion\ntitle: FAQ\n---\nlabel: One\nbody: Body\n```\n";
    struct Handler<F>(F);
    impl<F: FnMut(&mut World, &LiquidInteractionEnvelope) + Send + Sync> LiquidInteractionHandler
        for Handler<F>
    {
        fn on_interaction(&mut self, w: &mut World, e: &LiquidInteractionEnvelope) {
            (self.0)(w, e);
        }
    }
    #[derive(Default)]
    struct Tokens(Vec<LiquidInteractionSubscription>);
    fn event(w: &World, view: Entity, revision: u64) -> LiquidInteractionEnvelope {
        LiquidInteractionEnvelope {
            version: 1,
            view,
            source: lifecycle::location(w, lifecycle::key_at(w, view, 0).unwrap())
                .unwrap()
                .1,
            node_id: "accordion".into(),
            instance_id: "0".into(),
            event: LiquidInteraction::Expand,
            disposition: LiquidDisposition::LocalState,
            revision,
        }
    }
    fn subscribe<F: FnMut(&mut World, &LiquidInteractionEnvelope) + Send + Sync + 'static>(
        world: &mut World,
        f: F,
    ) -> LiquidInteractionSubscription {
        subscribe_liquid_interactions_owned(world, LiquidListenerOwner::World, Handler(f))
            .unwrap_or_else(|_| panic!("subscribe"))
    }
    #[test]
    fn id_snapshot_add_remove_self_remove_and_nested_fifo() {
        let mut w = World::new();
        let view = liquid_view(&mut w, DOC);
        w.insert_resource(Tokens::default());
        let log = Arc::new(Mutex::new(Vec::new()));
        let l = log.clone();
        let first = subscribe(&mut w, move |w, e| {
            l.lock().unwrap().push(("a", e.revision));
            let removed = std::mem::take(&mut w.resource_mut::<Tokens>().unwrap().0);
            drop(removed);
            let l = l.clone();
            let next = subscribe(w, move |_, e| l.lock().unwrap().push(("d", e.revision)));
            w.resource_mut::<Tokens>().unwrap().0.push(next);
            {
                let e = event(w, view, 2);
                emit(w, e);
            };
        });
        let l = log.clone();
        let second = subscribe(&mut w, move |_, e| {
            l.lock().unwrap().push(("b", e.revision))
        });
        let l = log.clone();
        let third = subscribe(&mut w, move |_, e| {
            l.lock().unwrap().push(("c", e.revision))
        });
        w.resource_mut::<Tokens>()
            .unwrap()
            .0
            .extend([first, second]);
        {
            let e = event(&w, view, 1);
            emit(&mut w, e);
        };
        assert_eq!(
            *log.lock().unwrap(),
            vec![("a", 1), ("c", 1), ("c", 2), ("d", 2)]
        );
        assert_eq!(counts(&w).1, 0);
        assert_eq!(counts(&w).2, 2);
        drop(third);
        drain_listeners(&mut w);
        assert_eq!(counts(&w).2, 1);
    }
    #[test]
    fn surface_filter_foreign_handle_disposal_and_panics_fail_closed() {
        let mut w = World::new();
        let view = liquid_view(&mut w, DOC);
        let other = liquid_view(&mut w, DOC);
        let owner = lifecycle::liquid_surface_handle(&w, view).unwrap();
        let other_owner = lifecycle::liquid_surface_handle(&w, other).unwrap();
        let count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let c = count.clone();
        let token = subscribe_liquid_interactions_owned(
            &mut w,
            LiquidListenerOwner::Surface(other_owner),
            Handler(move |_: &mut World, _: &LiquidInteractionEnvelope| {
                c.fetch_add(1, Ordering::SeqCst);
            }),
        )
        .unwrap_or_else(|_| panic!());
        {
            let e = event(&w, view, 1);
            emit(&mut w, e);
        };
        assert_eq!(count.load(Ordering::SeqCst), 0);
        let panic_token = subscribe(&mut w, move |_, _| panic!("intentional listener failure"));
        let log = Arc::new(Mutex::new(Vec::new()));
        let l = log.clone();
        let good = subscribe(&mut w, move |_, e| l.lock().unwrap().push(e.revision));
        {
            let e = event(&w, view, 2);
            emit(&mut w, e);
        };
        {
            let e = event(&w, view, 3);
            emit(&mut w, e);
        };
        assert_eq!(*log.lock().unwrap(), vec![2, 3]);
        assert_eq!(w.resource::<Listeners>().unwrap().panicked, 1);
        let mut foreign = World::new();
        let foreign_view = liquid_view(&mut foreign, DOC);
        assert_eq!(view, foreign_view); // entity equality confers no authority
        assert!(subscribe_liquid_interactions_owned(
            &mut foreign,
            LiquidListenerOwner::Surface(owner),
            Handler(|_: &mut World, _: &LiquidInteractionEnvelope| {})
        )
        .is_err());
        let dispose = subscribe_liquid_interactions_owned(
            &mut w,
            LiquidListenerOwner::Surface(owner),
            Handler(move |w: &mut World, _: &LiquidInteractionEnvelope| {
                w.remove::<crate::LiquidView>(view);
            }),
        )
        .unwrap_or_else(|_| panic!());
        let c = count.clone();
        let late = subscribe_liquid_interactions_owned(
            &mut w,
            LiquidListenerOwner::Surface(owner),
            Handler(move |_: &mut World, _: &LiquidInteractionEnvelope| {
                c.fetch_add(1, Ordering::SeqCst);
            }),
        )
        .unwrap_or_else(|_| panic!());
        {
            let e = event(&w, view, 4);
            emit(&mut w, e);
        };
        assert_eq!(count.load(Ordering::SeqCst), 0);
        lifecycle::drain(&mut w);
        assert_eq!(counts(&w).1, 1); // unrelated owner's listener remains
        drop((token, panic_token, good, dispose, late));
    }
    #[test]
    fn queue_capacity_and_persistent_budget_have_exact_pump_counts() {
        let mut w = World::new();
        w.insert_resource(LiquidLimits {
            callbacks: 2,
            envelopes: 2,
            ..LiquidLimits::default()
        });
        let view = liquid_view(&mut w, DOC);
        let count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let mut tokens = Vec::new();
        for _ in 0..3 {
            let c = count.clone();
            tokens.push(subscribe(&mut w, move |_, _| {
                c.fetch_add(1, Ordering::SeqCst);
            }));
        }
        {
            let e = event(&w, view, 1);
            emit(&mut w, e);
        };
        assert_eq!(count.load(Ordering::SeqCst), 2);
        assert_eq!(counts(&w).3, 1);
        {
            let e = event(&w, view, 2);
            emit(&mut w, e);
        };
        assert_eq!(counts(&w).3, 2);
        assert!(!can_emit(&w));
        let prior = counts(&w);
        {
            let e = event(&w, view, 3);
            emit(&mut w, e);
        };
        assert_eq!(counts(&w), prior);
        begin_pump(&mut w);
        assert_eq!(count.load(Ordering::SeqCst), 4);
        assert_eq!(counts(&w).3, 1);
        begin_pump(&mut w);
        assert_eq!(count.load(Ordering::SeqCst), 6);
        assert_eq!(counts(&w).3, 0);
        drop(tokens);
        drain_listeners(&mut w);
        assert_eq!(counts(&w).2, 0);
    }
    #[test]
    fn listener_capacity_and_id_exhaustion_return_untouched_handler() {
        let mut w = World::new();
        w.insert_resource(LiquidLimits {
            listeners: 1,
            ..LiquidLimits::default()
        });
        let one = subscribe(&mut w, |_, _| {});
        let result = subscribe_liquid_interactions_owned(
            &mut w,
            LiquidListenerOwner::World,
            Handler(|_: &mut World, _: &LiquidInteractionEnvelope| {}),
        );
        assert!(matches!(
            result,
            Err(LiquidSubscribeError {
                reason: crate::LiquidUpdateError::Capacity,
                ..
            })
        ));
        drop(one);
        drain_listeners(&mut w);
        listeners(&mut w).next = u64::MAX;
        let result = subscribe_liquid_interactions_owned(
            &mut w,
            LiquidListenerOwner::World,
            Handler(|_: &mut World, _: &LiquidInteractionEnvelope| {}),
        );
        assert!(matches!(
            result,
            Err(LiquidSubscribeError {
                reason: crate::LiquidUpdateError::Exhausted,
                ..
            })
        ));
        assert_eq!(counts(&w).2, 0);
    }
}
