//! Quads in logical points. No window and no GPU.
//!
//! [`paint`] walks a [`rndr_core::World`] and returns one [`Quad`] per solid,
//! glyph, selection, caret, and dirty mark. A surface plugin multiplies those
//! rects by [`rndr_core::Viewport::scale`] when it uploads them.

mod font;
mod latin;
mod snapshot;

pub use font::{atlas_r8, glyph_uv, solid_uv, ATLAS_H, ATLAS_W};
pub use latin::LatinFace;
pub use snapshot::quads_snapshot;

use rndr_core::{
    children_of, field_text_width, layered_roots, style_of, text_engine, theme_of, App, Caret,
    Clip, Color, DirtyMark, EditField, Field, Focus, Label, Parent, Radius, Rect, TextLayout,
    Viewport, World, CELL_H, TEXT_ORIGIN_X, TEXT_ORIGIN_Y,
};

/// One instanced quad in logical points.
#[derive(Clone, Copy, Debug)]
pub struct Quad {
    pub rect: Rect,
    pub color: Color,
    pub uv: [f32; 4],
    pub clip: Clip,
    /// Corner radius in logical points. Zero keeps a square.
    pub radius: f32,
}

/// One world frame, then the quads in points.
///
/// Runs Startup once, then Update, Layout, and the render schedule. The
/// surface multiplies the returned quads by [`rndr_core::Viewport::scale`]
/// when it presents. This does not open a window.
/// Quads plus foreign-surface rects for one frame.
#[derive(Clone, Debug)]
pub struct Frame {
    pub quads: Vec<Quad>,
    pub slots: Vec<SurfaceRect>,
}

/// A [`rndr_core::SurfaceSlot`] after layout, in logical points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SurfaceRect {
    pub id: u64,
    pub rect: Rect,
}

pub fn frame(app: &mut App) -> Frame {
    app.update();
    let quads = paint(app.world());
    let slots = surface_slots(app.world());
    app.render();
    Frame { quads, slots }
}

fn surface_slots(world: &World) -> Vec<SurfaceRect> {
    let mut slots = Vec::new();
    for entity in world.entities_with::<rndr_core::SurfaceSlot>() {
        let Some(slot) = world.get::<rndr_core::SurfaceSlot>(entity).copied() else {
            continue;
        };
        let Some(rect) = world.get::<Rect>(entity).copied() else {
            continue;
        };
        slots.push(SurfaceRect { id: slot.id, rect });
    }
    slots
}

/// Paint every colored rect in the world, parents before children.
pub fn paint(world: &World) -> Vec<Quad> {
    let mut out = Vec::new();
    let scale = world
        .resource::<Viewport>()
        .map(|viewport| viewport.pixels_per_point())
        .unwrap_or(1.0);
    let style = style_of(world);
    let solid = text_engine(world).solid_uv(&style);
    let theme = theme_of(world);
    for root in layered_roots(world) {
        walk(world, root, &mut |rect,
                                color,
                                clip,
                                _label,
                                caret,
                                dirty,
                                layout,
                                radius,
                                marks| {
            push(&mut out, rect, color, solid, clip, radius);
            if dirty {
                push_dirty_mark(&mut out, rect, clip, solid, theme.dirty);
            }
            if let (Some(layout), Some(caret)) = (layout, caret) {
                push_selection(
                    &mut out,
                    rect,
                    layout,
                    caret,
                    clip,
                    solid,
                    scale,
                    theme.selection,
                );
            }
            if let Some(layout) = layout {
                let text_color = _label.and_then(|label| label.color).unwrap_or(theme.text);
                for highlight in &layout.highlights {
                    push(
                        &mut out,
                        snap_box(
                            rect.x + TEXT_ORIGIN_X + highlight.x,
                            rect.y + TEXT_ORIGIN_Y + highlight.y,
                            highlight.w,
                            highlight.h,
                            scale,
                        ),
                        highlight.color,
                        solid,
                        clip,
                        0.0,
                    );
                }
                push_label(&mut out, rect, layout, clip, scale, text_color);
            }
            for mark in marks {
                let color = match mark.kind {
                    rndr_core::DecorationKind::Find => theme.selection,
                    rndr_core::DecorationKind::Diagnostic => Color::rgb(0.86, 0.28, 0.24),
                };
                push(
                    &mut out,
                    snap_box(
                        rect.x + TEXT_ORIGIN_X + mark.x,
                        rect.y + TEXT_ORIGIN_Y + mark.y,
                        mark.w,
                        mark.h,
                        scale,
                    ),
                    color,
                    solid,
                    clip,
                    0.0,
                );
            }
            if let Some(caret) = caret {
                push_caret(
                    &mut out,
                    rect,
                    layout,
                    caret,
                    clip,
                    solid,
                    scale,
                    theme.caret,
                );
            }
        });
    }
    out
}

fn walk(
    world: &World,
    entity: rndr_core::Entity,
    visit: &mut impl FnMut(
        Rect,
        Color,
        Clip,
        Option<&Label>,
        Option<&Caret>,
        bool,
        Option<&TextLayout>,
        f32,
        &[rndr_core::DecorationMark],
    ),
) {
    if !world.is_alive(entity) || world.has::<rndr_core::Hidden>(entity) {
        return;
    }
    if world.has::<rndr_core::SurfaceSlot>(entity) {
        for child in children_of(world, entity) {
            walk(world, child, visit);
        }
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
        let radius = world
            .get::<Radius>(entity)
            .map(|radius| radius.0)
            .unwrap_or(0.0);
        let needs_fallback =
            world.get::<Label>(entity).is_some() && world.get::<TextLayout>(entity).is_none();
        if needs_fallback {
            let text = world
                .get::<Label>(entity)
                .map(|label| label.text.clone())
                .unwrap_or_default();
            let fallback = measured(world, entity, &text, *rect);
            emit(
                world,
                entity,
                *rect,
                color,
                clip,
                dirty,
                Some(&fallback),
                radius,
                visit,
            );
        } else {
            emit(
                world,
                entity,
                *rect,
                color,
                clip,
                dirty,
                world.get::<TextLayout>(entity),
                radius,
                visit,
            );
        }
    }
    for child in children_of(world, entity) {
        walk(world, child, visit);
    }
}

// The walk hands the visitor rect, clip, text, and marks together.
#[allow(clippy::too_many_arguments)]
fn emit(
    world: &World,
    entity: rndr_core::Entity,
    rect: Rect,
    color: Color,
    clip: Clip,
    dirty: bool,
    layout: Option<&TextLayout>,
    radius: f32,
    visit: &mut impl FnMut(
        Rect,
        Color,
        Clip,
        Option<&Label>,
        Option<&Caret>,
        bool,
        Option<&TextLayout>,
        f32,
        &[rndr_core::DecorationMark],
    ),
) {
    let empty: &[rndr_core::DecorationMark] = &[];
    let marks = world
        .get::<rndr_core::DecorationMarks>(entity)
        .map(|marks| marks.marks.as_slice())
        .unwrap_or(empty);
    visit(
        rect,
        color,
        clip,
        world.get::<Label>(entity),
        world.get::<Caret>(entity),
        dirty,
        layout,
        radius,
        marks,
    );
}

fn lighten(color: Color) -> Color {
    Color::rgba(
        color.r + (1.0 - color.r) * 0.25,
        color.g + (1.0 - color.g) * 0.25,
        color.b + (1.0 - color.b) * 0.25,
        color.a,
    )
}

fn push(out: &mut Vec<Quad>, rect: Rect, color: Color, uv: [f32; 4], clip: Clip, radius: f32) {
    if !rect_hits_clip(rect, clip) {
        return;
    }
    out.push(Quad {
        rect,
        color,
        uv,
        clip,
        radius,
    });
}

fn rect_hits_clip(rect: Rect, clip: Clip) -> bool {
    rect.w > 0.0
        && rect.h > 0.0
        && clip.w > 0.0
        && clip.h > 0.0
        && rect.x < clip.x + clip.w
        && rect.y < clip.y + clip.h
        && rect.x + rect.w > clip.x
        && rect.y + rect.h > clip.y
}

fn snap_edge(points: f32, scale: f32) -> f32 {
    let scale = if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    };
    (points * scale).round() / scale
}

fn snap_box(x: f32, y: f32, w: f32, h: f32, scale: f32) -> Rect {
    let x1 = snap_edge(x, scale);
    let y1 = snap_edge(y, scale);
    let x2 = snap_edge(x + w, scale);
    let y2 = snap_edge(y + h, scale);
    Rect {
        x: x1,
        y: y1,
        w: (x2 - x1).max(0.0),
        h: (y2 - y1).max(0.0),
    }
}

fn push_dirty_mark(out: &mut Vec<Quad>, rect: Rect, clip: Clip, solid: [f32; 4], color: Color) {
    let size = 6.0;
    push(
        out,
        Rect {
            x: rect.x + 3.0,
            y: rect.y + TEXT_ORIGIN_Y + (CELL_H - size) * 0.5,
            w: size,
            h: size,
        },
        color,
        solid,
        clip,
        size * 0.5,
    );
}

fn measured(world: &World, entity: rndr_core::Entity, text: &str, rect: Rect) -> TextLayout {
    let style = style_of(world);
    let engine = text_engine(world);
    let wrap = world
        .get::<Field>(entity)
        .is_some_and(|field| field.kind == EditField::Body);
    let max_width = if wrap {
        field_text_width(rect.w, engine.unit_advance(&style))
    } else {
        f32::INFINITY
    };
    let spans = world
        .get::<rndr_core::Spans>(entity)
        .map(|spans| spans.runs.clone())
        .unwrap_or_default();
    engine.layout(text, &style, &spans, max_width)
}

// Selection quads need the label rect, layout, caret, clip, and scale together.
#[allow(clippy::too_many_arguments)]
fn push_selection(
    out: &mut Vec<Quad>,
    rect: Rect,
    layout: &TextLayout,
    caret: &Caret,
    clip: Clip,
    solid: [f32; 4],
    scale: f32,
    color: Color,
) {
    for span in layout.selection_boxes(caret.anchor, caret.index) {
        push(
            out,
            snap_box(
                rect.x + TEXT_ORIGIN_X + span.x,
                rect.y + TEXT_ORIGIN_Y + span.y,
                span.w,
                span.h,
                scale,
            ),
            color,
            solid,
            clip,
            0.0,
        );
    }
}

fn push_label(
    out: &mut Vec<Quad>,
    rect: Rect,
    layout: &TextLayout,
    clip: Clip,
    scale: f32,
    color: Color,
) {
    for glyph in &layout.glyphs {
        push(
            out,
            snap_box(
                rect.x + TEXT_ORIGIN_X + glyph.x,
                rect.y + TEXT_ORIGIN_Y + glyph.y,
                glyph.w,
                glyph.h,
                scale,
            ),
            glyph.color.unwrap_or(color),
            glyph.uv,
            clip,
            0.0,
        );
    }
}

// The caret quad needs the label rect, layout, caret, clip, and scale together.
#[allow(clippy::too_many_arguments)]
fn push_caret(
    out: &mut Vec<Quad>,
    rect: Rect,
    layout: Option<&TextLayout>,
    caret: &Caret,
    clip: Clip,
    solid: [f32; 4],
    scale: f32,
    color: Color,
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
        snap_box(
            rect.x + TEXT_ORIGIN_X + x,
            rect.y + TEXT_ORIGIN_Y + y,
            2.0,
            h,
            scale,
        ),
        color,
        solid,
        clip,
        0.0,
    );
}
