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
            (NodeId(node.id), platform_node(world, &node))
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

fn platform_node(world: &World, node: &rndr_core::AccessNode) -> Node {
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
    let details = entity_from_access_id(node.id)
        .map(|e| rndr_core::access_details(world, e))
        .unwrap_or_default();
    if details.focus {
        out.add_action(Action::Focus);
    }
    if details.activate {
        out.add_action(Action::Click);
    }
    if details.set_value {
        out.add_action(Action::SetValue);
    }
    if details.readonly {
        out.set_read_only();
    }
    if !details.description.is_empty() {
        out.set_description(details.description);
    }
    if details.panel {
        out.set_role(Role::TabPanel);
    }
    if let Some(id) = details.controls {
        out.set_controls(vec![NodeId(id)]);
    }
    if let Some(id) = details.labelled_by {
        out.set_labelled_by(vec![NodeId(id)]);
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

#[cfg(test)]
mod tests {
    use super::*;
    use rndr_core::*;

    #[test]
    fn actions_descriptions_and_real_panel_relations_match_dispatch() {
        let mut app = App::new();
        app.add_plugin(GenericInteraction);
        let button = app.world_mut().spawn((
            Button::new("save"),
            ReleaseActivation,
            Semantics {
                name: Some("Save".into()),
                description: "Save document".into(),
                readonly: false,
            },
        ));
        let tree = tree_update(app.world());
        let native = tree
            .nodes
            .iter()
            .find(|(id, _)| *id == NodeId(access_id(button)))
            .unwrap();
        assert!(native.1.supports_action(Action::Click));
        assert_eq!(native.1.description(), Some("Save document"));
        app.world_mut()
            .get_mut::<Semantics>(button)
            .unwrap()
            .readonly = true;
        let tree = tree_update(app.world());
        let native = &tree
            .nodes
            .iter()
            .find(|(id, _)| *id == NodeId(access_id(button)))
            .unwrap()
            .1;
        assert!(!native.supports_action(Action::Click));
        assert!(native.is_read_only());
        let text = app.world_mut().spawn((TextField, Label::new("value")));
        let tree = tree_update(app.world());
        let native = &tree
            .nodes
            .iter()
            .find(|(id, _)| *id == NodeId(access_id(text)))
            .unwrap()
            .1;
        assert!(!native.supports_action(Action::SetValue));
        assert!(!native.is_read_only());
        let group = app.world_mut().spawn((Tabs { selected: 1 },));
        let tab = app.world_mut().spawn((Tab { id: 1 },));
        set_parent(app.world_mut(), tab, group);
        let panel = app.world_mut().spawn((TabPanel,));
        link_tab_panel(app.world_mut(), tab, group, panel).unwrap();
        let tree = tree_update(app.world());
        let node = |entity| {
            &tree
                .nodes
                .iter()
                .find(|(id, _)| *id == NodeId(access_id(entity)))
                .unwrap()
                .1
        };
        assert_eq!(node(panel).role(), Role::TabPanel);
        assert_eq!(node(tab).controls(), &[NodeId(access_id(panel))]);
        assert_eq!(node(panel).labelled_by(), &[NodeId(access_id(tab))]);
    }
}
