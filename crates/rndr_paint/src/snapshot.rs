//! Stable text dumps of a [`Frame`](crate::Frame) for snapshot tests.

use crate::{Frame, SurfaceRect};

/// Dump `frame` as stable text for snapshot tests.
///
/// Rects, clips, and radii round to the nearest 0.5 point, so sub-point float
/// noise does not change the text. Colors print to three decimals and texture
/// coordinates to four. Quads stay in paint order. Slots are sorted by id, then
/// by rounded origin, so component-store iteration order stays out of the dump.
pub fn quads_snapshot(frame: &Frame) -> String {
    let mut lines = Vec::new();
    for (index, quad) in frame.quads.iter().enumerate() {
        lines.push(format!(
            "[{index}] rect {}",
            geom(quad.rect.x, quad.rect.y, quad.rect.w, quad.rect.h)
        ));
        lines.push(format!(
            "    clip {}",
            geom(quad.clip.x, quad.clip.y, quad.clip.w, quad.clip.h)
        ));
        lines.push(format!("    radius {:.1}", half_point(quad.radius)));
        lines.push(format!(
            "    rgba {:.3},{:.3},{:.3},{:.3}",
            quad.color.r, quad.color.g, quad.color.b, quad.color.a
        ));
        lines.push(format!(
            "    uv {:.4},{:.4},{:.4},{:.4}",
            quad.uv[0], quad.uv[1], quad.uv[2], quad.uv[3]
        ));
    }
    let mut slots: Vec<&SurfaceRect> = frame.slots.iter().collect();
    slots.sort_by_key(|slot| slot_key(slot));
    for slot in slots {
        lines.push(format!(
            "slot {} rect {}",
            slot.id,
            geom(slot.rect.x, slot.rect.y, slot.rect.w, slot.rect.h)
        ));
    }
    lines.join("\n")
}

fn geom(x: f32, y: f32, w: f32, h: f32) -> String {
    format!(
        "{:.1},{:.1} {:.1}x{:.1}",
        half_point(x),
        half_point(y),
        half_point(w),
        half_point(h)
    )
}

/// Nearest 0.5 point. Halfway cases round away from zero. Negative zero prints as zero.
fn half_point(value: f32) -> f32 {
    if !value.is_finite() {
        return value;
    }
    let rounded = (value * 2.0).round() / 2.0;
    if rounded == 0.0 {
        0.0
    } else {
        rounded
    }
}

fn slot_key(slot: &SurfaceRect) -> (u64, u32, u32, u32, u32) {
    (
        slot.id,
        half_point(slot.rect.x).to_bits(),
        half_point(slot.rect.y).to_bits(),
        half_point(slot.rect.w).to_bits(),
        half_point(slot.rect.h).to_bits(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Frame, Quad};
    use rndr_core::{Clip, Color, Rect};

    #[test]
    fn geometry_rounds_to_the_nearest_half_point() {
        let frame = Frame {
            quads: vec![Quad {
                rect: Rect {
                    x: 1.25,
                    y: 0.75,
                    w: 10.5,
                    h: 10.0 + 1e-4,
                },
                color: Color::rgba(0.2, 0.22, 0.24, 1.0),
                uv: [0.0, 0.5, 1.0, 1.0],
                clip: Clip {
                    x: -0.25,
                    y: 0.25,
                    w: 8.0,
                    h: -0.0,
                },
                radius: 1.25,
                atlas: 0,
            }],
            slots: vec![
                SurfaceRect {
                    id: 9,
                    rect: Rect {
                        x: 2.0,
                        y: 1.0,
                        w: 3.0,
                        h: 4.0,
                    },
                },
                SurfaceRect {
                    id: 3,
                    rect: Rect {
                        x: 0.25,
                        y: -0.25,
                        w: 1.0,
                        h: 1.0,
                    },
                },
            ],
            draws: Vec::new(),
        };
        assert_eq!(
            quads_snapshot(&frame),
            "\
[0] rect 1.5,1.0 10.5x10.0
    clip -0.5,0.5 8.0x0.0
    radius 1.5
    rgba 0.200,0.220,0.240,1.000
    uv 0.0000,0.5000,1.0000,1.0000
slot 3 rect 0.5,-0.5 1.0x1.0
slot 9 rect 2.0,1.0 3.0x4.0"
        );
    }
}
