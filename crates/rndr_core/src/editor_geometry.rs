//! Editor-only paragraph geometry. The assembled document contains caret rows,
//! never retained glyph quads. Cache eviction changes costs, not source bytes.
use crate::{TextEngine, TextLayout, TextSpan, TextStyle};
use std::collections::HashMap;
use std::sync::Arc;

pub(super) const CACHE_BYTES: usize = 8 * 1024 * 1024;

pub(super) struct Paragraph {
    text: String,
    spans: Vec<TextSpan>,
    layout: TextLayout,
}
impl Paragraph {
    fn bytes(&self) -> usize {
        self.text.capacity()
            + self.spans.capacity() * std::mem::size_of::<TextSpan>()
            + self
                .spans
                .iter()
                .filter_map(|s| s.family.as_ref())
                .map(String::capacity)
                .sum::<usize>()
            + geometry_bytes(&self.layout)
    }
}
pub(super) fn geometry_bytes(layout: &TextLayout) -> usize {
    layout.lines.capacity() * std::mem::size_of::<crate::TextLine>()
        + layout
            .lines
            .iter()
            .map(|l| l.edges.capacity() * std::mem::size_of::<(usize, f32)>())
            .sum::<usize>()
}
#[derive(Default)]
pub(super) struct Cache {
    entries: HashMap<u64, Vec<Arc<Paragraph>>>,
    pub bytes: usize,
}
#[derive(Default)]
pub(super) struct Work {
    pub paragraphs: u64,
    pub bytes: u64,
    pub reused: u64,
}

pub(super) fn build(
    engine: &dyn TextEngine,
    font: &dyn crate::Font,
    style: &TextStyle,
    width: f32,
    text: &str,
    spans: &[TextSpan],
    previous: Option<&Cache>,
) -> (TextLayout, Cache, Work) {
    build_bounded(
        engine,
        font,
        style,
        width,
        text,
        spans,
        previous,
        CACHE_BYTES,
    )
}
#[allow(clippy::too_many_arguments)]
fn build_bounded(
    engine: &dyn TextEngine,
    font: &dyn crate::Font,
    style: &TextStyle,
    width: f32,
    text: &str,
    spans: &[TextSpan],
    previous: Option<&Cache>,
    budget: usize,
) -> (TextLayout, Cache, Work) {
    use std::hash::{Hash, Hasher};
    let mut cache = Cache::default();
    let mut work = Work::default();
    let mut lines = Vec::new();
    let mut y = 0.0;
    let mut widest = 0.0f32;
    let mut base = 0;
    // Include the final empty paragraph, including an entirely empty document.
    let parts = text
        .split_inclusive('\n')
        .chain((text.is_empty() || text.ends_with('\n')).then_some(""));
    for part in parts {
        let end = base + part.len();
        let local: Vec<TextSpan> = spans
            .iter()
            .filter_map(|s| {
                let start = s.start.max(base);
                let stop = s.end.min(end);
                if start >= stop {
                    return None;
                }
                let mut s = s.clone();
                s.start = start - base;
                s.end = stop - base;
                Some(s)
            })
            .collect();
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        part.hash(&mut hash);
        // The font hash handles every metric-affecting and painted span field.
        let key = hash.finish() ^ crate::font::layout_cache_key(font, "", width, &local, "", 0);
        let found = previous
            .and_then(|c| c.entries.get(&key))
            .and_then(|items| items.iter().find(|p| p.text == part && p.spans == local))
            .or_else(|| {
                cache
                    .entries
                    .get(&key)
                    .and_then(|items| items.iter().find(|p| p.text == part && p.spans == local))
            })
            .cloned();
        let paragraph = if let Some(found) = found {
            work.reused += 1;
            found
        } else {
            work.paragraphs += 1;
            work.bytes += part.len() as u64;
            let mut layout = engine.layout(part, style, &local, width);
            layout.glyphs = Vec::new();
            layout.highlights = Vec::new();
            // A newline's terminal empty row belongs to the next paragraph.
            if part.ends_with('\n') && layout.lines.last().is_some_and(|l| l.start == part.len()) {
                layout.lines.pop();
                layout.height = layout.lines.last().map_or(0.0, |l| l.y + l.h);
            }
            Arc::new(Paragraph {
                text: part.to_owned(),
                spans: local,
                layout,
            })
        };
        if base > 0 {
            y += style.paragraph_spacing;
        }
        for line in &paragraph.layout.lines {
            let mut line = line.clone();
            line.start += base;
            line.end += base;
            line.y += y;
            for (at, _) in &mut line.edges {
                *at += base;
            }
            lines.push(line);
        }
        y += paragraph.layout.height;
        widest = widest.max(paragraph.layout.width);
        let already_retained = cache
            .entries
            .get(&key)
            .is_some_and(|items| items.iter().any(|p| Arc::ptr_eq(p, &paragraph)));
        if !already_retained {
            // Charge fixed per-entry storage too; never insert empty hash buckets
            // for rejected paragraphs after the budget has been exhausted.
            let bytes = paragraph.bytes()
                + std::mem::size_of::<Paragraph>()
                + std::mem::size_of::<(u64, Vec<Arc<Paragraph>>)>()
                + std::mem::size_of::<Arc<Paragraph>>();
            if bytes <= budget.saturating_sub(cache.bytes) {
                cache.bytes += bytes;
                cache.entries.entry(key).or_default().push(paragraph);
            }
        }
        base = end;
    }
    (
        TextLayout::shaped(Vec::new(), lines, widest, y),
        cache,
        work,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn paragraph_rows_match_whole_builtin_layout() {
        let world = crate::World::new();
        let engine = crate::text::text_engine(&world);
        let font = crate::font::text_face(&world);
        for text in [
            "",
            "\n",
            "a\n",
            "a\n\nβ\r\n",
            "é 👩‍💻 العربية\nend",
            "long long long line\nnext",
        ] {
            for width in [30.0, 200.0, f32::INFINITY] {
                for gap in [0.0, 5.0] {
                    let style = TextStyle {
                        paragraph_spacing: gap,
                        ..TextStyle::default()
                    };
                    let (got, _, _) = build(
                        engine.as_ref(),
                        font.as_ref(),
                        &style,
                        width,
                        text,
                        &[],
                        None,
                    );
                    let expected = engine.layout(text, &style, &[], width);
                    assert_eq!(
                        got.lines, expected.lines,
                        "{text:?} width {width} gap {gap}"
                    );
                    assert_eq!(got.height, expected.height);
                }
            }
        }
    }
    #[test]
    fn incremental_reuse_is_content_and_local_span_fenced() {
        let world = crate::World::new();
        let engine = crate::text::text_engine(&world);
        let font = crate::font::text_face(&world);
        let style = TextStyle::default();
        let text = "unchanged\noriginal\nlast";
        let (_, cache, _) = build(
            engine.as_ref(),
            font.as_ref(),
            &style,
            200.0,
            text,
            &[],
            None,
        );
        let (got, cache, work) = build(
            engine.as_ref(),
            font.as_ref(),
            &style,
            200.0,
            "unchanged\nchanged\nlast",
            &[],
            Some(&cache),
        );
        assert_eq!(work.paragraphs, 1);
        assert_eq!(work.bytes, 8);
        assert_eq!(work.reused, 2);
        let expected = engine.layout("unchanged\nchanged\nlast", &style, &[], 200.0);
        assert_eq!(got.lines, expected.lines);
        let spans = [TextSpan::new(10, 17, crate::Color::rgb(1.0, 1.0, 1.0)).weight(700)];
        let (_, _, work) = build(
            engine.as_ref(),
            font.as_ref(),
            &style,
            200.0,
            "unchanged\nchanged\nlast",
            &spans,
            Some(&cache),
        );
        assert_eq!(work.paragraphs, 1);
        assert_eq!(work.reused, 2);
        assert!(cache.bytes <= CACHE_BYTES);
    }
    #[test]
    fn huge_paragraph_cache_eviction_does_not_drop_rows() {
        let world = crate::World::new();
        let engine = crate::text::text_engine(&world);
        let font = crate::font::text_face(&world);
        let style = TextStyle::default();
        let text = "x".repeat(2_000);
        let (first, cache, _) = build_bounded(
            engine.as_ref(),
            font.as_ref(),
            &style,
            400.0,
            &text,
            &[],
            None,
            1024,
        );
        assert_eq!(cache.bytes, 0);
        assert!(cache.entries.is_empty());
        let distinct: String = (0..100).map(|i| format!("{i}\n")).collect();
        let (_, empty, _) = build_bounded(
            engine.as_ref(),
            font.as_ref(),
            &style,
            400.0,
            &distinct,
            &[],
            None,
            0,
        );
        assert!(empty.entries.is_empty());
        assert_eq!(empty.bytes, 0);
        let (again, _, _) = build_bounded(
            engine.as_ref(),
            font.as_ref(),
            &style,
            400.0,
            &text,
            &[],
            Some(&cache),
            1024,
        );
        assert_eq!(first.lines, again.lines);
        assert_eq!(first.lines.last().unwrap().end, text.len());
        assert!(first.glyphs.is_empty());
    }
}
