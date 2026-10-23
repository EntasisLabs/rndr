//! Display list for a 2D scene.
//!
//! [`paint`](crate::paint) still returns [`Quad`](crate::Quad)s, and that list
//! is the fast path for widget fills and glyphs. A canvas, an image, or a
//! layer is a [`Op`] recorded beside those quads. [`Placed::z`] is the quad
//! index the command is drawn before, so a slot's scene sits under the UI
//! that was laid out on top of it, and a list-row icon sits with its row.
//!
//! Transforms are applied on the GPU. Tessellation stays in local space and is
//! cached by geometry and zoom bucket, so a pan only rewrites the root camera.

use std::sync::Arc;

use rndr_core::{
    text_engine, Color, Rect, TextEngine, TextLayout, TextSpan, TextStyle, Viewport, World,
};

use crate::affine::{size_bucket, Affine, OUTLINE_PX};
use crate::path::{FillRule, Path, Stroke};

/// One recorded draw, in local space unless the variant says otherwise.
#[derive(Clone, Debug)]
pub enum Op {
    /// Filled and/or stroked path.
    Path(PathOp),
    /// Textured quad. The id is the one passed to the image loader.
    Image(ImageOp),
    /// Atlas glyphs, plus the source used to re-raster or switch to outlines.
    Glyphs(GlyphOp),
    /// Multiply the current transform by this one.
    PushTransform(Affine),
    /// Remove the transform pushed last. Extra pops are ignored.
    PopTransform,
    /// Start a group: opacity, clip, and an optional drop shadow.
    PushLayer(SceneLayer),
    /// End the group started last. Extra pops are ignored.
    PopLayer,
}

/// Where a [`Op`] sits relative to the UI quad list.
#[derive(Clone, Debug)]
pub struct Placed {
    /// Draw this before `quads[z]`. `z == quads.len()` draws after every quad.
    pub z: u32,
    /// The command.
    pub op: Op,
}

/// A path with an optional fill and an optional stroke.
#[derive(Clone, Debug)]
pub struct PathOp {
    /// Geometry in local units.
    pub path: Path,
    /// Interior. `None` draws no fill.
    pub fill: Option<FillStyle>,
    /// Centerline. `None` draws no stroke.
    pub stroke: Option<StrokeStyle>,
}

/// Fill color and coverage rule.
#[derive(Clone, Copy, Debug)]
pub struct FillStyle {
    /// sRGB fill with straight alpha.
    pub color: Color,
    /// Non-zero or even-odd.
    pub rule: FillRule,
}

/// Stroke color and geometry.
#[derive(Clone, Debug)]
pub struct StrokeStyle {
    /// sRGB color with straight alpha.
    pub color: Color,
    /// Width, caps, joins, dashes, and the hairline flag.
    pub stroke: Stroke,
}

/// Image draw. Crop is a UV rectangle. Rounded corners are a local-space mask.
#[derive(Clone, Copy, Debug)]
pub struct ImageOp {
    /// Key shared with the decoded texture.
    pub id: u64,
    /// Destination in local units.
    pub dest: Rect,
    /// Crop as `(u0, v0, u1, v1)` in the 0–1 range of the image.
    pub uv: [f32; 4],
    /// Corner radius in local units. Zero is square.
    pub radius: f32,
    /// Multiplier on the image alpha, in `0.0..=1.0`.
    pub opacity: f32,
}

/// One atlas glyph in local units.
#[derive(Clone, Copy, Debug)]
pub struct GlyphSprite {
    /// Left edge, local units.
    pub x: f32,
    /// Top edge, local units.
    pub y: f32,
    /// Width, local units.
    pub w: f32,
    /// Height, local units.
    pub h: f32,
    /// Atlas rectangle `(u0, v0, u1, v1)`.
    pub uv: [f32; 4],
    /// Tint. Coverage glyphs multiply it. Color glyphs use the texel rgb.
    pub color: Color,
    /// `0` samples the coverage atlas. Any other value samples the color atlas.
    pub atlas: u8,
}

/// Glyph run. Sprites are the atlas fallback. [`Self::source`] lets the
/// renderer re-raster when the size bucket changes and switch to outlines at
/// [`OUTLINE_PX`].
#[derive(Clone, Debug)]
pub struct GlyphOp {
    /// Sprites rasterized at [`Self::raster_scale`] pixels per world unit.
    pub sprites: Arc<[GlyphSprite]>,
    /// Em size in world units.
    pub em: f32,
    /// `pixels_per_point` the sprites were rastered with.
    pub raster_scale: f32,
    /// Text the renderer can shape again. `None` keeps the sprites as they are.
    pub source: Option<Arc<TextSource>>,
}

/// Text a canvas run was built from.
#[derive(Clone, Debug)]
pub struct TextSource {
    /// Shaped string.
    pub text: String,
    /// Family, size, weight, style, spacing, and alignment.
    pub style: TextStyle,
    /// Per-range color, weight, and underline.
    pub spans: Vec<TextSpan>,
    /// Wrap width in world units. Non-finite keeps hard breaks only.
    pub max_width: f32,
    /// Top-left of the text block in local units.
    pub origin: [f32; 2],
    /// Color for bytes no span tints.
    pub color: Color,
}

/// Group pushed around later commands.
#[derive(Clone, Debug)]
pub struct SceneLayer {
    /// Multiplier on the group's alpha. `1.0` is opaque. `0.0` draws nothing
    /// unless a shadow is set, in which case only the shadow is drawn.
    pub opacity: f32,
    /// Clip applied to the group, in the transform that is current at the push.
    pub clip: Option<ClipShape>,
    /// Drawn under the group when it is composited.
    pub effect: Option<Effect>,
}

impl Default for SceneLayer {
    fn default() -> Self {
        Self {
            opacity: 1.0,
            clip: None,
            effect: None,
        }
    }
}

impl SceneLayer {
    /// Group opacity, no clip and no effect.
    pub fn opacity(opacity: f32) -> Self {
        Self {
            opacity,
            ..Self::default()
        }
    }

    /// Screen-space rectangle, in logical points, ignoring the world transform.
    ///
    /// This is how a canvas is clipped to its [`rndr_core::SurfaceSlot`].
    pub fn scissor(rect: Rect) -> Self {
        Self {
            clip: Some(ClipShape::Scissor(rect)),
            ..Self::default()
        }
    }

    /// Rounded-rect clip in the current local space.
    pub fn clip_round_rect(rect: Rect, radius: f32) -> Self {
        Self {
            clip: Some(ClipShape::RoundRect { rect, radius }),
            ..Self::default()
        }
    }

    /// Path clip in the current local space.
    pub fn clip_path(path: Path) -> Self {
        Self {
            clip: Some(ClipShape::Path(path)),
            ..Self::default()
        }
    }

    /// Store `effect` and return the layer.
    pub fn with_effect(mut self, effect: Effect) -> Self {
        self.effect = Some(effect);
        self
    }
}

/// What a [`SceneLayer`] clips to.
#[derive(Clone, Debug)]
pub enum ClipShape {
    /// Rounded rectangle in local units.
    RoundRect {
        /// Clip rectangle.
        rect: Rect,
        /// Corner radius in local units.
        radius: f32,
    },
    /// Filled path in local units. The fill rule is non-zero.
    Path(Path),
    /// Rectangle in logical screen points. The world transform does not move it.
    Scissor(Rect),
}

/// Filter drawn with a layer. More passes can sit beside this one.
#[derive(Clone, Copy, Debug)]
pub enum Effect {
    /// Separable Gaussian of the group's alpha, offset and tinted.
    ///
    /// `offset` and `blur` are local units. `blur` is the standard deviation.
    DropShadow {
        /// Offset of the shadow, local x then y, y down.
        offset: [f32; 2],
        /// Gaussian sigma in local units.
        blur: f32,
        /// Shadow color. Alpha is the shadow opacity.
        color: Color,
    },
}

/// Root camera plus the commands recorded under it.
#[derive(Clone, Debug, Default)]
pub struct Scene {
    /// Applied before every command. Identity leaves local units in place.
    pub camera: Affine,
    /// Commands in painter order.
    pub commands: Vec<Op>,
}

/// Records a [`Scene`]. Push and pop are counted so a missing pop cannot leak
/// into the next draw, and an extra pop is ignored.
#[derive(Clone, Debug)]
pub struct Canvas {
    scene: Scene,
    transforms: u32,
    layers: u32,
}

impl Default for Canvas {
    fn default() -> Self {
        Self::new()
    }
}

impl Canvas {
    /// Empty scene, identity camera.
    pub fn new() -> Self {
        Self {
            scene: Scene::default(),
            transforms: 0,
            layers: 0,
        }
    }

    /// Replace the root camera.
    ///
    /// Pan and zoom belong here. Geometry recorded underneath is not rebuilt
    /// until the zoom crosses a bucket.
    pub fn set_camera(&mut self, camera: Affine) {
        self.scene.camera = camera;
    }

    /// The scene recorded so far, including the camera.
    pub fn scene(&self) -> &Scene {
        &self.scene
    }

    /// Finish and return the scene. Unbalanced pushes are closed.
    pub fn into_scene(mut self) -> Scene {
        self.close_open();
        self.scene
    }

    /// Wrap the recording in a scissor layer so it stays inside `slot`.
    ///
    /// The slot rectangle is in logical points. The camera is pushed inside
    /// that layer.
    pub fn finish_clipped(mut self, slot: Rect) -> Vec<Op> {
        self.close_open();
        let mut out = Vec::with_capacity(self.scene.commands.len() + 4);
        out.push(Op::PushLayer(SceneLayer::scissor(slot)));
        if self.scene.camera != Affine::IDENTITY {
            out.push(Op::PushTransform(self.scene.camera));
        }
        out.append(&mut self.scene.commands);
        if self.scene.camera != Affine::IDENTITY {
            out.push(Op::PopTransform);
        }
        out.push(Op::PopLayer);
        out
    }

    /// Push `affine` for commands recorded until [`pop_transform`](Self::pop_transform).
    pub fn push_transform(&mut self, affine: Affine) {
        self.scene.commands.push(Op::PushTransform(affine));
        self.transforms += 1;
    }

    /// Pop the transform pushed last.
    pub fn pop_transform(&mut self) {
        if self.transforms == 0 {
            return;
        }
        self.transforms -= 1;
        self.scene.commands.push(Op::PopTransform);
    }

    /// Push a group. Commands until [`pop_layer`](Self::pop_layer) are drawn
    /// into it.
    pub fn push_layer(&mut self, layer: SceneLayer) {
        self.scene.commands.push(Op::PushLayer(layer));
        self.layers += 1;
    }

    /// Pop the layer pushed last.
    pub fn pop_layer(&mut self) {
        if self.layers == 0 {
            return;
        }
        self.layers -= 1;
        self.scene.commands.push(Op::PopLayer);
    }

    /// Fill `path`.
    pub fn fill(&mut self, path: Path, color: Color, rule: FillRule) {
        self.scene.commands.push(Op::Path(PathOp {
            path,
            fill: Some(FillStyle { color, rule }),
            stroke: None,
        }));
    }

    /// Stroke `path`.
    pub fn stroke(&mut self, path: Path, color: Color, stroke: Stroke) {
        self.scene.commands.push(Op::Path(PathOp {
            path,
            fill: None,
            stroke: Some(StrokeStyle { color, stroke }),
        }));
    }

    /// Fill and stroke the same path.
    pub fn fill_stroke(
        &mut self,
        path: Path,
        fill: Color,
        rule: FillRule,
        stroke: Color,
        stroke_style: Stroke,
    ) {
        self.scene.commands.push(Op::Path(PathOp {
            path,
            fill: Some(FillStyle { color: fill, rule }),
            stroke: Some(StrokeStyle {
                color: stroke,
                stroke: stroke_style,
            }),
        }));
    }

    /// Draw image `id` into `dest`. `uv` crops it. `radius` rounds the corners.
    pub fn image(&mut self, id: u64, dest: Rect, uv: [f32; 4], radius: f32) {
        self.scene.commands.push(Op::Image(ImageOp {
            id,
            dest,
            uv,
            radius,
            opacity: 1.0,
        }));
    }

    /// Like [`image`](Self::image) with an alpha multiplier.
    pub fn image_opacity(&mut self, op: ImageOp) {
        self.scene.commands.push(Op::Image(op));
    }

    /// Shape `source` and record glyphs.
    ///
    /// The raster size is `style.size` times the camera scale times the current
    /// viewport scale, snapped with [`size_bucket`]. At or above [`OUTLINE_PX`]
    /// the sprites are still recorded, and the renderer replaces them with
    /// outlines when it can. Letter spacing, paragraph spacing, alignment, and
    /// underlines come from [`TextStyle`] and [`TextSpan`].
    pub fn text(&mut self, world: &World, source: TextSource) {
        let viewport = world
            .resource::<Viewport>()
            .map(|viewport| viewport.pixels_per_point())
            .unwrap_or(1.0);
        let device = source.style.size * self.scene.camera.uniform_scale() * viewport;
        // Atlas sprites stop at [`OUTLINE_PX`]. Larger sizes are filled from outlines.
        let bucket = size_bucket(device.min(OUTLINE_PX));
        let raster_scale = if source.style.size > 1.0e-3 {
            bucket / source.style.size
        } else {
            viewport
        };
        let engine = text_engine(world);
        let _guard = ScaleGuard {
            engine: Arc::clone(&engine),
            prev: viewport,
        };
        engine.set_pixels_per_point(raster_scale);
        let layout = engine.layout(&source.text, &source.style, &source.spans, source.max_width);
        drop(_guard);
        let sprites = sprites_from(&layout, &source);
        let underlines = underline_paths(&layout, &source);
        let em = source.style.size;
        self.scene.commands.push(Op::Glyphs(GlyphOp {
            sprites: Arc::from(sprites),
            em,
            raster_scale,
            source: Some(Arc::new(source)),
        }));
        for (path, color) in underlines {
            self.fill(path, color, FillRule::NonZero);
        }
    }

    fn close_open(&mut self) {
        while self.transforms > 0 {
            self.pop_transform();
        }
        while self.layers > 0 {
            self.pop_layer();
        }
    }
}

struct ScaleGuard {
    engine: Arc<dyn TextEngine>,
    prev: f32,
}

impl Drop for ScaleGuard {
    fn drop(&mut self) {
        self.engine.set_pixels_per_point(self.prev);
    }
}

fn sprites_from(layout: &TextLayout, source: &TextSource) -> Vec<GlyphSprite> {
    layout
        .glyphs
        .iter()
        .map(|glyph| GlyphSprite {
            x: source.origin[0] + glyph.x,
            y: source.origin[1] + glyph.y,
            w: glyph.w,
            h: glyph.h,
            uv: glyph.uv,
            color: glyph.color.unwrap_or(source.color),
            atlas: glyph.atlas,
        })
        .collect()
}

fn underline_paths(layout: &TextLayout, source: &TextSource) -> Vec<(Path, Color)> {
    let mut out = Vec::new();
    for span in &source.spans {
        if !span.underline || span.start >= span.end {
            continue;
        }
        for selection in layout.selection_boxes(span.start, span.end) {
            if selection.w <= 0.0 || selection.h <= 0.0 {
                continue;
            }
            let y = source.origin[1] + selection.y + selection.h - 1.0;
            let rect = Rect {
                x: source.origin[0] + selection.x,
                y,
                w: selection.w,
                h: 1.0,
            };
            out.push((Path::round_rect(rect, 0.0), span.color));
        }
    }
    out
}

/// Draws the interior of one [`rndr_core::SurfaceSlot`].
///
/// The painter is called from [`crate::frame`] with the world after layout.
/// State that changes every frame lives in a resource the painter reads.
pub trait CanvasPainter: Send + Sync {
    /// Record into `canvas`. The slot rect is not applied yet; [`crate::frame`]
    /// clips the scene to the slot.
    fn paint(&self, world: &World, canvas: &mut Canvas);
}

/// Painters keyed by [`rndr_core::SurfaceSlot::id`].
///
/// Insert this resource before the first frame. A slot with no painter stays
/// empty, which is what a foreign surface still looks like.
#[derive(Default)]
pub struct CanvasRegistry {
    painters: std::collections::HashMap<u64, Arc<dyn CanvasPainter>>,
}

impl CanvasRegistry {
    /// No painters.
    pub fn new() -> Self {
        Self::default()
    }

    /// Draw `slot` with `painter` on later frames. Replaces a painter already
    /// stored for that id.
    pub fn insert(&mut self, slot: u64, painter: impl CanvasPainter + 'static) {
        self.painters.insert(slot, Arc::new(painter));
    }

    /// Painter for `slot`, if one was inserted.
    pub fn get(&self, slot: u64) -> Option<Arc<dyn CanvasPainter>> {
        self.painters.get(&slot).map(Arc::clone)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extra_pop_does_not_unwind_the_root() {
        let mut canvas = Canvas::new();
        canvas.pop_transform();
        canvas.pop_layer();
        canvas.push_transform(Affine::translate(1.0, 2.0));
        canvas.pop_transform();
        canvas.pop_transform();
        let scene = canvas.into_scene();
        assert!(matches!(scene.commands[0], Op::PushTransform(_)));
        assert!(matches!(scene.commands[1], Op::PopTransform));
        assert_eq!(scene.commands.len(), 2);
    }

    #[test]
    fn clipped_finish_wraps_the_camera() {
        let mut canvas = Canvas::new();
        canvas.set_camera(Affine::scale(2.0));
        canvas.fill(
            Path::line([0.0, 0.0], [1.0, 0.0]),
            Color::rgb(1.0, 0.0, 0.0),
            FillRule::NonZero,
        );
        let ops = canvas.finish_clipped(Rect {
            x: 0.0,
            y: 0.0,
            w: 10.0,
            h: 10.0,
        });
        assert!(matches!(ops.first(), Some(Op::PushLayer(_))));
        assert!(matches!(ops.get(1), Some(Op::PushTransform(_))));
        assert!(matches!(ops.last(), Some(Op::PopLayer)));
    }
}
