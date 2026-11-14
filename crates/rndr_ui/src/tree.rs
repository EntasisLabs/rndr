//! Virtualized tree. The host owns the nodes. The tree owns focus, selection,
//! expansion gestures, and which rows are realized.
//!
//! Only the visible window is given layout and paint. A branch with no
//! children yet shows a loading row and asks the host to load them. Dragging
//! reuses [`crate::drag`]: before, after, and inside. The host applies
//! [`reorder_nodes`] and mounts again. See `docs/virtual-tree.md`.
//!
//! ```
//! use rndr_core::{Layout, Sim, Size, Viewport};
//! use rndr_ui::tree::{NodeId, TreeNode, TreeProps, VirtualTree};
//! use rndr_ui::{UiId, UiPlugin};
//!
//! let mut sim = Sim::new();
//! sim.app.add_plugin(UiPlugin);
//! sim.world().insert_resource(Viewport::new(320.0, 240.0));
//! let parent = sim.world().spawn((Layout {
//!     width: Size::Px(320.0),
//!     height: Size::Px(240.0),
//!     ..Layout::column()
//! },));
//! let nodes = [
//!     TreeNode::new(1, "src").expandable(true),
//!     TreeNode::new(2, "main.rs").child(NodeId(1)),
//! ];
//! let tree = VirtualTree::mount(
//!     sim.world(),
//!     parent,
//!     UiId(1),
//!     TreeProps::new("Files"),
//!     &nodes,
//!     &[NodeId(1)],
//! )
//! .unwrap();
//! sim.update();
//! sim.update();
//! assert!(tree.expanded(sim.world(), NodeId(1)));
//! assert_eq!(tree.visible_count(sim.world()), 2);
//! assert!(tree.row(sim.world(), NodeId(2)).is_some());
//! ```

use crate::drag::{
    self, committed, DragKeyboard, DragPayload, DragSource, DropGeometry, DropPlace, DropTarget,
};
use crate::lifecycle::{find_keyed_child, require_layout_parent};
use crate::{Token, Typography, UiError, UiId, UiRecipe, UiStyle, UiTheme};
use rndr_core::*;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;
use std::time::Duration;

const ROW_HEIGHT: f32 = 32.0;
const INDENT: f32 = 16.0;
const TYPEAHEAD: Duration = Duration::from_millis(500);
/// Depth is zero-based. 32 levels is the same cap as [`crate::collections::reorder_tree`].
const MAX_DEPTH: u8 = 32;
/// Marks a visible loading placeholder. The low bits are the parent index.
const LOADING: u32 = 1 << 31;

/// Host identity of one node. Unique inside one tree.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(pub u64);

/// One node the host knows about.
///
/// Children are the other nodes whose [`Self::parent`] is this id, in the
/// order they appear in the slice. A node may be [`Self::expandable`] before
/// any of those children exist.
#[derive(Clone, Debug, PartialEq)]
pub struct TreeNode {
    /// Stable id. Mount rejects a repeated id.
    pub id: NodeId,
    /// Parent id, or `None` for a root. The parent has to be in the same slice.
    pub parent: Option<NodeId>,
    /// Visible label. Must not be blank.
    pub label: String,
    /// Quieter text after the label. Blank hides it.
    pub secondary: String,
    /// Short trailing text, such as a count. Blank hides it.
    pub badge: String,
    /// Leading glyph. The accessible name stays the label.
    pub icon: Option<Icon>,
    /// Disabled nodes stay visible. They cannot focus, select, or activate.
    /// A disabled ancestor disables its descendants.
    pub enabled: bool,
    /// Disclosure is shown. Expanding with no children asks the host to load them.
    pub expandable: bool,
    /// The host is fetching children. An expanded node shows a loading row.
    pub loading: bool,
}

impl TreeNode {
    /// Enabled leaf named `label`, with no parent, icon, secondary text, or badge.
    pub fn new(id: u64, label: impl Into<String>) -> Self {
        Self {
            id: NodeId(id),
            parent: None,
            label: label.into(),
            secondary: String::new(),
            badge: String::new(),
            icon: None,
            enabled: true,
            expandable: false,
            loading: false,
        }
    }

    /// Set [`Self::parent`].
    pub fn child(mut self, parent: NodeId) -> Self {
        self.parent = Some(parent);
        self
    }

    /// Trailing secondary text.
    pub fn secondary(mut self, text: impl Into<String>) -> Self {
        self.secondary = text.into();
        self
    }

    /// Trailing badge.
    pub fn badge(mut self, text: impl Into<String>) -> Self {
        self.badge = text.into();
        self
    }

    /// Leading glyph.
    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    /// Whether the node can be focused and selected.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Show a disclosure. Children may arrive later.
    pub fn expandable(mut self, expandable: bool) -> Self {
        self.expandable = expandable;
        self
    }

    /// The host is fetching children.
    pub fn loading(mut self, loading: bool) -> Self {
        self.loading = loading;
        self
    }
}

/// How many nodes a click can select.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TreeSelection {
    /// Focus and activation. Clicks do not select.
    None,
    /// One id. A click replaces it.
    Single,
    /// A click replaces the set. Shift selects the visible range. Ctrl or
    /// Command toggles one id. Shift with Ctrl or Command adds that range.
    Multiple,
}

/// Label, selection, and whether rows can be dragged.
#[derive(Clone, Debug, PartialEq)]
pub struct TreeProps {
    /// Accessible name of the tree. Must not be blank. It is not drawn.
    pub name: String,
    /// Shown when the host passes no nodes.
    pub empty: String,
    /// A disabled tree does not focus, select, drag, or toggle.
    pub enabled: bool,
    /// [`TreeSelection::Single`] by default.
    pub selection: TreeSelection,
    /// When true, an enabled row can be dragged onto another row in this tree.
    /// The tree emits [`TreeEventKind::Reordered`] and does not move the node.
    pub reorder: bool,
    /// Uniform row height in logical points. Finite and at least 1.
    /// Virtualization uses this stride for every row, including a loading row.
    pub row_height: f32,
}

impl TreeProps {
    /// Enabled single-select tree named `name`. Rows are 32 points tall.
    /// Reorder is off. The empty text is "No items to display".
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            empty: "No items to display".into(),
            enabled: true,
            selection: TreeSelection::Single,
            reorder: false,
            row_height: ROW_HEIGHT,
        }
    }

    /// [`TreeSelection::Multiple`].
    pub fn multiple(mut self) -> Self {
        self.selection = TreeSelection::Multiple;
        self
    }

    /// [`TreeSelection::None`].
    pub fn navigation_only(mut self) -> Self {
        self.selection = TreeSelection::None;
        self
    }

    /// Enable [`Self::reorder`].
    pub fn with_reorder(mut self) -> Self {
        self.reorder = true;
        self
    }

    /// Uniform stride. Values below 1 or that are not finite are rejected at mount.
    pub fn row_height(mut self, height: f32) -> Self {
        self.row_height = height;
        self
    }

    /// Disable focus, selection, drag, and disclosure.
    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }
}

/// Where a context menu was requested.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TreeMenuRequest {
    /// Row under the pointer, or the focused row for a key.
    pub id: NodeId,
    /// Menu anchor in logical points.
    pub at: [f32; 2],
}

/// A requested move. [`TreeMove::place`] is before, after, or inside.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TreeMove {
    /// Node that moved.
    pub id: NodeId,
    /// Node it was dropped relative to.
    pub target: NodeId,
    /// Before, after, or inside [`Self::target`].
    pub place: DropPlace,
}

/// What the tree is asking the host to do, or what focus did.
///
/// Input sends are readable on Update of that frame, except
/// [`TreeEventKind::ContextMenu`]. That one is readable on the next frame,
/// because the secondary press is published at the Update boundary.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TreeEvent {
    /// Tree root.
    pub tree: Entity,
    /// What happened.
    pub kind: TreeEventKind,
}
impl Event for TreeEvent {}

/// Focus, selection, disclosure, load, rename, menu, and move.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TreeEventKind {
    /// Focus entered or left a row. `None` is leaving the tree.
    Focused(Option<NodeId>),
    /// [`TreeSelection::Single`] changed. `None` clears it.
    /// [`TreeSelection::Multiple`] uses [`Self::SelectionSetChanged`].
    Selected(Option<NodeId>),
    /// The multiple set changed. The event does not carry the ids.
    /// Read them from [`TreeHandle::selection`].
    SelectionSetChanged,
    /// Enter on an enabled row. This does not select.
    Activated(NodeId),
    /// A branch opened or closed. The next mount's expanded list is authoritative.
    /// The tree also applies the change locally so the row updates before that mount.
    Expanded(NodeId, bool),
    /// An expanded branch has no children yet and the host is not already loading.
    /// Mount again with the children, or with [`TreeNode::loading`] while they are in flight.
    LoadChildren(NodeId),
    /// The host should apply [`reorder_nodes`] and mount again.
    Reordered(TreeMove),
    /// F2 on the focused row. The tree does not edit the label.
    RenameRequested(NodeId),
    /// Right-click, Menu, or Shift+F10. Readable on the frame after the gesture.
    ContextMenu(TreeMenuRequest),
}

/// One mounted tree. [`Self::root`] is the scroll viewport.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TreeHandle {
    /// Scroll viewport. Set its width and height. Leave its children alone.
    pub root: Entity,
}

/// Virtualized tree. Mount again when the host's nodes or expansion change.
pub struct VirtualTree;

impl VirtualTree {
    /// Mount or update a tree under `parent`.
    ///
    /// `nodes` is the whole model the host has loaded, not the visible window.
    /// `expanded` is the open branches. An id that is not in `nodes` is rejected.
    /// A later mount with the same id reuses the viewport and any row still on screen.
    /// Invalid input is rejected before the mounted tree changes.
    pub fn mount(
        world: &mut World,
        parent: Entity,
        id: UiId,
        props: TreeProps,
        nodes: &[TreeNode],
        expanded: &[NodeId],
    ) -> Result<TreeHandle, UiError> {
        require_layout_parent(world, parent)?;
        admit_props(&props)?;
        let forest = Arc::new(build_forest(nodes)?);
        let mut bits = bits_with(forest.nodes.len());
        for id in expanded {
            let Some(index) = forest.by_id.get(id).copied() else {
                return Err(UiError::InvalidProps);
            };
            set_bit(&mut bits, index, true);
        }
        let bits: Arc<[u64]> = bits.into();
        if let Some(root) = find_keyed_child(world, parent, id) {
            if world.get::<TreeState>(root).is_none() {
                return Err(UiError::IdentityConflict);
            }
            validate(world, root)?;
            remount(world, root, props, forest, bits)?;
            Ok(TreeHandle { root })
        } else {
            let root = spawn_root(world, parent, id);
            let visible = Arc::new(flatten(&forest, &bits));
            let active = first_enabled(&forest, &visible);
            let state = TreeState {
                key: id,
                props,
                forest,
                expanded: bits,
                visible,
                selected: None,
                chosen: BTreeSet::new(),
                anchor: None,
                active,
                reported_focus: None,
                slots: BTreeMap::new(),
                spare: Vec::new(),
                top: spacer(world, root),
                bottom: spacer(world, root),
                empty: empty_label(world, root),
                typed: String::new(),
                typed_at: Duration::ZERO,
                ensure: None,
                booted: false,
            };
            world.insert(root, state);
            finish_mount(world, root, true);
            Ok(TreeHandle { root })
        }
    }
}

impl TreeHandle {
    /// Nodes in the last mounted model.
    pub fn node_count(self, world: &World) -> usize {
        world
            .get::<TreeState>(self.root)
            .map(|state| state.forest.nodes.len())
            .unwrap_or(0)
    }

    /// Rows in the expanded window, including a loading placeholder.
    pub fn visible_count(self, world: &World) -> usize {
        world
            .get::<TreeState>(self.root)
            .map(|state| state.visible.rows.len())
            .unwrap_or(0)
    }

    /// Row entities that currently have a visible index. Offscreen nodes are not here.
    pub fn realized_count(self, world: &World) -> usize {
        world
            .get::<TreeState>(self.root)
            .map(|state| state.slots.len())
            .unwrap_or(0)
    }

    /// Realized row for `id`, or `None` when it is offscreen, collapsed, or absent.
    pub fn row(self, world: &World, id: NodeId) -> Option<Entity> {
        let state = world.get::<TreeState>(self.root)?;
        let index = state.forest.by_id.get(&id).copied()?;
        state.slots.get(&index).map(|slot| slot.root)
    }

    /// Whether `id` is open. Unknown ids are closed.
    pub fn expanded(self, world: &World, id: NodeId) -> bool {
        world.get::<TreeState>(self.root).is_some_and(|state| {
            state
                .forest
                .by_id
                .get(&id)
                .is_some_and(|index| bit(&state.expanded, *index))
        })
    }

    /// Focused row while focus is inside this tree, including when it is parked
    /// on the viewport after a scroll. `None` when focus is elsewhere.
    pub fn focused(self, world: &World) -> Option<NodeId> {
        let state = world.get::<TreeState>(self.root)?;
        focus_inside(world, self.root)
            .then_some(state.active)
            .flatten()
    }

    /// The one selected id in [`TreeSelection::Single`]. Empty for other modes.
    pub fn selected(self, world: &World) -> Option<NodeId> {
        let state = world.get::<TreeState>(self.root)?;
        if state.props.selection == TreeSelection::Single {
            state.selected
        } else {
            None
        }
    }

    /// Selected ids. [`TreeSelection::Single`] has at most one.
    pub fn selection(self, world: &World) -> BTreeSet<NodeId> {
        let Some(state) = world.get::<TreeState>(self.root) else {
            return BTreeSet::new();
        };
        match state.props.selection {
            TreeSelection::None => BTreeSet::new(),
            TreeSelection::Single => state.selected.into_iter().collect(),
            TreeSelection::Multiple => state.chosen.clone(),
        }
    }

    /// Replace [`TreeSelection::Single`]. `None` clears every mode.
    /// [`TreeSelection::Multiple`] rejects `Some`: use [`Self::set_selection`].
    pub fn select(self, world: &mut World, id: Option<NodeId>) -> Result<(), UiError> {
        validate(world, self.root)?;
        let props = world.get::<TreeState>(self.root).unwrap().props.clone();
        if !props.enabled {
            return Err(UiError::InvalidProps);
        }
        match props.selection {
            TreeSelection::Multiple if id.is_some() => Err(UiError::InvalidProps),
            TreeSelection::None if id.is_some() => Err(UiError::InvalidProps),
            _ => {
                let ids: Vec<NodeId> = id.into_iter().collect();
                self.set_selection(world, &ids)
            }
        }
    }

    /// Replace the selection with `ids`. Each id has to exist and be enabled.
    /// [`TreeSelection::Single`] takes at most one. [`TreeSelection::None`] takes none.
    /// The first id is scrolled into view.
    pub fn set_selection(self, world: &mut World, ids: &[NodeId]) -> Result<(), UiError> {
        validate(world, self.root)?;
        let mut state = world.get::<TreeState>(self.root).unwrap().clone();
        if !state.props.enabled || !ids_allowed(&state, ids) {
            return Err(UiError::InvalidProps);
        }
        let reveal = ids.first().copied();
        if apply_selection(&mut state, ids) {
            emit_selection(world, self.root, &state);
        }
        if let Some(id) = reveal {
            state.active = Some(id);
        }
        world.insert(self.root, state);
        if let Some(id) = reveal {
            self.reveal(world, id)?;
        }
        Ok(())
    }

    /// Open or close a branch. A node that cannot have children is rejected.
    /// Disabled branches still open: a host can reveal through them.
    pub fn set_expanded(self, world: &mut World, id: NodeId, open: bool) -> Result<(), UiError> {
        validate(world, self.root)?;
        let index = {
            let state = world.get::<TreeState>(self.root).unwrap();
            let index = state
                .forest
                .by_id
                .get(&id)
                .copied()
                .ok_or(UiError::InvalidProps)?;
            if !is_branch(&state.forest, index) {
                return Err(UiError::InvalidProps);
            }
            index
        };
        apply_expansion(world, self.root, &[(index, open)]);
        Ok(())
    }

    /// Expand ancestors, scroll `id` into the viewport, and leave focus and
    /// selection where they are. The id has to be in the mounted model.
    pub fn reveal(self, world: &mut World, id: NodeId) -> Result<(), UiError> {
        validate(world, self.root)?;
        reveal_id(world, self.root, id, false)
    }

    /// [`Self::reveal`] and move focus to `id`. Does not select.
    /// A disabled node is rejected.
    pub fn focus(self, world: &mut World, id: NodeId) -> Result<(), UiError> {
        validate(world, self.root)?;
        reveal_id(world, self.root, id, true)
    }

    /// Despawn the viewport and its rows.
    pub fn unmount(self, world: &mut World) -> Result<(), UiError> {
        validate(world, self.root)?;
        let mut entities = vec![self.root];
        descendants(world, self.root, &mut entities);
        forget_interaction_targets(world, &entities);
        despawn(world, self.root);
        Ok(())
    }
}

/// Move `id` to `place` relative to `target` and return the forest in preorder.
///
/// Before and after keep the target's parent. Inside appends `id` and its
/// subtree as the target's last children. `None` when that would cycle, exceed
/// depth 32, leave the forest unchanged, or the slice is not a tree.
pub fn reorder_nodes(
    nodes: &[TreeNode],
    id: NodeId,
    target: NodeId,
    place: DropPlace,
) -> Option<Vec<TreeNode>> {
    if id == target || matches!(place, DropPlace::Edge(_)) {
        return None;
    }
    let mut index_of = HashMap::with_capacity(nodes.len());
    for (i, node) in nodes.iter().enumerate() {
        if node.label.trim().is_empty() || index_of.insert(node.id, i as u32).is_some() {
            return None;
        }
    }
    let id_i = *index_of.get(&id)?;
    let target_i = *index_of.get(&target)?;
    let n = nodes.len();
    let mut parent = vec![None; n];
    let mut kids: Vec<Vec<u32>> = vec![Vec::new(); n];
    let mut roots = Vec::new();
    for (i, node) in nodes.iter().enumerate() {
        match node.parent {
            None => roots.push(i as u32),
            Some(pid) => {
                let p = *index_of.get(&pid)?;
                if p == i as u32 {
                    return None;
                }
                parent[i] = Some(p);
                kids[p as usize].push(i as u32);
            }
        }
    }
    let (depth, below) = color_walk(&kids, &roots)?;
    if !accept_move(
        id_i,
        target_i,
        place,
        &Shape {
            parent: &parent,
            depth: &depth,
            below: &below,
            kids: &kids,
            roots: &roots,
        },
    ) {
        return None;
    }
    let old_parent = parent[id_i as usize];
    let new_parent = match place {
        DropPlace::Inside => Some(target_i),
        DropPlace::Before | DropPlace::After => parent[target_i as usize],
        DropPlace::Edge(_) => return None,
    };
    detach(&mut kids, &mut roots, old_parent, id_i);
    parent[id_i as usize] = new_parent;
    attach(&mut kids, &mut roots, new_parent, target_i, id_i, place);
    let (depth, _) = color_walk(&kids, &roots)?;
    if depth.iter().any(|d| *d >= MAX_DEPTH) {
        return None;
    }
    let mut out = Vec::with_capacity(n);
    let mut stack: Vec<u32> = roots.iter().rev().copied().collect();
    while let Some(current) = stack.pop() {
        let mut node = nodes[current as usize].clone();
        node.parent = parent[current as usize].map(|p| nodes[p as usize].id);
        out.push(node);
        for child in kids[current as usize].iter().rev() {
            stack.push(*child);
        }
    }
    if out.len() != n {
        return None;
    }
    let same = out
        .iter()
        .zip(nodes)
        .all(|pair| pair.0.id == pair.1.id && pair.0.parent == pair.1.parent);
    if same {
        None
    } else {
        Some(out)
    }
}

pub(crate) fn key_at(world: &World, entity: Entity) -> Option<UiId> {
    world.get::<TreeState>(entity).map(|state| state.key)
}

pub(crate) fn drive(world: &mut World) {
    note_drop(world);
    note_reorder_keys(world);
    let roots = world.entities_with::<TreeState>();
    for root in roots {
        let Some(state) = world.get::<TreeState>(root).cloned() else {
            continue;
        };
        let focus = focus_inside(world, root).then_some(state.active).flatten();
        if focus != state.reported_focus {
            emit(world, root, TreeEventKind::Focused(focus));
            if let Some(mut state) = world.get::<TreeState>(root).cloned() {
                state.reported_focus = focus;
                world.insert(root, state);
            }
        }
    }
}

pub(crate) fn publish(world: &mut World) {
    let presses: Vec<SecondaryPress> = events::<SecondaryPress>(world).to_vec();
    for press in presses {
        let Some(row) = row_of(world, press.entity) else {
            continue;
        };
        if row.loading || row.toggle {
            continue;
        }
        select_for_menu(world, row.tree, row.node);
        emit(
            world,
            row.tree,
            TreeEventKind::ContextMenu(TreeMenuRequest {
                id: row.node,
                at: press.at,
            }),
        );
    }
}

pub(crate) fn styles(world: &mut World) {
    let theme = world.resource::<UiTheme>().cloned().unwrap_or_default();
    let focus = world.resource::<Focus>().and_then(|focus| focus.entity);
    for root in world.entities_with::<TreeState>() {
        let Some(state) = world.get::<TreeState>(root).cloned() else {
            continue;
        };
        world.insert(
            root,
            UiRecipe(UiStyle {
                background: Some(Token::Content.into()),
                corner: Some(crate::Corner::Group),
                ..UiStyle::default()
            }),
        );
        if focus == Some(root) && state.props.enabled {
            world.insert(
                root,
                Border {
                    color: theme.color(Token::Focus),
                    width: 2.0,
                },
            );
        } else {
            world.remove::<Border>(root);
        }
        let _ = state;
    }
}

#[derive(Clone)]
struct Rec {
    id: NodeId,
    parent: Option<u32>,
    label: String,
    secondary: String,
    badge: String,
    icon: Option<Icon>,
    expandable: bool,
    loading: bool,
    live: bool,
    depth: u8,
    below: u8,
    sibling_index: u32,
    set_size: u32,
    child_start: u32,
    child_count: u32,
}

#[derive(Clone)]
struct Forest {
    nodes: Vec<Rec>,
    children: Vec<u32>,
    roots: Vec<u32>,
    by_id: HashMap<NodeId, u32>,
}

#[derive(Clone, Copy)]
struct Vis {
    key: u32,
    depth: u8,
    guides: u32,
    last: bool,
}

#[derive(Clone)]
struct Visible {
    rows: Vec<Vis>,
    /// Node index to visible index. `u32::MAX` when an ancestor is collapsed.
    place: Vec<u32>,
}

#[derive(Clone, Copy)]
struct Guide {
    column: Entity,
    bar: Entity,
    tick: Entity,
}

#[derive(Clone)]
struct Slot {
    root: Entity,
    guides: Entity,
    columns: Vec<Guide>,
    chevron: Entity,
    icon: Entity,
    label: Entity,
    secondary: Entity,
    badge: Entity,
}

#[derive(Clone)]
struct TreeState {
    key: UiId,
    props: TreeProps,
    forest: Arc<Forest>,
    expanded: Arc<[u64]>,
    visible: Arc<Visible>,
    selected: Option<NodeId>,
    chosen: BTreeSet<NodeId>,
    anchor: Option<NodeId>,
    active: Option<NodeId>,
    reported_focus: Option<NodeId>,
    slots: BTreeMap<u32, Slot>,
    spare: Vec<Slot>,
    top: Entity,
    bottom: Entity,
    empty: Entity,
    typed: String,
    typed_at: Duration,
    ensure: Option<NodeId>,
    booted: bool,
}
impl Component for TreeState {}

#[derive(Clone, Copy)]
struct TreeRow {
    tree: Entity,
    node: NodeId,
    toggle: bool,
    loading: bool,
}
impl Component for TreeRow {}

fn admit_props(props: &TreeProps) -> Result<(), UiError> {
    if props.name.trim().is_empty()
        || props.empty.trim().is_empty()
        || !props.row_height.is_finite()
        || props.row_height < 1.0
    {
        Err(UiError::InvalidProps)
    } else {
        Ok(())
    }
}

fn build_forest(nodes: &[TreeNode]) -> Result<Forest, UiError> {
    if nodes.len() >= LOADING as usize {
        return Err(UiError::InvalidProps);
    }
    let mut by_id = HashMap::with_capacity(nodes.len());
    for (i, node) in nodes.iter().enumerate() {
        if node.label.trim().is_empty() || by_id.insert(node.id, i as u32).is_some() {
            return Err(UiError::InvalidProps);
        }
    }
    let n = nodes.len();
    let mut parent = vec![None; n];
    let mut kids: Vec<Vec<u32>> = vec![Vec::new(); n];
    let mut roots = Vec::new();
    let mut sibling_index = vec![0u32; n];
    for (i, node) in nodes.iter().enumerate() {
        match node.parent {
            None => {
                sibling_index[i] = roots.len() as u32;
                roots.push(i as u32);
            }
            Some(pid) => {
                let Some(p) = by_id.get(&pid).copied() else {
                    return Err(UiError::InvalidProps);
                };
                if p == i as u32 {
                    return Err(UiError::InvalidProps);
                }
                parent[i] = Some(p);
                sibling_index[i] = kids[p as usize].len() as u32;
                kids[p as usize].push(i as u32);
            }
        }
    }
    let (depth, below) = color_walk(&kids, &roots).ok_or(UiError::InvalidProps)?;
    let mut children = Vec::new();
    let mut recs = Vec::with_capacity(n);
    for (i, node) in nodes.iter().enumerate() {
        let start = children.len() as u32;
        children.extend_from_slice(&kids[i]);
        let set_size = match parent[i] {
            Some(p) => kids[p as usize].len() as u32,
            None => roots.len() as u32,
        };
        recs.push(Rec {
            id: node.id,
            parent: parent[i],
            label: node.label.clone(),
            secondary: node.secondary.clone(),
            badge: node.badge.clone(),
            icon: node.icon,
            expandable: node.expandable,
            loading: node.loading,
            live: node.enabled,
            depth: depth[i],
            below: below[i],
            sibling_index: sibling_index[i],
            set_size,
            child_start: start,
            child_count: kids[i].len() as u32,
        });
    }
    let mut forest = Forest {
        nodes: recs,
        children,
        roots,
        by_id,
    };
    mark_live(&mut forest);
    let total = forest.nodes.len() as f32 * ROW_HEIGHT;
    if !total.is_finite() {
        return Err(UiError::InvalidProps);
    }
    Ok(forest)
}

fn color_walk(kids: &[Vec<u32>], roots: &[u32]) -> Option<(Vec<u8>, Vec<u8>)> {
    let n = kids.len();
    let mut depth = vec![0u8; n];
    let mut color = vec![0u8; n];
    let mut below = vec![0u8; n];
    fn walk(
        kids: &[Vec<u32>],
        depth: &mut [u8],
        color: &mut [u8],
        below: &mut [u8],
        index: u32,
        d: u8,
    ) -> Option<()> {
        let i = index as usize;
        if color[i] == 1 || d >= MAX_DEPTH {
            return None;
        }
        if color[i] == 2 {
            return Some(());
        }
        color[i] = 1;
        depth[i] = d;
        let mut extra = 0u8;
        for child in &kids[i] {
            walk(kids, depth, color, below, *child, d + 1)?;
            extra = extra.max(below[*child as usize].saturating_add(1));
        }
        below[i] = extra;
        color[i] = 2;
        Some(())
    }
    for root in roots {
        walk(kids, &mut depth, &mut color, &mut below, *root, 0)?;
    }
    if color.iter().any(|c| *c != 2) {
        None
    } else {
        Some((depth, below))
    }
}

fn mark_live(forest: &mut Forest) {
    let mut stack: Vec<(u32, bool)> = forest
        .roots
        .iter()
        .copied()
        .map(|root| (root, false))
        .collect();
    while let Some((index, parent_off)) = stack.pop() {
        let authored = forest.nodes[index as usize].live;
        let off = parent_off || !authored;
        forest.nodes[index as usize].live = !off;
        let start = forest.nodes[index as usize].child_start;
        let count = forest.nodes[index as usize].child_count;
        for n in 0..count {
            let child = forest.children[(start + n) as usize];
            stack.push((child, off));
        }
    }
}

fn flatten(forest: &Forest, expanded: &[u64]) -> Visible {
    let mut rows = Vec::new();
    let mut place = vec![u32::MAX; forest.nodes.len()];
    for (i, root) in forest.roots.iter().enumerate() {
        let last = i + 1 == forest.roots.len();
        walk(
            forest,
            expanded,
            *root,
            0,
            0,
            last,
            &mut WalkOut {
                rows: &mut rows,
                place: &mut place,
            },
        );
    }
    Visible { rows, place }
}

struct WalkOut<'a> {
    rows: &'a mut Vec<Vis>,
    place: &'a mut [u32],
}

fn walk(
    forest: &Forest,
    expanded: &[u64],
    index: u32,
    depth: u8,
    guides: u32,
    last: bool,
    out: &mut WalkOut<'_>,
) {
    out.place[index as usize] = out.rows.len() as u32;
    out.rows.push(Vis {
        key: index,
        depth,
        guides,
        last,
    });
    if !bit(expanded, index) {
        return;
    }
    let node = &forest.nodes[index as usize];
    let child_guides = if depth > 0 && !last {
        guides | (1 << (depth - 1))
    } else {
        guides
    };
    let count = node.child_count;
    let show_loading = node.loading || (count == 0 && node.expandable);
    if count == 0 {
        if show_loading && depth + 1 < MAX_DEPTH {
            out.rows.push(Vis {
                key: LOADING | index,
                depth: depth + 1,
                guides: child_guides,
                last: true,
            });
        }
        return;
    }
    for n in 0..count {
        let child = forest.children[(node.child_start + n) as usize];
        let child_last = n + 1 == count && !node.loading;
        walk(
            forest,
            expanded,
            child,
            depth + 1,
            child_guides,
            child_last,
            out,
        );
    }
    if node.loading && depth + 1 < MAX_DEPTH {
        out.rows.push(Vis {
            key: LOADING | index,
            depth: depth + 1,
            guides: child_guides,
            last: true,
        });
    }
}

fn is_branch(forest: &Forest, index: u32) -> bool {
    let node = &forest.nodes[index as usize];
    node.expandable || node.loading || node.child_count > 0
}

struct Shape<'a> {
    parent: &'a [Option<u32>],
    depth: &'a [u8],
    below: &'a [u8],
    kids: &'a [Vec<u32>],
    roots: &'a [u32],
}

fn accept_move(id: u32, target: u32, place: DropPlace, shape: &Shape<'_>) -> bool {
    if id == target || matches!(place, DropPlace::Edge(_)) {
        return false;
    }
    if contains(shape.parent, id, target) {
        return false;
    }
    let new_depth = match place {
        DropPlace::Inside => shape.depth[target as usize].saturating_add(1),
        DropPlace::Before | DropPlace::After => shape.depth[target as usize],
        DropPlace::Edge(_) => return false,
    };
    if u16::from(new_depth) + u16::from(shape.below[id as usize]) >= u16::from(MAX_DEPTH) {
        return false;
    }
    let siblings = |p: Option<u32>| -> &[u32] {
        match p {
            None => shape.roots,
            Some(i) => shape.kids[i as usize].as_slice(),
        }
    };
    let parent = shape.parent;
    match place {
        DropPlace::Before => {
            next_in(siblings(parent[id as usize]), id) != Some(target)
                || parent[id as usize] != parent[target as usize]
        }
        DropPlace::After => {
            next_in(siblings(parent[target as usize]), target) != Some(id)
                || parent[id as usize] != parent[target as usize]
        }
        DropPlace::Inside => {
            !(parent[id as usize] == Some(target) && next_in(siblings(Some(target)), id).is_none())
        }
        DropPlace::Edge(_) => false,
    }
}

fn contains(parent: &[Option<u32>], ancestor: u32, mut id: u32) -> bool {
    for _ in 0..=MAX_DEPTH {
        if id == ancestor {
            return true;
        }
        match parent[id as usize] {
            Some(next) => id = next,
            None => return false,
        }
    }
    true
}

fn next_in(siblings: &[u32], id: u32) -> Option<u32> {
    let index = siblings.iter().position(|s| *s == id)?;
    siblings.get(index + 1).copied()
}

fn detach(kids: &mut [Vec<u32>], roots: &mut Vec<u32>, parent: Option<u32>, id: u32) {
    let list = match parent {
        None => roots,
        Some(p) => &mut kids[p as usize],
    };
    list.retain(|child| *child != id);
}

fn attach(
    kids: &mut [Vec<u32>],
    roots: &mut Vec<u32>,
    parent: Option<u32>,
    target: u32,
    id: u32,
    place: DropPlace,
) {
    let list = match parent {
        None => roots,
        Some(p) => &mut kids[p as usize],
    };
    let at = match place {
        DropPlace::Inside => list.len(),
        DropPlace::Before => list.iter().position(|c| *c == target).unwrap_or(list.len()),
        DropPlace::After => list
            .iter()
            .position(|c| *c == target)
            .map(|i| i + 1)
            .unwrap_or(list.len()),
        DropPlace::Edge(_) => list.len(),
    };
    list.insert(at, id);
}

fn bits_with(n: usize) -> Vec<u64> {
    vec![0; n.div_ceil(64)]
}

fn bit(bits: &[u64], index: u32) -> bool {
    let i = index as usize;
    bits.get(i / 64)
        .is_some_and(|word| word & (1 << (i % 64)) != 0)
}

fn set_bit(bits: &mut [u64], index: u32, on: bool) {
    let i = index as usize;
    if let Some(word) = bits.get_mut(i / 64) {
        let mask = 1u64 << (i % 64);
        if on {
            *word |= mask;
        } else {
            *word &= !mask;
        }
    }
}

fn first_enabled(forest: &Forest, visible: &Visible) -> Option<NodeId> {
    visible.rows.iter().find_map(|row| {
        if is_loading(row.key) {
            return None;
        }
        let node = &forest.nodes[row.key as usize];
        node.live.then_some(node.id)
    })
}

fn is_loading(key: u32) -> bool {
    key & LOADING != 0
}

fn spawn_root(world: &mut World, parent: Entity, id: UiId) -> Entity {
    let root = world.spawn((
        Layout {
            width: Size::Fill,
            height: Size::Fill,
            ..Layout::column()
        },
        Scroll::default(),
        Focusable,
        KeyNavigation(navigate),
        ViewportRows(realize),
        ContextTarget,
    ));
    set_parent(world, root, parent);
    let _ = id;
    root
}

fn spacer(world: &mut World, parent: Entity) -> Entity {
    let entity = world.spawn((Layout {
        width: Size::Fill,
        height: Size::Px(0.0),
        ..Layout::column()
    },));
    set_parent(world, entity, parent);
    entity
}

fn empty_label(world: &mut World, parent: Entity) -> Entity {
    let entity = world.spawn((
        Layout {
            width: Size::Fill,
            ..Layout::column()
        },
        Label::new("").overflow(TextOverflow::Wrap),
        UiRecipe(UiStyle {
            foreground: Some(Token::Secondary.into()),
            typography: Some(Typography::Secondary),
            padding: Some(crate::Space::X8),
            ..UiStyle::default()
        }),
    ));
    set_parent(world, entity, parent);
    entity
}

fn remount(
    world: &mut World,
    root: Entity,
    props: TreeProps,
    forest: Arc<Forest>,
    expanded: Arc<[u64]>,
) -> Result<(), UiError> {
    let mut state = world.get::<TreeState>(root).unwrap().clone();
    let old_ids = expanded_ids(&state.forest, &state.expanded);
    let new_ids = expanded_ids(&forest, &expanded);
    let fresh = !state.booted;
    state.props = props;
    state.forest = forest;
    state.expanded = expanded;
    state.visible = Arc::new(flatten(&state.forest, &state.expanded));
    reconcile_selection(world, root, &mut state);
    if let Some(active) = state.active {
        if !on_screen(&state, active) {
            state.active = fallback(&state, active);
            if focus_inside(world, root) {
                state.ensure = state.active;
            }
        }
    } else {
        state.active = first_enabled(&state.forest, &state.visible);
    }
    if !fresh {
        for id in new_ids.difference(&old_ids) {
            emit(world, root, TreeEventKind::Expanded(*id, true));
            if needs_load(&state, *id) {
                emit(world, root, TreeEventKind::LoadChildren(*id));
            }
        }
        for id in old_ids.difference(&new_ids) {
            emit(world, root, TreeEventKind::Expanded(*id, false));
        }
    } else {
        for id in &new_ids {
            if needs_load(&state, *id) {
                emit(world, root, TreeEventKind::LoadChildren(*id));
            }
        }
    }
    state.booted = true;
    let ensure = state.ensure;
    world.insert(root, state);
    finish_mount(world, root, false);
    if let Some(id) = ensure {
        let _ = show_focus(world, root, id, true);
    }
    Ok(())
}

fn finish_mount(world: &mut World, root: Entity, first: bool) {
    if first {
        if let Some(mut state) = world.get::<TreeState>(root).cloned() {
            let pending = expanded_ids(&state.forest, &state.expanded);
            for id in &pending {
                if needs_load(&state, *id) {
                    emit(world, root, TreeEventKind::LoadChildren(*id));
                }
            }
            state.booted = true;
            world.insert(root, state);
        }
    }
    layout_propagate(world);
}

fn expanded_ids(forest: &Forest, bits: &[u64]) -> BTreeSet<NodeId> {
    let mut ids = BTreeSet::new();
    for (i, node) in forest.nodes.iter().enumerate() {
        if bit(bits, i as u32) {
            ids.insert(node.id);
        }
    }
    ids
}

fn needs_load(state: &TreeState, id: NodeId) -> bool {
    let Some(index) = state.forest.by_id.get(&id).copied() else {
        return false;
    };
    let node = &state.forest.nodes[index as usize];
    bit(&state.expanded, index) && node.child_count == 0 && node.expandable && !node.loading
}

fn reconcile_selection(world: &mut World, root: Entity, state: &mut TreeState) {
    match state.props.selection {
        TreeSelection::None => {
            if state.selected.take().is_some() {
                emit(world, root, TreeEventKind::Selected(None));
            }
            if !state.chosen.is_empty() {
                state.chosen.clear();
                state.anchor = None;
                emit(world, root, TreeEventKind::SelectionSetChanged);
            }
        }
        TreeSelection::Single => {
            state.chosen.clear();
            state.anchor = None;
            if state.selected.is_some_and(|id| !selectable(state, id)) {
                state.selected = None;
                emit(world, root, TreeEventKind::Selected(None));
            }
        }
        TreeSelection::Multiple => {
            if state.selected.take().is_some() {
                emit(world, root, TreeEventKind::Selected(None));
            }
            let before = state.chosen.clone();
            let kept: BTreeSet<NodeId> = state
                .chosen
                .iter()
                .copied()
                .filter(|id| selectable(state, *id))
                .collect();
            state.chosen = kept;
            if state.anchor.is_some_and(|id| !selectable(state, id)) {
                state.anchor = None;
            }
            if state.chosen != before {
                emit(world, root, TreeEventKind::SelectionSetChanged);
            }
        }
    }
}

fn selectable(state: &TreeState, id: NodeId) -> bool {
    state.props.enabled
        && state
            .forest
            .by_id
            .get(&id)
            .is_some_and(|index| state.forest.nodes[*index as usize].live)
}

fn on_screen(state: &TreeState, id: NodeId) -> bool {
    selectable(state, id)
        && state
            .forest
            .by_id
            .get(&id)
            .is_some_and(|index| state.visible.place[*index as usize] != u32::MAX)
}

fn fallback(state: &TreeState, id: NodeId) -> Option<NodeId> {
    let mut current = state.forest.by_id.get(&id).copied();
    while let Some(index) = current {
        let node = &state.forest.nodes[index as usize];
        if node.live && state.visible.place[index as usize] != u32::MAX {
            return Some(node.id);
        }
        current = node.parent;
    }
    first_enabled(&state.forest, &state.visible)
}

fn ids_allowed(state: &TreeState, ids: &[NodeId]) -> bool {
    match state.props.selection {
        TreeSelection::None => ids.is_empty(),
        TreeSelection::Single => ids.len() <= 1 && ids.iter().all(|id| selectable(state, *id)),
        TreeSelection::Multiple => ids.iter().all(|id| selectable(state, *id)),
    }
}

fn apply_selection(state: &mut TreeState, ids: &[NodeId]) -> bool {
    match state.props.selection {
        TreeSelection::None => false,
        TreeSelection::Single => {
            let next = ids.first().copied();
            if state.selected == next {
                false
            } else {
                state.selected = next;
                true
            }
        }
        TreeSelection::Multiple => {
            let next: BTreeSet<NodeId> = ids.iter().copied().collect();
            if state.chosen == next {
                false
            } else {
                state.chosen = next;
                state.anchor = ids.first().copied();
                true
            }
        }
    }
}

fn emit_selection(world: &mut World, root: Entity, state: &TreeState) {
    match state.props.selection {
        TreeSelection::Multiple => emit(world, root, TreeEventKind::SelectionSetChanged),
        _ => emit(world, root, TreeEventKind::Selected(state.selected)),
    }
}

fn reveal_id(world: &mut World, root: Entity, id: NodeId, focus: bool) -> Result<(), UiError> {
    let mut opens = Vec::new();
    {
        let state = world.get::<TreeState>(root).unwrap();
        let Some(index) = state.forest.by_id.get(&id).copied() else {
            return Err(UiError::InvalidProps);
        };
        if focus && (!state.props.enabled || !state.forest.nodes[index as usize].live) {
            return Err(UiError::InvalidProps);
        }
        let mut current = state.forest.nodes[index as usize].parent;
        while let Some(parent) = current {
            if !bit(&state.expanded, parent) {
                opens.push((parent, true));
            }
            current = state.forest.nodes[parent as usize].parent;
        }
    }
    if !opens.is_empty() {
        apply_expansion(world, root, &opens);
    }
    show_focus(world, root, id, focus)
}

fn show_focus(world: &mut World, root: Entity, id: NodeId, focus: bool) -> Result<(), UiError> {
    {
        let Some(mut state) = world.get::<TreeState>(root).cloned() else {
            return Err(UiError::Stale);
        };
        let Some(index) = state.forest.by_id.get(&id).copied() else {
            return Err(UiError::InvalidProps);
        };
        if state.visible.place[index as usize] == u32::MAX {
            return Err(UiError::InvalidProps);
        }
        state.ensure = Some(id);
        if focus {
            state.active = Some(id);
        }
        world.insert(root, state);
    }
    layout_propagate(world);
    if focus {
        let row = world
            .get::<TreeState>(root)
            .and_then(|state| {
                state
                    .forest
                    .by_id
                    .get(&id)
                    .and_then(|index| state.slots.get(index).map(|slot| slot.root))
            })
            .unwrap_or(root);
        request_focus(world, row);
    }
    Ok(())
}

fn apply_expansion(world: &mut World, root: Entity, changes: &[(u32, bool)]) {
    let Some(mut state) = world.get::<TreeState>(root).cloned() else {
        return;
    };
    let mut events = Vec::new();
    let mut bits = state.expanded.to_vec();
    let mut changed = false;
    for (index, open) in changes {
        if bit(&bits, *index) == *open {
            continue;
        }
        set_bit(&mut bits, *index, *open);
        changed = true;
        let id = state.forest.nodes[*index as usize].id;
        events.push(TreeEventKind::Expanded(id, *open));
        if *open && needs_load_index(&state.forest, *index) {
            events.push(TreeEventKind::LoadChildren(id));
        }
    }
    if !changed {
        return;
    }
    state.expanded = bits.into();
    state.visible = Arc::new(flatten(&state.forest, &state.expanded));
    if let Some(active) = state.active {
        if !on_screen(&state, active) {
            state.active = fallback(&state, active);
            state.ensure = state.active;
        }
    }
    let ensure = state.ensure;
    world.insert(root, state);
    for kind in events {
        emit(world, root, kind);
    }
    if let Some(id) = ensure {
        let _ = show_focus(world, root, id, true);
    } else {
        layout_propagate(world);
    }
}

fn needs_load_index(forest: &Forest, index: u32) -> bool {
    let node = &forest.nodes[index as usize];
    node.child_count == 0 && node.expandable && !node.loading
}

fn realize(world: &mut World, root: Entity) -> bool {
    if validate(world, root).is_err() {
        return false;
    }
    let Some(mut state) = world.get::<TreeState>(root).cloned() else {
        return false;
    };
    let changed = project(world, root, &mut state);
    world.insert(root, state);
    changed
}

fn project(world: &mut World, root: Entity, state: &mut TreeState) -> bool {
    let height = world.get::<Rect>(root).map(|rect| rect.h).unwrap_or(0.0);
    let row_h = state.props.row_height;
    let count = state.visible.rows.len() as u32;
    if height > 0.0 {
        if let Some(id) = state.ensure.take() {
            if let Some(index) = state.forest.by_id.get(&id).copied() {
                let place = state.visible.place[index as usize];
                if place != u32::MAX {
                    let offset = read_offset(world, root);
                    let next = into_view(offset, height, place, row_h, count);
                    write_offset(world, root, next);
                }
            }
        }
    }
    let offset = if height > 0.0 {
        let raw = read_offset(world, root);
        let clamped = clamp_offset(raw, height, row_h, count);
        if (clamped - raw).abs() > f32::EPSILON {
            write_offset(world, root, clamped);
        }
        clamped
    } else {
        read_offset(world, root)
    };
    let (first, len) = if height > 0.0 {
        list_window(offset, height, row_h, count)
    } else {
        (0, 0)
    };
    let wanted: Vec<Vis> = state
        .visible
        .rows
        .iter()
        .skip(first as usize)
        .take(len as usize)
        .copied()
        .collect();
    let top = first as f32 * row_h;
    let bottom = (count - first - len) as f32 * row_h;
    let mut changed = resize(world, state.top, top) | resize(world, state.bottom, bottom);
    changed |= rebind(world, root, state, &wanted);
    let keys: Vec<Entity> = wanted
        .iter()
        .filter_map(|vis| state.slots.get(&vis.key).map(|slot| slot.root))
        .collect();
    world.insert(
        root,
        Children {
            list: std::iter::once(state.top)
                .chain(keys)
                .chain([state.bottom, state.empty])
                .collect(),
        },
    );
    if count == 0 {
        world.remove::<Hidden>(state.empty);
        if let Some(label) = world.get_mut::<Label>(state.empty) {
            label.text = state.props.empty.clone();
        }
    } else {
        world.insert(state.empty, Hidden);
    }
    let active_bound = state.active.is_some_and(|id| {
        state
            .forest
            .by_id
            .get(&id)
            .is_some_and(|index| state.slots.contains_key(index))
    });
    world.insert(root, TabStop(!active_bound));
    world.insert(
        root,
        Semantics {
            name: Some(state.props.name.clone()),
            description: "Arrows move. Left and Right collapse and expand. Home and End jump. Type to search. Asterisk expands siblings. Space selects. Enter activates.".into(),
            readonly: false,
        },
    );
    world.insert(
        root,
        CollectionAccess {
            role: AccessRole::Tree,
            expanded: None,
            level: None,
            parent: None,
            position: None,
            set_size: None,
            multiselect: state.props.selection == TreeSelection::Multiple,
        },
    );
    if state.props.enabled {
        world.remove::<ControlDisabled>(root);
    } else {
        world.insert(root, ControlDisabled);
    }
    changed
}

fn read_offset(world: &World, root: Entity) -> f32 {
    world
        .get::<Scroll>(root)
        .map(|scroll| -scroll.offset[1])
        .unwrap_or(0.0)
        .max(0.0)
}

fn write_offset(world: &mut World, root: Entity, offset: f32) {
    if let Some(scroll) = world.get_mut::<Scroll>(root) {
        scroll.offset[1] = -offset.max(0.0);
    }
}

fn into_view(offset: f32, height: f32, index: u32, row: f32, count: u32) -> f32 {
    let y = index as f32 * row;
    let next = if y < offset {
        y
    } else if y + row > offset + height {
        (y + row - height).max(0.0)
    } else {
        offset
    };
    clamp_offset(next, height, row, count)
}

fn clamp_offset(offset: f32, height: f32, row: f32, count: u32) -> f32 {
    let max = (count as f32 * row - height).max(0.0);
    if offset.is_finite() {
        offset.clamp(0.0, max)
    } else {
        0.0
    }
}

fn resize(world: &mut World, entity: Entity, height: f32) -> bool {
    let Some(layout) = world.get_mut::<Layout>(entity) else {
        return false;
    };
    let next = Size::Px(height);
    let changed = layout.height != next;
    layout.height = next;
    changed
}

fn rebind(world: &mut World, root: Entity, state: &mut TreeState, wanted: &[Vis]) -> bool {
    let wanted_keys: BTreeSet<u32> = wanted.iter().map(|vis| vis.key).collect();
    let old: Vec<u32> = state.slots.keys().copied().collect();
    let mut changed = false;
    let mut spare = std::mem::take(&mut state.spare);
    for key in old {
        if !wanted_keys.contains(&key) {
            let Some(slot) = state.slots.remove(&key) else {
                continue;
            };
            park(world, root, slot.root);
            retire(world, slot.root);
            spare.push(slot);
            changed = true;
        }
    }
    for vis in wanted {
        if let std::collections::btree_map::Entry::Vacant(entry) = state.slots.entry(vis.key) {
            let slot = spare.pop().unwrap_or_else(|| create_slot(world, root));
            entry.insert(slot);
            changed = true;
        }
        bind_slot(world, root, state, *vis);
    }
    for slot in spare.drain(..) {
        retire(world, slot.root);
        despawn(world, slot.root);
        changed = true;
    }
    state.spare = spare;
    changed
}

fn park(world: &mut World, root: Entity, slot: Entity) {
    let focus = world.resource::<Focus>().and_then(|focus| focus.entity);
    let inside = focus.is_some_and(|focus| focus == slot || ancestor_slot(world, focus, slot));
    if inside {
        request_focus(world, root);
    }
}

fn ancestor_slot(world: &World, mut entity: Entity, slot: Entity) -> bool {
    for _ in 0..32 {
        let Some(parent) = world.get::<Parent>(entity).map(|parent| parent.entity) else {
            return false;
        };
        if parent == slot {
            return true;
        }
        entity = parent;
    }
    false
}

fn retire(world: &mut World, entity: Entity) {
    let mut entities = Vec::new();
    descendants(world, entity, &mut entities);
    entities.push(entity);
    forget_interaction_targets(world, &entities);
}

fn descendants(world: &World, entity: Entity, out: &mut Vec<Entity>) {
    for child in children_of(world, entity) {
        out.push(child);
        descendants(world, child, out);
    }
}

fn create_slot(world: &mut World, parent: Entity) -> Slot {
    let root = world.spawn((
        Layout {
            width: Size::Fill,
            height: Size::Px(ROW_HEIGHT),
            gap: 4.0,
            ..Layout::row()
        },
        Align {
            justify: Justify::Start,
            items: Items::Center,
        },
        Padding {
            left: 4.0,
            right: 8.0,
            ..Padding::default()
        },
        Focusable,
        ReleaseActivation,
        ContextTarget,
    ));
    world.insert(root, KeyNavigation(navigate));
    world.insert(root, ActivationHandler(activate));
    set_parent(world, root, parent);
    let guides = world.spawn((Layout {
        height: Size::Fill,
        gap: 0.0,
        ..Layout::row()
    },));
    set_parent(world, guides, root);
    let chevron = world.spawn((
        Layout {
            width: Size::Px(INDENT),
            height: Size::Px(INDENT),
            ..Layout::column()
        },
        Align::center(),
        Label::new(""),
        TabStop(false),
    ));
    set_parent(world, chevron, root);
    let icon = world.spawn((
        Layout {
            width: Size::Px(INDENT),
            height: Size::Px(INDENT),
            ..Layout::column()
        },
        Align::center(),
        Label::new(""),
    ));
    set_parent(world, icon, root);
    let label = world.spawn((
        Layout {
            width: Size::Fill,
            min_width: 0.0,
            ..Layout::column()
        },
        Label::new("").overflow(TextOverflow::Ellipsis),
    ));
    set_parent(world, label, root);
    let secondary = world.spawn((
        Layout {
            min_width: 0.0,
            ..Layout::column()
        },
        MaxSize::width(180.0),
        Label::new("").overflow(TextOverflow::Ellipsis),
    ));
    set_parent(world, secondary, root);
    let badge = world.spawn((
        Layout {
            min_width: 0.0,
            ..Layout::column()
        },
        MaxSize::width(72.0),
        Label::new("").overflow(TextOverflow::Ellipsis),
    ));
    set_parent(world, badge, root);
    Slot {
        root,
        guides,
        columns: Vec::new(),
        chevron,
        icon,
        label,
        secondary,
        badge,
    }
}

fn bind_slot(world: &mut World, tree: Entity, state: &mut TreeState, vis: Vis) {
    let Some(mut slot) = state.slots.get(&vis.key).cloned() else {
        return;
    };
    let row_h = state.props.row_height;
    if let Some(layout) = world.get_mut::<Layout>(slot.root) {
        layout.height = Size::Px(row_h);
    }
    let loading = is_loading(vis.key);
    let (node_index, label, secondary, badge, icon, branch, open, live, pos, set_size, parent) =
        if loading {
            let parent = vis.key & !LOADING;
            let node = &state.forest.nodes[parent as usize];
            let kids = node.child_count;
            (
                parent,
                "Loading…".to_string(),
                String::new(),
                String::new(),
                Some(Icon::Loader),
                false,
                false,
                false,
                kids + 1,
                kids + 1,
                Some(parent),
            )
        } else {
            let node = &state.forest.nodes[vis.key as usize];
            (
                vis.key,
                node.label.clone(),
                node.secondary.clone(),
                node.badge.clone(),
                node.icon,
                is_branch(&state.forest, vis.key),
                bit(&state.expanded, vis.key),
                node.live && state.props.enabled,
                node.sibling_index + 1,
                node.set_size,
                node.parent,
            )
        };
    let node_id = state.forest.nodes[node_index as usize].id;
    world.insert(
        slot.root,
        TreeRow {
            tree,
            node: node_id,
            toggle: false,
            loading,
        },
    );
    world.insert(
        slot.root,
        TabStop(state.active == Some(node_id) && !loading),
    );
    let selected = !loading && is_selected(state, node_id);
    world.insert(slot.root, SelectedState(selected));
    if loading {
        world.remove::<Button>(slot.root);
        world.remove::<Focusable>(slot.root);
        world.remove::<KeyNavigation>(slot.root);
        world.remove::<ActivationHandler>(slot.root);
        world.remove::<DragSource>(slot.root);
        world.remove::<DropTarget>(slot.root);
        world.insert(slot.root, ControlDisabled);
    } else {
        world.insert(slot.root, Focusable);
        world.insert(slot.root, KeyNavigation(navigate));
        world.insert(slot.root, ActivationHandler(activate));
        world.insert(
            slot.root,
            Button {
                action: "rndr-ui:select".into(),
                enabled: live,
            },
        );
        if live {
            world.remove::<ControlDisabled>(slot.root);
        } else {
            world.insert(slot.root, ControlDisabled);
        }
        bind_drag(world, tree, state, slot.root, node_id, &label, live);
    }
    paint_row(world, state, &slot, selected, live, loading);
    bind_guides(world, slot.guides, &mut slot.columns, vis, row_h);
    bind_chevron(
        world,
        tree,
        &slot,
        Chevron {
            id: node_id,
            label: &label,
            branch,
            open,
            live,
            loading,
        },
    );
    bind_icon(world, &slot, icon);
    set_label(world, slot.label, &label, !live);
    if secondary.trim().is_empty() || loading {
        world.insert(slot.secondary, Hidden);
    } else {
        world.remove::<Hidden>(slot.secondary);
        set_label(world, slot.secondary, secondary.trim(), true);
    }
    if badge.trim().is_empty() || loading {
        world.insert(slot.badge, Hidden);
    } else {
        world.remove::<Hidden>(slot.badge);
        set_label(world, slot.badge, badge.trim(), !live);
    }
    let name = accessible_name(&label, &secondary, &badge, loading);
    world.insert(
        slot.root,
        Semantics {
            name: Some(name),
            ..Semantics::default()
        },
    );
    let parent_entity = parent.and_then(|index| state.slots.get(&index).map(|slot| slot.root));
    world.insert(
        slot.root,
        CollectionAccess {
            role: AccessRole::TreeItem,
            expanded: branch.then_some(open),
            level: Some(usize::from(vis.depth) + 1),
            parent: parent_entity,
            position: Some(pos as usize),
            set_size: Some(set_size as usize),
            multiselect: false,
        },
    );
    let columns = slot.columns;
    if let Some(stored) = state.slots.get_mut(&vis.key) {
        stored.columns = columns;
    }
}

fn is_selected(state: &TreeState, id: NodeId) -> bool {
    match state.props.selection {
        TreeSelection::None => false,
        TreeSelection::Single => state.selected == Some(id),
        TreeSelection::Multiple => state.chosen.contains(&id),
    }
}

fn accessible_name(label: &str, secondary: &str, badge: &str, loading: bool) -> String {
    if loading {
        return "Loading".into();
    }
    let mut name = label.to_string();
    if !secondary.trim().is_empty() {
        name.push_str(", ");
        name.push_str(secondary.trim());
    }
    if !badge.trim().is_empty() {
        name.push_str(", ");
        name.push_str(badge.trim());
    }
    name
}

fn paint_row(
    world: &mut World,
    state: &TreeState,
    slot: &Slot,
    selected: bool,
    live: bool,
    loading: bool,
) {
    let theme = world.resource::<UiTheme>().cloned().unwrap_or_default();
    let focus = world.resource::<Focus>().and_then(|focus| focus.entity);
    let focused =
        focus.is_some_and(|focus| focus == slot.root || ancestor_slot(world, focus, slot.root));
    let hover = world
        .resource::<Input>()
        .and_then(|input| hit_test(world, input.pointer.position[0], input.pointer.position[1]));
    let hovered = hover.is_some_and(|hit| hit == slot.root || ancestor_slot(world, hit, slot.root));
    let pressed = interaction_pressed(world, slot.root);
    let fill = if selected {
        Token::Selected
    } else if pressed {
        Token::ControlPressed
    } else if hovered && live && !loading {
        Token::ControlHover
    } else {
        Token::Content
    };
    let color = theme.color(fill);
    world.insert(slot.root, color);
    world.insert(
        slot.root,
        UiRecipe(UiStyle {
            background: Some(fill.into()),
            corner: Some(crate::Corner::Control),
            ..UiStyle::default()
        }),
    );
    if focused && live && !loading && state.props.enabled {
        world.insert(
            slot.root,
            Border {
                color: theme.color(Token::Focus),
                width: 2.0,
            },
        );
    } else {
        world.remove::<Border>(slot.root);
    }
}

fn set_label(world: &mut World, entity: Entity, text: &str, secondary: bool) {
    let theme = world.resource::<UiTheme>().cloned().unwrap_or_default();
    let token = if secondary {
        Token::Secondary
    } else {
        Token::Text
    };
    if let Some(label) = world.get_mut::<Label>(entity) {
        label.text = text.to_string();
        label.color = Some(theme.color(token));
        label.overflow = TextOverflow::Ellipsis;
    }
}

fn bind_icon(world: &mut World, slot: &Slot, icon: Option<Icon>) {
    if let Some(icon) = icon {
        world.remove::<Hidden>(slot.icon);
        if let Some(label) = world.get_mut::<Label>(slot.icon) {
            label.text = icon.scalar().to_string();
        }
    } else {
        world.insert(slot.icon, Hidden);
    }
}

struct Chevron<'a> {
    id: NodeId,
    label: &'a str,
    branch: bool,
    open: bool,
    live: bool,
    loading: bool,
}

fn bind_chevron(world: &mut World, tree: Entity, slot: &Slot, chevron: Chevron<'_>) {
    let Chevron {
        id,
        label,
        branch,
        open,
        live,
        loading,
    } = chevron;
    world.remove::<Hidden>(slot.chevron);
    if branch && !loading {
        let icon = if open {
            Icon::ChevronDown
        } else {
            Icon::ChevronRight
        };
        if let Some(text) = world.get_mut::<Label>(slot.chevron) {
            text.text = icon.scalar().to_string();
        }
        world.insert(
            slot.chevron,
            Button {
                action: "rndr-ui:expand".into(),
                enabled: live,
            },
        );
        world.insert(slot.chevron, ReleaseActivation);
        world.insert(slot.chevron, ActivationHandler(activate));
        world.insert(
            slot.chevron,
            TreeRow {
                tree,
                node: id,
                toggle: true,
                loading: false,
            },
        );
        world.insert(
            slot.chevron,
            Semantics {
                name: Some(format!(
                    "{} {label}",
                    if open { "Collapse" } else { "Expand" }
                )),
                ..Semantics::default()
            },
        );
    } else {
        if let Some(text) = world.get_mut::<Label>(slot.chevron) {
            text.text.clear();
        }
        world.remove::<Button>(slot.chevron);
        world.remove::<ReleaseActivation>(slot.chevron);
        world.remove::<ActivationHandler>(slot.chevron);
        world.remove::<TreeRow>(slot.chevron);
        world.remove::<Semantics>(slot.chevron);
    }
}

fn bind_guides(world: &mut World, guides: Entity, columns: &mut Vec<Guide>, vis: Vis, row_h: f32) {
    let theme = world.resource::<UiTheme>().cloned().unwrap_or_default();
    let ink = theme.color(Token::SubtleStroke);
    let depth = usize::from(vis.depth);
    while columns.len() < depth {
        columns.push(spawn_guide(world, guides));
    }
    for (i, guide) in columns.iter().enumerate() {
        world.insert(
            guide.column,
            Layout {
                width: Size::Px(INDENT),
                height: Size::Px(row_h),
                ..Layout::column()
            },
        );
        if i >= depth {
            world.insert(guide.column, Hidden);
            continue;
        }
        world.remove::<Hidden>(guide.column);
        let elbow = i + 1 == depth;
        let continues = !elbow && (vis.guides & (1 << i)) != 0;
        let show_bar = elbow || continues;
        let full = !elbow || !vis.last;
        if show_bar {
            world.remove::<Hidden>(guide.bar);
            world.insert(guide.bar, ink);
            world.insert(
                guide.bar,
                Layout {
                    width: Size::Px(1.0),
                    height: Size::Px(if full { row_h } else { row_h * 0.5 }),
                    ..Layout::column()
                },
            );
            world.insert(
                guide.bar,
                Absolute {
                    left: Some(7.0),
                    top: Some(0.0),
                    ..Absolute::default()
                },
            );
        } else {
            world.insert(guide.bar, Hidden);
        }
        if elbow {
            world.remove::<Hidden>(guide.tick);
            world.insert(guide.tick, ink);
            world.insert(
                guide.tick,
                Layout {
                    width: Size::Px(8.0),
                    height: Size::Px(1.0),
                    ..Layout::column()
                },
            );
            world.insert(
                guide.tick,
                Absolute {
                    left: Some(8.0),
                    top: Some((row_h - 1.0) * 0.5),
                    ..Absolute::default()
                },
            );
        } else {
            world.insert(guide.tick, Hidden);
        }
    }
}

fn spawn_guide(world: &mut World, parent: Entity) -> Guide {
    let column = world.spawn((Layout {
        width: Size::Px(INDENT),
        height: Size::Px(ROW_HEIGHT),
        ..Layout::column()
    },));
    set_parent(world, column, parent);
    let bar = world.spawn((
        Layout {
            width: Size::Px(1.0),
            height: Size::Px(ROW_HEIGHT),
            ..Layout::column()
        },
        Absolute {
            left: Some(7.0),
            top: Some(0.0),
            ..Absolute::default()
        },
    ));
    set_parent(world, bar, column);
    let tick = world.spawn((
        Layout {
            width: Size::Px(8.0),
            height: Size::Px(1.0),
            ..Layout::column()
        },
        Absolute {
            left: Some(8.0),
            top: Some(15.0),
            ..Absolute::default()
        },
        Hidden,
    ));
    set_parent(world, tick, column);
    Guide { column, bar, tick }
}

fn bind_drag(
    world: &mut World,
    tree: Entity,
    state: &TreeState,
    row: Entity,
    id: NodeId,
    name: &str,
    live: bool,
) {
    if !(state.props.reorder && live) {
        world.remove::<DragSource>(row);
        world.remove::<DropTarget>(row);
        return;
    }
    world.insert(
        row,
        DragSource {
            payload: DragPayload {
                kind: drag::TREE_NODE,
                value: id.0,
                owner: Some(tree),
            },
            enabled: true,
            name: name.to_string(),
            keyboard: DragKeyboard::Host,
        },
    );
    world.insert(
        row,
        DropTarget {
            enabled: true,
            kinds: vec![drag::TREE_NODE],
            owner: Some(tree),
            geometry: DropGeometry::nest(Axis::Column),
            gate: Some(gate),
        },
    );
}

fn gate(world: &World, payload: DragPayload, target: Entity, place: DropPlace) -> bool {
    let Some(row) = world.get::<TreeRow>(target).copied() else {
        return false;
    };
    if row.loading
        || row.toggle
        || payload.owner != Some(row.tree)
        || payload.kind != drag::TREE_NODE
    {
        return false;
    }
    let Some(state) = world.get::<TreeState>(row.tree) else {
        return false;
    };
    let Some(id) = state.forest.by_id.get(&NodeId(payload.value)).copied() else {
        return false;
    };
    let Some(target) = state.forest.by_id.get(&row.node).copied() else {
        return false;
    };
    forest_accept(&state.forest, id, target, place)
}

fn forest_accept(forest: &Forest, id: u32, target: u32, place: DropPlace) -> bool {
    if id == target || matches!(place, DropPlace::Edge(_)) {
        return false;
    }
    if under(forest, target, id) {
        return false;
    }
    let new_depth = match place {
        DropPlace::Inside => forest.nodes[target as usize].depth.saturating_add(1),
        DropPlace::Before | DropPlace::After => forest.nodes[target as usize].depth,
        DropPlace::Edge(_) => return false,
    };
    if u16::from(new_depth) + u16::from(forest.nodes[id as usize].below) >= u16::from(MAX_DEPTH) {
        return false;
    }
    let same_parent = forest.nodes[id as usize].parent == forest.nodes[target as usize].parent;
    match place {
        DropPlace::Before => next_sibling(forest, id) != Some(target) || !same_parent,
        DropPlace::After => next_sibling(forest, target) != Some(id) || !same_parent,
        DropPlace::Inside => {
            let node = &forest.nodes[id as usize];
            !(node.parent == Some(target) && next_sibling(forest, id).is_none())
        }
        DropPlace::Edge(_) => false,
    }
}

fn under(forest: &Forest, mut id: u32, ancestor: u32) -> bool {
    for _ in 0..=MAX_DEPTH {
        if id == ancestor {
            return true;
        }
        match forest.nodes[id as usize].parent {
            Some(parent) => id = parent,
            None => return false,
        }
    }
    true
}

fn next_sibling(forest: &Forest, id: u32) -> Option<u32> {
    let node = &forest.nodes[id as usize];
    if node.sibling_index + 1 >= node.set_size {
        return None;
    }
    match node.parent {
        Some(parent) => {
            let parent = &forest.nodes[parent as usize];
            Some(forest.children[(parent.child_start + node.sibling_index + 1) as usize])
        }
        None => forest.roots.get(node.sibling_index as usize + 1).copied(),
    }
}

fn note_drop(world: &mut World) {
    let Some(drop) = committed(world) else {
        return;
    };
    if drop.payload.kind != drag::TREE_NODE {
        return;
    }
    let Some(owner) = drop.payload.owner else {
        return;
    };
    if world.get::<TreeState>(owner).is_none() {
        return;
    }
    let Some(row) = world.get::<TreeRow>(drop.target).copied() else {
        return;
    };
    if row.tree != owner || row.loading {
        return;
    }
    let Some(state) = world.get::<TreeState>(owner).cloned() else {
        return;
    };
    let Some(id_index) = state.forest.by_id.get(&NodeId(drop.payload.value)).copied() else {
        return;
    };
    let Some(target_index) = state.forest.by_id.get(&row.node).copied() else {
        return;
    };
    if !forest_accept(&state.forest, id_index, target_index, drop.place) {
        return;
    }
    let id = NodeId(drop.payload.value);
    let name = state.forest.nodes[id_index as usize].label.clone();
    let target_name = state.forest.nodes[target_index as usize].label.clone();
    emit(
        world,
        owner,
        TreeEventKind::Reordered(TreeMove {
            id,
            target: row.node,
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

fn navigate(world: &mut World, entity: Entity, event: &KeyEvent) -> bool {
    let Some(root) = tree_from(world, entity) else {
        return false;
    };
    if event.synthetic {
        return false;
    }
    let Some(state) = world.get::<TreeState>(root) else {
        return false;
    };
    if !state.props.enabled {
        return false;
    }
    let alt_move = event.modifiers.alt && !event.modifiers.command() && !event.modifiers.shift;
    match event.logical {
        LogicalKey::Named(key) if alt_move && state.props.reorder => {
            if event.released || event.repeat {
                return matches!(
                    key,
                    NamedKey::ArrowUp
                        | NamedKey::ArrowDown
                        | NamedKey::ArrowLeft
                        | NamedKey::ArrowRight
                );
            }
            keyboard_move(world, root, key)
        }
        LogicalKey::Named(key) => on_named(world, root, key, event),
        LogicalKey::Character(ch) if !event.modifiers.command() && !event.modifiers.alt => {
            if event.released || event.repeat {
                return true;
            }
            on_char(world, root, ch)
        }
        _ => false,
    }
}

fn on_named(world: &mut World, root: Entity, key: NamedKey, event: &KeyEvent) -> bool {
    if key == NamedKey::Space {
        let on_root = world.resource::<Focus>().and_then(|focus| focus.entity) == Some(root);
        if !on_root {
            return false;
        }
        if event.released || event.repeat || event.modifiers.alt {
            return on_root;
        }
        if let Some(id) = world.get::<TreeState>(root).and_then(|state| state.active) {
            click_select(world, root, id);
        }
        return true;
    }
    if event.modifiers.command() || event.modifiers.alt {
        return false;
    }
    let extend =
        event.modifiers.shift && selection_mode(world, root) == Some(TreeSelection::Multiple);
    let handled = matches!(
        key,
        NamedKey::ArrowUp
            | NamedKey::ArrowDown
            | NamedKey::ArrowLeft
            | NamedKey::ArrowRight
            | NamedKey::Home
            | NamedKey::End
            | NamedKey::PageUp
            | NamedKey::PageDown
            | NamedKey::Enter
            | NamedKey::F(2)
    );
    if !handled {
        return false;
    }
    if event.released || (event.repeat && matches!(key, NamedKey::Enter | NamedKey::F(2))) {
        return true;
    }
    match key {
        NamedKey::ArrowDown => move_by(world, root, 1, extend),
        NamedKey::ArrowUp => move_by(world, root, -1, extend),
        NamedKey::Home => jump(world, root, true, extend),
        NamedKey::End => jump(world, root, false, extend),
        NamedKey::PageDown => page(world, root, 1, extend),
        NamedKey::PageUp => page(world, root, -1, extend),
        NamedKey::ArrowRight => horizontal(world, root, true),
        NamedKey::ArrowLeft => horizontal(world, root, false),
        NamedKey::Enter => activate_focused(world, root),
        NamedKey::F(2) => rename_focused(world, root),
        _ => false,
    }
}

fn on_char(world: &mut World, root: Entity, ch: char) -> bool {
    if ch == '*' {
        expand_siblings(world, root);
        return true;
    }
    if ch.is_control() {
        return false;
    }
    let now = world
        .resource::<Time>()
        .map(|time| time.now)
        .unwrap_or_default();
    let mut state = world.get::<TreeState>(root).unwrap().clone();
    if now.saturating_sub(state.typed_at) > TYPEAHEAD {
        state.typed.clear();
    }
    state.typed.push(ch);
    state.typed_at = now;
    let prefix = state.typed.clone();
    let from = state.active;
    let found = find_prefix(&state, from, &prefix).or_else(|| {
        state.typed.clear();
        state.typed.push(ch);
        find_prefix(&state, from, &state.typed)
    });
    world.insert(root, state);
    if let Some(id) = found {
        let _ = show_focus(world, root, id, true);
    }
    true
}

fn find_prefix(state: &TreeState, from: Option<NodeId>, prefix: &str) -> Option<NodeId> {
    if prefix.is_empty() {
        return None;
    }
    let start = from
        .and_then(|id| state.forest.by_id.get(&id).copied())
        .map(|index| state.visible.place[index as usize])
        .filter(|place| *place != u32::MAX)
        .map(|place| place as usize);
    let rows = &state.visible.rows;
    let scan = |range: std::ops::Range<usize>| {
        for row in rows.iter().take(range.end).skip(range.start) {
            if is_loading(row.key) {
                continue;
            }
            let node = &state.forest.nodes[row.key as usize];
            if node.live && prefix_match(&node.label, prefix) {
                return Some(node.id);
            }
        }
        None
    };
    if let Some(start) = start {
        scan(start + 1..rows.len()).or_else(|| scan(0..start + 1))
    } else {
        scan(0..rows.len())
    }
}

fn prefix_match(label: &str, prefix: &str) -> bool {
    let mut label = label.chars().flat_map(char::to_lowercase);
    for ch in prefix.chars().flat_map(char::to_lowercase) {
        if label.next() != Some(ch) {
            return false;
        }
    }
    true
}

fn expand_siblings(world: &mut World, root: Entity) {
    let Some(state) = world.get::<TreeState>(root).cloned() else {
        return;
    };
    let Some(id) = state.active else {
        return;
    };
    let Some(index) = state.forest.by_id.get(&id).copied() else {
        return;
    };
    let parent = state.forest.nodes[index as usize].parent;
    let siblings: Vec<u32> = match parent {
        Some(parent) => {
            let node = &state.forest.nodes[parent as usize];
            state.forest.children
                [node.child_start as usize..node.child_start as usize + node.child_count as usize]
                .to_vec()
        }
        None => state.forest.roots.clone(),
    };
    let changes: Vec<(u32, bool)> = siblings
        .into_iter()
        .filter(|index| is_branch(&state.forest, *index) && !bit(&state.expanded, *index))
        .map(|index| (index, true))
        .collect();
    if !changes.is_empty() {
        apply_expansion(world, root, &changes);
    }
}

fn move_by(world: &mut World, root: Entity, delta: isize, extend: bool) -> bool {
    let Some(state) = world.get::<TreeState>(root) else {
        return false;
    };
    let order = enabled_order(state);
    if order.is_empty() {
        return true;
    }
    let current = state
        .active
        .and_then(|id| order.iter().position(|item| *item == id));
    let next = match (current, delta) {
        (Some(index), 1) => order.get(index + 1).copied(),
        (Some(index), -1) if index > 0 => Some(order[index - 1]),
        (None, 1) => order.first().copied(),
        (None, -1) => order.last().copied(),
        _ => None,
    };
    if let Some(id) = next {
        land(world, root, id, extend);
    }
    true
}

fn jump(world: &mut World, root: Entity, home: bool, extend: bool) -> bool {
    let Some(state) = world.get::<TreeState>(root) else {
        return false;
    };
    let order = enabled_order(state);
    let id = if home { order.first() } else { order.last() }.copied();
    if let Some(id) = id {
        land(world, root, id, extend);
    }
    true
}

fn page(world: &mut World, root: Entity, delta: isize, extend: bool) -> bool {
    let Some(state) = world.get::<TreeState>(root) else {
        return false;
    };
    let height = world
        .get::<Rect>(root)
        .map(|rect| rect.h)
        .unwrap_or(state.props.row_height);
    let lines = ((height / state.props.row_height).floor() as isize).max(1);
    let order = enabled_order(state);
    if order.is_empty() {
        return true;
    }
    let current = state
        .active
        .and_then(|id| order.iter().position(|item| *item == id))
        .unwrap_or(0);
    let next = if delta > 0 {
        current.saturating_add(lines as usize).min(order.len() - 1)
    } else {
        current.saturating_sub(lines as usize)
    };
    land(world, root, order[next], extend);
    true
}

fn horizontal(world: &mut World, root: Entity, right: bool) -> bool {
    let Some(state) = world.get::<TreeState>(root).cloned() else {
        return false;
    };
    let Some(id) = state.active else {
        return true;
    };
    let Some(index) = state.forest.by_id.get(&id).copied() else {
        return true;
    };
    let node = &state.forest.nodes[index as usize];
    if right {
        if is_branch(&state.forest, index) && !bit(&state.expanded, index) {
            apply_expansion(world, root, &[(index, true)]);
        } else if is_branch(&state.forest, index) && bit(&state.expanded, index) {
            if let Some(child) = first_child_enabled(&state, index) {
                land(world, root, child, false);
            }
        }
    } else if is_branch(&state.forest, index) && bit(&state.expanded, index) {
        apply_expansion(world, root, &[(index, false)]);
    } else if let Some(parent) = node.parent {
        let parent_id = state.forest.nodes[parent as usize].id;
        if state.forest.nodes[parent as usize].live {
            land(world, root, parent_id, false);
        }
    }
    true
}

fn first_child_enabled(state: &TreeState, index: u32) -> Option<NodeId> {
    let node = &state.forest.nodes[index as usize];
    for n in 0..node.child_count {
        let child = state.forest.children[(node.child_start + n) as usize];
        let rec = &state.forest.nodes[child as usize];
        if rec.live && state.visible.place[child as usize] != u32::MAX {
            return Some(rec.id);
        }
    }
    None
}

fn land(world: &mut World, root: Entity, id: NodeId, extend: bool) {
    if extend {
        let mut state = world.get::<TreeState>(root).unwrap().clone();
        if state.anchor.is_none() {
            state.anchor = state.active;
        }
        let anchor = state.anchor.unwrap_or(id);
        let order = enabled_order(&state);
        if let Some(range) = span(&order, anchor, id) {
            if state.chosen != range {
                state.chosen = range;
                emit(world, root, TreeEventKind::SelectionSetChanged);
            }
        }
        state.active = Some(id);
        state.ensure = Some(id);
        world.insert(root, state);
        let _ = show_focus(world, root, id, true);
    } else {
        let _ = show_focus(world, root, id, true);
    }
}

fn span(order: &[NodeId], anchor: NodeId, id: NodeId) -> Option<BTreeSet<NodeId>> {
    let a = order.iter().position(|item| *item == anchor)?;
    let b = order.iter().position(|item| *item == id)?;
    let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
    Some(order[lo..=hi].iter().copied().collect())
}

fn enabled_order(state: &TreeState) -> Vec<NodeId> {
    state
        .visible
        .rows
        .iter()
        .filter_map(|row| {
            if is_loading(row.key) {
                return None;
            }
            let node = &state.forest.nodes[row.key as usize];
            (node.live && state.props.enabled).then_some(node.id)
        })
        .collect()
}

fn activate_focused(world: &mut World, root: Entity) -> bool {
    let Some(id) = world.get::<TreeState>(root).and_then(|state| state.active) else {
        return true;
    };
    if world
        .get::<TreeState>(root)
        .is_some_and(|state| selectable(state, id))
    {
        emit(world, root, TreeEventKind::Activated(id));
    }
    true
}

fn rename_focused(world: &mut World, root: Entity) -> bool {
    let Some(id) = world.get::<TreeState>(root).and_then(|state| state.active) else {
        return true;
    };
    if world
        .get::<TreeState>(root)
        .is_some_and(|state| selectable(state, id))
    {
        emit(world, root, TreeEventKind::RenameRequested(id));
    }
    true
}

fn note_reorder_keys(world: &mut World) {
    let keys = world
        .resource::<Input>()
        .map(|input| input.keys.clone())
        .unwrap_or_default();
    let mut used = Vec::new();
    for (index, event) in keys.iter().enumerate() {
        if event.released
            || event.repeat
            || event.synthetic
            || !event.modifiers.alt
            || event.modifiers.command()
            || event.modifiers.shift
        {
            continue;
        }
        let LogicalKey::Named(key) = event.logical else {
            continue;
        };
        if !matches!(
            key,
            NamedKey::ArrowUp | NamedKey::ArrowDown | NamedKey::ArrowLeft | NamedKey::ArrowRight
        ) {
            continue;
        }
        let Some(focus) = world.resource::<Focus>().and_then(|focus| focus.entity) else {
            continue;
        };
        let Some(root) = tree_from(world, focus) else {
            continue;
        };
        let reorder = world
            .get::<TreeState>(root)
            .is_some_and(|state| state.props.reorder && state.props.enabled);
        if reorder && keyboard_move(world, root, key) {
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

fn keyboard_move(world: &mut World, root: Entity, key: NamedKey) -> bool {
    let handled = matches!(
        key,
        NamedKey::ArrowUp | NamedKey::ArrowDown | NamedKey::ArrowLeft | NamedKey::ArrowRight
    );
    if !handled {
        return false;
    }
    let Some(state) = world.get::<TreeState>(root).cloned() else {
        return false;
    };
    let Some(id) = state.active else {
        drag::announce(world, "Cannot move.");
        return true;
    };
    let Some(index) = state.forest.by_id.get(&id).copied() else {
        return true;
    };
    let name = state.forest.nodes[index as usize].label.clone();
    let movement = match key {
        NamedKey::ArrowUp | NamedKey::ArrowDown | NamedKey::ArrowRight => {
            let order = enabled_order(&state);
            let pos = order.iter().position(|item| *item == id);
            match (key, pos) {
                (NamedKey::ArrowUp, Some(pos)) if pos > 0 => {
                    Some((order[pos - 1], DropPlace::Before))
                }
                (NamedKey::ArrowDown, Some(pos)) => order
                    .get(pos + 1)
                    .copied()
                    .map(|target| (target, DropPlace::After)),
                (NamedKey::ArrowRight, Some(pos)) if pos > 0 => {
                    Some((order[pos - 1], DropPlace::Inside))
                }
                _ => None,
            }
        }
        NamedKey::ArrowLeft => state.forest.nodes[index as usize]
            .parent
            .map(|parent| (state.forest.nodes[parent as usize].id, DropPlace::After)),
        _ => None,
    };
    match movement {
        Some((target, place)) => {
            let target_index = state.forest.by_id[&target];
            if forest_accept(&state.forest, index, target_index, place) {
                let target_name = state.forest.nodes[target_index as usize].label.clone();
                emit(
                    world,
                    root,
                    TreeEventKind::Reordered(TreeMove { id, target, place }),
                );
                drag::announce(
                    world,
                    &format!("Moved {name} {} {target_name}.", drag::place_word(place)),
                );
            } else {
                drag::announce(world, &format!("Cannot move {name}."));
            }
        }
        None => drag::announce(world, &format!("Cannot move {name}.")),
    }
    true
}

fn activate(world: &mut World, entity: Entity) {
    let Some(row) = world.get::<TreeRow>(entity).copied() else {
        return;
    };
    if row.loading {
        return;
    }
    if row.toggle {
        let index = world
            .get::<TreeState>(row.tree)
            .and_then(|state| state.forest.by_id.get(&row.node).copied());
        if let Some(index) = index {
            let open = world
                .get::<TreeState>(row.tree)
                .is_some_and(|state| bit(&state.expanded, index));
            apply_expansion(world, row.tree, &[(index, !open)]);
            let _ = show_focus(world, row.tree, row.node, true);
        }
        return;
    }
    click_select(world, row.tree, row.node);
}

fn click_select(world: &mut World, root: Entity, id: NodeId) {
    let Some(mut state) = world.get::<TreeState>(root).cloned() else {
        return;
    };
    if !selectable(&state, id) {
        return;
    }
    let shift = world.resource::<Input>().is_some_and(|input| input.shift);
    let command = world
        .resource::<Input>()
        .is_some_and(|input| input.ctrl || input.super_key);
    match state.props.selection {
        TreeSelection::None => {
            state.active = Some(id);
            world.insert(root, state);
            let _ = show_focus(world, root, id, true);
        }
        TreeSelection::Single => {
            let next = if command && state.selected == Some(id) {
                None
            } else {
                Some(id)
            };
            if state.selected != next {
                state.selected = next;
                emit(world, root, TreeEventKind::Selected(next));
            }
            state.active = Some(id);
            world.insert(root, state);
            let _ = show_focus(world, root, id, true);
        }
        TreeSelection::Multiple => {
            let order = enabled_order(&state);
            let changed = if shift {
                let anchor = state.anchor.unwrap_or(id);
                if let Some(range) = span(&order, anchor, id) {
                    let next = if command {
                        state.chosen.union(&range).copied().collect()
                    } else {
                        range
                    };
                    if next != state.chosen {
                        state.chosen = next;
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            } else if command {
                if !state.chosen.remove(&id) {
                    state.chosen.insert(id);
                }
                state.anchor = Some(id);
                true
            } else {
                let next = BTreeSet::from([id]);
                let changed = state.chosen != next;
                state.chosen = next;
                state.anchor = Some(id);
                changed
            };
            if changed {
                emit(world, root, TreeEventKind::SelectionSetChanged);
            }
            state.active = Some(id);
            world.insert(root, state);
            let _ = show_focus(world, root, id, true);
        }
    }
}

fn select_for_menu(world: &mut World, root: Entity, id: NodeId) {
    let Some(state) = world.get::<TreeState>(root).cloned() else {
        return;
    };
    if !selectable(&state, id) || is_selected(&state, id) {
        return;
    }
    click_select(world, root, id);
}

fn selection_mode(world: &World, root: Entity) -> Option<TreeSelection> {
    world
        .get::<TreeState>(root)
        .map(|state| state.props.selection)
}

fn tree_from(world: &World, entity: Entity) -> Option<Entity> {
    if world.has::<TreeState>(entity) {
        Some(entity)
    } else {
        row_of(world, entity).map(|row| row.tree)
    }
}

fn row_of(world: &World, entity: Entity) -> Option<TreeRow> {
    world.get::<TreeRow>(entity).copied().or_else(|| {
        ancestor_with::<TreeRow>(world, entity).and_then(|row| world.get::<TreeRow>(row).copied())
    })
}

fn focus_inside(world: &World, root: Entity) -> bool {
    let Some(focus) = world.resource::<Focus>().and_then(|focus| focus.entity) else {
        return false;
    };
    focus == root || row_of(world, focus).is_some_and(|row| row.tree == root)
}

fn emit(world: &mut World, tree: Entity, kind: TreeEventKind) {
    send(world, TreeEvent { tree, kind });
}

fn validate(world: &World, root: Entity) -> Result<(), UiError> {
    let state = world.get::<TreeState>(root).ok_or(UiError::Stale)?;
    if !world.is_alive(root) || !world.has::<Scroll>(root) || !world.has::<Layout>(root) {
        return Err(UiError::Stale);
    }
    for entity in [state.top, state.bottom, state.empty] {
        if world.get::<Parent>(entity).map(|parent| parent.entity) != Some(root) {
            return Err(UiError::Stale);
        }
    }
    Ok(())
}
