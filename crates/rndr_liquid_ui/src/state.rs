//! Revisioned component state and the interaction envelope.
//!
//! Accordion, tabs, checklists, carousels and slides keep a record keyed by
//! the view and the block's source byte. Recycling and streaming appends leave
//! that key in place, so a host can read the state while the block is
//! offscreen and see it restored when the block returns. Each reader change
//! also publishes a [`LiquidInteractionEnvelope`].
use rndr_core::{Entity, World};
use std::collections::HashMap;

/// Where an interaction should be routed.
///
/// Component changes are [`LocalState`]. The other variants match the
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
}

impl LiquidInteraction {
    /// Stable name used in the envelope, matching the scene-event vocabulary.
    pub fn name(self) -> &'static str {
        match self {
            Self::Select => "select",
            Self::Expand => "expand",
            Self::Collapse => "collapse",
            Self::Edit => "edit",
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
    records: HashMap<(Entity, usize), LiquidComponentRecord>,
}

#[derive(Default)]
struct Listeners {
    handlers: Vec<Box<dyn LiquidInteractionHandler>>,
}

/// Subscribe `handler` to later interactions. Earlier subscribers are called first.
pub fn subscribe_liquid_interactions(
    world: &mut World,
    handler: impl LiquidInteractionHandler + 'static,
) {
    if world.resource_mut::<Listeners>().is_none() {
        world.insert_resource(Listeners::default());
    }
    world
        .resource_mut::<Listeners>()
        .unwrap()
        .handlers
        .push(Box::new(handler));
}

/// Every component record stored for `view`, in source order.
pub fn liquid_component_states(world: &World, view: Entity) -> Vec<LiquidComponentRecord> {
    let mut records: Vec<_> = world
        .resource::<ComponentStore>()
        .map(|store| {
            store
                .records
                .iter()
                .filter(|((owner, _), _)| *owner == view)
                .map(|(_, record)| record.clone())
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
    world
        .resource::<ComponentStore>()
        .and_then(|store| store.records.get(&(view, source)).cloned())
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
    let Some(mut listeners) = world
        .resource_mut::<Listeners>()
        .map(|listeners| std::mem::take(&mut listeners.handlers))
    else {
        return;
    };
    for handler in listeners.iter_mut() {
        handler.on_interaction(world, &envelope);
    }
    match world.resource_mut::<Listeners>() {
        Some(slot) => slot.handlers = listeners,
        None => world.insert_resource(Listeners {
            handlers: listeners,
        }),
    }
}

fn write(world: &mut World, view: Entity, record: LiquidComponentRecord) {
    if world.resource_mut::<ComponentStore>().is_none() {
        world.insert_resource(ComponentStore::default());
    }
    world
        .resource_mut::<ComponentStore>()
        .unwrap()
        .records
        .insert((view, record.source), record);
}

fn component_type_of(state: &LiquidComponentState) -> &'static str {
    match state {
        LiquidComponentState::Accordion { .. } => "accordion",
        LiquidComponentState::Tabs { .. } => "tabs",
        LiquidComponentState::Checklist { .. } => "checklist",
        LiquidComponentState::Selection { .. } => "selection",
    }
}
