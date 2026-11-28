//! Host-owned link destinations from the concrete source tree.

use rndr_markdown::{Green, InlineMark, TypedView};
use std::ops::Range;

pub(crate) fn destinations<'a>(bytes: &'a str, tree: &Green) -> Vec<(Range<usize>, &'a str)> {
    TypedView::new(bytes, tree)
        .marks()
        .into_iter()
        .filter_map(|mark| match mark {
            InlineMark::Link {
                range, destination, ..
            } => Some((range, destination)),
            InlineMark::WikiLink { range, target, .. } | InlineMark::Autolink { range, target } => {
                Some((range, target))
            }
            _ => None,
        })
        .filter(|(_, href)| !href.is_empty())
        .collect()
}

pub(crate) fn source(block: &crate::project::DisplayBlock, index: usize) -> Option<usize> {
    let display = index.checked_sub(block.chrome.len())?;
    let segment = block
        .segments
        .iter()
        .find(|seg| seg.display_start <= display && display < seg.display_end)?;
    let offset = if segment.display_end - segment.display_start
        == segment.source_end - segment.source_start
    {
        display - segment.display_start
    } else {
        0
    };
    Some(block.span.start + segment.source_start + offset)
}

pub(crate) fn hit_index(layout: &rndr_core::TextLayout, x: f32, y: f32) -> Option<usize> {
    if !x.is_finite() || !y.is_finite() {
        return None;
    }
    for line in &layout.lines {
        if y < line.y || y >= line.y + line.h {
            continue;
        }
        let mut edges = line.edges.clone();
        edges.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)));
        for pair in edges.windows(2) {
            let ((start, left), (end, right)) = (pair[0], pair[1]);
            if end > start && x >= left.min(right) && x < left.max(right) {
                return Some(start);
            }
        }
    }
    None
}

pub(crate) fn at(bytes: &str, tree: &Green, source: usize) -> Option<String> {
    destinations(bytes, tree)
        .into_iter()
        .find(|(range, _)| range.contains(&source))
        .map(|(_, href)| href.to_owned())
}
