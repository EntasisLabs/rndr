use rndr_core::*;
fn app() -> App {
    let mut app = App::new();
    app.add_plugin(GenericInteraction);
    app.world_mut().insert_resource(Viewport::new(400.0, 300.0));
    app
}
#[test]
fn generic_install_preserves_focused_legacy_list_rows() {
    let mut app = app();
    let list = app
        .world_mut()
        .spawn((Layout::column(), List::new(2, 30.0)));
    let row = app.world_mut().spawn((
        Layout::column(),
        ListItem {
            index: 0,
            selected: true,
        },
    ));
    set_parent(app.world_mut(), row, list);
    app.world_mut().resource_mut::<Focus>().unwrap().entity = Some(row);
    app.update();
    assert_eq!(app.world().resource::<Focus>().unwrap().entity, Some(row));
}
#[test]
fn unarmed_escape_still_reaches_legacy_overlay() {
    let mut app = app();
    let overlay = app
        .world_mut()
        .spawn((Layer(10), Layout::column(), Label::new("overlay")));
    app.world_mut().resource_mut::<Input>().unwrap().push_key(
        KeyEvent::named(NamedKey::Escape, Modifiers::new()),
        Platform::Mac,
    );
    app.update();
    assert!(!app.world().is_alive(overlay));
    assert_eq!(
        app.world().resource::<Fired>().unwrap().name.as_deref(),
        Some(DISMISS)
    );
}
#[test]
fn readonly_preserves_field_and_editor_text_ime_and_history() {
    for editor in [false, true] {
        let mut app = app();
        let entity = app.world_mut().spawn((
            Layout {
                width: Size::Fill,
                height: Size::Px(100.0),
                ..Layout::column()
            },
            Label::new("original"),
            Semantics {
                readonly: true,
                ..Semantics::default()
            },
        ));
        if editor {
            app.world_mut().insert(entity, Editor::default());
            app.world_mut().insert(entity, Buffer::from_str("original"));
        } else {
            app.world_mut().insert(entity, TextField);
        }
        request_focus(app.world_mut(), entity);
        let before = app
            .world()
            .get::<Buffer>(entity)
            .map(|b| (b.caret, b.anchor));
        let input = app.world_mut().resource_mut::<Input>().unwrap();
        input.chars = "bad".into();
        input.ime_commit = "unsafe".into();
        app.update();
        let text = if editor {
            app.world().get::<Buffer>(entity).unwrap().text()
        } else {
            app.world().get::<Label>(entity).unwrap().text.clone()
        };
        assert_eq!(text, "original");
        assert_eq!(
            try_apply_access(
                app.world_mut(),
                AccessCommand::SetValue {
                    entity,
                    value: "unsafe AT".into()
                }
            ),
            AccessCommandResult::UnsupportedSetValue
        );
        assert_eq!(
            before,
            app.world()
                .get::<Buffer>(entity)
                .map(|b| (b.caret, b.anchor))
        );
    }
}
#[test]
fn destroyed_semantic_tab_cannot_leave_an_exposed_orphan_panel() {
    let mut app = app();
    let group = app.world_mut().spawn((Layout::row(), Tabs { selected: 1 }));
    let tab = app.world_mut().spawn((Layout::column(), Tab { id: 1 }));
    set_parent(app.world_mut(), tab, group);
    let panel = app.world_mut().spawn((Layout::column(), TabPanel));
    link_tab_panel(app.world_mut(), tab, group, panel).unwrap();
    assert!(interaction_visible(app.world(), panel));
    despawn(app.world_mut(), tab);
    app.update();
    assert!(!interaction_visible(app.world(), panel));
    assert_eq!(
        try_activate(app.world_mut(), tab),
        ActivationResult::Rejected
    );
}
