//! Quads in logical points. No window and no GPU.
//!
//! [`paint`] walks a [`rndr_core::World`] and returns one [`Quad`] per solid,
//! glyph, selection, caret, and dirty mark. A surface plugin multiplies those
//! rects by [`rndr_core::Viewport::scale`] when it uploads them.

mod font;

pub use font::{atlas_r8, glyph_uv, solid_uv, ATLAS_H, ATLAS_W};

use rndr_core::{
    children_of, cols_for, roots, visual_pos, visual_selection_spans, wrap_lines, App, Caret, Clip,
    Color, DirtyMark, EditField, Field, Focus, Label, Parent, Rect, World, CELL_H, CELL_W,
    TEXT_ORIGIN_X, TEXT_ORIGIN_Y,
};

/// One instanced quad in logical points.
#[derive(Clone, Copy, Debug)]
pub struct Quad {
    pub rect: Rect,
    pub color: Color,
    pub uv: [f32; 4],
    pub clip: Clip,
}

/// One world frame, then the quads in points.
///
/// Runs Startup once, then Update, Layout, and the render schedule. The
/// surface multiplies the returned quads by [`rndr_core::Viewport::scale`]
/// when it presents. This does not open a window.
pub fn frame(app: &mut App) -> Vec<Quad> {
    app.update();
    let quads = paint(app.world());
    app.render();
    quads
}

/// Paint every colored rect in the world, parents before children.
pub fn paint(world: &World) -> Vec<Quad> {
    let mut out = Vec::new();
    for root in roots::<Rect>(world) {
        walk(world, root, &mut |rect,
                                color,
                                clip,
                                label,
                                caret,
                                dirty,
                                wrap| {
            push(&mut out, rect, color, solid_uv(), clip);
            if dirty {
                push_dirty_mark(&mut out, rect, clip);
            }
            if let (Some(label), Some(caret)) = (label, caret) {
                push_selection(&mut out, rect, label, caret, clip, wrap);
            }
            if let Some(label) = label {
                push_label(&mut out, rect, label, clip, wrap);
            }
            if let Some(caret) = caret {
                push_caret(&mut out, rect, label, caret, clip, wrap);
            }
        });
    }
    out
}

fn walk(
    world: &World,
    entity: rndr_core::Entity,
    visit: &mut impl FnMut(Rect, Color, Clip, Option<&Label>, Option<&Caret>, bool, bool),
) {
    if !world.is_alive(entity) {
        return;
    }
    if let (Some(rect), Some(color)) = (world.get::<Rect>(entity), world.get::<Color>(entity)) {
        let clip = world
            .get::<Clip>(entity)
            .copied()
            .unwrap_or_else(|| Clip::from_rect(*rect));
        let focused = world.resource::<Focus>().and_then(|focus| focus.entity);
        let highlight = focused == Some(entity)
            || world
                .get::<Parent>(entity)
                .is_some_and(|parent| focused == Some(parent.entity))
            || world.has::<Caret>(entity);
        let color = if highlight { lighten(*color) } else { *color };
        let dirty = world.has::<DirtyMark>(entity);
        let wrap = world
            .get::<Field>(entity)
            .is_some_and(|field| field.kind == EditField::Body);
        visit(
            *rect,
            color,
            clip,
            world.get::<Label>(entity),
            world.get::<Caret>(entity),
            dirty,
            wrap,
        );
    }
    for child in children_of(world, entity) {
        walk(world, child, visit);
    }
}

fn lighten(color: Color) -> Color {
    Color::rgba(
        color.r + (1.0 - color.r) * 0.25,
        color.g + (1.0 - color.g) * 0.25,
        color.b + (1.0 - color.b) * 0.25,
        color.a,
    )
}

fn push(out: &mut Vec<Quad>, rect: Rect, color: Color, uv: [f32; 4], clip: Clip) {
    out.push(Quad {
        rect,
        color,
        uv,
        clip,
    });
}

fn push_dirty_mark(out: &mut Vec<Quad>, rect: Rect, clip: Clip) {
    let size = 6.0;
    push(
        out,
        Rect {
            x: rect.x + 3.0,
            y: rect.y + TEXT_ORIGIN_Y + (CELL_H - size) * 0.5,
            w: size,
            h: size,
        },
        Color::rgb(0.86, 0.62, 0.20),
        solid_uv(),
        clip,
    );
}

fn text_cols(rect: Rect, wrap: bool) -> usize {
    if wrap {
        cols_for(rect.w)
    } else {
        usize::MAX
    }
}

fn push_selection(
    out: &mut Vec<Quad>,
    rect: Rect,
    label: &Label,
    caret: &Caret,
    clip: Clip,
    wrap: bool,
) {
    let color = Color::rgba(0.35, 0.55, 0.85, 0.55);
    let lines = wrap_lines(&label.text, text_cols(rect, wrap));
    for (line, start_col, end_col) in visual_selection_spans(&lines, caret.anchor, caret.index) {
        let cols = end_col.saturating_sub(start_col);
        let w = if cols == 0 { 2.0 } else { cols as f32 * CELL_W };
        push(
            out,
            Rect {
                x: rect.x + TEXT_ORIGIN_X + start_col as f32 * CELL_W,
                y: rect.y + TEXT_ORIGIN_Y + line as f32 * CELL_H,
                w,
                h: CELL_H,
            },
            color,
            solid_uv(),
            clip,
        );
    }
}

fn push_label(out: &mut Vec<Quad>, rect: Rect, label: &Label, clip: Clip, wrap: bool) {
    let mut y = rect.y + TEXT_ORIGIN_Y;
    let color = Color::rgb(0.94, 0.95, 0.97);
    for line in wrap_lines(&label.text, text_cols(rect, wrap)) {
        let mut x = rect.x + TEXT_ORIGIN_X;
        for ch in label.text[line.start..line.end].bytes() {
            if ch == b' ' {
                x += CELL_W;
                continue;
            }
            push(
                out,
                Rect {
                    x,
                    y,
                    w: CELL_W,
                    h: CELL_H,
                },
                color,
                glyph_uv(ch),
                clip,
            );
            x += CELL_W;
        }
        y += CELL_H;
    }
}

fn push_caret(
    out: &mut Vec<Quad>,
    rect: Rect,
    label: Option<&Label>,
    caret: &Caret,
    clip: Clip,
    wrap: bool,
) {
    let text = label.map(|label| label.text.as_str()).unwrap_or("");
    let (line, col) = visual_pos(&wrap_lines(text, text_cols(rect, wrap)), caret.index);
    push(
        out,
        Rect {
            x: rect.x + TEXT_ORIGIN_X + col as f32 * CELL_W,
            y: rect.y + TEXT_ORIGIN_Y + line as f32 * CELL_H,
            w: 2.0,
            h: CELL_H,
        },
        Color::rgb(0.94, 0.95, 0.97),
        solid_uv(),
        clip,
    );
}
