//! Text shaping seam.
//!
//! Labels, fields, the editor, and the caret measure and hit-test through
//! [`TextEngine`]. The builtin engine ([`BuiltinText`]) is what a world gets
//! until something replaces it. It shapes with the faces already in the
//! process: the 8×8 bitmap, and Inter when that face is the [`crate::TextFace`].
//!
//! A future engine implements the same trait with parley and swash:
//! - [`TextEngine::layout`] asks parley to shape a run at the given family,
//!   size, weight, and style, then breaks the paragraph into lines.
//! - Each shaped line is kept in the two-frame cache, keyed by the line text,
//!   the style, and the width. Lines touched this frame or the previous frame
//!   stay; the rest are dropped on the next [`TextEngine::begin_frame`].
//! - swash rasters outline glyphs into the coverage atlas and emoji into the
//!   color atlas. Glyphs record which atlas to sample (`PlacedGlyph::atlas`).
//! - [`TextEngine::fallbacks`] is the ordered family list parley walks when a
//!   face has no glyph.
//!
//! Core never mentions parley. Paint reads the [`crate::TextLayout`] the engine
//! returns.

use std::any::Any;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use crate::font::{
    layout_text_weighted, shared_bitmap, text_face, wrap_ranges, ColorAtlas, Font, FontAtlas,
    TextLayout, TextMeasure, TextSpan,
};
use crate::world::World;

/// Regular and bold weights. A run can override these per range.
pub const FONT_WEIGHT_REGULAR: u16 = 400;
pub const FONT_WEIGHT_BOLD: u16 = 700;

/// Upright or italic. The builtin atlas has no italic master; the value is
/// still part of the cache key so a later engine can.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FontStyle {
    Normal,
    Italic,
}

/// What to shape a run with. Theme colors are applied at paint time; this is
/// the font request.
#[derive(Clone, Debug, PartialEq)]
pub struct TextStyle {
    pub family: String,
    pub size: f32,
    pub weight: u16,
    pub style: FontStyle,
}

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            family: "bitmap".into(),
            size: 16.0,
            weight: FONT_WEIGHT_REGULAR,
            style: FontStyle::Normal,
        }
    }
}

impl TextStyle {
    pub fn from_face(font: &dyn Font) -> Self {
        Self {
            family: font.family_name().to_string(),
            size: font.em_size(),
            weight: FONT_WEIGHT_REGULAR,
            style: FontStyle::Normal,
        }
    }
}

/// Style of the world's [`crate::TextFace`], or the bitmap face when none is set.
pub fn style_of(world: &World) -> TextStyle {
    TextStyle::from_face(text_face(world).as_ref())
}

/// Shape and hit-test text. Insert an `Arc<dyn TextEngine>` to replace the
/// builtin. [`ensure_text_engine`] installs [`BuiltinText`] when the world has
/// none.
pub trait TextEngine: Send + Sync + 'static {
    fn as_any(&self) -> &dyn Any;

    /// Shape `text` into positioned glyphs, line metrics, and highlight rects.
    /// [`TextLayout::index_at`] and [`TextLayout::caret_xy`] are the hit test
    /// and caret. Lines are cached; unchanged text is not reshaped.
    fn layout(
        &self,
        text: &str,
        style: &TextStyle,
        runs: &[TextSpan],
        max_width: f32,
    ) -> TextLayout;

    fn measure(
        &self,
        text: &str,
        style: &TextStyle,
        runs: &[TextSpan],
        max_width: f32,
    ) -> TextMeasure;

    fn line_height(&self, style: &TextStyle) -> f32;
    /// Advance of `0`, used as the floor for a field's inner width.
    fn unit_advance(&self, style: &TextStyle) -> f32;
    fn solid_uv(&self, style: &TextStyle) -> [f32; 4];

    /// Coverage atlas for the default family. `None` means the 8×8 bitmap.
    fn coverage_atlas(&self) -> Option<Arc<FontAtlas>>;
    /// Color atlas for emoji. `None` until a face provides one.
    fn color_atlas(&self) -> Option<Arc<ColorAtlas>>;

    /// Ordered face names for `family`. Missing glyphs try the next name.
    fn fallbacks(&self, family: &str) -> Vec<String>;
    fn set_fallbacks(&self, family: &str, faces: &[String]);
    fn register_font(&self, name: &str, font: Arc<dyn Font>);

    /// Start a frame. Lines used in the new frame and the one before it stay;
    /// everything else is dropped.
    fn begin_frame(&self);
    /// How many line shapes missed the cache since this engine was built.
    fn shape_count(&self) -> u64;
}

/// The engine the world is using, creating the builtin one if needed.
pub fn text_engine(world: &World) -> Arc<dyn TextEngine> {
    if let Some(engine) = world.resource::<Arc<dyn TextEngine>>() {
        if let Some(builtin) = engine.as_any().downcast_ref::<BuiltinText>() {
            builtin.sync_face(text_face(world));
        }
        return Arc::clone(engine);
    }
    Arc::new(BuiltinText::with_face(text_face(world)))
}

/// Install [`BuiltinText`] if the world has no engine, and point it at the
/// current [`crate::TextFace`].
pub fn ensure_text_engine(world: &mut World) {
    if !world.has_resource::<Arc<dyn TextEngine>>() {
        let engine: Arc<dyn TextEngine> = Arc::new(BuiltinText::new());
        world.insert_resource(engine);
    }
    let face = text_face(world);
    let Some(engine) = world.resource::<Arc<dyn TextEngine>>() else {
        return;
    };
    if let Some(builtin) = engine.as_any().downcast_ref::<BuiltinText>() {
        builtin.sync_face(face);
    }
}

struct EngineState {
    fonts: HashMap<String, Arc<dyn Font>>,
    fallbacks: HashMap<String, Vec<String>>,
    default_family: String,
    cache: LineCache,
}

struct LineCache {
    frames: [HashMap<LineKey, Arc<TextLayout>>; 2],
    current: usize,
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct LineKey {
    text: String,
    family: String,
    size_bits: u32,
    weight: u16,
    italic: bool,
    width_bits: u32,
    runs: Vec<RunKey>,
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct RunKey {
    start: u32,
    end: u32,
    rgba: [u32; 4],
    weight: Option<u16>,
    face: u8,
    highlight: Option<[u32; 4]>,
}

/// Builtin shaper. One coverage atlas, an empty color-atlas slot, and a
/// family-to-faces map. Paragraphs are broken into lines and each line is
/// cached across the current frame and the previous one.
pub struct BuiltinText {
    state: Mutex<EngineState>,
    shapes: AtomicU64,
}

impl BuiltinText {
    pub fn new() -> Self {
        let mut fonts = HashMap::new();
        fonts.insert("bitmap".to_string(), shared_bitmap());
        let mut fallbacks = HashMap::new();
        fallbacks.insert("bitmap".to_string(), vec!["bitmap".to_string()]);
        Self {
            state: Mutex::new(EngineState {
                fonts,
                fallbacks,
                default_family: "bitmap".to_string(),
                cache: LineCache {
                    frames: [HashMap::new(), HashMap::new()],
                    current: 0,
                },
            }),
            shapes: AtomicU64::new(0),
        }
    }

    pub fn with_face(face: Arc<dyn Font>) -> Self {
        let engine = Self::new();
        engine.sync_face(face);
        engine
    }

    pub fn sync_face(&self, face: Arc<dyn Font>) {
        let name = face.family_name().to_string();
        self.register_font(&name, face);
        self.lock().default_family = name;
    }

    fn lock(&self) -> MutexGuard<'_, EngineState> {
        self.state.lock().unwrap_or_else(|err| err.into_inner())
    }

    fn fonts_for(&self, family: &str) -> Vec<Arc<dyn Font>> {
        let state = self.lock();
        let names = if let Some(list) = state.fallbacks.get(family) {
            list.clone()
        } else if family.is_empty() {
            vec![state.default_family.clone()]
        } else {
            vec![family.to_string()]
        };
        let mut fonts = Vec::new();
        for name in &names {
            if let Some(font) = state.fonts.get(name) {
                fonts.push(Arc::clone(font));
            }
        }
        if fonts.is_empty() {
            if let Some(font) = state.fonts.get(&state.default_family) {
                fonts.push(Arc::clone(font));
            }
        }
        if fonts.is_empty() {
            fonts.push(shared_bitmap());
        }
        fonts
    }
}

impl Default for BuiltinText {
    fn default() -> Self {
        Self::new()
    }
}

impl TextEngine for BuiltinText {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn layout(
        &self,
        text: &str,
        style: &TextStyle,
        runs: &[TextSpan],
        max_width: f32,
    ) -> TextLayout {
        let fonts = self.fonts_for(&style.family);
        let stack = FaceStack {
            fonts,
            size: style.size,
        };
        let ranges = wrap_ranges(&stack, text, max_width);
        let mut glyphs = Vec::new();
        let mut lines = Vec::new();
        let mut highlights = Vec::new();
        let mut width = 0.0f32;
        let mut y = 0.0f32;
        for (origin, end) in ranges {
            let line_text = &text[origin..end];
            let local_runs = clip_runs(runs, origin, end);
            let key = line_key(line_text, style, max_width, &local_runs);
            let cached = self.cached_line(key, || {
                layout_text_weighted(&stack, line_text, f32::INFINITY, &local_runs, style.weight)
            });
            let line_y = y;
            for glyph in &cached.glyphs {
                let mut glyph = *glyph;
                glyph.start += origin;
                glyph.end += origin;
                glyph.y += line_y;
                glyphs.push(glyph);
            }
            for line in &cached.lines {
                lines.push(crate::font::TextLine {
                    start: origin + line.start,
                    end: origin + line.end,
                    y: line_y + line.y,
                    h: line.h,
                    width: line.width,
                    edges: line
                        .edges
                        .iter()
                        .map(|(index, x)| (origin + index, *x))
                        .collect(),
                });
                y += line.h;
            }
            if cached.lines.is_empty() {
                y += stack.line_height().max(0.0);
            }
            for highlight in &cached.highlights {
                highlights.push(crate::font::TextHighlight {
                    x: highlight.x,
                    y: line_y + highlight.y,
                    w: highlight.w,
                    h: highlight.h,
                    color: highlight.color,
                });
            }
            width = width.max(cached.width);
        }
        if lines.is_empty() {
            let h = stack.line_height().max(0.0);
            lines.push(crate::font::TextLine {
                start: 0,
                end: 0,
                y: 0.0,
                h,
                width: 0.0,
                edges: vec![(0, 0.0)],
            });
            y = h;
        }
        TextLayout {
            glyphs,
            lines,
            highlights,
            width,
            height: y,
            cache_key: 0,
        }
    }

    fn measure(
        &self,
        text: &str,
        style: &TextStyle,
        runs: &[TextSpan],
        max_width: f32,
    ) -> TextMeasure {
        let layout = self.layout(text, style, runs, max_width);
        TextMeasure {
            width: layout.width,
            height: layout.height,
            line_count: layout.lines.len().max(1),
        }
    }

    fn line_height(&self, style: &TextStyle) -> f32 {
        FaceStack {
            fonts: self.fonts_for(&style.family),
            size: style.size,
        }
        .line_height()
    }

    fn unit_advance(&self, style: &TextStyle) -> f32 {
        FaceStack {
            fonts: self.fonts_for(&style.family),
            size: style.size,
        }
        .advance('0')
    }

    fn solid_uv(&self, style: &TextStyle) -> [f32; 4] {
        let fonts = self.fonts_for(&style.family);
        fonts
            .first()
            .map(|font| font.solid_uv())
            .unwrap_or_else(|| shared_bitmap().solid_uv())
    }

    fn coverage_atlas(&self) -> Option<Arc<FontAtlas>> {
        let family = self.lock().default_family.clone();
        self.fonts_for(&family)
            .into_iter()
            .find_map(|font| font.atlas())
    }

    fn color_atlas(&self) -> Option<Arc<ColorAtlas>> {
        let family = self.lock().default_family.clone();
        self.fonts_for(&family)
            .into_iter()
            .find_map(|font| font.color_atlas())
    }

    fn fallbacks(&self, family: &str) -> Vec<String> {
        let state = self.lock();
        if let Some(list) = state.fallbacks.get(family) {
            return list.clone();
        }
        if family.is_empty() {
            return vec![state.default_family.clone()];
        }
        vec![family.to_string()]
    }

    fn set_fallbacks(&self, family: &str, faces: &[String]) {
        self.lock()
            .fallbacks
            .insert(family.to_string(), faces.to_vec());
    }

    fn register_font(&self, name: &str, font: Arc<dyn Font>) {
        let mut state = self.lock();
        state.fonts.insert(name.to_string(), font);
        state.fallbacks.entry(name.to_string()).or_insert_with(|| {
            if name == "bitmap" {
                vec!["bitmap".to_string()]
            } else {
                vec![name.to_string(), "bitmap".to_string()]
            }
        });
    }

    fn begin_frame(&self) {
        let mut state = self.lock();
        let next = state.cache.current ^ 1;
        state.cache.current = next;
        state.cache.frames[next].clear();
    }

    fn shape_count(&self) -> u64 {
        self.shapes.load(Ordering::Relaxed)
    }
}

impl BuiltinText {
    fn cached_line(&self, key: LineKey, build: impl FnOnce() -> TextLayout) -> Arc<TextLayout> {
        {
            let mut state = self.lock();
            let current = state.cache.current;
            if let Some(hit) = state.cache.frames[current].get(&key) {
                return Arc::clone(hit);
            }
            let previous = current ^ 1;
            if let Some(hit) = state.cache.frames[previous].get(&key).cloned() {
                state.cache.frames[current].insert(key, Arc::clone(&hit));
                return hit;
            }
        }
        let line = Arc::new(build());
        {
            let mut state = self.lock();
            let current = state.cache.current;
            state.cache.frames[current].insert(key, Arc::clone(&line));
        }
        self.shapes.fetch_add(1, Ordering::Relaxed);
        line
    }
}

fn line_key(text: &str, style: &TextStyle, max_width: f32, runs: &[TextSpan]) -> LineKey {
    LineKey {
        text: text.to_string(),
        family: style.family.clone(),
        size_bits: style.size.to_bits(),
        weight: style.weight,
        italic: style.style == FontStyle::Italic,
        width_bits: max_width.to_bits(),
        runs: runs
            .iter()
            .map(|run| RunKey {
                start: run.start as u32,
                end: run.end as u32,
                rgba: [
                    run.color.r.to_bits(),
                    run.color.g.to_bits(),
                    run.color.b.to_bits(),
                    run.color.a.to_bits(),
                ],
                weight: run.weight,
                face: run.face,
                highlight: run.highlight.map(|color| {
                    [
                        color.r.to_bits(),
                        color.g.to_bits(),
                        color.b.to_bits(),
                        color.a.to_bits(),
                    ]
                }),
            })
            .collect(),
    }
}

fn clip_runs(runs: &[TextSpan], start: usize, end: usize) -> Vec<TextSpan> {
    let mut clipped = Vec::new();
    for run in runs {
        let from = run.start.max(start);
        let to = run.end.min(end);
        if from >= to {
            continue;
        }
        clipped.push(TextSpan {
            start: from - start,
            end: to - start,
            color: run.color,
            face: run.face,
            weight: run.weight,
            highlight: run.highlight,
        });
    }
    clipped
}

/// Faces tried in order for one style. Advances scale by `size / em_size`.
struct FaceStack {
    fonts: Vec<Arc<dyn Font>>,
    size: f32,
}

impl FaceStack {
    fn resolve(&self, ch: char) -> usize {
        self.fonts
            .iter()
            .position(|font| font.covers(ch))
            .unwrap_or(0)
    }

    fn scale_of(&self, font: &dyn Font) -> f32 {
        let em = font.em_size();
        if em.is_finite() && em > 0.0 {
            self.size / em
        } else {
            1.0
        }
    }
}

impl Font for FaceStack {
    fn line_height(&self) -> f32 {
        let font = &self.fonts[0];
        font.line_height() * self.scale_of(font.as_ref())
    }

    fn advance(&self, ch: char) -> f32 {
        let font = &self.fonts[self.resolve(ch)];
        font.advance(ch) * self.scale_of(font.as_ref())
    }

    fn kerning(&self, left: char, right: char) -> f32 {
        let a = self.resolve(left);
        let b = self.resolve(right);
        if a != b {
            return 0.0;
        }
        let font = &self.fonts[a];
        font.kerning(left, right) * self.scale_of(font.as_ref())
    }

    fn glyph_box(&self, ch: char) -> Option<crate::font::GlyphBox> {
        let font = &self.fonts[self.resolve(ch)];
        let scale = self.scale_of(font.as_ref());
        font.glyph_box(ch).map(|glyph| crate::font::GlyphBox {
            w: glyph.w * scale,
            h: glyph.h * scale,
            bearing_x: glyph.bearing_x * scale,
            bearing_y: glyph.bearing_y * scale,
        })
    }

    fn covers(&self, ch: char) -> bool {
        self.fonts.iter().any(|font| font.covers(ch))
    }

    fn family_name(&self) -> &'static str {
        self.fonts[0].family_name()
    }

    fn em_size(&self) -> f32 {
        self.size
    }

    fn glyph_uv(&self, ch: char) -> [f32; 4] {
        self.fonts[self.resolve(ch)].glyph_uv(ch)
    }

    fn solid_uv(&self) -> [f32; 4] {
        self.fonts[0].solid_uv()
    }

    fn atlas(&self) -> Option<Arc<FontAtlas>> {
        self.fonts[0].atlas()
    }

    fn color_atlas(&self) -> Option<Arc<ColorAtlas>> {
        self.fonts.iter().find_map(|font| font.color_atlas())
    }
}
