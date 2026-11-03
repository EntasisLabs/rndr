//! Quads in logical points, plus a display list for vector scenes. No window
//! and no GPU.
//!
//! The `highlight` feature forwards `rndr_core`'s fenced-code token colors.
//!
//! [`frame`] steps an [`rndr_core::App`] (Startup once, then Input, Update,
//! PostUpdate, and Layout) and returns one [`Quad`] per solid, glyph,
//! selection, caret, dirty mark, and fixed-pitch cell, plus [`SurfaceRect`]s
//! for foreign surfaces. Those quads are the fast path. A
//! [`CanvasRegistry`] painter on a [`rndr_core::SurfaceSlot`], and an
//! [`rndr_core::ImageView`] on a row, are recorded as [`Placed`] commands
//! beside the quads. A window crate multiplies rectangles by
//! [`rndr_core::Viewport::scale`] when it uploads them. [`paint`] is the same
//! walk without stepping the app, and it returns only the quads.
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

mod affine;
mod display;
mod font;
mod latin;
mod path;
mod snapshot;
mod tess;
mod zoom;

use std::cell::RefCell;

pub use affine::{bucket_scale, size_bucket, zoom_bucket, Affine, OUTLINE_PX};
pub use display::{
    Canvas, CanvasPainter, CanvasRegistry, ClipShape, Effect, FillStyle, GlyphOp, GlyphSprite,
    ImageOp, Op, PathOp, Placed, Scene, SceneLayer, StrokeStyle, TextSource,
};
pub use font::{atlas_r8, glyph_uv, solid_uv, ATLAS_H, ATLAS_W};
pub use latin::LatinFace;
pub use path::{Cap, FillRule, Join, Marker, Path, PathBuilder, Stroke};
pub use snapshot::quads_snapshot;
pub use tess::{tessellate_fill, tessellate_stroke, Mesh, MeshCache};
pub use zoom::{ZoomCamera, ZOOM_MAX_STEP, ZOOM_MIN_STEP};

use rndr_core::{
    children_of, layered_roots, style_for, style_of, text_engine, theme_of, App, Caret, Clip,
    Color, DirtyMark, EditField, Field, Focus, Label, MonoCache, MonoCursorShape, MonoGrid, Parent,
    Radius, Rect, Spans, TextLayout, TextSelection, Theme, Viewport, World, CELL_H, TEXT_ORIGIN_X,
    TEXT_ORIGIN_Y,
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
    /// Quads in [`paint`] order, parents before children. This is the fast path.
    pub quads: Vec<Quad>,
    /// [`rndr_core::SurfaceSlot`] rects after layout, in component-store
    /// order. [`quads_snapshot`] sorts these before it prints them.
    pub slots: Vec<SurfaceRect>,
    /// Canvas, image, and layer commands. Each one is drawn before
    /// [`Placed::z`] in [`Self::quads`]. Empty when the frame is quads only.
    pub draws: Vec<Placed>,
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
    frame_with_snapshot(app, |_| ()).0
}

/// Run the normal frame path, taking an immutable snapshot at the paint boundary
/// before application Render systems can mutate the world. This is not a second
/// reconstruction or update; the returned frame and snapshot share one world.
/// Consumers that depend on resources used during GPU submission must also
/// validate those resources after Render systems run.
pub fn frame_with_snapshot<T>(app: &mut App, snapshot: impl FnOnce(&World) -> T) -> (Frame, T) {
    frame_with_snapshot_frame(app, |world, _| snapshot(world))
}

/// The [`frame_with_snapshot`] boundary with access to the exact painted frame.
/// Headless consumers can submit and read back while paint-time resources are
/// still borrowed, before Render systems run. No second paint or update occurs.
pub fn frame_with_snapshot_frame<T>(
    app: &mut App,
    snapshot: impl FnOnce(&World, &Frame) -> T,
) -> (Frame, T) {
    app.update();
    let (quads, draws) = paint_parts(app.world());
    let frame = Frame {
        quads,
        draws,
        slots: surface_slots(app.world()),
    };
    let snapshot = snapshot(app.world(), &frame);
    app.render();
    (frame, snapshot)
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

/// Paint backgrounds and text in the world, parents before children.
///
/// Walks [`rndr_core::layered_roots`], so a higher layer is later in the
/// vec. A dead entity or a [`rndr_core::Hidden`] entity is skipped with
/// its children. A [`rndr_core::SurfaceSlot`] adds no quad; its children
/// still walk. An entity is filled only when it has both a
/// [`rndr_core::Rect`] and a [`rndr_core::Color`]. Text and its marks need
/// a rect but no background color. A label uses [`Label::color`] or
/// [`Theme::text`], independently of the background's alpha. Children of
/// an entity that has neither still walk. The clip is that entity's
/// [`rndr_core::Clip`], or the rect when the clip is absent.
///
/// The fill moves 25% of the way toward white when the entity is
/// focused, is a direct child of the focused entity, or has a
/// [`rndr_core::Caret`]. A [`rndr_core::TextSelection`] does not lighten
/// the fill. Alpha stays as authored. After the fill the order is the
/// dirty mark, a caret selection, highlight bars, a text selection, the
/// IME underline, link underlines, glyphs, decoration marks, and the
/// caret. Glyphs keep the layout UV and [`Quad::atlas`]. Other quads use
/// the text engine's solid UV and atlas `0`. Find marks use
/// [`rndr_core::Theme::selection`]. A diagnostic mark uses sRGB
/// `(0.86, 0.28, 0.24)`. The dirty mark is a 6-point circle in
/// [`rndr_core::Theme::dirty`]. The caret is 2 points wide and uses
/// [`rndr_core::Theme::caret`]. A text selection uses
/// [`rndr_core::Theme::selection`] and draws no caret. The IME underline
/// is at most 2 points tall, in that same caret color. A span underline
/// is 1 point tall, in the span color. A glyph that overlaps a caret
/// selection or a [`rndr_core::TextSelection`], and the part of an
/// underline inside that range, uses the label color, so a selected
/// link reads like the rest of the selected text.
///
/// Text, selection, the caret, and marks snap to the pixel grid of
/// [`rndr_core::Viewport::pixels_per_point`]. A missing viewport snaps
/// at one pixel per point. The entity fill keeps layout coordinates. A
/// quad that misses the clip, or whose width or height is not positive,
/// is left out. A [`rndr_core::Label`] with no [`rndr_core::TextLayout`]
/// is measured for this call only. A body field wraps; any other missing
/// layout stays one line. A [`rndr_core::Prompt`] shifts its text, selection,
/// and caret up by [`rndr_core::Prompt::scroll`] so the caret stays inside
/// the row, and clips those quads to the content band inside the vertical
/// padding. The fill keeps the full clip. The world stays unchanged. This
/// does not panic.
pub fn paint(world: &World) -> Vec<Quad> {
    paint_parts(world).0
}

fn paint_parts(world: &World) -> (Vec<Quad>, Vec<Placed>) {
    let quads = RefCell::new(Vec::new());
    let draws = RefCell::new(Vec::new());
    let scale = world
        .resource::<Viewport>()
        .map(|viewport| viewport.pixels_per_point())
        .unwrap_or(1.0);
    let style = style_of(world);
    let solid = text_engine(world).solid_uv(&style);
    let theme = theme_of(world);
    for root in layered_roots(world) {
        walk(world, root, &quads, &draws, scale, solid, &theme);
    }
    (quads.into_inner(), draws.into_inner())
}

#[allow(clippy::too_many_arguments)]
fn paint_visited(
    out: &mut Vec<Quad>,
    scale: f32,
    solid: [f32; 4],
    theme: &Theme,
    rect: Rect,
    color: Option<Color>,
    clip: Clip,
    label: Option<&Label>,
    caret: Option<&Caret>,
    selection: Option<&TextSelection>,
    spans: Option<&Spans>,
    dirty: bool,
    layout: Option<&TextLayout>,
    radius: f32,
    marks: &[rndr_core::DecorationMark],
    shift_y: f32,
    clip_text: bool,
    caret_row: Option<&rndr_core::CaretRow>,
    metrics: rndr_core::LabelBox,
) {
    let text_clip = if clip_text {
        clip.intersect(Clip {
            x: rect.x,
            y: rect.y + metrics.inset_y,
            w: rect.w,
            h: (rect.h - 2.0 * metrics.inset_y).max(0.0),
        })
    } else {
        clip
    };
    if let Some(color) = color {
        push(out, rect, color, solid, clip, radius, 0);
    }
    if dirty {
        push_dirty_mark(out, rect, clip, solid, theme.dirty);
    }
    // All text adornments share the same origin. Background/hit rectangles and
    // clips remain the real layout boxes; editing labels use legacy metrics.
    let rect = Rect {
        x: rect.x + metrics.inset_x - TEXT_ORIGIN_X,
        y: rect.y + metrics.inset_y - TEXT_ORIGIN_Y,
        ..rect
    };
    if let (Some(layout), Some(caret)) = (layout, caret) {
        push_selection(
            out,
            rect,
            layout,
            caret.anchor,
            caret.index,
            text_clip,
            solid,
            scale,
            theme.selection,
            shift_y,
        );
    }
    if let Some(layout) = layout {
        let text_color = label.and_then(|label| label.color).unwrap_or(theme.text);
        for highlight in &layout.highlights {
            push(
                out,
                snap_box(
                    rect.x + TEXT_ORIGIN_X + highlight.x,
                    rect.y + TEXT_ORIGIN_Y + highlight.y + shift_y,
                    highlight.w,
                    highlight.h,
                    scale,
                ),
                highlight.color,
                solid,
                text_clip,
                0.0,
                0,
            );
        }
        if caret.is_none() {
            if let Some(selection) = selection {
                push_selection(
                    out,
                    rect,
                    layout,
                    selection.anchor,
                    selection.head,
                    text_clip,
                    solid,
                    scale,
                    theme.selection,
                    shift_y,
                );
            }
        }
        if let Some((start, end)) = layout.ime_range {
            for span in layout.selection_boxes(start, end) {
                let bar = 2.0_f32.min(span.h);
                push(
                    out,
                    snap_box(
                        rect.x + TEXT_ORIGIN_X + span.x,
                        rect.y + TEXT_ORIGIN_Y + span.y + span.h - bar + shift_y,
                        span.w,
                        bar,
                        scale,
                    ),
                    theme.caret,
                    solid,
                    text_clip,
                    0.0,
                    0,
                );
            }
        }
        let selected = selection_range(caret, selection);
        if let Some(spans) = spans {
            for run in &spans.runs {
                if !run.underline || run.start >= run.end {
                    continue;
                }
                for (start, end, inside) in underline_parts(run.start, run.end, selected) {
                    let color = if inside { text_color } else { run.color };
                    for span in layout.selection_boxes(start, end) {
                        let bar = 1.0_f32.min(span.h);
                        push(
                            out,
                            snap_box(
                                rect.x + TEXT_ORIGIN_X + span.x,
                                rect.y + TEXT_ORIGIN_Y + span.y + span.h - bar + shift_y,
                                span.w,
                                bar,
                                scale,
                            ),
                            color,
                            solid,
                            text_clip,
                            0.0,
                            0,
                        );
                    }
                }
            }
        }
        push_label(
            out, rect, layout, text_clip, scale, text_color, selected, shift_y,
        );
    }
    for mark in marks {
        let color = match mark.kind {
            rndr_core::DecorationKind::Find => theme.selection,
            rndr_core::DecorationKind::Diagnostic => Color::rgb(0.86, 0.28, 0.24),
        };
        push(
            out,
            snap_box(
                rect.x + TEXT_ORIGIN_X + mark.x,
                rect.y + TEXT_ORIGIN_Y + mark.y + shift_y,
                mark.w,
                mark.h,
                scale,
            ),
            color,
            solid,
            text_clip,
            0.0,
            0,
        );
    }
    if let Some(caret) = caret {
        push_caret(
            out,
            rect,
            layout,
            caret,
            text_clip,
            solid,
            scale,
            theme.caret,
            shift_y,
            caret_row,
        );
    }
}

fn walk(
    world: &World,
    entity: rndr_core::Entity,
    quads: &RefCell<Vec<Quad>>,
    draws: &RefCell<Vec<Placed>>,
    scale: f32,
    solid: [f32; 4],
    theme: &Theme,
) {
    if !world.is_alive(entity) || world.has::<rndr_core::Hidden>(entity) {
        return;
    }
    if world.has::<rndr_core::SurfaceSlot>(entity) {
        record_canvas(world, entity, quads, draws);
        for child in children_of(world, entity) {
            walk(world, child, quads, draws, scale, solid, theme);
        }
        return;
    }
    record_image(world, entity, quads, draws);
    if world.has::<MonoGrid>(entity) {
        paint_mono(world, entity, &mut quads.borrow_mut(), scale, solid, theme);
        for child in children_of(world, entity) {
            walk(world, child, quads, draws, scale, solid, theme);
        }
        return;
    }
    if let Some(rect) = world.get::<Rect>(entity) {
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
        let color = world.get::<Color>(entity).copied().map(|color| {
            if highlight && !world.has::<rndr_core::ResolvedColor>(entity) {
                lighten(color)
            } else {
                color
            }
        });
        let dirty = world.has::<DirtyMark>(entity);
        let radius = world
            .get::<Radius>(entity)
            .map(|radius| radius.0)
            .unwrap_or(0.0);
        let needs_fallback =
            world.get::<Label>(entity).is_some() && world.get::<TextLayout>(entity).is_none();
        let mut guard = quads.borrow_mut();
        let mut paint = |rect: Rect,
                         color: Option<Color>,
                         clip: Clip,
                         label: Option<&Label>,
                         caret: Option<&Caret>,
                         selection: Option<&TextSelection>,
                         spans: Option<&Spans>,
                         dirty: bool,
                         layout: Option<&TextLayout>,
                         radius: f32,
                         marks: &[rndr_core::DecorationMark],
                         shift_y: f32,
                         clip_text: bool| {
            paint_visited(
                &mut guard,
                scale,
                solid,
                theme,
                rect,
                color,
                clip,
                label,
                caret,
                selection,
                spans,
                dirty,
                layout,
                radius,
                marks,
                shift_y,
                clip_text,
                world.get::<rndr_core::CaretRow>(entity),
                rndr_core::label_box(world, entity),
            );
        };
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
                &mut paint,
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
                &mut paint,
            );
        }
    }
    record_geometry(world, entity, quads.borrow().len() as u32, draws, false);
    for child in children_of(world, entity) {
        walk(world, child, quads, draws, scale, solid, theme);
    }
    record_geometry(world, entity, quads.borrow().len() as u32, draws, true);
    record_scroll_indicator(world, entity, quads, solid);
}

fn record_scroll_indicator(
    world: &World,
    entity: rndr_core::Entity,
    quads: &RefCell<Vec<Quad>>,
    solid: [f32; 4],
) {
    let (Some(style), Some(scroll), Some(rect), Some(clip)) = (
        world.get::<rndr_core::ScrollIndicator>(entity),
        world.get::<rndr_core::Scroll>(entity),
        world.get::<Rect>(entity),
        world.get::<Clip>(entity),
    ) else {
        return;
    };
    // Keep the affordance on the visible clipped edge, even when an outer
    // document clips this viewport's far edge. Input ownership is unchanged.
    let visible = clip.intersect(Clip::from_rect(*rect));
    for axis in 0..2 {
        let length = if axis == 0 { rect.w } else { rect.h };
        let visible_length = if axis == 0 { visible.w } else { visible.h };
        let content = scroll.content[axis];
        if !length.is_finite()
            || !content.is_finite()
            || length < 12.
            || visible_length < 12.
            || content <= length
        {
            continue;
        }
        let track = visible_length - 8.;
        let thumb = (track * length / content).max(18.).min(track);
        let position = (-scroll.offset[axis] / (content - length)).clamp(0., 1.) * (track - thumb);
        let thumb_rect = if axis == 0 {
            Rect {
                x: visible.x + 4. + position,
                y: visible.y + visible.h - 5.,
                w: thumb,
                h: 3.,
            }
        } else {
            Rect {
                x: visible.x + visible.w - 5.,
                y: visible.y + 4. + position,
                w: 3.,
                h: thumb,
            }
        };
        push(
            &mut quads.borrow_mut(),
            thumb_rect,
            style.color,
            solid,
            *clip,
            1.5,
            0,
        );
    }
}

// Ordinary paint commands: these use the same clip, z order, capture and GPU
// antialiasing as every other path. No font metrics or demo-specific offsets.
fn record_geometry(
    world: &World,
    entity: rndr_core::Entity,
    z: u32,
    draws: &RefCell<Vec<Placed>>,
    outline: bool,
) {
    if (outline && !world.has::<rndr_core::Border>(entity))
        || (!outline && !world.has::<rndr_core::CrossMark>(entity))
    {
        return;
    }
    let Some(rect) = world.get::<Rect>(entity).copied() else {
        return;
    };
    let clip = world
        .get::<Clip>(entity)
        .copied()
        .unwrap_or_else(|| Clip::from_rect(rect));
    if rect.w <= 0. || rect.h <= 0. || clip.w <= 0. || clip.h <= 0. {
        return;
    }
    let mut canvas = Canvas::new();
    if let Some(border) = world.get::<rndr_core::Border>(entity).filter(|_| outline) {
        if border.width.is_finite() && border.width > 0. {
            let width = border.width.min(rect.w.min(rect.h) * 0.5);
            let inset = width * 0.5;
            let radius = world
                .get::<Radius>(entity)
                .map(|r| r.0)
                .unwrap_or(0.)
                .max(0.)
                .min(rect.w.min(rect.h) * 0.5);
            canvas.stroke(
                Path::round_rect(
                    Rect {
                        x: rect.x + inset,
                        y: rect.y + inset,
                        w: rect.w - width,
                        h: rect.h - width,
                    },
                    (radius - inset).max(0.),
                ),
                border.color,
                Stroke {
                    width,
                    ..Stroke::default()
                },
            );
        }
    }
    if let Some(mark) = world
        .get::<rndr_core::CrossMark>(entity)
        .filter(|_| !outline)
    {
        let size = rect.w.min(rect.h);
        let half = size * 0.25;
        let center = [rect.x + rect.w * 0.5, rect.y + rect.h * 0.5];
        let stroke = Stroke {
            width: (size / 10.).max(1.),
            cap: Cap::Round,
            ..Stroke::default()
        };
        // Both strokes share one symmetric path; checkbox ink is opaque.
        let mut path = Path::builder();
        path.move_to([center[0] - half, center[1] - half])
            .line_to([center[0] + half, center[1] + half]);
        path.move_to([center[0] + half, center[1] - half])
            .line_to([center[0] - half, center[1] + half]);
        canvas.stroke(path.build(), mark.color, stroke);
    }
    for op in canvas.finish_clipped(Rect {
        x: clip.x,
        y: clip.y,
        w: clip.w,
        h: clip.h,
    }) {
        draws.borrow_mut().push(Placed { z, op });
    }
}

fn record_canvas(
    world: &World,
    entity: rndr_core::Entity,
    quads: &RefCell<Vec<Quad>>,
    draws: &RefCell<Vec<Placed>>,
) {
    let Some(slot) = world.get::<rndr_core::SurfaceSlot>(entity).copied() else {
        return;
    };
    let Some(rect) = world.get::<Rect>(entity).copied() else {
        return;
    };
    let Some(registry) = world.resource::<CanvasRegistry>() else {
        return;
    };
    let Some(painter) = registry.get(slot.id) else {
        return;
    };
    let mut canvas = Canvas::new();
    painter.paint(world, &mut canvas);
    let z = quads.borrow().len() as u32;
    for op in canvas.finish_clipped(rect) {
        draws.borrow_mut().push(Placed { z, op });
    }
}

fn record_image(
    world: &World,
    entity: rndr_core::Entity,
    quads: &RefCell<Vec<Quad>>,
    draws: &RefCell<Vec<Placed>>,
) {
    let Some(view) = world.get::<rndr_core::ImageView>(entity).copied() else {
        return;
    };
    let Some(rect) = world.get::<Rect>(entity).copied() else {
        return;
    };
    if rect.w <= 0.0 || rect.h <= 0.0 {
        return;
    }
    let z = quads.borrow().len() as u32;
    let clip = world.get::<Clip>(entity).copied().map(|clip| Rect {
        x: clip.x,
        y: clip.y,
        w: clip.w,
        h: clip.h,
    });
    draws.borrow_mut().push(Placed {
        z,
        op: Op::Image(ImageOp {
            id: view.image,
            dest: rect,
            uv: view.uv,
            radius: view.radius,
            opacity: 1.0,
            clip,
        }),
    });
}

fn paint_mono(
    world: &World,
    entity: rndr_core::Entity,
    out: &mut Vec<Quad>,
    scale: f32,
    solid: [f32; 4],
    theme: &Theme,
) {
    let Some(grid) = world.get::<MonoGrid>(entity) else {
        return;
    };
    let Some(rect) = world.get::<Rect>(entity).copied() else {
        return;
    };
    let clip = world
        .get::<Clip>(entity)
        .copied()
        .unwrap_or_else(|| Clip::from_rect(rect));
    if let Some(color) = world.get::<Color>(entity).copied() {
        push(out, rect, color, solid, clip, 0.0, 0);
    }
    let Some(cache) = world.get::<MonoCache>(entity) else {
        paint_cursor(out, rect, grid, clip, scale, solid);
        return;
    };
    for row in &cache.rows {
        for run in &row.backgrounds {
            push(
                out,
                place_run(rect, *run, scale),
                run.color,
                solid,
                clip,
                0.0,
                0,
            );
        }
    }
    for band in &grid.selection {
        let width = grid.cell_w * (band.end.saturating_sub(band.start)) as f32;
        if width <= 0.0 {
            continue;
        }
        push(
            out,
            snap_box(
                rect.x + band.start as f32 * grid.cell_w,
                rect.y + band.row as f32 * grid.cell_h,
                width,
                grid.cell_h,
                scale,
            ),
            theme.selection,
            solid,
            clip,
            0.0,
            0,
        );
    }
    for row in &cache.rows {
        for glyph in &row.glyphs {
            push(
                out,
                snap_box(rect.x + glyph.x, rect.y + glyph.y, glyph.w, glyph.h, scale),
                glyph.color.unwrap_or(theme.text),
                glyph.uv,
                clip,
                0.0,
                glyph.atlas,
            );
        }
        for run in &row.strokes {
            push(
                out,
                place_run(rect, *run, scale),
                run.color,
                solid,
                clip,
                0.0,
                0,
            );
        }
    }
    paint_cursor(out, rect, grid, clip, scale, solid);
    if let Some(cursor) = grid.cursor.filter(|cursor| cursor.visible) {
        if cursor.shape == MonoCursorShape::Block {
            if let Some(row) = cache.rows.get(cursor.row as usize) {
                let x0 = cursor.col as f32 * grid.cell_w;
                let x1 = x0 + grid.cell_w;
                let y0 = cursor.row as f32 * grid.cell_h;
                let y1 = y0 + grid.cell_h;
                for glyph in &row.glyphs {
                    if glyph.x >= x0 && glyph.x < x1 && glyph.y >= y0 && glyph.y < y1 {
                        push(
                            out,
                            snap_box(rect.x + glyph.x, rect.y + glyph.y, glyph.w, glyph.h, scale),
                            cursor.ink,
                            glyph.uv,
                            clip,
                            0.0,
                            glyph.atlas,
                        );
                    }
                }
            }
        }
    }
}

fn place_run(rect: Rect, run: rndr_core::MonoRun, scale: f32) -> Rect {
    snap_box(rect.x + run.x, rect.y + run.y, run.w, run.h, scale)
}

fn paint_cursor(
    out: &mut Vec<Quad>,
    rect: Rect,
    grid: &MonoGrid,
    clip: Clip,
    scale: f32,
    solid: [f32; 4],
) {
    let Some(cursor) = grid.cursor.filter(|cursor| cursor.visible) else {
        return;
    };
    if cursor.col >= grid.cols || cursor.row >= grid.rows {
        return;
    }
    let x = rect.x + cursor.col as f32 * grid.cell_w;
    let y = rect.y + cursor.row as f32 * grid.cell_h;
    let (dx, dy, w, h) = match cursor.shape {
        MonoCursorShape::Block => (0.0, 0.0, grid.cell_w, grid.cell_h),
        MonoCursorShape::Beam => (0.0, 0.0, 2.0_f32.min(grid.cell_w), grid.cell_h),
        MonoCursorShape::Underline => {
            let h = 2.0_f32.min(grid.cell_h);
            (0.0, grid.cell_h - h, grid.cell_w, h)
        }
    };
    push(
        out,
        snap_box(x + dx, y + dy, w, h, scale),
        cursor.color,
        solid,
        clip,
        0.0,
        0,
    );
}

// The walk hands the visitor rect, clip, text, and marks together.
#[allow(clippy::too_many_arguments)]
fn emit(
    world: &World,
    entity: rndr_core::Entity,
    rect: Rect,
    color: Option<Color>,
    clip: Clip,
    dirty: bool,
    layout: Option<&TextLayout>,
    radius: f32,
    visit: &mut impl FnMut(
        Rect,
        Option<Color>,
        Clip,
        Option<&Label>,
        Option<&Caret>,
        Option<&TextSelection>,
        Option<&Spans>,
        bool,
        Option<&TextLayout>,
        f32,
        &[rndr_core::DecorationMark],
        f32,
        bool,
    ),
) {
    let empty: &[rndr_core::DecorationMark] = &[];
    let marks = world
        .get::<rndr_core::DecorationMarks>(entity)
        .map(|marks| marks.marks.as_slice())
        .unwrap_or(empty);
    let prompt = world.get::<rndr_core::Prompt>(entity);
    let shift_y = -prompt.map(|prompt| prompt.scroll).unwrap_or(0.0);
    let clip_text = prompt.is_some() || world.has::<rndr_core::Editor>(entity);
    let spans = world.get::<Spans>(entity).map(|_| Spans {
        runs: rndr_core::spans_for_text(
            world,
            entity,
            world
                .get::<Label>(entity)
                .map_or("", |label| label.text.as_str()),
        ),
    });
    visit(
        rect,
        color,
        clip,
        world.get::<Label>(entity),
        world.get::<Caret>(entity),
        world.get::<TextSelection>(entity),
        spans.as_ref(),
        dirty,
        layout,
        radius,
        marks,
        shift_y,
        clip_text,
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
    if world.has::<rndr_core::Editor>(entity) {
        return rndr_core::layout_of(world, entity, text);
    }
    let style = style_for(world, entity);
    let engine = text_engine(world);
    let wrap = world
        .get::<Field>(entity)
        .is_some_and(|field| field.kind == EditField::Body)
        || world
            .get::<Label>(entity)
            .is_some_and(|label| label.overflow != rndr_core::TextOverflow::Visible);
    let max_width = if wrap {
        rndr_core::label_box(world, entity).text_width(rect.w, engine.unit_advance(&style))
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
    anchor: usize,
    index: usize,
    clip: Clip,
    solid: [f32; 4],
    scale: f32,
    color: Color,
    shift_y: f32,
) {
    for span in layout.selection_boxes(anchor, index) {
        push(
            out,
            snap_box(
                rect.x + TEXT_ORIGIN_X + span.x,
                rect.y + TEXT_ORIGIN_Y + span.y + shift_y,
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

fn selection_range(
    caret: Option<&Caret>,
    selection: Option<&TextSelection>,
) -> Option<(usize, usize)> {
    let (anchor, head) = if let Some(caret) = caret {
        (caret.anchor, caret.index)
    } else {
        let selection = selection?;
        (selection.anchor, selection.head)
    };
    if anchor == head {
        None
    } else {
        Some((anchor.min(head), anchor.max(head)))
    }
}

fn underline_parts(
    start: usize,
    end: usize,
    selected: Option<(usize, usize)>,
) -> Vec<(usize, usize, bool)> {
    let Some((sel_start, sel_end)) = selected else {
        return vec![(start, end, false)];
    };
    let overlap_start = start.max(sel_start);
    let overlap_end = end.min(sel_end);
    if overlap_start >= overlap_end {
        return vec![(start, end, false)];
    }
    let mut parts = Vec::new();
    if start < overlap_start {
        parts.push((start, overlap_start, false));
    }
    parts.push((overlap_start, overlap_end, true));
    if overlap_end < end {
        parts.push((overlap_end, end, false));
    }
    parts
}

fn glyph_in_selection(glyph: &rndr_core::PlacedGlyph, selected: Option<(usize, usize)>) -> bool {
    selected.is_some_and(|(start, end)| glyph.start < end && glyph.end > start)
}

#[allow(clippy::too_many_arguments)]
fn push_label(
    out: &mut Vec<Quad>,
    rect: Rect,
    layout: &TextLayout,
    clip: Clip,
    scale: f32,
    color: Color,
    selected: Option<(usize, usize)>,
    shift_y: f32,
) {
    for glyph in &layout.glyphs {
        let fill = if glyph_in_selection(glyph, selected) {
            color
        } else {
            glyph.color.unwrap_or(color)
        };
        push(
            out,
            snap_box(
                rect.x + TEXT_ORIGIN_X + glyph.x,
                rect.y + TEXT_ORIGIN_Y + glyph.y + shift_y,
                glyph.w,
                glyph.h,
                scale,
            ),
            fill,
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
    shift_y: f32,
    caret_row: Option<&rndr_core::CaretRow>,
) {
    let (x, y, h) = layout
        .map(|layout| {
            let index = layout.ime_cursor.unwrap_or(caret.index);
            let row = caret_row.filter(|hint| {
                layout.ime_cursor.is_none() && hint.index == index && hint.row < layout.lines.len()
            });
            let (x, y) = row.map_or_else(
                || layout.caret_xy(index),
                |hint| layout.caret_xy_on_line(index, hint.row),
            );
            let h = layout
                .lines
                .iter()
                .find(|line| line.y == y)
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
            rect.y + TEXT_ORIGIN_Y + y + shift_y,
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
