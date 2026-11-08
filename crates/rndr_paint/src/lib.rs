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
//! beside the quads. An entity with [`rndr_core::Opacity`] below one or a
//! non-identity [`rndr_core::Transform`] moves its subtree out of the quad list
//! into one [`SceneLayer`] of [`Op::Quads`] and draws, composited once. A
//! window crate multiplies rectangles by [`rndr_core::Viewport::scale`] when it
//! uploads them. [`paint`] is the same walk without stepping the app, and it
//! returns only the quads, so it omits composited subtrees.
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
    Canvas, CanvasPainter, CanvasRegistry, ClipShape, Effect, EffectSlotOp, FillStyle, GlyphOp,
    GlyphSprite, ImageOp, Op, PathOp, Placed, Scene, SceneLayer, StrokeStyle, SurfaceOp,
    TextSource,
};
pub use font::{atlas_r8, glyph_uv, solid_uv, ATLAS_H, ATLAS_W};
pub use latin::LatinFace;
pub use path::{Cap, FillRule, Join, Marker, Path, PathBuilder, Stroke};
pub use snapshot::quads_snapshot;

pub use tess::{tessellate_fill, tessellate_stroke, Mesh, MeshCache};
pub use zoom::{ZoomCamera, ZOOM_MAX_STEP, ZOOM_MIN_STEP};

use rndr_core::{
    layered_roots, style_for, style_of, text_engine, theme_of, App, Caret, Clip, Color, DirtyMark,
    EditField, Field, Focus, Label, MonoCache, MonoCursorShape, MonoGrid, Parent, Radius, Rect,
    Spans, TextLayout, TextSelection, Theme, Viewport, World, CELL_H, TEXT_ORIGIN_X, TEXT_ORIGIN_Y,
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
    /// Per-corner radii and the non-solid kinds: shadows, gradient bands,
    /// and border rings. The default is a plain fill or glyph.
    pub shape: QuadShape,
}

/// Geometry a [`Quad`] carries beyond a uniform [`Quad::radius`].
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct QuadShape {
    /// Radii in logical points: top-left, top-right, bottom-right,
    /// bottom-left. `None` uses [`Quad::radius`] on every corner.
    pub corners: Option<[f32; 4]>,
    /// What the quad paints inside its rounded rectangle.
    pub kind: QuadKind,
    /// Rounded, optionally feathered mask from the nearest
    /// [`rndr_core::ClipRounded`] ancestor (or the entity itself).
    pub round_clip: Option<RoundClip>,
}

/// Rounded mask in logical points, applied on top of [`Quad::clip`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RoundClip {
    /// Mask rectangle.
    pub rect: Rect,
    /// Top-left, top-right, bottom-right, bottom-left radii.
    pub corners: [f32; 4],
    /// Soft edge width inward from the rectangle; `0.0` is a hard edge.
    pub feather: f32,
}

/// What a [`Quad`] paints. Positions are logical points.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum QuadKind {
    /// Solid [`Quad::color`] or an atlas glyph.
    #[default]
    Fill,
    /// Gaussian shadow of the rounded box `shape` (with [`QuadShape::corners`]
    /// or [`Quad::radius`]). An outer shadow's quad covers the blurred
    /// extent; an inset shadow's quad is the box it darkens.
    Shadow {
        /// Box casting the shadow, already offset and spread.
        shape: Rect,
        /// The element's own box. An outer shadow is hidden inside it; an
        /// inset shadow paints only inside it.
        element: Rect,
        /// Gaussian standard deviation in points.
        sigma: f32,
        /// Darken inside the quad's box instead of outside `shape`.
        inset: bool,
    },
    /// Linear gradient band: [`Quad::color`] at `range[0]` to `to` at
    /// `range[1]`, where position runs 0 at `start` to 1 at `end`. Outside the
    /// range the band paints nothing, except below the first band and above
    /// the last, which `extend` clamps.
    Linear {
        /// Position 0.
        start: [f32; 2],
        /// Position 1.
        end: [f32; 2],
        /// Positions this band covers.
        range: [f32; 2],
        /// Color at `range[1]`.
        to: Color,
        /// Paint below `range[0]` (first band) and above `range[1]` (last band).
        extend: [bool; 2],
    },
    /// Radial gradient band; position is the distance from `center` divided
    /// by `radius`.
    Radial {
        /// Center.
        center: [f32; 2],
        /// Distance at position 1.
        radius: f32,
        /// Positions this band covers.
        range: [f32; 2],
        /// Color at `range[1]`.
        to: Color,
        /// Paint below `range[0]` (first band) and above `range[1]` (last band).
        extend: [bool; 2],
    },
    /// Border ring inside the rounded rectangle, top/right/bottom/left
    /// widths, with an optional dash and gap length (zero is solid).
    Ring {
        /// Top, right, bottom, left.
        widths: [f32; 4],
        /// Dash then gap; `[0, 0]` is solid.
        dash: [f32; 2],
    },
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

/// Live-only frame: [`App::update_live`] instead of [`App::update`], then
/// paint. Use it when only [`rndr_core::Live`] values changed.
pub fn frame_live(app: &mut App) -> Frame {
    frame_live_with_snapshot(app, |_| ()).0
}

/// [`frame_live`] with the [`frame_with_snapshot`] paint-boundary snapshot.
pub fn frame_live_with_snapshot<T>(
    app: &mut App,
    snapshot: impl FnOnce(&World) -> T,
) -> (Frame, T) {
    app.update_live();
    let (quads, draws) = paint_parts(app.world());
    let frame = Frame {
        quads,
        draws,
        slots: surface_slots(app.world()),
    };
    let snapshot = snapshot(app.world());
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
/// or strikethrough is 1 point tall, in the span color. A glyph that overlaps a caret
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
    paint_roots(world, &layered_roots(world))
}

/// Paint `root` and its descendants only.
///
/// Ancestors and siblings are not visited, so they cannot cover the subtree.
/// Coordinates stay in global logical points. A hidden root paints nothing.
pub fn paint_subtree(world: &World, root: rndr_core::Entity) -> (Vec<Quad>, Vec<Placed>) {
    paint_roots(world, &[root])
}

fn paint_roots(world: &World, roots: &[rndr_core::Entity]) -> (Vec<Quad>, Vec<Placed>) {
    let quads = RefCell::new(Vec::new());
    let draws = RefCell::new(Vec::new());
    let scale = world
        .resource::<Viewport>()
        .map(|viewport| viewport.pixels_per_point())
        .unwrap_or(1.0);
    let style = style_of(world);
    let solid = text_engine(world).solid_uv(&style);
    let theme = theme_of(world);
    for root in roots {
        walk(world, *root, &quads, &draws, scale, solid, &theme);
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
    secondary: &[rndr_core::source_presentation::SecondarySelectionMark],
    chrome: &[rndr_core::structured_chrome::StructuredChromePaint],
    decor: &Decor,
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
        out.extend_from_slice(&decor.before);
        let at = out.len();
        push(out, rect, color, solid, clip, radius, 0);
        if let (Some(corners), Some(fill)) = (decor.corners, out.get_mut(at)) {
            fill.shape.corners = Some(corners);
        }
    } else {
        out.extend_from_slice(&decor.before);
    }
    out.extend_from_slice(&decor.after);
    for layer in chrome {
        push(
            out,
            snap_box(
                layer.rect.x,
                layer.rect.y,
                layer.rect.w,
                layer.rect.h,
                scale,
            ),
            layer.color,
            solid,
            layer.clip,
            layer.radius,
            0,
        );
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
    for mark in secondary.iter().filter(|m| !m.caret) {
        push(
            out,
            snap_box(
                rect.x + TEXT_ORIGIN_X + mark.x,
                rect.y + TEXT_ORIGIN_Y + mark.y + shift_y,
                mark.w,
                mark.h,
                scale,
            ),
            theme.selection,
            solid,
            text_clip,
            0.0,
            0,
        );
    }
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
                if !run.strike || run.start >= run.end {
                    continue;
                }
                for (start, end, inside) in underline_parts(run.start, run.end, selected) {
                    let color = if inside { text_color } else { run.color };
                    for span in layout.selection_boxes(start, end) {
                        let baseline = layout
                            .lines
                            .iter()
                            .find(|line| (line.y - span.y).abs() < 0.01)
                            .map_or(span.h * 0.8, |line| line.baseline);
                        let bar = 1.0_f32.min(span.h);
                        push(
                            out,
                            snap_box(
                                rect.x + TEXT_ORIGIN_X + span.x,
                                rect.y + TEXT_ORIGIN_Y + span.y + baseline * STRIKE_RISE
                                    - bar * 0.5
                                    + shift_y,
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
    for mark in secondary.iter().filter(|m| m.caret) {
        push(
            out,
            snap_box(
                rect.x + TEXT_ORIGIN_X + mark.x,
                rect.y + TEXT_ORIGIN_Y + mark.y + shift_y,
                mark.w,
                mark.h,
                scale,
            ),
            theme.caret,
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
    let mask = world
        .get::<rndr_core::ClipRounded>(entity)
        .zip(world.get::<Rect>(entity))
        .map(|(clip, rect)| RoundClip {
            rect: *rect,
            corners: clip.corners,
            feather: clip.feather.max(0.0),
        });
    let Some(mask) = mask else {
        walk_group(world, entity, quads, draws, scale, solid, theme);
        return;
    };
    let quad_start = quads.borrow().len();
    let draw_start = draws.borrow().len();
    walk_group(world, entity, quads, draws, scale, solid, theme);
    // Inner masks were applied first and win for their own subtrees.
    let apply = |quad: &mut Quad| {
        quad.shape.round_clip.get_or_insert(mask);
    };
    quads.borrow_mut()[quad_start..].iter_mut().for_each(apply);
    for placed in &mut draws.borrow_mut()[draw_start..] {
        if let Op::Quads(inner) = &mut placed.op {
            inner.iter_mut().for_each(apply);
        }
    }
}

fn walk_group(
    world: &World,
    entity: rndr_core::Entity,
    quads: &RefCell<Vec<Quad>>,
    draws: &RefCell<Vec<Placed>>,
    scale: f32,
    solid: [f32; 4],
    theme: &Theme,
) {
    let Some(layer) = group_layer(world, entity) else {
        walk_entity(world, entity, quads, draws, scale, solid, theme);
        return;
    };
    if layer.opacity <= 0.0 {
        return;
    }
    // Paint the subtree as usual, then move its quads and draws into one
    // layer placed where the subtree started, keeping their relative order.
    let quad_start = quads.borrow().len();
    let draw_start = draws.borrow().len();
    walk_entity(world, entity, quads, draws, scale, solid, theme);
    let inner_quads = quads.borrow_mut().split_off(quad_start);
    let inner_draws = draws.borrow_mut().split_off(draw_start);
    let mut ops = vec![Op::PushLayer(layer)];
    let mut cursor = 0;
    for placed in inner_draws {
        let at = (placed.z as usize)
            .saturating_sub(quad_start)
            .min(inner_quads.len());
        if at > cursor {
            ops.push(Op::Quads(inner_quads[cursor..at].to_vec()));
            cursor = at;
        }
        ops.push(placed.op);
    }
    if cursor < inner_quads.len() {
        ops.push(Op::Quads(inner_quads[cursor..].to_vec()));
    }
    ops.push(Op::PopLayer);
    let z = quad_start as u32;
    draws
        .borrow_mut()
        .extend(ops.into_iter().map(|op| Placed { z, op }));
}

// Layer for an entity whose subtree is composited as one group. A transform
// needs a layout rect for its pivot; the parent's clip bounds the result.
fn group_layer(world: &World, entity: rndr_core::Entity) -> Option<SceneLayer> {
    let opacity = world
        .get::<rndr_core::Opacity>(entity)
        .map_or(1.0, |opacity| opacity.alpha());
    let transform = world
        .get::<rndr_core::Transform>(entity)
        .filter(|transform| !transform.is_identity())
        .zip(world.get::<Rect>(entity).copied());
    if opacity >= 1.0 && transform.is_none() {
        return None;
    }
    let mut layer = SceneLayer::opacity(opacity);
    if let Some((transform, rect)) = transform {
        layer.transform = Affine {
            m: transform.matrix(rect),
        };
        layer.clip = world
            .get::<Parent>(entity)
            .and_then(|parent| world.get::<Clip>(parent.entity))
            .map(|clip| {
                ClipShape::Scissor(Rect {
                    x: clip.x,
                    y: clip.y,
                    w: clip.w,
                    h: clip.h,
                })
            });
    }
    Some(layer)
}

#[allow(clippy::too_many_arguments)]
fn walk_entity(
    world: &World,
    entity: rndr_core::Entity,
    quads: &RefCell<Vec<Quad>>,
    draws: &RefCell<Vec<Placed>>,
    scale: f32,
    solid: [f32; 4],
    theme: &Theme,
) {
    if world.has::<rndr_core::SurfaceSlot>(entity) {
        record_canvas(world, entity, quads, draws);
        for child in rndr_core::paint_order(world, entity) {
            walk(world, child, quads, draws, scale, solid, theme);
        }
        return;
    }
    if world.has::<rndr_core::EffectSlot>(entity) {
        record_effect(world, entity, quads, draws);
        for child in rndr_core::paint_order(world, entity) {
            walk(world, child, quads, draws, scale, solid, theme);
        }
        return;
    }
    record_image(world, entity, quads, draws);
    if world.has::<MonoGrid>(entity) {
        paint_mono(world, entity, &mut quads.borrow_mut(), scale, solid, theme);
        for child in rndr_core::paint_order(world, entity) {
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
        let decor = decor_quads(world, entity, *rect, clip, radius, solid);
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
            // Structural selection is expressed by card/grid chrome, not a
            // full-height insertion caret or a false scalar text highlight.
            let chrome_selection = rndr_core::structured_native::attached_structured(world, entity)
                .and_then(|handle| {
                    world
                        .resource::<rndr_core::structured_native::StructuredDocuments>()?
                        .get(handle)
                })
                .is_some_and(|doc| match doc.selection() {
                    rndr_core::structured::StructuredSelection::Cells { .. } => true,
                    rndr_core::structured::StructuredSelection::Node(id) => doc
                        .blocks()
                        .iter()
                        .any(|b| b.id == id && b.kind == rndr_core::structured::BlockKind::Atomic),
                    _ => false,
                });
            paint_visited(
                &mut guard,
                scale,
                solid,
                theme,
                rect,
                color,
                clip,
                label,
                caret.filter(|_| !chrome_selection),
                selection.filter(|_| !chrome_selection),
                spans,
                dirty,
                layout,
                radius,
                marks,
                shift_y,
                clip_text,
                world.get::<rndr_core::CaretRow>(entity),
                rndr_core::label_box(world, entity),
                &rndr_core::source_presentation::secondary_selection_marks(world, entity),
                &rndr_core::structured_chrome::chrome_paint(world, entity),
                &decor,
            );
            // Structural markers occupy a host-reserved sibling gutter, not editable bytes.
            for mark in rndr_core::structured_chrome::structured_marks(world, entity) {
                if matches!(
                    mark.kind,
                    rndr_core::structured_chrome::StructuredMarkKind::TableCell { .. }
                        | rndr_core::structured_chrome::StructuredMarkKind::Atomic
                ) {
                    continue; // Layered behind text, not a second dark outline on top.
                }
                for r in rndr_core::structured_chrome::mark_rectangles(&mark) {
                    push(
                        &mut guard,
                        snap_box(r.x, r.y, r.w, r.h, scale),
                        theme.text,
                        solid,
                        mark.clip,
                        0.0,
                        0,
                    );
                }
            }
            // Provider overlays are additive and revalidated at the actual
            // paint boundary, never cached in host Decorations or Spans.
            let provider_clip = clip.intersect(Clip::from_rect(rect));
            for mark in rndr_core::source_presentation::provider_marks(world, entity) {
                push(
                    &mut guard,
                    snap_box(
                        rect.x
                            + if mark.gutter {
                                1.0
                            } else {
                                TEXT_ORIGIN_X + mark.x
                            },
                        rect.y + TEXT_ORIGIN_Y + mark.y + shift_y,
                        mark.w,
                        mark.h,
                        scale,
                    ),
                    mark.color,
                    solid,
                    provider_clip,
                    0.0,
                    0,
                );
            }
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
    // Above the entity's own fill, which shows as letterbox bars.
    record_surface(world, entity, quads, draws);
    record_geometry(world, entity, quads.borrow().len() as u32, draws, false);
    for child in rndr_core::paint_order(world, entity) {
        walk(world, child, quads, draws, scale, solid, theme);
    }
    record_geometry(world, entity, quads.borrow().len() as u32, draws, true);
    push_border_ring(world, entity, &mut quads.borrow_mut(), solid);
    record_scroll_indicator(world, entity, quads, solid);
    record_scrollbar(world, entity, quads, solid, theme);
}

// Interactive controls use exactly core's pointer/accessibility range. The
// narrower painted thumb does not shrink the full-width accessible hit track.
fn record_scrollbar(
    world: &World,
    entity: rndr_core::Entity,
    quads: &RefCell<Vec<Quad>>,
    solid: [f32; 4],
    theme: &Theme,
) {
    let Some(geometry) = rndr_core::scrollbar_geometry(world, entity) else {
        return;
    };
    let Some(bar) = world.get::<rndr_core::Scrollbar>(entity) else {
        return;
    };
    let active = rndr_core::scrollbar_dragging(world, entity)
        || world.resource::<Focus>().and_then(|f| f.entity) == Some(entity);
    let mut color = if active { theme.caret } else { theme.text };
    color.a *= if geometry.max <= 0.0 {
        0.15
    } else if active {
        0.85
    } else {
        0.4
    };
    let mut thumb = geometry.thumb;
    let cross = if bar.axis == rndr_core::Axis::Row {
        thumb.h
    } else {
        thumb.w
    };
    let inset = (cross * 0.25).min(4.0);
    if bar.axis == rndr_core::Axis::Row {
        thumb.y += inset;
        thumb.h -= inset * 2.0;
    } else {
        thumb.x += inset;
        thumb.w -= inset * 2.0;
    }
    let radius = thumb.w.min(thumb.h) * 0.5;
    push(
        &mut quads.borrow_mut(),
        thumb,
        color,
        solid,
        Clip::from_rect(geometry.track),
        radius,
        0,
    );
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
    if visible.w < 8. || visible.h < 8. {
        return;
    }
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
        let thumb = (track * visible_length / content).max(18.).min(track);
        // Ancestor clipping can hide either end of this port. Describe the
        // content interval actually visible, including its hidden prefix.
        // The minimum thumb size preserves legibility, not an extra hit area.
        let hidden_prefix = if axis == 0 {
            visible.x - rect.x
        } else {
            visible.y - rect.y
        };
        let start = -scroll.offset[axis] + hidden_prefix;
        let position = (start / (content - visible_length)).clamp(0., 1.) * (track - thumb);
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
    if (outline
        && (!world.has::<rndr_core::Border>(entity) || world.has::<rndr_core::BorderSides>(entity)))
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

fn record_effect(
    world: &World,
    entity: rndr_core::Entity,
    quads: &RefCell<Vec<Quad>>,
    draws: &RefCell<Vec<Placed>>,
) {
    let Some(slot) = world.get::<rndr_core::EffectSlot>(entity).copied() else {
        return;
    };
    let Some(rect) = world.get::<Rect>(entity).copied() else {
        return;
    };
    let clip = world
        .get::<Parent>(entity)
        .and_then(|parent| world.get::<Clip>(parent.entity))
        .map(|clip| Rect {
            x: clip.x,
            y: clip.y,
            w: clip.w,
            h: clip.h,
        });
    let z = quads.borrow().len() as u32;
    draws.borrow_mut().push(Placed {
        z,
        op: Op::EffectSlot(EffectSlotOp {
            id: slot.id,
            rect,
            clip,
            data: slot.data,
            backdrop: slot.backdrop,
        }),
    });
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

fn record_surface(
    world: &World,
    entity: rndr_core::Entity,
    quads: &RefCell<Vec<Quad>>,
    draws: &RefCell<Vec<Placed>>,
) {
    let Some(view) = world.get::<rndr_core::SurfaceView>(entity) else {
        return;
    };
    let Some(rect) = world.get::<Rect>(entity).copied() else {
        return;
    };
    let Some((dest, uv)) = view.placement(rect) else {
        return;
    };
    let clip = world.get::<Clip>(entity).copied().map(|clip| Rect {
        x: clip.x,
        y: clip.y,
        w: clip.w,
        h: clip.h,
    });
    let z = quads.borrow().len() as u32;
    draws.borrow_mut().push(Placed {
        z,
        op: Op::Surface(SurfaceOp {
            surface: view.surface.clone(),
            dest,
            uv,
            mirrored: view.mirrored,
            radius: view.radius,
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
    let selectable = world
        .get::<rndr_core::Selectable>(entity)
        .zip(world.get::<Label>(entity))
        .map(|(selection, label)| selection.clamped(&label.text))
        .filter(|selection| !selection.is_empty())
        .map(|selection| TextSelection {
            anchor: selection.anchor,
            head: selection.head,
        });
    visit(
        rect,
        color,
        clip,
        world.get::<Label>(entity),
        world.get::<Caret>(entity),
        world.get::<TextSelection>(entity).or(selectable.as_ref()),
        spans.as_ref(),
        dirty,
        layout,
        radius,
        marks,
        shift_y,
        clip_text,
    );
}

/// Decoration quads around an entity's fill.
#[derive(Default)]
struct Decor {
    corners: Option<[f32; 4]>,
    /// Outer shadows, behind the fill.
    before: Vec<Quad>,
    /// Gradient bands, then inset shadows, over the fill.
    after: Vec<Quad>,
}

fn decor_quads(
    world: &World,
    entity: rndr_core::Entity,
    rect: Rect,
    clip: Clip,
    radius: f32,
    solid: [f32; 4],
) -> Decor {
    let corners = world.get::<rndr_core::Corners>(entity).map(|c| c.0);
    let mut decor = Decor {
        corners,
        ..Decor::default()
    };
    let radii = corners.unwrap_or([radius; 4]);
    let quad = |rect: Rect, color: Color, clip: Clip, radii: [f32; 4], kind: QuadKind| Quad {
        rect,
        color,
        uv: solid,
        clip,
        radius,
        atlas: 0,
        shape: QuadShape {
            corners: Some(radii),
            kind,
            round_clip: None,
        },
    };
    if let Some(gradient) = world.get::<rndr_core::Gradient>(entity) {
        let (stops, axis) = match gradient {
            rndr_core::Gradient::Linear { angle, stops } => {
                let angle = if angle.is_finite() { *angle } else { 0.0 };
                let dir = [-angle.sin(), angle.cos()];
                let half = (rect.w * dir[0].abs() + rect.h * dir[1].abs()) * 0.5;
                let c = [rect.x + rect.w * 0.5, rect.y + rect.h * 0.5];
                (
                    stops,
                    Ok((
                        [c[0] - dir[0] * half, c[1] - dir[1] * half],
                        [c[0] + dir[0] * half, c[1] + dir[1] * half],
                    )),
                )
            }
            rndr_core::Gradient::Radial {
                center,
                radius,
                stops,
            } => (
                stops,
                Err((
                    [rect.x + rect.w * center[0], rect.y + rect.h * center[1]],
                    radius * rect.w.min(rect.h) * 0.5,
                )),
            ),
        };
        let mut stops: Vec<_> = stops.iter().filter(|s| s.offset.is_finite()).collect();
        stops.sort_by(|a, b| a.offset.total_cmp(&b.offset));
        let bands: Vec<_> = match stops.len() {
            0 => Vec::new(),
            1 => vec![(stops[0], stops[0])],
            _ => stops.windows(2).map(|w| (w[0], w[1])).collect(),
        };
        let last = bands.len().saturating_sub(1);
        for (index, (from, to)) in bands.into_iter().enumerate() {
            let range = [from.offset, to.offset.max(from.offset + 1.0e-6)];
            let extend = [index == 0, index == last];
            let kind = match axis {
                Ok((start, end)) => QuadKind::Linear {
                    start,
                    end,
                    range,
                    to: to.color,
                    extend,
                },
                Err((center, radius)) => QuadKind::Radial {
                    center,
                    radius,
                    range,
                    to: to.color,
                    extend,
                },
            };
            decor.after.push(quad(rect, from.color, clip, radii, kind));
        }
    }
    if let Some(rndr_core::Shadows(list)) = world.get::<rndr_core::Shadows>(entity) {
        let outer_clip = world
            .get::<Parent>(entity)
            .and_then(|p| world.get::<Clip>(p.entity).copied());
        for shadow in list.iter().filter(|s| s.color.a > 0.0) {
            let sigma = if shadow.blur.is_finite() {
                (shadow.blur * 0.5).max(0.0)
            } else {
                0.0
            };
            let spread = if shadow.inset {
                -shadow.spread
            } else {
                shadow.spread
            };
            let shape = Rect {
                x: rect.x + shadow.offset[0] - spread,
                y: rect.y + shadow.offset[1] - spread,
                w: (rect.w + 2.0 * spread).max(0.0),
                h: (rect.h + 2.0 * spread).max(0.0),
            };
            let shape_radii = radii.map(|r| if r > 0.0 { (r + spread).max(0.0) } else { 0.0 });
            if shadow.inset {
                decor.after.push(quad(
                    rect,
                    shadow.color,
                    clip,
                    shape_radii,
                    QuadKind::Shadow {
                        shape,
                        element: rect,
                        sigma,
                        inset: true,
                    },
                ));
            } else {
                let pad = 3.0 * sigma + 1.0;
                let bounds = Rect {
                    x: shape.x - pad,
                    y: shape.y - pad,
                    w: shape.w + 2.0 * pad,
                    h: shape.h + 2.0 * pad,
                };
                let clip = outer_clip.unwrap_or_else(|| Clip::from_rect(bounds));
                decor.before.push(quad(
                    bounds,
                    shadow.color,
                    clip,
                    shape_radii,
                    QuadKind::Shadow {
                        shape,
                        element: rect,
                        sigma,
                        inset: false,
                    },
                ));
            }
        }
    }
    decor
}

fn push_border_ring(
    world: &World,
    entity: rndr_core::Entity,
    out: &mut Vec<Quad>,
    solid: [f32; 4],
) {
    let Some(border) = world.get::<rndr_core::BorderSides>(entity).copied() else {
        return;
    };
    let Some(rect) = world.get::<Rect>(entity).copied() else {
        return;
    };
    let clip = world
        .get::<Clip>(entity)
        .copied()
        .unwrap_or_else(|| Clip::from_rect(rect));
    let radius = world.get::<Radius>(entity).map_or(0.0, |r| r.0);
    let corners = world
        .get::<rndr_core::Corners>(entity)
        .map_or([radius; 4], |c| c.0);
    let at = out.len();
    push(out, rect, border.color, solid, clip, radius, 0);
    if let Some(ring) = out.get_mut(at) {
        ring.shape = QuadShape {
            corners: Some(corners),
            kind: QuadKind::Ring {
                widths: border.widths,
                dash: border.dash.unwrap_or([0.0, 0.0]),
            },
            round_clip: None,
        };
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
        shape: QuadShape::default(),
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

/// A strikethrough's centre as a fraction of the line's baseline depth:
/// about half the x-height above the baseline for common faces.
const STRIKE_RISE: f32 = 0.7;

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
