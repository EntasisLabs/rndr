use rndr_core::*;
use rndr_ui::collections::Item;
use rndr_ui::{collections::*, controls, *};

fn compose(w: &mut World, slot: RowSlot, item: &Item) {
    let children = children_of(w, slot.content);
    if children.is_empty() {
        text(w, slot.content, &item.name, Typography::Label).unwrap();
        text(w, slot.content, "composed metadata", Typography::Secondary).unwrap();
    } else {
        w.get_mut::<Label>(children[0]).unwrap().text = item.name.clone();
    }
}
fn setup() -> (App, Entity) {
    let mut app = App::new();
    app.add_plugin(UiPlugin);
    app.add_system_front(Schedule::Input, apply_scroll);
    app.world_mut().insert_resource(Viewport::new(500.0, 500.0));
    let root = app.world_mut().spawn((Layout {
        width: Size::Fill,
        height: Size::Fill,
        ..Layout::column()
    },));
    (app, root)
}
fn items(n: u64) -> Vec<Item> {
    (1..=n).map(|i| Item::new(i, format!("Item {i}"))).collect()
}
fn list(app: &mut App, root: Entity, data: &[Item]) -> ViewHandle {
    ListView::mount(
        app.world_mut(),
        root,
        UiId(10),
        ViewProps::new("Test list"),
        data,
        compose,
    )
    .unwrap()
}
fn tree(app: &mut App, root: Entity) -> ViewHandle {
    let nodes = vec![
        TreeNode {
            item: Item::new(1, "Root"),
            parent: None,
        },
        TreeNode {
            item: Item::new(2, "Branch"),
            parent: Some(ItemId(1)),
        },
        TreeNode {
            item: Item::new(3, "Leaf"),
            parent: Some(ItemId(2)),
        },
        TreeNode {
            item: Item::new(4, "Sibling"),
            parent: Some(ItemId(1)),
        },
        TreeNode {
            item: Item::new(5, "Other root"),
            parent: None,
        },
    ];
    TreeView::mount(
        app.world_mut(),
        root,
        UiId(20),
        ViewProps::new("Test tree"),
        &nodes,
        compose,
    )
    .unwrap()
}
fn frame(app: &mut App) {
    assert!(!rndr_paint::frame(app).quads.is_empty());
}
fn key(app: &mut App, key: NamedKey) {
    app.world_mut().resource_mut::<Input>().unwrap().keys =
        vec![KeyEvent::named(key, Modifiers::default())];
    frame(app);
}
fn click(app: &mut App, entity: Entity) {
    let r = *app.world().get::<Rect>(entity).unwrap();
    let p = [r.x + r.w * 0.5, r.y + r.h * 0.5];
    app.world_mut().resource_mut::<Input>().unwrap().pointer = Pointer {
        position: p,
        press_at: Some(p),
        pressed: true,
        down: true,
        ..Pointer::default()
    };
    frame(app);
    app.world_mut().resource_mut::<Input>().unwrap().pointer = Pointer {
        position: p,
        released: true,
        ..Pointer::default()
    };
    frame(app);
    app.world_mut().resource_mut::<Input>().unwrap().pointer = Pointer::default();
}
fn kinds(app: &App, view: ViewHandle) -> Vec<CollectionEventKind> {
    events::<CollectionEvent>(app.world())
        .iter()
        .filter(|e| e.view == view.root)
        .map(|e| e.kind)
        .collect()
}
#[test]
fn stable_identity_and_composed_subtrees_survive_reorder_updates_and_reconciliation() {
    let (mut app, root) = setup();
    let data = items(4);
    let h = list(&mut app, root, &data);
    frame(&mut app);
    let before = h.row(app.world(), ItemId(2)).unwrap();
    let content = children_of(app.world(), before.content);
    h.select(app.world_mut(), Some(ItemId(2))).unwrap();
    h.focus(app.world_mut(), ItemId(3)).unwrap();
    frame(&mut app);
    let mut reordered = data.clone();
    reordered.reverse();
    reordered[2].name = "Renamed".into();
    assert_eq!(h, list(&mut app, root, &reordered));
    frame(&mut app);
    assert_eq!(h.row(app.world(), ItemId(2)).unwrap().root, before.root);
    assert_eq!(children_of(app.world(), before.content), content);
    assert_eq!(
        app.world().get::<Label>(content[0]).unwrap().text,
        "Renamed"
    );
    assert_eq!(h.selected(app.world()), Some(ItemId(2)));
    assert_eq!(h.focused(app.world()), Some(ItemId(3)));
    let filtered: Vec<_> = reordered
        .into_iter()
        .filter(|i| i.id != ItemId(2) && i.id != ItemId(3))
        .collect();
    list(&mut app, root, &filtered);
    frame(&mut app);
    assert_eq!(h.selected(app.world()), None);
    assert_eq!(h.focused(app.world()), Some(ItemId(1)));
    assert!(!app.world().is_alive(before.root));
    assert!(kinds(&app, h).contains(&CollectionEventKind::Selected(None)));
    list(&mut app, root, &data);
    frame(&mut app);
    assert_ne!(h.row(app.world(), ItemId(2)).unwrap().root, before.root);
    assert_eq!(h.selected(app.world()), None);
}
#[test]
fn keyboard_focus_space_selection_and_enter_activation_are_distinct() {
    let (mut app, root) = setup();
    let h = list(&mut app, root, &items(4));
    frame(&mut app);
    h.focus(app.world_mut(), ItemId(1)).unwrap();
    key(&mut app, NamedKey::ArrowDown);
    assert_eq!(h.focused(app.world()), Some(ItemId(2)));
    assert_eq!(h.selected(app.world()), None);
    key(&mut app, NamedKey::Enter);
    assert!(kinds(&app, h).contains(&CollectionEventKind::Activated(ItemId(2))));
    assert_eq!(h.selected(app.world()), None);
    key(&mut app, NamedKey::Space);
    assert_eq!(h.selected(app.world()), None);
    let mut release = KeyEvent::named(NamedKey::Space, Modifiers::default());
    release.released = true;
    app.world_mut().resource_mut::<Input>().unwrap().keys = vec![release];
    frame(&mut app);
    assert_eq!(h.selected(app.world()), Some(ItemId(2)));
    key(&mut app, NamedKey::End);
    assert_eq!(h.focused(app.world()), Some(ItemId(4)));
    key(&mut app, NamedKey::Home);
    assert_eq!(h.focused(app.world()), Some(ItemId(1)));
    assert_eq!(h.selected(app.world()), Some(ItemId(2)));
}
#[test]
fn chronological_tab_navigation_and_enter_share_one_dispatcher() {
    let (mut app, root) = setup();
    let before = controls::Button::mount(
        app.world_mut(),
        root,
        UiId(1),
        controls::ButtonProps::new("Before", controls::HostIntent::Command(1)),
    )
    .unwrap();
    let h = list(&mut app, root, &items(4));
    let after = controls::Button::mount(
        app.world_mut(),
        root,
        UiId(2),
        controls::ButtonProps::new("After", controls::HostIntent::Command(2)),
    )
    .unwrap();
    frame(&mut app);
    request_focus(app.world_mut(), before.root);
    app.world_mut().resource_mut::<Input>().unwrap().keys =
        [NamedKey::Tab, NamedKey::ArrowDown, NamedKey::Enter]
            .into_iter()
            .map(|k| KeyEvent::named(k, Modifiers::default()))
            .collect();
    frame(&mut app);
    assert_eq!(h.focused(app.world()), Some(ItemId(2)));
    assert!(kinds(&app, h).contains(&CollectionEventKind::Activated(ItemId(2))));
    key(&mut app, NamedKey::Tab);
    assert_eq!(
        app.world().resource::<Focus>().unwrap().entity,
        Some(after.root)
    );
    let mut shift = Modifiers::default();
    shift.shift = true;
    app.world_mut().resource_mut::<Input>().unwrap().keys =
        vec![KeyEvent::named(NamedKey::Tab, shift)];
    frame(&mut app);
    assert_eq!(h.focused(app.world()), Some(ItemId(2)));
    let mut repeat = KeyEvent::named(NamedKey::Enter, Modifiers::default());
    repeat.repeat = true;
    app.world_mut().resource_mut::<Input>().unwrap().keys = vec![repeat];
    frame(&mut app);
    assert!(!kinds(&app, h)
        .iter()
        .any(|e| matches!(e, CollectionEventKind::Activated(_))));
}
#[test]
fn pointer_release_selects_and_drag_escape_blur_cancel() {
    let (mut app, root) = setup();
    let h = list(&mut app, root, &items(4));
    frame(&mut app);
    let row = h.row(app.world(), ItemId(2)).unwrap();
    let label = children_of(app.world(), row.content)[0];
    click(&mut app, label);
    assert_eq!(h.focused(app.world()), Some(ItemId(2)));
    assert_eq!(h.selected(app.world()), Some(ItemId(2)));
    assert!(!kinds(&app, h)
        .iter()
        .any(|e| matches!(e, CollectionEventKind::Activated(_))));
    let r = *app
        .world()
        .get::<Rect>(h.row(app.world(), ItemId(3)).unwrap().root)
        .unwrap();
    let p = [r.x + 10.0, r.y + 10.0];
    for cancel in [NamedKey::Escape, NamedKey::ArrowDown] {
        app.world_mut().resource_mut::<Input>().unwrap().pointer = Pointer {
            position: p,
            press_at: Some(p),
            pressed: true,
            down: true,
            ..Pointer::default()
        };
        frame(&mut app);
        app.world_mut()
            .resource_mut::<Input>()
            .unwrap()
            .pointer
            .pressed = false;
        key(&mut app, cancel);
        app.world_mut().resource_mut::<Input>().unwrap().pointer = Pointer {
            position: p,
            released: true,
            ..Pointer::default()
        };
        frame(&mut app);
        assert_eq!(h.selected(app.world()), Some(ItemId(2)));
    }
    key(&mut app, NamedKey::Space);
    send(app.world_mut(), WindowFocus::Blurred);
    frame(&mut app);
    let mut release = KeyEvent::named(NamedKey::Space, Modifiers::default());
    release.released = true;
    app.world_mut().resource_mut::<Input>().unwrap().keys = vec![release];
    frame(&mut app);
    assert_eq!(h.selected(app.world()), Some(ItemId(2)));
}
#[test]
fn nested_tree_navigation_collapse_and_removal_have_safe_focus() {
    let (mut app, root) = setup();
    let h = tree(&mut app, root);
    frame(&mut app);
    h.focus(app.world_mut(), ItemId(1)).unwrap();
    key(&mut app, NamedKey::ArrowRight);
    assert!(h.expanded(app.world(), ItemId(1)));
    assert_eq!(h.focused(app.world()), Some(ItemId(1)));
    key(&mut app, NamedKey::ArrowRight);
    assert_eq!(h.focused(app.world()), Some(ItemId(2)));
    key(&mut app, NamedKey::ArrowRight);
    key(&mut app, NamedKey::ArrowRight);
    assert_eq!(h.focused(app.world()), Some(ItemId(3)));
    h.select(app.world_mut(), Some(ItemId(3))).unwrap();
    key(&mut app, NamedKey::ArrowLeft);
    assert_eq!(h.focused(app.world()), Some(ItemId(2)));
    key(&mut app, NamedKey::ArrowLeft);
    assert!(!h.expanded(app.world(), ItemId(2)));
    assert_eq!(h.selected(app.world()), Some(ItemId(3)));
    assert!(!access_tree(app.world())
        .nodes
        .iter()
        .any(|n| n.id == access_id(h.row(app.world(), ItemId(3)).unwrap().root)));
    h.set_expanded(app.world_mut(), ItemId(2), true).unwrap();
    h.focus(app.world_mut(), ItemId(3)).unwrap();
    h.set_expanded(app.world_mut(), ItemId(1), false).unwrap();
    frame(&mut app);
    assert_eq!(h.focused(app.world()), Some(ItemId(1)));
    assert_eq!(h.selected(app.world()), Some(ItemId(3)));
    key(&mut app, NamedKey::End);
    assert_eq!(h.focused(app.world()), Some(ItemId(5)));
    key(&mut app, NamedKey::ArrowUp);
    assert_eq!(h.focused(app.world()), Some(ItemId(1)));
    let remaining = vec![TreeNode {
        item: Item::new(5, "Other root"),
        parent: None,
    }];
    TreeView::mount(
        app.world_mut(),
        root,
        UiId(20),
        ViewProps::new("Test tree"),
        &remaining,
        compose,
    )
    .unwrap();
    frame(&mut app);
    assert_eq!(h.focused(app.world()), Some(ItemId(5)));
    assert_eq!(h.selected(app.world()), None);
}
#[test]
fn surviving_branch_becoming_leaf_retires_disclosure_and_indent_selects_row() {
    let (mut app, root) = setup();
    let h = tree(&mut app, root);
    h.set_expanded(app.world_mut(), ItemId(1), true).unwrap();
    h.set_expanded(app.world_mut(), ItemId(2), true).unwrap();
    h.focus(app.world_mut(), ItemId(2)).unwrap();
    frame(&mut app);
    let slot = h.row(app.world(), ItemId(2)).unwrap();
    let host = controls::Button::mount(
        app.world_mut(),
        root,
        UiId(99),
        controls::ButtonProps::new("Host", controls::HostIntent::Command(99)),
    )
    .unwrap();
    try_activate(app.world_mut(), host.root);
    let nodes = [
        TreeNode {
            item: Item::new(1, "Root"),
            parent: None,
        },
        TreeNode {
            item: Item::new(2, "Now a leaf"),
            parent: Some(ItemId(1)),
        },
        TreeNode {
            item: Item::new(5, "Other"),
            parent: None,
        },
    ];
    TreeView::mount(
        app.world_mut(),
        root,
        UiId(20),
        ViewProps::new("Tree"),
        &nodes,
        compose,
    )
    .unwrap();
    frame(&mut app);
    assert_eq!(h.row(app.world(), ItemId(2)).unwrap().root, slot.root);
    assert!(!h.expanded(app.world(), ItemId(2)));
    assert!(kinds(&app, h).contains(&CollectionEventKind::Expanded(ItemId(2), false)));
    let host_requests = take_action_intents(app.world_mut());
    assert_eq!(host_requests.len(), 1);
    assert_eq!(host_requests[0].entity, host.root);
    key(&mut app, NamedKey::ArrowLeft);
    assert_eq!(h.focused(app.world()), Some(ItemId(1)));
    let indent = children_of(app.world(), children_of(app.world(), slot.root)[0])[0];
    assert!(!app.world().has::<Button>(indent));
    click(&mut app, indent);
    assert_eq!(h.focused(app.world()), Some(ItemId(2)));
    assert_eq!(h.selected(app.world()), Some(ItemId(2)));
    assert!(h.set_expanded(app.world_mut(), ItemId(2), true).is_err());
}
#[test]
fn pointer_disclosure_and_accessibility_use_the_same_canonical_targets() {
    let (mut app, root) = setup();
    let h = tree(&mut app, root);
    frame(&mut app);
    let row = h.row(app.world(), ItemId(1)).unwrap();
    let inner = children_of(app.world(), row.root)[0];
    let toggle = children_of(app.world(), inner)[0];
    click(&mut app, toggle);
    assert!(h.expanded(app.world(), ItemId(1)));
    assert_eq!(h.selected(app.world()), None);
    let t = access_tree(app.world());
    assert_eq!(
        t.nodes
            .iter()
            .find(|n| n.id == access_id(h.root))
            .unwrap()
            .role,
        AccessRole::Tree
    );
    let branch = t
        .nodes
        .iter()
        .find(|n| n.id == access_id(row.root))
        .unwrap();
    assert_eq!(branch.role, AccessRole::TreeItem);
    assert_eq!(branch.expanded, Some(true));
    assert_eq!(branch.name, "Root");
    assert_eq!(
        try_apply_access(app.world_mut(), AccessCommand::Activate(row.root)),
        AccessCommandResult::Activation(ActivationResult::Accepted)
    );
    frame(&mut app);
    assert_eq!(h.selected(app.world()), Some(ItemId(1)));
    assert_eq!(
        try_apply_access(app.world_mut(), AccessCommand::Activate(toggle)),
        AccessCommandResult::Activation(ActivationResult::Accepted)
    );
    frame(&mut app);
    assert!(!h.expanded(app.world(), ItemId(1)));
    let after = controls::Button::mount(
        app.world_mut(),
        root,
        UiId(99),
        controls::ButtonProps::new("After tree", controls::HostIntent::Command(99)),
    )
    .unwrap();
    frame(&mut app);
    assert_eq!(
        try_apply_access(app.world_mut(), AccessCommand::Focus(toggle)),
        AccessCommandResult::Applied
    );
    key(&mut app, NamedKey::Tab);
    assert_eq!(
        app.world().resource::<Focus>().unwrap().entity,
        Some(after.root)
    );
}
#[test]
fn scroll_reveal_clipping_empty_disabled_and_none_selection_are_integrated() {
    let (mut app, root) = setup();
    let mut data = items(40);
    data[1].enabled = false;
    let h = list(&mut app, root, &data);
    app.world_mut().get_mut::<Layout>(h.root).unwrap().height = Size::Px(150.0);
    frame(&mut app);
    assert!(h.focus(app.world_mut(), ItemId(2)).is_err());
    h.focus(app.world_mut(), ItemId(1)).unwrap();
    key(&mut app, NamedKey::ArrowDown);
    assert_eq!(h.focused(app.world()), Some(ItemId(3)));
    key(&mut app, NamedKey::End);
    frame(&mut app);
    assert!(app.world().get::<Scroll>(h.root).unwrap().offset[1] < -100.0);
    let row = h.row(app.world(), ItemId(40)).unwrap();
    let rect = app.world().get::<Rect>(row.root).unwrap();
    let port = app.world().get::<Rect>(h.root).unwrap();
    assert!(rect.y >= port.y - 1.0 && rect.y + rect.h <= port.y + port.h + 1.0);
    let selected = h.selected(app.world());
    h.reveal(app.world_mut(), ItemId(1)).unwrap();
    // Reveal itself does not focus, and does not select. Subsequent normal focus
    // reveal will of course bring the focused row back in the following frame.
    assert_eq!(h.focused(app.world()), Some(ItemId(40)));
    assert_eq!(h.selected(app.world()), selected);
    request_focus(app.world_mut(), root);
    frame(&mut app);
    let port = *app.world().get::<Rect>(h.root).unwrap();
    app.world_mut()
        .resource_mut::<Input>()
        .unwrap()
        .pointer
        .position = [port.x + 10.0, port.y + 10.0];
    app.world_mut().resource_mut::<Input>().unwrap().scroll = [0.0, -100.0];
    frame(&mut app);
    assert!(app.world().get::<Scroll>(h.root).unwrap().offset[1] < 0.0);
    let mut props = ViewProps::new("Test list");
    props.enabled = false;
    ListView::mount(
        app.world_mut(),
        root,
        UiId(10),
        props.clone(),
        &data,
        compose,
    )
    .unwrap();
    frame(&mut app);
    assert!(h.select(app.world_mut(), Some(ItemId(1))).is_err());
    assert!(h.focus(app.world_mut(), ItemId(1)).is_err());
    assert!(access_tree(app.world())
        .nodes
        .iter()
        .filter(|n| n.role == AccessRole::ListItem)
        .all(|n| !n.enabled && !n.focusable));
    props.enabled = true;
    props.selection = SelectionMode::None;
    ListView::mount(
        app.world_mut(),
        root,
        UiId(10),
        props.clone(),
        &data,
        compose,
    )
    .unwrap();
    frame(&mut app);
    h.focus(app.world_mut(), ItemId(1)).unwrap();
    key(&mut app, NamedKey::Enter);
    assert!(kinds(&app, h).contains(&CollectionEventKind::Activated(ItemId(1))));
    assert_eq!(h.selected(app.world()), None);
    ListView::mount(app.world_mut(), root, UiId(10), props, &[], compose).unwrap();
    frame(&mut app);
    assert_eq!(
        app.world().resource::<Focus>().unwrap().entity,
        Some(h.root)
    );
    assert!(access_tree(app.world())
        .nodes
        .iter()
        .any(|n| n.name.contains("No items")));
    assert_eq!(app.world().get::<Scroll>(h.root).unwrap().offset[1], 0.0);
}
#[test]
fn malformed_or_oversize_inputs_reject_atomically_and_lifetimes_are_generation_checked() {
    let (mut app, root) = setup();
    let h = list(&mut app, root, &items(2));
    frame(&mut app);
    let entity = h.row(app.world(), ItemId(1)).unwrap().root;
    for data in [
        vec![Item::new(1, "one"), Item::new(1, "dup")],
        items(MAX_ITEMS as u64 + 1),
    ] {
        assert_eq!(
            ListView::mount(
                app.world_mut(),
                root,
                UiId(10),
                ViewProps::new("List"),
                &data,
                compose
            ),
            Err(UiError::InvalidProps)
        );
        assert_eq!(h.row(app.world(), ItemId(1)).unwrap().root, entity);
    }
    let bad = vec![TreeNode {
        item: Item::new(1, "one"),
        parent: Some(ItemId(9)),
    }];
    assert_eq!(
        TreeView::mount(
            app.world_mut(),
            root,
            UiId(99),
            ViewProps::new("Tree"),
            &bad,
            compose
        ),
        Err(UiError::InvalidProps)
    );
    assert_eq!(
        TreeView::mount(
            app.world_mut(),
            root,
            UiId(10),
            ViewProps::new("Tree"),
            &[],
            compose
        ),
        Err(UiError::IdentityConflict)
    );
    assert!(controls::Button::mount(
        app.world_mut(),
        root,
        UiId(10),
        controls::ButtonProps::new("Conflict", controls::HostIntent::Command(1))
    )
    .is_err());
    h.unmount(app.world_mut()).unwrap();
    app.update();
    assert_eq!(h.focus(app.world_mut(), ItemId(1)), Err(UiError::Stale));
    let fresh = list(&mut app, root, &items(2));
    assert_ne!(fresh.root, h.root);
    assert!(!app.world().is_alive(entity));
}
#[test]
fn scoped_collection_drain_leaves_unrelated_host_requests_in_order() {
    let (mut app, root) = setup();
    let h = list(&mut app, root, &items(2));
    let b = controls::Button::mount(
        app.world_mut(),
        root,
        UiId(1),
        controls::ButtonProps::new("Host", controls::HostIntent::Command(42)),
    )
    .unwrap();
    frame(&mut app);
    try_activate(app.world_mut(), b.root);
    let row = h.row(app.world(), ItemId(1)).unwrap().root;
    try_activate(app.world_mut(), row);
    frame(&mut app);
    assert_eq!(h.selected(app.world()), Some(ItemId(1)));
    let pending = take_action_intents(app.world_mut());
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].entity, b.root);
}
#[test]
fn painted_theme_density_clips_and_independent_states_are_real_component_output() {
    for appearance in [Appearance::Light, Appearance::Dark] {
        for contrast in [Contrast::Standard, Contrast::High] {
            let mut heights = Vec::new();
            for density in [Density::Comfortable, Density::Compact] {
                let (mut app, root) = setup();
                let theme = UiTheme {
                    appearance,
                    contrast,
                    density,
                    ..UiTheme::default()
                };
                app.world_mut().insert_resource(theme.clone());
                let h = list(&mut app, root, &items(8));
                app.world_mut().get_mut::<Layout>(h.root).unwrap().height = Size::Px(150.0);
                h.select(app.world_mut(), Some(ItemId(1))).unwrap();
                h.focus(app.world_mut(), ItemId(2)).unwrap();
                let output = rndr_paint::frame(&mut app);
                assert!(output.quads.iter().any(
                    |q| q.uv == rndr_paint::solid_uv() && q.color == theme.color(Token::Focus)
                ));
                assert!(output
                    .quads
                    .iter()
                    .any(|q| q.uv == rndr_paint::solid_uv()
                        && q.color == theme.color(Token::Selected)));
                let port = *app.world().get::<Rect>(h.root).unwrap();
                let glyphs: Vec<_> = output
                    .quads
                    .iter()
                    .filter(|q| q.uv != rndr_paint::solid_uv())
                    .collect();
                assert!(!glyphs.is_empty());
                for glyph in glyphs {
                    assert!(glyph.clip.w <= port.w + 0.01 && glyph.clip.h <= port.h + 0.01);
                    assert!(
                        glyph.clip.y >= port.y - 0.01
                            && glyph.clip.y + glyph.clip.h <= port.y + port.h + 0.01
                    );
                }
                let row = h.row(app.world(), ItemId(2)).unwrap();
                heights.push(app.world().get::<Rect>(row.root).unwrap().h);
                let mut props = ViewProps::new("Disabled view");
                props.enabled = false;
                ListView::mount(app.world_mut(), root, UiId(10), props, &items(8), compose)
                    .unwrap();
                let disabled = rndr_paint::frame(&mut app);
                assert_eq!(h.selected(app.world()), Some(ItemId(1)));
                assert_eq!(h.focused(app.world()), None);
                assert!(disabled
                    .quads
                    .iter()
                    .any(|q| q.uv != rndr_paint::solid_uv()
                        && q.color == theme.color(Token::Disabled)));
            }
            assert!(heights[1] < heights[0]);
        }
    }
}
#[test]
fn disabled_tree_ancestry_reconciles_selection_and_focus_without_hidden_targets() {
    let (mut app, root) = setup();
    let h = tree(&mut app, root);
    h.set_expanded(app.world_mut(), ItemId(1), true).unwrap();
    h.set_expanded(app.world_mut(), ItemId(2), true).unwrap();
    h.select(app.world_mut(), Some(ItemId(3))).unwrap();
    h.focus(app.world_mut(), ItemId(3)).unwrap();
    frame(&mut app);
    let mut branch = Item::new(1, "Disabled branch");
    branch.enabled = false;
    let nodes = [
        TreeNode {
            item: branch,
            parent: None,
        },
        TreeNode {
            item: Item::new(3, "Enabled child of disabled branch"),
            parent: Some(ItemId(1)),
        },
    ];
    TreeView::mount(
        app.world_mut(),
        root,
        UiId(20),
        ViewProps::new("Tree"),
        &nodes,
        compose,
    )
    .unwrap();
    frame(&mut app);
    assert_eq!(h.selected(app.world()), None);
    assert_eq!(h.focused(app.world()), None);
    assert!(h.focus(app.world_mut(), ItemId(3)).is_err());
    assert!(h.select(app.world_mut(), Some(ItemId(3))).is_err());
    assert!(access_tree(app.world())
        .nodes
        .iter()
        .filter(|n| n.role == AccessRole::TreeItem)
        .all(|n| !n.enabled && !n.focusable));
    let row = h.row(app.world(), ItemId(3)).unwrap();
    app.world_mut().remove::<Layout>(row.content);
    assert!(h.row(app.world(), ItemId(3)).is_none());
    assert_eq!(h.focus(app.world_mut(), ItemId(3)), Err(UiError::Stale));
}
#[test]
fn retained_budget_is_measured_honestly_not_virtualized() {
    let (mut app, root) = setup();
    let h = list(&mut app, root, &items(MAX_ITEMS as u64));
    let start = std::time::Instant::now();
    frame(&mut app);
    let first = start.elapsed();
    let count = app.world().entities_with::<Layout>().len();
    h.focus(app.world_mut(), ItemId(1)).unwrap();
    key(&mut app, NamedKey::End);
    frame(&mut app);
    assert_eq!(app.world().entities_with::<Layout>().len(), count);
    assert_eq!(
        access_tree(app.world())
            .nodes
            .iter()
            .filter(|n| n.role == AccessRole::ListItem)
            .count(),
        MAX_ITEMS
    );
    eprintln!("retained_budget rows={} layout_entities={} first_frame_ms={:.2}; layout/accessibility remain O(N), not virtualized", MAX_ITEMS, count, first.as_secs_f64()*1000.0);
}
#[test]
fn filtering_or_removing_an_armed_id_cannot_select_a_replacement() {
    let (mut app, root) = setup();
    let h = list(&mut app, root, &items(3));
    frame(&mut app);
    h.focus(app.world_mut(), ItemId(2)).unwrap();
    key(&mut app, NamedKey::Space);
    list(&mut app, root, &[Item::new(9, "Replacement")]);
    frame(&mut app);
    let mut release = KeyEvent::named(NamedKey::Space, Modifiers::default());
    release.released = true;
    app.world_mut().resource_mut::<Input>().unwrap().keys = vec![release];
    frame(&mut app);
    assert_eq!(h.focused(app.world()), Some(ItemId(9)));
    assert_eq!(h.selected(app.world()), None);
    assert!(take_action_intents(app.world_mut()).is_empty());
    let target = h.row(app.world(), ItemId(9)).unwrap().root;
    let mut event = KeyEvent::named(NamedKey::Enter, Modifiers::default());
    event.synthetic = true;
    app.world_mut().resource_mut::<Input>().unwrap().keys = vec![event];
    frame(&mut app);
    assert!(!kinds(&app, h)
        .iter()
        .any(|e| matches!(e, CollectionEventKind::Activated(_))));
    assert_eq!(
        app.world().resource::<Focus>().unwrap().entity,
        Some(target)
    );
}
#[test]
fn core_scroll_drag_cancels_pointer_selection_and_nested_hierarchy_is_projected() {
    let (mut app, root) = setup();
    let h = list(&mut app, root, &items(30));
    frame(&mut app);
    let row = h.row(app.world(), ItemId(2)).unwrap();
    let r = *app.world().get::<Rect>(row.root).unwrap();
    let p = [r.x + r.w * 0.5, r.y + r.h * 0.5];
    app.world_mut().resource_mut::<Input>().unwrap().pointer = Pointer {
        position: p,
        press_at: Some(p),
        pressed: true,
        down: true,
        ..Pointer::default()
    };
    frame(&mut app);
    app.world_mut().resource_mut::<Input>().unwrap().pointer = Pointer {
        position: [p[0], p[1] - 60.0],
        down: true,
        ..Pointer::default()
    };
    frame(&mut app);
    app.world_mut().resource_mut::<Input>().unwrap().pointer = Pointer {
        position: [p[0], p[1] - 60.0],
        released: true,
        ..Pointer::default()
    };
    frame(&mut app);
    assert_eq!(h.selected(app.world()), None);
    h.unmount(app.world_mut()).unwrap();
    let t = tree(&mut app, root);
    t.set_expanded(app.world_mut(), ItemId(1), true).unwrap();
    t.set_expanded(app.world_mut(), ItemId(2), true).unwrap();
    frame(&mut app);
    let projection = access_tree(app.world());
    let ids = |id| access_id(t.row(app.world(), ItemId(id)).unwrap().root);
    let node = |id| projection.nodes.iter().find(|n| n.id == id).unwrap();
    assert_eq!(node(access_id(t.root)).children, vec![ids(1), ids(5)]);
    assert!(node(ids(1)).children.contains(&ids(2)) && node(ids(1)).children.contains(&ids(4)));
    assert!(node(ids(2)).children.contains(&ids(3)));
    assert_eq!(
        app.world()
            .get::<CollectionAccess>(t.row(app.world(), ItemId(3)).unwrap().root)
            .unwrap()
            .level,
        Some(3)
    );
}
#[path = "../examples/support/data_gallery.rs"]
mod gallery;
#[test]
fn gallery_large_dataset_pages_bound_mounted_rows_and_keep_data_app_owned() {
    let mut app = gallery::create(1040.0, 820.0, UiTheme::default());
    frame(&mut app);
    let g = app.world().resource::<gallery::Gallery>().unwrap().clone();
    try_activate(app.world_mut(), g.buttons[8].root);
    frame(&mut app);
    frame(&mut app);
    assert_eq!(
        app.world()
            .resource::<gallery::Gallery>()
            .unwrap()
            .records
            .len(),
        10000
    );
    let count = |app: &App| {
        access_tree(app.world())
            .nodes
            .iter()
            .filter(|n| n.role == AccessRole::ListItem)
            .count()
    };
    assert_eq!(count(&app), 128);
    assert!(g.list.row(app.world(), ItemId(1)).is_some());
    try_activate(app.world_mut(), g.buttons[10].root);
    frame(&mut app);
    frame(&mut app);
    assert_eq!(app.world().resource::<gallery::Gallery>().unwrap().page, 1);
    assert!(g.list.row(app.world(), ItemId(1)).is_none());
    assert!(g.list.row(app.world(), ItemId(129)).is_some());
    assert_eq!(count(&app), 128);
    let mounted = app.world().entities_with::<Layout>().len();
    try_activate(app.world_mut(), g.buttons[9].root);
    frame(&mut app);
    frame(&mut app);
    assert!(g.list.row(app.world(), ItemId(1)).is_some());
    assert_eq!(app.world().entities_with::<Layout>().len(), mounted);
    assert_eq!(
        app.world()
            .resource::<gallery::Gallery>()
            .unwrap()
            .records
            .len(),
        10000
    );
    eprintln!("application_paging records=10000 page_rows=128 mounted_layout_entities={mounted}");
}
#[test]
fn gallery_public_components_render_mutate_filter_resize_and_show_empty_states() {
    let mut app = gallery::create(1040.0, 820.0, UiTheme::default());
    frame(&mut app);
    let g = app.world().resource::<gallery::Gallery>().unwrap().clone();
    let row = g.list.row(app.world(), ItemId(1)).unwrap().root;
    try_activate(app.world_mut(), g.buttons[3].root);
    frame(&mut app); // Reverse
    assert_eq!(g.list.row(app.world(), ItemId(1)).unwrap().root, row);
    request_focus(app.world_mut(), g.filter);
    app.world_mut().resource_mut::<Input>().unwrap().keys =
        vec![KeyEvent::character('T', Modifiers::default()).with_text("Typography")];
    frame(&mut app);
    assert_eq!(
        app.world().resource::<gallery::Gallery>().unwrap().query,
        "Typography"
    );
    assert_eq!(
        access_tree(app.world())
            .nodes
            .iter()
            .filter(|n| n.role == AccessRole::ListItem)
            .count(),
        1
    );
    app.world_mut().resource_mut::<Viewport>().unwrap().width = 380.0;
    frame(&mut app);
    assert_eq!(
        app.world().get::<Layout>(g.columns).unwrap().axis,
        Axis::Column
    );
    app.world_mut()
        .resource_mut::<gallery::Gallery>()
        .unwrap()
        .query = "nothing matches".into();
    gallery::reconcile(app.world_mut());
    frame(&mut app);
    assert!(access_tree(app.world())
        .nodes
        .iter()
        .any(|n| n.name.contains("No matches")));
    app.world_mut()
        .resource_mut::<gallery::Gallery>()
        .unwrap()
        .records
        .clear();
    gallery::reconcile(app.world_mut());
    frame(&mut app);
    assert!(access_tree(app.world())
        .nodes
        .iter()
        .any(|n| n.name.contains("collection is empty")));
}
