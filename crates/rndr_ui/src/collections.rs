//! Retained, application-owned data browsers. See docs/data-components.md.
//! No Shell, filesystem, global Selection, or pooled index identity is used.
mod virtual_list;
use crate::{text, Corner, Token, Typography, UiError, UiId, UiRecipe, UiStyle, UiTheme};
use rndr_core::*;
use std::collections::{BTreeMap, BTreeSet};
pub use virtual_list::{VirtualCompose, VirtualListPolicy, VirtualViewHandle};

/// Deliberate retained-data safety budget, not a virtualization claim.
pub const MAX_ITEMS: usize = 1024;
/// Stable application identity, scoped to one collection. Never use the row index.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ItemId(pub u64);
/// Initial selection contracts; multiple/range selection are not implemented.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectionMode {
    /// Focus and activation only.
    None,
    /// Zero or one selected ID; focus does not follow selection.
    Single,
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
}
impl ViewProps {
    /// Enabled single-select view with an empty state.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            empty: "No items to display".into(),
            enabled: true,
            selection: SelectionMode::Single,
        }
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
    /// Selection changed or was reconciled away.
    Selected(Option<ItemId>),
    /// Enter requested host activation, independently of selection. A virtual
    /// list may explicitly opt into palette double-click activation too.
    /// This is not the screen-reader Activate action: on a row that follows
    /// canonical click/Space selection (or no selection in None mode).
    Activated(ItemId),
    /// Branch expansion changed.
    Expanded(ItemId, bool),
}
/// Composed slots. Retained ListView/TreeView support passive content with a
/// stable subtree lifetime. `mount_virtual` supports nested controls but recycles
/// slots: keep application state by ItemId and never persist these entities.
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
        world.insert(row.slot.root, SelectedState(view.selected == Some(*id)));
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
}
impl ViewHandle {
    /// Validate generation and the owned interaction/content slot topology.
    /// Wrapping compositions use this before their own atomic reconciliation.
    pub fn validate(self, world: &World) -> Result<(), UiError> {
        validate(world, self.root)
    }

    /// Current selected stable ID. Collapsing a branch does not clear selection.
    pub fn selected(self, world: &World) -> Option<ItemId> {
        world.get::<View>(self.root).and_then(|v| v.selected)
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
    pub fn select(self, world: &mut World, id: Option<ItemId>) -> Result<(), UiError> {
        validate(world, self.root)?;
        let mut v = world.get::<View>(self.root).unwrap().clone();
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
        let _ = handle.select(world, Some(owner.id));
    }
}
pub(crate) fn requests(world: &mut World) {
    virtual_list::requests(world);
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
            let selected = v.selected == Some(*id);
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
