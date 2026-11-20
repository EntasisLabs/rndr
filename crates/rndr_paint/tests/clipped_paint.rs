//! Culling ordinary paint must not cull descendants or independent draw paths.
use rndr_core::{
    set_parent, BackdropBlur, Blur, Border, Clip, Color, Gradient, Label, Opacity, Rect, Shadow,
    Shadows, Transform, World,
};
use rndr_paint::{paint_subtree, Op, QuadKind};

fn rect(x: f32, y: f32) -> Rect {
    Rect {
        x,
        y,
        w: 60.0,
        h: 30.0,
    }
}

fn clip() -> Clip {
    Clip {
        x: 0.0,
        y: 0.0,
        w: 100.0,
        h: 100.0,
    }
}

// A zero-stop gradient emits nothing but retains the ordinary (unculled) path.
// This compares exact quad and operation payloads/order, not only visible count.
fn assert_matches_unculled(world: &mut World, root: rndr_core::Entity, row: rndr_core::Entity) {
    let actual = paint_subtree(world, root);
    world.insert(
        row,
        Gradient::Linear {
            angle: 0.0,
            stops: vec![],
        },
    );
    let reference = paint_subtree(world, root);
    assert_eq!(format!("{actual:?}"), format!("{reference:?}"));
}

#[test]
fn empty_clip_skips_only_own_paint_and_preserves_visible_descendants() {
    for (w, h) in [(0.0, 100.0), (100.0, 0.0), (-1.0, 100.0)] {
        let mut world = World::new();
        let row = world.spawn((
            rect(200.0, 200.0),
            Clip { w, h, ..clip() },
            Label::new("clipped fallback label"),
            Color::rgb(1.0, 0.0, 0.0),
        ));
        let child = world.spawn((
            rect(10.0, 10.0),
            clip(),
            Label::new("visible overflow"),
            Color::rgb(0.0, 1.0, 0.0),
        ));
        set_parent(&mut world, child, row);
        let (quads, draws) = paint_subtree(&world, row);
        assert!(draws.is_empty());
        assert!(quads.iter().any(|q| q.color == Color::rgb(0.0, 1.0, 0.0)));
        assert!(!quads.iter().any(|q| q.color == Color::rgb(1.0, 0.0, 0.0)));
        assert_matches_unculled(&mut world, row, row);
    }
}

#[test]
fn nonempty_clip_keeps_text_overflow_even_when_box_is_offscreen() {
    let mut world = World::new();
    // Text starts left of the viewport but extends into it. Checking just the
    // layout box against the viewport would incorrectly discard the label.
    let row = world.spawn((
        Rect {
            x: -70.0,
            y: 0.0,
            w: 5.0,
            h: 30.0,
        },
        clip(),
        Label::new("long overflowing text"),
    ));
    assert!(!paint_subtree(&world, row).0.is_empty());
    assert_matches_unculled(&mut world, row, row);
}

#[test]
fn clipped_entity_keeps_transforms_blur_opacity_and_geometry_order() {
    let mut world = World::new();
    let root = world.spawn((rect(0.0, 0.0), clip()));
    let row = world.spawn((
        rect(200.0, 200.0),
        Clip { h: 0.0, ..clip() },
        Label::new("clipped"),
        Opacity(0.5),
        Transform::translate(-200.0, -200.0),
        Blur(2.0),
    ));
    world.insert(
        row,
        Border {
            width: 1.0,
            color: Color::rgb(0.0, 0.0, 1.0),
        },
    );
    set_parent(&mut world, row, root);
    let child = world.spawn((rect(10.0, 10.0), clip(), Color::rgb(0.0, 1.0, 0.0)));
    set_parent(&mut world, child, row);
    let (_, draws) = paint_subtree(&world, root);
    assert!(draws.iter().any(|p| matches!(p.op, Op::PushLayer(_))));
    assert!(draws.iter().any(|p| matches!(p.op, Op::Quads(_))));
    assert!(matches!(draws.last().unwrap().op, Op::PopLayer));
    assert_matches_unculled(&mut world, root, row);
}

#[test]
fn empty_own_clip_keeps_shadow_using_parent_clip_and_backdrop_operation() {
    let mut world = World::new();
    let root = world.spawn((rect(0.0, 0.0), clip()));
    let row = world.spawn((
        rect(100.0, 0.0),
        Clip { w: 0.0, ..clip() },
        Label::new("clipped"),
        Shadows(vec![Shadow::drop(0.0, 8.0, Color::rgb(0.0, 0.0, 0.0))]),
        BackdropBlur(3.0),
    ));
    set_parent(&mut world, row, root);
    let (quads, draws) = paint_subtree(&world, root);
    assert!(quads
        .iter()
        .any(|q| matches!(q.shape.kind, QuadKind::Shadow { .. }) && q.clip.w == 100.0));
    assert!(draws.iter().any(|p| matches!(p.op, Op::Backdrop(_))));
    assert_matches_unculled(&mut world, root, row);
}

#[test]
fn missing_clip_retains_fallback_paint() {
    let mut world = World::new();
    let row = world.spawn((rect(0.0, 0.0), Label::new("visible")));
    assert!(!paint_subtree(&world, row).0.is_empty());
    assert_matches_unculled(&mut world, row, row);
}
