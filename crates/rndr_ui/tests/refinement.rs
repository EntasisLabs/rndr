use rndr_core::*;
use rndr_ui::{
    controls::{self, *},
    *,
};

fn app(width: f32, theme: UiTheme) -> (App, Entity) {
    let mut a = App::new();
    a.add_plugin(UiPlugin);
    a.world_mut().insert_resource(Viewport::new(width, 5000.));
    a.world_mut().insert_resource(theme);
    let root = a.world_mut().spawn((Layout {
        width: Size::Px(width),
        ..Layout::column()
    },));
    (a, root)
}
fn face(a: &App, h: ControlHandle) -> Entity {
    a.world().get::<Parent>(h.label).unwrap().entity
}

#[test]
fn display_metrics_measure_shape_paint_and_release_together() {
    let (mut a, root) = app(48., UiTheme::default());
    let label = text(a.world_mut(), root, "MMM", Typography::Label).unwrap();
    let frame = rndr_paint::frame(&mut a);
    let r = *a.world().get::<Rect>(label).unwrap();
    let shaped = a.world().get::<TextLayout>(label).unwrap();
    assert_eq!(shaped.lines.len(), 1);
    assert_eq!(r.h, 20.);
    let glyph = frame
        .quads
        .iter()
        .find(|q| q.uv != rndr_paint::solid_uv())
        .unwrap();
    assert!((glyph.rect.x - r.x - shaped.glyphs[0].x).abs() <= 1.);
    assert!((glyph.rect.y - r.y - shaped.glyphs[0].y).abs() <= 1.);
    // Removing the opt-in restores the legacy field box AND invalidates wrap
    // cache through the new content width; no stale glyph layout is reused.
    a.world_mut().remove::<LabelBox>(label);
    a.update();
    assert!(a.world().get::<TextLayout>(label).unwrap().lines.len() > 1);
    assert!(a.world().get::<Rect>(label).unwrap().h >= 36.);
    a.world_mut().insert(label, LabelBox::CONTENT);
    a.update();
    assert_eq!(a.world().get::<TextLayout>(label).unwrap().lines.len(), 1);
    assert_eq!(a.world().get::<Rect>(label).unwrap().h, 20.);
}

#[test]
fn label_box_never_changes_editor_selection_or_legacy_metrics() {
    let mut w = World::new();
    let e = w.spawn((Label::new("source"), LabelBox::CONTENT));
    assert_eq!(label_box(&w, e), LabelBox::CONTENT);
    w.insert(e, Caret::default());
    assert_eq!(label_box(&w, e), LabelBox::default());
    w.remove::<Caret>(e);
    w.insert(e, TextSelection { anchor: 0, head: 3 });
    assert_eq!(label_box(&w, e), LabelBox::default());
    w.remove::<TextSelection>(e);
    w.insert(e, Buffer::from_str("exact source"));
    assert_eq!(label_box(&w, e), LabelBox::default());
    w.remove::<Buffer>(e);
    for invalid in [f32::NAN, f32::INFINITY, -1.] {
        w.insert(
            e,
            LabelBox {
                inset_x: invalid,
                ..LabelBox::CONTENT
            },
        );
        assert_eq!(label_box(&w, e), LabelBox::default());
    }
    assert_eq!(field_text_width(100., 14.), 76.);
}

#[test]
fn workspace_is_content_width_and_text_scale_driven_without_mutating_on_error() {
    for density in [Density::Comfortable, Density::Compact] {
        for scale in [1., 2.] {
            let theme = UiTheme {
                density,
                text_scale: scale,
                ..UiTheme::default()
            };
            let boundary = 680. * scale + theme.space(Space::X16);
            for (width, row) in [
                (boundary - 1., false),
                (boundary, true),
                (boundary + 1., true),
            ] {
                let metrics = WorkspaceLayout::resolve(width, &theme);
                assert_eq!(metrics.side_by_side, row);
                assert!(metrics.document_width >= 0.);
                if row {
                    assert_eq!(
                        metrics.navigation_width + metrics.gap + metrics.document_width,
                        width
                    );
                }
            }
        }
    }
    let theme = UiTheme::default();
    for width in [f32::NAN, f32::INFINITY, -1., 0., 320.] {
        let metrics = WorkspaceLayout::resolve(width, &theme);
        assert!(!metrics.side_by_side);
        assert!(metrics.document_width.is_finite());
    }
    let (mut a, root) = app(1000., theme.clone());
    let nav = a.world_mut().spawn((Layout::column(),));
    let before = *a.world().get::<Layout>(root).unwrap();
    assert_eq!(
        WorkspaceLayout::resolve(1000., &theme).apply(a.world_mut(), root, nav),
        Err(UiError::Stale)
    );
    let after = a.world().get::<Layout>(root).unwrap();
    assert_eq!(
        (after.axis, after.width, after.gap),
        (before.axis, before.width, before.gap)
    );
    set_parent(a.world_mut(), nav, root);
    WorkspaceLayout::resolve(1000., &theme)
        .apply(a.world_mut(), root, nav)
        .unwrap();
    assert_eq!(a.world().get::<Layout>(root).unwrap().axis, Axis::Row);
    WorkspaceLayout::resolve(320., &theme)
        .apply(a.world_mut(), root, nav)
        .unwrap();
    assert_eq!(a.world().get::<Layout>(nav).unwrap().width, Size::Fill);
}

#[test]
fn adaptive_modes_preserve_selection_focus_targets_and_release_authored_widths() {
    for density in [Density::Comfortable, Density::Compact] {
        for scale in [1., 2.] {
            let (mut a, root) = app(
                2000.,
                UiTheme {
                    density,
                    text_scale: scale,
                    ..UiTheme::default()
                },
            );
            let modes = ModeSelector::mount(
                a.world_mut(),
                root,
                UiId(1),
                "Document mode",
                &["Preview", "A longer source editor label"],
                TabOrientation::Horizontal,
                0,
            )
            .unwrap();
            a.world_mut()
                .get_mut::<Layout>(modes.tabs[0].root)
                .unwrap()
                .width = Size::Px(93.);
            let original = a.world().get::<Layout>(modes.group).unwrap().width;
            for width in [1800., 160., 1800., 160.] {
                a.world_mut().get_mut::<Layout>(root).unwrap().width = Size::Px(width);
                modes.fit(a.world_mut(), width).unwrap();
                a.update();
                assert!(request_focus(a.world_mut(), modes.tabs[0].root));
                let row = width == 1800.;
                assert_eq!(
                    a.world().get::<Layout>(modes.group).unwrap().axis,
                    if row { Axis::Row } else { Axis::Column }
                );
                assert_eq!(
                    a.world().get::<ManualTabs>(modes.group).unwrap().0,
                    if row {
                        TabOrientation::Horizontal
                    } else {
                        TabOrientation::Vertical
                    }
                );
                assert_eq!(
                    a.world().get::<Tabs>(modes.group).unwrap().selected,
                    a.world().get::<Tab>(modes.tabs[0].root).unwrap().id
                );
                for tab in &modes.tabs {
                    let rect = a.world().get::<Rect>(tab.root).unwrap();
                    assert!(
                        rect.w <= width
                            && rect.h >= a.world().resource::<UiTheme>().unwrap().target_min()
                    );
                    assert!(!a.world().has::<Hidden>(tab.root));
                    if row {
                        assert_eq!(
                            a.world().get::<TextLayout>(tab.label).unwrap().lines.len(),
                            1
                        );
                    }
                }
            }
            modes.clear_fit(a.world_mut()).unwrap();
            a.update();
            assert_eq!(
                a.world().get::<Layout>(modes.tabs[0].root).unwrap().width,
                Size::Px(93.)
            );
            assert_eq!(
                a.world().get::<Layout>(modes.group).unwrap().width,
                original
            );
            assert_eq!(
                a.world().resource::<Focus>().unwrap().entity,
                Some(modes.tabs[0].root)
            );
            assert_eq!(
                modes.fit(a.world_mut(), f32::NAN),
                Err(UiError::InvalidProps)
            );
        }
    }
}

#[test]
fn quiet_host_plane_uses_same_update_palette_and_honors_explicit_color_takeover() {
    let (mut a, root) = app(1000., UiTheme::default());
    let surface = Surface::Content
        .mount(a.world_mut(), root, UiId(1))
        .unwrap();
    let mut props = ButtonProps::new("Quiet", HostIntent::Command(1));
    props.variant = ButtonVariant::Quiet;
    let button = controls::Button::mount(a.world_mut(), surface.content, UiId(2), props).unwrap();
    for appearance in [Appearance::Light, Appearance::Dark, Appearance::Light] {
        let theme = UiTheme {
            appearance,
            ..UiTheme::default()
        };
        a.world_mut().insert_resource(theme.clone());
        a.update();
        assert_eq!(
            *a.world().get::<Color>(face(&a, button)).unwrap(),
            theme.color(Token::Content)
        );
    }
    let custom = Color::rgb(0.31, 0.34, 0.37);
    a.world_mut().insert(surface.root, custom);
    a.world_mut().insert_resource(UiTheme {
        appearance: Appearance::Dark,
        ..UiTheme::default()
    });
    a.update();
    assert_eq!(*a.world().get::<Color>(face(&a, button)).unwrap(), custom);
    assert_eq!(*a.world().get::<Color>(surface.root).unwrap(), custom);
}

#[test]
fn navigation_metadata_is_secondary_visible_named_and_owned() {
    let (mut a, root) = app(240., UiTheme::default());
    let row = NavigationRow::mount(
        a.world_mut(),
        root,
        UiId(1),
        ButtonProps::new("Project notes", HostIntent::Command(1)),
        NavigationMeta {
            selected: true,
            dirty: true,
            secondary: Some("Draft".into()),
        },
    )
    .unwrap();
    a.update();
    assert_eq!(
        a.world().get::<Label>(row.label).unwrap().text,
        "| Project notes"
    );
    let secondary = children_of(a.world(), face(&a, row))
        .into_iter()
        .find(|e| *e != row.label)
        .unwrap();
    let label = a.world().get::<Label>(secondary).unwrap();
    assert_eq!(label.text, "Draft · Unsaved");
    assert_eq!(
        label.color,
        Some(UiTheme::default().color(Token::Secondary))
    );
    assert_eq!(a.world().get::<TextStyle>(secondary).unwrap().size, 12.);
    let name = a
        .world()
        .get::<Semantics>(row.root)
        .unwrap()
        .name
        .as_deref()
        .unwrap();
    assert!(name.contains("Draft") && name.contains("unsaved"));
    let rect = a.world().get::<Rect>(row.root).unwrap();
    assert!(
        rect.h <= 64.,
        "metadata should not inherit a second 36pt field row: {rect:?}"
    );
    row.destroy(a.world_mut()).unwrap();
    assert!(!a.world().is_alive(secondary));
}
