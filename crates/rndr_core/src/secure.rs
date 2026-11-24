//! Masked text: a [`crate::TextField`] whose value is shown as bullets.
//!
//! The [`crate::Label`] keeps the real value, so editing, the caret, undo
//! and the host's reads are unchanged. Shaping lays out one bullet per
//! character and maps the layout's byte offsets back onto the real text, so
//! hit testing and selection land on the real characters. Copy, cut and the
//! primary selection never carry the value, and accessibility reads bullets.
use crate::font::TextLayout;
use crate::world::Component;

/// Shows this field's text as bullets. Put it next to a
/// [`crate::TextField`].
#[derive(Clone, Copy, Debug, Default)]
pub struct Secure;

impl Component for Secure {}

/// The character every scalar is shown as.
pub const MASK: char = '•';

/// One `•` per character of `text`.
pub fn mask(text: &str) -> String {
    text.chars().map(|_| MASK).collect()
}

/// Maps a layout shaped from [`mask`]`(real)` back onto `real`'s byte offsets.
pub(crate) fn unmask(real: &str, layout: &mut TextLayout) {
    let offsets: Vec<usize> = real
        .char_indices()
        .map(|(i, _)| i)
        .chain(std::iter::once(real.len()))
        .collect();
    let last = offsets.len() - 1;
    let map = |byte: usize| offsets[(byte / MASK.len_utf8()).min(last)];
    for line in &mut layout.lines {
        line.start = map(line.start);
        line.end = map(line.end);
        for edge in &mut line.edges {
            edge.0 = map(edge.0);
        }
    }
    layout.ime_cursor = layout.ime_cursor.map(map);
    layout.ime_range = layout.ime_range.map(|(a, b)| (map(a), map(b)));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::font::TextLine;

    #[test]
    fn offsets_map_back_onto_the_real_characters() {
        let real = "aé€😀";
        assert_eq!(mask(real), "••••");
        let shaped = mask(real);
        let edges = shaped
            .char_indices()
            .map(|(i, _)| i)
            .chain([shaped.len()])
            .enumerate()
            .map(|(n, i)| (i, n as f32))
            .collect();
        let mut layout = TextLayout::shaped(
            Vec::new(),
            vec![TextLine {
                start: 0,
                end: shaped.len(),
                y: 0.,
                h: 10.,
                baseline: 8.,
                underline_thickness: 1.,
                underline_offset: 1.,
                width: 4.,
                edges,
            }],
            4.,
            10.,
        );
        layout.ime_range = Some((3, 9));
        unmask(real, &mut layout);
        let line = &layout.lines[0];
        assert_eq!((line.start, line.end), (0, real.len()));
        let bytes: Vec<usize> = line.edges.iter().map(|e| e.0).collect();
        assert_eq!(bytes, [0, 1, 3, 6, 10]);
        assert_eq!(layout.ime_range, Some((1, 6)));
    }
}
