//! Retained integration with canonical source viewport, not native capture evidence.
use rndr_core::editing::*;
use rndr_core::*;
use rndr_ui::editing::*;
use rndr_ui::{UiId, UiPlugin};

fn boot() -> (App, Entity, DocumentId, EditorSurfaceHandle) {
    boot_order(0)
}
fn boot_order(order: u8) -> (App, Entity, DocumentId, EditorSurfaceHandle) {
    let mut app = App::new();
    if order == 1 {
        app.add_plugin(Shell);
    }
    app.add_plugin(UiPlugin);
    if order == 2 {
        app.add_plugin(Shell);
    }
    app.world_mut().insert_resource(Viewport::new(320.0, 180.0));
    let mut docs = SourceDocuments::default();
    let doc = docs.open("line é 👩‍💻\r\n".repeat(100));
    app.world_mut().insert_resource(docs);
    let parent = app.world_mut().spawn((Layout {
        width: Size::Fill,
        height: Size::Fill,
        ..Layout::column()
    },));
    let view = EditorSurface::mount(
        app.world_mut(),
        parent,
        UiId(1),
        EditorSurfaceProps::new(doc, "Source"),
    )
    .unwrap();
    for _ in 0..3 {
        app.update();
    }
    (app, parent, doc, view)
}
fn bar(app: &App, view: EditorSurfaceHandle) -> Entity {
    children_of(app.world(), view.root)
        .into_iter()
        .find(|e| app.world().has::<Scrollbar>(*e))
        .unwrap()
}
#[test]
fn wheel_over_editor_and_track_needs_no_focus_and_preserves_source() {
    for order in [0, 1, 2] {
        for over_track in [false, true] {
            for readonly in [false, true] {
                let (mut app, parent, doc, view) = boot_order(order);
                let bar = bar(&app, view);
                if readonly {
                    let mut props = EditorSurfaceProps::new(doc, "Readonly");
                    props.readonly = true;
                    EditorSurface::mount(app.world_mut(), parent, UiId(1), props).unwrap();
                }
                let before = app
                    .world()
                    .resource::<SourceDocuments>()
                    .unwrap()
                    .get(doc)
                    .unwrap()
                    .snapshot();
                let selection = app
                    .world()
                    .resource::<SourceDocuments>()
                    .unwrap()
                    .get(doc)
                    .unwrap()
                    .selection();
                for focus in [None, Some(view.editor), Some(bar)] {
                    set_scrollbar_value(app.world_mut(), bar, 0.0);
                    app.world_mut().resource_mut::<Focus>().unwrap().entity = focus;
                    let rect = *app
                        .world()
                        .get::<Rect>(if over_track { bar } else { view.editor })
                        .unwrap();
                    let input = app.world_mut().resource_mut::<Input>().unwrap();
                    input.pointer = Pointer {
                        position: [rect.x + rect.w / 2.0, rect.y + 10.0],
                        ..Default::default()
                    };
                    input.scroll = [0.0, -96.0];
                    app.update();
                    assert!(
                        app.world().get::<Editor>(view.editor).unwrap().offset > 0.0,
                        "order={order}, track={over_track}, readonly={readonly}, focus={focus:?}"
                    );
                    assert_eq!(app.world().resource::<Focus>().unwrap().entity, focus);
                    app.world_mut().resource_mut::<Input>().unwrap().scroll = [0.0, 96.0];
                    app.update();
                    assert_eq!(app.world().get::<Editor>(view.editor).unwrap().offset, 0.0);
                }
                let session = app
                    .world_mut()
                    .resource_mut::<SourceDocuments>()
                    .unwrap()
                    .get_mut(doc)
                    .unwrap();
                assert_eq!(session.text(), before.text);
                assert_eq!(session.selection(), selection);
                assert_eq!(session.revision(), before.revision);
                assert_eq!(session.drain_events().count(), 0);
            }
        }
    }
}
#[test]
fn wheel_over_line_numbers_scrolls_editor_even_without_scrollbar() {
    let (mut app, parent, doc, view) = boot();
    let mut props = EditorSurfaceProps::new(doc, "Source");
    props.scrollbar = false;
    props.gutter = true;
    EditorSurface::mount(app.world_mut(), parent, UiId(1), props).unwrap();
    app.update();
    let gutter = children_of(app.world(), view.root)
        .into_iter()
        .find(|e| app.world().has::<Gutter>(*e))
        .unwrap();
    let rect = *app.world().get::<Rect>(gutter).unwrap();
    let input = app.world_mut().resource_mut::<Input>().unwrap();
    input.pointer.position = [rect.x + 5.0, rect.y + 10.0];
    input.scroll = [0.0, -64.0];
    app.update();
    assert!(app.world().get::<Editor>(view.editor).unwrap().offset > 0.0);
}
#[test]
fn tiny_trackpad_deltas_accumulate_and_direction_reverses_at_bounds() {
    for over_track in [false, true] {
        let (mut app, _, _, view) = boot();
        let bar = bar(&app, view);
        let rect = *app
            .world()
            .get::<Rect>(if over_track { bar } else { view.editor })
            .unwrap();
        app.world_mut()
            .resource_mut::<Input>()
            .unwrap()
            .pointer
            .position = [rect.x + rect.w / 2.0, rect.y + 10.0];
        for _ in 0..128 {
            app.world_mut().resource_mut::<Input>().unwrap().scroll = [0.0, -0.5];
            app.update();
        }
        assert!(app.world().get::<Editor>(view.editor).unwrap().offset >= 32.0);
        for _ in 0..128 {
            app.world_mut().resource_mut::<Input>().unwrap().scroll = [0.0, 0.5];
            app.update();
        }
        assert_eq!(app.world().get::<Editor>(view.editor).unwrap().offset, 0.0);
        app.world_mut().resource_mut::<Input>().unwrap().scroll = [0.0, -100_000.0];
        app.update();
        let end = scrollbar_geometry(app.world(), bar).unwrap();
        assert_eq!(end.value, end.max);
        app.world_mut().resource_mut::<Input>().unwrap().scroll = [0.0, 32.0];
        app.update();
        assert!(scrollbar_geometry(app.world(), bar).unwrap().value < end.max);
    }
}
#[test]
fn retained_scrollbar_is_canonical_readonly_and_does_not_edit_selection_or_history() {
    let (mut app, parent, doc, view) = boot();
    let bar = bar(&app, view);
    let snapshot = app
        .world()
        .resource::<SourceDocuments>()
        .unwrap()
        .get(doc)
        .unwrap()
        .snapshot();
    let mut props = EditorSurfaceProps::new(doc, "Readonly source");
    props.readonly = true;
    assert_eq!(
        EditorSurface::mount(app.world_mut(), parent, UiId(1), props).unwrap(),
        view
    );
    assert!(scrollbar_geometry(app.world(), bar).unwrap().max > 0.0);
    app.world_mut().resource_mut::<Focus>().unwrap().entity = Some(bar);
    app.world_mut()
        .resource_mut::<Input>()
        .unwrap()
        .keys
        .push(KeyEvent::named(NamedKey::End, Modifiers::new()));
    app.update();
    let geometry = scrollbar_geometry(app.world(), bar).unwrap();
    assert!(geometry.value > 0.0);
    assert!(app.world().get::<Editor>(view.editor).unwrap().offset > 0.0);
    let session = app
        .world_mut()
        .resource_mut::<SourceDocuments>()
        .unwrap()
        .get_mut(doc)
        .unwrap();
    assert_eq!(session.snapshot().text, snapshot.text);
    assert_eq!(session.revision(), snapshot.revision);
    assert_eq!(session.drain_events().count(), 0);
    assert_eq!(session.undo(), Err(EditError::Readonly));
    session.set_readonly(false).unwrap();
    assert!(session.undo().unwrap().is_none());
}
#[test]
fn scrollbar_input_precedes_parent_scroll_in_both_shell_plugin_orders() {
    for order in [0, 1, 2] {
        let (mut app, _, _, view) = boot_order(order);
        let bar = bar(&app, view);
        app.world_mut().resource_mut::<Focus>().unwrap().entity = Some(bar);
        app.world_mut()
            .resource_mut::<Input>()
            .unwrap()
            .keys
            .push(KeyEvent::named(NamedKey::End, Modifiers::new()));
        app.update();
        assert!(
            scrollbar_geometry(app.world(), bar).unwrap().value > 0.0,
            "order {order}"
        );
        set_scrollbar_value(app.world_mut(), bar, 0.0);
        let track = scrollbar_geometry(app.world(), bar).unwrap().track;
        let input = app.world_mut().resource_mut::<Input>().unwrap();
        input.pointer = Pointer {
            position: [track.x + 12.0, track.y + 12.0],
            ..Default::default()
        };
        input.scroll = [0.0, -100.0];
        app.update();
        assert_eq!(
            scrollbar_geometry(app.world(), bar).unwrap().value,
            100.0,
            "wheel must be consumed once, order {order}"
        );
    }
}
#[test]
fn scrollbar_press_cannot_discard_managed_preedit_or_same_frame_commit() {
    for commit in [false, true] {
        let (mut app, _, doc, view) = boot();
        let bar = bar(&app, view);
        app.world_mut().resource_mut::<Focus>().unwrap().entity = Some(view.editor);
        app.world_mut().resource_mut::<Input>().unwrap().ime_preedit = "に".into();
        app.update();
        assert!(app
            .world()
            .resource::<SourceDocuments>()
            .unwrap()
            .get(doc)
            .unwrap()
            .is_composing());
        let geometry = scrollbar_geometry(app.world(), bar).unwrap();
        let at = [
            geometry.track.x + geometry.track.w / 2.0,
            geometry.track.y + 2.0,
        ];
        let before = app
            .world()
            .resource::<SourceDocuments>()
            .unwrap()
            .get(doc)
            .unwrap()
            .text()
            .to_owned();
        let input = app.world_mut().resource_mut::<Input>().unwrap();
        input.pointer = Pointer {
            position: at,
            press_at: Some(at),
            pressed: true,
            down: true,
            ..Default::default()
        };
        if commit {
            input.ime_preedit.clear();
            input.ime_commit = "日".into();
        }
        app.update();
        assert_eq!(
            app.world().resource::<Focus>().unwrap().entity,
            Some(view.editor)
        );
        let session = app
            .world()
            .resource::<SourceDocuments>()
            .unwrap()
            .get(doc)
            .unwrap();
        if commit {
            assert_eq!(session.text(), format!("{before}日"));
        } else {
            assert_eq!(session.text(), before);
            assert!(session.is_composing());
        }
    }
}
#[test]
fn retained_track_press_does_not_start_editor_selection() {
    let (mut app, _, doc, view) = boot();
    let bar = bar(&app, view);
    let geometry = scrollbar_geometry(app.world(), bar).unwrap();
    assert!(set_scrollbar_value(app.world_mut(), bar, 0.0));
    app.world_mut().resource_mut::<Focus>().unwrap().entity = Some(view.editor);
    let before = app
        .world()
        .resource::<SourceDocuments>()
        .unwrap()
        .get(doc)
        .unwrap()
        .selection();
    let at = [
        geometry.track.x + geometry.track.w / 2.0,
        geometry.track.y + geometry.track.h - 2.0,
    ];
    app.world_mut().resource_mut::<Input>().unwrap().pointer = Pointer {
        position: at,
        press_at: Some(at),
        pressed: true,
        down: true,
        ..Default::default()
    };
    app.update();
    assert_eq!(
        app.world().resource::<Focus>().unwrap().entity,
        Some(view.editor)
    );
    assert!(app.world().get::<Editor>(view.editor).unwrap().offset > 0.0);
    assert_eq!(
        app.world()
            .resource::<SourceDocuments>()
            .unwrap()
            .get(doc)
            .unwrap()
            .selection(),
        before
    );
    app.world_mut().resource_mut::<Input>().unwrap().pointer = Pointer {
        position: at,
        released: true,
        ..Default::default()
    };
    app.update();
    assert_eq!(
        app.world()
            .resource::<SourceDocuments>()
            .unwrap()
            .get(doc)
            .unwrap()
            .selection(),
        before
    );
}
#[test]
fn scrollbar_toggle_preserves_view_and_borrowed_children_but_teardown_owns_bar() {
    let (mut app, parent, doc, view) = boot();
    let old_bar = bar(&app, view);
    let borrowed = app.world_mut().spawn((Layout::column(),));
    set_parent(app.world_mut(), borrowed, view.root);
    assert!(set_scrollbar_value(app.world_mut(), old_bar, 200.0));
    let offset = app.world().get::<Editor>(view.editor).unwrap().offset;
    app.world_mut().resource_mut::<Focus>().unwrap().entity = Some(old_bar);
    let mut props = EditorSurfaceProps::new(doc, "Source");
    props.scrollbar = false;
    EditorSurface::mount(app.world_mut(), parent, UiId(1), props).unwrap();
    assert!(!app.world().is_alive(old_bar));
    assert_ne!(
        app.world().resource::<Focus>().unwrap().entity,
        Some(old_bar)
    );
    assert_eq!(
        app.world().get::<Editor>(view.editor).unwrap().offset,
        offset
    );
    EditorSurface::mount(
        app.world_mut(),
        parent,
        UiId(1),
        EditorSurfaceProps::new(doc, "Source"),
    )
    .unwrap();
    let new_bar = bar(&app, view);
    assert_ne!(new_bar, old_bar);
    view.destroy(app.world_mut()).unwrap();
    assert!(!app.world().is_alive(new_bar));
    assert!(app.world().is_alive(borrowed));
    assert!(app.world().get::<Parent>(borrowed).is_none());
}
