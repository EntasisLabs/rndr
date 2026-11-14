//! Retained, application-owned data browsers. See docs/data-components.md.
//! No Shell, filesystem, global Selection, or pooled index identity is used.
mod selection_membership;
mod virtual_list;
use crate::{text, Corner, Token, Typography, UiError, UiId, UiRecipe, UiStyle, UiTheme};
use rndr_core::*;
use selection_membership::{gesture, reconcile, Membership, MembershipChange};
use std::collections::{BTreeMap, BTreeSet};
pub use virtual_list::{VariableAnchor, VirtualCompose, VirtualListPolicy, VirtualViewHandle};

/// Deliberate retained-data safety budget, not a virtualization claim.
pub const MAX_ITEMS: usize = 1024;
/// Stable application identity, scoped to one collection. Never use the row index.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ItemId(pub u64);
/// `None` and `Single` keep their previous behavior. `Multiple` and `Range`
/// are additional modes. This crate is 0.y: an exhaustive match outside the
/// workspace has to grow an arm for each new variant. Hosts that set `None`
/// or `Single` are unchanged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectionMode {
    /// Focus and activation only.
    None,
    /// Zero or one selected ID; focus does not follow selection.
    Single,
    /// Toggle an enabled id on the selection gesture. Arrows do not change the set.
    Multiple,
    /// A plain gesture selects one enabled id and moves the range anchor.
    /// Shift selects the enabled ids from that anchor through the gesture id
    /// in current logical order and does not move the anchor.
    Range,
}
/// Application-supplied metadata, independent of the composed visible content.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    /// Stable identity.
    pub id: ItemId,
    /// Persistent accessible name, including any relevant metadata.
    pub name: String,
    /// Disabled rows remain visible but cannot focus, select or activate.
    /// In a tree, disabled ancestry also disables descendants.
    pub enabled: bool,
}
impl Item {
    /// Enabled item.
    pub fn new(id: u64, name: impl Into<String>) -> Self {
        Self {
            id: ItemId(id),
            name: name.into(),
            enabled: true,
        }
    }
}
/// Flat preorder node description. Parent must precede and contain its descendants
/// contiguously. No lazy loading or hidden data ownership is implied.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TreeNode {
    /// Stable row metadata.
    pub item: Item,
    /// Parent identity, or None for a root.
    pub parent: Option<ItemId>,
}
/// Component properties; mount again to reconcile data/props without replacing IDs.
#[derive(Clone, Debug)]
pub struct ViewProps {
    /// Nonempty persistent collection name.
    pub name: String,
    /// Plain-language empty/no-results content.
    pub empty: String,
    /// Entire collection availability.
    pub enabled: bool,
    /// Single by default. No automatic initial selection.
    pub selection: SelectionMode,
    /// When true, an enabled visible row can be dragged onto another row in
    /// this view. The view emits [`CollectionEventKind::Reordered`] and does
    /// not change its order. Default false, so a press past the scroll slop
    /// still scrolls.
    pub reorder: bool,
}
impl ViewProps {
    /// Enabled single-select view with an empty state.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            empty: "No items to display".into(),
            enabled: true,
            selection: SelectionMode::Single,
            reorder: false,
        }
    }

    /// Enable [`Self::reorder`].
    pub fn with_reorder(mut self) -> Self {
        self.reorder = true;
        self
    }
}
/// Observable state transitions. Input sends are visible to the host on Update;
/// programmatic mount/setter sends become visible on the following frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CollectionEvent {
    /// Generation-checked collection root.
    pub view: Entity,
    /// Explicit transition; activation never silently selects.
    pub kind: CollectionEventKind,
}
impl Event for CollectionEvent {}
/// Separate focus, selection, activation and disclosure transitions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CollectionEventKind {
    /// Actual canonical focus entered/left a row (None includes leaving the view).
    Focused(Option<ItemId>),
    /// Selection changed or was reconciled away. `None` and `Single` only.
    /// The value is the one selected id, or `None` when the slot is clear.
    Selected(Option<ItemId>),
    /// `Multiple` or `Range` membership changed. The event does not carry the
    /// ids. Read them from the list or table handle.
    SelectionSetChanged,
    /// Enter requested host activation, independently of selection. A virtual
    /// list may explicitly opt into palette double-click activation too.
    /// This is not the screen-reader Activate action: on a row that follows
    /// canonical click/Space selection (or no selection in None mode).
    Activated(ItemId),
    /// Branch expansion changed.
    Expanded(ItemId, bool),
    /// The host should apply [`reorder_list`] or [`reorder_tree`] and mount
    /// again. The view order is unchanged until that mount.
    Reordered(ItemMove),
}
/// A requested move. `place` comes from [`crate::drag::DropPlace`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ItemMove {
    /// Row that moved.
    pub id: ItemId,
    /// Row it was dropped relative to.
    pub target: ItemId,
    /// Before, after, or inside `target`. Edge places are not emitted.
    pub place: crate::drag::DropPlace,
}
/// Composed slots. Retained ListView/TreeView support passive content with a
/// stable subtree lifetime. `mount_virtual` and `mount_variable` support nested
/// controls but recycle slots: keep application state by ItemId and never persist
/// these entities.
#[derive(Clone, Copy, Debug)]
pub struct RowSlot {
    /// Managed canonical interaction target. Do not remove/reparent/replace.
    pub root: Entity,
    /// Caller-populated subtree. Retained: preserved while this ID is present.
    /// Virtual: ephemeral; destroyed before recycling to another key.
    pub content: Entity,
}
#[derive(Clone)]
struct Row {
    item: Item,
    parent: Option<ItemId>,
    depth: usize,
    slot: RowSlot,
    inner: Entity,
    chevron: Entity,
    branch: bool,
    visible: bool,
}
#[derive(Clone)]
struct View {
    key: UiId,
    tree: bool,
    props: ViewProps,
    rows: BTreeMap<ItemId, Row>,
    order: Vec<ItemId>,
    expanded: BTreeSet<ItemId>,
    selected: Option<ItemId>,
    membership: Membership,
    active: Option<ItemId>,
    reported_focus: Option<ItemId>,
    empty: Entity,
}
impl Component for View {}
#[derive(Clone, Copy)]
struct RowOwner {
    view: Entity,
    id: ItemId,
    toggle: bool,
}
impl Component for RowOwner {}
/// Generation-checked shared list/tree operations. The root is the core Scroll
/// viewport: set its Layout height/width in your composition after first mount.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ViewHandle {
    /// Managed viewport. Do not reparent internal slots or write managed components.
    pub root: Entity,
}
/// Composable stable-ID flat collection.
pub struct ListView;
/// Composable stable-ID hierarchical collection.
pub struct TreeView;
impl ListView {
    /// Reconcile metadata and call `compose` once per supplied row. Reuse children
    /// in each content slot; callbacks are not run on every frame or scroll.
    /// Invalid/duplicate/oversize input is rejected before any mutation.
    pub fn mount(
        world: &mut World,
        parent: Entity,
        key: UiId,
        props: ViewProps,
        items: &[Item],
        compose: impl FnMut(&mut World, RowSlot, &Item),
    ) -> Result<ViewHandle, UiError> {
        let nodes: Vec<_> = items
            .iter()
            .cloned()
            .map(|item| TreeNode { item, parent: None })
            .collect();
        mount(world, parent, key, props, &nodes, false, compose)
    }
}
impl TreeView {
    /// Reconcile a preorder forest; maximum depth 32 and MAX_ITEMS total nodes.
    /// Collapsed descendants retain content and selection but are hidden from
    /// layout, hit testing, Tab and accessibility. Focus moves to the ancestor.
    pub fn mount(
        world: &mut World,
        parent: Entity,
        key: UiId,
        props: ViewProps,
        nodes: &[TreeNode],
        compose: impl FnMut(&mut World, RowSlot, &Item),
    ) -> Result<ViewHandle, UiError> {
        mount(world, parent, key, props, nodes, true, compose)
    }
}
pub(crate) fn key_at(world: &World, e: Entity) -> Option<UiId> {
    world
        .get::<View>(e)
        .map(|v| v.key)
        .or_else(|| virtual_list::key_at(world, e))
}
pub(crate) fn owns_entity(world: &World, e: Entity) -> bool {
    virtual_list::owns(world, e)
        || world.has::<View>(e)
        || world.has::<RowOwner>(e)
        || ancestor_with::<View>(world, e).is_some()
}
fn emit(world: &mut World, view: Entity, kind: CollectionEventKind) {
    send(world, CollectionEvent { view, kind });
}
fn mount(
    world: &mut World,
    parent: Entity,
    key: UiId,
    props: ViewProps,
    nodes: &[TreeNode],
    tree: bool,
    mut compose: impl FnMut(&mut World, RowSlot, &Item),
) -> Result<ViewHandle, UiError> {
    if !world.is_alive(parent) || !world.has::<Layout>(parent) {
        return Err(UiError::Stale);
    }
    if nodes.len() > MAX_ITEMS || props.name.trim().is_empty() || props.empty.trim().is_empty() {
        return Err(UiError::InvalidProps);
    }
    let mut depths = BTreeMap::new();
    let mut path = Vec::new();
    for node in nodes {
        if node.item.name.trim().is_empty() || depths.contains_key(&node.item.id) {
            return Err(UiError::InvalidProps);
        }
        let depth = if let Some(p) = node.parent {
            let Some(i) = path.iter().position(|id| *id == p) else {
                return Err(UiError::InvalidProps);
            };
            path.truncate(i + 1);
            i + 1
        } else {
            path.clear();
            0
        };
        if depth >= 32 {
            return Err(UiError::InvalidProps);
        }
        depths.insert(node.item.id, depth);
        path.push(node.item.id);
    }
    let existing = children_of(world, parent).into_iter().find(|e| {
        key_at(world, *e)
            .or_else(|| crate::controls::key_at(world, *e))
            .or_else(|| crate::surfaces::key_at(world, *e))
            == Some(key)
    });
    let root = if let Some(e) = existing {
        let v = world.get::<View>(e).ok_or(UiError::IdentityConflict)?;
        if v.tree != tree {
            return Err(UiError::IdentityConflict);
        }
        validate(world, e)?;
        e
    } else {
        let e = world.spawn((
            Layout {
                width: Size::Fill,
                height: Size::Px(280.0),
                padding: 4.0,
                gap: 2.0,
                ..Layout::column()
            },
            Scroll::default(),
            Focusable,
            ResolvedColor,
        ));
        set_parent(world, e, parent);
        let empty = text(world, e, &props.empty, Typography::Secondary)?;
        world.get_mut::<Layout>(empty).unwrap().height = Size::Fill;
        world.insert(
            empty,
            LabelBox {
                inset_x: 8.0,
                inset_y: 8.0,
                min_height: 0.0,
            },
        );
        world.insert(
            e,
            View {
                key,
                tree,
                props: props.clone(),
                rows: BTreeMap::new(),
                order: vec![],
                expanded: BTreeSet::new(),
                selected: None,
                membership: Membership::default(),
                active: None,
                reported_focus: None,
                empty,
            },
        );
        e
    };
    world.insert(root, KeyNavigation(navigate));
    let mut view = world.get::<View>(root).unwrap().clone();
    let focused = focused_id(world, root);
    let old_index = focused
        .and_then(|id| view.order.iter().position(|i| *i == id))
        .unwrap_or(0);
    let ids: BTreeSet<_> = nodes.iter().map(|n| n.item.id).collect();
    let removed: Vec<_> = view
        .rows
        .keys()
        .filter(|id| !ids.contains(id))
        .copied()
        .collect();
    for id in removed {
        let row = view.rows.remove(&id).unwrap();
        let mut owned = vec![row.slot.root];
        descendants(world, row.slot.root, &mut owned);
        forget_interaction_targets(world, &owned);
        despawn(world, row.slot.root);
    }
    view.expanded.retain(|id| ids.contains(id));
    view.props = props;
    let old_order = view.order.clone();
    view.order = nodes.iter().map(|n| n.item.id).collect();
    let branches: BTreeSet<_> = nodes.iter().filter_map(|n| n.parent).collect();
    // An ID can survive while its last child disappears. Retire disclosure
    // state too: a leaf must not trap Left in an impossible collapse operation.
    let retired: Vec<_> = view
        .expanded
        .iter()
        .filter(|id| !branches.contains(id))
        .copied()
        .collect();
    for id in retired {
        view.expanded.remove(&id);
        emit(world, root, CollectionEventKind::Expanded(id, false));
    }
    for node in nodes {
        let id = node.item.id;
        let depth = depths[&id];
        if let std::collections::btree_map::Entry::Vacant(entry) = view.rows.entry(id) {
            let row = world.spawn((
                Layout {
                    width: Size::Fill,
                    padding: 3.0,
                    ..Layout::column()
                },
                Button::new("rndr-ui:select"),
                ReleaseActivation,
                Focusable,
                ResolvedColor,
            ));
            set_parent(world, row, root);
            world.insert(
                row,
                RowOwner {
                    view: root,
                    id,
                    toggle: false,
                },
            );
            world.insert(row, KeyNavigation(navigate));
            let inner = world.spawn((Layout {
                width: Size::Fill,
                gap: 6.0,
                ..Layout::row()
            },));
            set_parent(world, inner, row);
            let chevron = world.spawn((
                Layout {
                    width: Size::Px(22.0),
                    height: Size::Px(26.0),
                    ..Layout::column()
                },
                Label::new(""),
                LabelBox::CONTENT,
                Button::new("rndr-ui:expand"),
                ReleaseActivation,
                TabStop(false),
            ));
            world.insert(
                chevron,
                RowOwner {
                    view: root,
                    id,
                    toggle: true,
                },
            );
            world.insert(chevron, KeyNavigation(navigate));
            set_parent(world, chevron, inner);
            let content = world.spawn((Layout {
                width: Size::Fill,
                ..Layout::column()
            },));
            set_parent(world, content, inner);
            entry.insert(Row {
                item: node.item.clone(),
                parent: node.parent,
                depth,
                slot: RowSlot { root: row, content },
                inner,
                chevron,
                branch: false,
                visible: true,
            });
        }
        let row = view.rows.get_mut(&id).unwrap();
        if row.item.enabled != node.item.enabled
            || row.parent != node.parent
            || row.branch != branches.contains(&id)
        {
            forget_interaction_targets(world, &[row.slot.root, row.chevron]);
        }
        row.item = node.item.clone();
        row.parent = node.parent;
        row.depth = depth;
        row.branch = branches.contains(&id);
        compose(world, row.slot, &row.item);
    }
    // Reorder canonical child edges, preserving each row entity and its subtree.
    world.insert(
        root,
        Children {
            list: std::iter::once(view.empty)
                .chain(view.order.iter().map(|id| view.rows[id].slot.root))
                .collect(),
        },
    );
    let label = world.get_mut::<Label>(view.empty).unwrap();
    label.text = view.props.empty.clone();
    label.overflow = TextOverflow::Wrap;
    if (view.selected.is_some_and(|id| !node_enabled(&view, id))
        || view.props.selection == SelectionMode::None)
        && view.selected.take().is_some()
    {
        emit(world, root, CollectionEventKind::Selected(None));
    }
    if matches!(
        view.props.selection,
        SelectionMode::Multiple | SelectionMode::Range
    ) {
        view.selected = None;
    }
    let logical_order = view.order.clone();
    let enabled_now: Vec<ItemId> = logical_order
        .iter()
        .copied()
        .filter(|id| view.props.enabled && node_enabled(&view, *id))
        .collect();
    let mode = view.props.selection;
    let change = reconcile(
        mode,
        &mut view.membership,
        &old_order,
        &logical_order,
        |id| enabled_now.contains(&id),
    );
    if change == MembershipChange::Set {
        emit(world, root, CollectionEventKind::SelectionSetChanged);
    }
    sync(world, root, &mut view);
    if let Some(id) = focused {
        let target = safe_target(&view, id).or_else(|| {
            available(&view)
                .get(old_index.min(available(&view).len().saturating_sub(1)))
                .copied()
        });
        focus_target(world, root, &mut view, target);
    } else if world.resource::<Focus>().and_then(|f| f.entity) == Some(root) {
        // Empty view receives rows: keep the canonical focus safe and useful.
        let target = available(&view).first().copied();
        focus_target(world, root, &mut view, target);
    }
    world.insert(root, view);
    Ok(ViewHandle { root })
}
fn validate(world: &World, root: Entity) -> Result<(), UiError> {
    let view = world.get::<View>(root).ok_or(UiError::Stale)?;
    if !world.has::<Scroll>(root)
        || !world.has::<Layout>(root)
        || !world.has::<Label>(view.empty)
        || !world.has::<Layout>(view.empty)
    {
        return Err(UiError::Stale);
    }
    for row in view.rows.values() {
        if [row.slot.root, row.inner, row.slot.content, row.chevron]
            .iter()
            .any(|e| !world.has::<Layout>(*e))
            || !world.has::<Label>(row.chevron)
            || world.has::<Button>(row.chevron) != row.branch
            || !world.has::<Button>(row.slot.root)
            || world.get::<Parent>(row.slot.root).map(|p| p.entity) != Some(root)
            || world.get::<Parent>(row.inner).map(|p| p.entity) != Some(row.slot.root)
            || world.get::<Parent>(row.slot.content).map(|p| p.entity) != Some(row.inner)
            || world.get::<Parent>(row.chevron).map(|p| p.entity) != Some(row.inner)
        {
            return Err(UiError::Stale);
        }
    }
    Ok(())
}
fn descendants(world: &World, e: Entity, out: &mut Vec<Entity>) {
    for c in children_of(world, e) {
        out.push(c);
        descendants(world, c, out);
    }
}
fn enabled(view: &View, id: ItemId) -> bool {
    view.props.enabled && node_enabled(view, id)
}
fn node_enabled(view: &View, mut id: ItemId) -> bool {
    for _ in 0..32 {
        let Some(row) = view.rows.get(&id) else {
            return false;
        };
        if !row.item.enabled {
            return false;
        }
        let Some(parent) = row.parent else {
            return true;
        };
        id = parent;
    }
    false
}
fn row_chosen(view: &View, id: ItemId) -> bool {
    match view.props.selection {
        SelectionMode::None => false,
        SelectionMode::Single => view.selected == Some(id),
        SelectionMode::Multiple | SelectionMode::Range => view.membership.ids.contains(&id),
    }
}
fn gesture_shift(world: &World) -> bool {
    world.resource::<Input>().is_some_and(|input| input.shift)
}
fn available(view: &View) -> Vec<ItemId> {
    view.order
        .iter()
        .copied()
        .filter(|id| view.rows[id].visible && enabled(view, *id))
        .collect()
}
fn safe_target(view: &View, mut id: ItemId) -> Option<ItemId> {
    for _ in 0..32 {
        let r = view.rows.get(&id)?;
        if r.visible && enabled(view, id) {
            return Some(id);
        }
        id = r.parent?;
    }
    None
}
fn focused_id(world: &World, root: Entity) -> Option<ItemId> {
    let focus = world.resource::<Focus>()?.entity?;
    let owner = world.get::<RowOwner>(focus)?;
    (owner.view == root).then_some(owner.id)
}
fn focus_target(world: &mut World, root: Entity, view: &mut View, id: Option<ItemId>) {
    let target = id.map(|id| view.rows[&id].slot.root).unwrap_or(root);
    if request_focus(world, target) {
        view.active = id;
        for (row_id, row) in &view.rows {
            world.insert(row.slot.root, TabStop(Some(*row_id) == id));
        }
    } else if world
        .resource::<Focus>()
        .and_then(|f| f.entity)
        .is_some_and(|f| f == root || world.get::<RowOwner>(f).is_some_and(|o| o.view == root))
    {
        cancel_activation(world);
        world.resource_mut::<Focus>().unwrap().entity = None;
    }
}
fn sync(world: &mut World, root: Entity, view: &mut View) {
    let visible: BTreeMap<_, _> = view
        .order
        .iter()
        .map(|id| {
            let mut p = view.rows[id].parent;
            let mut visible = true;
            while let Some(parent) = p {
                visible &= view.expanded.contains(&parent);
                p = view.rows[&parent].parent;
            }
            (*id, visible)
        })
        .collect();
    // Only newly hidden owned subtrees lose armed/queued interactions. In
    // particular, expansion must not cancel an unrelated host's Space release.
    let mut invalidated = Vec::new();
    for (id, row) in &mut view.rows {
        if row.visible && !visible[id] {
            invalidated.push(row.slot.root);
            descendants(world, row.slot.root, &mut invalidated);
        }
        row.visible = visible[id];
    }
    forget_interaction_targets(world, &invalidated);
    let available = available(view);
    if !view.active.is_some_and(|id| available.contains(&id)) {
        view.active = available.first().copied();
    }
    let focused = focused_id(world, root);
    if let Some(id) = focused.filter(|id| available.contains(id)) {
        view.active = Some(id);
    }
    world.insert(root, TabStop(available.is_empty()));
    world.insert(
        root,
        CollectionAccess {
            role: if view.tree {
                AccessRole::Tree
            } else {
                AccessRole::List
            },
            expanded: None,
            level: None,
            parent: None,
        },
    );
    world.insert(root, Semantics { name: Some(view.props.name.clone()), description: "Arrows move focus. Space or click selects. Enter activates. Tree Left/Right collapses/expands.".into(), readonly: false });
    if view.props.enabled {
        world.remove::<ControlDisabled>(root);
    } else {
        world.insert(root, ControlDisabled);
    }
    if view.order.is_empty() {
        world.remove::<Hidden>(view.empty);
    } else {
        world.insert(view.empty, Hidden);
    }
    for (id, row) in &view.rows {
        let enabled = enabled(view, *id);
        if row.visible {
            world.remove::<Hidden>(row.slot.root);
        } else {
            world.insert(row.slot.root, Hidden);
        }
        world.insert(
            row.slot.root,
            Button {
                action: "rndr-ui:select".into(),
                enabled,
            },
        );
        world.insert(row.slot.root, ActivationHandler(activate));
        world.insert(row.slot.root, SelectedState(row_chosen(view, *id)));
        world.insert(row.slot.root, TabStop(view.active == Some(*id)));
        world.insert(
            row.slot.root,
            Semantics {
                name: Some(row.item.name.clone()),
                ..Semantics::default()
            },
        );
        world.insert(
            row.slot.root,
            CollectionAccess {
                role: if view.tree {
                    AccessRole::TreeItem
                } else {
                    AccessRole::ListItem
                },
                expanded: row.branch.then_some(view.expanded.contains(id)),
                level: view.tree.then_some(row.depth + 1),
                parent: row.parent.map(|p| view.rows[&p].slot.root),
            },
        );
        world.get_mut::<Layout>(row.chevron).unwrap().width =
            Size::Px(32.0 + row.depth as f32 * 16.0);
        world.get_mut::<Layout>(row.chevron).unwrap().height = Size::Px(32.0);
        let inset_y = if world
            .resource::<UiTheme>()
            .is_some_and(|t| t.density == crate::Density::Compact)
        {
            4.0
        } else {
            7.0
        };
        world.insert(
            row.chevron,
            LabelBox {
                inset_x: 6.0 + row.depth as f32 * 16.0,
                inset_y,
                min_height: 32.0,
            },
        );
        if row.branch {
            world.insert(
                row.chevron,
                Button {
                    action: "rndr-ui:expand".into(),
                    enabled,
                },
            );
            world.insert(row.chevron, ReleaseActivation);
            world.insert(row.chevron, ActivationHandler(activate));
            world.insert(row.chevron, KeyNavigation(navigate));
            world.insert(
                row.chevron,
                RowOwner {
                    view: root,
                    id: *id,
                    toggle: true,
                },
            );
        } else {
            // A leaf's indent is passive hit area belonging to its row, not a
            // disabled button that swallows pointer selection.
            world.remove::<Button>(row.chevron);
            world.remove::<ReleaseActivation>(row.chevron);
            world.remove::<ActivationHandler>(row.chevron);
            world.remove::<KeyNavigation>(row.chevron);
            world.remove::<RowOwner>(row.chevron);
        }
        world.insert(
            row.chevron,
            Semantics {
                name: Some(format!(
                    "{} {}",
                    if view.expanded.contains(id) {
                        "Collapse"
                    } else {
                        "Expand"
                    },
                    row.item.name
                )),
                ..Semantics::default()
            },
        );
        world.get_mut::<Label>(row.chevron).unwrap().text = if row.branch {
            if view.expanded.contains(id) {
                "−"
            } else {
                "+"
            }
        } else {
            ""
        }
        .into();
        if view.tree {
            world.remove::<Hidden>(row.chevron);
        } else {
            world.insert(row.chevron, Hidden);
        }
    }
    bind_drag(world, root, view);
}
impl ViewHandle {
    /// Validate generation and the owned interaction/content slot topology.
    /// Wrapping compositions use this before their own atomic reconciliation.
    pub fn validate(self, world: &World) -> Result<(), UiError> {
        validate(world, self.root)
    }

    /// Current selected stable ID for `None` and `Single`.
    /// `Multiple` and `Range` keep this empty; read [`Self::selection`].
    pub fn selected(self, world: &World) -> Option<ItemId> {
        world.get::<View>(self.root).and_then(|v| v.selected)
    }
    /// Ids in the `Multiple` or `Range` set. Empty for `None` and `Single`.
    pub fn selection(self, world: &World) -> BTreeSet<ItemId> {
        world
            .get::<View>(self.root)
            .map(|view| view.membership.ids.clone())
            .unwrap_or_default()
    }
    /// Range origin. `None` when the mode is not `Range` or the anchor was cleared.
    pub fn range_anchor(self, world: &World) -> Option<ItemId> {
        let view = world.get::<View>(self.root)?;
        if view.props.selection != SelectionMode::Range {
            return None;
        }
        view.membership.anchor
    }
    /// Canonical focused row, including a disclosure button's owner.
    pub fn focused(self, world: &World) -> Option<ItemId> {
        focused_id(world, self.root)
    }
    /// Stable row slots (including collapsed nodes); None for absent/stale IDs.
    pub fn row(self, world: &World, id: ItemId) -> Option<RowSlot> {
        validate(world, self.root).ok()?;
        world.get::<View>(self.root)?.rows.get(&id).map(|r| r.slot)
    }
    /// Branch expansion state.
    pub fn expanded(self, world: &World, id: ItemId) -> bool {
        world
            .get::<View>(self.root)
            .is_some_and(|v| v.expanded.contains(&id))
    }
    /// Select an enabled present ID without focusing or activating. None clears.
    /// `None` and `Single` keep this contract. `Multiple` and `Range` reject
    /// `Some` here: membership changes on the selection gesture, and `None`
    /// clears the set.
    pub fn select(self, world: &mut World, id: Option<ItemId>) -> Result<(), UiError> {
        validate(world, self.root)?;
        let mut v = world.get::<View>(self.root).unwrap().clone();
        if matches!(
            v.props.selection,
            SelectionMode::Multiple | SelectionMode::Range
        ) {
            if id.is_some() || !v.props.enabled {
                return Err(UiError::InvalidProps);
            }
            if v.membership.ids.is_empty() && v.membership.anchor.is_none() {
                return Ok(());
            }
            v.membership.ids.clear();
            v.membership.anchor = None;
            emit(world, self.root, CollectionEventKind::SelectionSetChanged);
            sync(world, self.root, &mut v);
            world.insert(self.root, v);
            return Ok(());
        }
        if !v.props.enabled
            || (id.is_some()
                && (v.props.selection == SelectionMode::None
                    || !id.is_some_and(|id| enabled(&v, id))))
        {
            return Err(UiError::InvalidProps);
        }
        if v.selected != id {
            v.selected = id;
            emit(world, self.root, CollectionEventKind::Selected(id));
        }
        sync(world, self.root, &mut v);
        world.insert(self.root, v);
        Ok(())
    }
    /// Expand/collapse an enabled branch. A collapsing focused descendant moves
    /// to this ancestor; no hidden row keeps focus. Selection remains separate.
    pub fn set_expanded(self, world: &mut World, id: ItemId, open: bool) -> Result<(), UiError> {
        validate(world, self.root)?;
        let mut v = world.get::<View>(self.root).unwrap().clone();
        if !enabled(&v, id) || !v.rows.get(&id).is_some_and(|r| r.branch) {
            return Err(UiError::InvalidProps);
        }
        let focused = focused_id(world, self.root);
        let changed = if open {
            v.expanded.insert(id)
        } else {
            v.expanded.remove(&id)
        };
        if changed {
            emit(world, self.root, CollectionEventKind::Expanded(id, open));
        }
        sync(world, self.root, &mut v);
        if let Some(f) = focused {
            let target = safe_target(&v, f);
            // Preserve a surviving canonical target, including an armed
            // disclosure in another branch. Normalize only invalidated focus.
            let still_focusable = world
                .resource::<Focus>()
                .and_then(|focus| focus.entity)
                .is_some_and(|entity| access_details(world, entity).focus);
            if target != Some(f) || !still_focusable {
                focus_target(world, self.root, &mut v, target);
            }
        }
        world.insert(self.root, v);
        Ok(())
    }
    /// Focus and reveal a visible enabled row using canonical core focus reveal.
    /// Does not select. Expand ancestors first for a collapsed node.
    pub fn focus(self, world: &mut World, id: ItemId) -> Result<(), UiError> {
        validate(world, self.root)?;
        let mut v = world.get::<View>(self.root).unwrap().clone();
        if !available(&v).contains(&id) {
            return Err(UiError::InvalidProps);
        }
        focus_target(world, self.root, &mut v, Some(id));
        world.insert(self.root, v);
        Ok(())
    }
    /// Reveal a visible row without moving focus or selection. Layout must have
    /// run at least once. Uses the same core scroll viewport and logical rects.
    pub fn reveal(self, world: &mut World, id: ItemId) -> Result<(), UiError> {
        validate(world, self.root)?;
        let v = world.get::<View>(self.root).unwrap();
        let row = v
            .rows
            .get(&id)
            .filter(|r| r.visible)
            .ok_or(UiError::InvalidProps)?;
        let entity = row.slot.root;
        if !world.has::<Rect>(entity) {
            return Err(UiError::Stale);
        }
        reveal_entity(world, entity);
        Ok(())
    }
    /// Destroy only this owned subtree and cancel its queued/armed interactions.
    pub fn unmount(self, world: &mut World) -> Result<(), UiError> {
        validate(world, self.root)?;
        let mut entities = vec![self.root];
        descendants(world, self.root, &mut entities);
        forget_interaction_targets(world, &entities);
        despawn(world, self.root);
        Ok(())
    }
}
// Called per event inside core's chronological input dispatcher. Tab, Space
// and pointer retain the canonical generic policy. Enter activates separately.
fn navigate(world: &mut World, _entity: Entity, event: &KeyEvent) -> bool {
    let keys = vec![event.clone()];
    let mut retained = Vec::new();
    let mut consumed = false;
    for event in keys {
        let focus = world.resource::<Focus>().and_then(|f| f.entity);
        let owner = focus.and_then(|e| world.get::<RowOwner>(e).copied());
        let root = owner
            .map(|o| o.view)
            .or_else(|| focus.filter(|e| world.has::<View>(*e)));
        let Some(root) = root
            .filter(|e| validate(world, *e).is_ok() && access_details(world, focus.unwrap()).focus)
        else {
            retained.push(event);
            continue;
        };
        // Refresh the roving stop before canonical Tab dispatch, including
        // pointer-focus + Tab delivered in one platform batch.
        let mut current = world.get::<View>(root).unwrap().clone();
        sync(world, root, &mut current);
        world.insert(root, current);
        let LogicalKey::Named(key) = event.logical else {
            retained.push(event);
            continue;
        };
        // AT may focus the separate disclosure command, which is deliberately
        // not a Tab stop. Normalize to its owning row before canonical Tab
        // traversal rather than jumping to the beginning of the whole app.
        if key == NamedKey::Tab && !event.released && !event.synthetic {
            if let Some(owner) = owner.filter(|o| o.toggle) {
                let mut v = world.get::<View>(root).unwrap().clone();
                focus_target(world, root, &mut v, Some(owner.id));
                world.insert(root, v);
            }
        }
        if key == NamedKey::Enter && owner.is_some_and(|o| o.toggle) {
            retained.push(event);
            continue;
        }
        let handled = matches!(
            key,
            NamedKey::ArrowUp
                | NamedKey::ArrowDown
                | NamedKey::ArrowLeft
                | NamedKey::ArrowRight
                | NamedKey::Home
                | NamedKey::End
                | NamedKey::Enter
        );
        if !handled || event.modifiers.command() || event.modifiers.alt || event.modifiers.shift {
            retained.push(event);
            continue;
        }
        consumed = true;
        if event.released || event.synthetic {
            continue;
        }
        let mut v = world.get::<View>(root).unwrap().clone();
        let order = available(&v);
        let id = owner.map(|o| o.id);
        let index = id.and_then(|id| order.iter().position(|i| *i == id));
        let next = match key {
            NamedKey::ArrowDown => order
                .get(
                    index
                        .map(|i| (i + 1).min(order.len().saturating_sub(1)))
                        .unwrap_or(0),
                )
                .copied(),
            NamedKey::ArrowUp => order.get(index.unwrap_or(0).saturating_sub(1)).copied(),
            NamedKey::Home => order.first().copied(),
            NamedKey::End => order.last().copied(),
            _ => None,
        };
        if let Some(next) = next {
            focus_target(world, root, &mut v, Some(next));
            world.insert(root, v);
            continue;
        }
        if let Some(id) = id {
            if key == NamedKey::Enter && !event.repeat {
                emit(world, root, CollectionEventKind::Activated(id));
            } else if v.tree && key == NamedKey::ArrowRight {
                if v.rows[&id].branch && !v.expanded.contains(&id) {
                    let _ = (ViewHandle { root }).set_expanded(world, id, true);
                } else if let Some(child) =
                    order.iter().find(|i| v.rows[i].parent == Some(id)).copied()
                {
                    focus_target(world, root, &mut v, Some(child));
                    world.insert(root, v);
                }
            } else if v.tree && key == NamedKey::ArrowLeft {
                if v.expanded.contains(&id) {
                    let _ = (ViewHandle { root }).set_expanded(world, id, false);
                } else if let Some(parent) = v.rows[&id].parent.and_then(|p| safe_target(&v, p)) {
                    focus_target(world, root, &mut v, Some(parent));
                    world.insert(root, v);
                }
            }
        }
    }
    consumed
}
fn apply_view_gesture(world: &mut World, root: Entity, id: ItemId) -> Result<(), UiError> {
    let mut view = world.get::<View>(root).ok_or(UiError::Stale)?.clone();
    if !view.props.enabled {
        return Err(UiError::InvalidProps);
    }
    let order: Vec<ItemId> = view
        .order
        .iter()
        .copied()
        .filter(|id| view.rows[id].visible)
        .collect();
    let enabled_now: Vec<ItemId> = order
        .iter()
        .copied()
        .filter(|id| enabled(&view, *id))
        .collect();
    let mode = view.props.selection;
    let change = gesture(
        mode,
        &mut view.selected,
        &mut view.membership,
        id,
        gesture_shift(world),
        &order,
        |id| enabled_now.contains(&id),
    );
    match change {
        MembershipChange::Quiet => {}
        MembershipChange::Single(selected) => {
            emit(world, root, CollectionEventKind::Selected(selected));
        }
        MembershipChange::Set => emit(world, root, CollectionEventKind::SelectionSetChanged),
    }
    sync(world, root, &mut view);
    world.insert(root, view);
    Ok(())
}
// Runs only after canonical activation admission, before later navigation.
fn activate(world: &mut World, entity: Entity) {
    let Some(owner) = world.get::<RowOwner>(entity).copied() else {
        return;
    };
    let handle = ViewHandle { root: owner.view };
    if owner.toggle {
        let _ = handle.set_expanded(world, owner.id, !handle.expanded(world, owner.id));
        let _ = handle.focus(world, owner.id);
    } else {
        let _ = apply_view_gesture(world, owner.view, owner.id);
    }
}
pub(crate) fn requests(world: &mut World) {
    virtual_list::requests(world);
    note_reorder_drop(world);
    note_reorder_keys(world);
    for root in world.entities_with::<View>() {
        if validate(world, root).is_err() {
            continue;
        }
        let mut v = world.get::<View>(root).unwrap().clone();
        sync(world, root, &mut v);
        let focus = focused_id(world, root);
        if focus != v.reported_focus {
            emit(world, root, CollectionEventKind::Focused(focus));
            v.reported_focus = focus;
        }
        world.insert(root, v);
    }
}
fn bind_drag(world: &mut World, root: Entity, view: &View) {
    let kind = if view.tree {
        crate::drag::TREE_NODE
    } else {
        crate::drag::LIST_ROW
    };
    let geometry = if view.tree {
        crate::drag::DropGeometry::nest(Axis::Column)
    } else {
        crate::drag::DropGeometry::reorder(Axis::Column)
    };
    let gate = if view.tree { tree_gate } else { list_gate };
    for (id, row) in &view.rows {
        let live = view.props.reorder && row.visible && enabled(view, *id);
        if live {
            world.insert(
                row.slot.root,
                crate::drag::DragSource {
                    payload: crate::drag::DragPayload {
                        kind,
                        value: id.0,
                        owner: Some(root),
                    },
                    enabled: true,
                    name: row.item.name.clone(),
                    keyboard: crate::drag::DragKeyboard::Host,
                },
            );
            world.insert(
                row.slot.root,
                crate::drag::DropTarget {
                    enabled: true,
                    kinds: vec![kind],
                    owner: Some(root),
                    geometry,
                    gate: Some(gate),
                },
            );
        } else {
            world.remove::<crate::drag::DragSource>(row.slot.root);
            world.remove::<crate::drag::DropTarget>(row.slot.root);
        }
    }
}
fn list_gate(
    world: &World,
    payload: crate::drag::DragPayload,
    target: Entity,
    place: crate::drag::DropPlace,
) -> bool {
    let Some(owner) = world.get::<RowOwner>(target).copied() else {
        return false;
    };
    if payload.owner != Some(owner.view) {
        return false;
    }
    let Some(view) = world.get::<View>(owner.view) else {
        return false;
    };
    !view.tree && reorder_list(&items_of(view), ItemId(payload.value), owner.id, place).is_some()
}
fn tree_gate(
    world: &World,
    payload: crate::drag::DragPayload,
    target: Entity,
    place: crate::drag::DropPlace,
) -> bool {
    let Some(owner) = world.get::<RowOwner>(target).copied() else {
        return false;
    };
    if payload.owner != Some(owner.view) {
        return false;
    }
    let Some(view) = world.get::<View>(owner.view) else {
        return false;
    };
    view.tree && reorder_tree(&nodes_of(view), ItemId(payload.value), owner.id, place).is_some()
}
fn items_of(view: &View) -> Vec<Item> {
    view.order
        .iter()
        .map(|id| view.rows[id].item.clone())
        .collect()
}
fn nodes_of(view: &View) -> Vec<TreeNode> {
    view.order
        .iter()
        .map(|id| TreeNode {
            item: view.rows[id].item.clone(),
            parent: view.rows[id].parent,
        })
        .collect()
}
fn note_reorder_drop(world: &mut World) {
    let Some(drop) = crate::drag::committed(world) else {
        return;
    };
    if virtual_list::note_drop(world, drop) {
        return;
    }
    let Some(owner) = drop.payload.owner else {
        return;
    };
    let Some(row) = world.get::<RowOwner>(drop.target).copied() else {
        return;
    };
    if row.view != owner || !world.has::<View>(owner) {
        return;
    }
    let view = world.get::<View>(owner).unwrap().clone();
    if !view.props.reorder {
        return;
    }
    publish_move(
        world,
        owner,
        &view,
        ItemId(drop.payload.value),
        row.id,
        drop.place,
    );
}
fn note_reorder_keys(world: &mut World) {
    let keys = world
        .resource::<Input>()
        .map(|input| input.keys.clone())
        .unwrap_or_default();
    let mut used = Vec::new();
    for (index, event) in keys.iter().enumerate() {
        if !reorder_chord(event) {
            continue;
        }
        let LogicalKey::Named(key) = event.logical else {
            continue;
        };
        if apply_reorder_key(world, key) {
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
    !event.released
        && !event.repeat
        && !event.synthetic
        && event.modifiers.alt
        && !event.modifiers.command()
        && !event.modifiers.shift
        && matches!(
            event.logical,
            LogicalKey::Named(
                NamedKey::ArrowUp
                    | NamedKey::ArrowDown
                    | NamedKey::ArrowLeft
                    | NamedKey::ArrowRight
            )
        )
}
fn apply_reorder_key(world: &mut World, key: NamedKey) -> bool {
    let Some(focus) = world.resource::<Focus>().and_then(|focus| focus.entity) else {
        return false;
    };
    if world.has::<CapturesKeys>(focus) {
        return false;
    }
    if let Some(owner) = world.get::<RowOwner>(focus).copied() {
        return retained_key(world, owner.view, owner.id, key);
    }
    virtual_list::reorder_key(world, focus, key)
}
fn retained_key(world: &mut World, root: Entity, id: ItemId, key: NamedKey) -> bool {
    let Some(view) = world.get::<View>(root).cloned() else {
        return false;
    };
    if !view.props.reorder || !view.props.enabled {
        return false;
    }
    let order = available(&view);
    let name = view
        .rows
        .get(&id)
        .map(|row| row.item.name.clone())
        .unwrap_or_default();
    let index = order.iter().position(|item| *item == id);
    let movement = index.and_then(|index| match key {
        NamedKey::ArrowUp if index > 0 => Some((order[index - 1], crate::drag::DropPlace::Before)),
        NamedKey::ArrowDown => order
            .get(index + 1)
            .copied()
            .map(|target| (target, crate::drag::DropPlace::After)),
        NamedKey::ArrowRight if view.tree && index > 0 => {
            Some((order[index - 1], crate::drag::DropPlace::Inside))
        }
        NamedKey::ArrowLeft if view.tree => view
            .rows
            .get(&id)
            .and_then(|row| row.parent)
            .map(|parent| (parent, crate::drag::DropPlace::After)),
        _ => None,
    });
    match movement {
        Some((target, place)) => publish_move(world, root, &view, id, target, place),
        None => crate::drag::announce(world, &format!("Cannot move {name}.")),
    }
    true
}
fn publish_move(
    world: &mut World,
    root: Entity,
    view: &View,
    id: ItemId,
    target: ItemId,
    place: crate::drag::DropPlace,
) {
    let name = view
        .rows
        .get(&id)
        .map(|row| row.item.name.clone())
        .unwrap_or_default();
    let ok = if view.tree {
        reorder_tree(&nodes_of(view), id, target, place).is_some()
    } else {
        reorder_list(&items_of(view), id, target, place).is_some()
    };
    if !ok {
        crate::drag::announce(world, &format!("Cannot move {name}."));
        return;
    }
    let target_name = view
        .rows
        .get(&target)
        .map(|row| row.item.name.clone())
        .unwrap_or_default();
    emit(
        world,
        root,
        CollectionEventKind::Reordered(ItemMove { id, target, place }),
    );
    crate::drag::announce(
        world,
        &format!(
            "Moved {name} {} {target_name}.",
            crate::drag::place_word(place)
        ),
    );
}
/// Move `id` to `place` relative to `target`. `None` when the order would not
/// change, the ids are missing, or `place` is not before or after.
pub fn reorder_list(
    items: &[Item],
    id: ItemId,
    target: ItemId,
    place: crate::drag::DropPlace,
) -> Option<Vec<Item>> {
    if id == target
        || !matches!(
            place,
            crate::drag::DropPlace::Before | crate::drag::DropPlace::After
        )
    {
        return None;
    }
    let mut seen = BTreeSet::new();
    if items.iter().any(|item| !seen.insert(item.id)) {
        return None;
    }
    let from = items.iter().position(|item| item.id == id)?;
    let mut next: Vec<Item> = items.iter().filter(|item| item.id != id).cloned().collect();
    let mut index = next.iter().position(|item| item.id == target)?;
    if place == crate::drag::DropPlace::After {
        index += 1;
    }
    next.insert(index, items[from].clone());
    let same = next.iter().zip(items).all(|(next, old)| next.id == old.id);
    if same {
        None
    } else {
        Some(next)
    }
}
/// Move the subtree at `id` to `place` relative to `target`.
///
/// Before and after keep the target's parent. Inside appends the subtree as
/// the target's last children. `None` when that would cycle, exceed depth 32,
/// leave the forest unchanged, or the nodes are not a unique preorder.
pub fn reorder_tree(
    nodes: &[TreeNode],
    id: ItemId,
    target: ItemId,
    place: crate::drag::DropPlace,
) -> Option<Vec<TreeNode>> {
    if id == target || matches!(place, crate::drag::DropPlace::Edge(_)) {
        return None;
    }
    let map = parent_map(nodes)?;
    if under(&map, target, id) {
        return None;
    }
    let start = nodes.iter().position(|node| node.item.id == id)?;
    let end = subtree_end(nodes, &map, start);
    if nodes
        .iter()
        .skip(end)
        .any(|node| under(&map, node.item.id, id))
    {
        return None;
    }
    let mut moved = nodes[start..end].to_vec();
    let mut rest: Vec<TreeNode> = nodes[..start]
        .iter()
        .chain(nodes[end..].iter())
        .cloned()
        .collect();
    let target_index = rest.iter().position(|node| node.item.id == target)?;
    let rest_map = parent_map(&rest)?;
    let insert_at = match place {
        crate::drag::DropPlace::Before => target_index,
        crate::drag::DropPlace::After | crate::drag::DropPlace::Inside => {
            subtree_end(&rest, &rest_map, target_index)
        }
        crate::drag::DropPlace::Edge(_) => return None,
    };
    moved[0].parent = match place {
        crate::drag::DropPlace::Inside => Some(target),
        crate::drag::DropPlace::Before | crate::drag::DropPlace::After => rest[target_index].parent,
        crate::drag::DropPlace::Edge(_) => return None,
    };
    for (offset, node) in moved.into_iter().enumerate() {
        rest.insert(insert_at + offset, node);
    }
    let built = parent_map(&rest)?;
    for node in &rest {
        depth_of(&built, node.item.id)?;
    }
    let same = rest.len() == nodes.len()
        && rest
            .iter()
            .zip(nodes)
            .all(|(next, old)| next.item.id == old.item.id && next.parent == old.parent);
    if same {
        None
    } else {
        Some(rest)
    }
}
fn parent_map(nodes: &[TreeNode]) -> Option<BTreeMap<ItemId, Option<ItemId>>> {
    let mut map = BTreeMap::new();
    for node in nodes {
        if map.insert(node.item.id, node.parent).is_some() {
            return None;
        }
    }
    for parent in map.values().flatten() {
        if !map.contains_key(parent) {
            return None;
        }
    }
    Some(map)
}
fn under(map: &BTreeMap<ItemId, Option<ItemId>>, id: ItemId, ancestor: ItemId) -> bool {
    if id == ancestor {
        return true;
    }
    let mut current = id;
    for _ in 0..32 {
        match map.get(&current).copied().flatten() {
            Some(parent) if parent == ancestor => return true,
            Some(parent) => current = parent,
            None => return false,
        }
    }
    false
}
fn subtree_end(nodes: &[TreeNode], map: &BTreeMap<ItemId, Option<ItemId>>, start: usize) -> usize {
    let id = nodes[start].item.id;
    let mut end = start + 1;
    while end < nodes.len() && nodes[end].item.id != id && under(map, nodes[end].item.id, id) {
        end += 1;
    }
    end
}
fn depth_of(map: &BTreeMap<ItemId, Option<ItemId>>, id: ItemId) -> Option<usize> {
    let mut depth = 0usize;
    let mut current = id;
    for _ in 0..33 {
        match map.get(&current).copied() {
            Some(Some(parent)) => {
                depth += 1;
                if depth >= 32 {
                    return None;
                }
                current = parent;
            }
            Some(None) => return Some(depth),
            None => return None,
        }
    }
    None
}
pub(crate) fn styles(world: &mut World) {
    let theme = world.resource::<UiTheme>().cloned().unwrap_or_default();
    for viewport in world.entities_with::<Scroll>() {
        let mut color = theme.color(Token::Secondary);
        color.a = 0.55;
        world.insert(viewport, ScrollIndicator { color });
    }
    let focus = world.resource::<Focus>().and_then(|f| f.entity);
    let hovered = world
        .resource::<Input>()
        .and_then(|i| hit_test(world, i.pointer.position[0], i.pointer.position[1]));
    for root in world.entities_with::<View>() {
        if validate(world, root).is_err() {
            continue;
        }
        let v = world.get::<View>(root).unwrap().clone();
        world.insert(
            root,
            UiRecipe(UiStyle {
                background: Some(Token::Content.into()),
                corner: Some(Corner::Group),
                ..UiStyle::default()
            }),
        );
        if focus == Some(root) && v.props.enabled {
            world.insert(
                root,
                Border {
                    color: theme.color(Token::Focus),
                    width: 2.,
                },
            );
        } else {
            world.remove::<Border>(root);
        }
        world.insert(
            v.empty,
            UiRecipe(UiStyle {
                background: Some(Token::Content.into()),
                foreground: Some(
                    if v.props.enabled {
                        Token::Secondary
                    } else {
                        Token::Disabled
                    }
                    .into(),
                ),
                typography: Some(Typography::Secondary),
                padding: Some(crate::Space::X8),
                ..UiStyle::default()
            }),
        );
        for (id, row) in &v.rows {
            let enabled = enabled(&v, *id);
            let selected = row_chosen(&v, *id);
            let focused = focus == Some(row.slot.root) || focus == Some(row.chevron);
            let hover = hovered.is_some_and(|e| {
                e == row.slot.root || ancestor_with::<RowOwner>(world, e) == Some(row.slot.root)
            });
            let fill = if selected {
                Token::Selected
            } else if interaction_pressed(world, row.slot.root) {
                Token::ControlPressed
            } else if hover && enabled {
                Token::ControlHover
            } else {
                Token::Content
            };
            world.insert(
                row.inner,
                UiRecipe(UiStyle {
                    background: Some(crate::UiColor::Explicit(Color::rgba(0., 0., 0., 0.))),
                    corner: Some(Corner::Compact),
                    ..UiStyle::default()
                }),
            );
            // One row surface owns fill, including its disclosure/indent. Focus
            // is a bounded outline, never a filled ancestor viewport or gutter.
            if focused && enabled {
                world.insert(
                    row.slot.root,
                    Border {
                        color: theme.color(Token::Focus),
                        width: 2.,
                    },
                );
            } else {
                world.remove::<Border>(row.slot.root);
            }
            world.insert(
                row.slot.root,
                UiRecipe(UiStyle {
                    background: Some(fill.into()),
                    corner: Some(Corner::Control),
                    ..UiStyle::default()
                }),
            );
            world.insert(
                row.slot.content,
                UiRecipe(UiStyle {
                    background: Some(crate::UiColor::Explicit(Color::rgba(0., 0., 0., 0.))),
                    corner: Some(Corner::Compact),
                    ..UiStyle::default()
                }),
            );
            world.get_mut::<Layout>(row.slot.content).unwrap().padding =
                if world.has::<crate::data::TableRow>(row.slot.root) {
                    crate::data::TABLE_INSET - 3.0
                } else if theme.density == crate::Density::Compact {
                    4.0
                } else {
                    7.0
                };
            let mut entities = vec![row.slot.content];
            descendants(world, row.slot.content, &mut entities);
            for e in entities {
                if world.has::<Label>(e) {
                    let mut recipe = world.get::<UiRecipe>(e).cloned().unwrap_or_default();
                    recipe.0.foreground = Some(
                        if !enabled {
                            Token::Disabled
                        } else if recipe.0.typography == Some(Typography::Secondary) {
                            Token::Secondary
                        } else {
                            Token::Text
                        }
                        .into(),
                    );
                    world.insert(e, recipe);
                }
            }
            world.insert(
                row.chevron,
                UiRecipe(UiStyle {
                    foreground: Some(
                        if enabled {
                            Token::Text
                        } else {
                            Token::Disabled
                        }
                        .into(),
                    ),
                    typography: Some(Typography::Label),
                    ..UiStyle::default()
                }),
            );
        }
    }
}
