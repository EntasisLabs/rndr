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
use crate::prompt::Prompt;
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
    /// Application-owned hierarchical collection.
    Tree,
    /// One node in an application-owned tree.
    TreeItem,
    /// Row-focused data table, not an editable grid.
    Table,
    /// Stable data row.
    Row,
    /// Named table column header.
    ColumnHeader,
    /// Passive data cell.
    Cell,
    /// Two-state checked control.
    CheckBox,
    /// Noneditable popup selection trigger.
    ComboBox,
    /// Single or multiple selection popup.
    ListBox,
    /// Stable option in a ListBox.
    Option,
    /// Interactive canonical viewport scroll track.
    ScrollBar,
    /// Source code. The value is the code text. The name is the language or
    /// title when one was set.
    Code,
    /// A tooltip that describes another node. Pair it with [`DescribedBy`]
    /// on the node it describes.
    Tooltip,
    /// A positioned image. The name is the label or the source.
    Image,
}
/// Supplemental selection projection; input still uses canonical Button admission.
#[derive(Clone, Debug)]
pub struct SelectionAccess {
    /// Projected role.
    pub role: AccessRole,
    /// Checked state for CheckBox only.
    pub checked: Option<bool>,
    /// Expanded state for trigger only.
    pub expanded: Option<bool>,
    /// True for multiple-selection ListBox.
    pub multiselect: bool,
    /// Live controlled popup, only while expanded.
    pub controls: Option<Entity>,
    /// Live trigger labelling the popup.
    pub labelled_by: Option<Entity>,
}
impl crate::Component for SelectionAccess {}
/// Passive table projection; does not add cell focus, input or model ownership.
#[derive(Clone, Debug)]
pub struct DataAccess {
    /// Table, Row, ColumnHeader, Cell or an internal Group.
    pub role: AccessRole,
    /// Stable live column header entities labelling this cell.
    pub headers: Vec<Entity>,
}
impl crate::Component for DataAccess {}

/// Semantic projection for composed collections; does not install input or data ownership.
#[derive(Clone, Copy, Debug)]
pub struct CollectionAccess {
    /// Collection or row role.
    pub role: AccessRole,
    /// Expanded state for branches only.
    pub expanded: Option<bool>,
    /// One-based tree depth (rows only).
    pub level: Option<usize>,
    /// Logical tree parent row. The layout may be a flat scroll stack; the
    /// accessibility projection restores hierarchy without a second layout tree.
    pub parent: Option<Entity>,
}
impl crate::Component for CollectionAccess {}

/// Pins an [`AccessRole`] onto an entity that would otherwise be a plain
/// label, a nameless box, or skipped.
///
/// Specific widgets still win: a button, field, list, table, disclosure or
/// scrollbar keeps its own role. This is checked immediately before the
/// plain [`crate::Label`] role.
#[derive(Clone, Copy, Debug)]
pub struct AccessMark(pub AccessRole);
impl crate::Component for AccessMark {}

/// Entities whose content describes this one.
///
/// The desktop bridge maps these through AccessKit `described_by`, the same
/// way [`DataAccess::headers`] describes a cell. Targets that are hidden or
/// no longer alive are omitted.
#[derive(Clone, Debug)]
pub struct DescribedBy(pub Vec<Entity>);
impl crate::Component for DescribedBy {}

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
    // Keep logical tree hierarchy even when the layout is a flat visible stack.
    // Iterating projected preorder preserves application sibling reorderings.
    let ids: std::collections::BTreeSet<_> = nodes.iter().map(|n| n.id).collect();
    let mut moved = std::collections::BTreeSet::new();
    let mut logical = std::collections::BTreeMap::<u64, Vec<u64>>::new();
    for node in &nodes {
        if let Some(parent) = entity_from_access_id(node.id)
            .and_then(|e| world.get::<CollectionAccess>(e))
            .and_then(|a| a.parent)
            .map(access_id)
            .filter(|p| ids.contains(p))
        {
            moved.insert(node.id);
            logical.entry(parent).or_default().push(node.id);
        }
    }
    children.retain(|id| !moved.contains(id));
    for node in &mut nodes {
        node.children.retain(|id| !moved.contains(id));
        if let Some(children) = logical.remove(&node.id) {
            node.children.extend(children);
        }
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
    /// Set a focused editor's directed selection against a published text state.
    /// Readonly selection is allowed; this does not replace text or steal focus.
    SetTextSelection {
        /// Target generation-qualified view.
        entity: Entity,
        /// Exact previously published source state.
        state: crate::AccessTextState,
        /// Directed document byte endpoints.
        selection: crate::editing::SourceSelection,
    },
    /// Set a scrollbar's positive numeric offset. Replacing text is unsupported.
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
    let _ = try_apply_access(world, command);
}

/// Observable result for an accessibility request. The compatibility void
/// entry point delegates here; unsupported mutations never alter text/history.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccessCommandResult {
    /// Focus/selection/disclosure applied.
    Applied,
    /// Canonical button/tab result, including cardinality and overflow.
    Activation(crate::ActivationResult),
    /// Text SetValue requires a validated editor-history operation, not a raw write.
    UnsupportedSetValue,
    /// Invalid/hidden/disabled/readonly/unsupported target.
    Rejected,
}

/// Validate and apply the exact same matrix the native bridge advertises.
pub fn try_apply_access(world: &mut World, command: AccessCommand) -> AccessCommandResult {
    match command {
        AccessCommand::Focus(entity) => {
            if crate::request_focus(world, entity) {
                AccessCommandResult::Applied
            } else {
                AccessCommandResult::Rejected
            }
        }
        AccessCommand::Activate(entity) => {
            if !crate::access_details(world, entity).activate {
                return AccessCommandResult::Rejected;
            }
            if world.has::<Button>(entity) || world.has::<Tab>(entity) {
                AccessCommandResult::Activation(crate::try_activate(world, entity))
            } else {
                activate(world, entity);
                AccessCommandResult::Applied
            }
        }
        AccessCommand::SetTextSelection {
            entity,
            state,
            selection,
        } => crate::access_text::set_selection(world, entity, state, selection),
        AccessCommand::SetValue { entity, value } if world.has::<crate::Scrollbar>(entity) => {
            if value
                .parse::<f32>()
                .ok()
                .is_some_and(|v| crate::set_scrollbar_value(world, entity, v))
            {
                AccessCommandResult::Applied
            } else {
                AccessCommandResult::Rejected
            }
        }
        AccessCommand::SetValue { .. } => AccessCommandResult::UnsupportedSetValue,
    }
}

fn activate(world: &mut World, entity: Entity) {
    if !world.is_alive(entity) {
        return;
    }
    if world.has::<Button>(entity) || world.has::<Tab>(entity) {
        crate::try_activate(world, entity);
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
        AccessRole::Button
            | AccessRole::CheckBox
            | AccessRole::ComboBox
            | AccessRole::Option
            | AccessRole::ListItem
            | AccessRole::TreeItem
            | AccessRole::Label
            | AccessRole::Code
            | AccessRole::Tooltip
            | AccessRole::Image
    ) {
        for child in children_of(world, entity) {
            children.extend(collect(world, child, focus, out));
        }
    }
    // Composed tree rows fold passive content into the supplied name, but
    // expose their separate disclosure command to assistive technologies.
    if role == AccessRole::TreeItem
        && world
            .get::<CollectionAccess>(entity)
            .is_some_and(|a| a.expanded.is_some())
    {
        children.extend(collection_commands(world, entity, focus, out));
    }
    let name = world
        .get::<crate::Semantics>(entity)
        .and_then(|s| s.name.clone())
        .unwrap_or_else(|| match role {
            AccessRole::TextInput => text_name(world, entity),
            _ => visible_text(world, entity),
        });
    let value = match role {
        AccessRole::TextInput => text_value(world, entity),
        AccessRole::Code => world
            .get::<Label>(entity)
            .map(|label| label.text.clone())
            .unwrap_or_default(),
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
        focusable: if world.has::<CollectionAccess>(entity) || world.has::<SelectionAccess>(entity)
        {
            crate::access_details(world, entity).focus
        } else {
            world.has::<Focusable>(entity)
                || matches!(
                    role,
                    AccessRole::Button | AccessRole::TextInput | AccessRole::List | AccessRole::Tab
                )
        },
        focused: focus.is_some_and(|focus| {
            focus == entity
                || (!world.has::<CollectionAccess>(entity) && focus_inside(world, entity, focus))
        }),
        selected: selected(world, entity, role),
        enabled: crate::interaction::policy(world, entity).0,
        expanded: world
            .get::<SelectionAccess>(entity)
            .and_then(|s| s.expanded)
            .or_else(|| {
                world
                    .get::<CollectionAccess>(entity)
                    .and_then(|a| a.expanded)
                    .or_else(|| world.get::<Disclosure>(entity).map(|d| d.open))
            }),
    };
    out.push(node);
    vec![id]
}

// Passive composition wrappers do not hide a branch's separate AT command.
fn collection_commands(
    world: &World,
    entity: Entity,
    focus: Option<Entity>,
    out: &mut Vec<AccessNode>,
) -> Vec<u64> {
    let mut commands = Vec::new();
    for child in children_of(world, entity) {
        if world.has::<Hidden>(child) {
            continue;
        }
        if world.has::<Button>(child) {
            commands.extend(collect(world, child, focus, out));
        } else {
            commands.extend(collection_commands(world, child, focus, out));
        }
    }
    commands
}

fn role_of(world: &World, entity: Entity) -> Option<AccessRole> {
    if world.has::<crate::Scrollbar>(entity) {
        return Some(AccessRole::ScrollBar);
    }
    if let Some(access) = world.get::<SelectionAccess>(entity) {
        return Some(access.role);
    }
    if let Some(access) = world.get::<DataAccess>(entity) {
        return Some(access.role);
    }
    if let Some(access) = world.get::<CollectionAccess>(entity) {
        return Some(access.role);
    }
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
    if let Some(mark) = world.get::<AccessMark>(entity) {
        return Some(mark.0);
    }
    if world.has::<Label>(entity) {
        return Some(AccessRole::Label);
    }
    if world.has::<crate::TabPanel>(entity) {
        return Some(AccessRole::Group);
    }
    if world.has::<Focusable>(entity) {
        return Some(AccessRole::Group);
    }
    None
}

fn selected(world: &World, entity: Entity, role: AccessRole) -> bool {
    if let Some(state) = world.get::<crate::SelectedState>(entity) {
        return state.0;
    }
    match role {
        AccessRole::ListItem => world
            .get::<ListItem>(entity)
            .is_some_and(|item| item.selected),
        AccessRole::Tab => {
            let id = world.get::<Tab>(entity).map(|tab| tab.id);
            let tabs = crate::tabs::tab_group(world, entity);
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

fn text_name(world: &World, entity: Entity) -> String {
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
        if world.has::<Hidden>(child)
            || (role_of(world, child).is_some_and(|role| role != AccessRole::Label))
        {
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
    consider.extend(world.entities_with::<crate::Editor>());
    consider.extend(world.entities_with::<Focusable>());
    consider.extend(world.entities_with::<Disclosure>());
    consider.extend(world.entities_with::<Tabs>());
    consider.extend(world.entities_with::<crate::TabPanel>());
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
