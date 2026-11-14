//! Tab strip for editor and document tabs.
//!
//! The host owns the tabs and their order. Activating a tab shows its panel.
//! Closing, reordering, moving a tab to another strip, and dropping one on a
//! pane edge emit events; mount again with the order the host kept. A
//! [`SplitSash`](crate::split::SplitSash) is how a host turns a split request
//! into panes. Dragging uses [`crate::drag`]: tabs are before/after targets,
//! and each pane's body uses edge bands for the split request.
//!
//! ```
//! use rndr_core::{Layout, Sim, Size, Viewport};
//! use rndr_ui::tabs::{TabId, TabSpec, TabStrip, TabStripProps};
//! use rndr_ui::{UiId, UiPlugin};
//!
//! let mut sim = Sim::new();
//! sim.app.add_plugin(UiPlugin);
//! sim.world().insert_resource(Viewport::new(640.0, 400.0));
//! let parent = sim.world().spawn((Layout {
//!     width: Size::Px(640.0),
//!     height: Size::Px(400.0),
//!     ..Layout::column()
//! },));
//! let tabs = [TabSpec::new(1, "Notes"), TabSpec::new(2, "Plan")];
//! let strip = TabStrip::mount(
//!     sim.world(),
//!     parent,
//!     UiId(1),
//!     TabStripProps::new("Documents"),
//!     &tabs,
//!     TabId(1),
//! )
//! .unwrap();
//! sim.update();
//! assert_eq!(strip.selected(sim.world()), Some(TabId(1)));
//! ```

use crate::drag::{
    self, committed, DragKeyboard, DragKind, DragPayload, DragSource, DropGeometry, DropPlace,
    DropTarget,
};
use crate::lifecycle::{find_keyed_child, require_layout_parent};
use crate::menu::{open_menu, MenuAt, MenuEntry, MenuItem, MenuPicked};
use crate::{Token, UiError, UiId, UiTheme};
use rndr_core::*;
use std::collections::BTreeSet;

const TAB_WIDTH: f32 = 120.0;
const BAR_HEIGHT: f32 = 32.0;
const CHEVRON: f32 = 28.0;
const SCROLL_STEP: f32 = 120.0;
/// Cross-axis and along-axis bands of a pane body. The interior is one inside place.
const PANE_EDGE: f32 = 0.22;

/// Payload kind for a tab. List rows use 1 and tree nodes use 2.
pub const TAB: DragKind = DragKind(3);

/// Host identity of one tab. Unique within a strip.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TabId(pub u64);

/// One tab the host wants shown.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TabSpec {
    /// Stable id. Mount rejects a strip that repeats one.
    pub id: TabId,
    /// Visible label. Must not be blank.
    pub label: String,
    /// Optional leading glyph.
    pub icon: Option<Icon>,
    /// Draws a mark and appends ", unsaved" to the accessible name.
    pub dirty: bool,
    /// Draws a pin. [`Self::pinned`] also clears [`Self::closable`].
    pub pinned: bool,
    /// Close button, middle-click, Delete, and Ctrl+W (Cmd+W on macOS).
    pub closable: bool,
}

impl TabSpec {
    /// Closable tab with no icon, dirty mark, or pin.
    pub fn new(id: u64, label: impl Into<String>) -> Self {
        Self {
            id: TabId(id),
            label: label.into(),
            icon: None,
            dirty: false,
            pinned: false,
            closable: true,
        }
    }

    /// Leading glyph. The accessible name stays the label.
    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    /// Unsaved mark.
    pub fn dirty(mut self, dirty: bool) -> Self {
        self.dirty = dirty;
        self
    }

    /// Pin the tab and clear [`Self::closable`].
    ///
    /// Call [`Self::closable`] afterwards to allow closing a pinned tab.
    /// Pinned tabs stay in a leading group: a reorder that would place one
    /// after an unpinned tab is refused.
    pub fn pinned(mut self, pinned: bool) -> Self {
        self.pinned = pinned;
        if pinned {
            self.closable = false;
        }
        self
    }

    /// Whether the host allows this tab to close.
    pub fn closable(mut self, closable: bool) -> Self {
        self.closable = closable;
        self
    }
}

/// Label and whether this strip is the active group.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TabStripProps {
    /// Accessible name of the tablist. Must not be blank.
    pub label: String,
    /// The selected tab uses the selected fill. An inactive strip keeps a quieter one.
    pub active: bool,
    /// Disabled strips do not focus, activate, drag, or close.
    pub enabled: bool,
}

impl TabStripProps {
    /// Enabled active strip named `label`.
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            active: true,
            enabled: true,
        }
    }

    /// Mark this strip as the active group, or not.
    pub fn active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }

    /// Refuse focus, activation, drag, and close.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

/// Where a pane-edge drop asks the host to open a split.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SplitDirection {
    /// Leading edge of a horizontal split.
    Left,
    /// Trailing edge of a horizontal split.
    Right,
    /// Leading edge of a vertical split.
    Above,
    /// Trailing edge of a vertical split.
    Below,
}

impl SplitDirection {
    /// Map a drop edge. Row edges are left and right. Column edges are above and below.
    pub fn from_edge(edge: drag::DropEdge) -> Self {
        match (edge.axis, edge.end) {
            (Axis::Row, drag::DropEnd::Leading) => Self::Left,
            (Axis::Row, drag::DropEnd::Trailing) => Self::Right,
            (Axis::Column, drag::DropEnd::Leading) => Self::Above,
            (Axis::Column, drag::DropEnd::Trailing) => Self::Below,
        }
    }

    /// Axis a host passes to [`crate::split::SplitSash`] for this edge.
    pub fn axis(self) -> Axis {
        match self {
            Self::Left | Self::Right => Axis::Row,
            Self::Above | Self::Below => Axis::Column,
        }
    }

    /// The dragged tab is the leading pane.
    pub fn leading(self) -> bool {
        matches!(self, Self::Left | Self::Above)
    }
}

/// A same-strip reorder. The strip does not apply it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TabMove {
    /// Tab that moved.
    pub id: TabId,
    /// Tab the pointer or key named.
    pub target: TabId,
    /// [`DropPlace::Before`] or [`DropPlace::After`].
    pub place: DropPlace,
}

/// A tab dragged from one strip onto another.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TabTransfer {
    /// Tab that moved.
    pub id: TabId,
    /// Strip the drag started on.
    pub from: Entity,
    /// Strip under the pointer.
    pub to: Entity,
    /// Tab under the pointer. `None` when the drop was the pane interior.
    pub target: Option<TabId>,
    /// Before, after, or inside.
    pub place: DropPlace,
}

/// A drop on a pane edge. The host can mount a [`crate::split::SplitSash`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TabSplitRequest {
    /// Tab that was dragged.
    pub id: TabId,
    /// Strip whose pane received the drop.
    pub strip: Entity,
    /// Which edge.
    pub direction: SplitDirection,
}

/// What the host is being asked to change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabStripEventKind {
    /// The selected tab changed by click, Enter, Space, or the overflow menu.
    Activated(TabId),
    /// Close button, middle-click, Delete, or Ctrl+W. The tab stays until the host removes it.
    CloseRequested(TabId),
    /// Same-strip reorder. Apply [`reorder_tabs`] or ignore it.
    Reordered(TabMove),
    /// The tab changed strips. Apply [`transfer_tab`] or ignore it.
    Moved(TabTransfer),
    /// Pane-edge drop. The strip does not split itself.
    SplitRequested(TabSplitRequest),
}

/// One request from a strip. Readable in Update after the Input that sent it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TabStripEvent {
    /// Strip the request is about. For a move or a split, this is the strip under the pointer.
    pub strip: Entity,
    /// What happened.
    pub kind: TabStripEventKind,
}
impl Event for TabStripEvent {}

/// Move `id` to `place` relative to `target`.
///
/// `None` when the ids are missing, the order would not change, `place` is
/// not before or after, or the move would put a pinned tab after an unpinned
/// one (or the reverse).
pub fn reorder_tabs(
    tabs: &[TabSpec],
    id: TabId,
    target: TabId,
    place: DropPlace,
) -> Option<Vec<TabSpec>> {
    if id == target {
        return None;
    }
    let moving = tabs.iter().find(|tab| tab.id == id)?.clone();
    let mut next: Vec<TabSpec> = tabs.iter().filter(|tab| tab.id != id).cloned().collect();
    let index = insertion(&next, &moving, Some(target), place)?;
    next.insert(index, moving);
    let same = next.iter().zip(tabs).all(|(next, old)| next.id == old.id);
    if same {
        None
    } else {
        Some(next)
    }
}

/// Remove `id` from `from` and insert it in `to`.
///
/// `target` is the tab under the pointer. `None` with [`DropPlace::Inside`]
/// appends, keeping a pinned tab in the pinned group. `None` when `id` is
/// missing, `to` already has that id, or the place is not allowed.
pub fn transfer_tab(
    from: &[TabSpec],
    to: &[TabSpec],
    id: TabId,
    target: Option<TabId>,
    place: DropPlace,
) -> Option<(Vec<TabSpec>, Vec<TabSpec>)> {
    if to.iter().any(|tab| tab.id == id) {
        return None;
    }
    let moving = from.iter().find(|tab| tab.id == id)?.clone();
    let source: Vec<TabSpec> = from.iter().filter(|tab| tab.id != id).cloned().collect();
    let mut dest = to.to_vec();
    let index = insertion(&dest, &moving, target, place)?;
    dest.insert(index, moving);
    Some((source, dest))
}

fn insertion(
    dest: &[TabSpec],
    moving: &TabSpec,
    target: Option<TabId>,
    place: DropPlace,
) -> Option<usize> {
    let pinned_end = dest
        .iter()
        .position(|tab| !tab.pinned)
        .unwrap_or(dest.len());
    let index = match (target, place) {
        (None, DropPlace::Inside) => {
            if moving.pinned {
                pinned_end
            } else {
                dest.len()
            }
        }
        (Some(target), DropPlace::Before | DropPlace::After) => {
            let mut index = dest.iter().position(|tab| tab.id == target)?;
            if place == DropPlace::After {
                index += 1;
            }
            if moving.pinned && index > pinned_end {
                return None;
            }
            if !moving.pinned && index < pinned_end {
                return None;
            }
            index
        }
        _ => return None,
    };
    Some(index)
}

/// One mounted strip. Content for a tab belongs in [`TabStripHandle::panel`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TabStripHandle {
    /// Strip root.
    pub root: Entity,
    /// Tablist. Carries [`Tabs`] and the roving tab stop.
    pub list: Entity,
    /// Pane body. A drop on its edge is a split request.
    pub body: Entity,
    /// Horizontal scrollport of the tab labels.
    pub scroll: Entity,
}

/// Semantic tab strip: a scrollable tablist, one panel per tab, and drag.
pub struct TabStrip;

impl TabStrip {
    /// Mount or update a strip under `parent`.
    ///
    /// `tabs` is the order to show. `selected` must be one of those ids.
    /// Labels must not be blank, and ids must be unique in this strip.
    /// A later mount with the same id reuses the root, the surviving tabs, and
    /// their panels. A tab omitted from `tabs` is despawned with its panel.
    pub fn mount(
        world: &mut World,
        parent: Entity,
        id: UiId,
        props: TabStripProps,
        tabs: &[TabSpec],
        selected: TabId,
    ) -> Result<TabStripHandle, UiError> {
        require_layout_parent(world, parent)?;
        admit(&props, tabs, selected)?;
        if let Some(root) = find_keyed_child(world, parent, id) {
            let state = world
                .get::<StripState>(root)
                .cloned()
                .ok_or(UiError::IdentityConflict)?;
            reconcile(world, root, &state, &props, tabs, selected)?;
            Ok(handle_of(root, &state))
        } else {
            let root = world.spawn((Layout {
                width: Size::Fill,
                height: Size::Fill,
                ..Layout::column()
            },));
            set_parent(world, root, parent);
            let bar = world.spawn((
                Layout {
                    width: Size::Fill,
                    height: Size::Px(BAR_HEIGHT),
                    ..Layout::row()
                },
                Align {
                    items: Items::Center,
                    ..Align::default()
                },
                ResolvedColor,
            ));
            set_parent(world, bar, root);
            let leading = chrome_button(
                world,
                bar,
                root,
                ChromeKind::Leading,
                Icon::ChevronLeft,
                "Scroll left",
            );
            let scroll = world.spawn((
                Layout {
                    width: Size::Fill,
                    height: Size::Px(BAR_HEIGHT),
                    ..Layout::row()
                },
                Scroll::default(),
                ResolvedColor,
            ));
            set_parent(world, scroll, bar);
            let list = world.spawn((
                Layout {
                    axis: Axis::Row,
                    width: Size::Hug,
                    height: Size::Px(BAR_HEIGHT),
                    ..Layout::row()
                },
                Tabs {
                    selected: selected.0,
                },
                ManualTabs(TabOrientation::Horizontal),
                ResolvedColor,
            ));
            set_parent(world, list, scroll);
            let trailing = chrome_button(
                world,
                bar,
                root,
                ChromeKind::Trailing,
                Icon::ChevronRight,
                "Scroll right",
            );
            let overflow = chrome_button(
                world,
                bar,
                root,
                ChromeKind::Overflow,
                Icon::Ellipsis,
                "More tabs",
            );
            let body = world.spawn((
                Layout {
                    width: Size::Fill,
                    height: Size::Fill,
                    ..Layout::column()
                },
                ResolvedColor,
            ));
            set_parent(world, body, root);
            let state = StripState {
                key: id,
                props: props.clone(),
                specs: Vec::new(),
                list,
                scroll,
                body,
                leading,
                trailing,
                overflow,
                menu: None,
                published: Some(selected),
                reveal: true,
                shown_index: None,
                shown_port: 0.0,
            };
            world.insert(root, state.clone());
            bind_pane(world, body, root);
            reconcile(world, root, &state, &props, tabs, selected)?;
            Ok(handle_of(root, &state))
        }
    }
}

impl TabStripHandle {
    /// The tab header, if `id` is currently mounted.
    pub fn tab(self, world: &World, id: TabId) -> Option<Entity> {
        tab_entity(world, self.list, id)
    }

    /// The panel for `id`. The host parents document content here.
    ///
    /// The panel is despawned when a later mount omits that id.
    pub fn panel(self, world: &World, id: TabId) -> Option<Entity> {
        tab_entity(world, self.list, id)
            .and_then(|tab| world.get::<TabSlot>(tab).map(|slot| slot.panel))
    }

    /// Selected tab, if the strip is still mounted and that id is present.
    pub fn selected(self, world: &World) -> Option<TabId> {
        let state = world.get::<StripState>(self.root)?;
        let selected = TabId(world.get::<Tabs>(state.list)?.selected);
        state
            .specs
            .iter()
            .any(|tab| tab.id == selected)
            .then_some(selected)
    }

    /// Chevron that scrolls toward the first tab.
    pub fn scroll_back(self, world: &World) -> Option<Entity> {
        world
            .get::<StripState>(self.root)
            .map(|state| state.leading)
    }

    /// Chevron that scrolls toward the last tab.
    pub fn scroll_forward(self, world: &World) -> Option<Entity> {
        world
            .get::<StripState>(self.root)
            .map(|state| state.trailing)
    }

    /// Button that opens the menu of tabs that are not fully in view.
    pub fn overflow_button(self, world: &World) -> Option<Entity> {
        world
            .get::<StripState>(self.root)
            .map(|state| state.overflow)
    }

    /// Despawn the strip, its tabs, and its panels.
    pub fn unmount(self, world: &mut World) -> Result<(), UiError> {
        let state = world
            .get::<StripState>(self.root)
            .cloned()
            .ok_or(UiError::Stale)?;
        if let Some(menu) = state.menu {
            if world.is_alive(menu) {
                close_popup(world, menu, false);
            }
        }
        let gone = subtree(world, self.root);
        forget_interaction_targets(world, &gone);
        despawn(world, self.root);
        Ok(())
    }
}

fn handle_of(root: Entity, state: &StripState) -> TabStripHandle {
    TabStripHandle {
        root,
        list: state.list,
        body: state.body,
        scroll: state.scroll,
    }
}

fn admit(props: &TabStripProps, tabs: &[TabSpec], selected: TabId) -> Result<(), UiError> {
    if props.label.trim().is_empty() || tabs.is_empty() {
        return Err(UiError::InvalidProps);
    }
    let mut seen = BTreeSet::new();
    for tab in tabs {
        if tab.label.trim().is_empty() || !seen.insert(tab.id) {
            return Err(UiError::InvalidProps);
        }
    }
    if !seen.contains(&selected) {
        return Err(UiError::InvalidProps);
    }
    Ok(())
}

fn reconcile(
    world: &mut World,
    root: Entity,
    state: &StripState,
    props: &TabStripProps,
    tabs: &[TabSpec],
    selected: TabId,
) -> Result<(), UiError> {
    let previous = tabs_of(world, state.list);
    let mut ordered = Vec::new();
    for spec in tabs {
        if let Some((entity, _)) = previous.iter().find(|(_, slot)| slot.id == spec.id) {
            update_tab(world, root, *entity, spec, props.enabled)?;
            ordered.push(*entity);
        } else {
            ordered.push(create_tab(world, root, state, spec, props.enabled)?);
        }
    }
    for (entity, slot) in &previous {
        if !tabs.iter().any(|spec| spec.id == slot.id) {
            destroy_tab(world, *entity, slot);
        }
    }
    order_children(world, state.list, &ordered);
    let panels: Vec<Entity> = ordered
        .iter()
        .filter_map(|tab| world.get::<TabSlot>(*tab).map(|slot| slot.panel))
        .collect();
    order_children(world, state.body, &panels);
    world.insert(
        state.list,
        Tabs {
            selected: selected.0,
        },
    );
    world.insert(state.list, ManualTabs(TabOrientation::Horizontal));
    world.insert(
        state.list,
        Semantics {
            name: Some(props.label.clone()),
            ..Semantics::default()
        },
    );
    show_selected(world, state.list, selected);
    if props.enabled {
        world.remove::<ControlDisabled>(root);
    } else {
        world.insert(root, ControlDisabled);
    }
    let mut next = world
        .get::<StripState>(root)
        .cloned()
        .unwrap_or_else(|| state.clone());
    let previous_index = state.specs.iter().position(|spec| spec.id == selected);
    let next_index = tabs.iter().position(|spec| spec.id == selected);
    next.props = props.clone();
    next.specs = tabs.to_vec();
    next.published = Some(selected);
    next.reveal = next.reveal || previous_index != next_index;
    world.insert(root, next);
    for tab in ordered {
        sync_drag(world, root, tab, props.enabled);
        if world.get::<TabRelation>(tab).is_none() {
            let panel = world
                .get::<TabSlot>(tab)
                .map(|slot| slot.panel)
                .ok_or(UiError::Stale)?;
            link_tab_panel(world, tab, state.list, panel).map_err(|_| UiError::Stale)?;
        }
    }
    show_selected(world, state.list, selected);
    Ok(())
}

fn create_tab(
    world: &mut World,
    root: Entity,
    state: &StripState,
    spec: &TabSpec,
    enabled: bool,
) -> Result<Entity, UiError> {
    let tab = world.spawn((
        Layout {
            width: Size::Px(TAB_WIDTH),
            height: Size::Px(BAR_HEIGHT),
            padding: 4.0,
            gap: 4.0,
            ..Layout::row()
        },
        Align {
            items: Items::Center,
            ..Align::default()
        },
        Focusable,
        ReleaseActivation,
        ResolvedColor,
        Tab { id: spec.id.0 },
    ));
    set_parent(world, tab, state.list);
    let icon = glyph(world, tab, Icon::File);
    let pin = glyph(world, tab, Icon::Pin);
    let title = world.spawn((
        Layout {
            width: Size::Fill,
            height: Size::Px(BAR_HEIGHT),
            ..Layout::column()
        },
        Label::new(&spec.label).overflow(TextOverflow::Ellipsis),
        ResolvedColor,
    ));
    set_parent(world, title, tab);
    let dirty = world.spawn((
        Layout {
            width: Size::Px(8.0),
            height: Size::Px(8.0),
            ..Layout::column()
        },
        ResolvedColor,
    ));
    set_parent(world, dirty, tab);
    let close = close_button(world, tab, root, spec);
    let panel = world.spawn((
        Layout {
            width: Size::Fill,
            height: Size::Fill,
            ..Layout::column()
        },
        TabPanel,
        ResolvedColor,
    ));
    set_parent(world, panel, state.body);
    world.insert(
        tab,
        TabSlot {
            strip: root,
            id: spec.id,
            panel,
            icon,
            pin,
            title,
            dirty,
            close,
            closable: spec.closable,
        },
    );
    update_tab(world, root, tab, spec, enabled)?;
    Ok(tab)
}

fn update_tab(
    world: &mut World,
    root: Entity,
    tab: Entity,
    spec: &TabSpec,
    enabled: bool,
) -> Result<(), UiError> {
    let mut slot = world.get::<TabSlot>(tab).copied().ok_or(UiError::Stale)?;
    slot.id = spec.id;
    slot.strip = root;
    slot.closable = spec.closable;
    world.insert(tab, Tab { id: spec.id.0 });
    world.insert(tab, slot);
    if let Some(label) = world.get_mut::<Label>(slot.title) {
        label.text = spec.label.clone();
    }
    if let Some(label) = world.get_mut::<Label>(slot.icon) {
        if let Some(icon) = spec.icon {
            label.text = icon.scalar().to_string();
        }
    }
    set_shown(world, slot.icon, spec.icon.is_some());
    set_shown(world, slot.pin, spec.pinned);
    set_shown(world, slot.dirty, spec.dirty);
    set_shown(world, slot.close, spec.closable);
    if let Some(button) = world.get_mut::<Button>(slot.close) {
        button.enabled = spec.closable && enabled;
    }
    world.insert(
        slot.close,
        Semantics {
            name: Some(format!("Close {}", spec.label)),
            ..Semantics::default()
        },
    );
    world.insert(slot.close, CloseButton { strip: root, tab });
    world.insert(
        tab,
        Semantics {
            name: Some(access_name(spec)),
            ..Semantics::default()
        },
    );
    sync_drag(world, root, tab, enabled);
    Ok(())
}

fn sync_drag(world: &mut World, root: Entity, tab: Entity, enabled: bool) {
    let Some(slot) = world.get::<TabSlot>(tab).copied() else {
        return;
    };
    let name = world
        .get::<StripState>(root)
        .and_then(|state| state.specs.iter().find(|spec| spec.id == slot.id))
        .map(|spec| spec.label.clone())
        .or_else(|| {
            world
                .get::<Semantics>(tab)
                .and_then(|semantics| semantics.name.clone())
        })
        .unwrap_or_default();
    world.insert(
        tab,
        DragSource {
            payload: DragPayload {
                kind: TAB,
                value: slot.id.0,
                owner: Some(root),
            },
            enabled,
            name,
            keyboard: DragKeyboard::Host,
        },
    );
    world.insert(
        tab,
        DropTarget {
            enabled,
            kinds: vec![TAB],
            owner: None,
            geometry: DropGeometry::reorder(Axis::Row),
            gate: Some(tab_gate),
        },
    );
}

fn bind_pane(world: &mut World, body: Entity, strip: Entity) {
    world.insert(body, PaneTarget { strip });
    world.insert(
        body,
        DropTarget {
            enabled: true,
            kinds: vec![TAB],
            owner: None,
            geometry: DropGeometry {
                axis: Axis::Column,
                edge: PANE_EDGE,
                cross_edge: PANE_EDGE,
                inside: 1.0,
            },
            gate: Some(pane_gate),
        },
    );
}

fn tab_gate(world: &World, payload: DragPayload, target: Entity, place: DropPlace) -> bool {
    if payload.kind != TAB {
        return false;
    }
    let Some(slot) = world.get::<TabSlot>(target).copied() else {
        return false;
    };
    let Some(source) = payload.owner else {
        return false;
    };
    let Some(source_specs) = world
        .get::<StripState>(source)
        .map(|state| state.specs.clone())
    else {
        return false;
    };
    let Some(dest_specs) = world
        .get::<StripState>(slot.strip)
        .map(|state| state.specs.clone())
    else {
        return false;
    };
    let id = TabId(payload.value);
    if source == slot.strip {
        reorder_tabs(&dest_specs, id, slot.id, place).is_some()
    } else {
        transfer_tab(&source_specs, &dest_specs, id, Some(slot.id), place).is_some()
    }
}

fn pane_gate(world: &World, payload: DragPayload, target: Entity, place: DropPlace) -> bool {
    if payload.kind != TAB {
        return false;
    }
    let Some(pane) = world.get::<PaneTarget>(target).copied() else {
        return false;
    };
    let Some(source) = payload.owner else {
        return false;
    };
    match place {
        DropPlace::Edge(_) => true,
        DropPlace::Inside if source != pane.strip => {
            let source_specs = world
                .get::<StripState>(source)
                .map(|state| state.specs.clone())
                .unwrap_or_default();
            let dest_specs = world
                .get::<StripState>(pane.strip)
                .map(|state| state.specs.clone())
                .unwrap_or_default();
            transfer_tab(
                &source_specs,
                &dest_specs,
                TabId(payload.value),
                None,
                place,
            )
            .is_some()
        }
        _ => false,
    }
}

fn destroy_tab(world: &mut World, tab: Entity, slot: &TabSlot) {
    let mut gone = subtree(world, tab);
    gone.extend(subtree(world, slot.panel));
    forget_interaction_targets(world, &gone);
    despawn(world, tab);
    despawn(world, slot.panel);
}

fn show_selected(world: &mut World, list: Entity, selected: TabId) {
    for (_, slot) in tabs_of(world, list) {
        if slot.id == selected {
            world.remove::<Hidden>(slot.panel);
        } else {
            world.insert(slot.panel, Hidden);
            world.insert(slot.panel, Rect::default());
        }
    }
}

fn access_name(spec: &TabSpec) -> String {
    let mut name = spec.label.clone();
    if spec.dirty {
        name.push_str(", unsaved");
    }
    if spec.pinned {
        name.push_str(", pinned");
    }
    name
}

fn glyph(world: &mut World, parent: Entity, icon: Icon) -> Entity {
    let entity = world.spawn((
        Layout {
            width: Size::Px(16.0),
            height: Size::Px(16.0),
            ..Layout::column()
        },
        Label::new(icon.scalar().to_string()),
        ResolvedColor,
    ));
    set_parent(world, entity, parent);
    entity
}

fn close_button(world: &mut World, tab: Entity, strip: Entity, spec: &TabSpec) -> Entity {
    let entity = world.spawn((
        Layout {
            width: Size::Px(16.0),
            height: Size::Px(16.0),
            ..Layout::column()
        },
        Button::new("tab-close"),
        Focusable,
        ReleaseActivation,
        TabStop(false),
        ResolvedColor,
    ));
    world.insert(entity, Label::new(Icon::X.scalar().to_string()));
    world.insert(
        entity,
        Semantics {
            name: Some(format!("Close {}", spec.label)),
            ..Semantics::default()
        },
    );
    world.insert(entity, CloseButton { strip, tab });
    world.insert(entity, ActivationHandler(on_close));
    set_parent(world, entity, tab);
    entity
}

fn chrome_button(
    world: &mut World,
    parent: Entity,
    strip: Entity,
    kind: ChromeKind,
    icon: Icon,
    name: &str,
) -> Entity {
    let action = match kind {
        ChromeKind::Leading => "tab-scroll-leading",
        ChromeKind::Trailing => "tab-scroll-trailing",
        ChromeKind::Overflow => "tab-overflow",
    };
    let entity = world.spawn((
        Layout {
            width: Size::Px(CHEVRON),
            height: Size::Px(BAR_HEIGHT),
            ..Layout::column()
        },
        Button::new(action),
        Focusable,
        ReleaseActivation,
        ResolvedColor,
    ));
    world.insert(entity, Label::new(icon.scalar().to_string()));
    world.insert(
        entity,
        Semantics {
            name: Some(name.to_string()),
            ..Semantics::default()
        },
    );
    world.insert(entity, ChromeButton { strip, kind });
    world.insert(
        entity,
        ActivationHandler(match kind {
            ChromeKind::Overflow => on_overflow,
            _ => on_chevron,
        }),
    );
    if kind != ChromeKind::Overflow {
        world.insert(entity, TabStop(false));
    }
    set_parent(world, entity, parent);
    entity
}

fn on_close(world: &mut World, entity: Entity) {
    let Some(close) = world.get::<CloseButton>(entity).copied() else {
        return;
    };
    if !tab_closable(world, close.tab) {
        return;
    }
    request_focus(world, close.tab);
    emit(
        world,
        close.strip,
        TabStripEventKind::CloseRequested(tab_id(world, close.tab)),
    );
}

fn on_chevron(world: &mut World, entity: Entity) {
    let Some(button) = world.get::<ChromeButton>(entity).copied() else {
        return;
    };
    let delta = match button.kind {
        ChromeKind::Leading => SCROLL_STEP,
        ChromeKind::Trailing => -SCROLL_STEP,
        ChromeKind::Overflow => return,
    };
    scroll_by(world, button.strip, delta);
}

fn on_overflow(world: &mut World, entity: Entity) {
    let Some(button) = world.get::<ChromeButton>(entity).copied() else {
        return;
    };
    if world
        .get::<Button>(entity)
        .is_some_and(|button| !button.enabled)
    {
        return;
    }
    open_overflow(world, button.strip, entity);
}

fn open_overflow(world: &mut World, strip: Entity, anchor: Entity) {
    let Some(state) = world.get::<StripState>(strip).cloned() else {
        return;
    };
    let hidden = hidden_tabs(world, &state);
    if hidden.is_empty() {
        return;
    }
    let selected = world.get::<Tabs>(state.list).map(|tabs| tabs.selected);
    let entries: Vec<MenuEntry> = hidden
        .iter()
        .map(|spec| {
            MenuEntry::Item(
                MenuItem::new(spec.id.0, spec.label.clone()).checked(Some(spec.id.0) == selected),
            )
        })
        .collect();
    if let Ok(menu) = open_menu(world, anchor, MenuAt::Below, &entries) {
        if let Some(state) = world.get_mut::<StripState>(strip) {
            state.menu = Some(menu.root);
        }
    }
}

fn scroll_by(world: &mut World, strip: Entity, delta: f32) {
    let Some(port) = world.get::<StripState>(strip).map(|state| state.scroll) else {
        return;
    };
    let rect = world.get::<Rect>(port).copied().unwrap_or_default();
    let mut scroll = world.get::<Scroll>(port).copied().unwrap_or_default();
    scroll.offset[0] += delta;
    scroll.clamp_to(rect.w, rect.h);
    world.insert(port, scroll);
}

fn reveal_index(world: &mut World, state: &StripState, index: usize) {
    let rect = world.get::<Rect>(state.scroll).copied().unwrap_or_default();
    if rect.w <= 0.0 {
        return;
    }
    let mut scroll = world
        .get::<Scroll>(state.scroll)
        .copied()
        .unwrap_or_default();
    let tab_x = index as f32 * TAB_WIDTH;
    let start = -scroll.offset[0];
    let end = start + rect.w;
    if tab_x < start {
        scroll.offset[0] = -tab_x;
    } else if tab_x + TAB_WIDTH > end {
        scroll.offset[0] = -(tab_x + TAB_WIDTH - rect.w);
    }
    scroll.clamp_to(rect.w, rect.h);
    world.insert(state.scroll, scroll);
}

pub(crate) fn drive(world: &mut World) {
    note_drop(world);
    note_keys(world);
    note_middle(world);
    let strips = world.entities_with::<StripState>();
    for strip in strips {
        note_selection(world, strip);
        sync_scroll_controls(world, strip);
        keep_visible(world, strip);
    }
}

pub(crate) fn publish(world: &mut World) {
    let picks: Vec<MenuPicked> = events::<MenuPicked>(world).to_vec();
    let strips = world.entities_with::<StripState>();
    for strip in strips {
        let menu = world.get::<StripState>(strip).and_then(|state| state.menu);
        let Some(menu) = menu else {
            continue;
        };
        if let Some(pick) = picks.iter().find(|pick| pick.menu == menu) {
            apply_menu(world, strip, TabId(pick.item));
        } else if !world.is_alive(menu) || world.has::<Hidden>(menu) {
            if let Some(state) = world.get_mut::<StripState>(strip) {
                state.menu = None;
            }
        }
    }
}

pub(crate) fn styles(world: &mut World) {
    let theme = world.resource::<UiTheme>().cloned().unwrap_or_default();
    let focus = world.resource::<Focus>().and_then(|focus| focus.entity);
    let strips = world.entities_with::<StripState>();
    for strip in strips {
        let Some(state) = world.get::<StripState>(strip).cloned() else {
            continue;
        };
        paint(world, strip, &theme, Token::Canvas);
        paint(world, state.body, &theme, Token::Content);
        let engaged = state.props.active || contains_focus(world, strip, focus);
        let hovered = world
            .resource::<Input>()
            .map(|input| input.pointer.position)
            .and_then(|point| hit_test(world, point[0], point[1]))
            .and_then(|hit| ancestor_with::<TabSlot>(world, hit));
        let selected = world.get::<Tabs>(state.list).map(|tabs| tabs.selected);
        for (tab, _) in tabs_of(world, state.list) {
            let is_selected = world
                .get::<Tab>(tab)
                .is_some_and(|header| Some(header.id) == selected);
            let token = if !state.props.enabled {
                Token::Navigation
            } else if is_selected && engaged {
                Token::Selected
            } else if is_selected {
                Token::Raised
            } else if hovered == Some(tab) {
                Token::ControlHover
            } else {
                Token::Navigation
            };
            paint(world, tab, &theme, token);
            let ink = theme.color(if state.props.enabled {
                Token::Text
            } else {
                Token::Disabled
            });
            tint_labels(world, tab, ink);
            if state.props.enabled && focus == Some(tab) {
                world.insert(
                    tab,
                    Border {
                        color: theme.color(Token::Focus),
                        width: 2.0,
                    },
                );
            } else {
                world.remove::<Border>(tab);
            }
            if let Some(slot) = world.get::<TabSlot>(tab).copied() {
                paint(world, slot.dirty, &theme, Token::WarningFill);
            }
        }
        for button in [state.leading, state.trailing, state.overflow] {
            let enabled = world
                .get::<Button>(button)
                .is_some_and(|button| button.enabled);
            let ink = theme.color(if enabled && state.props.enabled {
                Token::Text
            } else {
                Token::Disabled
            });
            if let Some(label) = world.get_mut::<Label>(button) {
                label.color = Some(ink);
            }
        }
    }
}

pub(crate) fn key_at(world: &World, entity: Entity) -> Option<UiId> {
    world.get::<StripState>(entity).map(|state| state.key)
}

fn note_drop(world: &mut World) {
    let Some(drop) = committed(world) else {
        return;
    };
    if drop.payload.kind != TAB {
        return;
    }
    let id = TabId(drop.payload.value);
    let Some(source) = drop.payload.owner else {
        return;
    };
    if let Some(slot) = world.get::<TabSlot>(drop.target).copied() {
        let name = drag_name(world, source, id);
        let target_name = drag_name(world, slot.strip, slot.id);
        if source == slot.strip {
            emit(
                world,
                source,
                TabStripEventKind::Reordered(TabMove {
                    id,
                    target: slot.id,
                    place: drop.place,
                }),
            );
            drag::announce(
                world,
                &format!(
                    "Moved {name} {} {target_name}.",
                    drag::place_word(drop.place)
                ),
            );
        } else {
            emit(
                world,
                slot.strip,
                TabStripEventKind::Moved(TabTransfer {
                    id,
                    from: source,
                    to: slot.strip,
                    target: Some(slot.id),
                    place: drop.place,
                }),
            );
            drag::announce(
                world,
                &format!(
                    "Moved {name} {} {target_name}.",
                    drag::place_word(drop.place)
                ),
            );
        }
        return;
    }
    if let Some(pane) = world.get::<PaneTarget>(drop.target).copied() {
        let name = drag_name(world, source, id);
        match drop.place {
            DropPlace::Edge(edge) => {
                let direction = SplitDirection::from_edge(edge);
                emit(
                    world,
                    pane.strip,
                    TabStripEventKind::SplitRequested(TabSplitRequest {
                        id,
                        strip: pane.strip,
                        direction,
                    }),
                );
                drag::announce(
                    world,
                    &format!("Split {name} {}.", direction_word(direction)),
                );
            }
            DropPlace::Inside => {
                emit(
                    world,
                    pane.strip,
                    TabStripEventKind::Moved(TabTransfer {
                        id,
                        from: source,
                        to: pane.strip,
                        target: None,
                        place: DropPlace::Inside,
                    }),
                );
                let label = world
                    .get::<StripState>(pane.strip)
                    .map(|state| state.props.label.clone())
                    .unwrap_or_default();
                drag::announce(world, &format!("Moved {name} to {label}."));
            }
            _ => {}
        }
    }
}

fn note_keys(world: &mut World) {
    let keys = world
        .resource::<Input>()
        .map(|input| input.keys.clone())
        .unwrap_or_default();
    if keys.is_empty() {
        return;
    }
    let mut used = Vec::new();
    for (index, event) in keys.iter().enumerate() {
        if event.released || event.repeat || event.synthetic {
            continue;
        }
        let handled = if reorder_chord(event) {
            let LogicalKey::Named(key) = event.logical else {
                continue;
            };
            apply_reorder_key(world, key)
        } else if close_chord(event) {
            apply_close_key(world)
        } else {
            false
        };
        if handled {
            used.push(index);
        }
    }
    if used.is_empty() {
        return;
    }
    if let Some(input) = world.resource_mut::<Input>() {
        input.keys = input
            .keys
            .iter()
            .cloned()
            .enumerate()
            .filter(|(index, _)| !used.contains(index))
            .map(|(_, event)| event)
            .collect();
        if input.keys.is_empty() {
            input.key = None;
        }
    }
}

fn reorder_chord(event: &KeyEvent) -> bool {
    event.modifiers.alt
        && !event.modifiers.command()
        && !event.modifiers.shift
        && matches!(
            event.logical,
            LogicalKey::Named(NamedKey::ArrowLeft | NamedKey::ArrowRight)
        )
}

fn close_chord(event: &KeyEvent) -> bool {
    if event.modifiers.alt || event.modifiers.shift {
        return false;
    }
    match event.logical {
        LogicalKey::Named(NamedKey::Delete) => !event.modifiers.command(),
        LogicalKey::Character('w' | 'W') => event.modifiers.command(),
        _ => false,
    }
}

fn apply_reorder_key(world: &mut World, key: NamedKey) -> bool {
    let Some((tab, slot)) = focused_tab(world) else {
        return false;
    };
    if world.has::<CapturesKeys>(tab) {
        return false;
    }
    let Some(state) = world.get::<StripState>(slot.strip).cloned() else {
        return false;
    };
    if !state.props.enabled {
        return true;
    }
    let index = state.specs.iter().position(|spec| spec.id == slot.id);
    let movement = index.and_then(|index| match key {
        NamedKey::ArrowLeft if index > 0 => Some((state.specs[index - 1].id, DropPlace::Before)),
        NamedKey::ArrowRight => state
            .specs
            .get(index + 1)
            .map(|spec| (spec.id, DropPlace::After)),
        _ => None,
    });
    let name = drag_name(world, slot.strip, slot.id);
    match movement {
        Some((target, place)) if reorder_tabs(&state.specs, slot.id, target, place).is_some() => {
            let target_name = drag_name(world, slot.strip, target);
            emit(
                world,
                slot.strip,
                TabStripEventKind::Reordered(TabMove {
                    id: slot.id,
                    target,
                    place,
                }),
            );
            drag::announce(
                world,
                &format!("Moved {name} {} {target_name}.", drag::place_word(place)),
            );
        }
        _ => drag::announce(world, &format!("Cannot move {name}.")),
    }
    true
}

fn apply_close_key(world: &mut World) -> bool {
    let Some(slot) = close_target(world) else {
        return false;
    };
    emit(
        world,
        slot.strip,
        TabStripEventKind::CloseRequested(slot.id),
    );
    true
}

fn note_middle(world: &mut World) {
    let Some(input) = world.resource::<Input>().cloned() else {
        return;
    };
    if !input.pointer.pressed || input.pointer.button != PointerButton::Middle {
        return;
    }
    let at = input.pointer.press_at.unwrap_or(input.pointer.position);
    let Some(hit) = hit_test(world, at[0], at[1]) else {
        return;
    };
    let Some(tab) = ancestor_with::<TabSlot>(world, hit) else {
        return;
    };
    if !tab_closable(world, tab) {
        return;
    }
    let Some(slot) = world.get::<TabSlot>(tab).copied() else {
        return;
    };
    emit(
        world,
        slot.strip,
        TabStripEventKind::CloseRequested(slot.id),
    );
}

fn note_selection(world: &mut World, strip: Entity) {
    let Some(state) = world.get::<StripState>(strip).cloned() else {
        return;
    };
    let selected = world
        .get::<Tabs>(state.list)
        .map(|tabs| TabId(tabs.selected))
        .filter(|id| state.specs.iter().any(|spec| spec.id == *id));
    if selected != state.published {
        if let Some(id) = selected {
            emit(world, strip, TabStripEventKind::Activated(id));
        }
        if let Some(state) = world.get_mut::<StripState>(strip) {
            state.published = selected;
            state.reveal = true;
        }
    }
}

fn keep_visible(world: &mut World, strip: Entity) {
    let Some(state) = world.get::<StripState>(strip).cloned() else {
        return;
    };
    let rect = world.get::<Rect>(state.scroll).copied().unwrap_or_default();
    if rect.w <= 0.0 {
        return;
    }
    let Some(selected) = state.published else {
        return;
    };
    let Some(index) = state.specs.iter().position(|spec| spec.id == selected) else {
        return;
    };
    let force =
        state.reveal || state.shown_index != Some(index) || (state.shown_port - rect.w).abs() > 0.5;
    if force {
        reveal_index(world, &state, index);
    }
    if let Some(state) = world.get_mut::<StripState>(strip) {
        state.shown_index = Some(index);
        state.shown_port = rect.w;
        state.reveal = false;
    }
}

fn sync_scroll_controls(world: &mut World, strip: Entity) {
    let Some(state) = world.get::<StripState>(strip).cloned() else {
        return;
    };
    let rect = world.get::<Rect>(state.scroll).copied().unwrap_or_default();
    let scroll = world
        .get::<Scroll>(state.scroll)
        .copied()
        .unwrap_or_default();
    let min_x = (rect.w - scroll.content[0]).min(0.0);
    let can_back = scroll.offset[0] < -0.5;
    let can_forward = rect.w > 0.0 && scroll.offset[0] > min_x + 0.5;
    set_button_enabled(world, state.leading, state.props.enabled && can_back);
    set_button_enabled(world, state.trailing, state.props.enabled && can_forward);
    let hidden = hidden_tabs(world, &state);
    set_button_enabled(
        world,
        state.overflow,
        state.props.enabled && !hidden.is_empty(),
    );
}

fn apply_menu(world: &mut World, strip: Entity, id: TabId) {
    let Some(state) = world.get::<StripState>(strip).cloned() else {
        return;
    };
    if !state.specs.iter().any(|spec| spec.id == id) {
        return;
    }
    world.insert(state.list, Tabs { selected: id.0 });
    show_selected(world, state.list, id);
    if let Some(tab) = tab_entity(world, state.list, id) {
        request_focus(world, tab);
    }
    if let Some(state) = world.get_mut::<StripState>(strip) {
        state.menu = None;
        state.reveal = true;
        state.shown_index = None;
    }
}

fn hidden_tabs(world: &World, state: &StripState) -> Vec<TabSpec> {
    let Some(port) = world.get::<Rect>(state.scroll).copied() else {
        return Vec::new();
    };
    if port.w <= 0.0 {
        return Vec::new();
    }
    state
        .specs
        .iter()
        .filter(|spec| {
            tab_entity(world, state.list, spec.id)
                .and_then(|tab| world.get::<Rect>(tab).copied())
                .is_some_and(|rect| {
                    rect.w > 0.0
                        && (rect.x < port.x - 1.0 || rect.x + rect.w > port.x + port.w + 1.0)
                })
        })
        .cloned()
        .collect()
}

fn focused_tab(world: &World) -> Option<(Entity, TabSlot)> {
    let focus = world.resource::<Focus>().and_then(|focus| focus.entity)?;
    if world.has::<CapturesKeys>(focus) {
        return None;
    }
    let tab = if world.has::<TabSlot>(focus) {
        focus
    } else {
        ancestor_with::<TabSlot>(world, focus)?
    };
    world.get::<TabSlot>(tab).copied().map(|slot| (tab, slot))
}

fn close_target(world: &World) -> Option<TabSlot> {
    let focus = world.resource::<Focus>().and_then(|focus| focus.entity)?;
    if world.has::<CapturesKeys>(focus) {
        return None;
    }
    if let Some((_, slot)) = focused_tab(world) {
        return tab_closable(
            world,
            tab_entity(world, world.get::<StripState>(slot.strip)?.list, slot.id)?,
        )
        .then_some(slot);
    }
    let strip = ancestor_with::<StripState>(world, focus)?;
    let state = world.get::<StripState>(strip)?;
    if !state.props.enabled {
        return None;
    }
    let selected = TabId(world.get::<Tabs>(state.list)?.selected);
    let tab = tab_entity(world, state.list, selected)?;
    tab_closable(world, tab)
        .then(|| world.get::<TabSlot>(tab).copied())
        .flatten()
}

fn tab_closable(world: &World, tab: Entity) -> bool {
    world.get::<TabSlot>(tab).is_some_and(|slot| slot.closable)
        && access_details(world, tab).activate
}

fn tab_id(world: &World, tab: Entity) -> TabId {
    world
        .get::<TabSlot>(tab)
        .map(|slot| slot.id)
        .unwrap_or(TabId(0))
}

fn tab_entity(world: &World, list: Entity, id: TabId) -> Option<Entity> {
    tabs_of(world, list)
        .into_iter()
        .find(|(_, slot)| slot.id == id)
        .map(|(tab, _)| tab)
}

fn tabs_of(world: &World, list: Entity) -> Vec<(Entity, TabSlot)> {
    children_of(world, list)
        .into_iter()
        .filter_map(|tab| world.get::<TabSlot>(tab).copied().map(|slot| (tab, slot)))
        .collect()
}

fn drag_name(world: &World, strip: Entity, id: TabId) -> String {
    world
        .get::<StripState>(strip)
        .and_then(|state| state.specs.iter().find(|spec| spec.id == id))
        .map(|spec| spec.label.clone())
        .unwrap_or_default()
}

fn direction_word(direction: SplitDirection) -> &'static str {
    match direction {
        SplitDirection::Left => "left",
        SplitDirection::Right => "right",
        SplitDirection::Above => "above",
        SplitDirection::Below => "below",
    }
}

fn emit(world: &mut World, strip: Entity, kind: TabStripEventKind) {
    send(world, TabStripEvent { strip, kind });
}

fn set_shown(world: &mut World, entity: Entity, shown: bool) {
    if shown {
        world.remove::<Hidden>(entity);
    } else if !world.has::<Hidden>(entity) {
        world.insert(entity, Hidden);
    }
}

fn set_button_enabled(world: &mut World, entity: Entity, enabled: bool) {
    if let Some(button) = world.get_mut::<Button>(entity) {
        button.enabled = enabled;
    }
    if enabled {
        world.remove::<ControlDisabled>(entity);
    } else {
        world.insert(entity, ControlDisabled);
    }
}

fn paint(world: &mut World, entity: Entity, theme: &UiTheme, token: Token) {
    world.insert(entity, theme.color(token));
}

fn tint_labels(world: &mut World, tab: Entity, color: Color) {
    for child in children_of(world, tab) {
        if let Some(label) = world.get_mut::<Label>(child) {
            label.color = Some(color);
        }
    }
}

fn contains_focus(world: &World, ancestor: Entity, focus: Option<Entity>) -> bool {
    let Some(mut current) = focus else {
        return false;
    };
    for _ in 0..64 {
        if current == ancestor {
            return true;
        }
        match world.get::<Parent>(current).map(|parent| parent.entity) {
            Some(parent) if world.is_alive(parent) => current = parent,
            _ => return false,
        }
    }
    false
}

fn order_children(world: &mut World, parent: Entity, order: &[Entity]) {
    if let Some(children) = world.get_mut::<Children>(parent) {
        children.list = order.to_vec();
    } else if !order.is_empty() {
        world.insert(
            parent,
            Children {
                list: order.to_vec(),
            },
        );
    }
}

fn subtree(world: &World, root: Entity) -> Vec<Entity> {
    let mut out = vec![root];
    let mut index = 0;
    while index < out.len() {
        out.extend(children_of(world, out[index]));
        index += 1;
    }
    out
}

#[derive(Clone)]
struct StripState {
    key: UiId,
    props: TabStripProps,
    specs: Vec<TabSpec>,
    list: Entity,
    scroll: Entity,
    body: Entity,
    leading: Entity,
    trailing: Entity,
    overflow: Entity,
    menu: Option<Entity>,
    published: Option<TabId>,
    reveal: bool,
    shown_index: Option<usize>,
    shown_port: f32,
}
impl Component for StripState {}

#[derive(Clone, Copy)]
struct TabSlot {
    strip: Entity,
    id: TabId,
    panel: Entity,
    icon: Entity,
    pin: Entity,
    title: Entity,
    dirty: Entity,
    close: Entity,
    closable: bool,
}
impl Component for TabSlot {}

#[derive(Clone, Copy)]
struct PaneTarget {
    strip: Entity,
}
impl Component for PaneTarget {}

#[derive(Clone, Copy)]
struct CloseButton {
    strip: Entity,
    tab: Entity,
}
impl Component for CloseButton {}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ChromeKind {
    Leading,
    Trailing,
    Overflow,
}

#[derive(Clone, Copy)]
struct ChromeButton {
    strip: Entity,
    kind: ChromeKind,
}
impl Component for ChromeButton {}
