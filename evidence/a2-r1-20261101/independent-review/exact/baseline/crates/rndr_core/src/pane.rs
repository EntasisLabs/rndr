use crate::caret::{EditField, Field};
use crate::disclosure::Disclosure;
use crate::entity::Entity;
use crate::hierarchy::{children_of, despawn, set_parent};
use crate::layout::{Axis, Layout, Scroll, Size};
use crate::split::Splitter;
use crate::style::{Body, Color, Label};
use crate::theme::theme_of;
use crate::world::{Component, World};

impl Component for Pane {}

/// The entity whose interior is shown in a [`Pane`].
#[derive(Clone, Copy, Debug, Default)]
pub struct Selection {
    /// Tree node to show, or `None` for the empty pane. A dead id is stored
    /// as given; [`apply_pane`] treats it as no selection.
    pub entity: Option<Entity>,
}

/// Right-hand surface. Spawned children are the current body, not the pane itself.
#[derive(Clone, Debug, Default)]
pub struct Pane {
    /// Children [`apply_pane`] spawned for the current body. Despawned on the
    /// next rebuild. These are not the pane entity itself.
    pub spawned: Vec<Entity>,
    /// Selection currently reflected in [`Self::spawned`]. `None` is the
    /// empty body, including after a dead selection.
    pub shown: Option<Entity>,
}

/// Window-filling row for a tree plus a detail pane.
pub fn shell() -> Layout {
    Layout {
        axis: Axis::Row,
        gap: 0.0,
        padding: 0.0,
        width: Size::Fill,
        height: Size::Fill,
        min_width: 0.0,
        min_height: 0.0,
    }
}

/// Vertical drag bar between a [`tree_column`] and a [`detail_pane`].
pub fn splitter() -> (Layout, Splitter) {
    (
        Layout {
            axis: Axis::Column,
            gap: 0.0,
            padding: 0.0,
            width: Size::Px(8.0),
            height: Size::Fill,
            min_width: 0.0,
            min_height: 0.0,
        },
        Splitter::default(),
    )
}

/// Scrollable tree column on the left of a [`shell`].
pub fn tree_column() -> (Layout, Scroll) {
    (
        Layout {
            axis: Axis::Column,
            gap: 8.0,
            padding: 16.0,
            width: Size::Px(280.0),
            height: Size::Fill,
            min_width: 0.0,
            min_height: 0.0,
        },
        Scroll::default(),
    )
}

/// Fill pane that [`apply_pane`] rebuilds from [`Selection`].
pub fn detail_pane() -> (Layout, Scroll, Pane) {
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
        Pane::default(),
    )
}

/// Point [`Selection`] at `entity`.
///
/// Does nothing when the resource is missing. [`crate::Shell`] inserts one.
/// A dead id is stored as given; [`apply_pane`] treats it as no selection.
pub fn set_selection(world: &mut World, entity: Option<Entity>) {
    if let Some(selection) = world.resource_mut::<Selection>() {
        selection.entity = entity;
    }
}

/// Rebuild every [`Pane`] so it matches [`Selection`].
///
/// A dead or missing selection clears the pane and leaves one "Nothing open"
/// row, and skips that work when the empty body is already up. A pane whose
/// [`Pane::shown`] is already the live selection is left alone. Any other
/// pane despawns [`Pane::spawned`] and rebuilds: an editable title (the
/// node's label, or "Untitled"), an editable body from [`crate::Body`], a
/// status line (`Open`, `Closed`, or `Leaf`), then one row per disclosure
/// child label.
pub fn apply_pane(world: &mut World) {
    let selected = world
        .resource::<Selection>()
        .and_then(|s| s.entity)
        .filter(|entity| world.is_alive(*entity));
    let panes = world.entities_with::<Pane>();
    for pane in panes {
        sync_pane(world, pane, selected);
    }
}

fn sync_pane(world: &mut World, pane: Entity, selected: Option<Entity>) {
    let Some(current) = world.get::<Pane>(pane).cloned() else {
        return;
    };
    if selected.is_none() {
        if current.shown.is_none() && !current.spawned.is_empty() {
            return;
        }
        clear_pane(world, pane);
        let empty = body_row(world, pane, "Nothing open", theme_of(world).empty);
        if let Some(p) = world.get_mut::<Pane>(pane) {
            p.spawned = vec![empty];
            p.shown = None;
        }
        return;
    }
    let selected = selected.unwrap();
    if current.shown == Some(selected) {
        return;
    }
    clear_pane(world, pane);
    let spawned = spawn_body(world, pane, selected);
    if let Some(p) = world.get_mut::<Pane>(pane) {
        p.spawned = spawned;
        p.shown = Some(selected);
    }
}

fn clear_pane(world: &mut World, pane: Entity) {
    let spawned = world
        .get::<Pane>(pane)
        .map(|p| p.spawned.clone())
        .unwrap_or_default();
    for child in spawned {
        despawn(world, child);
    }
    if let Some(p) = world.get_mut::<Pane>(pane) {
        p.spawned.clear();
        p.shown = None;
    }
}

/// Newline-based row height, in logical points.
///
/// The count is `text.split('\n').count()`, so an empty string is one row.
/// The result uses the bitmap line height and is at least 36. A hugging body
/// is sized by layout from the face.
pub fn field_height(text: &str) -> f32 {
    crate::wrap::field_height_lines(text.split('\n').count())
}

fn spawn_body(world: &mut World, pane: Entity, selected: Entity) -> Vec<Entity> {
    let title = node_label(world, selected).unwrap_or_else(|| "Untitled".to_string());
    let body = world
        .get::<Body>(selected)
        .map(|body| body.text.clone())
        .unwrap_or_default();
    let status = if let Some(disclosure) = world.get::<Disclosure>(selected) {
        if disclosure.open {
            "Open"
        } else {
            "Closed"
        }
    } else {
        "Leaf"
    };
    let child_labels: Vec<String> = world
        .get::<Disclosure>(selected)
        .map(|d| d.children.iter().map(|spec| spec.label.clone()).collect())
        .unwrap_or_default();

    let mut spawned = Vec::new();
    let theme = theme_of(world);
    spawned.push(field_row(
        world,
        pane,
        title,
        theme.title,
        Some(EditField::Title),
        36.0,
    ));
    spawned.push(field_row(
        world,
        pane,
        body.clone(),
        theme.body,
        Some(EditField::Body),
        field_height(&body),
    ));
    spawned.push(field_row(world, pane, status, theme.status, None, 36.0));
    for label in child_labels {
        spawned.push(field_row(world, pane, label, theme.child, None, 36.0));
    }
    spawned
}

fn field_row(
    world: &mut World,
    pane: Entity,
    text: impl Into<String>,
    color: Color,
    field: Option<EditField>,
    height: f32,
) -> Entity {
    let mut layout = Layout::row();
    layout.padding = 8.0;
    layout.width = Size::Fill;
    if field == Some(crate::caret::EditField::Body) {
        layout.height = Size::Hug;
        layout.min_height = 36.0;
    } else {
        layout.height = Size::Px(height);
    }
    let text = text.into();
    let row = if let Some(kind) = field {
        world.spawn((layout, color, Label::new(text), Field { kind }))
    } else {
        world.spawn((layout, color, Label::new(text)))
    };
    set_parent(world, row, pane);
    row
}

fn body_row(world: &mut World, pane: Entity, text: impl Into<String>, color: Color) -> Entity {
    field_row(world, pane, text, color, None, 36.0)
}

fn node_label(world: &World, entity: Entity) -> Option<String> {
    for child in children_of(world, entity) {
        if let Some(label) = world.get::<Label>(child) {
            return Some(label.text.clone());
        }
    }
    world.get::<Label>(entity).map(|label| label.text.clone())
}
