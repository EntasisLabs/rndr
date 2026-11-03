use rndr_core::*;
use rndr_ui::collections::Item;
use rndr_ui::{collections::*, data::*, UiId, UiPlugin};
fn app(width: f32) -> (App, Entity) {
    let mut a = App::new();
    a.add_plugin(UiPlugin);
    a.world_mut().insert_resource(Viewport::new(width, 500.));
    let r = a.world_mut().spawn((Layout {
        width: Size::Fill,
        height: Size::Fill,
        ..Layout::column()
    },));
    (a, r)
}
#[test]
fn table_column_boundaries_share_one_grid_in_both_densities_and_after_scroll() {
    for compact in [false, true] {
        for width in [800., 220.] {
            let (mut a, r) = app(width);
            a.world_mut().insert_resource(rndr_ui::UiTheme {
                density: if compact {
                    rndr_ui::Density::Compact
                } else {
                    rndr_ui::Density::Comfortable
                },
                ..Default::default()
            });
            let cols: Vec<_> = [130., 160., 48.]
                .iter()
                .enumerate()
                .map(|(i, w)| Column {
                    id: ColumnId(i as u64 + 1),
                    label: if i == 2 {
                        "A very long column title at the minimum width".into()
                    } else {
                        format!("Column {i}")
                    },
                    width: *w,
                    alignment: match i {
                        0 => CellAlignment::Start,
                        1 => CellAlignment::Center,
                        _ => CellAlignment::End,
                    },
                    sortable: true,
                })
                .collect();
            let h = DataTable::mount(
                a.world_mut(),
                r,
                UiId(1),
                TableProps::new("Grid"),
                &cols,
                &[Item::new(1, "Record")],
                |w, slot, _, _| {
                    rndr_ui::text(w, slot.content, "Value", rndr_ui::Typography::Label).unwrap();
                },
            )
            .unwrap();
            rndr_paint::frame(&mut a);
            for scroll in [0., -100.] {
                a.world_mut().get_mut::<Scroll>(h.root).unwrap().offset[0] = scroll;
                rndr_paint::frame(&mut a);
                for c in &cols {
                    let header = *a
                        .world()
                        .get::<Rect>(h.header(a.world(), c.id).unwrap())
                        .unwrap();
                    let cell = *a
                        .world()
                        .get::<Rect>(h.cell(a.world(), ItemId(1), c.id).unwrap().root)
                        .unwrap();
                    let label = a
                        .world()
                        .entities_with::<Label>()
                        .into_iter()
                        .find(|e| {
                            a.world()
                                .get::<Label>(*e)
                                .is_some_and(|l| l.text == c.label)
                        })
                        .unwrap();
                    assert_eq!(
                        a.world().get::<TextStyle>(label).unwrap().align,
                        match c.alignment {
                            CellAlignment::Start => TextAlign::Start,
                            CellAlignment::Center => TextAlign::Center,
                            CellAlignment::End => TextAlign::End,
                        }
                    );
                    let clip = a.world().get::<Clip>(label).unwrap();
                    assert!(clip.x >= header.x && clip.y >= header.y);
                    assert!(clip.x + clip.w <= header.x + header.w + 0.01);
                    assert!(clip.y + clip.h <= header.y + header.h + 0.01);
                    assert!(
                        (header.x - cell.x).abs() < 0.01,
                        "density {compact} width {width}: header {header:?} cell {cell:?}"
                    );
                    assert!(
                        (header.w - cell.w).abs() < 0.01,
                        "header/cell widths {header:?} {cell:?}"
                    );
                }
            }
        }
    }
}
fn nested() -> (App, Entity, Entity) {
    let (mut a, outer) = app(200.);
    a.world_mut().get_mut::<Layout>(outer).unwrap().height = Size::Px(150.);
    a.world_mut().insert(outer, Scroll::default());
    let inner = a.world_mut().spawn((
        Layout {
            width: Size::Px(180.),
            height: Size::Px(100.),
            ..Layout::column()
        },
        Scroll::default(),
    ));
    set_parent(a.world_mut(), inner, outer);
    let content = a.world_mut().spawn((Layout {
        width: Size::Px(400.),
        height: Size::Px(300.),
        ..Layout::column()
    },));
    set_parent(a.world_mut(), content, inner);
    let tail = a.world_mut().spawn((Layout {
        width: Size::Px(180.),
        height: Size::Px(300.),
        ..Layout::column()
    },));
    set_parent(a.world_mut(), tail, outer);
    rndr_paint::frame(&mut a);
    (a, outer, inner)
}
fn wheel(a: &mut App, p: [f32; 2], delta: [f32; 2]) {
    let i = a.world_mut().resource_mut::<Input>().unwrap();
    i.pointer.position = p;
    i.scroll = delta;
    apply_scroll(a.world_mut());
}
#[test]
fn nested_wheel_consumes_only_available_distance_and_chains_each_axis() {
    let (mut a, o, i) = nested();
    wheel(&mut a, [50., 50.], [-30., -230.]);
    assert_eq!(a.world().get::<Scroll>(i).unwrap().offset, [-30., -200.]);
    assert_eq!(a.world().get::<Scroll>(o).unwrap().offset, [0., -30.]);
    wheel(&mut a, [50., 20.], [0., 50.]);
    assert_eq!(a.world().get::<Scroll>(i).unwrap().offset[1], -150.);
    assert_eq!(a.world().get::<Scroll>(o).unwrap().offset[1], -30.);
    assert_eq!(a.world().resource::<Input>().unwrap().scroll, [0., 0.]);
}
#[test]
fn outside_or_clipped_content_never_targets_an_unrelated_scrollport() {
    let (mut a, o, i) = nested();
    wheel(&mut a, [250., 50.], [0., -70.]);
    assert_eq!(a.world().get::<Scroll>(i).unwrap().offset, [0., 0.]);
    assert_eq!(a.world().get::<Scroll>(o).unwrap().offset, [0., 0.]);
    wheel(&mut a, [50., 125.], [0., -70.]);
    assert_eq!(a.world().get::<Scroll>(i).unwrap().offset, [0., 0.]);
    assert_eq!(a.world().get::<Scroll>(o).unwrap().offset, [0., -70.]);
}
#[test]
fn non_scrollable_inner_axis_chains_without_remapping() {
    let (mut a, o, i) = nested();
    let child = children_of(a.world(), i)[0];
    a.world_mut().get_mut::<Layout>(child).unwrap().height = Size::Px(80.);
    layout_propagate(a.world_mut());
    wheel(&mut a, [50., 50.], [-20., -40.]);
    assert_eq!(a.world().get::<Scroll>(i).unwrap().offset, [-20., 0.]);
    assert_eq!(a.world().get::<Scroll>(o).unwrap().offset, [0., -40.]);
}
#[test]
fn checkbox_indicator_uses_face_geometry_and_keeps_disabled_checked_semantics() {
    use rndr_ui::selection::{Checkbox, SelectionProps};
    for scale in [1., 1.5, 2.] {
        let (mut a, r) = app(500.);
        a.world_mut()
            .resource_mut::<rndr_ui::UiTheme>()
            .unwrap()
            .text_scale = scale;
        let mut props = SelectionProps::new("Preference");
        let h = Checkbox::mount(a.world_mut(), r, UiId(5), props.clone(), true).unwrap();
        rndr_paint::frame(&mut a);
        let marks = a.world().entities_with::<CrossMark>();
        assert_eq!(marks.len(), 1);
        let mark = marks[0];
        assert!(!a.world().has::<Label>(mark));
        let face = a.world().get::<Parent>(mark).unwrap().entity;
        assert_eq!(a.world().get::<Rect>(mark), a.world().get::<Rect>(face));
        assert_eq!(
            a.world().get::<SelectionAccess>(h.root).unwrap().checked,
            Some(true)
        );
        assert!(request_focus(a.world_mut(), h.root));
        props.enabled = false;
        let same = Checkbox::mount(a.world_mut(), r, UiId(5), props, true).unwrap();
        assert_eq!(same.root, h.root);
        rndr_paint::frame(&mut a);
        assert!(!access_details(a.world(), h.root).focus);
        assert!(!access_details(a.world(), h.root).activate);
        assert_eq!(
            a.world().get::<SelectionAccess>(h.root).unwrap().checked,
            Some(true)
        );
    }
}
#[test]
fn collection_focus_never_becomes_a_filled_viewport_and_disabled_rows_stay_quiet() {
    let (mut a, r) = app(500.);
    let mut disabled = Item::new(2, "Unavailable");
    disabled.enabled = false;
    let h = ListView::mount(
        a.world_mut(),
        r,
        UiId(9),
        ViewProps::new("Records"),
        &[Item::new(1, "Enabled"), disabled],
        |w, slot, item| {
            rndr_ui::text(w, slot.content, &item.name, rndr_ui::Typography::Label).unwrap();
        },
    )
    .unwrap();
    rndr_paint::frame(&mut a);
    request_focus(a.world_mut(), h.root);
    let frame = rndr_paint::frame(&mut a);
    let focus = a
        .world()
        .resource::<rndr_ui::UiTheme>()
        .unwrap()
        .color(rndr_ui::Token::Focus);
    assert!(!frame
        .quads
        .iter()
        .any(|q| q.uv == rndr_paint::solid_uv() && q.color == focus));
    assert!(a.world().has::<Border>(h.root));
    let row = h.row(a.world(), ItemId(2)).unwrap().root;
    assert!(!request_focus(a.world_mut(), row));
    assert!(!a.world().has::<Border>(row));
    assert!(!access_details(a.world(), row).activate);
}
#[test]
fn focus_ring_layers_are_concentric_in_light_dark_and_high_contrast() {
    for dark in [false, true] {
        for high in [false, true] {
            let (mut a, r) = app(500.);
            let theme = a.world_mut().resource_mut::<rndr_ui::UiTheme>().unwrap();
            theme.appearance = if dark {
                rndr_ui::Appearance::Dark
            } else {
                rndr_ui::Appearance::Light
            };
            theme.contrast = if high {
                rndr_ui::Contrast::High
            } else {
                rndr_ui::Contrast::Standard
            };
            let h = rndr_ui::controls::Button::mount(
                a.world_mut(),
                r,
                UiId(10),
                rndr_ui::controls::ButtonProps::new(
                    "Disable",
                    rndr_ui::controls::HostIntent::Command(1),
                ),
            )
            .unwrap();
            rndr_paint::frame(&mut a);
            request_focus(a.world_mut(), h.root);
            rndr_paint::frame(&mut a);
            let mut e = h.root;
            let mut prior: Option<(f32, f32)> = None;
            for radius in [10., 8., 6., 5.] {
                let actual = a.world().get::<Radius>(e).unwrap().0;
                assert_eq!(actual, radius);
                if let Some((outer, inset)) = prior {
                    assert_eq!(outer - inset, actual);
                }
                prior = Some((actual, a.world().get::<Layout>(e).unwrap().padding));
                e = children_of(a.world(), e)[0];
            }
        }
    }
}
#[test]
fn revealing_an_oversized_already_visible_target_does_not_jump_to_trailing_edge() {
    let (mut a, o, i) = nested();
    let content = children_of(a.world(), i)[0];
    reveal_entity(a.world_mut(), content);
    assert_eq!(a.world().get::<Scroll>(i).unwrap().offset, [0., 0.]);
    assert_eq!(a.world().get::<Scroll>(o).unwrap().offset, [0., 0.]);
}

#[test]
fn scroll_affordances_dock_inside_partially_clipped_inner_viewports() {
    let (mut a, outer, inner) = nested();
    let spacer = a.world_mut().spawn((Layout {
        width: Size::Px(180.),
        height: Size::Px(110.),
        ..Layout::column()
    },));
    set_parent(a.world_mut(), spacer, outer);
    let tail = children_of(a.world(), outer)[1];
    a.world_mut().insert(
        outer,
        Children {
            list: vec![spacer, inner, tail],
        },
    );
    let output = rndr_paint::frame(&mut a);
    let clip = *a.world().get::<Clip>(inner).unwrap();
    let color = a.world().get::<ScrollIndicator>(inner).unwrap().color;
    assert!(clip.h < 100. && clip.h > 0.);
    let thumbs: Vec<_> = output
        .quads
        .iter()
        .filter(|q| q.color == color && q.clip == clip)
        .collect();
    assert_eq!(thumbs.len(), 2);
    for thumb in thumbs {
        assert!(thumb.rect.x >= clip.x && thumb.rect.y >= clip.y);
        assert!(thumb.rect.x + thumb.rect.w <= clip.x + clip.w);
        assert!(thumb.rect.y + thumb.rect.h <= clip.y + clip.h);
    }
}
