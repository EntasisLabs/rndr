use rndr_core::editing::{Affinity, EditOrigin};
use rndr_core::structured::{
    AtomicPayload, BlockKind, ContainerKind, Operation, Position, StructuredDocument,
    TextSelection, Transaction,
};
use rndr_core::structured_chrome::{
    mark_rectangles, structured_marks, StructuredChromeGutter, StructuredMarkKind,
};
use rndr_core::structured_native::{
    attach_structured, project, structured_display, StructuredDocuments, StructuredHandle,
};
use rndr_core::*;

fn apply(doc: &mut StructuredDocument, operations: Vec<Operation>) {
    doc.apply(Transaction {
        document: doc.id(),
        base: doc.revision(),
        origin: EditOrigin::Host(1),
        group: None,
        operations,
    })
    .unwrap();
}
fn table() -> StructuredDocument {
    let mut doc = StructuredDocument::new("");
    let at = doc.position_at(0, Affinity::After).unwrap();
    apply(
        &mut doc,
        vec![Operation::InsertTable {
            at,
            rows: 2,
            columns: 2,
            header: true,
        }],
    );
    let ids: Vec<_> = doc
        .blocks()
        .iter()
        .filter(|b| {
            doc.ancestors(b.id)
                .unwrap()
                .iter()
                .any(|c| matches!(c.kind, ContainerKind::Cell { .. }))
        })
        .map(|b| b.id)
        .collect();
    for (id, text) in ids.into_iter().zip([
        "long cell wraps over many visual rows long cell",
        "β",
        "third",
        "fourth",
    ]) {
        let at = Position {
            block: id,
            offset: 0,
            affinity: Affinity::After,
        };
        apply(
            &mut doc,
            vec![Operation::Replace {
                range: TextSelection {
                    anchor: at,
                    head: at,
                },
                text: text.into(),
            }],
        );
    }
    doc
}
fn setup(doc: StructuredDocument) -> (App, Entity, StructuredHandle) {
    let mut app = App::new();
    app.add_plugin(Chrome);
    app.world_mut().insert_resource(Viewport::new(400., 600.));
    let mut docs = StructuredDocuments::default();
    let handle = docs.insert(doc);
    app.world_mut().insert_resource(docs);
    let view = app.world_mut().spawn((
        Layout {
            width: Size::Px(280.),
            height: Size::Px(580.),
            ..Layout::column()
        },
        Buffer::from_str(""),
        Editor::default(),
        Label::new(""),
        StructuredChromeGutter { width: 24. },
    ));
    attach_structured(app.world_mut(), view, handle).unwrap();
    app.update();
    app.update();
    (app, view, handle)
}
#[test]
fn table_cells_share_row_origin_wrap_locally_and_advance_by_tallest_cell() {
    let (app, view, _) = setup(table());
    let marks: Vec<_> = structured_marks(app.world(), view)
        .into_iter()
        .filter(|m| matches!(m.kind, StructuredMarkKind::TableCell { .. }))
        .collect();
    assert_eq!(marks.len(), 4);
    assert_eq!(marks[0].rect.y, marks[1].rect.y);
    assert_eq!(marks[0].rect.h, marks[1].rect.h);
    assert_eq!(marks[0].rect.w, marks[1].rect.w);
    assert_eq!(marks[0].rect.x + marks[0].rect.w, marks[1].rect.x);
    assert!(marks[2].rect.y >= marks[0].rect.y + marks[0].rect.h - 0.1);
    assert!(marks[0].rect.h > marks[2].rect.h);
    let layout = app.world().get::<TextLayout>(view).unwrap();
    let display = structured_display(app.world(), view).unwrap();
    for mark in &marks {
        let block = display
            .blocks
            .iter()
            .find(|b| b.block == mark.block)
            .unwrap();
        let (x, y) = layout.caret_xy(block.range.start);
        let rect = app.world().get::<Rect>(view).unwrap();
        assert!(rect.x + TEXT_ORIGIN_X + x > mark.rect.x);
        assert!(rect.y + TEXT_ORIGIN_Y + y > mark.rect.y);
        for selected in layout.selection_boxes(block.range.start, block.range.end) {
            assert!(
                rect.x + TEXT_ORIGIN_X + selected.x + selected.w <= mark.rect.x + mark.rect.w + 0.1
            );
        }
        assert_eq!(mark_rectangles(mark).len(), 4);
    }
    assert!(layout.lines.windows(2).all(|w| w[0].start <= w[1].start));
    assert!(layout.lines.windows(2).any(|w| w[0].y > w[1].y));
}
#[test]
fn column_insertion_invalidates_shape_without_requiring_text_changes() {
    let doc = table();
    let table_id = doc
        .containers()
        .iter()
        .find(|c| c.kind == ContainerKind::Table)
        .unwrap()
        .id;
    let (mut app, view, handle) = setup(doc);
    let old_width = structured_marks(app.world(), view)
        .into_iter()
        .find(|m| matches!(m.kind, StructuredMarkKind::TableCell { .. }))
        .unwrap()
        .rect
        .w;
    let doc = app
        .world_mut()
        .resource_mut::<StructuredDocuments>()
        .unwrap()
        .get_mut(handle)
        .unwrap();
    apply(
        doc,
        vec![Operation::InsertColumn {
            table: table_id,
            index: 1,
        }],
    );
    project(app.world_mut(), view);
    app.update();
    app.update();
    let new_width = structured_marks(app.world(), view)
        .into_iter()
        .find(|m| matches!(m.kind, StructuredMarkKind::TableCell { .. }))
        .unwrap()
        .rect
        .w;
    assert!(new_width < old_width);
    let marks: Vec<_> = structured_marks(app.world(), view)
        .into_iter()
        .filter(|m| matches!(m.kind, StructuredMarkKind::TableCell { .. }))
        .collect();
    assert_eq!(marks.len(), 6);
    assert_eq!(marks[0].rect.w, marks[2].rect.w);
    assert_eq!(marks[0].rect.y, marks[2].rect.y);
}
#[test]
fn atomic_cards_reserve_real_height_and_have_inert_outline() {
    let mut doc = StructuredDocument::new("beforeafter");
    let at = doc.position_at(6, Affinity::After).unwrap();
    apply(
        &mut doc,
        vec![Operation::InsertAtomic {
            at,
            payload: AtomicPayload {
                kind: "image".into(),
                fallback: "offline image".into(),
                attributes: Default::default(),
            },
        }],
    );
    let (app, view, _) = setup(doc);
    let mark = structured_marks(app.world(), view)
        .into_iter()
        .find(|m| m.kind == StructuredMarkKind::Atomic)
        .unwrap();
    assert!(mark.rect.h >= 64.);
    assert_eq!(mark_rectangles(&mark).len(), 4);
    let display = structured_display(app.world(), view).unwrap();
    let atomic = display
        .blocks
        .iter()
        .position(|b| b.kind == BlockKind::Atomic)
        .unwrap();
    let layout = app.world().get::<TextLayout>(view).unwrap();
    let (_, y) = layout.caret_xy(display.blocks[atomic + 1].range.start);
    let rect = app.world().get::<Rect>(view).unwrap();
    assert!(rect.y + TEXT_ORIGIN_Y + y >= mark.rect.y + mark.rect.h);
    // Both the secondary type label and fallback are inert, padded glyphs.
    let glyphs: Vec<_> = layout
        .glyphs
        .iter()
        .filter(|g| g.start == display.blocks[atomic].range.start && g.end == g.start)
        .collect();
    assert!(!glyphs.is_empty());
    assert!(glyphs.iter().any(|g| g.color.is_some()));
    for g in glyphs {
        assert!(rect.x + TEXT_ORIGIN_X + g.x >= mark.rect.x + 12.0);
        assert!(rect.y + TEXT_ORIGIN_Y + g.y >= mark.rect.y + 12.0);
        assert!(rect.x + TEXT_ORIGIN_X + g.x + g.w <= mark.rect.x + mark.rect.w - 12.0);
        assert!(rect.y + TEXT_ORIGIN_Y + g.y + g.h <= mark.rect.y + mark.rect.h - 12.0);
    }
}

#[test]
fn quiet_headers_keep_thin_boundaries_and_semantic_weight() {
    use rndr_core::structured_chrome::{chrome_paint, chrome_palette};
    let (app, view, _) = setup(table());
    let palette = chrome_palette(app.world(), view);
    assert_ne!(palette.border, theme_of(app.world()).text);
    let marks = structured_marks(app.world(), view);
    let header = marks
        .iter()
        .find(|m| matches!(m.kind, StructuredMarkKind::TableCell { header: true, .. }))
        .unwrap();
    assert_eq!(mark_rectangles(header)[0].h, 1.0);
    assert!(chrome_paint(app.world(), view)
        .iter()
        .any(|p| p.rect == header.rect && p.color == palette.header));
    let layout = app.world().get::<TextLayout>(view).unwrap();
    let spans = app.world().get::<Spans>(view).unwrap();
    assert_eq!(spans.runs[0].weight, Some(600));
    assert!(spans.runs.iter().any(|s| s.weight == Some(400)));
    assert!(layout.glyphs.iter().any(|g| g.weight == 600));
}

#[test]
fn palette_follows_surface_and_local_overrides_win_without_history() {
    use rndr_core::structured_chrome::{chrome_palette, StructuredChromeStyle};
    let (mut app, view, handle) = setup(table());
    let before = {
        let doc = app
            .world()
            .resource::<StructuredDocuments>()
            .unwrap()
            .get(handle)
            .unwrap();
        (doc.plain_text(), doc.revision(), doc.selection())
    };
    app.world_mut().insert(view, Color::rgb(1.0, 1.0, 1.0));
    app.world_mut().get_mut::<Label>(view).unwrap().color = Some(Color::rgb(0.1, 0.1, 0.1));
    let light = chrome_palette(app.world(), view);
    app.world_mut().insert(view, Color::rgb(0.1, 0.1, 0.1));
    app.world_mut().get_mut::<Label>(view).unwrap().color = Some(Color::rgb(0.9, 0.9, 0.9));
    let dark = chrome_palette(app.world(), view);
    assert!(light.surface.r > 0.9 && dark.surface.r < 0.2);
    let global = Color::rgb(0.2, 0.4, 0.6);
    let local = Color::rgb(0.4, 0.6, 0.2);
    app.world_mut().insert_resource(StructuredChromeStyle {
        border: Some(global),
        ..Default::default()
    });
    app.world_mut().insert(
        view,
        StructuredChromeStyle {
            border: Some(local),
            radius: Some(f32::NAN),
            ..Default::default()
        },
    );
    assert_eq!(chrome_palette(app.world(), view).border, local);
    assert_eq!(chrome_palette(app.world(), view).radius, 6.0);
    let after = app
        .world()
        .resource::<StructuredDocuments>()
        .unwrap()
        .get(handle)
        .unwrap();
    assert_eq!(before.0, after.plain_text());
    assert_eq!(before.1, after.revision());
    assert_eq!(before.2, after.selection());
}
