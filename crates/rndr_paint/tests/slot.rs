use rndr_core::{
    set_parent, shell, splitter, App, Axis, Color, Input, Layout, Rect, Scroll, Shell, Size,
    SurfaceSlot, Viewport,
};
use rndr_paint::frame;

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

fn slot_quads(app: &mut App, id: u64) -> (Rect, Vec<Rect>) {
    let painted = frame(app);
    let slot = painted
        .slots
        .iter()
        .find(|slot| slot.id == id)
        .expect("slot")
        .rect;
    let quads = painted.quads.into_iter().map(|quad| quad.rect).collect();
    (slot, quads)
}

fn interior_is_empty(slot: Rect, quads: &[Rect]) {
    for quad in quads {
        let inside = quad.x >= slot.x
            && quad.y >= slot.y
            && quad.x + quad.w <= slot.x + slot.w + 0.01
            && quad.y + quad.h <= slot.y + slot.h + 0.01
            && quad.w > 1.0
            && quad.h > 1.0;
        assert!(!inside, "quad {quad:?} covers slot {slot:?}");
    }
}

#[test]
fn slot_follows_scroll() {
    let mut app = App::new();
    app.world_mut().insert_resource(Viewport::new(200.0, 200.0));
    let (port, slot) = {
        let world = app.world_mut();
        let port = world.spawn((
            Layout {
                axis: Axis::Column,
                gap: 0.0,
                padding: 0.0,
                width: Size::Px(200.0),
                height: Size::Px(100.0),
                min_width: 0.0,
                min_height: 0.0,
            },
            Scroll::default(),
            Color::rgb(0.1, 0.1, 0.1),
        ));
        let slot = world.spawn((
            px(180.0, 180.0),
            Color::rgb(0.2, 0.2, 0.3),
            SurfaceSlot { id: 7 },
        ));
        set_parent(world, slot, port);
        (port, slot)
    };
    app.update();
    let before = app.world().get::<Rect>(slot).unwrap().y;
    app.world_mut().get_mut::<Scroll>(port).unwrap().offset[1] = -30.0;
    let (reported, quads) = slot_quads(&mut app, 7);
    let after = app.world().get::<Rect>(slot).unwrap();
    assert!(after.y < before);
    assert_eq!(reported, *after);
    interior_is_empty(reported, &quads);
}

#[test]
fn slot_follows_split() {
    let mut app = App::new();
    app.add_plugin(Shell);
    app.world_mut().insert_resource(Viewport::new(800.0, 400.0));
    let (seam, slot) = {
        let world = app.world_mut();
        let parent = world.spawn((shell(), Color::rgb(0.08, 0.08, 0.08)));
        let leading = world.spawn((
            Layout {
                axis: Axis::Column,
                gap: 0.0,
                padding: 0.0,
                width: Size::Px(200.0),
                height: Size::Fill,
                min_width: 0.0,
                min_height: 0.0,
            },
            Color::rgb(0.2, 0.2, 0.2),
        ));
        set_parent(world, leading, parent);
        let (split_layout, split) = splitter();
        let seam = world.spawn((split_layout, split, Color::rgb(0.3, 0.3, 0.3)));
        set_parent(world, seam, parent);
        let slot = world.spawn((fill(), Color::rgb(0.15, 0.16, 0.2), SurfaceSlot { id: 3 }));
        set_parent(world, slot, parent);
        (seam, slot)
    };
    app.update();
    let before = app.world().get::<Rect>(slot).unwrap().x;
    let seam_rect = *app.world().get::<Rect>(seam).unwrap();
    let input = app.world_mut().resource_mut::<Input>().unwrap();
    input.pointer.position = [seam_rect.x + seam_rect.w / 2.0, 40.0];
    input.pointer.pressed = true;
    input.pointer.down = true;
    app.update();
    let input = app.world_mut().resource_mut::<Input>().unwrap();
    input.pointer.position[0] += 48.0;
    input.pointer.pressed = false;
    input.pointer.down = true;
    let (reported, quads) = slot_quads(&mut app, 3);
    assert!(reported.x > before);
    interior_is_empty(reported, &quads);
}
