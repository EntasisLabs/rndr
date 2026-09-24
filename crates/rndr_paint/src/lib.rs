//! Quads in logical points. No window and no GPU.
//!
//! [`paint`] walks a [`rndr_core::World`] and returns one [`Quad`] per solid,
//! glyph, selection, caret, and dirty mark. A surface plugin multiplies those
//! rects by [`rndr_core::Viewport::scale`] when it uploads them.

mod font;

pub use font::{atlas_r8, glyph_uv, solid_uv, ATLAS_H, ATLAS_W};

use rndr_core::{
    children_of, field_text_width, layout_text, roots, text_face, App, Caret, Clip, Color,
    DirtyMark, EditField, Field, Focus, Label, Parent, Rect, TextLayout, World, CELL_H,
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
                                _label,
                                caret,
                                dirty,
                                layout| {
            push(&mut out, rect, color, solid_uv(), clip);
            if dirty {
                push_dirty_mark(&mut out, rect, clip);
            }
            if let (Some(layout), Some(caret)) = (layout.as_ref(), caret) {
                push_selection(&mut out, rect, layout, caret, clip);
            }
            if let Some(layout) = layout.as_ref() {
                push_label(&mut out, rect, layout, clip);
            }
            if let Some(caret) = caret {
                push_caret(&mut out, rect, layout.as_ref(), caret, clip);
            }
        });
    }
    out
}

fn walk(
    world: &World,
    entity: rndr_core::Entity,
    visit: &mut impl FnMut(Rect, Color, Clip, Option<&Label>, Option<&Caret>, bool, Option<TextLayout>),
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
        let layout = world.get::<Label>(entity).map(|label| {
            world
                .get::<TextLayout>(entity)
                .cloned()
                .unwrap_or_else(|| measured(world, entity, &label.text, *rect))
        });
        visit(
            *rect,
            color,
            clip,
            world.get::<Label>(entity),
            world.get::<Caret>(entity),
            dirty,
            layout,
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

fn measured(world: &World, entity: rndr_core::Entity, text: &str, rect: Rect) -> TextLayout {
    let font = text_face(world);
    let wrap = world
        .get::<Field>(entity)
        .is_some_and(|field| field.kind == EditField::Body);
    let max_width = if wrap {
        field_text_width(rect.w, font.advance('0'))
    } else {
        f32::INFINITY
    };
    layout_text(font.as_ref(), text, max_width)
}

fn push_selection(out: &mut Vec<Quad>, rect: Rect, layout: &TextLayout, caret: &Caret, clip: Clip) {
    let color = Color::rgba(0.35, 0.55, 0.85, 0.55);
    for span in layout.selection_boxes(caret.anchor, caret.index) {
        push(
            out,
            Rect {
                x: rect.x + TEXT_ORIGIN_X + span.x,
                y: rect.y + TEXT_ORIGIN_Y + span.y,
                w: span.w,
                h: span.h,
            },
            color,
            solid_uv(),
            clip,
        );
    }
}

fn push_label(out: &mut Vec<Quad>, rect: Rect, layout: &TextLayout, clip: Clip) {
    let color = Color::rgb(0.94, 0.95, 0.97);
    for glyph in &layout.glyphs {
        if !glyph.ch.is_ascii() {
            continue;
        }
        let code = glyph.ch as u8;
        push(
            out,
            Rect {
                x: rect.x + TEXT_ORIGIN_X + glyph.x,
                y: rect.y + TEXT_ORIGIN_Y + glyph.y,
                w: glyph.w,
                h: glyph.h,
            },
            color,
            glyph_uv(code),
            clip,
        );
    }
}

fn push_caret(
    out: &mut Vec<Quad>,
    rect: Rect,
    layout: Option<&TextLayout>,
    caret: &Caret,
    clip: Clip,
) {
    let (x, y, h) = layout
        .map(|layout| {
            let (x, y) = layout.caret_xy(caret.index);
            let h = layout.lines.first().map(|line| line.h).unwrap_or(CELL_H);
            (x, y, h)
        })
        .unwrap_or((0.0, 0.0, CELL_H));
    push(
        out,
        Rect {
            x: rect.x + TEXT_ORIGIN_X + x,
            y: rect.y + TEXT_ORIGIN_Y + y,
            w: 2.0,
            h,
        },
        Color::rgb(0.94, 0.95, 0.97),
        solid_uv(),
        clip,
    );
}
