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
    if let Some(data) =
        entity_from_access_id(node.id).and_then(|e| world.get::<rndr_core::DataAccess>(e))
    {
        if !data.headers.is_empty() {
            out.set_described_by(
                data.headers
                    .iter()
                    .filter(|e| world.is_alive(**e))
                    .map(|e| NodeId(rndr_core::access_id(*e)))
                    .collect::<Vec<_>>(),
            );
        }
    }
    if let Some(id) = details.labelled_by {
        out.set_labelled_by(vec![NodeId(id)]);
    }
    if let Some(orientation) = details.orientation {
        out.set_orientation(match orientation {
            rndr_core::TabOrientation::Horizontal => accesskit::Orientation::Horizontal,
            rndr_core::TabOrientation::Vertical => accesskit::Orientation::Vertical,
        });
    }
    if let Some(s) =
        entity_from_access_id(node.id).and_then(|e| world.get::<rndr_core::SelectionAccess>(e))
    {
        if let Some(checked) = s.checked {
            out.set_toggled(if checked {
                accesskit::Toggled::True
            } else {
                accesskit::Toggled::False
            });
        }
        if s.multiselect {
            out.set_multiselectable();
        }
        if let Some(e) = s.controls.filter(|e| world.is_alive(*e)) {
            out.set_controls(vec![NodeId(rndr_core::access_id(e))]);
        }
        if let Some(e) = s.labelled_by.filter(|e| world.is_alive(*e)) {
            out.set_labelled_by(vec![NodeId(rndr_core::access_id(e))]);
        }
    }
    if !node.enabled {
        out.set_disabled();
    }
    if node.selected
        || matches!(
            node.role,
            AccessRole::ListItem | AccessRole::TreeItem | AccessRole::Row | AccessRole::Option
        )
    {
        out.set_selected(node.selected);
    }
    if let Some(level) = entity_from_access_id(node.id)
        .and_then(|e| world.get::<rndr_core::CollectionAccess>(e))
        .and_then(|a| a.level)
    {
        out.set_level(level);
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
        AccessRole::Tree => Role::Tree,
        AccessRole::TreeItem => Role::TreeItem,
        AccessRole::Table => Role::Table,
        AccessRole::Row => Role::Row,
        AccessRole::ColumnHeader => Role::ColumnHeader,
        AccessRole::Cell => Role::Cell,
        AccessRole::CheckBox => Role::CheckBox,
        AccessRole::ComboBox => Role::ComboBox,
        AccessRole::ListBox => Role::ListBox,
        AccessRole::Option => Role::ListBoxOption,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rndr_core::*;

    #[test]
    fn composed_tree_roles_hierarchy_level_selection_and_focus_reach_native_adapter() {
        let mut app = App::new();
        app.add_plugin(GenericInteraction);
        let w = app.world_mut();
        let tree = w.spawn((
            Layout::column(),
            Focusable,
            CollectionAccess {
                role: AccessRole::Tree,
                expanded: None,
                level: None,
                parent: None,
            },
        ));
        let branch = w.spawn((
            Layout::row(),
            Button::new("select"),
            ReleaseActivation,
            SelectedState(false),
            CollectionAccess {
                role: AccessRole::TreeItem,
                expanded: Some(true),
                level: Some(1),
                parent: None,
            },
        ));
        let leaf = w.spawn((
            Layout::row(),
            Button::new("select"),
            ReleaseActivation,
            SelectedState(true),
            CollectionAccess {
                role: AccessRole::TreeItem,
                expanded: None,
                level: Some(2),
                parent: Some(branch),
            },
        ));
        set_parent(w, branch, tree);
        set_parent(w, leaf, tree);
        assert!(request_focus(w, leaf));
        let t = tree_update(w);
        let node = |e| {
            &t.nodes
                .iter()
                .find(|(id, _)| *id == NodeId(access_id(e)))
                .unwrap()
                .1
        };
        assert_eq!(node(tree).role(), Role::Tree);
        assert_eq!(node(branch).role(), Role::TreeItem);
        assert_eq!(node(branch).is_expanded(), Some(true));
        assert_eq!(node(branch).is_selected(), Some(false));
        assert_eq!(node(leaf).is_selected(), Some(true));
        assert_eq!(node(leaf).level(), Some(2));
        assert_eq!(node(tree).children(), &[NodeId(access_id(branch))]);
        assert_eq!(node(branch).children(), &[NodeId(access_id(leaf))]);
        assert_eq!(t.focus, NodeId(access_id(leaf)));
        w.get_mut::<Button>(leaf).unwrap().enabled = false;
        let t = tree_update(w);
        let leaf = &t
            .nodes
            .iter()
            .find(|(id, _)| *id == NodeId(access_id(leaf)))
            .unwrap()
            .1;
        assert!(leaf.is_disabled());
        assert!(!leaf.supports_action(Action::Click));
        assert!(!leaf.supports_action(Action::Focus));
    }
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
    #[test]
    fn manual_orientation_selected_and_disabled_matrix_reaches_accesskit() {
        use rndr_core::*;
        let mut app = App::new();
        app.add_plugin(GenericInteraction);
        let group = app.world_mut().spawn((
            Layout::row(),
            Tabs { selected: 0 },
            ManualTabs(TabOrientation::Vertical),
            Semantics {
                name: Some("Reading mode".into()),
                ..Semantics::default()
            },
        ));
        let row = app.world_mut().spawn((
            Layout::column(),
            Button::new("row"),
            ReleaseActivation,
            SelectedState(true),
            ControlDisabled,
            Semantics {
                name: Some("Draft, unsaved".into()),
                ..Semantics::default()
            },
        ));
        app.update();
        let tree = tree_update(app.world());
        let node = |e| {
            &tree
                .nodes
                .iter()
                .find(|(id, _)| *id == NodeId(access_id(e)))
                .unwrap()
                .1
        };
        assert_eq!(
            node(group).orientation(),
            Some(accesskit::Orientation::Vertical)
        );
        assert_eq!(node(row).is_selected(), Some(true));
        assert!(node(row).is_disabled());
        assert!(!node(row).supports_action(accesskit::Action::Focus));
        assert!(!node(row).supports_action(accesskit::Action::Click));
        assert_eq!(node(row).label(), Some("Draft, unsaved"));
    }
    #[test]
    fn selection_checked_expanded_multiselect_and_option_states_reach_accesskit() {
        use rndr_core::*;
        let mut app = App::new();
        app.add_plugin(GenericInteraction);
        let w = app.world_mut();
        let meta = |role| SelectionAccess {
            role,
            checked: None,
            expanded: None,
            multiselect: false,
            controls: None,
            labelled_by: None,
        };
        let check = w.spawn((
            Layout::row(),
            Button::new("check"),
            Focusable,
            Semantics {
                name: Some("Enabled".into()),
                ..Default::default()
            },
        ));
        let mut cm = meta(AccessRole::CheckBox);
        cm.checked = Some(true);
        w.insert(check, cm);
        let trigger = w.spawn((
            Layout::row(),
            Button::new("trigger"),
            Focusable,
            Semantics {
                name: Some("Channels".into()),
                ..Default::default()
            },
        ));
        let popup = w.spawn((Layout::column(), Focusable));
        let mut lm = meta(AccessRole::ListBox);
        lm.multiselect = true;
        lm.labelled_by = Some(trigger);
        w.insert(popup, lm);
        let mut tm = meta(AccessRole::ComboBox);
        tm.expanded = Some(true);
        tm.controls = Some(popup);
        w.insert(trigger, tm);
        let option = w.spawn((
            Layout::row(),
            Button::new("option"),
            Focusable,
            SelectedState(true),
            Semantics {
                name: Some("Design".into()),
                readonly: true,
                ..Default::default()
            },
            meta(AccessRole::Option),
        ));
        set_parent(w, option, popup);
        let t = tree_update(w);
        let native = |e| {
            &t.nodes
                .iter()
                .find(|(id, _)| *id == NodeId(access_id(e)))
                .unwrap()
                .1
        };
        assert_eq!(native(check).role(), Role::CheckBox);
        assert_eq!(native(check).toggled(), Some(accesskit::Toggled::True));
        assert_eq!(native(trigger).role(), Role::ComboBox);
        assert_eq!(native(trigger).is_expanded(), Some(true));
        assert_eq!(native(trigger).controls(), &[NodeId(access_id(popup))]);
        assert_eq!(native(popup).role(), Role::ListBox);
        assert!(native(popup).is_multiselectable());
        assert_eq!(native(popup).labelled_by(), &[NodeId(access_id(trigger))]);
        assert_eq!(native(option).role(), Role::ListBoxOption);
        assert_eq!(native(option).is_selected(), Some(true));
        assert!(native(option).is_read_only());
        assert!(!native(option).supports_action(Action::Click));
    }
    #[test]
    fn table_roles_values_header_description_and_no_cell_focus_reach_accesskit() {
        use rndr_core::*;
        let mut app = App::new();
        app.add_plugin(GenericInteraction);
        let w = app.world_mut();
        let table = w.spawn((
            Layout::column(),
            DataAccess {
                role: AccessRole::Table,
                headers: vec![],
            },
            Semantics {
                name: Some("Projects".into()),
                ..Semantics::default()
            },
        ));
        let header = w.spawn((
            Label::new("Progress"),
            DataAccess {
                role: AccessRole::ColumnHeader,
                headers: vec![],
            },
        ));
        set_parent(w, header, table);
        let row = w.spawn((
            Button::new("row"),
            ReleaseActivation,
            Focusable,
            SelectedState(true),
            CollectionAccess {
                role: AccessRole::Row,
                expanded: None,
                level: None,
                parent: None,
            },
        ));
        set_parent(w, row, table);
        let cell = w.spawn((
            Layout::column(),
            DataAccess {
                role: AccessRole::Cell,
                headers: vec![header],
            },
        ));
        set_parent(w, cell, row);
        let text = w.spawn((Label::new("35 percent"),));
        set_parent(w, text, cell);
        let t = tree_update(w);
        let native = |e| {
            &t.nodes
                .iter()
                .find(|(id, _)| *id == NodeId(access_id(e)))
                .unwrap()
                .1
        };
        assert_eq!(native(table).role(), Role::Table);
        assert_eq!(native(header).role(), Role::ColumnHeader);
        assert_eq!(native(row).role(), Role::Row);
        assert_eq!(native(row).is_selected(), Some(true));
        assert_eq!(native(cell).role(), Role::Cell);
        assert_eq!(native(cell).label(), Some("35 percent"));
        assert_eq!(native(cell).described_by(), &[NodeId(access_id(header))]);
        assert!(native(cell).labelled_by().is_empty());
        assert!(!native(cell).supports_action(Action::Focus));
        assert!(!native(cell).supports_action(Action::Click));
    }
}
