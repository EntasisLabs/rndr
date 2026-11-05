use rndr_core::editing::{Affinity, EditOrigin};
use rndr_core::structured::{
    BlockKind, Marks, StructuredDocument, StructuredSelection, TextSelection,
};
use rndr_core::structured_native::*;
use rndr_core::*;
use rndr_ui::structured::*;
use rndr_ui::{UiId, UiPlugin};
#[test]
fn structured_nested_commands_keep_tree_identity_and_shared_history() {
    use rndr_core::structured::{ContainerKind, ListKind};
    let (mut app, _, view, id) = setup();
    select(&mut app, view, 0, 11);
    view.command(app.world_mut(), StructuredCommand::List(ListKind::Task))
        .unwrap();
    let before = app
        .world()
        .resource::<StructuredDocuments>()
        .unwrap()
        .get(id)
        .unwrap()
        .containers()
        .to_vec();
    select(&mut app, view, 7, 7);
    view.command(app.world_mut(), StructuredCommand::Indent)
        .unwrap();
    let doc = app
        .world()
        .resource::<StructuredDocuments>()
        .unwrap()
        .get(id)
        .unwrap();
    assert_eq!(doc.plain_text(), "abcdef\nnext");
    assert_eq!(
        doc.ancestors(doc.blocks()[1].id)
            .unwrap()
            .iter()
            .filter(|c| matches!(c.kind, ContainerKind::Item { .. }))
            .count(),
        2
    );
    view.command(app.world_mut(), StructuredCommand::ToggleTask)
        .unwrap();
    let doc = app
        .world()
        .resource::<StructuredDocuments>()
        .unwrap()
        .get(id)
        .unwrap();
    assert!(matches!(
        doc.ancestors(doc.blocks()[1].id).unwrap()[0].kind,
        ContainerKind::Item {
            checked: Some(true)
        }
    ));
    view.command(app.world_mut(), StructuredCommand::Undo)
        .unwrap();
    view.command(app.world_mut(), StructuredCommand::Undo)
        .unwrap();
    let doc = app
        .world()
        .resource::<StructuredDocuments>()
        .unwrap()
        .get(id)
        .unwrap();
    assert_eq!(doc.containers(), before);
    assert_eq!(
        app.world().get::<Buffer>(view.editor).unwrap().text(),
        "abcdef\nnext"
    );
}
#[test]
fn structured_styles_metadata_and_readonly_are_atomic() {
    use rndr_core::structured::TextStyle;
    let (mut app, _, view, id) = setup();
    select(&mut app, view, 5, 1);
    let style = TextStyle {
        color: Some([20, 100, 210]),
        family: Some("monospace".into()),
        ..Default::default()
    };
    view.command(app.world_mut(), StructuredCommand::TextStyle(style.clone()))
        .unwrap();
    let spans = &app.world().get::<Spans>(view.editor).unwrap().runs;
    assert!(spans
        .iter()
        .any(|s| s.family.as_deref() == Some("monospace")
            && s.color == Color::rgb(20.0 / 255.0, 100.0 / 255.0, 210.0 / 255.0)));
    let before = app
        .world()
        .resource::<StructuredDocuments>()
        .unwrap()
        .get(id)
        .unwrap()
        .revision();
    assert!(view
        .command(
            app.world_mut(),
            StructuredCommand::Attributes([("bad key".into(), "x".into())].into())
        )
        .is_err());
    assert_eq!(
        app.world()
            .resource::<StructuredDocuments>()
            .unwrap()
            .get(id)
            .unwrap()
            .revision(),
        before
    );
    view.command(
        app.world_mut(),
        StructuredCommand::Attributes([("label".into(), "inert".into())].into()),
    )
    .unwrap();
    app.world_mut()
        .resource_mut::<StructuredDocuments>()
        .unwrap()
        .get_mut(id)
        .unwrap()
        .set_readonly(true);
    for command in [
        StructuredCommand::Quote,
        StructuredCommand::Indent,
        StructuredCommand::ToggleTask,
        StructuredCommand::TextStyle(style),
    ] {
        assert!(view.command(app.world_mut(), command).is_err());
    }
}
#[test]
fn structured_gutter_is_owned_and_never_clipboard_text() {
    let (mut app, _, view, _) = setup();
    let owned = app.world().get::<Children>(view.root).unwrap().list.clone();
    assert_eq!(owned.len(), 2);
    assert!(app
        .world()
        .has::<rndr_core::structured_chrome::StructuredChromeGutter>(view.editor));
    assert_eq!(
        app.world().get::<Buffer>(view.editor).unwrap().text(),
        "abcdef\nnext"
    );
    view.destroy(app.world_mut()).unwrap();
    assert!(owned.iter().all(|e| !app.world().is_alive(*e)));
}
#[test]
fn structured_nested_text_pointer_uses_the_painted_indent() {
    use rndr_core::structured::ListKind;
    let (mut app, _, view, id) = setup();
    select(&mut app, view, 0, 11);
    view.command(app.world_mut(), StructuredCommand::List(ListKind::Task))
        .unwrap();
    select(&mut app, view, 7, 7);
    view.command(app.world_mut(), StructuredCommand::Indent)
        .unwrap();
    app.update();
    app.update();
    let layout = app.world().get::<TextLayout>(view.editor).unwrap();
    let (x, y) = layout.caret_xy(8);
    assert!(x > 48.0);
    let rect = app.world().get::<Rect>(view.editor).unwrap();
    let point = [rect.x + TEXT_ORIGIN_X + x, rect.y + TEXT_ORIGIN_Y + y + 2.0];
    *app.world_mut().resource_mut::<Input>().unwrap() = Input {
        pointer: Pointer {
            position: point,
            press_at: Some(point),
            pressed: true,
            ..Default::default()
        },
        ..Default::default()
    };
    app.update();
    *app.world_mut().resource_mut::<Input>().unwrap() = Input::default();
    let doc = app
        .world()
        .resource::<StructuredDocuments>()
        .unwrap()
        .get(id)
        .unwrap();
    let selection = doc.text_selection().unwrap();
    assert_eq!(selection.head.block, doc.blocks()[1].id);
    assert_eq!(selection.head.offset, 1);
    assert_eq!(selection.anchor, selection.head);
}

fn click_mark(app: &mut App, view: StructuredSurfaceHandle, rule: bool) {
    use rndr_core::structured_chrome::{structured_marks, StructuredMarkKind};
    let mark = structured_marks(app.world(), view.editor)
        .into_iter()
        .find(|m| {
            if rule {
                m.kind == StructuredMarkKind::HorizontalRule
            } else {
                matches!(m.kind, StructuredMarkKind::Task(_))
            }
        })
        .unwrap();
    let point = [
        mark.rect.x + mark.rect.w * 0.5,
        mark.rect.y + mark.rect.h * 0.5,
    ];
    *app.world_mut().resource_mut::<Input>().unwrap() = Input {
        pointer: Pointer {
            position: point,
            press_at: Some(point),
            pressed: true,
            ..Default::default()
        },
        ..Default::default()
    };
    app.update();
    *app.world_mut().resource_mut::<Input>().unwrap() = Input::default();
}
#[test]
fn structured_tab_routes_through_full_frame_without_focusing_toolbar() {
    use rndr_core::structured::{ContainerKind, ListKind};
    for legacy in [false, true] {
        let (mut app, _, view, id) = setup();
        let button = app
            .world_mut()
            .spawn((Focusable, Layout::column(), Label::new("Toolbar")));
        select(&mut app, view, 0, 11);
        view.command(app.world_mut(), StructuredCommand::List(ListKind::Task))
            .unwrap();
        select(&mut app, view, 7, 7);
        app.update();
        for (shift, depth) in [(false, 2), (true, 1)] {
            *app.world_mut().resource_mut::<Input>().unwrap() = if legacy {
                Input {
                    key: Some(Key::Tab),
                    shift,
                    ..Default::default()
                }
            } else {
                Input {
                    keys: vec![KeyEvent::named(
                        NamedKey::Tab,
                        Modifiers {
                            shift,
                            ..Modifiers::new()
                        },
                    )],
                    ..Default::default()
                }
            };
            app.update();
            assert_eq!(
                app.world().resource::<Focus>().unwrap().entity,
                Some(view.editor)
            );
            let doc = app
                .world()
                .resource::<StructuredDocuments>()
                .unwrap()
                .get(id)
                .unwrap();
            assert_eq!(
                doc.ancestors(doc.blocks()[1].id)
                    .unwrap()
                    .iter()
                    .filter(|c| matches!(c.kind, ContainerKind::Item { .. }))
                    .count(),
                depth
            );
            assert_eq!(doc.plain_text(), "abcdef\nnext");
        }
        *app.world_mut().resource_mut::<Input>().unwrap() = Input::default();
        app.world_mut()
            .resource_mut::<StructuredDocuments>()
            .unwrap()
            .get_mut(id)
            .unwrap()
            .set_readonly(true);
        app.world_mut().resource_mut::<Input>().unwrap().keys =
            vec![KeyEvent::named(NamedKey::Tab, Modifiers::new())];
        app.update();
        assert_eq!(
            app.world().resource::<Focus>().unwrap().entity,
            Some(view.editor)
        );
        assert_ne!(
            app.world().resource::<Focus>().unwrap().entity,
            Some(button)
        );
    }
    // Ordinary paragraphs retain the generic editor's focus-traversal policy.
    let (mut app, _, view, _) = setup();
    app.world_mut()
        .spawn((Focusable, Layout::column(), Label::new("Toolbar")));
    app.world_mut().resource_mut::<Input>().unwrap().keys =
        vec![KeyEvent::named(NamedKey::Tab, Modifiers::new())];
    app.update();
    assert_ne!(
        app.world().resource::<Focus>().unwrap().entity,
        Some(view.editor)
    );
}
#[test]
fn structured_checkbox_preserves_scrolled_viewport_with_distant_caret() {
    use rndr_core::structured::{ContainerKind, ListKind};
    let text = (0..100).map(|i| format!("task {i}\n")).collect::<String>();
    for focused in [true, false] {
        let (mut app, _, view, id) = setup_text(&text);
        select(&mut app, view, 0, text.len());
        view.command(app.world_mut(), StructuredCommand::List(ListKind::Task))
            .unwrap();
        select(&mut app, view, text.len() - 1, text.len() - 1);
        app.update();
        app.update();
        let before = app
            .world()
            .resource::<StructuredDocuments>()
            .unwrap()
            .get(id)
            .unwrap()
            .selection();
        app.world_mut()
            .get_mut::<Editor>(view.editor)
            .unwrap()
            .offset = 0.0;
        app.update();
        if !focused {
            app.world_mut().insert_resource(Focus { entity: None });
        }
        click_mark(&mut app, view, false);
        app.update();
        let doc = app
            .world()
            .resource::<StructuredDocuments>()
            .unwrap()
            .get(id)
            .unwrap();
        assert_eq!(doc.selection(), before);
        assert!(matches!(
            doc.ancestors(doc.blocks()[0].id).unwrap()[0].kind,
            ContainerKind::Item {
                checked: Some(true)
            }
        ));
        assert_eq!(app.world().get::<Editor>(view.editor).unwrap().offset, 0.0);
        // Keyboard navigation still deliberately reveals the text caret.
        app.world_mut().resource_mut::<Input>().unwrap().keys =
            vec![KeyEvent::named(NamedKey::ArrowLeft, Modifiers::new())];
        app.update();
        assert!(app.world().get::<Editor>(view.editor).unwrap().offset > 0.0);
    }
}
#[test]
fn structured_checkbox_pointer_preserves_selection_and_has_one_history_step() {
    use rndr_core::structured::{ContainerKind, ListKind};
    let (mut app, _, view, id) = setup();
    select(&mut app, view, 0, 11);
    view.command(app.world_mut(), StructuredCommand::List(ListKind::Task))
        .unwrap();
    select(&mut app, view, 5, 1);
    app.update();
    app.update();
    let before = app
        .world()
        .resource::<StructuredDocuments>()
        .unwrap()
        .get(id)
        .unwrap()
        .selection();
    app.world_mut().insert_resource(Focus { entity: None });
    click_mark(&mut app, view, false);
    let d = app
        .world()
        .resource::<StructuredDocuments>()
        .unwrap()
        .get(id)
        .unwrap();
    assert_eq!(d.selection(), before);
    assert!(matches!(
        d.ancestors(d.blocks()[0].id).unwrap()[0].kind,
        ContainerKind::Item {
            checked: Some(true)
        }
    ));
    assert_eq!(
        app.world().resource::<Focus>().unwrap().entity,
        Some(view.editor)
    );
    view.command(app.world_mut(), StructuredCommand::Undo)
        .unwrap();
    let d = app
        .world()
        .resource::<StructuredDocuments>()
        .unwrap()
        .get(id)
        .unwrap();
    assert_eq!(d.selection(), before);
    assert!(matches!(
        d.ancestors(d.blocks()[0].id).unwrap()[0].kind,
        ContainerKind::Item {
            checked: Some(false)
        }
    ));
    app.update();
    app.update();
    app.world_mut()
        .resource_mut::<StructuredDocuments>()
        .unwrap()
        .get_mut(id)
        .unwrap()
        .set_readonly(true);
    let revision = app
        .world()
        .resource::<StructuredDocuments>()
        .unwrap()
        .get(id)
        .unwrap()
        .revision();
    click_mark(&mut app, view, false);
    let d = app
        .world()
        .resource::<StructuredDocuments>()
        .unwrap()
        .get(id)
        .unwrap();
    assert_eq!(d.revision(), revision);
    assert_eq!(d.selection(), before);
}
#[test]
fn structured_rule_pointer_selects_node_and_readonly_does_not_delete() {
    let (mut app, _, view, id) = setup();
    select(&mut app, view, 3, 3);
    view.command(app.world_mut(), StructuredCommand::HorizontalRule)
        .unwrap();
    app.update();
    app.update();
    click_mark(&mut app, view, true);
    let d = app
        .world()
        .resource::<StructuredDocuments>()
        .unwrap()
        .get(id)
        .unwrap();
    let StructuredSelection::Node(rule) = d.selection() else {
        panic!("rule must be a node selection")
    };
    assert_eq!(
        d.blocks().iter().find(|b| b.id == rule).unwrap().kind,
        BlockKind::HorizontalRule
    );
    app.world_mut()
        .resource_mut::<StructuredDocuments>()
        .unwrap()
        .get_mut(id)
        .unwrap()
        .set_readonly(true);
    assert!(view
        .command(app.world_mut(), StructuredCommand::DeleteBlock)
        .is_err());
    assert_eq!(
        app.world()
            .resource::<StructuredDocuments>()
            .unwrap()
            .get(id)
            .unwrap()
            .selection(),
        StructuredSelection::Node(rule)
    );
}
#[test]
fn structured_quoted_task_pointer_targets_nearest_item() {
    use rndr_core::structured::{ContainerKind, ListKind};
    let (mut app, _, view, id) = setup();
    select(&mut app, view, 1, 1);
    view.command(app.world_mut(), StructuredCommand::List(ListKind::Task))
        .unwrap();
    view.command(app.world_mut(), StructuredCommand::Quote)
        .unwrap();
    app.update();
    app.update();
    click_mark(&mut app, view, false);
    let d = app
        .world()
        .resource::<StructuredDocuments>()
        .unwrap()
        .get(id)
        .unwrap();
    assert!(d
        .ancestors(d.blocks()[0].id)
        .unwrap()
        .iter()
        .any(|c| matches!(
            c.kind,
            ContainerKind::Item {
                checked: Some(true)
            }
        )));
    view.command(app.world_mut(), StructuredCommand::Undo)
        .unwrap();
    let d = app
        .world()
        .resource::<StructuredDocuments>()
        .unwrap()
        .get(id)
        .unwrap();
    assert_eq!(
        d.ancestors(d.blocks()[0].id).unwrap()[0].kind,
        ContainerKind::Quote
    );
}
#[test]
fn structured_task_pointer_refuses_stale_projection_and_native_preedit() {
    use rndr_core::structured::ListKind;
    use rndr_core::structured_chrome::structured_marks;
    for composing in [false, true] {
        let (mut app, _, view, id) = setup();
        view.command(app.world_mut(), StructuredCommand::List(ListKind::Task))
            .unwrap();
        app.update();
        app.update();
        let mark = structured_marks(app.world(), view.editor)
            .into_iter()
            .next()
            .unwrap();
        let point = [mark.rect.x + 5.0, mark.rect.y + 5.0];
        if !composing {
            app.world_mut()
                .resource_mut::<StructuredDocuments>()
                .unwrap()
                .get_mut(id)
                .unwrap()
                .set_block_kind(BlockKind::Heading(2), EditOrigin::Host(7))
                .unwrap();
        }
        let before = app
            .world()
            .resource::<StructuredDocuments>()
            .unwrap()
            .get(id)
            .unwrap()
            .revision();
        *app.world_mut().resource_mut::<Input>().unwrap() = Input {
            pointer: Pointer {
                position: point,
                press_at: Some(point),
                pressed: true,
                ..Default::default()
            },
            ime_preedit: if composing {
                "仮".into()
            } else {
                String::new()
            },
            ..Default::default()
        };
        app.update();
        let d = app
            .world()
            .resource::<StructuredDocuments>()
            .unwrap()
            .get(id)
            .unwrap();
        assert_eq!(d.revision(), before);
        assert!(d.containers().iter().all(|c| !matches!(
            c.kind,
            rndr_core::structured::ContainerKind::Item {
                checked: Some(true)
            }
        )));
    }
}
fn setup() -> (App, Entity, StructuredSurfaceHandle, StructuredHandle) {
    setup_text("abcdef\nnext")
}
fn setup_text(text: &str) -> (App, Entity, StructuredSurfaceHandle, StructuredHandle) {
    let mut app = App::new();
    app.add_plugin(UiPlugin);
    app.world_mut().insert_resource(Viewport::new(640.0, 480.0));
    let mut docs = StructuredDocuments::default();
    let id = docs.insert(StructuredDocument::new(text));
    app.world_mut().insert_resource(docs);
    let parent = app.world_mut().spawn((Layout {
        width: Size::Fill,
        height: Size::Fill,
        ..Layout::column()
    },));
    let view = StructuredSurface::mount(
        app.world_mut(),
        parent,
        UiId(10),
        StructuredSurfaceProps::new(id, "Tree editor"),
    )
    .unwrap();
    app.world_mut().insert_resource(Focus {
        entity: Some(view.editor),
    });
    app.update();
    (app, parent, view, id)
}
fn select(app: &mut App, view: StructuredSurfaceHandle, a: usize, h: usize) {
    let r = app
        .world()
        .get::<StructuredBinding>(view.editor)
        .unwrap()
        .revision;
    set_display_selection(app.world_mut(), view.editor, r, a, h).unwrap();
}
#[test]
fn structured_toolbar_keeps_directed_saved_selection_and_additive_marks() {
    let (mut app, _, view, id) = setup();
    select(&mut app, view, 5, 1);
    let before = app
        .world()
        .resource::<StructuredDocuments>()
        .unwrap()
        .get(id)
        .unwrap()
        .selection();
    app.world_mut().insert_resource(Focus { entity: None });
    for command in [
        StructuredCommand::Strong,
        StructuredCommand::Emphasis,
        StructuredCommand::Link(Some("https://example.com".into())),
    ] {
        view.command(app.world_mut(), command).unwrap();
    }
    let doc = app
        .world()
        .resource::<StructuredDocuments>()
        .unwrap()
        .get(id)
        .unwrap();
    assert_eq!(doc.selection(), before);
    let marked = &doc.blocks()[0].runs[1].marks;
    assert!(marked.strong && marked.emphasis && marked.link.is_some());
    assert!(app
        .world()
        .get::<Spans>(view.editor)
        .unwrap()
        .runs
        .iter()
        .any(|s| s.italic && s.underline && s.weight == Some(700)));
    view.command(app.world_mut(), StructuredCommand::Strong)
        .unwrap();
    let marked = &app
        .world()
        .resource::<StructuredDocuments>()
        .unwrap()
        .get(id)
        .unwrap()
        .blocks()[0]
        .runs[1]
        .marks;
    assert!(!marked.strong && marked.emphasis && marked.link.is_some());
}
#[test]
fn structured_invalid_link_and_empty_selection_refuse_without_history() {
    let (mut app, _, view, id) = setup();
    select(&mut app, view, 0, 2);
    let r = app
        .world()
        .resource::<StructuredDocuments>()
        .unwrap()
        .get(id)
        .unwrap()
        .revision();
    assert!(view
        .command(
            app.world_mut(),
            StructuredCommand::Link(Some("javascript:alert(1)".into()))
        )
        .is_err());
    assert_eq!(
        app.world()
            .resource::<StructuredDocuments>()
            .unwrap()
            .get(id)
            .unwrap()
            .revision(),
        r
    );
    select(&mut app, view, 1, 1);
    assert_eq!(
        view.command(app.world_mut(), StructuredCommand::Strong),
        Err(StructuredSurfaceError::SelectText)
    );
}
#[test]
fn structured_toolbar_refuses_active_and_new_native_preedit() {
    let (mut app, _, view, id) = setup();
    select(&mut app, view, 1, 4);
    app.world_mut().resource_mut::<Input>().unwrap().ime_preedit = "仮".into();
    assert!(view
        .command(app.world_mut(), StructuredCommand::Strong)
        .is_err());
    app.update();
    assert!(is_composing(app.world(), view.editor));
    app.world_mut().insert_resource(Focus { entity: None });
    app.update();
    assert!(view
        .command(
            app.world_mut(),
            StructuredCommand::Block(BlockKind::Heading(2))
        )
        .is_err());
    assert!(view.destroy(app.world_mut()).is_err());
    assert_eq!(
        app.world()
            .resource::<StructuredDocuments>()
            .unwrap()
            .get(id)
            .unwrap()
            .plain_text(),
        "abcdef\nnext"
    );
}
#[test]
fn structured_retained_identity_teardown_and_document_history_survive() {
    let (mut app, parent, view, id) = setup();
    app.world_mut()
        .get_mut::<Semantics>(view.editor)
        .unwrap()
        .readonly = true;
    let same = StructuredSurface::mount(
        app.world_mut(),
        parent,
        UiId(10),
        StructuredSurfaceProps::new(id, "Renamed tree"),
    )
    .unwrap();
    assert_eq!(view, same);
    assert!(app.world().get::<Semantics>(view.editor).unwrap().readonly);
    assert!(view
        .command(app.world_mut(), StructuredCommand::Strong)
        .is_err());
    app.world_mut()
        .get_mut::<Semantics>(view.editor)
        .unwrap()
        .readonly = false;
    select(&mut app, view, 0, 3);
    view.command(app.world_mut(), StructuredCommand::Strong)
        .unwrap();
    let borrowed = app.world_mut().spawn((Layout::column(),));
    set_parent(app.world_mut(), borrowed, view.root);
    view.destroy(app.world_mut()).unwrap();
    assert!(app.world().is_alive(borrowed));
    assert!(app.world().get::<Parent>(borrowed).is_none());
    assert!(!app.world().is_alive(view.editor));
    assert!(view.validate(app.world()).is_err());
    let next = StructuredSurface::mount(
        app.world_mut(),
        parent,
        UiId(11),
        StructuredSurfaceProps::new(id, "Remount"),
    )
    .unwrap();
    assert_ne!(next.editor, view.editor);
    next.command(app.world_mut(), StructuredCommand::Undo)
        .unwrap();
    assert_eq!(
        app.world()
            .resource::<StructuredDocuments>()
            .unwrap()
            .get(id)
            .unwrap()
            .blocks()[0]
            .runs[0]
            .marks,
        Marks::default()
    );
}
#[test]
fn structured_second_view_refusal_does_not_rebind_existing_history() {
    let (mut app, parent, view, id) = setup();
    assert!(StructuredSurface::mount(
        app.world_mut(),
        parent,
        UiId(20),
        StructuredSurfaceProps::new(id, "Second")
    )
    .is_err());
    view.validate(app.world()).unwrap();
    assert_eq!(attached_structured(app.world(), view.editor), Some(id));
}
#[test]
fn structured_readonly_toolbar_and_native_advertising_agree() {
    let (mut app, _, view, id) = setup();
    select(&mut app, view, 0, 3);
    app.world_mut()
        .resource_mut::<StructuredDocuments>()
        .unwrap()
        .get_mut(id)
        .unwrap()
        .set_readonly(true);
    app.update();
    assert!(access_details(app.world(), view.editor).readonly);
    assert!(view
        .command(app.world_mut(), StructuredCommand::Strong)
        .is_err());
    assert!(view
        .command(app.world_mut(), StructuredCommand::Undo)
        .is_err());
    view.command(app.world_mut(), StructuredCommand::SelectBlock)
        .unwrap();
    assert!(matches!(
        app.world()
            .resource::<StructuredDocuments>()
            .unwrap()
            .get(id)
            .unwrap()
            .selection(),
        StructuredSelection::Node(_)
    ));
}
#[test]
fn structured_first_frame_spans_match_resolved_foreground_and_theme_changes() {
    let (mut app, _, view, _) = setup();
    for appearance in [rndr_ui::Appearance::Light, rndr_ui::Appearance::Dark] {
        rndr_ui::apply_theme_bridge(
            app.world_mut(),
            rndr_ui::UiTheme {
                appearance,
                ..Default::default()
            },
        );
        app.update();
        let foreground = app
            .world()
            .get::<Label>(view.editor)
            .unwrap()
            .color
            .unwrap();
        let background = *app.world().get::<Color>(view.editor).unwrap();
        assert_ne!(
            foreground, background,
            "glyph foreground is not an entity fill"
        );
        assert_eq!(
            app.world().get::<Spans>(view.editor).unwrap().runs[0].color,
            foreground
        );
    }
}
#[test]
fn structured_generic_tree_snapshot_is_not_markdown_codec() {
    let (mut app, _, view, id) = setup();
    select(&mut app, view, 0, 6);
    view.command(app.world_mut(), StructuredCommand::Strong)
        .unwrap();
    view.command(
        app.world_mut(),
        StructuredCommand::Block(BlockKind::Heading(2)),
    )
    .unwrap();
    let doc = app
        .world()
        .resource::<StructuredDocuments>()
        .unwrap()
        .get(id)
        .unwrap();
    let generic = format!("{:?}", doc.blocks());
    assert!(generic.contains("Heading(2)") && generic.contains("strong: true"));
    assert_eq!(doc.plain_text(), "abcdef\nnext");
    assert!(!doc.plain_text().contains("**"));
}
#[test]
fn structured_formatting_and_keyboard_share_exactly_one_history() {
    let (mut app, _, view, id) = setup();
    select(&mut app, view, 0, 3);
    view.command(app.world_mut(), StructuredCommand::Strong)
        .unwrap();
    app.world_mut().resource_mut::<Input>().unwrap().chars = "é".into();
    app.update();
    assert_eq!(
        app.world()
            .resource::<StructuredDocuments>()
            .unwrap()
            .get(id)
            .unwrap()
            .plain_text(),
        "édef\nnext"
    );
    view.command(app.world_mut(), StructuredCommand::Undo)
        .unwrap();
    let doc = app
        .world()
        .resource::<StructuredDocuments>()
        .unwrap()
        .get(id)
        .unwrap();
    assert_eq!(doc.plain_text(), "abcdef\nnext");
    assert!(doc.blocks()[0].runs[0].marks.strong);
    view.command(app.world_mut(), StructuredCommand::Undo)
        .unwrap();
    assert_eq!(
        app.world()
            .resource::<StructuredDocuments>()
            .unwrap()
            .get(id)
            .unwrap()
            .blocks()[0]
            .runs[0]
            .marks,
        Marks::default()
    );
}
#[test]
fn structured_external_typed_selection_and_block_navigation_contract() {
    let (mut app, _, view, id) = setup();
    let doc = app
        .world_mut()
        .resource_mut::<StructuredDocuments>()
        .unwrap()
        .get_mut(id)
        .unwrap();
    let a = doc.position_at(7, Affinity::After).unwrap();
    let h = doc.position_at(11, Affinity::After).unwrap();
    doc.set_selection(StructuredSelection::Text(TextSelection {
        anchor: a,
        head: h,
    }))
    .unwrap();
    project(app.world_mut(), view.editor);
    let display = structured_display(app.world(), view.editor).unwrap();
    assert_eq!(display.blocks[1].range, 7..11);
    assert_eq!(
        display.selection,
        StructuredSelection::Text(TextSelection { anchor: a, head: h })
    );
    app.world_mut()
        .resource_mut::<StructuredDocuments>()
        .unwrap()
        .get_mut(id)
        .unwrap()
        .set_block_kind(BlockKind::Heading(3), EditOrigin::Host(1))
        .unwrap();
    assert!(structured_display(app.world(), view.editor).is_none());
}
