use rndr_core::editing::{attach_source, SourceDocuments, SourceSelection};
use rndr_core::source_selection::{rectangular_selections, RectangleRow, SourceSelections};
use rndr_core::{
    App, Buffer, CapturesKeys, Chrome, Editor, Focus, Input, KeyEvent, Label, Modifiers, Platform,
};

fn boot() -> (App, rndr_core::editing::DocumentId, rndr_core::Entity) {
    let mut app = App::new();
    app.add_plugin(Chrome);
    app.world_mut().insert_resource(Platform::Windows);
    app.world_mut().insert_resource(Focus::default());
    let mut docs = SourceDocuments::default();
    let id = docs.open("ab\ncd");
    app.world_mut().insert_resource(docs);
    let view = app.world_mut().spawn((
        Buffer::from_str(""),
        Editor::default(),
        Label::new(""),
        CapturesKeys,
    ));
    attach_source(app.world_mut(), view, id).unwrap();
    app.world_mut().resource_mut::<Focus>().unwrap().entity = Some(view);
    app.update();
    (app, id, view)
}
fn key(app: &mut App, ch: char, command: bool) {
    let mut modifiers = Modifiers::new();
    modifiers.ctrl = command;
    app.world_mut()
        .resource_mut::<Input>()
        .unwrap()
        .keys
        .push(KeyEvent::character(ch, modifiers));
    app.update();
}
fn set(
    app: &mut App,
    id: rndr_core::editing::DocumentId,
    view: rndr_core::Entity,
) -> SourceSelections {
    let doc = app
        .world_mut()
        .resource_mut::<SourceDocuments>()
        .unwrap()
        .get_mut(id)
        .unwrap();
    let selections = SourceSelections::new(
        doc.text(),
        vec![
            SourceSelection { anchor: 1, head: 1 },
            SourceSelection { anchor: 4, head: 4 },
        ],
        1,
    )
    .unwrap();
    doc.set_view_selections(view, doc.revision(), selections.clone())
        .unwrap();
    selections
}
#[test]
fn chrome_native_queue_types_and_undo_restores_all_carets() {
    let (mut app, id, view) = boot();
    let before = set(&mut app, id, view);
    key(&mut app, 'X', false);
    assert_eq!(app.world().get::<Buffer>(view).unwrap().text(), "aXb\ncXd");
    let doc = app
        .world()
        .resource::<SourceDocuments>()
        .unwrap()
        .get(id)
        .unwrap();
    assert_eq!(doc.selections().ranges().len(), 2);
    assert_eq!(app.world().get::<Buffer>(view).unwrap().caret, 6);
    key(&mut app, 'z', true);
    assert_eq!(app.world().get::<Buffer>(view).unwrap().text(), "ab\ncd");
    let doc = app
        .world()
        .resource::<SourceDocuments>()
        .unwrap()
        .get(id)
        .unwrap();
    assert_eq!(doc.view_selections(view), Some(&before));
    key(&mut app, 'y', true);
    assert_eq!(app.world().get::<Buffer>(view).unwrap().text(), "aXb\ncXd");
}
#[test]
fn native_ime_preedit_explicitly_collapses_secondary_carets() {
    let (mut app, id, view) = boot();
    set(&mut app, id, view);
    let input = app.world_mut().resource_mut::<Input>().unwrap();
    input.ime_enabled = true;
    input.ime_preedit = "日".into();
    app.update();
    let doc = app
        .world()
        .resource::<SourceDocuments>()
        .unwrap()
        .get(id)
        .unwrap();
    assert!(doc.is_composing());
    assert_eq!(
        doc.selections().ranges(),
        &[SourceSelection { anchor: 4, head: 4 }]
    );
    let input = app.world_mut().resource_mut::<Input>().unwrap();
    input.ime_preedit.clear();
    input.ime_commit = "日本".into();
    app.update();
    assert_eq!(
        app.world().get::<Buffer>(view).unwrap().text(),
        "ab\nc日本d"
    );
    key(&mut app, 'z', true);
    assert_eq!(app.world().get::<Buffer>(view).unwrap().text(), "ab\ncd");
}
#[test]
fn shaped_rectangle_command_uses_actual_window_geometry() {
    let (mut app, id, view) = boot();
    app.world_mut().insert(
        view,
        rndr_core::Layout {
            width: rndr_core::Size::Px(160.0),
            height: rndr_core::Size::Px(80.0),
            ..rndr_core::Layout::column()
        },
    );
    app.update();
    let snapshot = rndr_core::access_text(app.world(), view).unwrap();
    let first = &snapshot.layout.lines[0];
    let last = &snapshot.layout.lines[1];
    let x = first.edges.iter().find(|(b, _)| *b == 1).unwrap().1;
    let anchor = [
        snapshot.origin.0,
        snapshot.origin.1 + first.y + first.h / 2.0,
    ];
    let head = [
        snapshot.origin.0 + x,
        snapshot.origin.1 + last.y + last.h / 2.0,
    ];
    rndr_core::source_selection::select_source_rectangle(app.world_mut(), view, anchor, head)
        .unwrap();
    let set = app
        .world()
        .resource::<SourceDocuments>()
        .unwrap()
        .get(id)
        .unwrap()
        .view_selections(view)
        .unwrap();
    assert_eq!(
        set.ranges(),
        &[
            SourceSelection { anchor: 0, head: 1 },
            SourceSelection { anchor: 3, head: 4 }
        ]
    );
    key(&mut app, 'X', false);
    assert_eq!(app.world().get::<Buffer>(view).unwrap().text(), "Xb\nXd");
    key(&mut app, 'z', true);
    assert_eq!(app.world().get::<Buffer>(view).unwrap().text(), "ab\ncd");
}
#[test]
fn rectangular_visual_rows_drive_native_replacement() {
    let (mut app, id, view) = boot();
    let doc = app
        .world_mut()
        .resource_mut::<SourceDocuments>()
        .unwrap()
        .get_mut(id)
        .unwrap();
    let rows = [
        RectangleRow {
            stops: vec![(0., 0), (1., 1), (2., 2)],
        },
        RectangleRow {
            stops: vec![(0., 3), (1., 4), (2., 5)],
        },
    ];
    let selections = rectangular_selections(doc.text(), &rows, 0., 1., 0).unwrap();
    doc.set_view_selections(view, doc.revision(), selections)
        .unwrap();
    key(&mut app, 'X', false);
    assert_eq!(app.world().get::<Buffer>(view).unwrap().text(), "Xb\nXd");
    key(&mut app, 'z', true);
    assert_eq!(app.world().get::<Buffer>(view).unwrap().text(), "ab\ncd");
}
