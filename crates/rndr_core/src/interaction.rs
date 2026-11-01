//! Additive interaction policy. Legacy widgets do not opt in implicitly.
use crate::*;
use std::collections::VecDeque;

/// Per-button release activation; omit to retain legacy pointer-press/Enter.
#[derive(Clone, Copy, Debug, Default)]
pub struct ReleaseActivation;
impl Component for ReleaseActivation {}

/// Explicit persistent name, supplementary description and readonly state.
/// Placeholder and current value are never used as persistent names.
#[derive(Clone, Debug, Default)]
pub struct Semantics {
    /// Explicit name wins over visible label composition.
    pub name: Option<String>,
    /// Supplementary help, not a substitute for a name.
    pub description: String,
    /// Mutation and activation are denied; focus is still permitted.
    pub readonly: bool,
}
impl Component for Semantics {}

/// Real tab/panel relation, independent of the physical hierarchy.
/// The tab also carries `Tab`; group carries `Tabs`, panel carries `TabPanel`.
#[derive(Clone, Copy, Debug)]
pub struct TabRelation {
    /// Named tablist entity.
    pub group: Entity,
    /// Associated real panel, excluded while inactive.
    pub panel: Entity,
}
impl Component for TabRelation {}

/// Supplemental accessibility data without changing `AccessNode` literals.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AccessDetails {
    /// Supplementary description.
    pub description: String,
    /// Readonly, distinct from disabled.
    pub readonly: bool,
    /// Permitted focus action.
    pub focus: bool,
    /// Permitted activation action.
    pub activate: bool,
    /// SetValue is deliberately unsupported until validated history integration.
    pub set_value: bool,
    /// True for a real tab panel (platform role TabPanel).
    pub panel: bool,
    /// Tab controls this panel's access id.
    pub controls: Option<u64>,
    /// Panel is labelled by this tab's access id.
    pub labelled_by: Option<u64>,
}

/// Validate generation and every ancestor's visibility.
pub fn interaction_visible(world: &World, mut entity: Entity) -> bool {
    for _ in 0..256 {
        if !world.is_alive(entity) || world.has::<Hidden>(entity) {
            return false;
        }
        match world.get::<Parent>(entity) {
            Some(parent) => entity = parent.entity,
            None => return true,
        }
    }
    false // malformed/cyclic hierarchy is not actionable
}

/// Current allowed-action matrix; platform advertising and dispatch share it.
pub fn access_details(world: &World, entity: Entity) -> AccessDetails {
    if !interaction_visible(world, entity) {
        return AccessDetails::default();
    }
    let semantics = world.get::<Semantics>(entity).cloned().unwrap_or_default();
    let enabled = world.get::<Button>(entity).is_none_or(|b| b.enabled);
    let relation = world.get::<TabRelation>(entity).filter(|r| {
        world.has::<Tabs>(r.group) && world.has::<TabPanel>(r.panel) && world.is_alive(r.panel)
    });
    AccessDetails {
        description: semantics.description,
        readonly: semantics.readonly,
        focus: enabled
            && (world.has::<Focusable>(entity)
                || world.has::<Button>(entity)
                || world.has::<TextField>(entity)
                || world.has::<Prompt>(entity)
                || world.has::<Editor>(entity)
                || world.has::<List>(entity)
                || world.has::<Tab>(entity)),
        activate: enabled
            && !semantics.readonly
            && (world.has::<Button>(entity)
                || world.has::<List>(entity)
                || world.has::<ListItem>(entity)
                || world.has::<Disclosure>(entity)
                || (world.has::<Tab>(entity)
                    && (relation.is_some() || crate::tabs::tab_group(world, entity).is_some()))),
        set_value: false,
        panel: world.has::<TabPanel>(entity),
        controls: relation.map(|r| access_id(r.panel)),
        labelled_by: world
            .entities_with::<TabRelation>()
            .into_iter()
            .find(|tab| {
                world
                    .get::<TabRelation>(*tab)
                    .is_some_and(|r| r.panel == entity)
            })
            .map(access_id),
    }
}

/// Canonical intent from an opted-in button. Never also copied into `Fired`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActionIntent {
    /// Generation-checked source entity.
    pub entity: Entity,
    /// Opaque host action; never executed by core.
    pub name: String,
}

/// Explicit bounded pending-intent policy: one outstanding intent per target.
/// Hosts drain on Update. No eviction, overwrite, or implicit Fired fanout.
#[derive(Default, Debug)]
pub struct ActionIntents {
    pending: VecDeque<ActionIntent>,
    /// Number of refused attempts due to capacity (observable, never silent).
    pub overflow: u64,
    /// Number of overlapping requests coalesced while that target is pending.
    pub coalesced: u64,
}
impl ActionIntents {
    /// Maximum pending targets, not a claim of an unbounded/lossless queue.
    pub const CAPACITY: usize = 64;
    /// Pending intents in receive order.
    pub fn pending(&self) -> &VecDeque<ActionIntent> {
        &self.pending
    }
}

/// Result from the canonical activation entry point.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActivationResult {
    /// Accepted once.
    Accepted,
    /// Same target already outstanding; explicit supported-cardinality limit.
    AlreadyPending,
    /// Capacity exhausted; no mutation or eviction occurred.
    Overflow,
    /// Stale, hidden, disabled, readonly, unsupported, or not installed.
    Rejected,
}

/// Drain and revalidate queued intents. Stale/hidden/disabled/readonly targets
/// are discarded before delivery; hosts must still validate their own command.
pub fn take_action_intents(world: &mut World) -> Vec<ActionIntent> {
    let pending: Vec<_> = world
        .resource_mut::<ActionIntents>()
        .map(|q| q.pending.drain(..).collect())
        .unwrap_or_default();
    pending
        .into_iter()
        .filter(|intent| {
            access_details(world, intent.entity).activate
                && world
                    .get::<Button>(intent.entity)
                    .is_some_and(|b| b.action == intent.name)
        })
        .collect()
}

/// Shared pointer/keyboard/AT button and tab activation. Does not execute host actions.
pub fn try_activate(world: &mut World, entity: Entity) -> ActivationResult {
    if !access_details(world, entity).activate {
        return ActivationResult::Rejected;
    }
    if let Some(button) = world.get::<Button>(entity).cloned() {
        if world.has::<ReleaseActivation>(entity) {
            let Some(queue) = world.resource_mut::<ActionIntents>() else {
                return ActivationResult::Rejected;
            };
            if queue.pending.iter().any(|i| i.entity == entity) {
                queue.coalesced += 1;
                return ActivationResult::AlreadyPending;
            }
            if queue.pending.len() == ActionIntents::CAPACITY {
                queue.overflow += 1;
                return ActivationResult::Overflow;
            }
            queue.pending.push_back(ActionIntent {
                entity,
                name: button.action,
            });
        } else if let Some(fired) = world.resource_mut::<Fired>() {
            fired.name = Some(button.action);
        } else {
            return ActivationResult::Rejected;
        }
        return ActivationResult::Accepted;
    }
    if let Some(tab) = world.get::<Tab>(entity).copied() {
        if let Some(group) = crate::tabs::tab_group(world, entity) {
            world.get_mut::<Tabs>(group).unwrap().selected = tab.id;
            crate::tabs::sync_panels(world);
            sync_related_panels(world);
            return ActivationResult::Accepted;
        }
    }
    ActivationResult::Rejected
}

/// Focus through the same validation path used by keyboard and AT.
pub fn request_focus(world: &mut World, entity: Entity) -> bool {
    if !access_details(world, entity).focus {
        return false;
    }
    if world.resource::<Focus>().and_then(|f| f.entity) != Some(entity) {
        cancel_activation(world);
    }
    world.insert_resource(Focus {
        entity: Some(entity),
    });
    true
}

/// Generic focus policy, no Shell/file-tree selection, edit, filesystem, or quit side effects.
/// Installs Chrome idempotently and uses its existing Input stage, not another dispatcher.
pub struct GenericInteraction;
#[derive(Default)]
struct InteractionState {
    pointer: Option<(Entity, u64)>,
    space: Option<Entity>,
    focus: Option<Entity>,
}
impl Plugin for GenericInteraction {
    fn build(self, app: &mut App) {
        app.add_plugin(Chrome);
        if app.world().has_resource::<InteractionState>() {
            return;
        }
        app.world_mut().insert_resource(InteractionState::default());
        app.world_mut().insert_resource(ActionIntents::default());
        if !app.world().has_resource::<Focus>() {
            app.world_mut().insert_resource(Focus::default());
        }
    }
}

/// Cancel armed pointer/keyboard input, e.g. on host capture loss. No activation.
pub fn cancel_activation(world: &mut World) {
    if let Some(state) = world.resource_mut::<InteractionState>() {
        state.pointer = None;
        state.space = None;
    }
}

fn focus_order(world: &World) -> Vec<Entity> {
    fn walk(world: &World, entity: Entity, out: &mut Vec<Entity>) {
        if !interaction_visible(world, entity) {
            return;
        }
        if access_details(world, entity).focus {
            out.push(entity);
        }
        for child in children_of(world, entity) {
            walk(world, child, out);
        }
    }
    let mut out = vec![];
    let mut roots = roots::<Layout>(world);
    roots.sort_by_key(|e| (e.index(), e.generation()));
    for root in roots {
        walk(world, root, &mut out);
    }
    out
}

pub(crate) fn sync_related_panels(world: &mut World) {
    for tab in world.entities_with::<TabRelation>() {
        let relation = *world.get::<TabRelation>(tab).unwrap();
        let selected = world
            .get::<Tab>(tab)
            .zip(world.get::<Tabs>(relation.group))
            .is_some_and(|(tab, group)| tab.id == group.selected);
        if !world.has::<TabPanel>(relation.panel) {
            continue;
        }
        if selected {
            world.remove::<Hidden>(relation.panel);
        } else {
            world.insert(relation.panel, Hidden);
        }
    }
    if !world.has_resource::<InteractionState>() {
        return;
    }
    if let Some(entity) = world.resource::<Focus>().and_then(|f| f.entity) {
        if !access_details(world, entity).focus {
            world.resource_mut::<Focus>().unwrap().entity = None;
        }
    }
}

pub(crate) fn apply_generic_input(world: &mut World) {
    if !world.has_resource::<InteractionState>() {
        return;
    }
    sync_related_panels(world);
    let Some(input) = world.resource::<Input>().cloned() else {
        return;
    };
    let focus = world.resource::<Focus>().and_then(|f| f.entity);
    let blur = events::<WindowFocus>(world).contains(&WindowFocus::Blurred);
    if blur || world.resource::<InteractionState>().unwrap().focus != focus {
        cancel_activation(world);
    }
    let escape = input.key == Some(Key::Escape)
        || input
            .keys
            .iter()
            .any(|e| e.logical == LogicalKey::Named(NamedKey::Escape) && !e.released);
    if blur || escape {
        cancel_activation(world);
        if blur {
            return;
        }
    }
    if world
        .resource::<InteractionState>()
        .unwrap()
        .space
        .is_some_and(|e| !access_details(world, e).activate)
    {
        world.resource_mut::<InteractionState>().unwrap().space = None;
    }
    if !escape && input.pointer.pressed && input.pointer.button == PointerButton::Left {
        let p = input.pointer.press_at.unwrap_or(input.pointer.position);
        if let Some(hit) = crate::layout::hit_test(world, p[0], p[1]) {
            let mut current = Some(hit);
            while let Some(entity) = current {
                if access_details(world, entity).focus {
                    request_focus(world, entity);
                    if world.has::<ReleaseActivation>(entity)
                        && access_details(world, entity).activate
                    {
                        world.resource_mut::<InteractionState>().unwrap().pointer =
                            Some((entity, input.pointer.id));
                    }
                    break;
                }
                current = world.get::<Parent>(entity).map(|p| p.entity);
            }
        }
    }
    let armed = world.resource::<InteractionState>().unwrap().pointer;
    if !escape {
        if let Some((entity, id)) = armed {
            if id != input.pointer.id
                || !access_details(world, entity).activate
                || (!input.pointer.down && !input.pointer.released)
            {
                cancel_activation(world);
            } else if input.pointer.released {
                world.resource_mut::<InteractionState>().unwrap().pointer = None;
                let inside = crate::layout::hit_test(
                    world,
                    input.pointer.position[0],
                    input.pointer.position[1],
                )
                .is_some_and(|hit| {
                    hit == entity
                        || crate::layout::ancestor_with::<Button>(world, hit) == Some(entity)
                });
                if inside {
                    try_activate(world, entity);
                }
            }
        }
    }
    // Rich queue owns its legacy shim. Never process both representations.
    let keys = if input.keys.is_empty() {
        input
            .key
            .and_then(|key| match key {
                Key::Tab => Some(NamedKey::Tab),
                Key::Enter => Some(NamedKey::Enter),
                Key::Escape => Some(NamedKey::Escape),
                _ => None,
            })
            .map(|key| {
                vec![KeyEvent::named(
                    key,
                    Modifiers {
                        shift: input.shift,
                        ctrl: input.ctrl,
                        alt: input.alt,
                        super_key: input.super_key,
                    },
                )]
            })
            .unwrap_or_default()
    } else {
        input.keys.clone()
    };
    let mut retained = Vec::new();
    for event in keys {
        let focus = world.resource::<Focus>().and_then(|f| f.entity);
        let editor_owns = focus.is_some_and(|e| world.has::<CapturesKeys>(e));
        let mut consumed = false;
        if !editor_owns && !event.modifiers.command() && !event.modifiers.alt {
            if event.logical == LogicalKey::Named(NamedKey::Tab) && !event.released {
                let order = focus_order(world);
                if !order.is_empty() {
                    let index = focus.and_then(|f| order.iter().position(|e| *e == f));
                    let index = match index {
                        Some(i) if event.modifiers.shift => (i + order.len() - 1) % order.len(),
                        Some(i) => (i + 1) % order.len(),
                        None if event.modifiers.shift => order.len() - 1,
                        None => 0,
                    };
                    cancel_activation(world);
                    request_focus(world, order[index]);
                }
                consumed = true;
            } else if event.logical == LogicalKey::Named(NamedKey::Escape) {
                cancel_activation(world);
                consumed = true;
            } else if let Some(entity) = focus.filter(|e| world.has::<ReleaseActivation>(*e)) {
                match event.logical {
                    LogicalKey::Named(NamedKey::Enter) => {
                        if !event.released && !event.repeat && !event.synthetic {
                            try_activate(world, entity);
                        }
                        consumed = true;
                    }
                    LogicalKey::Named(NamedKey::Space) => {
                        if event.synthetic {
                            cancel_activation(world);
                        } else if event.released {
                            let armed = world
                                .resource_mut::<InteractionState>()
                                .unwrap()
                                .space
                                .take();
                            if armed == Some(entity) {
                                try_activate(world, entity);
                            }
                        } else if !event.repeat && access_details(world, entity).activate {
                            world.resource_mut::<InteractionState>().unwrap().space = Some(entity);
                        }
                        consumed = true;
                    }
                    _ => {}
                }
            }
        }
        if consumed {
            if let Some(input) = world.resource_mut::<Input>() {
                input.key = None;
            }
        } else if !input.keys.is_empty() {
            retained.push(event);
        }
    }
    if !input.keys.is_empty() {
        world.resource_mut::<Input>().unwrap().keys = retained;
    }
    let focus = world.resource::<Focus>().and_then(|f| f.entity);
    world.resource_mut::<InteractionState>().unwrap().focus = focus;
    crate::focus::ensure_focus_visible(world);
}

/// Structural validation failure for a semantic tab/panel association.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabLinkError {
    /// Missing live Tab, Tabs, TabPanel or direct group-child relationship.
    InvalidTarget,
    /// Another tab already owns the panel or uses the same id in this group.
    Duplicate,
}

/// Bind a real panel before exposing a tab. No widget, buffer switch or host action.
/// Panels remain caller-owned; destruction/relinking is the host's responsibility.
pub fn link_tab_panel(
    world: &mut World,
    tab: Entity,
    group: Entity,
    panel: Entity,
) -> Result<(), TabLinkError> {
    let Some(id) = world.get::<Tab>(tab).map(|t| t.id) else {
        return Err(TabLinkError::InvalidTarget);
    };
    if !world.has::<Tabs>(group)
        || !world.has::<TabPanel>(panel)
        || world.get::<Parent>(tab).map(|p| p.entity) != Some(group)
    {
        return Err(TabLinkError::InvalidTarget);
    }
    if world
        .entities_with::<TabRelation>()
        .into_iter()
        .any(|other| {
            other != tab
                && world.get::<TabRelation>(other).is_some_and(|r| {
                    r.panel == panel
                        || (r.group == group && world.get::<Tab>(other).is_some_and(|t| t.id == id))
                })
        })
    {
        return Err(TabLinkError::Duplicate);
    }
    world.insert(tab, TabRelation { group, panel });
    sync_related_panels(world);
    Ok(())
}

/// Manual-tab focus navigation only: never changes active panel/selection.
/// A2 maps orientation-appropriate arrows and Home/End onto these operations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabFocusMove {
    /// Wrap to the next tab.
    Next,
    /// Wrap to the previous tab.
    Previous,
    /// First available tab.
    First,
    /// Last available tab.
    Last,
}

/// Focus a related tab in hierarchy order, independently of the active panel.
pub fn move_tab_focus(world: &mut World, group: Entity, movement: TabFocusMove) -> bool {
    if !world.has::<Tabs>(group) {
        return false;
    }
    let tabs: Vec<_> = children_of(world, group)
        .into_iter()
        .filter(|tab| {
            world
                .get::<TabRelation>(*tab)
                .is_some_and(|r| r.group == group)
                && access_details(world, *tab).focus
        })
        .collect();
    if tabs.is_empty() {
        return false;
    }
    let current = world
        .resource::<Focus>()
        .and_then(|f| f.entity)
        .and_then(|f| tabs.iter().position(|tab| *tab == f));
    let index = match movement {
        TabFocusMove::First => 0,
        TabFocusMove::Last => tabs.len() - 1,
        TabFocusMove::Next => current.map(|i| (i + 1) % tabs.len()).unwrap_or(0),
        TabFocusMove::Previous => current
            .map(|i| (i + tabs.len() - 1) % tabs.len())
            .unwrap_or(tabs.len() - 1),
    };
    request_focus(world, tabs[index])
}

// Only the opt-in policy needs key-up and synthetic cancellation edges. This
// does not change what legacy editors/widgets receive from the hotkey guard.
pub(crate) fn needs_activation_edges(world: &World) -> bool {
    world.has_resource::<InteractionState>()
        && world
            .resource::<Focus>()
            .and_then(|f| f.entity)
            .is_some_and(|e| world.has::<ReleaseActivation>(e))
}
