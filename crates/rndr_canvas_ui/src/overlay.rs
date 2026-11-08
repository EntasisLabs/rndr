//! Editor chrome drawn over the scene: hover outline, selection frame and
//! handles, marquee, lasso, eraser trail and snap guides. Drawn in scene
//! coordinates with sizes divided by the zoom, so it keeps a constant size
//! on screen.

use crate::editor::{handle_points, rotate_point, CanvasEditor, HANDLE_SIZE};
use rndr_canvas::layout::Axis;
use rndr_canvas::text::TextMeasure;
use rndr_canvas::{LineGeometry, Rect as CanvasRect, SceneIndex, Vec2};
use rndr_core::{Color, Rect};
use rndr_paint::{Canvas, Cap, FillRule, Path, Stroke};

/// Overlay colors.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OverlayTheme {
    pub accent: Color,
    pub handle_fill: Color,
    pub guide: Color,
    pub marquee_fill: Color,
    pub eraser: Color,
}

impl Default for OverlayTheme {
    fn default() -> Self {
        Self {
            accent: Color::rgba(0.23, 0.51, 0.96, 1.0),
            handle_fill: Color::rgba(1.0, 1.0, 1.0, 1.0),
            guide: Color::rgba(0.93, 0.27, 0.53, 1.0),
            marquee_fill: Color::rgba(0.23, 0.51, 0.96, 0.08),
            eraser: Color::rgba(0.5, 0.5, 0.5, 0.35),
        }
    }
}

fn p(v: Vec2) -> [f32; 2] {
    [v.x as f32, v.y as f32]
}

fn outline(canvas: &mut Canvas, points: &[Vec2], color: Color, width: f32) {
    if points.len() < 2 {
        return;
    }
    let pts: Vec<[f32; 2]> = points.iter().map(|v| p(*v)).collect();
    canvas.stroke(Path::polygon(&pts), color, Stroke::solid(width));
}

fn square(canvas: &mut Canvas, at: Vec2, size: f32, theme: &OverlayTheme, width: f32) {
    let half = size / 2.0;
    let rect = Rect {
        x: at.x as f32 - half,
        y: at.y as f32 - half,
        w: size,
        h: size,
    };
    canvas.fill_stroke(
        Path::round_rect(rect, size * 0.2),
        theme.handle_fill,
        FillRule::NonZero,
        theme.accent,
        Stroke::solid(width),
    );
}

fn corners(r: CanvasRect) -> Vec<Vec2> {
    r.corners().to_vec()
}

/// Paints the editor's overlay; call after painting the scene with the
/// same camera.
pub fn paint_overlay(
    canvas: &mut Canvas,
    editor: &CanvasEditor,
    measure: &dyn TextMeasure,
    theme: &OverlayTheme,
) {
    let zoom = editor.camera.zoom.max(1e-6) as f32;
    let px = 1.0 / zoom;
    let nodes = editor.nodes();
    let index = SceneIndex::with_measure(&nodes, measure);
    let state = editor.overlay();

    let node_outline = |canvas: &mut Canvas, id: &str, width: f32| {
        let Some(node) = index.get(id) else { return };
        let m = index.world_matrix(node);
        if let Some(line) = LineGeometry::of(node) {
            canvas.stroke(
                Path::line(p(m.apply(line.start)), p(m.apply(line.end))),
                theme.accent,
                Stroke::solid(width).with_cap(Cap::Round),
            );
            return;
        }
        let (w, h) = index.dimensions(node);
        let pts: Vec<Vec2> = CanvasRect::new(0.0, 0.0, w, h)
            .corners()
            .iter()
            .map(|c| m.apply(*c))
            .collect();
        outline(canvas, &pts, theme.accent, width);
    };

    if let Some(hover) = &state.hover {
        if !editor.selection().contains(hover) {
            node_outline(canvas, hover, px * 1.5);
        }
    }
    if let Some(id) = editor.editing_text() {
        node_outline(canvas, id, px);
    }
    let selected = index.selection_roots(editor.selection());
    if selected.len() > 1 {
        for node in &selected {
            node_outline(canvas, &node.id, px);
        }
    }
    let frame = state
        .rotating
        .or_else(|| index.selection_frame(editor.selection()));
    if let (Some(frame), false) = (frame, selected.is_empty()) {
        let single_line = selected.len() == 1 && LineGeometry::of(selected[0]).is_some();
        if single_line {
            let node = selected[0];
            let line = LineGeometry::of(node).expect("checked above");
            let m = index.world_matrix(node);
            node_outline(canvas, &node.id, px);
            for end in [line.start, line.end] {
                square(canvas, m.apply(end), HANDLE_SIZE * px, theme, px);
            }
        } else {
            let box_points: Vec<Vec2> =
                corners(CanvasRect::new(0.0, 0.0, frame.width, frame.height))
                    .into_iter()
                    .map(|c| frame.matrix.apply(c))
                    .collect();
            outline(canvas, &box_points, theme.accent, px * 1.5);
            let top = frame.matrix.apply(Vec2::new(frame.width / 2.0, 0.0));
            let knob = rotate_point(&frame, editor.camera.zoom);
            canvas.stroke(Path::line(p(top), p(knob)), theme.accent, Stroke::solid(px));
            canvas.fill_stroke(
                Path::ellipse(p(knob), [HANDLE_SIZE * px * 0.6; 2]),
                theme.handle_fill,
                FillRule::NonZero,
                theme.accent,
                Stroke::solid(px),
            );
            for (_, at) in handle_points(&frame) {
                square(canvas, at, HANDLE_SIZE * px, theme, px);
            }
        }
    }
    if let Some(m) = state.marquee {
        let r = Rect {
            x: m.x as f32,
            y: m.y as f32,
            w: m.width as f32,
            h: m.height as f32,
        };
        canvas.fill_stroke(
            Path::round_rect(r, 0.0),
            theme.marquee_fill,
            FillRule::NonZero,
            theme.accent,
            Stroke::solid(px),
        );
    }
    if state.lasso.len() > 1 {
        let pts: Vec<[f32; 2]> = state.lasso.iter().map(|v| p(*v)).collect();
        canvas.fill_stroke(
            Path::polygon(&pts),
            theme.marquee_fill,
            FillRule::EvenOdd,
            theme.accent,
            Stroke::solid(px).dashed(vec![4.0 * px, 3.0 * px]),
        );
    }
    let trail = &state.eraser;
    for pair in trail.windows(2) {
        canvas.stroke(
            Path::line(p(pair[0]), p(pair[1])),
            theme.eraser,
            Stroke::solid(8.0 * px).with_cap(Cap::Round),
        );
    }
    for guide in &state.guides {
        let (a, b) = match guide.axis {
            Axis::X => (
                Vec2::new(guide.value, guide.from),
                Vec2::new(guide.value, guide.to),
            ),
            Axis::Y => (
                Vec2::new(guide.from, guide.value),
                Vec2::new(guide.to, guide.value),
            ),
        };
        canvas.stroke(Path::line(p(a), p(b)), theme.guide, Stroke::solid(px));
    }
}
