use rndr_core::*;

#[test]
fn clipped_thumb_intervals_are_correct_on_both_axes_and_do_not_change_hits() {
    // No layout rewrite: independently supply exact geometry for both leading
    // and trailing clips, interior offsets, and both scroll boundaries.
    for axis in 0..2 {
        for (leading, extent) in [(0., 100.), (30., 70.), (0., 40.), (20., 60.)] {
            for offset in [0., -50., -200.] {
                let mut w = World::new();
                let rect = Rect {
                    x: 10.,
                    y: 20.,
                    w: 100.,
                    h: 100.,
                };
                let mut clip = Clip::from_rect(rect);
                if axis == 0 {
                    clip.x += leading;
                    clip.w = extent;
                } else {
                    clip.y += leading;
                    clip.h = extent;
                }
                let color = Color::rgb(0.2, 0.4, 0.6);
                let mut scroll = Scroll {
                    content: [100., 100.],
                    offset: [0., 0.],
                };
                scroll.content[axis] = 300.;
                scroll.offset[axis] = offset;
                let entity = w.spawn((rect, clip, scroll, ScrollIndicator { color }));
                let before = hit_test(&w, clip.x + 1., clip.y + 1.);
                let quads = rndr_paint::paint(&w);
                assert_eq!(before, Some(entity));
                assert_eq!(hit_test(&w, clip.x + 1., clip.y + 1.), before);
                assert_eq!(hit_test(&w, rect.x - 1., rect.y), None);
                assert_eq!(quads.len(), 1);
                let thumb = quads[0].rect;
                let track = extent - 8.;
                let size = (track * extent / 300.).max(18.).min(track);
                let position = (-offset + leading) / (300. - extent) * (track - size);
                let (start, actual_size, origin) = if axis == 0 {
                    (thumb.x, thumb.w, clip.x)
                } else {
                    (thumb.y, thumb.h, clip.y)
                };
                assert!((actual_size - size).abs() < 0.001);
                assert!((start - origin - 4. - position).abs() < 0.001);
                assert!(thumb.x >= clip.x && thumb.y >= clip.y);
                assert!(thumb.x + thumb.w <= clip.x + clip.w);
                assert!(thumb.y + thumb.h <= clip.y + clip.h);
            }
        }
    }
}

#[test]
fn near_empty_cross_axis_clip_does_not_emit_outside_indicator_geometry() {
    let mut w = World::new();
    w.spawn((
        Rect {
            x: 0.,
            y: 0.,
            w: 100.,
            h: 100.,
        },
        Clip {
            x: 0.,
            y: 0.,
            w: 1.,
            h: 100.,
        },
        Scroll {
            offset: [0., 0.],
            content: [100., 300.],
        },
        ScrollIndicator {
            color: Color::rgb(1., 1., 1.),
        },
    ));
    assert!(rndr_paint::paint(&w).is_empty());
}
