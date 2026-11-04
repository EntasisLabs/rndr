use rndr_core::*;

#[test]
fn pointer_thumb_highlight_clears_on_release_without_stealing_focus() {
    let mut world = World::new();
    world.insert_resource(Input::default());
    let target = world.spawn((
        Rect {
            x: 40.0,
            y: 0.0,
            w: 100.0,
            h: 100.0,
        },
        Scroll {
            content: [100.0, 500.0],
            offset: [0.0, 0.0],
        },
    ));
    world.insert_resource(Focus {
        entity: Some(target),
    });
    let bar = world.spawn((
        Rect {
            x: 0.0,
            y: 0.0,
            w: 24.0,
            h: 100.0,
        },
        Scrollbar {
            target,
            axis: Axis::Column,
        },
        Focusable,
    ));
    let idle = rndr_paint::paint(&world);
    world.resource_mut::<Input>().unwrap().pointer = Pointer {
        position: [12.0, 10.0],
        press_at: Some([12.0, 10.0]),
        pressed: true,
        down: true,
        ..Default::default()
    };
    apply_scrollbars(&mut world);
    assert!(scrollbar_dragging(&world, bar));
    let held = rndr_paint::paint(&world);
    assert_ne!(idle[0].color, held[0].color);
    world.resource_mut::<Input>().unwrap().pointer = Pointer {
        position: [12.0, 10.0],
        released: true,
        ..Default::default()
    };
    apply_scrollbars(&mut world);
    assert!(!scrollbar_dragging(&world, bar));
    assert_eq!(world.resource::<Focus>().unwrap().entity, Some(target));
    let released = rndr_paint::paint(&world);
    assert_eq!(idle[0].color, released[0].color);
    // Explicit keyboard/AT focus still provides a visible focus affordance.
    world.resource_mut::<Focus>().unwrap().entity = Some(bar);
    assert_ne!(rndr_paint::paint(&world)[0].color, idle[0].color);
}

#[test]
fn interactive_thumb_paint_uses_canonical_clipped_geometry_without_mutating_range() {
    for axis in [Axis::Row, Axis::Column] {
        let mut world = World::new();
        let target = world.spawn((
            Rect {
                x: 0.,
                y: 0.,
                w: 100.,
                h: 100.,
            },
            Scroll {
                offset: [-75., -75.],
                content: [400., 400.],
            },
        ));
        let track = if axis == Axis::Row {
            Rect {
                x: 0.,
                y: 100.,
                w: 100.,
                h: 18.,
            }
        } else {
            Rect {
                x: 100.,
                y: 0.,
                w: 18.,
                h: 100.,
            }
        };
        let bar = world.spawn((
            track,
            Clip::from_rect(track),
            Scrollbar { target, axis },
            Focusable,
        ));
        let geometry = scrollbar_geometry(&world, bar).unwrap();
        let quads = rndr_paint::paint(&world);
        assert_eq!(quads.len(), 1);
        let thumb = quads[0].rect;
        if axis == Axis::Row {
            assert_eq!((thumb.x, thumb.w), (geometry.thumb.x, geometry.thumb.w));
            assert!(thumb.h < geometry.thumb.h);
        } else {
            assert_eq!((thumb.y, thumb.h), (geometry.thumb.y, geometry.thumb.h));
            assert!(thumb.w < geometry.thumb.w);
        }
        assert_eq!(scrollbar_geometry(&world, bar).unwrap(), geometry);
        world.insert(bar, Hidden);
        assert!(rndr_paint::paint(&world).is_empty());
    }
}
