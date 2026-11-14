//! Additive interaction policy. Legacy widgets do not opt in implicitly.
use crate::*;
use std::collections::VecDeque;

/// Per-button release activation; omit to retain legacy pointer-press/Enter.
#[derive(Clone, Copy, Debug, Default)]
pub struct ReleaseActivation;
impl Component for ReleaseActivation {}

/// Opt out of sequential Tab traversal without denying pointer, arrow or AT focus.
/// Composite widgets maintain one roving stop using the canonical focus policy.
#[derive(Clone, Copy, Debug)]
pub struct TabStop(pub bool);
impl Component for TabStop {}

/// Explicit canonical focus delegation for a passive composite row. The callback
/// may update logical focus and return an existing external focus owner (such as
/// a palette search field). Called only after the requested target is admitted;
/// the returned entity is revalidated. Must not request focus recursively. Only
/// the requested entity participates: embedded controls never inherit delegation.
#[derive(Clone, Copy)]
pub struct FocusTarget(pub fn(&mut World, Entity) -> Entity);
impl Component for FocusTarget {}

/// Composite navigation hook executed in the canonical chronological key loop,
/// after pointer focus and before Tab/manual tabs/button handling. Return true
/// to consume only owned keys. Modified and blurred input bypass it. An explicit
/// hook on a CapturesKeys editor opts into pre-editor forwarding (e.g. a palette
/// search field); unhandled keys still reach that editor. Transactional editor
/// ownership is resolved first. No ancestor hook runs for a nested control.
#[derive(Clone, Copy)]
pub struct KeyNavigation(pub fn(&mut World, Entity, &KeyEvent) -> bool);
impl Component for KeyNavigation {}

/// Component-owned semantic effect at the canonical accepted-activation boundary.
/// Runs synchronously for pointer, keyboard and accessibility, after eligibility
/// and FIFO capacity checks. Consumes its own intent; unrelated host intents stay
/// queued in order. Use for composite state, never a second raw-input dispatcher.
#[derive(Clone, Copy)]
pub struct ActivationHandler(pub fn(&mut World, Entity));
impl Component for ActivationHandler {}

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
#[derive(Clone, Copy)]
struct PanelLink {
    tab: Entity,
    group: Entity,
}
impl Component for PanelLink {}

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
    /// Explicit orientation for opted-in manual tablists.
    pub orientation: Option<TabOrientation>,
    /// Checked state for two-state Checkbox controls.
    pub checked: Option<bool>,
    /// Multiple selection supported by a ListBox.
    pub multiselect: bool,
    /// Access id of the combobox active descendant, when that option is alive
    /// and visible. Focus stays on the field.
    pub active_descendant: Option<u64>,
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

// Public ECS components may be removed/reparented after linking. Never trust
// the original link validation when advertising, activating or showing panels.
pub(crate) fn valid_tab_relation(world: &World, tab: Entity) -> Option<TabRelation> {
    let relation = *world.get::<TabRelation>(tab)?;
    (world.has::<Tab>(tab)
        && world.has::<Tabs>(relation.group)
        && world.has::<TabPanel>(relation.panel)
        && world.get::<Parent>(tab).map(|p| p.entity) == Some(relation.group))
    .then_some(relation)
}

/// Inherited availability and readonly policy, including logical popup ownership.
pub(crate) fn policy(world: &World, mut entity: Entity) -> (bool, bool) {
    let mut readonly = false;
    for _ in 0..256 {
        if !world.is_alive(entity)
            || world.has::<Hidden>(entity)
            || world.has::<ControlDisabled>(entity)
            || world.get::<Button>(entity).is_some_and(|b| !b.enabled)
        {
            return (false, readonly);
        }
        readonly |= world.get::<Semantics>(entity).is_some_and(|s| s.readonly);
        readonly |= crate::structured_native::attached_structured(world, entity)
            .and_then(|id| {
                world
                    .resource::<crate::structured_native::StructuredDocuments>()?
                    .get(id)
            })
            .is_some_and(crate::structured::StructuredDocument::is_readonly);
        match world
            .get::<crate::FocusOwner>(entity)
            .map(|o| o.0)
            .or_else(|| world.get::<Parent>(entity).map(|p| p.entity))
        {
            Some(parent) => entity = parent,
            None => return (true, readonly),
        }
    }
    (false, readonly)
}

/// Current allowed-action matrix; platform advertising and dispatch share it.
pub fn access_details(world: &World, entity: Entity) -> AccessDetails {
    if !interaction_visible(world, entity) {
        return AccessDetails::default();
    }
    let semantics = world.get::<Semantics>(entity).cloned().unwrap_or_default();
    let (available, inherited_readonly) = policy(world, entity);
    let enabled = available
        && crate::overlay::modal_floor(world)
            .is_none_or(|floor| crate::overlay::layer_of(world, entity) >= floor);
    let relation = valid_tab_relation(world, entity);
    let selection = world.get::<crate::SelectionAccess>(entity);
    let valid_row = valid_list_row(world, entity);
    AccessDetails {
        description: semantics.description,
        readonly: inherited_readonly,
        focus: enabled
            && (world.has::<Focusable>(entity)
                || world.has::<Button>(entity)
                || world.has::<TextField>(entity)
                || world.has::<Prompt>(entity)
                || world.has::<Editor>(entity)
                || world.has::<List>(entity)
                || world.has::<Tab>(entity)
                || valid_row),
        activate: enabled
            && !inherited_readonly
            && (world.has::<Button>(entity)
                || world
                    .get::<List>(entity)
                    .is_some_and(|l| l.selected.is_some_and(|i| i < l.count))
                || valid_row
                || world.has::<Disclosure>(entity)
                || (world.has::<Tab>(entity)
                    && (if world.has::<TabRelation>(entity) {
                        relation.is_some()
                    } else {
                        crate::tabs::tab_group(world, entity).is_some()
                    }))),
        set_value: enabled
            && !inherited_readonly
            && ((world.has::<crate::Scrollbar>(entity)
                && crate::scrollbar_geometry(world, entity).is_some())
                || world.has::<crate::NumericAccess>(entity)),
        orientation: world.get::<ManualTabs>(entity).map(|p| p.0),
        panel: world.has::<TabPanel>(entity),
        checked: selection.and_then(|s| s.checked),
        multiselect: selection.is_some_and(|s| s.multiselect),
        controls: selection
            .and_then(|s| s.controls)
            .filter(|e| world.is_alive(*e))
            .map(access_id)
            .or_else(|| relation.map(|r| access_id(r.panel))),
        labelled_by: selection
            .and_then(|s| s.labelled_by)
            .filter(|e| world.is_alive(*e))
            .map(access_id)
            .or_else(|| {
                world
                    .entities_with::<TabRelation>()
                    .into_iter()
                    .find(|tab| valid_tab_relation(world, *tab).is_some_and(|r| r.panel == entity))
                    .map(access_id)
            }),
        active_descendant: selection
            .and_then(|s| s.active_descendant)
            .filter(|e| world.is_alive(*e) && !world.has::<crate::Hidden>(*e))
            .map(access_id),
    }
}

fn valid_list_row(world: &World, entity: Entity) -> bool {
    let Some(row) = world.get::<ListItem>(entity) else {
        return false;
    };
    let mut current = world.get::<Parent>(entity).map(|p| p.entity);
    for _ in 0..256 {
        let Some(e) = current else {
            return false;
        };
        if let Some(list) = world.get::<List>(e) {
            return row.index < list.count;
        }
        current = world.get::<Parent>(e).map(|p| p.entity);
    }
    false
}

pub(crate) fn readonly(world: &World, entity: Entity) -> bool {
    policy(world, entity).1
}

// Constrain user input, not programmatic validated source-edit operations.
// This opt-in guard leaves navigation, selection and Copy available.
pub(crate) fn sanitize_readonly_input(world: &mut World) {
    let Some(entity) = world.resource::<Focus>().and_then(|f| f.entity) else {
        return;
    };
    if !readonly(world, entity)
        || !(world.has::<TextField>(entity)
            || world.has::<Prompt>(entity)
            || world.has::<Editor>(entity))
    {
        return;
    }
    let Some(input) = world.resource_mut::<Input>() else {
        return;
    };
    input.chars.clear();
    input.ime_commit.clear();
    input.ime_preedit.clear();
    input.ime_cursor = None;
    input.key = input.key.filter(|key| {
        matches!(
            key,
            Key::Up | Key::Down | Key::Left | Key::Right | Key::Copy | Key::Tab | Key::Escape
        )
    });
    input.keys.retain(readonly_field_key);
    for event in &mut input.keys {
        event.text = None;
    }
}

pub(crate) fn readonly_field_key(event: &KeyEvent) -> bool {
    matches!(
        event.logical,
        LogicalKey::Named(
            NamedKey::ArrowUp
                | NamedKey::ArrowDown
                | NamedKey::ArrowLeft
                | NamedKey::ArrowRight
                | NamedKey::Home
                | NamedKey::End
                | NamedKey::PageUp
                | NamedKey::PageDown
                | NamedKey::Tab
                | NamedKey::Escape
        )
    ) || (event.modifiers.command()
        && matches!(event.logical, LogicalKey::Character('c' | 'C' | 'a' | 'A')))
}

/// Canonical intent from an opted-in button. Never also copied into `Fired`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActionIntent {
    /// Generation-checked source entity.
    pub entity: Entity,
    /// Opaque host action; never executed by core.
    pub name: String,
}

/// Bounded FIFO of distinct requests, including repeated requests for one target.
/// Hosts drain on Update. No eviction, overwrite, or implicit Fired fanout.
#[derive(Default, Debug)]
pub struct ActionIntents {
    pending: VecDeque<ActionIntent>,
    /// Number of refused attempts due to capacity (observable, never silent).
    pub overflow: u64,
}
impl ActionIntents {
    /// Maximum pending requests. Full queues reject explicitly without eviction.
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
    /// Capacity exhausted; no mutation or eviction occurred.
    Overflow,
    /// Stale, hidden, disabled, readonly, unsupported, or not installed.
    Rejected,
}

/// Drain and revalidate queued intents. Stale/hidden/disabled/readonly targets
/// are discarded before delivery; hosts must still validate their own command.
pub fn take_action_intents(world: &mut World) -> Vec<ActionIntent> {
    take_action_intents_matching(world, |_, _| true)
}

/// Drain only requests owned by a component. Unmatched requests retain FIFO
/// order for other consumers. Matched requests receive the usual revalidation.
pub fn take_action_intents_matching(
    world: &mut World,
    owns: impl Fn(&World, &ActionIntent) -> bool,
) -> Vec<ActionIntent> {
    let (pending, remaining): (Vec<_>, Vec<_>) = world
        .resource::<ActionIntents>()
        .map(|q| q.pending.iter().cloned().partition(|i| owns(world, i)))
        .unwrap_or_default();
    if let Some(q) = world.resource_mut::<ActionIntents>() {
        q.pending = remaining.into();
    }
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
            if queue.pending.len() == ActionIntents::CAPACITY {
                queue.overflow += 1;
                return ActivationResult::Overflow;
            }
            queue.pending.push_back(ActionIntent {
                entity,
                name: button.action,
            });
            if let Some(handler) = world.get::<ActivationHandler>(entity).copied() {
                // Admission used the same FIFO64 policy. Consume exactly this
                // accepted request before its semantic effect can invalidate
                // targets or recursively activate another component.
                world
                    .resource_mut::<ActionIntents>()
                    .unwrap()
                    .pending
                    .pop_back();
                (handler.0)(world, entity);
            }
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

/// Remove references to a destroyed owned subtree without disturbing unrelated
/// focus, capture or queued intents. Component lifecycle owners call this before despawn.
pub fn forget_interaction_targets(world: &mut World, entities: &[Entity]) {
    if let Some(state) = world.resource_mut::<InteractionState>() {
        if state.pointer.is_some_and(|(e, _)| entities.contains(&e)) {
            state.pointer = None;
        }
        if state.space.is_some_and(|e| entities.contains(&e)) {
            state.space = None;
        }
        if state.focus.is_some_and(|e| entities.contains(&e)) {
            state.focus = None;
        }
    }
    if let Some(focus) = world.resource_mut::<Focus>() {
        if focus.entity.is_some_and(|e| entities.contains(&e)) {
            focus.entity = None;
        }
    }
    if let Some(queue) = world.resource_mut::<ActionIntents>() {
        queue.pending.retain(|i| !entities.contains(&i.entity));
    }
    if let Some(drag) = world.resource_mut::<ScrollDrag>() {
        if drag.entity.is_some_and(|e| entities.contains(&e)) {
            drag.entity = None;
            drag.active = false;
        }
    }
    if let Some(drag) = world.resource_mut::<Drag>() {
        if drag.entity.is_some_and(|e| entities.contains(&e)) {
            drag.entity = None;
        }
    }
}

/// Focus through the same validation path used by keyboard and AT.
pub fn request_focus(world: &mut World, entity: Entity) -> bool {
    if !access_details(world, entity).focus {
        return false;
    }
    let entity = world
        .get::<FocusTarget>(entity)
        .copied()
        .map(|target| (target.0)(world, entity))
        .unwrap_or(entity);
    if !access_details(world, entity).focus {
        return false;
    }
    if world.resource::<Focus>().and_then(|f| f.entity) != Some(entity) {
        crate::field::transaction_focus_lost(world, Some(entity));
        cancel_activation(world);
    }
    if let Some(state) = world.resource_mut::<InteractionState>() {
        state.focus = Some(entity);
    }
    world.insert_resource(Focus {
        entity: Some(entity),
    });
    crate::focus::resume_focus_reveal(world);
    true
}

// Passive surfaces (e.g. markdown text) can take keyboard ownership without
// creating a focusable control. Keep synchronous blur and armed-input cleanup
// on the same path as request_focus, and avoid a duplicate blur next frame.
pub(crate) fn release_focus(world: &mut World) {
    crate::field::transaction_focus_lost(world, None);
    cancel_activation(world);
    if let Some(state) = world.resource_mut::<InteractionState>() {
        state.focus = None;
    }
    if let Some(focus) = world.resource_mut::<Focus>() {
        focus.entity = None;
    }
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
        app.add_system_front(Schedule::Input, crate::secondary::apply_secondary);
    }
}

/// Cancel armed pointer/keyboard input, e.g. on host capture loss. No activation.
pub fn cancel_activation(world: &mut World) {
    if let Some(state) = world.resource_mut::<InteractionState>() {
        state.pointer = None;
        state.space = None;
    }
}

/// Read-only view of canonical armed state for visuals, not another input path.
/// Cancellation, disabled/readonly and visibility are revalidated on every query.
pub fn interaction_pressed(world: &World, entity: Entity) -> bool {
    access_details(world, entity).activate
        && world
            .resource::<InteractionState>()
            .is_some_and(|s| s.space == Some(entity) || s.pointer.is_some_and(|(e, _)| e == entity))
}

fn focus_order(world: &World) -> Vec<Entity> {
    fn walk(world: &World, entity: Entity, out: &mut Vec<Entity>) {
        if !interaction_visible(world, entity) {
            return;
        }
        if access_details(world, entity).focus
            && world.get::<TabStop>(entity).is_none_or(|s| s.0)
            && crate::manual_tabs::roving_stop(world, entity)
        {
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
    for panel in world.entities_with::<PanelLink>() {
        let link = *world.get::<PanelLink>(panel).unwrap();
        if !valid_tab_relation(world, link.tab)
            .is_some_and(|r| r.panel == panel && r.group == link.group)
        {
            world.insert(panel, Hidden);
            world.remove::<PanelLink>(panel);
        }
    }
    for tab in world.entities_with::<TabRelation>() {
        let relation = *world.get::<TabRelation>(tab).unwrap();
        let selected = valid_tab_relation(world, tab).is_some()
            && world
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
    crate::popup::prepare(world);
    sync_related_panels(world);
    let Some(input) = world.resource::<Input>().cloned() else {
        return;
    };
    let focus = world.resource::<Focus>().and_then(|f| f.entity);
    let blur = events::<WindowFocus>(world).contains(&WindowFocus::Blurred);
    let previous = world.resource::<InteractionState>().unwrap().focus;
    if blur || previous != focus {
        crate::field::notify_focus_lost(world, previous, if blur { None } else { focus });
        cancel_activation(world);
    }
    let had_armed = world
        .resource::<InteractionState>()
        .is_some_and(|s| s.pointer.is_some() || s.space.is_some());
    let escape = input.key == Some(Key::Escape)
        || input
            .keys
            .iter()
            .any(|e| e.logical == LogicalKey::Named(NamedKey::Escape) && !e.released);
    if blur || escape {
        cancel_activation(world);
        if blur {
            // The hotkey filter may have preserved private control edges.
            // No keyboard input may escape to legacy widgets on blur.
            if let Some(input) = world.resource_mut::<Input>() {
                input.keys.clear();
                input.key = None;
                input.chars.clear();
                input.ime_commit.clear();
                input.ime_preedit.clear();
                input.ime_cursor = None;
            }
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
        if let Some(hit) = crate::layout::hit_test(world, p[0], p[1])
            .filter(|hit| !crate::selectable::owns_press(world, input.pointer.button, *hit))
        {
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
    if world
        .resource::<ScrollDrag>()
        .is_some_and(|drag| drag.active)
    {
        world.resource_mut::<InteractionState>().unwrap().pointer = None;
    }
    let armed = world.resource::<InteractionState>().unwrap().pointer;
    if !escape {
        if let Some((entity, id)) = armed {
            if id != input.pointer.id
                || (input.pointer.released && input.pointer.button != PointerButton::Left)
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
                        || crate::layout::ancestor_with::<ReleaseActivation>(world, hit)
                            == Some(entity)
                });
                if inside {
                    try_activate(world, entity);
                }
            }
        }
    }
    crate::field::prepare_transaction_field(world);
    // Rich queue owns its legacy shim. Never process both representations.
    let keys = if input.keys.is_empty() {
        input
            .key
            .and_then(|key| match key {
                Key::Tab => Some(NamedKey::Tab),
                Key::Enter => Some(NamedKey::Enter),
                Key::Escape => Some(NamedKey::Escape),
                Key::Up => Some(NamedKey::ArrowUp),
                Key::Down => Some(NamedKey::ArrowDown),
                Key::Left => Some(NamedKey::ArrowLeft),
                Key::Right => Some(NamedKey::ArrowRight),
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
        let plain = !event.modifiers.command() && !event.modifiers.alt;
        // Resolve every Space up, even if modifiers/ownership changed. Never
        // leave an unmatched arm for a later unrelated release to commit.
        let space_up = event.logical == LogicalKey::Named(NamedKey::Space) && event.released;
        let released_arm = if space_up {
            world
                .resource_mut::<InteractionState>()
                .unwrap()
                .space
                .take()
        } else {
            None
        };
        if event.synthetic || !plain {
            cancel_activation(world);
        }
        let scope_consumed = crate::popup::key(world, &event);
        let focus = world.resource::<Focus>().and_then(|f| f.entity);
        let mut consumed = scope_consumed
            || focus.is_some_and(|e| crate::field::transaction_key(world, e, &event));
        if !consumed && plain {
            let navigation = focus.and_then(|e| world.get::<KeyNavigation>(e).map(|n| (e, n.0)));
            let handled = navigation.is_some_and(|(e, handler)| handler(world, e, &event));
            // A composite hook may normalize an auxiliary focus target to its
            // roving row before falling through. Continue from actual focus,
            // not the stale pre-hook entity (also respect a newly focused editor).
            let focus = world.resource::<Focus>().and_then(|f| f.entity);
            if handled {
                consumed = true;
            } else if crate::source_extensions::reserves_key(world, &event)
                || crate::structured_native::reserves_key(world, &event)
            {
                // Source completion/IME and structured list controls execute in
                // the canonical editor stage, not the global focus prefilter.
            } else if focus.is_some_and(|e| {
                world.has::<CapturesKeys>(e) && !world.has::<FieldTransactions>(e)
                    // Source and legacy editors use Tab for focus traversal,
                    // including Tab returned by popup dismissal. No implicit
                    // indentation policy is installed by the generic surface.
                    && !(world.has::<Editor>(e)
                        && event.logical == LogicalKey::Named(NamedKey::Tab)
                        && !crate::source_extensions::reserves_key(world, &event))
            }) {
                // Retain this event for the editor's canonical input stage.
            } else if event.logical == LogicalKey::Named(NamedKey::Tab) {
                let order = focus_order(world);
                if !order.is_empty() && !event.released && !event.synthetic && !event.repeat {
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
            } else if crate::manual_tabs::manual_tab_key(world, focus, &event) {
                consumed = true;
            } else if event.logical == LogicalKey::Named(NamedKey::Escape) {
                cancel_activation(world);
                consumed = had_armed;
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
                            if !event.repeat && released_arm == Some(entity) {
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
        // Edges retained before focus traversal are private to this policy:
        // legacy consumers must never receive key-up or synthetic events.
        if !consumed && !input.keys.is_empty() && !event.released && !event.synthetic {
            retained.push(event);
        } else if consumed && input.keys.is_empty() {
            world.resource_mut::<Input>().unwrap().key = None;
        }
    }
    if !input.keys.is_empty() {
        let platform = crate::keys::platform_of(world);
        let input = world.resource_mut::<Input>().unwrap();
        input.key = retained
            .first()
            .and_then(|e| crate::keys::legacy_key(e, platform));
        input.chars = retained
            .iter()
            .filter_map(KeyEvent::insertable_text)
            .collect();
        input.keys = retained;
    }
    let focus = world.resource::<Focus>().and_then(|f| f.entity);
    world.resource_mut::<InteractionState>().unwrap().focus = focus;
    if input.pointer.pressed || input.key.is_some() || !input.keys.is_empty() {
        crate::focus::resume_focus_reveal(world);
    }
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
    if let Some(old) = world.get::<TabRelation>(tab).copied() {
        if old.panel != panel && world.is_alive(old.panel) {
            world.insert(old.panel, Hidden);
            world.remove::<PanelLink>(old.panel);
        }
    }
    world.insert(tab, TabRelation { group, panel });
    world.insert(panel, PanelLink { tab, group });
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
            valid_tab_relation(world, *tab).is_some_and(|r| r.group == group)
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

// Focus can change by pointer or Tab later in this Input pass. Preserve
// unregistered control edges in order until generic dispatch knows their owner;
// it strips private edges before handing the remaining queue to legacy code.
pub(crate) fn needs_activation_edges(world: &World) -> bool {
    world.has_resource::<InteractionState>()
}
