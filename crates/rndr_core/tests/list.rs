use rndr_core::{
    children_of, events, platform_of, set_parent, App, Axis, Chrome, Color, Context, Focus, Input,
    KeyEvent, Label, Layout, List, ListActivated, ListItem, ListKeys, ListRows, Modifiers,
    NamedKey, Rect, Size, Viewport, LIST_ACTIVATE,
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
    assert_eq!(app.world().get::<List>(list).unwrap().selected, Some(10));
}

fn navigable(count: u32, height: f32) -> (App, rndr_core::Entity) {
    let mut app = App::new();
    app.add_plugin(Chrome);
    app.world_mut()
        .insert_resource(Viewport::new(320.0, height + 80.0));
    app.world_mut().insert_resource(Focus::default());
    let root = app.world_mut().spawn((
        Layout {
            axis: Axis::Column,
            gap: 0.0,
            padding: 0.0,
            width: Size::Px(160.0),
            height: Size::Px(height),
            min_width: 0.0,
            min_height: 0.0,
        },
        Context::new("Palette"),
        Color::rgb(0.08, 0.09, 0.1),
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
        List::new(count, 20.0),
        ListKeys,
    ));
    set_parent(app.world_mut(), list, root);
    app.world_mut().resource_mut::<Focus>().unwrap().entity = Some(root);
    app.update();
    app.update();
    (app, list)
}

fn press(app: &mut App, key: NamedKey) {
    let platform = platform_of(app.world());
    app.world_mut()
        .resource_mut::<Input>()
        .unwrap()
        .push_key(KeyEvent::named(key, Modifiers::new()), platform);
    app.update();
    let input = app.world_mut().resource_mut::<Input>().unwrap();
    input.keys.clear();
    input.key = None;
    input.chars.clear();
}

fn selected(app: &App, list: rndr_core::Entity) -> Option<u32> {
    app.world().get::<List>(list).unwrap().selected
}

#[test]
fn arrows_page_home_and_end_move_the_selection() {
    let (mut app, list) = navigable(100, 200.0);
    assert_eq!(selected(&app, list), Some(0));
    press(&mut app, NamedKey::ArrowDown);
    press(&mut app, NamedKey::ArrowDown);
    assert_eq!(selected(&app, list), Some(2));
    press(&mut app, NamedKey::ArrowUp);
    assert_eq!(selected(&app, list), Some(1));
    press(&mut app, NamedKey::PageDown);
    assert_eq!(selected(&app, list), Some(11));
    press(&mut app, NamedKey::PageUp);
    assert_eq!(selected(&app, list), Some(1));
    press(&mut app, NamedKey::End);
    assert_eq!(selected(&app, list), Some(99));
    press(&mut app, NamedKey::Home);
    assert_eq!(selected(&app, list), Some(0));
    press(&mut app, NamedKey::ArrowUp);
    assert_eq!(selected(&app, list), Some(0));
}

#[test]
fn shrinking_the_count_clamps_selection_and_resets_scroll() {
    let (mut app, list) = navigable(100, 200.0);
    {
        let list = app.world_mut().get_mut::<List>(list).unwrap();
        list.selected = Some(40);
        list.offset = 800.0;
        list.count = 10;
    }
    app.update();
    let list = app.world().get::<List>(list).unwrap();
    assert_eq!(list.selected, Some(9));
    assert_eq!(list.offset, 0.0);

    let (mut app, list) = navigable(100, 200.0);
    {
        let list = app.world_mut().get_mut::<List>(list).unwrap();
        list.selected = Some(4);
        list.offset = 500.0;
        list.count = 30;
    }
    app.update();
    let list = app.world().get::<List>(list).unwrap();
    assert_eq!(list.selected, Some(4));
    assert_eq!(list.offset, 0.0);
}

#[test]
fn selection_scrolls_into_view() {
    let (mut app, list) = navigable(100, 200.0);
    {
        let list = app.world_mut().get_mut::<List>(list).unwrap();
        list.selected = Some(50);
        list.offset = 0.0;
    }
    app.update();
    let offset = app.world().get::<List>(list).unwrap().offset;
    assert_eq!(offset, 820.0);
    let rows = pool(&app, list);
    let first = app.world().get::<ListItem>(rows[0]).unwrap().index;
    let last = app
        .world()
        .get::<ListItem>(*rows.last().unwrap())
        .unwrap()
        .index;
    assert!(first <= 50 && last >= 50, "row window {first}..={last}");

    press(&mut app, NamedKey::End);
    let list = app.world().get::<List>(list).unwrap();
    assert_eq!(list.selected, Some(99));
    assert_eq!(list.offset, 1800.0);
}

#[test]
fn enter_and_double_click_activate_the_row() {
    let (mut app, list) = navigable(20, 200.0);
    press(&mut app, NamedKey::ArrowDown);
    press(&mut app, NamedKey::Enter);
    let activated = events::<ListActivated>(app.world()).to_vec();
    assert_eq!(activated, vec![ListActivated { list, index: 1 }]);
    assert!(app
        .world()
        .resource::<rndr_core::KeymapFired>()
        .unwrap()
        .actions
        .iter()
        .any(|action| action == LIST_ACTIVATE));

    let (mut app, list) = navigable(20, 200.0);
    let row = pool(&app, list)[3];
    let rect = *app.world().get::<Rect>(row).unwrap();
    let x = rect.x + 4.0;
    let y = rect.y + 4.0;
    click(&mut app, x, y);
    assert!(events::<ListActivated>(app.world()).is_empty());
    click(&mut app, x, y);
    assert_eq!(
        events::<ListActivated>(app.world()),
        &[ListActivated { list, index: 3 }]
    );
}

fn click(app: &mut App, x: f32, y: f32) {
    {
        let input = app.world_mut().resource_mut::<Input>().unwrap();
        input.pointer.position = [x, y];
        input.pointer.pressed = true;
        input.pointer.down = true;
    }
    app.update();
    let input = app.world_mut().resource_mut::<Input>().unwrap();
    input.pointer.pressed = false;
}

#[test]
fn hover_selects_the_row_under_the_pointer() {
    let (mut app, list) = navigable(20, 200.0);
    {
        let input = app.world_mut().resource_mut::<Input>().unwrap();
        input.pointer.position = [-20.0, -20.0];
    }
    app.update();
    let row = pool(&app, list)[4];
    let rect = *app.world().get::<Rect>(row).unwrap();
    app.world_mut()
        .resource_mut::<Input>()
        .unwrap()
        .pointer
        .position = [rect.x + 3.0, rect.y + 3.0];
    app.update();
    assert_eq!(selected(&app, list), Some(4));
}

fn paint_title(world: &mut rndr_core::World, row: rndr_core::Entity, index: u32) {
    if let Some(label) = world.get_mut::<Label>(row) {
        label.text = format!("item-{index}");
    }
}

#[test]
fn row_builder_supplies_text_and_rows_follow_width() {
    let (mut app, list) = boot();
    app.world_mut().insert(list, ListRows(paint_title));
    app.world_mut().get_mut::<List>(list).unwrap().offset = 200.0;
    app.update();
    let rows = pool(&app, list);
    assert_eq!(app.world().get::<Label>(rows[0]).unwrap().text, "item-10");
    let width = app.world().get::<Rect>(rows[0]).unwrap().w;
    assert!((width - 120.0).abs() < 0.5, "row width {width}");

    app.world_mut().get_mut::<Layout>(list).unwrap().width = Size::Px(220.0);
    app.world_mut().resource_mut::<Viewport>().unwrap().width = 400.0;
    app.update();
    let rows = pool(&app, list);
    let width = app.world().get::<Rect>(rows[0]).unwrap().w;
    assert!((width - 220.0).abs() < 0.5, "resized row width {width}");
}

#[test]
fn rows_exist_on_the_first_frame() {
    let mut app = App::new();
    app.add_plugin(Chrome);
    app.world_mut().insert_resource(Viewport::new(160.0, 240.0));
    let list = app.world_mut().spawn((
        list_layout(),
        Color::rgb(0.1, 0.1, 0.12),
        List::new(30, 20.0),
    ));
    app.update();
    let rows = pool(&app, list);
    assert_eq!(rows.len(), 10);
    assert!(app.world().get::<Rect>(rows[0]).unwrap().h > 0.0);

    let mut app = App::new();
    app.add_plugin(Chrome);
    app.world_mut().insert_resource(Viewport::new(200.0, 300.0));
    let parent = app.world_mut().spawn(list_layout());
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
        List::new(30, 20.0),
    ));
    set_parent(app.world_mut(), list, parent);
    app.update();
    let rows = pool(&app, list);
    assert_eq!(rows.len(), 10);
    assert!(app.world().get::<Rect>(rows[0]).unwrap().h > 0.0);
}

fn selection_fully_inside(app: &App, list: rndr_core::Entity, height: f32) {
    let state = app.world().get::<List>(list).unwrap();
    let selected = state.selected.expect("selection");
    let top = selected as f32 * state.row_height;
    let bottom = top + state.row_height;
    assert!(
        top + 0.01 >= state.offset,
        "row {selected} starts at {top}, offset {}",
        state.offset
    );
    assert!(
        bottom <= state.offset + height + 0.01,
        "row {selected} ends at {bottom}, view {}",
        state.offset + height
    );
    let row = pool(app, list)
        .into_iter()
        .find(|row| app.world().get::<ListItem>(*row).unwrap().index == selected)
        .expect("selected row is pooled");
    let rect = app.world().get::<Rect>(row).unwrap();
    let list_rect = app.world().get::<Rect>(list).unwrap();
    assert!(rect.y + 0.5 >= list_rect.y);
    assert!(rect.y + rect.h <= list_rect.y + list_rect.h + 0.5);
}

#[test]
fn selection_stays_fully_visible_past_a_partial_row() {
    let (mut app, list) = navigable(100, 190.0);
    for _ in 0..9 {
        press(&mut app, NamedKey::ArrowDown);
    }
    assert_eq!(selected(&app, list), Some(9));
    selection_fully_inside(&app, list, 190.0);
    press(&mut app, NamedKey::ArrowDown);
    assert_eq!(selected(&app, list), Some(10));
    selection_fully_inside(&app, list, 190.0);

    press(&mut app, NamedKey::End);
    assert_eq!(selected(&app, list), Some(99));
    selection_fully_inside(&app, list, 190.0);
    press(&mut app, NamedKey::PageUp);
    assert_eq!(selected(&app, list), Some(90));
    selection_fully_inside(&app, list, 190.0);
    press(&mut app, NamedKey::Home);
    assert_eq!(selected(&app, list), Some(0));
    selection_fully_inside(&app, list, 190.0);
    press(&mut app, NamedKey::PageDown);
    assert_eq!(selected(&app, list), Some(9));
    selection_fully_inside(&app, list, 190.0);
}
