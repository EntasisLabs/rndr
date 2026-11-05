use rndr_core::editing::EditOrigin;
use rndr_core::structured::{ContainerKind, ListKind, Operation, StructuredDocument, Transaction};
use rndr_core::structured_chrome::{
    mark_rectangles, structured_marks, StructuredChromeGutter, StructuredMarkKind,
};
use rndr_core::structured_native::{
    attach_structured, detach_structured, project, StructuredDocuments,
};
use rndr_core::*;

fn setup(kind: ContainerKind) -> (App, Entity, rndr_core::structured_native::StructuredHandle) {
    setup_text(kind, "alpha\nbeta")
}

fn setup_text(
    kind: ContainerKind,
    text: &str,
) -> (App, Entity, rndr_core::structured_native::StructuredHandle) {
    let mut app = App::new();
    app.add_plugin(Chrome);
    app.world_mut().insert_resource(Viewport::new(400., 240.));
    let mut doc = StructuredDocument::new(text);
    let a = doc
        .position_at(0, rndr_core::editing::Affinity::After)
        .unwrap();
    let b = doc
        .position_at(text.len(), rndr_core::editing::Affinity::After)
        .unwrap();
    doc.apply(Transaction {
        document: doc.id(),
        base: doc.revision(),
        origin: EditOrigin::Host(1),
        group: None,
        operations: vec![Operation::Wrap {
            range: rndr_core::structured::TextSelection { anchor: a, head: b },
            kind,
        }],
    })
    .unwrap();
    let mut docs = StructuredDocuments::default();
    let handle = docs.insert(doc);
    app.world_mut().insert_resource(docs);
    let root = app.world_mut().spawn((Layout {
        width: Size::Px(400.),
        height: Size::Px(240.),
        ..Layout::row()
    },));
    let gutter = app.world_mut().spawn((Layout {
        width: Size::Px(100.),
        height: Size::Px(220.),
        ..Layout::column()
    },));
    let view = app.world_mut().spawn((
        Layout {
            width: Size::Px(280.),
            height: Size::Px(220.),
            ..Layout::column()
        },
        Buffer::from_str(""),
        Editor::default(),
        Label::new(""),
        StructuredChromeGutter { width: 100. },
    ));
    set_parent(app.world_mut(), gutter, root);
    set_parent(app.world_mut(), view, root);
    attach_structured(app.world_mut(), view, handle).unwrap();
    app.update();
    app.update();
    (app, view, handle)
}

#[test]
fn ordered_markers_are_typed_numbered_and_painted_before_indented_text() {
    let (app, view, _) = setup(ContainerKind::List(ListKind::Ordered { start: 9 }));
    let marks = structured_marks(app.world(), view);
    assert_eq!(
        marks.iter().map(|m| m.kind).collect::<Vec<_>>(),
        vec![
            StructuredMarkKind::Ordered(9),
            StructuredMarkKind::Ordered(10)
        ]
    );
    let rect = app.world().get::<Rect>(view).unwrap();
    let quads = rndr_paint::paint(app.world());
    for mark in marks {
        let display = rndr_core::structured_native::structured_display(app.world(), view).unwrap();
        let block = display
            .blocks
            .iter()
            .find(|b| b.block == mark.block)
            .unwrap();
        let (text_x, _) = app
            .world()
            .get::<TextLayout>(view)
            .unwrap()
            .caret_xy(block.range.start);
        assert!(mark.rect.x >= rect.x);
        assert!(mark.rect.x + mark.rect.w < rect.x + TEXT_ORIGIN_X + text_x);
        assert!(mark.clip.x < rect.x);
        assert!(mark.rect.y >= rect.y);
        for r in mark_rectangles(&mark) {
            assert!(quads.iter().any(|q| (q.rect.x - r.x).abs() < 1.0
                && (q.rect.y - r.y).abs() < 1.0
                && q.clip == mark.clip));
        }
    }
}

#[test]
fn stale_buffer_revision_label_layout_and_detach_suppress_markers() {
    let (mut app, view, handle) = setup(ContainerKind::List(ListKind::Task));
    assert_eq!(structured_marks(app.world(), view).len(), 2);
    app.world_mut().insert(view, Buffer::from_str("fake"));
    assert!(structured_marks(app.world(), view).is_empty());
    project(app.world_mut(), view);
    app.update();
    app.update();
    assert_eq!(structured_marks(app.world(), view).len(), 2);
    app.world_mut().get_mut::<Label>(view).unwrap().text = "unshaped".into();
    assert!(structured_marks(app.world(), view).is_empty());
    project(app.world_mut(), view);
    app.update();
    app.update();
    app.world_mut().get_mut::<Rect>(view).unwrap().w += 10.;
    assert!(structured_marks(app.world(), view).is_empty());
    app.update();
    app.update();
    let doc = app
        .world_mut()
        .resource_mut::<StructuredDocuments>()
        .unwrap()
        .get_mut(handle)
        .unwrap();
    let block = doc.blocks()[0].id;
    doc.apply(Transaction {
        document: doc.id(),
        base: doc.revision(),
        origin: EditOrigin::Host(1),
        group: None,
        operations: vec![Operation::ToggleTask { block }],
    })
    .unwrap();
    assert!(structured_marks(app.world(), view).is_empty());
    project(app.world_mut(), view);
    app.update();
    app.update();
    assert_eq!(
        structured_marks(app.world(), view)[0].kind,
        StructuredMarkKind::Task(true)
    );
    detach_structured(app.world_mut(), view).unwrap();
    assert!(structured_marks(app.world(), view).is_empty());
}

#[test]
fn reservation_is_required_and_quotes_have_real_bars() {
    let (mut app, view, _) = setup(ContainerKind::Quote);
    let marks = structured_marks(app.world(), view);
    assert_eq!(marks.len(), 2);
    assert!(marks
        .iter()
        .all(|m| m.kind == StructuredMarkKind::Quote && mark_rectangles(m).len() == 1));
    app.world_mut().remove::<StructuredChromeGutter>(view);
    assert!(structured_marks(app.world(), view).is_empty());
}

#[test]
fn nested_list_indentation_and_bullet_geometry_follow_containers() {
    let (mut app, view, handle) = setup(ContainerKind::List(ListKind::Bullet));
    let original = structured_marks(app.world(), view);
    assert_eq!(original[0].kind, StructuredMarkKind::Bullet);
    let doc = app
        .world_mut()
        .resource_mut::<StructuredDocuments>()
        .unwrap()
        .get_mut(handle)
        .unwrap();
    let block = doc.blocks()[1].id;
    doc.apply(Transaction {
        document: doc.id(),
        base: doc.revision(),
        origin: EditOrigin::Host(2),
        group: None,
        operations: vec![Operation::Indent { block }],
    })
    .unwrap();
    project(app.world_mut(), view);
    app.update();
    app.update();
    let marks = structured_marks(app.world(), view);
    assert_eq!(marks.len(), 2);
    assert!(marks[1].rect.x > marks[0].rect.x);
    assert_eq!(mark_rectangles(&marks[0]).len(), 1);
    let layout = app.world().get::<TextLayout>(view).unwrap();
    assert_eq!(layout.caret_xy(0).0, 24.0);
    assert_eq!(layout.caret_xy(6).0, 48.0);
    let (x, y) = layout.caret_xy(6);
    assert_eq!(layout.index_at(x, y + 1.0), 6);
    assert_eq!(layout.selection_boxes(6, 10)[0].x, x);
    assert_eq!(
        app.world().get::<Buffer>(view).unwrap().text(),
        "alpha\nbeta"
    );
    // Undo changes only structure, not bytes: it must still invalidate shaping.
    app.world_mut()
        .resource_mut::<StructuredDocuments>()
        .unwrap()
        .get_mut(handle)
        .unwrap()
        .undo()
        .unwrap();
    project(app.world_mut(), view);
    app.update();
    assert_eq!(
        app.world().get::<TextLayout>(view).unwrap().caret_xy(6).0,
        24.0
    );
}

#[test]
fn wrapped_rows_and_empty_selection_keep_their_structural_origin() {
    let text = "alpha\nlong task text wraps onto several visual rows here\n";
    let (mut app, view, _) = setup_text(ContainerKind::List(ListKind::Task), text);
    app.world_mut().get_mut::<Layout>(view).unwrap().width = Size::Px(150.0);
    app.update();
    app.update();
    let layout = app.world().get::<TextLayout>(view).unwrap();
    let task_rows: Vec<_> = layout
        .lines
        .iter()
        .filter(|l| l.start >= 6 && l.start < text.len())
        .collect();
    assert!(task_rows.len() > 1);
    for line in task_rows {
        assert_eq!(line.edges[0].1, 24.0);
        assert_eq!(layout.index_at(24.0, line.y + 1.0), line.start);
        assert!(line.edges.iter().all(|(_, x)| *x <= 126.01));
    }
    let boxes = layout.selection_boxes(0, text.len() + 1);
    assert!(boxes.iter().all(|b| b.x >= 24.0));
    assert_eq!(boxes.last().unwrap().w, 2.0);
    assert_eq!(layout.caret_xy(text.len()).0, 24.0);
}

#[test]
fn horizontal_rule_uses_atomic_node_metadata_and_editable_width() {
    let (mut app, view, handle) = setup(ContainerKind::Quote);
    let doc = app
        .world_mut()
        .resource_mut::<StructuredDocuments>()
        .unwrap()
        .get_mut(handle)
        .unwrap();
    let at = doc
        .position_at(0, rndr_core::editing::Affinity::After)
        .unwrap();
    doc.apply(Transaction {
        document: doc.id(),
        base: doc.revision(),
        origin: EditOrigin::Host(3),
        group: None,
        operations: vec![Operation::InsertHorizontalRule { at }],
    })
    .unwrap();
    project(app.world_mut(), view);
    app.update();
    app.update();
    app.world_mut().get_mut::<Editor>(view).unwrap().offset = 0.0;
    app.update();
    app.update();
    let marks = structured_marks(app.world(), view);
    let hr = marks
        .iter()
        .find(|m| m.kind == StructuredMarkKind::HorizontalRule)
        .unwrap();
    let rect = app.world().get::<Rect>(view).unwrap();
    assert_eq!(hr.rect.w, rect.w - 2.0 * TEXT_ORIGIN_X - 24.0);
    assert_eq!(hr.rect.x, rect.x + TEXT_ORIGIN_X + 24.0);
    assert_eq!(mark_rectangles(hr), vec![hr.rect]);
}
