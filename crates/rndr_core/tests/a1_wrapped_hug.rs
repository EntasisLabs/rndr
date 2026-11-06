use rndr_core::*;
fn append(world: &mut World, parent: Entity, bundle: impl Bundle) -> Entity {
    let e = world.spawn(bundle);
    set_parent(world, e, parent);
    e
}
fn contains_hug_children(world: &World) {
    for parent in world.entities_with::<Layout>() {
        let layout = world.get::<Layout>(parent).unwrap();
        if layout.height != Size::Hug {
            continue;
        }
        let rect = *world.get::<Rect>(parent).unwrap();
        for child in children_of(world, parent) {
            if world.has::<Hidden>(child) {
                continue;
            }
            let child_rect = *world.get::<Rect>(child).unwrap();
            assert!(
                child_rect.y >= rect.y - 0.01
                    && child_rect.y + child_rect.h <= rect.y + rect.h + 0.01,
                "{parent:?} {rect:?} clips {child:?} {child_rect:?}"
            );
        }
    }
}
#[test]
fn nested_wrapped_hug_heights_are_correct_on_first_frame_resize_and_scale() {
    let mut app = App::new();
    app.add_plugin(Chrome);
    app.world_mut()
        .insert_resource(Viewport::new(900.0, 20000.0));
    let root = app.world_mut().spawn((Layout {
        width: Size::Px(280.0),
        ..Layout::column()
    },));
    let card = append(
        app.world_mut(),
        root,
        (Layout {
            width: Size::Fill,
            padding: 16.0,
            gap: 8.0,
            ..Layout::column()
        },),
    );
    append(
        app.world_mut(),
        card,
        (
            Layout {
                width: Size::Fill,
                ..Layout::column()
            },
            Label::new("A very long translated-style title that must wrap")
                .overflow(TextOverflow::Wrap),
            TextStyle {
                size: 24.0,
                ..TextStyle::default()
            },
        ),
    );
    let slot = append(
        app.world_mut(),
        card,
        (Layout {
            width: Size::Fill,
            ..Layout::column()
        },),
    );
    let body = append(
        app.world_mut(),
        slot,
        (
            Layout {
                width: Size::Fill,
                ..Layout::column()
            },
            Label::new("Nested reading content wraps without clipping. ".repeat(8))
                .overflow(TextOverflow::Wrap),
            TextStyle {
                size: 16.0,
                ..TextStyle::default()
            },
        ),
    );
    for width in [280.0, 160.0, 600.0, 240.0] {
        for size in [16.0, 32.0] {
            app.world_mut().get_mut::<Layout>(root).unwrap().width = Size::Px(width);
            app.world_mut().get_mut::<TextStyle>(body).unwrap().size = size;
            app.update();
            contains_hug_children(app.world());
        }
    }
}
#[test]
fn row_fill_widths_determine_hug_cross_height_before_clipping() {
    let mut app = App::new();
    app.add_plugin(Chrome);
    app.world_mut()
        .insert_resource(Viewport::new(800.0, 20000.0));
    let row = app.world_mut().spawn((Layout {
        width: Size::Px(240.0),
        gap: 8.0,
        ..Layout::row()
    },));
    for _ in 0..2 {
        let column = append(
            app.world_mut(),
            row,
            (Layout {
                width: Size::Fill,
                padding: 8.0,
                ..Layout::column()
            },),
        );
        append(
            app.world_mut(),
            column,
            (
                Layout {
                    width: Size::Fill,
                    ..Layout::column()
                },
                Label::new("Long wrapping row content. ".repeat(10)).overflow(TextOverflow::Wrap),
            ),
        );
    }
    app.update();
    contains_hug_children(app.world());
}
