//! Selected and unselected rows, with the text the row builder supplied.

use rndr_core::{
    App, Axis, Chrome, Color, Context, Focus, Label, Layout, List, ListItem, ListKeys, ListRows,
    Size, Viewport,
};
use rndr_paint::{frame, quads_snapshot};

fn build(world: &mut rndr_core::World, row: rndr_core::Entity, index: u32) {
    let selected = world.get::<ListItem>(row).is_some_and(|item| item.selected);
    world.insert(
        row,
        if selected {
            Color::rgb(0.24, 0.40, 0.62)
        } else {
            Color::rgb(0.12, 0.13, 0.15)
        },
    );
    if let Some(label) = world.get_mut::<Label>(row) {
        label.text = if index == 0 { "A" } else { "B" }.to_string();
    }
}

#[test]
fn selected_row_paints_the_builder_text() {
    let mut app = App::new();
    app.add_plugin(Chrome);
    app.world_mut().insert_resource(Viewport::new(200.0, 80.0));
    app.world_mut().insert_resource(Focus::default());
    let root = app.world_mut().spawn((
        Layout {
            axis: Axis::Column,
            gap: 0.0,
            padding: 0.0,
            width: Size::Px(80.0),
            height: Size::Px(40.0),
            min_width: 0.0,
            min_height: 0.0,
        },
        Context::new("Palette"),
    ));
    let list = app.world_mut().spawn((
        Layout {
            axis: Axis::Column,
            gap: 0.0,
            padding: 0.0,
            width: Size::Fill,
            height: Size::Fill,
            min_width: 0.0,
            min_height: 0.0,
        },
        Color::rgb(0.1, 0.1, 0.12),
        List::new(2, 20.0),
        ListKeys,
        ListRows(build),
    ));
    rndr_core::set_parent(app.world_mut(), list, root);
    app.world_mut().resource_mut::<Focus>().unwrap().entity = Some(root);
    // The first frame places the list. The next one pools the rows.
    frame(&mut app);
    let painted = frame(&mut app);
    insta::assert_snapshot!(quads_snapshot(&painted));
}
