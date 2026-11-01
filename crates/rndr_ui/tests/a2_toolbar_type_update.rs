use rndr_core::*;
use rndr_ui::{
    controls::{self, *},
    *,
};
#[test]
fn toolbar_reflows_using_current_resolved_type_in_the_same_update() {
    let mut a = App::new();
    a.add_plugin(UiPlugin);
    let root = a.world_mut().spawn((Layout {
        width: Size::Px(400.0),
        ..Layout::column()
    },));
    let t = Toolbar::mount(a.world_mut(), root, UiId(1), 400.0).unwrap();
    for i in 0..3 {
        controls::Button::mount(
            a.world_mut(),
            t.root,
            UiId(i),
            ButtonProps::new("MMMM", HostIntent::Command(i)),
        )
        .unwrap();
    }
    a.update();
    assert_eq!(a.world().get::<Layout>(t.root).unwrap().axis, Axis::Row);
    a.world_mut().insert_resource(UiTheme {
        text_scale: 2.0,
        ..UiTheme::default()
    });
    a.update();
    assert_eq!(
        a.world().get::<Layout>(t.root).unwrap().axis,
        Axis::Column,
        "do not measure previous-frame TextStyle and overflow for a frame"
    );
    let frame = rndr_paint::frame(&mut a);
    assert!(frame.quads.iter().any(|q| q.uv != rndr_paint::solid_uv()));
}
