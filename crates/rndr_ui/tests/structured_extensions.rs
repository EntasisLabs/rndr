use rndr_core::editing::{Affinity, EditOrigin};
use rndr_core::structured::*;
use rndr_core::structured::{Position, TextSelection};
use rndr_core::structured_chrome::{structured_marks, StructuredMarkKind};
use rndr_core::structured_embeds::AtomicMount;
use rndr_core::structured_native::*;
use rndr_core::*;
use rndr_ui::structured::*;
use rndr_ui::structured_embeds::AtomicPanelHandle;
use rndr_ui::{UiId, UiPlugin};

fn setup() -> (App, Entity, StructuredSurfaceHandle, StructuredHandle) {
    let mut app = App::new();
    app.add_plugin(UiPlugin);
    app.world_mut().insert_resource(Viewport::new(640.0, 600.0));
    app.world_mut().insert_resource(Platform::Windows);
    let mut docs = StructuredDocuments::default();
    let id = docs.insert(StructuredDocument::new("before\nafter"));
    app.world_mut().insert_resource(docs);
    let root = app.world_mut().spawn((Layout {
        width: Size::Fill,
        height: Size::Fill,
        ..Layout::column()
    },));
    let view = StructuredSurface::mount(
        app.world_mut(),
        root,
        UiId(90),
        StructuredSurfaceProps::new(id, "Extensions"),
    )
    .unwrap();
    app.world_mut().insert_resource(Focus {
        entity: Some(view.editor),
    });
    app.update();
    (app, root, view, id)
}
fn doc(app: &App, id: StructuredHandle) -> &StructuredDocument {
    app.world()
        .resource::<StructuredDocuments>()
        .unwrap()
        .get(id)
        .unwrap()
}
fn doc_mut(app: &mut App, id: StructuredHandle) -> &mut StructuredDocument {
    app.world_mut()
        .resource_mut::<StructuredDocuments>()
        .unwrap()
        .get_mut(id)
        .unwrap()
}
fn command(app: &mut App, view: StructuredSurfaceHandle, c: StructuredCommand) {
    view.command(app.world_mut(), c).unwrap();
    app.update();
}
fn key(app: &mut App, key: NamedKey, modifiers: Modifiers) {
    *app.world_mut().resource_mut::<Input>().unwrap() = Input {
        keys: vec![KeyEvent::named(key, modifiers)],
        ..Default::default()
    };
    app.update();
    *app.world_mut().resource_mut::<Input>().unwrap() = Input::default();
}
fn chord(app: &mut App, letter: &str) {
    let event = KeyEvent::character(
        letter.chars().next().unwrap(),
        Modifiers {
            ctrl: true,
            ..Modifiers::new()
        },
    );
    *app.world_mut().resource_mut::<Input>().unwrap() = Input {
        keys: vec![event],
        ..Default::default()
    };
    app.update();
    *app.world_mut().resource_mut::<Input>().unwrap() = Input::default();
}
fn table(app: &App, id: StructuredHandle) -> Vec<Vec<ContainerId>> {
    let d = doc(app, id);
    d.table_cells(
        d.containers()
            .iter()
            .find(|c| c.kind == ContainerKind::Table)
            .unwrap()
            .id,
    )
    .unwrap()
}
fn select_cell(
    app: &mut App,
    view: StructuredSurfaceHandle,
    id: StructuredHandle,
    cell: ContainerId,
) {
    let p = Position {
        block: doc(app, id).cell_first_block(cell).unwrap(),
        offset: 0,
        affinity: Affinity::After,
    };
    doc_mut(app, id)
        .set_selection(StructuredSelection::Text(TextSelection {
            anchor: p,
            head: p,
        }))
        .unwrap();
    project(app.world_mut(), view.editor);
    app.update();
}
fn payload(text: &str) -> AtomicPayload {
    AtomicPayload {
        kind: "embedded".into(),
        fallback: text.into(),
        attributes: Default::default(),
    }
}
fn atom(app: &mut App, view: StructuredSurfaceHandle, id: StructuredHandle) -> BlockId {
    command(
        app,
        view,
        StructuredCommand::Atomic(payload("nested draft")),
    );
    doc(app, id)
        .blocks()
        .iter()
        .find(|b| b.kind == BlockKind::Atomic)
        .unwrap()
        .id
}

#[test]
fn table_tab_visits_cells_before_focus_and_escapes_at_model_edges() {
    let (mut app, _, view, id) = setup();
    let button = app.world_mut().spawn((
        Button::new("unrelated"),
        Focusable::default(),
        Layout::column(),
    ));
    command(
        &mut app,
        view,
        StructuredCommand::Table {
            rows: 2,
            columns: 2,
            header: true,
        },
    );
    let grid = table(&app, id);
    let revision = doc(&app, id).revision();
    for cell in [grid[0][1], grid[1][0], grid[1][1]] {
        key(&mut app, NamedKey::Tab, Modifiers::new());
        assert_eq!(
            app.world().resource::<Focus>().unwrap().entity,
            Some(view.editor)
        );
        assert_ne!(
            app.world().resource::<Focus>().unwrap().entity,
            Some(button)
        );
        let head = doc(&app, id).text_selection().unwrap().head;
        assert_eq!(
            doc(&app, id)
                .cell_address(head.block)
                .unwrap()
                .unwrap()
                .cell,
            cell
        );
    }
    key(&mut app, NamedKey::Tab, Modifiers::new());
    assert!(doc(&app, id)
        .cell_address(doc(&app, id).text_selection().unwrap().head.block)
        .unwrap()
        .is_none());
    assert_eq!(doc(&app, id).revision(), revision);
    select_cell(&mut app, view, id, grid[0][0]);
    key(
        &mut app,
        NamedKey::Tab,
        Modifiers {
            shift: true,
            ..Modifiers::new()
        },
    );
    assert!(doc(&app, id)
        .cell_address(doc(&app, id).text_selection().unwrap().head.block)
        .unwrap()
        .is_none());
}
#[test]
fn table_pointer_uses_column_whitespace_and_border_shift_rectangle() {
    let (mut app, _, view, id) = setup();
    command(
        &mut app,
        view,
        StructuredCommand::Table {
            rows: 2,
            columns: 2,
            header: true,
        },
    );
    let grid = table(&app, id);
    let marks: Vec<_> = structured_marks(app.world(), view.editor)
        .into_iter()
        .filter(|m| matches!(m.kind, StructuredMarkKind::TableCell { .. }))
        .collect();
    assert_eq!(
        marks.len(),
        4,
        "view {:?} offset {:?} lines {:?} marks {:?}",
        app.world().get::<Rect>(view.editor),
        app.world().get::<Editor>(view.editor),
        app.world().get::<TextLayout>(view.editor).map(|l| &l.lines),
        marks
    );
    let click = |app: &mut App, x: f32, y: f32, shift: bool| {
        *app.world_mut().resource_mut::<Input>().unwrap() = Input {
            pointer: Pointer {
                position: [x, y],
                press_at: Some([x, y]),
                pressed: true,
                released: true,
                ..Default::default()
            },
            shift,
            ..Default::default()
        };
        app.update();
        *app.world_mut().resource_mut::<Input>().unwrap() = Input::default();
        app.update();
    };
    let r = marks[0].rect;
    click(&mut app, r.x + r.w - 10.0, r.y + r.h / 2.0, false);
    assert_eq!(
        doc(&app, id)
            .cell_address(doc(&app, id).text_selection().unwrap().head.block)
            .unwrap()
            .unwrap()
            .cell,
        grid[0][0]
    );
    let r = marks[0].rect;
    click(&mut app, r.x + 1.0, r.y + 1.0, false);
    let r = marks[3].rect;
    click(&mut app, r.x + 1.0, r.y + 1.0, true);
    assert_eq!(
        doc(&app, id).selection(),
        StructuredSelection::Cells {
            anchor: grid[0][0],
            head: grid[1][1]
        }
    );
    let buffer = app.world().get::<Buffer>(view.editor).unwrap();
    assert_eq!(
        buffer.anchor, buffer.caret,
        "cell rectangles must not masquerade as a scalar range"
    );
    assert_eq!(
        structured_marks(app.world(), view.editor)
            .iter()
            .filter(|m| matches!(m.kind, StructuredMarkKind::TableCell { selected: true, .. }))
            .count(),
        4
    );
}
#[test]
fn table_rectangle_rich_copy_paste_clear_undo_and_readonly() {
    let (mut app, _, view, id) = setup();
    command(
        &mut app,
        view,
        StructuredCommand::Table {
            rows: 2,
            columns: 2,
            header: true,
        },
    );
    let grid = table(&app, id);
    select_cell(&mut app, view, id, grid[0][0]);
    let d = doc_mut(&mut app, id);
    d.replace_selection("bold é", EditOrigin::Keyboard).unwrap();
    let p = d.text_selection().unwrap().head;
    let a = Position { offset: 0, ..p };
    d.set_selection(StructuredSelection::Text(TextSelection {
        anchor: a,
        head: p,
    }))
    .unwrap();
    d.set_marks(
        Marks {
            strong: true,
            ..Marks::default()
        },
        EditOrigin::Keyboard,
    )
    .unwrap();
    d.set_selection(StructuredSelection::Cells {
        anchor: grid[0][0],
        head: grid[0][0],
    })
    .unwrap();
    project(app.world_mut(), view.editor);
    app.update();
    chord(&mut app, "c");
    select_cell(&mut app, view, id, grid[1][1]);
    chord(&mut app, "v");
    let block = doc(&app, id).cell_first_block(grid[1][1]).unwrap();
    let b = doc(&app, id)
        .blocks()
        .iter()
        .find(|b| b.id == block)
        .unwrap();
    assert_eq!(b.text(), "bold é");
    assert!(b.runs[0].marks.strong);
    command(&mut app, view, StructuredCommand::SelectTable);
    let before = doc(&app, id).plain_text();
    key(&mut app, NamedKey::Delete, Modifiers::new());
    assert!(doc(&app, id)
        .blocks()
        .iter()
        .filter(|b| doc(&app, id).cell_address(b.id).unwrap().is_some())
        .all(|b| b.text().is_empty()));
    command(&mut app, view, StructuredCommand::Undo);
    assert_eq!(doc(&app, id).plain_text(), before);
    doc_mut(&mut app, id).set_readonly(true);
    let revision = doc(&app, id).revision();
    key(&mut app, NamedKey::Delete, Modifiers::new());
    assert!(view
        .command(app.world_mut(), StructuredCommand::DeleteRow)
        .is_err());
    assert_eq!(doc(&app, id).revision(), revision);
}
#[test]
fn table_column_edits_and_wrapped_arrow_navigation_share_geometry() {
    let (mut app, _, view, id) = setup();
    command(
        &mut app,
        view,
        StructuredCommand::Table {
            rows: 2,
            columns: 2,
            header: true,
        },
    );
    let grid = table(&app, id);
    select_cell(&mut app, view, id, grid[0][1]);
    key(&mut app, NamedKey::ArrowDown, Modifiers::new());
    assert_eq!(
        doc(&app, id)
            .cell_address(doc(&app, id).text_selection().unwrap().head.block)
            .unwrap()
            .unwrap()
            .cell,
        grid[1][1]
    );
    key(&mut app, NamedKey::ArrowUp, Modifiers::new());
    assert_eq!(
        doc(&app, id)
            .cell_address(doc(&app, id).text_selection().unwrap().head.block)
            .unwrap()
            .unwrap()
            .cell,
        grid[0][1]
    );
    command(&mut app, view, StructuredCommand::InsertColumn(true));
    assert_eq!(table(&app, id)[0].len(), 3);
    command(&mut app, view, StructuredCommand::DeleteColumn);
    assert_eq!(table(&app, id)[0].len(), 2);
    command(&mut app, view, StructuredCommand::Undo);
    assert_eq!(table(&app, id)[0].len(), 3);
}
#[test]
fn embedded_panel_is_retained_and_commits_once_into_parent_history() {
    let (mut app, root, view, id) = setup();
    let block = atom(&mut app, view, id);
    let panel =
        AtomicPanelHandle::mount(app.world_mut(), root, UiId(91), view.editor, block).unwrap();
    app.update();
    let repeated =
        AtomicPanelHandle::mount(app.world_mut(), root, UiId(91), view.editor, block).unwrap();
    assert_eq!(panel.root, repeated.root);
    panel.focus(app.world_mut()).unwrap();
    *app.world_mut().resource_mut::<Input>().unwrap() = Input {
        chars: "child only".into(),
        ..Default::default()
    };
    app.update();
    *app.world_mut().resource_mut::<Input>().unwrap() = Input::default();
    assert_eq!(
        doc(&app, id)
            .blocks()
            .iter()
            .find(|b| b.id == block)
            .unwrap()
            .copy_fallback(),
        "nested draft"
    );
    let draft = app
        .world()
        .get::<Buffer>(panel.editor)
        .unwrap()
        .text()
        .to_owned();
    panel.commit(app.world_mut()).unwrap();
    assert_eq!(
        doc(&app, id)
            .blocks()
            .iter()
            .find(|b| b.id == block)
            .unwrap()
            .copy_fallback(),
        draft
    );
    command(&mut app, view, StructuredCommand::Undo);
    assert_eq!(
        doc(&app, id)
            .blocks()
            .iter()
            .find(|b| b.id == block)
            .unwrap()
            .copy_fallback(),
        "nested draft"
    );
    assert_eq!(
        app.world().get::<Buffer>(panel.editor).unwrap().text(),
        "nested draft"
    );
    panel.unmount(app.world_mut());
    assert!(!app.world().is_alive(panel.editor));
}
#[test]
fn embedded_requests_reject_blur_policy_rebind_and_delete_undo_resurrection() {
    for scenario in 0..4 {
        let (mut app, root, view, id) = setup();
        let block = atom(&mut app, view, id);
        let panel =
            AtomicPanelHandle::mount(app.world_mut(), root, UiId(91), view.editor, block).unwrap();
        app.update();
        panel.focus(app.world_mut()).unwrap();
        let mount = *app.world().get::<AtomicMount>(panel.root).unwrap();
        let request = mount.request(app.world()).unwrap();
        match scenario {
            0 => {
                request_focus(app.world_mut(), view.editor);
            }
            1 => {
                doc_mut(&mut app, id).set_readonly(true);
                doc_mut(&mut app, id).set_readonly(false);
            }
            2 => {
                detach_structured(app.world_mut(), view.editor).unwrap();
                attach_structured(app.world_mut(), view.editor, id).unwrap();
            }
            _ => {
                let d = doc_mut(&mut app, id);
                d.set_selection(StructuredSelection::Node(block)).unwrap();
                d.delete_forward(EditOrigin::Host(0)).unwrap();
                d.undo().unwrap();
                assert!(mount.validate(app.world()).is_err());
            }
        }
        let revision = doc(&app, id).revision();
        assert!(request.commit(app.world_mut(), payload("STALE")).is_err());
        assert_eq!(doc(&app, id).revision(), revision);
    }
}
#[test]
fn embedded_foreign_intent_has_no_side_effect_and_child_escape_cannot_leak_text() {
    let (mut app, root, view, id) = setup();
    let block = atom(&mut app, view, id);
    let panel =
        AtomicPanelHandle::mount(app.world_mut(), root, UiId(91), view.editor, block).unwrap();
    app.update();
    panel.focus(app.world_mut()).unwrap();
    let revision = doc(&app, id).revision();
    let intent = panel.foreign_intent(app.world()).unwrap();
    assert_eq!(intent.block, block);
    assert_eq!(doc(&app, id).revision(), revision);
    *app.world_mut().resource_mut::<Input>().unwrap() = Input {
        keys: vec![
            KeyEvent::named(NamedKey::Escape, Modifiers::new()),
            KeyEvent::character('x', Modifiers::new()),
        ],
        ..Default::default()
    };
    app.update();
    assert_eq!(doc(&app, id).revision(), revision);
    assert!(!app.world().is_alive(panel.editor));
    assert_eq!(
        app.world().resource::<Focus>().unwrap().entity,
        Some(view.editor)
    );
}
#[test]
fn foreign_child_cannot_commit_parent_even_through_async_token() {
    let (mut app, root, view, id) = setup();
    let block = atom(&mut app, view, id);
    let panel =
        AtomicPanelHandle::mount_foreign(app.world_mut(), root, UiId(92), view.editor, block)
            .unwrap();
    app.update();
    panel.focus(app.world_mut()).unwrap();
    let mount = *app.world().get::<AtomicMount>(panel.root).unwrap();
    let revision = doc(&app, id).revision();
    assert!(panel.commit(app.world_mut()).is_err());
    let intent = panel.foreign_intent(app.world()).unwrap();
    assert!(intent
        .request
        .commit(app.world_mut(), payload("forbidden"))
        .is_err());
    assert_eq!(doc(&app, id).revision(), revision);
    assert!(mount.validate(app.world()).is_ok());
}
#[test]
fn child_deletion_revokes_requests_closes_detached_popup_and_repairs_focus() {
    let (mut app, root, view, id) = setup();
    let block = atom(&mut app, view, id);
    let panel =
        AtomicPanelHandle::mount(app.world_mut(), root, UiId(92), view.editor, block).unwrap();
    app.update();
    panel.focus(app.world_mut()).unwrap();
    let mount = *app.world().get::<AtomicMount>(panel.root).unwrap();
    let request = mount.request(app.world()).unwrap();
    let popup = app.world_mut().spawn((
        Layout::column(),
        AnchoredPopup {
            anchor: panel.editor,
            max_height: 80.0,
            closed: |_, _| {},
        },
        Hidden,
    ));
    assert!(open_popup(app.world_mut(), popup));
    doc_mut(&mut app, id)
        .set_selection(StructuredSelection::Node(block))
        .unwrap();
    command(&mut app, view, StructuredCommand::DeleteBlock);
    assert!(!app.world().is_alive(popup));
    assert!(!app.world().is_alive(panel.editor));
    assert_eq!(
        app.world().resource::<Focus>().unwrap().entity,
        Some(view.editor)
    );
    assert!(request.commit(app.world_mut(), payload("late")).is_err());
}
#[test]
fn nested_child_wheel_and_drag_never_move_parent_selection_or_viewport() {
    let (mut app, root, view, id) = setup();
    let block = atom(&mut app, view, id);
    let panel =
        AtomicPanelHandle::mount(app.world_mut(), root, UiId(92), view.editor, block).unwrap();
    app.update();
    panel.focus(app.world_mut()).unwrap();
    let mount = *app.world().get::<AtomicMount>(panel.root).unwrap();
    mount
        .reset_draft(app.world_mut(), &"long child row\n".repeat(100))
        .unwrap();
    app.update();
    let parent_selection = doc(&app, id).selection();
    let parent_offset = app.world().get::<Editor>(view.editor).unwrap().offset;
    let rect = *app.world().get::<Rect>(panel.editor).unwrap();
    *app.world_mut().resource_mut::<Input>().unwrap() = Input {
        pointer: Pointer {
            position: [rect.x + 30.0, rect.y + 30.0],
            ..Default::default()
        },
        scroll: [0.0, -72.0],
        ..Default::default()
    };
    app.update();
    assert!(app.world().get::<Editor>(panel.editor).unwrap().offset > 0.0);
    *app.world_mut().resource_mut::<Input>().unwrap() = Input {
        pointer: Pointer {
            position: [rect.x + 30.0, rect.y + 30.0],
            press_at: Some([rect.x + 10.0, rect.y + 20.0]),
            pressed: true,
            down: true,
            ..Default::default()
        },
        ..Default::default()
    };
    app.update();
    *app.world_mut().resource_mut::<Input>().unwrap() = Input {
        pointer: Pointer {
            position: [rect.x + 80.0, rect.y + 60.0],
            released: true,
            ..Default::default()
        },
        ..Default::default()
    };
    app.update();
    assert_eq!(doc(&app, id).selection(), parent_selection);
    assert_eq!(
        app.world().get::<Editor>(view.editor).unwrap().offset,
        parent_offset
    );
}
#[test]
fn embedded_ime_readonly_and_deleted_atom_teardown() {
    let (mut app, root, view, id) = setup();
    let block = atom(&mut app, view, id);
    let panel =
        AtomicPanelHandle::mount(app.world_mut(), root, UiId(91), view.editor, block).unwrap();
    app.update();
    panel.focus(app.world_mut()).unwrap();
    *app.world_mut().resource_mut::<Input>().unwrap() = Input {
        ime_preedit: "仮".into(),
        ime_enabled: true,
        ..Default::default()
    };
    app.update();
    assert!(panel.commit(app.world_mut()).is_err());
    doc_mut(&mut app, id).set_readonly(true);
    *app.world_mut().resource_mut::<Input>().unwrap() = Input::default();
    app.update();
    assert!(!app.world().is_alive(panel.editor));
    assert!(!app.world().is_alive(panel.root));
    assert_eq!(
        doc(&app, id)
            .blocks()
            .iter()
            .find(|b| b.id == block)
            .unwrap()
            .copy_fallback(),
        "nested draft"
    );
}
