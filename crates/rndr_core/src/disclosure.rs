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
#[derive(Clone, Debug)]
pub struct NodeSpec {
    pub label: String,
    pub color: Color,
    pub children: Vec<NodeSpec>,
}

impl NodeSpec {
    pub fn leaf(label: impl Into<String>, color: Color) -> Self {
        Self {
            label: label.into(),
            color,
            children: Vec::new(),
        }
    }

    pub fn branch(label: impl Into<String>, color: Color, children: Vec<NodeSpec>) -> Self {
        Self {
            label: label.into(),
            color,
            children,
        }
    }
}

/// Open state plus the subtree this node can spawn.
#[derive(Clone, Debug)]
pub struct Disclosure {
    pub open: bool,
    pub children: Vec<NodeSpec>,
    pub spawned: Vec<Entity>,
}

impl Disclosure {
    pub fn new(children: Vec<NodeSpec>) -> Self {
        Self {
            open: false,
            children,
            spawned: Vec::new(),
        }
    }
}

/// Spawn a disclosure node: a hugging column with a header row.
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
pub fn toggle_disclosures(world: &mut World) {
    let Some(input) = world.resource::<Input>().cloned() else {
        return;
    };
    if !input.pressed {
        return;
    }
    let Some(hit) = hit_test(world, input.cursor[0], input.cursor[1]) else {
        return;
    };
    let Some(target) = ancestor_with::<Disclosure>(world, hit) else {
        return;
    };
    toggle_disclosure(world, target);
}

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
pub fn page_layout() -> Layout {
    Layout {
        axis: Axis::Column,
        gap: 8.0,
        padding: 16.0,
        width: Size::Fill,
        height: Size::Hug,
    }
}

/// Window-filling column that offsets overflowing children.
pub fn scroll_page() -> (Layout, Scroll) {
    (
        Layout {
            axis: Axis::Column,
            gap: 8.0,
            padding: 16.0,
            width: Size::Fill,
            height: Size::Fill,
        },
        Scroll::default(),
    )
}
