//! Push [`rndr_core::access_tree`] to the platform accessibility adapter.

use accesskit::{Action, ActionData, Live, Node, NodeId, Rect, Role, TreeId, TreeInfo, TreeUpdate};
use rndr_core::{
    access_tree, entity_from_access_id, AccessCommand, AccessRole, World, ACCESS_ROOT,
};

#[derive(Default)]
pub(crate) struct Bridge {
    // All node IDs, including entity IDs, share this retained allocator. Never
    // recycle an ID: queued actions must not acquire a new owner after removal.
    ids: std::collections::HashMap<u64, NodeId>,
    used: std::collections::HashSet<NodeId>,
    next: u64,
    active: std::collections::HashMap<NodeId, rndr_core::Entity>,
    text: super::a11y_text::PublishedText,
    scrollbars: std::collections::HashMap<NodeId, (rndr_core::Entity, rndr_core::Axis)>,
}
impl Bridge {
    fn allocate(&mut self) -> NodeId {
        if self.next == 0 {
            self.next = ACCESS_ROOT - 1;
        }
        loop {
            let id = NodeId(self.next);
            self.next = self
                .next
                .checked_sub(1)
                .expect("accessibility ID space exhausted");
            if id.0 != 0 && self.used.insert(id) {
                return id;
            }
        }
    }
    fn id(&mut self, raw: u64) -> NodeId {
        if raw == ACCESS_ROOT {
            return NodeId(ACCESS_ROOT);
        }
        if let Some(id) = self.ids.get(&raw) {
            return *id;
        }
        let preferred = NodeId(raw);
        let id = if self.used.insert(preferred) {
            preferred
        } else {
            self.allocate()
        };
        self.ids.insert(raw, id);
        id
    }
    pub(crate) fn update(&mut self, world: &World) -> TreeUpdate {
        let tree = access_tree(world);
        self.active.clear();
        self.scrollbars.clear();
        for node in &tree.nodes {
            let id = self.id(node.id);
            if let Some(entity) = entity_from_access_id(node.id) {
                self.active.insert(id, entity);
                if let Some(bar) = world.get::<rndr_core::Scrollbar>(entity) {
                    self.scrollbars.insert(id, (bar.target, bar.axis));
                }
            }
        }
        let mut published = super::a11y_text::PublishedText::default();
        let mut focus = NodeId(ACCESS_ROOT);
        let mut extra = Vec::new();
        let mut nodes: Vec<_> = tree
            .nodes
            .into_iter()
            .map(|node| {
                if node.focused {
                    focus = self.id(node.id);
                }
                let id = self.id(node.id);
                let mut out = platform_node(world, &node);
                // Remap every relationship, not just tree children.
                out.set_children(
                    node.children
                        .iter()
                        .map(|id| self.id(*id))
                        .collect::<Vec<_>>(),
                );
                out.set_controls(
                    out.controls()
                        .iter()
                        .map(|id| self.id(id.0))
                        .collect::<Vec<_>>(),
                );
                out.set_labelled_by(
                    out.labelled_by()
                        .iter()
                        .map(|id| self.id(id.0))
                        .collect::<Vec<_>>(),
                );
                out.set_described_by(
                    out.described_by()
                        .iter()
                        .map(|id| self.id(id.0))
                        .collect::<Vec<_>>(),
                );
                if let Some(snapshot) =
                    entity_from_access_id(node.id).and_then(|e| rndr_core::access_text(world, e))
                {
                    let old = self.text.editors.remove(&id);
                    if let Some(mut text) =
                        super::a11y_text::publish(id, snapshot, old.as_ref(), || self.allocate())
                    {
                        out.set_role(Role::MultilineTextInput);
                        out.clear_value();
                        out.set_clips_children();
                        if text.snapshot.readonly {
                            out.set_read_only();
                            out.remove_action(Action::SetValue);
                        }
                        if text.snapshot.selectable {
                            out.add_action(Action::SetTextSelection);
                        }
                        out.set_children(text.runs.iter().map(|r| r.id).collect::<Vec<_>>());
                        if let Some(selection) = text.selection() {
                            out.set_text_selection(selection);
                        }
                        extra.append(&mut text.nodes);
                        published.editors.insert(id, text);
                    }
                }
                (id, out)
            })
            .collect();
        nodes.extend(extra);
        self.text = published;
        TreeUpdate {
            nodes,
            tree: Some(TreeInfo::new(NodeId(tree.root))),
            tree_id: TreeId::ROOT,
            focus,
        }
    }

    pub(crate) fn command(
        &self,
        world: &World,
        request: &accesskit::ActionRequest,
    ) -> Option<AccessCommand> {
        if request.target_tree != TreeId::ROOT {
            return None;
        }
        let entity = *self.active.get(&request.target_node)?;
        if request.action == Action::SetTextSelection {
            return self.text.command(world, entity, request);
        }
        if world.has::<rndr_core::Scrollbar>(entity)
            && !self.scrollbars.contains_key(&request.target_node)
        {
            return None;
        }
        if let Some((target, axis)) = self.scrollbars.get(&request.target_node) {
            let bar = world.get::<rndr_core::Scrollbar>(entity)?;
            if (bar.target, bar.axis) != (*target, *axis)
                || rndr_core::scrollbar_geometry(world, entity).is_none()
            {
                return None;
            }
        }
        match request.action {
            Action::Increment | Action::Decrement if world.has::<rndr_core::Scrollbar>(entity) => {
                let g = rndr_core::scrollbar_geometry(world, entity)?;
                let value = (g.value
                    + if request.action == Action::Increment {
                        40.
                    } else {
                        -40.
                    })
                .clamp(0., g.max);
                Some(AccessCommand::SetValue {
                    entity,
                    value: value.to_string(),
                })
            }
            Action::Increment | Action::Decrement
                if world.has::<rndr_core::NumericAccess>(entity) =>
            {
                if !rndr_core::access_details(world, entity).set_value {
                    return None;
                }
                let access = world.get::<rndr_core::NumericAccess>(entity).copied()?;
                let delta = if request.action == Action::Increment {
                    access.step
                } else {
                    -access.step
                };
                let next = access.normalize(access.value + delta)?;
                Some(AccessCommand::SetValue {
                    entity,
                    value: rndr_core::NumericAccess::text(next),
                })
            }
            Action::Focus => Some(AccessCommand::Focus(entity)),
            Action::Click => Some(AccessCommand::Activate(entity)),
            Action::SetValue => {
                let value = match request.data.as_ref() {
                    Some(ActionData::Value(value)) => value.to_string(),
                    Some(ActionData::NumericValue(value))
                        if world.has::<rndr_core::Scrollbar>(entity) =>
                    {
                        value.to_string()
                    }
                    Some(ActionData::NumericValue(value))
                        if world.has::<rndr_core::NumericAccess>(entity) =>
                    {
                        rndr_core::NumericAccess::text(*value)
                    }
                    _ => return None,
                };
                Some(AccessCommand::SetValue { entity, value })
            }
            _ => None,
        }
    }
}

#[cfg(test)]
fn tree_update(world: &World) -> TreeUpdate {
    Bridge::default().update(world)
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
    if matches!(
        node.role,
        AccessRole::TextInput | AccessRole::Code | AccessRole::SpinButton
    ) && !node.value.is_empty()
    {
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
    let mut described: Vec<NodeId> = entity_from_access_id(node.id)
        .and_then(|e| world.get::<rndr_core::DataAccess>(e))
        .map(|data| {
            data.headers
                .iter()
                .filter(|e| world.is_alive(**e))
                .map(|e| NodeId(rndr_core::access_id(*e)))
                .collect()
        })
        .unwrap_or_default();
    if let Some(by) =
        entity_from_access_id(node.id).and_then(|e| world.get::<rndr_core::DescribedBy>(e))
    {
        described.extend(
            by.0.iter()
                .filter(|e| world.is_alive(**e) && !world.has::<rndr_core::Hidden>(**e))
                .map(|e| NodeId(rndr_core::access_id(*e))),
        );
    }
    if !described.is_empty() {
        out.set_described_by(described);
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
    if let Some(entity) =
        entity_from_access_id(node.id).filter(|e| world.has::<rndr_core::Scrollbar>(*e))
    {
        if let Some(g) = rndr_core::scrollbar_geometry(world, entity) {
            let bar = world.get::<rndr_core::Scrollbar>(entity).unwrap();
            out.set_numeric_value(f64::from(g.value));
            out.set_min_numeric_value(0.);
            out.set_max_numeric_value(f64::from(g.max));
            out.set_numeric_value_step(40.);
            out.set_numeric_value_jump(f64::from(g.page));
            out.set_orientation(if bar.axis == rndr_core::Axis::Row {
                accesskit::Orientation::Horizontal
            } else {
                accesskit::Orientation::Vertical
            });
            out.set_controls(vec![NodeId(rndr_core::access_id(bar.target))]);
            out.add_action(Action::SetValue);
            out.add_action(Action::Increment);
            out.add_action(Action::Decrement);
        } else {
            out.set_disabled();
            out.remove_action(Action::Focus);
        }
    }
    if let Some(access) = entity_from_access_id(node.id)
        .and_then(|entity| world.get::<rndr_core::NumericAccess>(entity))
        .copied()
    {
        if let Some(value) = access.normalize(access.value) {
            out.set_numeric_value(value);
        }
        out.set_min_numeric_value(access.min.min(access.max));
        out.set_max_numeric_value(access.min.max(access.max));
        if access.step.is_finite() && access.step > 0.0 {
            out.set_numeric_value_step(access.step);
        }
        if access.jump.is_finite() && access.jump > 0.0 {
            out.set_numeric_value_jump(access.jump);
        }
        out.set_orientation(if access.horizontal {
            accesskit::Orientation::Horizontal
        } else {
            accesskit::Orientation::Vertical
        });
        if details.set_value {
            out.add_action(Action::SetValue);
            out.add_action(Action::Increment);
            out.add_action(Action::Decrement);
        }
    }
    if let Some(live) =
        entity_from_access_id(node.id).and_then(|entity| world.get::<rndr_core::LiveRegion>(entity))
    {
        out.set_live(match live.0 {
            rndr_core::LivePoliteness::Polite => Live::Polite,
            rndr_core::LivePoliteness::Assertive => Live::Assertive,
        });
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
        AccessRole::ScrollBar => Role::ScrollBar,
        AccessRole::Code => Role::Code,
        AccessRole::Tooltip => Role::Tooltip,
        AccessRole::Image => Role::Image,
        AccessRole::Slider => Role::Slider,
        AccessRole::SpinButton => Role::SpinButton,
        AccessRole::Status => Role::Status,
    }
}

#[cfg(test)]
mod scrollbar_tests {
    use super::*;
    use rndr_core::*;
    #[test]
    fn native_scrollbar_range_actions_and_retarget_fence() {
        let mut w = World::new();
        w.insert_resource(Viewport::new(400., 300.));
        let port = w.spawn((
            Layout {
                width: Size::Px(100.),
                height: Size::Px(100.),
                ..Layout::column()
            },
            Scroll::default(),
        ));
        let content = w.spawn((Layout {
            width: Size::Px(100.),
            height: Size::Px(500.),
            ..Layout::column()
        },));
        set_parent(&mut w, content, port);
        let bar = w.spawn((
            Layout {
                width: Size::Px(24.),
                height: Size::Px(100.),
                ..Layout::column()
            },
            Focusable,
            Scrollbar {
                target: port,
                axis: Axis::Column,
            },
        ));
        layout_propagate(&mut w);
        let mut bridge = Bridge::default();
        let tree = bridge.update(&w);
        let id = bridge.id(access_id(bar));
        let node = &tree.nodes.iter().find(|(n, _)| *n == id).unwrap().1;
        assert_eq!(node.role(), Role::ScrollBar);
        assert_eq!(node.numeric_value(), Some(0.));
        assert_eq!(node.max_numeric_value(), Some(400.));
        assert_eq!(node.orientation(), Some(accesskit::Orientation::Vertical));
        assert!(node.supports_action(Action::SetValue));
        assert!(node.supports_action(Action::Increment));
        assert!(node.supports_action(Action::Decrement));
        let request = accesskit::ActionRequest {
            action: Action::SetValue,
            target_tree: TreeId::ROOT,
            target_node: id,
            data: Some(ActionData::NumericValue(120.)),
        };
        let command = bridge.command(&w, &request).unwrap();
        assert_eq!(
            try_apply_access(&mut w, command),
            AccessCommandResult::Applied
        );
        assert_eq!(w.get::<Scroll>(port).unwrap().offset[1], -120.);
        let mut increment = request.clone();
        increment.action = Action::Increment;
        increment.data = None;
        let command = bridge.command(&w, &increment).unwrap();
        assert_eq!(
            try_apply_access(&mut w, command),
            AccessCommandResult::Applied
        );
        assert_eq!(w.get::<Scroll>(port).unwrap().offset[1], -160.);
        let other = w.spawn((
            Layout {
                width: Size::Px(100.),
                height: Size::Px(100.),
                ..Layout::column()
            },
            Scroll::default(),
        ));
        w.get_mut::<Scrollbar>(bar).unwrap().target = other;
        assert!(
            bridge.command(&w, &request).is_none(),
            "queued action must not acquire a retargeted viewport"
        );
        w.get_mut::<Scrollbar>(bar).unwrap().target = port;
        w.despawn_entity(port);
        assert!(bridge.command(&w, &request).is_none());
        let tree = bridge.update(&w);
        let node = &tree.nodes.iter().find(|(n, _)| *n == id).unwrap().1;
        assert!(node.is_disabled());
        assert!(!node.supports_action(Action::SetValue));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rndr_core::*;

    #[test]
    fn synthetic_ids_and_future_entity_ids_share_one_non_recycled_namespace() {
        let mut bridge = Bridge::default();
        let synthetic = bridge.allocate();
        assert_ne!(synthetic, NodeId(ACCESS_ROOT));
        let entity_id = bridge.id(synthetic.0);
        assert_ne!(
            entity_id, synthetic,
            "future entity must not alias an old text run"
        );
        assert_eq!(bridge.id(synthetic.0), entity_id);
        let another = bridge.allocate();
        assert_ne!(another, synthetic);
        assert_ne!(another, entity_id);
        assert_eq!(bridge.id(ACCESS_ROOT), NodeId(ACCESS_ROOT));
    }
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

    #[test]
    fn code_value_and_tooltip_described_by_reach_accesskit() {
        use rndr_core::*;
        let mut w = World::new();
        let plot = w.spawn((
            Layout::column(),
            AccessMark(AccessRole::Group),
            Semantics {
                name: Some("Weekly".into()),
                ..Semantics::default()
            },
        ));
        let tip = w.spawn((
            Label::new("Mon — Visits: 3"),
            AccessMark(AccessRole::Tooltip),
        ));
        set_parent(&mut w, tip, plot);
        w.insert(plot, DescribedBy(vec![tip]));
        let code = w.spawn((
            Label::new("fn main() {}"),
            AccessMark(AccessRole::Code),
            Semantics {
                name: Some("rust".into()),
                ..Semantics::default()
            },
        ));
        let t = tree_update(&w);
        let native = |e| {
            &t.nodes
                .iter()
                .find(|(id, _)| *id == NodeId(access_id(e)))
                .unwrap()
                .1
        };
        assert_eq!(native(plot).role(), Role::Group);
        assert_eq!(native(plot).label(), Some("Weekly"));
        assert_eq!(native(plot).described_by(), &[NodeId(access_id(tip))]);
        assert_eq!(native(tip).role(), Role::Tooltip);
        assert_eq!(native(code).role(), Role::Code);
        assert_eq!(native(code).label(), Some("rust"));
        assert_eq!(native(code).value(), Some("fn main() {}"));
    }

    #[test]
    fn slider_spin_button_and_status_reach_accesskit() {
        let mut app = App::new();
        let slider = app.world_mut().spawn((
            Layout::column(),
            Focusable,
            AccessMark(AccessRole::Slider),
            NumericAccess {
                value: 40.,
                min: 0.,
                max: 100.,
                step: 1.,
                jump: 10.,
                horizontal: true,
            },
            Semantics {
                name: Some("Volume".into()),
                ..Semantics::default()
            },
        ));
        let spin = app.world_mut().spawn((
            Layout::column(),
            Label::new("4"),
            TextField,
            Focusable,
            AccessMark(AccessRole::SpinButton),
            NumericAccess {
                value: 4.,
                min: 0.,
                max: 8.,
                step: 1.,
                jump: 2.,
                horizontal: true,
            },
        ));
        app.world_mut().insert(
            spin,
            Semantics {
                name: Some("Count".into()),
                readonly: true,
                ..Semantics::default()
            },
        );
        let status = app.world_mut().spawn((
            Label::new("Saved"),
            AccessMark(AccessRole::Status),
            LiveRegion(LivePoliteness::Polite),
            Semantics {
                name: Some("Saved".into()),
                ..Semantics::default()
            },
        ));
        let hidden_tip = app.world_mut().spawn((
            Label::new("hidden"),
            AccessMark(AccessRole::Tooltip),
            Hidden,
        ));
        app.world_mut()
            .insert(slider, DescribedBy(vec![hidden_tip]));
        let mut bridge = Bridge::default();
        let tree = bridge.update(app.world());
        let native = |entity| {
            tree.nodes
                .iter()
                .find(|(id, _)| *id == NodeId(access_id(entity)))
                .map(|(_, node)| node)
        };
        let slider_node = native(slider).unwrap();
        assert_eq!(slider_node.role(), Role::Slider);
        assert_eq!(slider_node.label(), Some("Volume"));
        assert_eq!(slider_node.numeric_value(), Some(40.));
        assert_eq!(slider_node.min_numeric_value(), Some(0.));
        assert_eq!(slider_node.max_numeric_value(), Some(100.));
        assert_eq!(slider_node.numeric_value_step(), Some(1.));
        assert_eq!(slider_node.numeric_value_jump(), Some(10.));
        assert_eq!(
            slider_node.orientation(),
            Some(accesskit::Orientation::Horizontal)
        );
        assert!(slider_node.supports_action(Action::SetValue));
        assert!(slider_node.supports_action(Action::Increment));
        assert!(slider_node.described_by().is_empty());
        let spin_node = native(spin).unwrap();
        assert_eq!(spin_node.role(), Role::SpinButton);
        assert_eq!(spin_node.value(), Some("4"));
        assert_eq!(spin_node.numeric_value(), Some(4.));
        assert!(spin_node.is_read_only());
        assert!(!spin_node.supports_action(Action::SetValue));
        let status_node = native(status).unwrap();
        assert_eq!(status_node.role(), Role::Status);
        assert_eq!(status_node.live(), Some(accesskit::Live::Polite));
        assert!(native(hidden_tip).is_none());
        let request = accesskit::ActionRequest {
            action: Action::Increment,
            target_tree: TreeId::ROOT,
            target_node: NodeId(access_id(slider)),
            data: None,
        };
        let command = bridge.command(app.world(), &request).unwrap();
        assert_eq!(
            try_apply_access(app.world_mut(), command),
            AccessCommandResult::Applied
        );
        assert_eq!(app.world().get::<NumericAccess>(slider).unwrap().value, 40.);
        app.update();
        assert_eq!(
            events::<NumericAccessSet>(app.world()),
            &[NumericAccessSet {
                entity: slider,
                value: 41.,
            }]
        );
    }
}
