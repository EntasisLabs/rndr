use rndr_core::*;
use rndr_ui::{
    controls::{self, *},
    *,
};

const LONG: &str = "This is a very long but valid command label with many words";
fn base(width: f32) -> (App, Entity, ToolbarHandle) {
    let mut a = App::new();
    a.add_plugin(UiPlugin);
    a.world_mut().insert_resource(Viewport::new(4000., 8000.));
    let p = a.world_mut().spawn((Layout {
        width: Size::Px(width),
        ..Layout::column()
    },));
    let t = Toolbar::mount(a.world_mut(), p, UiId(1), width).unwrap();
    (a, p, t)
}
fn button(a: &mut App, t: ToolbarHandle, id: u64, label: &str) -> ControlHandle {
    controls::Button::mount(
        a.world_mut(),
        t.root,
        UiId(id),
        ButtonProps::new(label, HostIntent::Command(id)),
    )
    .unwrap()
}
fn natural(a: &App, h: ControlHandle) -> f32 {
    text_engine(a.world())
        .measure(
            &a.world().get::<Label>(h.label).unwrap().text,
            a.world().get::<TextStyle>(h.label).unwrap(),
            &[],
            f32::INFINITY,
        )
        .width
}
// Derive the exact boundary from the real owned layout chain, not the old
// summed-width heuristic or a duplicate hardcoded estimate of its padding.
fn needed(a: &App, h: ControlHandle) -> f32 {
    let mut e = h.label;
    let mut insets = 0.;
    loop {
        insets += 2. * a.world().get::<Layout>(e).unwrap().padding;
        if e == h.root {
            break;
        }
        e = a.world().get::<Parent>(e).unwrap().entity;
    }
    (natural(a, h) + insets).max(a.world().get::<Layout>(h.root).unwrap().min_width)
}
fn axis(a: &App, t: ToolbarHandle) -> Axis {
    a.world().get::<Layout>(t.root).unwrap().axis
}
fn resize(a: &mut App, p: Entity, t: ToolbarHandle, width: f32) {
    a.world_mut().get_mut::<Layout>(p).unwrap().width = Size::Px(width);
    assert_eq!(Toolbar::mount(a.world_mut(), p, UiId(1), width).unwrap(), t);
}
fn fitted(a: &App, t: ToolbarHandle, handles: &[ControlHandle]) {
    assert_eq!(axis(a, t), Axis::Row);
    for h in handles {
        let r = a.world().get::<Rect>(h.label).unwrap();
        assert!(r.w + 0.001 >= natural(a, *h), "{r:?} < {}", natural(a, *h));
    }
}

// Promoted unchanged assertion from the independent R1 failing probe.
#[test]
fn unequal_toolbar_labels_must_fit_each_row_share() {
    let (mut a, _, t) = base(1000.);
    let long = button(&mut a, t, 2, LONG);
    button(&mut a, t, 3, "Go");
    a.update();
    let natural = natural(&a, long);
    let r = *a.world().get::<Rect>(long.label).unwrap();
    let axis = axis(&a, t);
    println!("axis={axis:?} natural_label_width={natural} actual_label_rect={r:?}");
    assert!(
        axis == Axis::Column || r.w + 0.01 >= natural,
        "row selected even though longest label does not fit its allocated width"
    );
}

#[test]
fn equal_share_boundary_both_densities_and_type_scales() {
    for density in [Density::Comfortable, Density::Compact] {
        for scale in [1., 2.] {
            let (mut a, p, t) = base(4000.);
            a.world_mut().insert_resource(UiTheme {
                density,
                text_scale: scale,
                ..UiTheme::default()
            });
            let long = button(&mut a, t, 2, LONG);
            let short = button(&mut a, t, 3, "Go");
            a.update();
            let boundary = 2. * needed(&a, long).max(needed(&a, short)) + 8.;
            // Immediately below, exactly at, and above fit; both directions.
            for (width, expected) in [
                (boundary - 0.25, Axis::Column),
                (boundary, Axis::Row),
                (boundary + 0.25, Axis::Row),
                (boundary - 0.25, Axis::Column),
                (boundary, Axis::Row),
            ] {
                resize(&mut a, p, t, width);
                a.update();
                assert_eq!(
                    axis(&a, t),
                    expected,
                    "density={density:?} scale={scale} width={width}"
                );
                if expected == Axis::Row {
                    fitted(&a, t, &[long, short]);
                }
            }
        }
    }
}

#[test]
fn same_update_width_scale_and_label_changes_preserve_identity_and_actions() {
    for density in [Density::Comfortable, Density::Compact] {
        let (mut a, p, t) = base(1000.);
        a.world_mut().insert_resource(UiTheme {
            density,
            ..UiTheme::default()
        });
        let h = button(&mut a, t, 2, "Go");
        let other = button(&mut a, t, 3, "OK");
        a.update();
        assert!(request_focus(a.world_mut(), h.root));
        for (width, scale, label, expected) in [
            (1000., 1., LONG, Axis::Column), // label row -> stack
            (1000., 1., "Go", Axis::Row),    // label stack -> row
            (1000., 1., "MMMMMMMMMMMMMMMMMMMM", Axis::Row),
            (1000., 2., "MMMMMMMMMMMMMMMMMMMM", Axis::Column), // font row -> stack
            (1000., 1., "MMMMMMMMMMMMMMMMMMMM", Axis::Row),    // font stack -> row
            (200., 1., "MMMMMMMMMMMMMMMMMMMM", Axis::Column),  // width row -> stack
            (1000., 1., "MMMMMMMMMMMMMMMMMMMM", Axis::Row),    // width stack -> row
            (32., 2., LONG, Axis::Column),                     // combined/narrow finite geometry
            (2000., 1., LONG, Axis::Row),                      // combined stack -> row
        ] {
            resize(&mut a, p, t, width);
            a.world_mut().insert_resource(UiTheme {
                density,
                text_scale: scale,
                ..UiTheme::default()
            });
            assert_eq!(button(&mut a, t, 2, label), h);
            a.update();
            assert_eq!(axis(&a, t), expected, "{width} {scale} {label}");
            assert_eq!(a.world().resource::<Focus>().unwrap().entity, Some(h.root));
            assert_eq!(a.world().get::<Label>(h.label).unwrap().text, label);
            assert_eq!(
                a.world().get::<Semantics>(h.root).unwrap().name.as_deref(),
                Some(label)
            );
            if expected == Axis::Row {
                fitted(&a, t, &[h, other]);
            }
            for control in [h, other] {
                let r = a.world().get::<Rect>(control.root).unwrap();
                assert!(r.x.is_finite() && r.y.is_finite() && r.w.is_finite() && r.h.is_finite());
                assert!(r.w >= 28. && r.h >= 28.);
                assert_eq!(
                    try_activate(a.world_mut(), control.root),
                    ActivationResult::Accepted
                );
                let request = take_action_intents(a.world_mut()).remove(0);
                let id = if control == h { 2 } else { 3 };
                assert_eq!(
                    host_intent(a.world(), &request),
                    Some(HostIntent::Command(id))
                );
            }
        }
    }
}

#[test]
fn mixed_icon_and_source_actions_use_their_actual_label_styles() {
    for density in [Density::Comfortable, Density::Compact] {
        for scale in [1., 2.] {
            let (mut a, p, t) = base(4000.);
            a.world_mut().insert_resource(UiTheme {
                density,
                text_scale: scale,
                ..UiTheme::default()
            });
            let long = button(&mut a, t, 2, LONG);
            let icon = IconButton::mount(
                a.world_mut(),
                t.root,
                UiId(3),
                "+",
                "Add",
                ButtonProps::new("ignored", HostIntent::Command(3)),
            )
            .unwrap();
            let source = SourceAction::mount(
                a.world_mut(),
                t.root,
                UiId(4),
                SourceLocator {
                    document: 1,
                    revision: 2,
                    anchor: 3,
                },
                true,
            )
            .unwrap();
            a.update();
            let boundary = 3.
                * needed(&a, long)
                    .max(needed(&a, icon))
                    .max(needed(&a, source))
                + 16.;
            for (w, expected) in [(boundary - 0.25, Axis::Column), (boundary, Axis::Row)] {
                resize(&mut a, p, t, w);
                a.update();
                assert_eq!(axis(&a, t), expected);
                if expected == Axis::Row {
                    fitted(&a, t, &[long, icon, source]);
                }
                assert_eq!(
                    a.world()
                        .get::<Semantics>(icon.root)
                        .unwrap()
                        .name
                        .as_deref(),
                    Some("Add")
                );
                assert_eq!(
                    a.world().get::<Label>(source.label).unwrap().text,
                    "Show source"
                );
            }
        }
    }
}

#[test]
fn empty_single_hidden_unknown_policy_and_toolbar_padding() {
    let (mut a, p, t) = base(1000.);
    a.update();
    assert_eq!(axis(&a, t), Axis::Row);
    let long = button(&mut a, t, 2, LONG);
    a.update();
    fitted(&a, t, &[long]);
    let short = button(&mut a, t, 3, "Go");
    a.world_mut().insert(long.root, Hidden);
    a.update();
    fitted(&a, t, &[short]);
    let unknown = a.world_mut().spawn((Layout::column(),));
    set_parent(a.world_mut(), unknown, t.root);
    a.update();
    assert_eq!(axis(&a, t), Axis::Column);
    a.world_mut().insert(unknown, Hidden);
    a.update();
    fitted(&a, t, &[short]);
    a.world_mut().remove::<Hidden>(long.root);
    a.world_mut().get_mut::<Layout>(t.root).unwrap().padding = 7.;
    let boundary = 2. * needed(&a, long) + 8. + 14.;
    for (w, expected) in [(boundary - 0.25, Axis::Column), (boundary, Axis::Row)] {
        resize(&mut a, p, t, w);
        a.update();
        assert_eq!(axis(&a, t), expected);
        if expected == Axis::Row {
            fitted(&a, t, &[long, short]);
        }
    }
}
