//! Paint decorations become typed quads in paint order.

use rndr_core::{
    set_parent, App, Border, BorderSides, ClipRounded, Color, Corners, Entity, Gradient, Layout,
    Shadow, Shadows, Size, Viewport,
};
use rndr_paint::{frame, QuadKind, QuadShape};

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

fn scene() -> (App, Entity) {
    let mut app = App::new();
    app.world_mut().insert_resource(Viewport::new(200.0, 100.0));
    let root = boxed(&mut app, 200.0, 100.0, Color::rgb(1.0, 1.0, 1.0));
    let tile = boxed(&mut app, 40.0, 40.0, Color::rgb(0.5, 0.5, 0.5));
    set_parent(app.world_mut(), tile, root);
    (app, tile)
}

#[test]
fn undecorated_quads_keep_the_plain_shape() {
    let (mut app, tile) = scene();
    app.world_mut().insert(
        tile,
        Border {
            color: Color::rgb(0.0, 0.0, 0.0),
            width: 1.0,
        },
    );
    let plain = frame(&mut app);
    assert_eq!(plain.quads.len(), 2);
    assert!(plain.quads.iter().all(|q| q.shape == QuadShape::default()));
    assert!(!plain.draws.is_empty(), "a Border stays a stroked path");
}

#[test]
fn decorations_paint_shadow_fill_gradient_inset_then_ring() {
    let (mut app, tile) = scene();
    let world = app.world_mut();
    world.insert(tile, Corners([8.0, 0.0, 8.0, 0.0]));
    world.insert(
        tile,
        Gradient::vertical(Color::rgb(1.0, 0.0, 0.0), Color::rgb(0.0, 0.0, 1.0)),
    );
    let mut inset = Shadow::drop(2.0, 4.0, Color::rgba(0.0, 0.0, 0.0, 0.5));
    inset.inset = true;
    world.insert(
        tile,
        Shadows(vec![
            Shadow::drop(4.0, 8.0, Color::rgba(0.0, 0.0, 0.0, 0.5)),
            inset,
        ]),
    );
    world.insert(
        tile,
        Border {
            color: Color::rgb(0.0, 0.0, 0.0),
            width: 1.0,
        },
    );
    world.insert(
        tile,
        BorderSides::uniform(Color::rgb(0.0, 0.0, 1.0), 2.0).dashed(4.0, 2.0),
    );
    let painted = frame(&mut app);
    assert!(
        painted.draws.is_empty(),
        "BorderSides replaces the Border path"
    );
    let quads = &painted.quads[1..];
    assert_eq!(quads.len(), 5, "{quads:#?}");
    let corners = Some([8.0, 0.0, 8.0, 0.0]);
    match quads[0].shape.kind {
        QuadKind::Shadow {
            shape,
            element,
            sigma,
            inset: false,
        } => {
            assert_eq!(sigma, 4.0, "blur is 2 sigma");
            assert_eq!((shape.y, element.y), (4.0, 0.0));
            assert!(quads[0].rect.x < 0.0 && quads[0].rect.h > 40.0);
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(quads[1].shape.kind, QuadKind::Fill);
    assert_eq!(quads[1].shape.corners, corners);
    match quads[2].shape.kind {
        QuadKind::Linear {
            start,
            end,
            range,
            extend,
            ..
        } => {
            assert_eq!((start, end), ([20.0, 0.0], [20.0, 40.0]));
            assert_eq!((range, extend), ([0.0, 1.0], [true, true]));
        }
        other => panic!("{other:?}"),
    }
    assert!(matches!(
        quads[3].shape.kind,
        QuadKind::Shadow { inset: true, .. }
    ));
    assert_eq!(
        quads[4].shape.kind,
        QuadKind::Ring {
            widths: [2.0; 4],
            dash: [4.0, 2.0],
        }
    );
    assert!(quads.iter().all(|q| q.shape.corners == corners));
}

#[test]
fn clip_rounded_masks_the_subtree_and_inner_masks_win() {
    let (mut app, tile) = scene();
    let child = boxed(&mut app, 20.0, 20.0, Color::rgb(1.0, 0.0, 0.0));
    let inner = boxed(&mut app, 10.0, 10.0, Color::rgb(0.0, 0.0, 1.0));
    let world = app.world_mut();
    set_parent(world, child, tile);
    set_parent(world, inner, child);
    world.insert(tile, ClipRounded::radius(10.0).feathered(4.0));
    world.insert(inner, ClipRounded::radius(2.0));
    let painted = frame(&mut app);
    let quads = &painted.quads;
    assert_eq!(quads.len(), 4, "{quads:#?}");
    assert_eq!(
        quads[0].shape.round_clip, None,
        "the root is outside the mask"
    );
    let outer = quads[1].shape.round_clip.expect("the tile masks itself");
    assert_eq!(outer.rect, quads[1].rect);
    assert_eq!(outer.corners, [10.0; 4]);
    assert_eq!(outer.feather, 4.0);
    assert_eq!(quads[2].shape.round_clip, Some(outer));
    let nested = quads[3].shape.round_clip.expect("inner mask");
    assert_eq!(nested.corners, [2.0; 4]);
    assert_eq!(nested.rect, quads[3].rect);
}
