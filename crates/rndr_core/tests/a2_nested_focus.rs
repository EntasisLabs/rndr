use rndr_core::*;
#[test]
fn nested_scroll_reveals_focused_target_through_each_ancestor() {
    let mut a = App::new();
    a.add_plugin(GenericInteraction);
    let outer = a.world_mut().spawn((
        Layout {
            width: Size::Px(300.0),
            height: Size::Px(140.0),
            ..Layout::column()
        },
        Scroll::default(),
    ));
    let spacer = a.world_mut().spawn((Layout {
        width: Size::Fill,
        height: Size::Px(220.0),
        ..Layout::column()
    },));
    set_parent(a.world_mut(), spacer, outer);
    let inner = a.world_mut().spawn((
        Layout {
            width: Size::Fill,
            height: Size::Px(80.0),
            ..Layout::column()
        },
        Scroll::default(),
    ));
    set_parent(a.world_mut(), inner, outer);
    let filler = a.world_mut().spawn((Layout {
        width: Size::Fill,
        height: Size::Px(300.0),
        ..Layout::column()
    },));
    set_parent(a.world_mut(), filler, inner);
    let target = a.world_mut().spawn((
        Layout {
            width: Size::Fill,
            height: Size::Px(40.0),
            ..Layout::column()
        },
        Button::new("nested"),
        ReleaseActivation,
        Focusable,
    ));
    set_parent(a.world_mut(), target, inner);
    a.update();
    assert!(request_focus(a.world_mut(), target));
    a.update();
    let t = *a.world().get::<Rect>(target).unwrap();
    let o = *a.world().get::<Rect>(outer).unwrap();
    let i = *a.world().get::<Rect>(inner).unwrap();
    assert!(a.world().get::<Scroll>(inner).unwrap().offset[1] < 0.0);
    assert!(
        a.world().get::<Scroll>(outer).unwrap().offset[1] < 0.0,
        "outer ancestor must also reveal target"
    );
    assert!(
        t.y >= o.y && t.y + t.h <= o.y + o.h + 0.01,
        "target must be inside outer viewport: {t:?}"
    );
    assert!(
        t.y >= i.y && t.y + t.h <= i.y + i.h + 0.01,
        "target must be inside inner viewport: {t:?}"
    );
    assert_eq!(hit_test(a.world(), t.x + 12.0, t.y + 12.0), Some(target));
}
