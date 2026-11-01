use rndr_core::*;
use rndr_ui::{
    controls::{self, *},
    *,
};
const LONG: &str = "This is a very long but valid command label with many words";
fn setup(
    width: f32,
    scale: f32,
    density: Density,
    type_scale: f32,
) -> (App, Entity, ToolbarHandle, ControlHandle, ControlHandle) {
    let mut a = App::new();
    a.add_plugin(UiPlugin);
    a.world_mut().insert_resource(Viewport {
        scale,
        ..Viewport::new(8000., 30000.)
    });
    a.world_mut().insert_resource(UiTheme {
        density,
        text_scale: type_scale,
        ..UiTheme::default()
    });
    let p = a.world_mut().spawn((Layout {
        width: Size::Px(width),
        ..Layout::column()
    },));
    let t = Toolbar::mount(a.world_mut(), p, UiId(1), width).unwrap();
    let h = button(&mut a, t, 2, LONG);
    let short = button(&mut a, t, 3, "Go");
    (a, p, t, h, short)
}
fn button(a: &mut App, t: ToolbarHandle, id: u64, text: &str) -> ControlHandle {
    controls::Button::mount(
        a.world_mut(),
        t.root,
        UiId(id),
        ButtonProps::new(text, HostIntent::Command(id)),
    )
    .unwrap()
}
fn resize(a: &mut App, p: Entity, width: f32) {
    a.world_mut().get_mut::<Layout>(p).unwrap().width = Size::Px(width);
    Toolbar::mount(a.world_mut(), p, UiId(1), width).unwrap();
}
fn row(a: &App, t: ToolbarHandle) -> bool {
    a.world().get::<Layout>(t.root).unwrap().axis == Axis::Row
}
// This assertion consumes the engine's FINAL content and line/glyph geometry.
// It does not assert a reimplementation of the toolbar's fit predicate.
fn unwrapped(a: &App, h: ControlHandle) {
    let w = a.world();
    let style = w.get::<TextStyle>(h.label).unwrap();
    let engine = text_engine(w);
    let label = w.get::<Label>(h.label).unwrap();
    let natural = engine.layout(&label.text, style, &[], f32::INFINITY);
    let actual = w.get::<TextLayout>(h.label).unwrap();
    let r = w.get::<Rect>(h.label).unwrap();
    let content = field_text_width(r.w, engine.unit_advance(style));
    assert_eq!(
        actual.lines.len(),
        1,
        "label={} rect={r:?} content={content} actual={actual:?}",
        label.text
    );
    assert!(
        content >= natural.width,
        "content={content} natural={}",
        natural.width
    );
    assert_eq!(actual.lines, natural.lines);
    assert_eq!(actual.glyphs, natural.glyphs);
    assert_eq!(actual.width, natural.width);
}
#[test]
fn integer_inner_text_reproduction_and_exact_pixel_boundaries() {
    for density in [Density::Comfortable, Density::Compact] {
        let (mut a, p, t, h, short) = setup(1696., 1., density, 1.);
        a.update();
        assert!(!row(&a, t), "old label-box boundary must stack");
        // Actual builtin layout: unwrapped 826, content inset 24, nested padding18.
        // 1744 is the true integer snapped fit; two points below loses a point
        // of content. Subpixel widths may legitimately snap to the same fit.
        for (width, expected_row) in [
            (1742., false),
            (1744., true),
            (1746., true),
            (1742., false),
            (1744., true),
        ] {
            resize(&mut a, p, width);
            a.update();
            assert_eq!(
                row(&a, t),
                expected_row,
                "width={width} density={density:?}"
            );
            if expected_row {
                unwrapped(&a, h);
                unwrapped(&a, short);
            }
        }
    }
}
#[test]
fn fractional_content_boundaries_with_nested_origin_and_updates() {
    for density in [Density::Comfortable, Density::Compact] {
        for scale in [1., 1.25, 1.5, 1.75, 2.] {
            for ty in [1., 1.1, 1.25, 1.5, 2.] {
                let (mut a, p, t, h, short) = setup(5000., scale, density, ty);
                a.world_mut().get_mut::<Layout>(p).unwrap().padding = 0.3;
                a.world_mut().get_mut::<Layout>(t.root).unwrap().padding = 0.7;
                a.update();
                let style = a.world().get::<TextStyle>(h.label).unwrap();
                let natural = text_engine(a.world())
                    .measure(LONG, style, &[], f32::INFINITY)
                    .width;
                // Sweep around both the rejected old boundary and true content
                // boundary. These are stimuli, not our correctness oracle.
                for center in [
                    2. * (natural + 18.) + 8.,
                    2. * (natural + 42.) + 8. + 2. * 0.7,
                ] {
                    for extra in [-4., -2., -0.5, 0., 0.25, 0.5, 1., 2., 4., -4., 4.] {
                        resize(&mut a, p, center + extra);
                        a.update();
                        if row(&a, t) {
                            unwrapped(&a, h);
                            unwrapped(&a, short);
                        }
                    }
                }
                resize(&mut a, p, 6000.);
                a.update();
                assert!(row(&a, t));
                unwrapped(&a, h);
            }
        }
    }
}
#[test]
fn same_update_label_type_width_both_ways_uses_final_line_geometry() {
    for density in [Density::Comfortable, Density::Compact] {
        let (mut a, p, t, h, short) = setup(1696., 1.75, density, 1.);
        a.update();
        assert!(!row(&a, t));
        assert!(request_focus(a.world_mut(), h.root));
        for (width, ty, label, expected) in [
            (1696., 1., "Go", true),
            (1696., 1., LONG, false),
            (2000., 1., LONG, true),
            (2000., 2., LONG, false),
            (2000., 1., LONG, true),
            (1000., 1., LONG, false),
            (2000., 1., LONG, true),
        ] {
            resize(&mut a, p, width);
            a.world_mut().insert_resource(UiTheme {
                density,
                text_scale: ty,
                ..UiTheme::default()
            });
            assert_eq!(button(&mut a, t, 2, label), h);
            a.update();
            assert_eq!(row(&a, t), expected);
            assert_eq!(a.world().resource::<Focus>().unwrap().entity, Some(h.root));
            if expected {
                unwrapped(&a, h);
                unwrapped(&a, short);
            }
            assert_eq!(
                try_activate(a.world_mut(), h.root),
                ActivationResult::Accepted
            );
            let req = take_action_intents(a.world_mut());
            assert_eq!(req.len(), 1);
            assert_eq!(
                host_intent(a.world(), &req[0]),
                Some(HostIntent::Command(2))
            );
        }
    }
}
#[test]
fn hidden_ancestor_malformed_and_scroll_rejected_measurement_are_safe() {
    let (mut a, p, t, h, _) = setup(1696., 1.75, Density::Comfortable, 1.);
    a.world_mut().get_mut::<Layout>(p).unwrap().height = Size::Px(40.);
    a.world_mut().insert(p, Scroll::default());
    let outer = a.world_mut().spawn((Layout::column(),));
    set_parent(a.world_mut(), p, outer);
    a.world_mut().insert(p, Hidden);
    a.update(); // No Rect is allocated under a never-shown hidden ancestor.
    a.world_mut().remove::<Hidden>(p);
    a.update();
    assert!(!row(&a, t));
    a.world_mut().get_mut::<Scroll>(p).unwrap().offset[1] = -30.;
    a.update();
    assert!(!row(&a, t));
    assert_eq!(a.world().get::<Scroll>(p).unwrap().offset[1], -30.);
    assert!(a.world().get::<TextLayout>(h.label).unwrap().lines.len() <= 1);
    let label = a.world_mut().remove::<Label>(h.label).unwrap();
    a.update(); // Malformed owned label must stack, not panic or adopt a child.
    assert!(!row(&a, t));
    a.world_mut().insert(h.label, label);
    resize(&mut a, p, 2000.);
    a.update();
    assert!(row(&a, t));
    unwrapped(&a, h);
}
