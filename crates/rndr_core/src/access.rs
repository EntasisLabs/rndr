//! Accessibility tree for the widgets already in the world.
//!
//! [`access_tree`] walks the hierarchy and emits a role, a name, and bounds
//! for each interactive widget. Layout containers that have no role of their
//! own are flattened, so a button is not buried under nameless groups.
//! [`Hidden`](crate::Hidden) subtrees are skipped.
//!
//! The desktop runner pushes this tree to the platform on macOS and Windows.
//! [`apply_access`] performs a focus, activate, or set-value command from
//! that platform. A list can also be walked from the keyboard: a [`ListKeys`](crate::ListKeys)
//! list is focusable, and arrows move its selection when focus is on it.

use crate::button::Button;
use crate::disclosure::{toggle_disclosure, Disclosure, Focusable};
use crate::editor::Buffer;
use crate::entity::Entity;
use crate::event::send;
use crate::field::TextField;
use crate::hierarchy::{children_of, Parent};
use crate::input::Focus;
use crate::layout::Rect;
use crate::list::{List, ListActivated, ListItem};
use crate::prompt::{Placeholder, Prompt};
use crate::style::Label;
use crate::tabs::{Hidden, Tab, Tabs};
use crate::world::World;

/// Window id that is not an entity. Entity ids start at 1.
pub const ACCESS_ROOT: u64 = u64::MAX;

/// Role stored on an [`AccessNode`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccessRole {
    /// The window. One root, id [`ACCESS_ROOT`].
    Window,
    /// A group of other nodes. Used for a focusable row that is not a more
    /// specific widget.
    Group,
    /// A [`crate::Button`].
    Button,
    /// A [`Label`] that is not the text of a button or list row.
    Label,
    /// A [`TextField`], [`Prompt`], or [`crate::Editor`].
    TextInput,
    /// A [`List`].
    List,
    /// One pooled [`ListItem`].
    ListItem,
    /// A [`Tabs`] header row.
    TabList,
    /// One [`crate::Tab`].
    Tab,
    /// A [`Disclosure`].
    Disclosure,
}

/// One node in [`AccessTree`].
#[derive(Clone, Debug, PartialEq)]
pub struct AccessNode {
    /// [`access_id`] of the entity, or [`ACCESS_ROOT`] for the window.
    pub id: u64,
    /// Widget kind.
    pub role: AccessRole,
    /// Name read by a client. Empty when the widget has no label.
    pub name: String,
    /// Current text of a field, prompt, or editor. Empty for other roles.
    pub value: String,
    /// Layout rect in logical points. Zero when the entity has no [`Rect`].
    pub bounds: Rect,
    /// Child ids, in hierarchy order.
    pub children: Vec<u64>,
    /// The node can take keyboard focus.
    pub focusable: bool,
    /// This node, or a descendant that was folded into it, is focused.
    pub focused: bool,
    /// Selected tab or list row.
    pub selected: bool,
    /// `false` only for a disabled [`crate::Button`].
    pub enabled: bool,
    /// `Some` when the node is a disclosure. `true` means it is open.
    pub expanded: Option<bool>,
}

/// The window node plus every node [`access_tree`] emitted.
#[derive(Clone, Debug, PartialEq)]
pub struct AccessTree {
    /// [`ACCESS_ROOT`].
    pub root: u64,
    /// Nodes in preorder. The first is the window when the tree is non-empty.
    pub nodes: Vec<AccessNode>,
}

/// Stable id for `entity`. Never zero and never [`ACCESS_ROOT`] for a real entity.
pub fn access_id(entity: Entity) -> u64 {
    let raw = (u64::from(entity.generation()) << 32) | u64::from(entity.index());
    let id = raw.saturating_add(1);
    if id == ACCESS_ROOT {
        ACCESS_ROOT - 1
    } else {
        id
    }
}

/// Entity encoded by [`access_id`], whether or not it is still alive.
///
/// [`ACCESS_ROOT`] returns `None`.
pub fn entity_from_access_id(id: u64) -> Option<Entity> {
    if id == 0 || id == ACCESS_ROOT {
        return None;
    }
    let raw = id - 1;
    Some(Entity::new(raw as u32, (raw >> 32) as u32))
}

/// Build the tree from `world`.
///
/// The window node is always present. Its children are the interactive widgets
/// and labels that are not hidden and not folded into a parent button or row.
pub fn access_tree(world: &World) -> AccessTree {
    let focus = world.resource::<Focus>().and_then(|focus| focus.entity);
    let mut nodes = Vec::new();
    let mut children = Vec::new();
    for root in widget_roots(world) {
        children.extend(collect(world, root, focus, &mut nodes));
    }
    nodes.insert(
        0,
        AccessNode {
            id: ACCESS_ROOT,
            role: AccessRole::Window,
            name: window_name(world),
            value: String::new(),
            bounds: window_bounds(world),
            children,
            focusable: false,
            focused: false,
            selected: false,
            enabled: true,
            expanded: None,
        },
    );
    AccessTree {
        root: ACCESS_ROOT,
        nodes,
    }
}

/// What a platform accessibility client asked the app to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AccessCommand {
    /// Move [`Focus`] to an entity.
    Focus(Entity),
    /// Press a button, activate a list row, or toggle a disclosure.
    Activate(Entity),
    /// Replace the text of a field, prompt, or editor.
    SetValue {
        /// Target entity.
        entity: Entity,
        /// New text.
        value: String,
    },
}

/// Apply `command`. A dead entity is ignored.
///
/// Activate on a button writes [`crate::Fired`] when the button is enabled.
/// Activate on a list item selects it and sends [`ListActivated`]. Activate on
/// a list sends [`ListActivated`] for the current selection. Activate on a
/// disclosure toggles it.
pub fn apply_access(world: &mut World, command: AccessCommand) {
    match command {
        AccessCommand::Focus(entity) => {
            if !world.is_alive(entity) {
                return;
            }
            if let Some(focus) = world.resource_mut::<Focus>() {
                focus.entity = Some(entity);
            } else {
                world.insert_resource(Focus {
                    entity: Some(entity),
                });
            }
        }
        AccessCommand::Activate(entity) => activate(world, entity),
        AccessCommand::SetValue { entity, value } => set_value(world, entity, value),
    }
}

fn activate(world: &mut World, entity: Entity) {
    if !world.is_alive(entity) {
        return;
    }
    if let Some(button) = world.get::<Button>(entity).cloned() {
        if button.enabled {
            if let Some(fired) = world.resource_mut::<crate::action::Fired>() {
                fired.name = Some(button.action);
            }
        }
        return;
    }
    if let Some(item) = world.get::<ListItem>(entity).copied() {
        if let Some(list) = ancestor_list(world, entity) {
            if let Some(list_state) = world.get_mut::<List>(list) {
                list_state.selected = Some(item.index);
            }
            send(
                world,
                ListActivated {
                    list,
                    index: item.index,
                },
            );
        }
        return;
    }
    if world.has::<List>(entity) {
        if let Some(index) = world.get::<List>(entity).and_then(|list| list.selected) {
            send(
                world,
                ListActivated {
                    list: entity,
                    index,
                },
            );
        }
        return;
    }
    if world.has::<Disclosure>(entity) {
        toggle_disclosure(world, entity);
    }
}

fn set_value(world: &mut World, entity: Entity, value: String) {
    if !world.is_alive(entity) {
        return;
    }
    if world.has::<TextField>(entity) || world.has::<Prompt>(entity) {
        if let Some(label) = world.get_mut::<Label>(entity) {
            label.text = value;
        }
        return;
    }
    if let Some(buffer) = world.get_mut::<Buffer>(entity) {
        let len = buffer.len();
        buffer.delete_range(0, len);
        buffer.insert_str(0, &value);
        buffer.caret = buffer.len();
        buffer.anchor = buffer.caret;
    }
}

fn ancestor_list(world: &World, mut entity: Entity) -> Option<Entity> {
    for _ in 0..64 {
        if world.has::<List>(entity) {
            return Some(entity);
        }
        entity = world.get::<Parent>(entity)?.entity;
        if !world.is_alive(entity) {
            return None;
        }
    }
    None
}

fn collect(
    world: &World,
    entity: Entity,
    focus: Option<Entity>,
    out: &mut Vec<AccessNode>,
) -> Vec<u64> {
    if !world.is_alive(entity) || world.has::<Hidden>(entity) {
        return Vec::new();
    }
    let Some(role) = role_of(world, entity) else {
        let mut ids = Vec::new();
        for child in children_of(world, entity) {
            ids.extend(collect(world, child, focus, out));
        }
        return ids;
    };
    let id = access_id(entity);
    let mut children = Vec::new();
    if !matches!(
        role,
        AccessRole::Button | AccessRole::ListItem | AccessRole::Label
    ) {
        for child in children_of(world, entity) {
            children.extend(collect(world, child, focus, out));
        }
    }
    let name = match role {
        AccessRole::TextInput => text_name(world, entity),
        _ => visible_text(world, entity),
    };
    let value = match role {
        AccessRole::TextInput => text_value(world, entity),
        _ => String::new(),
    };
    let node = AccessNode {
        id,
        role,
        name,
        value,
        bounds: world.get::<Rect>(entity).copied().unwrap_or(Rect {
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 0.0,
        }),
        children,
        focusable: world.has::<Focusable>(entity)
            || matches!(
                role,
                AccessRole::Button | AccessRole::TextInput | AccessRole::List | AccessRole::Tab
            ),
        focused: focus.is_some_and(|focus| focus == entity || focus_inside(world, entity, focus)),
        selected: selected(world, entity, role),
        enabled: world
            .get::<Button>(entity)
            .map(|button| button.enabled)
            .unwrap_or(true),
        expanded: world
            .get::<Disclosure>(entity)
            .map(|disclosure| disclosure.open),
    };
    out.push(node);
    vec![id]
}

fn role_of(world: &World, entity: Entity) -> Option<AccessRole> {
    if world.has::<Button>(entity) {
        return Some(AccessRole::Button);
    }
    if world.has::<TextField>(entity)
        || world.has::<Prompt>(entity)
        || world.has::<crate::editor::Editor>(entity)
    {
        return Some(AccessRole::TextInput);
    }
    if world.has::<List>(entity) {
        return Some(AccessRole::List);
    }
    if world.has::<ListItem>(entity) {
        return Some(AccessRole::ListItem);
    }
    if world.has::<Tabs>(entity) {
        return Some(AccessRole::TabList);
    }
    if world.has::<Tab>(entity) {
        return Some(AccessRole::Tab);
    }
    if world.has::<Disclosure>(entity) {
        return Some(AccessRole::Disclosure);
    }
    if world.has::<Label>(entity) {
        return Some(AccessRole::Label);
    }
    if world.has::<Focusable>(entity) {
        return Some(AccessRole::Group);
    }
    None
}

fn selected(world: &World, entity: Entity, role: AccessRole) -> bool {
    match role {
        AccessRole::ListItem => world
            .get::<ListItem>(entity)
            .is_some_and(|item| item.selected),
        AccessRole::Tab => {
            let id = world.get::<Tab>(entity).map(|tab| tab.id);
            let tabs = ancestor_tabs(world, entity);
            match (id, tabs) {
                (Some(id), Some(tabs)) => world
                    .get::<Tabs>(tabs)
                    .is_some_and(|tabs| tabs.selected == id),
                _ => false,
            }
        }
        _ => false,
    }
}

fn ancestor_tabs(world: &World, mut entity: Entity) -> Option<Entity> {
    for _ in 0..64 {
        if world.has::<Tabs>(entity) {
            return Some(entity);
        }
        entity = world.get::<Parent>(entity)?.entity;
        if !world.is_alive(entity) {
            return None;
        }
    }
    None
}

fn text_name(world: &World, entity: Entity) -> String {
    if let Some(placeholder) = world.get::<Placeholder>(entity) {
        if !placeholder.text.is_empty() {
            return placeholder.text.clone();
        }
    }
    if world.has::<Prompt>(entity) {
        "Prompt".to_string()
    } else if world.has::<crate::editor::Editor>(entity) {
        "Editor".to_string()
    } else {
        "Text field".to_string()
    }
}

fn text_value(world: &World, entity: Entity) -> String {
    if let Some(buffer) = world.get::<Buffer>(entity) {
        return buffer.text();
    }
    world
        .get::<Label>(entity)
        .map(|label| label.text.clone())
        .unwrap_or_default()
}

fn visible_text(world: &World, entity: Entity) -> String {
    let mut parts = Vec::new();
    if let Some(label) = world.get::<Label>(entity) {
        if !label.text.is_empty() {
            parts.push(label.text.clone());
        }
    }
    for child in children_of(world, entity) {
        if world.has::<Hidden>(child) || role_of(world, child).is_some() {
            continue;
        }
        let nested = visible_text(world, child);
        if !nested.is_empty() {
            parts.push(nested);
        }
    }
    parts.join(" ")
}

fn focus_inside(world: &World, ancestor: Entity, focus: Entity) -> bool {
    let mut current = focus;
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

fn widget_roots(world: &World) -> Vec<Entity> {
    let mut roots = Vec::new();
    let mut consider = world.entities_with::<Label>();
    consider.extend(world.entities_with::<Button>());
    consider.extend(world.entities_with::<List>());
    consider.extend(world.entities_with::<TextField>());
    consider.extend(world.entities_with::<Prompt>());
    consider.extend(world.entities_with::<Focusable>());
    consider.extend(world.entities_with::<Disclosure>());
    consider.extend(world.entities_with::<Tabs>());
    for entity in consider {
        let root = top(world, entity);
        if !roots.contains(&root) {
            roots.push(root);
        }
    }
    roots
}

fn top(world: &World, mut entity: Entity) -> Entity {
    for _ in 0..64 {
        match world.get::<Parent>(entity).map(|parent| parent.entity) {
            Some(parent) if world.is_alive(parent) => entity = parent,
            _ => break,
        }
    }
    entity
}

fn window_name(world: &World) -> String {
    world
        .resource::<crate::window::WindowConfig>()
        .map(|config| config.title.clone())
        .unwrap_or_else(|| "rndr".to_string())
}

fn window_bounds(world: &World) -> Rect {
    world
        .resource::<crate::input::Viewport>()
        .map(|viewport| Rect {
            x: 0.0,
            y: 0.0,
            w: viewport.width,
            h: viewport.height,
        })
        .unwrap_or(Rect {
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 0.0,
        })
}
