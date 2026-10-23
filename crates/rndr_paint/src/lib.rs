//! Quads in logical points. No window and no GPU.
//!
//! [`frame`] steps an [`rndr_core::App`] (Startup once, then Input, Update,
//! PostUpdate, and Layout) and returns one [`Quad`] per solid, glyph,
//! selection, caret, and dirty mark, plus [`SurfaceRect`]s for foreign
//! surfaces. A window crate multiplies those rectangles by
//! [`rndr_core::Viewport::scale`] when it uploads them. [`paint`] is the same
//! walk without stepping the app.
//!
//! # Example
//!
//! ```
//! use rndr_core::{App, Axis, Color, Label, Layout, Size, Viewport};
//! use rndr_paint::frame;
//!
//! let mut app = App::new();
//! app.world_mut()
//!     .insert_resource(Viewport::new(200.0, 100.0));
//! app.world_mut().spawn((
//!     Layout {
//!         axis: Axis::Column,
//!         gap: 0.0,
//!         padding: 0.0,
//!         width: Size::Px(80.0),
//!         height: Size::Px(32.0),
//!         min_width: 0.0,
//!         min_height: 0.0,
//!     },
//!     Color::rgb(0.2, 0.3, 0.4),
//!     Label::new("hi"),
//! ));
//! let painted = frame(&mut app);
//! assert!(!painted.quads.is_empty());
//! ```

#![deny(missing_docs)]

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
///
/// [`paint`] builds one for a fill, a glyph, a selection, a caret, or a
/// mark. A surface multiplies the rectangle by [`rndr_core::Viewport::scale`]
/// when it uploads.
#[derive(Clone, Copy, Debug)]
pub struct Quad {
    /// Axis-aligned rectangle in logical points. The origin is the top-left.
    pub rect: Rect,
    /// sRGB fill with straight alpha. See [`rndr_core::Color`].
    pub color: Color,
    /// Atlas rectangle `(u0, v0, u1, v1)` in the 0–1 range of [`Self::atlas`].
    ///
    /// A solid fill names a white coverage texel. A glyph names its cell.
    pub uv: [f32; 4],
    /// Scissor in logical points, origin top-left. Fragments outside it are
    /// discarded.
    pub clip: Clip,
    /// Corner radius in logical points. Zero keeps a square.
    pub radius: f32,
    /// `0` samples the coverage atlas. `1` samples the color atlas.
    /// Any non-zero value selects the color atlas.
    /// Snapshot text ignores this so a coverage glyph stays on the old dump.
    pub atlas: u8,
}

/// One world frame, then the quads in points.
///
/// Quads plus foreign-surface rects for one frame. The surface multiplies
/// the returned quads by [`rndr_core::Viewport::scale`] when it presents.
#[derive(Clone, Debug)]
pub struct Frame {
    /// Quads in [`paint`] order, parents before children.
    pub quads: Vec<Quad>,
    /// [`rndr_core::SurfaceSlot`] rects after layout, in component-store
    /// order. [`quads_snapshot`] sorts these before it prints them.
    pub slots: Vec<SurfaceRect>,
}

/// A [`rndr_core::SurfaceSlot`] after layout, in logical points.
///
/// [`paint`] skips the slot entity. The host places the foreign surface
/// on [`Self::rect`]. A slot entity with no [`rndr_core::Rect`] is absent
/// from [`frame`]'s list.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SurfaceRect {
    /// [`rndr_core::SurfaceSlot::id`] copied from the entity.
    pub id: u64,
    /// Layout rect in logical points, origin top-left.
    pub rect: Rect,
}

/// Run one frame and return its quads and foreign-surface rects.
///
/// Runs Startup once, then Update, Layout, and paint. Input runs
/// before Update, PostUpdate runs before Layout, and the render
/// schedule runs after paint. This does not open a window.
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
///
/// Walks [`rndr_core::layered_roots`], so a higher layer is later in the
/// vec. A dead entity or a [`rndr_core::Hidden`] entity is skipped with
/// its children. A [`rndr_core::SurfaceSlot`] adds no quad; its children
/// still walk. An entity is filled only when it has both a
/// [`rndr_core::Rect`] and a [`rndr_core::Color`]. Children of an entity
/// that has neither still walk. The clip is that entity's
/// [`rndr_core::Clip`], or the rect when the clip is absent.
///
/// The fill moves 25% of the way toward white when the entity is
/// focused, is a direct child of the focused entity, or has a
/// [`rndr_core::Caret`]. Alpha stays as authored. After the fill the
/// order is the dirty mark, the selection, highlight bars, the IME
/// underline, glyphs, decoration marks, and the caret. Glyphs keep the
/// layout UV and [`Quad::atlas`]. Other quads use the text engine's
/// solid UV and atlas `0`. Find marks use [`rndr_core::Theme::selection`].
/// A diagnostic mark uses sRGB `(0.86, 0.28, 0.24)`. The dirty mark is
/// a 6-point circle in [`rndr_core::Theme::dirty`]. The caret is 2
/// points wide and uses [`rndr_core::Theme::caret`]. The IME underline
/// is at most 2 points tall, in that same caret color.
///
/// Text, selection, the caret, and marks snap to the pixel grid of
/// [`rndr_core::Viewport::pixels_per_point`]. A missing viewport snaps
/// at one pixel per point. The entity fill keeps layout coordinates. A
/// quad that misses the clip, or whose width or height is not positive,
/// is left out. A [`rndr_core::Label`] with no [`rndr_core::TextLayout`]
/// is measured for this call only. A body field wraps; any other missing
/// layout stays one line. The world stays unchanged. This does not panic.
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
            push(&mut out, rect, color, solid, clip, radius, 0);
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
                        0,
                    );
                }
                if let Some((start, end)) = layout.ime_range {
                    for span in layout.selection_boxes(start, end) {
                        let bar = 2.0_f32.min(span.h);
                        push(
                            &mut out,
                            snap_box(
                                rect.x + TEXT_ORIGIN_X + span.x,
                                rect.y + TEXT_ORIGIN_Y + span.y + span.h - bar,
                                span.w,
                                bar,
                                scale,
                            ),
                            theme.caret,
                            solid,
                            clip,
                            0.0,
                            0,
                        );
                    }
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
                    0,
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

fn push(
    out: &mut Vec<Quad>,
    rect: Rect,
    color: Color,
    uv: [f32; 4],
    clip: Clip,
    radius: f32,
    atlas: u8,
) {
    if !rect_hits_clip(rect, clip) {
        return;
    }
    out.push(Quad {
        rect,
        color,
        uv,
        clip,
        radius,
        atlas,
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
        0,
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
            0,
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
            glyph.atlas,
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
            let index = layout.ime_cursor.unwrap_or(caret.index);
            let (x, y) = layout.caret_xy(index);
            let h = layout
                .lines
                .iter()
                .find(|line| index >= line.start && index <= line.end)
                .map(|line| line.h)
                .or_else(|| layout.lines.first().map(|line| line.h))
                .unwrap_or(CELL_H);
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
        0,
    );
}
