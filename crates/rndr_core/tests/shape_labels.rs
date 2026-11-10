//! `shape_labels` must not rebuild a layout it is about to discard.
//!
//! [`BuiltinText::shape_count`] counts line-cache misses. A hit still allocates
//! a fresh [`TextLayout`]. Markdown and structured views paid that rebuild on
//! every idle frame, then threw it away when the entity key matched. This
//! engine counts `layout` and `measure` calls so the regression sees the
//! rebuild, not only the miss.

use std::any::Any;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use rndr_core::structured::StructuredDocument;
use rndr_core::structured_chrome::StructuredChromeGutter;
use rndr_core::structured_native::{attach_structured, StructuredDocuments};
use rndr_core::{
    markdown_view, App, Buffer, BuiltinText, CapturesKeys, Children, Chrome, ColorAtlas, Editor,
    Focus, Focusable, Font, FontAtlas, Input, Label, Layout, MarkdownPlugin, Scroll, Size,
    TextEngine, TextLayout, TextMeasure, TextSpan, TextStyle, Viewport,
};

struct CountingEngine {
    inner: BuiltinText,
    calls: AtomicU64,
    epoch: AtomicU64,
}

impl CountingEngine {
    fn new() -> Self {
        Self {
            inner: BuiltinText::new(),
            calls: AtomicU64::new(0),
            epoch: AtomicU64::new(0),
        }
    }

    fn calls(&self) -> u64 {
        self.calls.load(Ordering::Relaxed)
    }

    fn bump_epoch(&self) {
        self.epoch.fetch_add(1, Ordering::Relaxed);
    }
}

impl TextEngine for CountingEngine {
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
        self.calls.fetch_add(1, Ordering::Relaxed);
        self.inner.layout(text, style, runs, max_width)
    }

    fn measure(
        &self,
        text: &str,
        style: &TextStyle,
        runs: &[TextSpan],
        max_width: f32,
    ) -> TextMeasure {
        self.calls.fetch_add(1, Ordering::Relaxed);
        self.inner.measure(text, style, runs, max_width)
    }

    fn line_height(&self, style: &TextStyle) -> f32 {
        self.inner.line_height(style)
    }

    fn unit_advance(&self, style: &TextStyle) -> f32 {
        self.inner.unit_advance(style)
    }

    fn solid_uv(&self, style: &TextStyle) -> [f32; 4] {
        self.inner.solid_uv(style)
    }

    fn coverage_atlas(&self) -> Option<Arc<FontAtlas>> {
        self.inner.coverage_atlas()
    }

    fn color_atlas(&self) -> Option<Arc<ColorAtlas>> {
        self.inner.color_atlas()
    }

    fn fallbacks(&self, family: &str) -> Vec<String> {
        self.inner.fallbacks(family)
    }

    fn set_fallbacks(&self, family: &str, faces: &[String]) {
        self.inner.set_fallbacks(family, faces);
    }

    fn register_font(&self, name: &str, font: Arc<dyn Font>) {
        self.inner.register_font(name, font);
    }

    fn begin_frame(&self) {
        self.inner.begin_frame();
    }

    fn touch_cached(&self, text: &str, style: &TextStyle, runs: &[TextSpan], max_width: f32) {
        self.inner.touch_cached(text, style, runs, max_width);
    }

    fn touch_lines(&self, ids: &[u64]) {
        self.inner.touch_lines(ids);
    }

    fn shape_count(&self) -> u64 {
        self.calls()
    }

    fn cache_epoch(&self) -> u64 {
        self.inner
            .cache_epoch()
            .wrapping_add(self.epoch.load(Ordering::Relaxed))
    }

    fn set_pixels_per_point(&self, scale: f32) {
        self.inner.set_pixels_per_point(scale);
    }
}

fn harness() -> (App, Arc<CountingEngine>) {
    let mut app = App::new();
    let engine = Arc::new(CountingEngine::new());
    let shared: Arc<dyn TextEngine> = Arc::clone(&engine) as Arc<dyn TextEngine>;
    app.world_mut().insert_resource(shared);
    (app, engine)
}

fn during(engine: &CountingEngine, frame: impl FnOnce()) -> u64 {
    let before = engine.calls();
    frame();
    engine.calls() - before
}

/// Paragraphs differ, so a line-cache hit on repeated text cannot hide a
/// per-block rebuild.
fn distinct_lines(count: usize, separator: &str) -> String {
    (0..count)
        .map(|index| format!("Paragraph {index} alpha beta gamma delta epsilon zeta {index}."))
        .collect::<Vec<_>>()
        .join(separator)
}

fn warm(app: &mut App, engine: &CountingEngine) -> u64 {
    app.update();
    app.update();
    app.update();
    let settled = engine.calls();
    let idle = during(engine, || app.update());
    assert_eq!(idle, 0, "idle frame called the shaper {idle} times");
    settled
}

#[test]
fn structured_idle_does_not_reshape_and_ime_width_and_epoch_still_do() {
    let (mut app, engine) = harness();
    app.add_plugin(Chrome);
    app.world_mut()
        .insert_resource(Viewport::new(640.0, 1400.0));
    let source = distinct_lines(24, "\n");
    let mut docs = StructuredDocuments::default();
    let handle = docs.insert(StructuredDocument::new(&source));
    app.world_mut().insert_resource(docs);
    let view = app.world_mut().spawn((
        Layout {
            width: Size::Px(480.0),
            height: Size::Px(1200.0),
            ..Layout::column()
        },
        Buffer::from_str(""),
        Editor::default(),
        Label::new(""),
        StructuredChromeGutter { width: 24.0 },
        Focusable,
    ));
    app.world_mut().insert(view, CapturesKeys);
    attach_structured(app.world_mut(), view, handle).unwrap();
    app.world_mut()
        .insert_resource(Focus { entity: Some(view) });

    let cold = warm(&mut app, &engine);
    assert!(
        cold >= 24,
        "structured text did not shape each paragraph on the cold path ({cold})"
    );
    let painted = app.world().get::<TextLayout>(view).unwrap().clone();
    assert!(painted.lines.len() >= 24);
    let idle_paint = during(&engine, || app.update());
    assert_eq!(idle_paint, 0);
    assert_eq!(app.world().get::<TextLayout>(view).unwrap(), &painted);

    let preview = during(&engine, || {
        app.world_mut().resource_mut::<Input>().unwrap().ime_preedit = "QQ".into();
        app.update();
    });
    assert!(preview > 0, "IME preview did not reshape");
    let composed = app.world().get::<TextLayout>(view).unwrap().clone();
    assert!(composed.glyphs.iter().any(|glyph| glyph.ch == 'Q'));
    assert!(composed.ime_range.is_some());
    let held = during(&engine, || app.update());
    assert_eq!(held, 0, "a stable IME preview reshaped");
    assert_eq!(app.world().get::<TextLayout>(view).unwrap(), &composed);

    let cleared = during(&engine, || {
        app.world_mut()
            .resource_mut::<Input>()
            .unwrap()
            .ime_preedit
            .clear();
        app.update();
    });
    assert!(cleared > 0, "clearing the IME preview did not reshape");
    assert!(app
        .world()
        .get::<TextLayout>(view)
        .unwrap()
        .glyphs
        .iter()
        .all(|glyph| glyph.ch != 'Q'));

    let before_lines = app.world().get::<TextLayout>(view).unwrap().lines.len();
    let narrowed = during(&engine, || {
        app.world_mut().get_mut::<Layout>(view).unwrap().width = Size::Px(96.0);
        app.update();
        app.update();
    });
    assert!(narrowed > 0, "a width change did not reshape");
    assert!(
        app.world().get::<TextLayout>(view).unwrap().lines.len() > before_lines,
        "narrower width did not wrap additional lines"
    );
    let settled = during(&engine, || app.update());
    assert_eq!(settled, 0, "idle frame after a width change reshaped");

    let epoch = during(&engine, || {
        engine.bump_epoch();
        app.update();
    });
    assert!(epoch > 0, "an engine epoch change did not reshape");

    let replacement = Arc::new(CountingEngine::new());
    let shared: Arc<dyn TextEngine> = Arc::clone(&replacement) as Arc<dyn TextEngine>;
    app.world_mut().insert_resource(shared);
    app.update();
    assert!(
        replacement.calls() > 0,
        "swapping the engine kept the previous layout"
    );
}

#[test]
fn markdown_idle_and_scroll_do_not_reshape_distinct_blocks() {
    let (mut app, engine) = harness();
    app.add_plugin(MarkdownPlugin);
    app.world_mut().insert_resource(Viewport::new(960.0, 420.0));
    let view = markdown_view(app.world_mut(), distinct_lines(32, "\n\n"));
    let cold = warm(&mut app, &engine);
    let labels = app.world().entities_with::<Label>().len();
    assert!(labels >= 32, "expected one label per block, got {labels}");
    assert!(
        cold >= labels as u64,
        "cold start shaped {cold} times for {labels} labels"
    );
    let sample = block_layout(&app, view);
    let idle = during(&engine, || app.update());
    assert_eq!(idle, 0, "markdown idle frame reshaped");
    assert_eq!(block_layout(&app, view), sample);

    let before_offset = app.world().get::<Scroll>(view).unwrap().offset[1];
    let scrolled = during(&engine, || {
        {
            let input = app.world_mut().resource_mut::<Input>().unwrap();
            input.pointer.position = [40.0, 40.0];
            input.scroll = [0.0, 480.0];
        }
        app.update();
    });
    let after_offset = app.world().get::<Scroll>(view).unwrap().offset[1];
    assert!(
        (after_offset - before_offset).abs() > 1.0,
        "wheel did not move the markdown scroll offset ({before_offset} -> {after_offset})"
    );
    assert_eq!(scrolled, 0, "markdown scroll reshaped unchanged blocks");
    assert_eq!(block_layout(&app, view), sample);
    let after_scroll = during(&engine, || app.update());
    assert_eq!(after_scroll, 0, "idle frame after a scroll reshaped");

    app.world_mut()
        .get_mut::<rndr_core::MarkdownView>(view)
        .unwrap()
        .push_str("\n\nTail paragraph omega 999 unique.\n");
    app.update();
    let after_append = during(&engine, || app.update());
    assert_eq!(after_append, 0, "idle frame after an append reshaped");
}

fn block_layout(app: &App, view: rndr_core::Entity) -> TextLayout {
    let child = app.world().get::<Children>(view).unwrap().list[0];
    app.world().get::<TextLayout>(child).unwrap().clone()
}

/// Release characterization. Paragraphs are unique so the line cache cannot
/// collapse the document. No timing threshold: record the printed row.
/// `shape_only_p50_us` times `apply_text_layout` alone, after the scroll
/// samples, so it is the key check and not a full frame.
///
/// ```text
/// cargo test -p rndr_core --release --test shape_labels -- --ignored --nocapture markdown_idle_shape_cost
/// ```
#[test]
#[ignore = "opt-in release characterization of markdown idle shaping; no timing threshold"]
fn markdown_idle_shape_cost() {
    println!(
        "blocks,labels,cold_us,idle_p50_us,idle_p95_us,scroll_p50_us,scroll_p95_us,idle_calls,scroll_calls,shape_only_p50_us"
    );
    for blocks in [500, 2_000, 8_000] {
        let (mut app, engine) = harness();
        app.add_plugin(MarkdownPlugin);
        app.world_mut().insert_resource(Viewport::new(960.0, 640.0));
        let _view = markdown_view(app.world_mut(), distinct_lines(blocks, "\n\n"));
        let cold_at = Instant::now();
        app.update();
        let cold = cold_at.elapsed().as_micros();
        app.update();
        app.update();
        let labels = app.world().entities_with::<Label>().len();
        let mut idle_us = Vec::new();
        let mut idle_calls = Vec::new();
        for _ in 0..21 {
            let before = engine.calls();
            let start = Instant::now();
            app.update();
            idle_us.push(start.elapsed().as_micros());
            idle_calls.push(engine.calls() - before);
        }
        let mut scroll_us = Vec::new();
        let mut scroll_calls = Vec::new();
        for _ in 0..21 {
            let before = engine.calls();
            {
                let input = app.world_mut().resource_mut::<Input>().unwrap();
                input.pointer.position = [48.0, 48.0];
                input.scroll = [0.0, 64.0];
            }
            let start = Instant::now();
            app.update();
            scroll_us.push(start.elapsed().as_micros());
            scroll_calls.push(engine.calls() - before);
        }
        idle_us.sort_unstable();
        scroll_us.sort_unstable();
        let mut shape_only = Vec::new();
        for _ in 0..21 {
            let start = Instant::now();
            rndr_core::apply_text_layout(app.world_mut());
            shape_only.push(start.elapsed().as_micros());
        }
        shape_only.sort_unstable();
        let idle_call = median_calls(&idle_calls);
        let scroll_call = median_calls(&scroll_calls);
        println!(
            "{blocks},{labels},{cold},{},{},{},{},{idle_call},{scroll_call},{}",
            idle_us[10], idle_us[19], scroll_us[10], scroll_us[19], shape_only[10]
        );
    }
}

fn median_calls(samples: &[u64]) -> u64 {
    let mut ordered = samples.to_vec();
    ordered.sort_unstable();
    ordered[ordered.len() / 2]
}
