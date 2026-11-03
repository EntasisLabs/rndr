//! End-to-end host model wiring through public reusable components.
#[path = "../examples/support/data_editors_gallery.rs"]
mod gallery;
use rndr_core::*;
use rndr_ui::{collections::ItemId, data::*, UiTheme};
fn frame(a: &mut App) {
    rndr_paint::frame(a);
}
fn batch(a: &mut App, keys: Vec<KeyEvent>) {
    a.world_mut().resource_mut::<Input>().unwrap().keys = keys;
    frame(a);
}
fn k(n: NamedKey) -> KeyEvent {
    KeyEvent::named(n, Modifiers::default())
}
#[test]
fn joint_gallery_commits_model_repaints_cells_and_switches_objects_without_draft_leak() {
    let mut a = gallery::create(1180., 960., UiTheme::default());
    frame(&mut a);
    frame(&mut a);
    let g = a.world().resource::<gallery::Gallery>().unwrap().clone();
    let h = g.inspector.unwrap();
    let table = g.table.unwrap();
    let editor = h.slot(a.world(), PropertyId(1)).unwrap().editor;
    request_focus(a.world_mut(), editor);
    batch(
        &mut a,
        vec![
            KeyEvent::character('!', Modifiers::default()),
            k(NamedKey::Enter),
        ],
    );
    assert_eq!(
        a.world().resource::<gallery::Gallery>().unwrap().records[0].title,
        "Brand refresh!"
    );
    let cell = table.cell(a.world(), ItemId(1), ColumnId(1)).unwrap();
    assert_eq!(
        a.world()
            .get::<Label>(children_of(a.world(), cell.content)[0])
            .unwrap()
            .text,
        "Brand refresh!"
    );
    request_focus(a.world_mut(), editor);
    batch(&mut a, vec![KeyEvent::character('?', Modifiers::default())]);
    table.rows.select(a.world_mut(), Some(ItemId(2))).unwrap();
    frame(&mut a);
    assert_eq!(
        a.world().resource::<gallery::Gallery>().unwrap().selected,
        Some(2)
    );
    assert_eq!(
        h.draft(a.world(), PropertyId(1)).unwrap().raw,
        "Mobile patterns"
    );
    assert_eq!(
        a.world().resource::<gallery::Gallery>().unwrap().records[0].title,
        "Brand refresh!"
    );
    let header = table.header(a.world(), ColumnId(1)).unwrap();
    request_focus(a.world_mut(), header);
    batch(&mut a, vec![k(NamedKey::Enter)]);
    assert_eq!(
        a.world()
            .resource::<gallery::Gallery>()
            .unwrap()
            .sort
            .unwrap()
            .direction,
        SortDirection::Ascending
    );
    assert_eq!(table.rows.selected(a.world()), Some(ItemId(2)));
}
#[test]
fn joint_gallery_nonzero_visible_cell_geometry_and_column_header_associations() {
    let mut a = gallery::create(1180., 960., UiTheme::default());
    frame(&mut a);
    frame(&mut a);
    let table = a
        .world()
        .resource::<gallery::Gallery>()
        .unwrap()
        .table
        .unwrap();
    for row in 1..=8 {
        for col in 1..=3 {
            let slot = table.cell(a.world(), ItemId(row), ColumnId(col)).unwrap();
            let r = a.world().get::<Rect>(slot.root).unwrap();
            assert!(r.w >= 100. && r.h >= 20., "row={row} col={col} rect={r:?}");
            let label = children_of(a.world(), slot.content)[0];
            let lr = a.world().get::<Rect>(label).unwrap();
            assert!(lr.w > 0. && lr.h > 0.);
            if col > 1 {
                let text = &a.world().get::<Label>(label).unwrap().text;
                let engine = text_engine(a.world());
                let style = style_for(a.world(), label);
                let natural = engine.measure(text, &style, &[], f32::INFINITY);
                assert!(
                    lr.w >= natural.width + 0.5,
                    "aligned label needs rounding guard"
                );
                let measured = engine.measure(
                    text,
                    &style,
                    &[],
                    label_box(a.world(), label).text_width(lr.w, engine.unit_advance(&style)),
                );
                assert_eq!(
                    measured.line_count, 1,
                    "numeric/boolean value must not soft-wrap into a clipped line"
                );
            }
            assert!(
                lr.x >= r.x && lr.x + lr.w <= r.x + r.w + 1.,
                "{lr:?} not inside {r:?}"
            );
        }
    }
    let tree = access_tree(a.world());
    for n in &tree.nodes {
        if n.role == AccessRole::Cell {
            let e = entity_from_access_id(n.id).unwrap();
            assert_eq!(a.world().get::<DataAccess>(e).unwrap().headers.len(), 1);
        }
    }
}
#[test]
fn narrow_and_scaled_layout_remain_scroll_reachable_not_compressed_to_zero() {
    for (width, scale) in [(380., 1.), (1180., 2.)] {
        let mut a = gallery::create(
            width,
            960.,
            UiTheme {
                text_scale: scale,
                reading_scale: scale,
                ..UiTheme::default()
            },
        );
        frame(&mut a);
        frame(&mut a);
        let g = a.world().resource::<gallery::Gallery>().unwrap();
        let t = a.world().get::<Rect>(g.table_parent).unwrap();
        let i = a.world().get::<Rect>(g.inspector_parent).unwrap();
        assert!(t.h >= 440. && i.h >= 680.);
        assert!(i.y >= t.y + t.h);
        assert!(a.world().get::<Scroll>(g.root).unwrap().content[1] > 960.);
    }
}

#[test]
fn every_synthetic_preset_reconciles_without_disabled_selection_mutation_or_panic() {
    for preset in ["disabled", "empty", "filtered", "readonly"] {
        let mut a = gallery::create(1180., 960., UiTheme::default());
        frame(&mut a);
        {
            let g = a.world_mut().resource_mut::<gallery::Gallery>().unwrap();
            match preset {
                "disabled" => g.disabled = true,
                "empty" => {
                    g.records.clear();
                    g.selected = None;
                }
                "filtered" => g.filtered = true,
                "readonly" => g.readonly = true,
                _ => unreachable!(),
            }
        }
        gallery::reconcile(a.world_mut());
        frame(&mut a);
        frame(&mut a);
        let g = a.world().resource::<gallery::Gallery>().unwrap();
        let t = g.table.unwrap();
        let i = g.inspector.unwrap();
        assert!(t.validate(a.world()).is_ok() && i.validate(a.world()).is_ok());
        if preset == "disabled" {
            assert_eq!(t.rows.selected(a.world()), Some(ItemId(1)));
            let e = t.header(a.world(), ColumnId(1)).unwrap();
            assert!(!request_focus(a.world_mut(), e));
        }
        if preset == "empty" {
            assert!(i.slot(a.world(), PropertyId(1)).is_none());
        }
    }
}
#[test]
fn independent_reading_scale_enlarges_builtin_editor_line_boxes() {
    let mut a = gallery::create(
        1180.,
        960.,
        UiTheme {
            reading_scale: 2.,
            ..UiTheme::default()
        },
    );
    frame(&mut a);
    frame(&mut a);
    let h = a
        .world()
        .resource::<gallery::Gallery>()
        .unwrap()
        .inspector
        .unwrap();
    let e = h.slot(a.world(), PropertyId(1)).unwrap().editor;
    let style = style_for(a.world(), e);
    assert_eq!(
        style.size,
        a.world()
            .resource::<UiTheme>()
            .unwrap()
            .typography(rndr_ui::Typography::Code)
            .size
    );
    assert!(style.size >= 26.);
    let r = a.world().get::<Rect>(e).unwrap();
    assert!(r.h >= text_engine(a.world()).line_height(&style) + 16. - 1.);
}
