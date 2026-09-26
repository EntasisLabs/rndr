use rndr_core::{
    set_parent, App, Axis, Buffer, CapturesKeys, Chrome, Color, Editor, Focus, Gutter, Input,
    Label, Layout, Rect, Size, TextLayout, Viewport, CELL_H, TEXT_ORIGIN_X, TEXT_ORIGIN_Y,
};

fn px(w: f32, h: f32) -> Layout {
    Layout {
        axis: Axis::Column,
        gap: 0.0,
        padding: 0.0,
        width: Size::Px(w),
        height: Size::Px(h),
        min_width: 0.0,
        min_height: 0.0,
    }
}

fn fill() -> Layout {
    Layout {
        axis: Axis::Column,
        gap: 0.0,
        padding: 0.0,
        width: Size::Fill,
        height: Size::Fill,
        min_width: 0.0,
        min_height: 0.0,
    }
}

#[test]
fn insert_and_delete_in_the_middle() {
    let mut buffer = Buffer::from_str("ab\nc");
    assert_eq!(buffer.line_count(), 2);
    assert_eq!(buffer.line_start(1), 3);
    buffer.insert_str(1, "XY");
    assert_eq!(buffer.text(), "aXYb\nc");
    buffer.delete_range(1, 3);
    assert_eq!(buffer.text(), "ab\nc");
    assert_eq!(buffer.line_start(1), 3);
}

fn boot(text: &str, height: f32) -> (App, rndr_core::Entity, rndr_core::Entity) {
    let mut app = App::new();
    app.add_plugin(Chrome);
    app.world_mut()
        .insert_resource(Viewport::new(300.0, height));
    app.world_mut().insert_resource(Focus::default());
    let (body, gutter) = {
        let world = app.world_mut();
        let row = world.spawn((
            Layout {
                axis: Axis::Row,
                gap: 0.0,
                padding: 0.0,
                width: Size::Px(280.0),
                height: Size::Px(height),
                min_width: 0.0,
                min_height: 0.0,
            },
            Color::rgb(0.1, 0.1, 0.1),
        ));
        let gutter = world.spawn((
            px(40.0, height),
            Color::rgb(0.12, 0.12, 0.14),
            Label::new(""),
            Gutter,
        ));
        set_parent(world, gutter, row);
        let body = world.spawn((
            fill(),
            Color::rgb(0.14, 0.15, 0.16),
            Label::new(""),
            Buffer::from_str(text),
            Editor::default(),
            CapturesKeys,
        ));
        set_parent(world, body, row);
        (body, gutter)
    };
    app.update();
    app.update();
    (app, body, gutter)
}

#[test]
fn visible_window_does_not_place_every_line() {
    let text = "a\n".repeat(100_000);
    let (app, body, gutter) = boot(&text, 200.0);
    let glyphs = app.world().get::<TextLayout>(body).unwrap().glyphs.len();
    assert!(glyphs < 100, "glyphs {glyphs}");
    assert!(glyphs > 0);
    assert_eq!(
        app.world().get::<Buffer>(body).unwrap().line_count(),
        100_000
    );
    let numbers = &app.world().get::<Label>(gutter).unwrap().text;
    assert!(numbers.lines().count() < 40);
    assert!(numbers.starts_with('1'));
}

#[test]
fn gutter_follows_the_scrolled_window() {
    let text = "a\n".repeat(100);
    let (mut app, body, gutter) = boot(&text, 200.0);
    app.world_mut().get_mut::<Editor>(body).unwrap().offset = 49.0 * CELL_H;
    app.update();
    let numbers = &app.world().get::<Label>(gutter).unwrap().text;
    assert!(numbers.starts_with("50"));
    assert!(numbers.lines().count() < 40);
}

#[test]
fn click_maps_to_a_buffer_offset() {
    let (mut app, body, _) = boot("hello\nworld", 80.0);
    app.world_mut().resource_mut::<Focus>().unwrap().entity = Some(body);
    let rect = *app.world().get::<Rect>(body).unwrap();
    let input = app.world_mut().resource_mut::<Input>().unwrap();
    input.pointer.position = [
        rect.x + TEXT_ORIGIN_X + 1.0,
        rect.y + TEXT_ORIGIN_Y + CELL_H + 1.0,
    ];
    input.pointer.pressed = true;
    input.pointer.down = true;
    app.update();
    assert_eq!(app.world().get::<Buffer>(body).unwrap().caret, 6);
}
