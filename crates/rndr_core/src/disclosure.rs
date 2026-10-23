use crate::entity::Entity;
use crate::hierarchy::{despawn, set_parent};
use crate::input::Input;
use crate::layout::{ancestor_with, hit_test, Axis, Layout, Scroll, Size};
use crate::style::{Body, Color, Label};
use crate::world::{Component, World};

impl Component for Disclosure {}
impl Component for Focusable {}

/// Marks a column that can own the keystream. Headers and the page are not focusable.
#[derive(Clone, Copy, Debug, Default)]
pub struct Focusable;

/// Expand template: child labels (and optional nested templates) to spawn when opened.
///
/// [`spawn_node`] builds one entity from a spec. [`toggle_disclosure`] spawns
/// [`NodeSpec::children`] when a closed node that is not a directory opens.
#[derive(Clone, Debug)]
pub struct NodeSpec {
    /// Header text of the spawned node.
    pub label: String,
    /// Fill color of the header row.
    pub color: Color,
    /// Nested templates. Empty means [`spawn_node`] omits [`Disclosure`].
    pub children: Vec<NodeSpec>,
}

impl NodeSpec {
    /// Template with no nested nodes. [`spawn_node`] omits [`Disclosure`].
    pub fn leaf(label: impl Into<String>, color: Color) -> Self {
        Self {
            label: label.into(),
            color,
            children: Vec::new(),
        }
    }

    /// Template whose [`NodeSpec::children`] are spawned when the node opens.
    pub fn branch(label: impl Into<String>, color: Color, children: Vec<NodeSpec>) -> Self {
        Self {
            label: label.into(),
            color,
            children,
        }
    }
}

/// Open state plus the subtree this node can spawn.
///
/// A closed node keeps its templates and an empty [`Disclosure::spawned`].
/// Opening a directory lists the filesystem instead of [`Disclosure::children`].
/// Closing saves dirty filesystem descendants, then despawns that spawned list.
#[derive(Clone, Debug)]
pub struct Disclosure {
    /// Whether the spawned children are currently in the world.
    pub open: bool,
    /// Templates used on open when this node is not a directory.
    pub children: Vec<NodeSpec>,
    /// Entities created the last time this node opened. Cleared on close.
    pub spawned: Vec<Entity>,
}

impl Disclosure {
    /// Closed disclosure. `children` are used on open when it is not a directory.
    pub fn new(children: Vec<NodeSpec>) -> Self {
        Self {
            open: false,
            children,
            spawned: Vec::new(),
        }
    }
}

/// Spawn a disclosure node: a hugging column with a header row.
///
/// The column fills width, hugs height, and leaves a 4-point gap. A spec with
/// children also gets a closed [`Disclosure`]. A leaf does not. Both get
/// [`Focusable`] and an empty [`Body`]. The header is a child row, 36 points
/// tall, padded by 8, filled with `spec.color`, and labeled with `spec.label`.
/// Returns the column.
pub fn spawn_node(world: &mut World, spec: NodeSpec) -> Entity {
    let mut layout = Layout::column();
    layout.gap = 4.0;
    layout.width = Size::Fill;
    layout.height = Size::Hug;

    let can_expand = !spec.children.is_empty();
    let root = if can_expand {
        world.spawn((
            layout,
            Disclosure::new(spec.children),
            Focusable,
            Body::default(),
        ))
    } else {
        world.spawn((layout, Focusable, Body::default()))
    };

    let mut header = Layout::row();
    header.padding = 8.0;
    header.width = Size::Fill;
    header.height = Size::Px(36.0);
    let header_entity = world.spawn((header, spec.color, Label::new(spec.label)));
    set_parent(world, header_entity, root);
    root
}

/// Toggle the disclosure under the pointer. The shell does not run this: click selects, Right expands.
///
/// Returns with no change when [`Input`] is missing, the pointer is not on
/// its press edge, the point hits nothing, or the hit has no [`Disclosure`]
/// on itself or an ancestor.
pub fn toggle_disclosures(world: &mut World) {
    let Some(input) = world.resource::<Input>().cloned() else {
        return;
    };
    if !input.pointer.pressed {
        return;
    }
    let Some(hit) = hit_test(world, input.pointer.position[0], input.pointer.position[1]) else {
        return;
    };
    let Some(target) = ancestor_with::<Disclosure>(world, hit) else {
        return;
    };
    toggle_disclosure(world, target);
}

/// Open `target` if it is closed, or close it if it is open.
///
/// Returns immediately when `target` is dead or has no [`Disclosure`].
/// Closing saves dirty filesystem descendants, despawns [`Disclosure::spawned`],
/// and clears that list. Opening a directory [`crate::FsPath`] spawns a node
/// per child of that path. Opening any other node spawns
/// [`Disclosure::children`] with [`spawn_node`] and parents them to `target`.
pub fn toggle_disclosure(world: &mut World, target: Entity) {
    let Some(current) = world.get::<Disclosure>(target).cloned() else {
        return;
    };
    if current.open {
        crate::fs::flush_dirty_subtree(world, target);
        for child in current.spawned {
            despawn(world, child);
        }
        if let Some(disclosure) = world.get_mut::<Disclosure>(target) {
            disclosure.open = false;
            disclosure.spawned.clear();
        }
        return;
    }

    let mut spawned = Vec::new();
    if world
        .get::<crate::fs::FsPath>(target)
        .is_some_and(|fs| fs.dir)
    {
        spawned = crate::fs::spawn_fs_children(world, target);
    } else {
        for spec in current.children {
            let child = spawn_node(world, spec);
            set_parent(world, child, target);
            spawned.push(child);
        }
    }
    if let Some(disclosure) = world.get_mut::<Disclosure>(target) {
        disclosure.open = true;
        disclosure.spawned = spawned;
    }
}

/// Convenience layout for a page that stacks root disclosures.
///
/// Column, 8-point gap, 16-point padding, fill width, hug height. Minimums
/// are zero.
pub fn page_layout() -> Layout {
    Layout {
        axis: Axis::Column,
        gap: 8.0,
        padding: 16.0,
        width: Size::Fill,
        height: Size::Hug,
        min_width: 0.0,
        min_height: 0.0,
    }
}

/// Window-filling column that offsets overflowing children.
///
/// The layout matches [`page_layout`] except the height fills the parent.
/// The [`Scroll`] offset and content size start at zero.
pub fn scroll_page() -> (Layout, Scroll) {
    (
        Layout {
            axis: Axis::Column,
            gap: 8.0,
            padding: 16.0,
            width: Size::Fill,
            height: Size::Fill,
            min_width: 0.0,
            min_height: 0.0,
        },
        Scroll::default(),
    )
}
