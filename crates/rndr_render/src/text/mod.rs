//! Parley shaping and swash rasterization behind [`TextEngine`].
//!
//! Lines are cached for this frame and the previous one. A cache hit does not
//! shape again. Glyphs live in a coverage atlas or a color atlas until a new
//! glyph does not fit. Forgetting a line does not free its pixels, and a later
//! shape reuses them instead of rasterizing again. Only dirty rectangles are
//! uploaded.

mod atlas;
mod fonts;
mod shape;

#[cfg(test)]
mod tests;

use std::any::Any;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use parley::fontique;
use parley::{FontContext, LayoutContext};
use rndr_core::{
    Color, ColorAtlas, Font, FontAtlas, FontStyle, TextEngine, TextLayout, TextMeasure, TextSpan,
    TextStyle,
};
use swash::scale::ScaleContext;

use atlas::{Atlas, GlyphKey};
use shape::{shape_text, Shaped};

/// How the engine loads fonts and how large the glyph atlases may grow.
pub struct TextEngineConfig {
    /// Discover fonts installed on the machine. Tests that must not depend on
    /// the host set this to false and use the bundled faces.
    pub system_fonts: bool,
    /// Starting shelf size, in pixels. Grows by doubling.
    pub atlas_size: u32,
    /// Largest shelf. Past this, a glyph that does not fit evicts the least
    /// recently used one that no cached line still needs.
    pub max_atlas: u32,
}

impl Default for TextEngineConfig {
    fn default() -> Self {
        Self {
            system_fonts: true,
            atlas_size: 512,
            max_atlas: 4096,
        }
    }
}

/// One shelf upload. `full` means the texture was recreated and `pixels` is the
/// whole image. Otherwise `rects` are the regions written since the last take.
pub struct AtlasUpload {
    pub coverage: AtlasLayer,
    pub color: AtlasLayer,
}

pub struct AtlasLayer {
    pub width: u32,
    pub height: u32,
    /// 1 for coverage, 4 for straight RGBA.
    pub bpp: usize,
    pub full: bool,
    pub pixels: Vec<u8>,
    pub rects: Vec<AtlasRegion>,
}

pub struct AtlasRegion {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub bytes: Vec<u8>,
}

/// Production text engine. System fonts plus the bundled Inter, Noto Sans
/// Arabic, and Noto Color Emoji faces.
pub struct ParleyText {
    state: Mutex<State>,
    shapes: AtomicU64,
}

pub(super) struct State {
    font_cx: FontContext,
    layout_cx: LayoutContext<()>,
    scale_cx: ScaleContext,
    coverage: Atlas,
    color: Atlas,
    cache: FrameCache,
    fallbacks: HashMap<String, Vec<String>>,
    scale: f32,
    epoch: u64,
    rasters: u64,
}

struct FrameCache {
    frames: [HashMap<CacheKey, Arc<CachedLine>>; 2],
    current: usize,
}

struct CachedLine {
    layout: TextLayout,
    keys: Vec<GlyphKey>,
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct CacheKey {
    text: String,
    family: String,
    size_bits: u32,
    weight: u16,
    italic: bool,
    width_bits: u32,
    scale_bits: u32,
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

impl ParleyText {
    /// System fonts, with the bundled faces registered first.
    pub fn new() -> Self {
        Self::with_config(TextEngineConfig::default())
    }

    /// Bundled faces only. Snapshots and shaping tests use this so the host's
    /// font set cannot change the pixels.
    pub fn bundled() -> Self {
        Self::with_config(TextEngineConfig {
            system_fonts: false,
            ..TextEngineConfig::default()
        })
    }

    pub fn with_config(config: TextEngineConfig) -> Self {
        let mut font_cx = FontContext {
            collection: fontique::Collection::new(fontique::CollectionOptions {
                shared: false,
                system_fonts: config.system_fonts,
            }),
            source_cache: fontique::SourceCache::default(),
        };
        register_bundled(&mut font_cx.collection);
        let size = config.atlas_size.max(32);
        let max = config.max_atlas.max(size);
        Self {
            state: Mutex::new(State {
                font_cx,
                layout_cx: LayoutContext::new(),
                scale_cx: ScaleContext::new(),
                coverage: Atlas::new(false, size, max),
                color: Atlas::new(true, size, max),
                cache: FrameCache {
                    frames: [HashMap::new(), HashMap::new()],
                    current: 0,
                },
                fallbacks: HashMap::new(),
                scale: 1.0,
                epoch: 0,
                rasters: 0,
            }),
            shapes: AtomicU64::new(0),
        }
    }

    /// Register a font collection from bytes. The family name is read from the file.
    pub fn register_bytes(&self, bytes: &[u8]) {
        let mut state = self.lock();
        let blob = fontique::Blob::new(Arc::new(bytes.to_vec()));
        state.font_cx.collection.register_fonts(blob, None);
        state.invalidate_lines();
    }

    /// Swash rasters. An atlas hit does not increment this.
    pub fn raster_count(&self) -> u64 {
        self.lock().rasters
    }

    /// Glyphs dropped because the shelf was full.
    pub fn eviction_count(&self) -> u64 {
        let state = self.lock();
        state.coverage.evictions + state.color.evictions
    }

    /// Dirty atlas regions since the last call. The GPU uploads these and nothing else.
    pub fn take_uploads(&self) -> AtlasUpload {
        let mut state = self.lock();
        AtlasUpload {
            coverage: layer_from(state.coverage.take_dirty()),
            color: layer_from(state.color.take_dirty()),
        }
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().expect("text engine")
    }
}

impl Default for ParleyText {
    fn default() -> Self {
        Self::new()
    }
}

fn register_bundled(collection: &mut fontique::Collection) {
    let mut emoji = None;
    let mut sans = None;
    for bytes in fonts::BUNDLED {
        let blob = fontique::Blob::new(Arc::new(bytes.to_vec()));
        for (id, _) in collection.register_fonts(blob, None) {
            let Some(family) = collection.family(id) else {
                continue;
            };
            match family.name() {
                "Noto Color Emoji" => emoji = Some(id),
                "Inter" => sans = Some(id),
                _ => {}
            }
        }
    }
    // Parley asks for the generic emoji family on emoji clusters, and for
    // sans-serif after the named list. Point those at the bundled faces so a
    // host with no fonts still resolves them.
    if let Some(id) = emoji {
        collection.append_generic_families(fontique::GenericFamily::Emoji, std::iter::once(id));
    }
    if let Some(id) = sans {
        collection.append_generic_families(fontique::GenericFamily::SansSerif, std::iter::once(id));
    }
}

fn layer_from(delta: atlas::AtlasDelta) -> AtlasLayer {
    AtlasLayer {
        width: delta.width,
        height: delta.height,
        bpp: delta.bpp,
        full: delta.full,
        pixels: delta.pixels,
        rects: delta
            .rects
            .into_iter()
            .map(|rect| AtlasRegion {
                x: rect.x,
                y: rect.y,
                width: rect.width,
                height: rect.height,
                bytes: rect.bytes,
            })
            .collect(),
    }
}

impl State {
    fn invalidate_lines(&mut self) {
        let mut dropped = Vec::new();
        for frame in &mut self.cache.frames {
            dropped.extend(std::mem::take(frame).into_values());
        }
        for entry in dropped {
            self.release(&entry.keys);
        }
        self.epoch = self.epoch.wrapping_add(1);
    }

    fn release(&mut self, keys: &[GlyphKey]) {
        self.coverage.release(keys);
        self.color.release(keys);
    }

    fn remember(&mut self, keys: &[GlyphKey]) {
        self.coverage.add_refs(keys);
        self.color.add_refs(keys);
    }

    fn touch(&mut self, keys: &[GlyphKey]) {
        self.coverage.touch(keys);
        self.color.touch(keys);
    }
}

impl TextEngine for ParleyText {
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
        let key = cache_key(text, style, runs, max_width, self.lock().scale);
        {
            let mut state = self.lock();
            let current = state.cache.current;
            if let Some(hit) = state.cache.frames[current].get(&key).cloned() {
                state.touch(&hit.keys);
                return hit.layout.clone();
            }
            let previous = current ^ 1;
            if let Some(hit) = state.cache.frames[previous].get(&key).cloned() {
                state.touch(&hit.keys);
                state.remember(&hit.keys);
                state.cache.frames[current].insert(key, Arc::clone(&hit));
                return hit.layout.clone();
            }
        }
        let shaped = {
            let mut state = self.lock();
            shape_text(&mut state, text, style, runs, max_width)
        };
        // A grow inside shape_text may have cleared the shelf. Re-read the key's
        // scale and store the line that matches the atlases just written.
        let mut state = self.lock();
        let key = cache_key(text, style, runs, max_width, state.scale);
        if let Some(hit) = state.cache.frames[state.cache.current].get(&key).cloned() {
            state.touch(&hit.keys);
            return hit.layout.clone();
        }
        state.remember(&shaped.keys);
        let layout = shaped.layout.clone();
        let current = state.cache.current;
        state.cache.frames[current].insert(key, Arc::new(cached(shaped)));
        drop(state);
        self.shapes.fetch_add(1, Ordering::Relaxed);
        layout
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
        let layout = self.layout(" ", style, &[], f32::INFINITY);
        layout
            .lines
            .first()
            .map(|line| line.h)
            .filter(|h| h.is_finite() && *h > 0.0)
            .unwrap_or(style.size.max(1.0))
    }

    fn unit_advance(&self, style: &TextStyle) -> f32 {
        let layout = self.layout("0", style, &[], f32::INFINITY);
        if layout.width.is_finite() && layout.width > 0.0 {
            layout.width
        } else {
            (style.size * 0.5).max(1.0)
        }
    }

    fn solid_uv(&self, _style: &TextStyle) -> [f32; 4] {
        self.lock().coverage.solid_uv()
    }

    fn coverage_atlas(&self) -> Option<Arc<FontAtlas>> {
        let (width, height, pixels) = self.lock().coverage.snapshot();
        Some(Arc::new(FontAtlas {
            width,
            height,
            r8: pixels.into(),
        }))
    }

    fn color_atlas(&self) -> Option<Arc<ColorAtlas>> {
        let (width, height, pixels) = self.lock().color.snapshot();
        Some(Arc::new(ColorAtlas {
            width,
            height,
            rgba: pixels.into(),
        }))
    }

    fn fallbacks(&self, family: &str) -> Vec<String> {
        let state = self.lock();
        if let Some(list) = state.fallbacks.get(family) {
            return list.clone();
        }
        let mut names: Vec<String> = Vec::new();
        let primary = if family.is_empty() || family == "bitmap" {
            "Inter".to_string()
        } else {
            family.to_string()
        };
        names.push(primary);
        for name in ["Noto Sans Arabic", "Noto Sans CJK SC", "Noto Color Emoji"] {
            if !names.iter().any(|have| have == name) {
                names.push(name.to_string());
            }
        }
        names
    }

    fn set_fallbacks(&self, family: &str, faces: &[String]) {
        let mut state = self.lock();
        state.fallbacks.insert(family.to_string(), faces.to_vec());
        state.invalidate_lines();
    }

    fn register_font(&self, name: &str, _font: Arc<dyn Font>) {
        // The trait hands over a shaped face, not font bytes. Callers that have
        // the file use [`ParleyText::register_bytes`]. The name still joins the
        // fallback list so a later byte registration is requested by family.
        let mut state = self.lock();
        state
            .fallbacks
            .entry(name.to_string())
            .or_insert_with(|| vec![name.to_string()]);
    }

    fn begin_frame(&self) {
        let mut state = self.lock();
        let next = state.cache.current ^ 1;
        let dropped = std::mem::take(&mut state.cache.frames[next]);
        for (_, entry) in dropped {
            state.release(&entry.keys);
        }
        state.cache.current = next;
    }

    fn shape_count(&self) -> u64 {
        self.shapes.load(Ordering::Relaxed)
    }

    fn cache_epoch(&self) -> u64 {
        self.lock().epoch
    }

    fn set_pixels_per_point(&self, scale: f32) {
        let scale = if scale.is_finite() && scale > 0.0 {
            scale
        } else {
            1.0
        };
        self.lock().scale = scale;
    }
}

fn cached(shaped: Shaped) -> CachedLine {
    CachedLine {
        layout: shaped.layout,
        keys: shaped.keys,
    }
}

fn cache_key(
    text: &str,
    style: &TextStyle,
    runs: &[TextSpan],
    max_width: f32,
    scale: f32,
) -> CacheKey {
    CacheKey {
        text: text.to_string(),
        family: style.family.clone(),
        size_bits: style.size.to_bits(),
        weight: style.weight,
        italic: style.style == FontStyle::Italic,
        width_bits: max_width.to_bits(),
        scale_bits: scale.to_bits(),
        runs: runs
            .iter()
            .map(|run| RunKey {
                start: run.start as u32,
                end: run.end as u32,
                rgba: color_bits(run.color),
                weight: run.weight,
                face: run.face,
                highlight: run.highlight.map(color_bits),
            })
            .collect(),
    }
}

fn color_bits(color: Color) -> [u32; 4] {
    [
        color.r.to_bits(),
        color.g.to_bits(),
        color.b.to_bits(),
        color.a.to_bits(),
    ]
}
