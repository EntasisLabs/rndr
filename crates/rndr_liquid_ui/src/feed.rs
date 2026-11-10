//! Async feed loads for `feed` blocks and dashboard tiles.
//!
//! The host supplies bytes through [`LiquidHost::load_feed`]. A later failure
//! keeps the last ready body. The record is the component state for that
//! source, so it survives host-slot recycling. Interval polls run only while
//! the block is mounted.
use crate::host::{FeedBody, FeedDatatype, FeedEntry, FeedOutcome, FeedRequest, LiquidEvent};
use crate::render;
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
    let outcome = view::with_host(world, |host, world| host.load_feed(world, &request));
    if matches!(outcome, FeedOutcome::Defer) {
        mark(world, root, spec, false);
        return FeedOutcome::Defer;
    }
    mark(world, root, spec, true);
    let previous = stored(world, view, source, &spec.instance);
    let mut slot = previous.clone().unwrap_or_else(|| empty_slot(spec));
    slot.datatype = spec.datatype.as_str().to_string();
    match &outcome {
        FeedOutcome::Defer => return FeedOutcome::Defer,
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
    let record = upsert(world, view, source, block, slot);
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
