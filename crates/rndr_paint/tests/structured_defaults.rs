use rndr_core::editing::{Affinity, EditOrigin};
use rndr_core::structured::{
    AtomicPayload, Operation, StructuredDocument, StructuredSelection, Transaction,
};
use rndr_core::structured_chrome::{
    chrome_paint, chrome_palette, structured_marks, StructuredChromeGutter, StructuredMarkKind,
};
use rndr_core::structured_native::{attach_structured, project, StructuredDocuments};
use rndr_core::*;

#[test]
fn rounded_card_layers_precede_caption_and_node_selection_has_no_scalar_caret() {
    let mut app = App::new();
    app.add_plugin(Chrome);
    app.world_mut().insert_resource(Viewport::new(400.0, 400.0));
    let mut doc = StructuredDocument::new("before\nafter");
    let at = doc.position_at(0, Affinity::After).unwrap();
    doc.apply(Transaction {
        document: doc.id(),
        base: doc.revision(),
        origin: EditOrigin::Host(1),
        group: None,
        operations: vec![Operation::InsertAtomic {
            at,
            payload: AtomicPayload {
                kind: "image".into(),
                fallback: "Alt text".into(),
                attributes: Default::default(),
            },
        }],
    })
    .unwrap();
    let block = doc
        .blocks()
        .iter()
        .find(|b| b.atomic_payload().is_some())
        .unwrap()
        .id;
    doc.set_selection(StructuredSelection::Node(block)).unwrap();
    let mut docs = StructuredDocuments::default();
    let handle = docs.insert(doc);
    app.world_mut().insert_resource(docs);
    let view = app.world_mut().spawn((
        Layout {
            width: Size::Px(360.0),
            height: Size::Px(380.0),
            ..Layout::column()
        },
        Buffer::from_str(""),
        Editor::default(),
        Label::new(""),
        StructuredChromeGutter { width: 4.0 },
        Color::rgb(1.0, 1.0, 1.0),
    ));
    attach_structured(app.world_mut(), view, handle).unwrap();
    app.world_mut()
        .insert_resource(Focus { entity: Some(view) });
    for selected in [true, false] {
        if !selected {
            let doc = app
                .world_mut()
                .resource_mut::<StructuredDocuments>()
                .unwrap()
                .get_mut(handle)
                .unwrap();
            let at = doc.position_at(0, Affinity::After).unwrap();
            doc.set_selection(StructuredSelection::Text(
                rndr_core::structured::TextSelection {
                    anchor: at,
                    head: at,
                },
            ))
            .unwrap();
            project(app.world_mut(), view);
        }
        app.update();
        app.update();
        let palette = chrome_palette(app.world(), view);
        let layers = chrome_paint(app.world(), view);
        assert_eq!(layers.len(), 2);
        assert!(layers.iter().all(|l| l.radius > 0.0));
        assert_eq!(
            layers[0].color,
            if selected {
                palette.accent
            } else {
                palette.border
            }
        );
        let marks = structured_marks(app.world(), view);
        let card = marks
            .iter()
            .find(|m| m.kind == StructuredMarkKind::Atomic)
            .unwrap();
        let quads = rndr_paint::paint(app.world());
        let background = quads
            .iter()
            .position(|q| q.color == layers[1].color && q.radius > 0.0)
            .unwrap();
        let first_glyph = quads
            .iter()
            .position(|q| q.uv != rndr_paint::solid_uv())
            .unwrap();
        assert!(
            background < first_glyph,
            "surface must not obscure the caption"
        );
        if selected {
            assert!(
                !quads.iter().any(|q| q.uv == rndr_paint::solid_uv()
                    && q.rect.w == 2.0
                    && q.rect.h >= card.rect.h
                    && q.color == theme_of(app.world()).caret),
                "no full-height insertion caret over a selected card"
            );
        }
    }
}
