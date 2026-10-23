//! Push [`rndr_core::access_tree`] to the platform accessibility adapter.

use accesskit::{Action, ActionData, Node, NodeId, Rect, Role, TreeId, TreeInfo, TreeUpdate};
use rndr_core::{
    access_tree, entity_from_access_id, AccessCommand, AccessRole, World, ACCESS_ROOT,
};

pub(crate) fn tree_update(world: &World) -> TreeUpdate {
    let tree = access_tree(world);
    let mut focus = NodeId(ACCESS_ROOT);
    let nodes = tree
        .nodes
        .into_iter()
        .map(|node| {
            if node.focused {
                focus = NodeId(node.id);
            }
            (NodeId(node.id), platform_node(&node))
        })
        .collect();
    TreeUpdate {
        nodes,
        tree: Some(TreeInfo::new(NodeId(tree.root))),
        tree_id: TreeId::ROOT,
        focus,
    }
}

pub(crate) fn command(request: &accesskit::ActionRequest) -> Option<AccessCommand> {
    let entity = entity_from_access_id(request.target_node.0)?;
    match request.action {
        Action::Focus => Some(AccessCommand::Focus(entity)),
        Action::Click => Some(AccessCommand::Activate(entity)),
        Action::SetValue => {
            let value = match request.data.as_ref() {
                Some(ActionData::Value(value)) => value.to_string(),
                _ => return None,
            };
            Some(AccessCommand::SetValue { entity, value })
        }
        _ => None,
    }
}

fn platform_node(node: &rndr_core::AccessNode) -> Node {
    let mut out = Node::new(role(node.role));
    if node.role == AccessRole::Label {
        if !node.name.is_empty() {
            out.set_value(node.name.clone());
        }
    } else if !node.name.is_empty() {
        out.set_label(node.name.clone());
    }
    if node.role == AccessRole::TextInput && !node.value.is_empty() {
        out.set_value(node.value.clone());
    }
    out.set_bounds(Rect {
        x0: f64::from(node.bounds.x),
        y0: f64::from(node.bounds.y),
        x1: f64::from(node.bounds.x + node.bounds.w),
        y1: f64::from(node.bounds.y + node.bounds.h),
    });
    if !node.children.is_empty() {
        let children: Vec<NodeId> = node.children.iter().copied().map(NodeId).collect();
        out.set_children(children);
    }
    if node.focusable {
        out.add_action(Action::Focus);
    }
    if matches!(
        node.role,
        AccessRole::Button
            | AccessRole::ListItem
            | AccessRole::List
            | AccessRole::Disclosure
            | AccessRole::Tab
    ) {
        out.add_action(Action::Click);
    }
    if node.role == AccessRole::TextInput {
        out.add_action(Action::SetValue);
    }
    if !node.enabled {
        out.set_disabled();
    }
    if node.selected {
        out.set_selected(true);
    }
    if let Some(expanded) = node.expanded {
        out.set_expanded(expanded);
    }
    out
}

fn role(role: AccessRole) -> Role {
    match role {
        AccessRole::Window => Role::Window,
        AccessRole::Group => Role::Group,
        AccessRole::Button => Role::Button,
        AccessRole::Label => Role::Label,
        AccessRole::TextInput => Role::TextInput,
        AccessRole::List => Role::List,
        AccessRole::ListItem => Role::ListItem,
        AccessRole::TabList => Role::TabList,
        AccessRole::Tab => Role::Tab,
        AccessRole::Disclosure => Role::DisclosureTriangle,
    }
}
