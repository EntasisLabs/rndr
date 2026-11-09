//! `Blur` becomes a blurred `SceneLayer`; `BackdropBlur` an `Op::Backdrop`.

use rndr_core::{set_parent, App, BackdropBlur, Blur, Color, Corners, Layout, Size, Viewport};
use rndr_paint::{frame, ClipShape, Op};

fn sized(w: f32, h: f32) -> Layout {
    Layout {
        width: Size::Px(w),
        height: Size::Px(h),
        ..Layout::column()
    }
}

fn app() -> App {
    let mut app = App::new();
    app.world_mut().insert_resource(Viewport::new(100.0, 50.0));
    app
}

#[test]
fn blur_groups_the_subtree_and_clips_to_the_parent() {
    let mut app = app();
    let root = app
        .world_mut()
        .spawn((sized(100.0, 50.0), Color::rgb(1.0, 1.0, 1.0)));
    let card = app
        .world_mut()
        .spawn((sized(40.0, 30.0), Color::rgb(0.0, 0.0, 0.0), Blur(5.0)));
    set_parent(app.world_mut(), card, root);
    let painted = frame(&mut app);
    let Op::PushLayer(layer) = &painted.draws[0].op else {
        panic!("expected a layer");
    };
    assert_eq!(layer.blur, 5.0);
    assert_eq!(layer.opacity, 1.0);
    let Some(ClipShape::Scissor(clip)) = layer.clip else {
        panic!("expected the parent's clip");
    };
    assert_eq!((clip.w, clip.h), (100.0, 50.0));
    assert!(matches!(painted.draws.last().unwrap().op, Op::PopLayer));
}

#[test]
fn zero_or_bad_blur_paints_without_a_layer() {
    for sigma in [0.0, -2.0, f32::NAN] {
        let mut app = app();
        app.world_mut()
            .spawn((sized(40.0, 30.0), Color::rgb(0.0, 0.0, 0.0), Blur(sigma)));
        let painted = frame(&mut app);
        assert!(painted.draws.is_empty(), "{sigma}");
        assert_eq!(painted.quads.len(), 1);
    }
}

#[test]
fn backdrop_sits_before_the_entity_fill_with_its_largest_corner() {
    let mut app = app();
    let root = app
        .world_mut()
        .spawn((sized(100.0, 50.0), Color::rgb(1.0, 1.0, 1.0)));
    let panel = app.world_mut().spawn((
        sized(40.0, 30.0),
        Color::rgba(1.0, 1.0, 1.0, 0.4),
        Corners([2.0, 8.0, 4.0, 0.0]),
        BackdropBlur(6.0),
    ));
    let child = app
        .world_mut()
        .spawn((sized(10.0, 10.0), Color::rgb(0.0, 0.0, 1.0)));
    set_parent(app.world_mut(), panel, root);
    set_parent(app.world_mut(), child, panel);
    let painted = frame(&mut app);
    assert_eq!(painted.draws.len(), 1);
    assert_eq!(
        painted.draws[0].z, 1,
        "after the root, before the panel fill"
    );
    let Op::Backdrop(op) = &painted.draws[0].op else {
        panic!("expected a backdrop");
    };
    assert_eq!((op.rect.w, op.rect.h), (40.0, 30.0));
    assert_eq!(op.radius, 8.0);
    assert_eq!(op.sigma, 6.0);
    assert_eq!(op.clip.map(|clip| clip.w), Some(100.0));
    assert_eq!(painted.quads.len(), 3);
}
