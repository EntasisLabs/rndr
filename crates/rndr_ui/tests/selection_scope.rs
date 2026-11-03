use rndr_core::*;
use rndr_ui::{selection::*, UiId, UiPlugin};
fn setup() -> (App, Entity, SelectionHandle) {
    let mut a = App::new();
    a.add_plugin(UiPlugin);
    a.world_mut().insert_resource(Viewport::new(400., 400.));
    let r = a.world_mut().spawn((Layout {
        width: Size::Fill,
        height: Size::Fill,
        ..Layout::column()
    },));
    let h = MultiSelect::mount(
        a.world_mut(),
        r,
        UiId(1),
        SelectionProps::new("Channels"),
        &[SelectOption::new(1, "Design"), SelectOption::new(2, "Code")],
        &[],
    )
    .unwrap();
    rndr_paint::frame(&mut a);
    request_focus(a.world_mut(), h.root);
    a.world_mut().resource_mut::<Input>().unwrap().keys =
        vec![KeyEvent::named(NamedKey::Enter, Modifiers::default())];
    rndr_paint::frame(&mut a);
    (a, r, h)
}
#[test]
fn invalid_escape_tab_edges_never_close_or_transact_and_real_escape_does_not_dismiss_background_layer(
) {
    let (mut a, r, h) = setup();
    let p = h.popup(a.world()).unwrap();
    a.world_mut().insert(r, Layer(0));
    for named in [NamedKey::Escape, NamedKey::Tab] {
        for mode in 0..3 {
            let mut k = KeyEvent::named(named, Modifiers::default());
            match mode {
                0 => k.synthetic = true,
                1 => k.repeat = true,
                _ => k.released = true,
            }
            a.world_mut().resource_mut::<Input>().unwrap().keys = vec![k];
            rndr_paint::frame(&mut a);
            assert!(!a.world().has::<Hidden>(p));
            assert!(a.world().is_alive(r));
            assert!(events::<SelectionChange>(a.world()).is_empty());
        }
    }
    a.world_mut().resource_mut::<Input>().unwrap().keys =
        vec![KeyEvent::named(NamedKey::Escape, Modifiers::default())];
    rndr_paint::frame(&mut a);
    assert!(a.world().has::<Hidden>(p));
    assert!(a.world().is_alive(r));
    assert_eq!(a.world().resource::<Focus>().unwrap().entity, Some(h.root));
}
#[test]
fn higher_modal_takes_focus_scope_and_unmount_retires_detached_popup() {
    let (mut a, _, h) = setup();
    let p = h.popup(a.world()).unwrap();
    let modal = a.world_mut().spawn((
        Layout::column(),
        Layer(10),
        Modal,
        Button::new("modal"),
        Focusable,
    ));
    request_focus(a.world_mut(), modal);
    rndr_paint::frame(&mut a);
    assert!(a.world().has::<Hidden>(p));
    assert_eq!(a.world().resource::<Focus>().unwrap().entity, Some(modal));
    h.unmount(a.world_mut()).unwrap();
    assert!(!a.world().is_alive(p));
    assert!(!a.world().is_alive(h.root));
    assert!(h.unmount(a.world_mut()).is_err());
}
#[test]
fn offscreen_anchor_placement_stays_in_viewport_on_both_sides() {
    for y in [-1000., -10., 0., 100., 400., 2000.] {
        let r = popup_rect(
            Rect {
                x: -100.,
                y,
                w: 600.,
                h: 60.,
            },
            Viewport::new(300., 400.),
            500.,
            320.,
        );
        assert!(r.x >= 0. && r.y >= 0. && r.x + r.w <= 300. && r.y + r.h <= 400.);
    }
}
