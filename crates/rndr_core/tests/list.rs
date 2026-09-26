use rndr_core::{
    children_of, App, Axis, Chrome, Color, Input, Layout, List, ListItem, Rect, Size, Viewport,
};

fn list_layout() -> Layout {
    Layout {
        axis: Axis::Column,
        gap: 0.0,
        padding: 0.0,
        width: Size::Px(120.0),
        height: Size::Px(200.0),
        min_width: 0.0,
        min_height: 0.0,
    }
}

fn boot() -> (App, rndr_core::Entity) {
    let mut app = App::new();
    app.add_plugin(Chrome);
    app.world_mut().insert_resource(Viewport::new(160.0, 240.0));
    let list = app.world_mut().spawn((
        list_layout(),
        Color::rgb(0.1, 0.1, 0.12),
        List::new(10_000, 20.0),
    ));
    app.update();
    app.update();
    (app, list)
}

fn pool(app: &App, list: rndr_core::Entity) -> Vec<rndr_core::Entity> {
    children_of(app.world(), list)
        .into_iter()
        .filter(|child| app.world().has::<ListItem>(*child))
        .collect()
}

#[test]
fn pool_tracks_the_visible_window() {
    let (app, list) = boot();
    let rows = pool(&app, list);
    assert_eq!(rows.len(), 10);
    assert_eq!(app.world().get::<ListItem>(rows[0]).unwrap().index, 0);
    assert!(app.world().entities_with::<ListItem>().len() < 20);
}

#[test]
fn offset_maps_to_the_first_index() {
    let (mut app, list) = boot();
    app.world_mut().get_mut::<List>(list).unwrap().offset = 200.0;
    app.update();
    let rows = pool(&app, list);
    assert_eq!(rows.len(), 10);
    assert_eq!(app.world().get::<ListItem>(rows[0]).unwrap().index, 10);
}

#[test]
fn click_reports_the_item_index() {
    let (mut app, list) = boot();
    app.world_mut().get_mut::<List>(list).unwrap().offset = 200.0;
    app.update();
    let row = pool(&app, list)[0];
    let rect = *app.world().get::<Rect>(row).unwrap();
    let input = app.world_mut().resource_mut::<Input>().unwrap();
    input.pointer.position = [rect.x + 4.0, rect.y + 4.0];
    input.pointer.pressed = true;
    input.pointer.down = true;
    app.update();
    assert_eq!(app.world().get::<List>(list).unwrap().clicked, Some(10));
}
