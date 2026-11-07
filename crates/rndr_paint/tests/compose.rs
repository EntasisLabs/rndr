//! `Opacity` and `Transform` move a subtree into one composited layer.

use rndr_core::{
    set_parent, App, Clip, Color, Entity, Layout, Opacity, Rect, Size, Transform, Viewport,
};
use rndr_paint::{frame, Affine, ClipShape, Op};

fn boxed(app: &mut App, w: f32, h: f32, color: Color) -> Entity {
    app.world_mut().spawn((
        Layout {
            width: Size::Px(w),
            height: Size::Px(h),
            ..Layout::row()
        },
        color,
    ))
}

// Root, then a group with one nested child, then a later sibling.
fn scene() -> (App, Entity, Entity) {
    let mut app = App::new();
    app.world_mut().insert_resource(Viewport::new(200.0, 100.0));
    let root = boxed(&mut app, 200.0, 100.0, Color::rgb(1.0, 1.0, 1.0));
    let group = boxed(&mut app, 40.0, 40.0, Color::rgb(1.0, 0.0, 0.0));
    let inner = boxed(&mut app, 20.0, 20.0, Color::rgb(0.0, 0.0, 1.0));
    let after = boxed(&mut app, 20.0, 20.0, Color::rgb(0.0, 1.0, 0.0));
    set_parent(app.world_mut(), group, root);
    set_parent(app.world_mut(), inner, group);
    set_parent(app.world_mut(), after, root);
    (app, root, group)
}

#[test]
fn plain_and_identity_values_stay_on_the_quad_path() {
    let (mut app, _, _) = scene();
    let plain = frame(&mut app);
    assert_eq!(plain.quads.len(), 4);
    assert!(plain.draws.is_empty());
    let (mut app, _, group) = scene();
    app.world_mut().insert(group, Opacity(1.0));
    app.world_mut().insert(group, Transform::IDENTITY);
    let same = frame(&mut app);
    assert_eq!(same.quads.len(), 4);
    assert!(same.draws.is_empty());
}

#[test]
fn opacity_groups_the_subtree_in_one_layer_at_its_paint_position() {
    let (mut app, _, group) = scene();
    app.world_mut().insert(group, Opacity(0.5));
    let painted = frame(&mut app);
    // Root before the group, the later sibling after it.
    assert_eq!(painted.quads.len(), 2);
    assert_eq!(painted.quads[0].color.r, 1.0);
    assert_eq!(painted.quads[1].color.g, 1.0);
    assert!(painted.draws.iter().all(|placed| placed.z == 1));
    let ops: Vec<&Op> = painted.draws.iter().map(|placed| &placed.op).collect();
    assert_eq!(ops.len(), 3);
    let Op::PushLayer(layer) = ops[0] else {
        panic!("expected a layer, got {:?}", ops[0]);
    };
    assert_eq!(layer.opacity, 0.5);
    assert_eq!(layer.transform, Affine::IDENTITY);
    assert!(layer.clip.is_none());
    let Op::Quads(quads) = ops[1] else {
        panic!("expected quads, got {:?}", ops[1]);
    };
    assert_eq!(quads.len(), 2);
    assert_eq!(quads[0].color.r, 1.0);
    assert_eq!(quads[1].color.b, 1.0);
    assert!(matches!(ops[2], Op::PopLayer));
}

#[test]
fn zero_opacity_paints_nothing_for_the_subtree() {
    let (mut app, _, group) = scene();
    app.world_mut().insert(group, Opacity(0.0));
    let painted = frame(&mut app);
    assert_eq!(painted.quads.len(), 2);
    assert!(painted.draws.is_empty());
}

#[test]
fn transform_composites_inside_the_parent_clip() {
    let (mut app, root, group) = scene();
    let transform = Transform::rotate(0.3).with_origin(0.0, 0.0);
    app.world_mut().insert(group, transform);
    let painted = frame(&mut app);
    let rect = *app.world().get::<Rect>(group).unwrap();
    let parent = *app.world().get::<Clip>(root).unwrap();
    let Op::PushLayer(layer) = &painted.draws[0].op else {
        panic!("expected a layer");
    };
    assert_eq!(layer.opacity, 1.0);
    assert_eq!(layer.transform.m, transform.matrix(rect));
    let Some(ClipShape::Scissor(scissor)) = &layer.clip else {
        panic!("expected the parent scissor, got {:?}", layer.clip);
    };
    assert_eq!(
        [scissor.x, scissor.y, scissor.w, scissor.h],
        [parent.x, parent.y, parent.w, parent.h]
    );
}

#[test]
fn nested_groups_nest_layers() {
    let (mut app, _, group) = scene();
    app.world_mut().insert(group, Opacity(0.5));
    let inner = rndr_core::children_of(app.world(), group)[0];
    app.world_mut()
        .insert(inner, Transform::translate(5.0, 0.0));
    let painted = frame(&mut app);
    let kinds: Vec<&str> = painted
        .draws
        .iter()
        .map(|placed| match placed.op {
            Op::PushLayer(_) => "push",
            Op::PopLayer => "pop",
            Op::Quads(_) => "quads",
            _ => "other",
        })
        .collect();
    assert_eq!(kinds, ["push", "quads", "push", "quads", "pop", "pop"]);
}
