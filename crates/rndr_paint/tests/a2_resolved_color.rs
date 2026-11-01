use rndr_core::*;
#[test]
fn resolved_color_preserves_focus_pair_and_legacy_still_brightens() {
    let mut app = App::new();
    app.add_plugin(GenericInteraction);
    let color = Color::rgb(40.0 / 255.0, 93.0 / 255.0, 168.0 / 255.0);
    let root = app.world_mut().spawn((Layout {
        width: Size::Px(180.0),
        ..Layout::column()
    },));
    let legacy = app.world_mut().spawn((
        Layout {
            width: Size::Px(80.0),
            height: Size::Px(40.0),
            ..Layout::column()
        },
        Button::new("legacy"),
        color,
    ));
    set_parent(app.world_mut(), legacy, root);
    let modern = app.world_mut().spawn((
        Layout {
            width: Size::Px(80.0),
            height: Size::Px(40.0),
            ..Layout::column()
        },
        Button::new("modern"),
        color,
        ResolvedColor,
    ));
    set_parent(app.world_mut(), modern, root);
    app.update();
    for (e, unchanged) in [(legacy, false), (modern, true)] {
        request_focus(app.world_mut(), e);
        let frame = rndr_paint::frame(&mut app);
        let r = *app.world().get::<Rect>(e).unwrap();
        let q = frame
            .quads
            .iter()
            .find(|q| q.rect == r && q.uv == rndr_paint::solid_uv())
            .unwrap();
        assert_eq!(q.color == color, unchanged);
    }
}
