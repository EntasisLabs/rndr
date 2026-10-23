//! Parley layout turned into [`TextLayout`] glyphs. Rasterization goes through swash.

use parley::style::{FontFamily, StyleProperty};
use parley::{Affinity, Alignment, AlignmentOptions, Cursor, PositionedLayoutItem};
use rndr_core::{
    Color, FontStyle, PlacedGlyph, TextHighlight, TextLayout, TextLine, TextSpan, TextStyle,
};
use swash::scale::image::Content;
use swash::scale::{Render, ScaleContext, Source, StrikeWith};
use swash::zeno::{Format, Vector};
use swash::FontRef;

use super::atlas::{GlyphKey, Inserted, SUBPIXEL_BINS};
use super::State;

pub(super) struct Shaped {
    pub layout: TextLayout,
    pub keys: Vec<GlyphKey>,
}

pub(super) fn shape_text(
    state: &mut State,
    text: &str,
    style: &TextStyle,
    runs: &[TextSpan],
    max_width: f32,
) -> Shaped {
    for _ in 0..4 {
        let coverage_gen = state.coverage.generation();
        let color_gen = state.color.generation();
        state.coverage.begin_build();
        state.color.begin_build();
        let shaped = shape_once(state, text, style, runs, max_width);
        if state.coverage.generation() == coverage_gen && state.color.generation() == color_gen {
            return shaped;
        }
        // A grow dropped the shelf. The line cache's UVs are stale.
        state.invalidate_lines();
    }
    shape_once(state, text, style, runs, max_width)
}

fn shape_once(
    state: &mut State,
    text: &str,
    style: &TextStyle,
    runs: &[TextSpan],
    max_width: f32,
) -> Shaped {
    let scale = state.scale;
    let css = state.family_css(&style.family);
    let family_runs: Vec<(usize, usize, String)> = runs
        .iter()
        .filter_map(|run| {
            let end = run.end.min(text.len());
            let start = run.start.min(end);
            if start >= end {
                return None;
            }
            run.family
                .as_ref()
                .map(|family| (start, end, state.family_css(family)))
        })
        .collect();
    let mut builder = state
        .layout_cx
        .ranged_builder(&mut state.font_cx, text, scale, false);
    builder.push_default(StyleProperty::FontFamily(FontFamily::Source(
        std::borrow::Cow::Owned(css),
    )));
    builder.push_default(StyleProperty::FontSize(style.size));
    builder.push_default(StyleProperty::FontWeight(parley::FontWeight::new(
        style.weight as f32,
    )));
    builder.push_default(StyleProperty::FontStyle(match style.style {
        FontStyle::Italic => parley::FontStyle::Italic,
        FontStyle::Normal => parley::FontStyle::Normal,
    }));
    for run in runs {
        let end = run.end.min(text.len());
        let start = run.start.min(end);
        if start >= end {
            continue;
        }
        if let Some(weight) = run.weight {
            builder.push(
                StyleProperty::FontWeight(parley::FontWeight::new(weight as f32)),
                start..end,
            );
        }
        if run.italic {
            builder.push(
                StyleProperty::FontStyle(parley::FontStyle::Italic),
                start..end,
            );
        }
    }
    for (start, end, family) in family_runs {
        builder.push(
            StyleProperty::FontFamily(FontFamily::Source(std::borrow::Cow::Owned(family))),
            start..end,
        );
    }
    let mut parley_layout = builder.build(text);
    let max_px = if max_width.is_finite() && max_width > 0.0 {
        Some(max_width * scale)
    } else {
        None
    };
    parley_layout.break_all_lines(max_px);
    parley_layout.align(Alignment::Start, AlignmentOptions::default());

    let mut glyphs = Vec::new();
    let mut lines = Vec::new();
    let mut keys = Vec::new();
    for line in parley_layout.lines() {
        let metrics = line.metrics();
        let range = line.text_range();
        let mut indexes = vec![range.start, range.end];
        for item in line.items() {
            let PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                continue;
            };
            for cluster in glyph_run.run().visual_clusters() {
                let text_range = cluster.text_range();
                indexes.push(text_range.start);
                indexes.push(text_range.end);
            }
        }
        indexes.sort_unstable();
        indexes.dedup();
        let mut edges = Vec::with_capacity(indexes.len());
        let line_top = f64::from(metrics.block_min_coord);
        let line_bottom = line_top + f64::from(metrics.line_height.max(0.0));
        for index in indexes {
            // Downstream at a soft-wrap boundary sits at x=0 on the next
            // line. Upstream keeps that boundary on this line's end.
            // A hard break ignores affinity and still draws the following
            // byte on the next line; that stop is omitted here so a click
            // in this line's left padding cannot select the next line.
            let affinity = if index == range.end && index != range.start {
                Affinity::Upstream
            } else {
                Affinity::Downstream
            };
            let cursor = Cursor::from_byte_index(&parley_layout, index, affinity);
            let caret = cursor.geometry(&parley_layout, 1.0);
            let mid = (caret.y0 + caret.y1) * 0.5;
            if mid < line_top - 0.5 || mid >= line_bottom - 0.5 {
                continue;
            }
            edges.push((index, (caret.x0 as f32) / scale));
        }
        if edges.is_empty() {
            edges.push((range.start, 0.0));
        }
        for item in line.items() {
            let PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                continue;
            };
            let run = glyph_run.run();
            let font = run.font().clone();
            let px_size = run.font_size();
            let coords: Vec<i16> = run
                .normalized_coords()
                .iter()
                .map(|c| c.to_bits())
                .collect();
            let mut positioned = glyph_run.positioned_glyphs();
            for cluster in run.visual_clusters() {
                let text_range = cluster.text_range();
                let ch = text
                    .get(text_range.clone())
                    .and_then(|slice| slice.chars().next())
                    .unwrap_or('\u{FFFD}');
                let color = span_color(runs, text_range.start);
                let weight = span_weight(runs, text_range.start).unwrap_or(style.weight);
                let face = span_face(runs, text_range.start);
                for _glyph in cluster.glyphs() {
                    let Some(glyph) = positioned.next() else {
                        break;
                    };
                    if let Some(placed) = raster_glyph(
                        state,
                        GlyphPaint {
                            font: &font,
                            coords: &coords,
                            glyph_id: glyph.id,
                            px_size,
                            x_px: glyph.x,
                            baseline_px: glyph.y,
                            scale,
                            start: text_range.start,
                            end: text_range.end,
                            ch,
                            color,
                            weight,
                            face,
                        },
                        &mut keys,
                    ) {
                        glyphs.push(placed);
                    }
                }
            }
        }
        let width = metrics.advance / scale;
        lines.push(TextLine {
            start: range.start,
            end: range.end,
            y: metrics.block_min_coord / scale,
            h: metrics.line_height.max(0.0) / scale,
            width,
            edges,
        });
    }
    if lines.is_empty() {
        let h = (style.size * 1.2).max(1.0);
        lines.push(TextLine {
            start: 0,
            end: 0,
            y: 0.0,
            h,
            width: 0.0,
            edges: vec![(0, 0.0)],
        });
    }
    let width = parley_layout.width() / scale;
    let height = parley_layout
        .height()
        .max(lines.iter().map(|line| line.y + line.h).fold(0.0, f32::max));
    let mut layout = TextLayout::shaped(glyphs, lines, width, height);
    for span in runs {
        let Some(color) = span.highlight else {
            continue;
        };
        for selection in layout.selection_boxes(span.start, span.end) {
            layout.highlights.push(TextHighlight {
                x: selection.x,
                y: selection.y,
                w: selection.w,
                h: selection.h,
                color,
            });
        }
    }
    Shaped { layout, keys }
}

struct GlyphPaint<'a> {
    font: &'a parley::FontData,
    coords: &'a [i16],
    glyph_id: u32,
    px_size: f32,
    x_px: f32,
    baseline_px: f32,
    scale: f32,
    start: usize,
    end: usize,
    ch: char,
    color: Option<Color>,
    weight: u16,
    face: u8,
}

fn raster_glyph(
    state: &mut State,
    paint: GlyphPaint<'_>,
    keys: &mut Vec<GlyphKey>,
) -> Option<PlacedGlyph> {
    let GlyphPaint {
        font,
        coords,
        glyph_id,
        px_size,
        x_px,
        baseline_px,
        scale,
        start,
        end,
        ch,
        color,
        weight,
        face,
    } = paint;
    let paint_at = PlacedInk {
        x_px,
        baseline_px,
        scale,
        start,
        end,
        ch,
        color,
        weight,
        face,
    };
    let Ok(id) = u16::try_from(glyph_id) else {
        return None;
    };
    let bytes = font.data.as_ref();
    let frac = x_px - x_px.floor();
    let bin = ((frac * f32::from(SUBPIXEL_BINS)) as u8).min(SUBPIXEL_BINS - 1);
    let font_key = font_id(font);
    let size_bits = px_size.to_bits();
    // The line cache only keeps two frames. The pixels stay, so a hide/show or
    // a scroll back reuses the slot instead of running swash again.
    if let Some(hit) = state.coverage.resident(GlyphKey {
        font: font_key,
        glyph: glyph_id,
        size_bits,
        bin,
        color: false,
    }) {
        keys.push(hit.key);
        return Some(place_glyph(hit, &paint_at, false));
    }
    if let Some(hit) = state.color.resident(GlyphKey {
        font: font_key,
        glyph: glyph_id,
        size_bits,
        bin: 0,
        color: true,
    }) {
        keys.push(hit.key);
        return Some(place_glyph(hit, &paint_at, true));
    }
    state.rasters = state.rasters.wrapping_add(1);
    let sources = [
        Source::ColorBitmap(StrikeWith::BestFit),
        Source::ColorOutline(0),
        Source::Outline,
    ];
    // Probe at a zero offset so a color bitmap is recognized before a mask
    // is shifted into a subpixel bin.
    let probe = {
        let font_ref = FontRef::from_index(bytes, font.index as usize)?;
        let mut scaler = scaler_for(&mut state.scale_cx, font_ref, px_size, coords);
        Render::new(&sources)
            .format(Format::Alpha)
            .offset(Vector::new(0.0, 0.0))
            .render(&mut scaler, id)?
    };
    let color_glyph = probe.content == Content::Color;
    let bin = if color_glyph { 0 } else { bin };
    let offset = if color_glyph {
        0.0
    } else {
        f32::from(bin) / f32::from(SUBPIXEL_BINS)
    };
    let image = if offset == 0.0 {
        probe
    } else {
        let font_ref = FontRef::from_index(bytes, font.index as usize)?;
        let mut scaler = scaler_for(&mut state.scale_cx, font_ref, px_size, coords);
        Render::new(&sources)
            .format(Format::Alpha)
            .offset(Vector::new(offset, 0.0))
            .render(&mut scaler, id)?
    };
    let width = image.placement.width;
    let height = image.placement.height;
    if width == 0 || height == 0 {
        return None;
    }
    let key = GlyphKey {
        font: font_key,
        glyph: glyph_id,
        size_bits,
        bin,
        color: color_glyph,
    };
    let atlas = if color_glyph {
        &mut state.color
    } else {
        &mut state.coverage
    };
    let ink = match atlas.insert(
        key,
        width,
        height,
        image.placement.left,
        image.placement.top,
        &image.data,
    ) {
        Inserted::Ready(ink) => ink,
        Inserted::Grew | Inserted::Skip => return None,
    };
    keys.push(ink.key);
    Some(place_glyph(ink, &paint_at, color_glyph))
}

struct PlacedInk {
    x_px: f32,
    baseline_px: f32,
    scale: f32,
    start: usize,
    end: usize,
    ch: char,
    color: Option<Color>,
    weight: u16,
    face: u8,
}

fn place_glyph(ink: super::atlas::Ink, at: &PlacedInk, color_glyph: bool) -> PlacedGlyph {
    let scale = if at.scale.is_finite() && at.scale > 0.0 {
        at.scale
    } else {
        1.0
    };
    let pen_x = at.x_px.floor() + ink.left as f32;
    let pen_y = at.baseline_px.round() - ink.top as f32;
    PlacedGlyph {
        start: at.start,
        end: at.end,
        ch: at.ch,
        x: pen_x / scale,
        y: pen_y / scale,
        w: ink.width as f32 / scale,
        h: ink.height as f32 / scale,
        uv: ink.uv,
        color: at.color,
        face: at.face,
        weight: at.weight,
        atlas: u8::from(color_glyph),
    }
}

fn scaler_for<'a>(
    scale_cx: &'a mut ScaleContext,
    font: FontRef<'a>,
    px_size: f32,
    coords: &[i16],
) -> swash::scale::Scaler<'a> {
    let mut builder = scale_cx.builder(font).size(px_size.max(0.0)).hint(false);
    if !coords.is_empty() {
        builder = builder.normalized_coords(coords);
    }
    builder.build()
}

fn font_id(font: &parley::FontData) -> u64 {
    let ptr = font.data.as_ref().as_ptr() as u64;
    ptr ^ (u64::from(font.index) << 48)
}

fn span_color(spans: &[TextSpan], index: usize) -> Option<Color> {
    spans
        .iter()
        .find(|span| index >= span.start && index < span.end)
        .map(|span| span.color)
}

fn span_face(spans: &[TextSpan], index: usize) -> u8 {
    spans
        .iter()
        .find(|span| index >= span.start && index < span.end)
        .map(|span| span.face)
        .unwrap_or(0)
}

fn span_weight(spans: &[TextSpan], index: usize) -> Option<u16> {
    spans
        .iter()
        .find(|span| index >= span.start && index < span.end)
        .and_then(|span| span.weight)
}

impl State {
    pub(super) fn family_css(&self, family: &str) -> String {
        let primary = if family.is_empty() || family == "bitmap" {
            "Inter"
        } else {
            family
        };
        let mut names: Vec<&str> = Vec::new();
        names.push(primary);
        if let Some(list) = self
            .fallbacks
            .get(primary)
            .or_else(|| self.fallbacks.get(family))
        {
            for name in list {
                if name != "bitmap" && !names.contains(&name.as_str()) {
                    names.push(name);
                }
            }
        }
        for name in DEFAULT_FALLBACKS {
            if !names.contains(name) {
                names.push(name);
            }
        }
        let mut css = names.join(", ");
        css.push_str(", sans-serif, emoji");
        css
    }
}

const DEFAULT_FALLBACKS: &[&str] = &[
    "Inter",
    "Noto Sans Arabic",
    "Noto Sans CJK SC",
    "Noto Sans JP",
    "Noto Sans SC",
    "Noto Color Emoji",
];
