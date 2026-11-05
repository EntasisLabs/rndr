//! Additive source-provider paint projection. No host Spans, Decorations, or
//! gutter components are replaced. Reads revalidate tickets at paint time, so
//! refresh scheduling cannot leave a cached stale mark on screen.
use crate::source_extensions::{DiagnosticSeverity, ProviderId, SourceExtensions};
use crate::{Color, Entity, Label, TextLayout, World};

/// One live provider fragment, relative to the visible text origin.
#[derive(Clone, Debug)]
pub struct ProviderMark {
    /// Owning registration (paint order is low-to-high provider priority).
    pub provider: ProviderId,
    /// Left edge relative to text; gutter marks instead occupy the view's edge.
    pub x: f32,
    /// Top edge relative to text.
    pub y: f32,
    /// Fragment width.
    pub w: f32,
    /// Fragment height.
    pub h: f32,
    /// Original generic decoration color, or diagnostic severity color.
    pub color: Color,
    /// Whether this is a view-edge gutter indicator, not a text overlay.
    pub gutter: bool,
    /// Semantic class or diagnostic/gutter description for host accessibility.
    pub label: String,
}

/// One secondary source selection fragment, relative to the visible text origin.
#[derive(Clone, Debug)]
pub struct SecondarySelectionMark {
    /// Left edge relative to text.
    pub x: f32,
    /// Top edge relative to text.
    pub y: f32,
    /// Fragment width.
    pub w: f32,
    /// Fragment height.
    pub h: f32,
    /// True for a collapsed secondary caret; false for a selection background.
    pub caret: bool,
}

/// Project secondary selections from the current attached source session.
///
/// The primary is already painted by the normal text path. This additive path
/// needs no extension registry and never mutates host spans or decorations.
/// Unprojected selections, stale buffers, detach, and IME suppress all marks.
/// Hidden/offscreen fragments use the same document geometry as native input.
pub fn secondary_selection_marks(world: &World, view: Entity) -> Vec<SecondarySelectionMark> {
    let Some(binding) = world.get::<crate::editing::SourceBinding>(view) else {
        return Vec::new();
    };
    let Some(doc) = world
        .resource::<crate::editing::SourceDocuments>()
        .and_then(|docs| docs.get(binding.document))
    else {
        return Vec::new();
    };
    let Some(buffer) = world.get::<crate::Buffer>(view) else {
        return Vec::new();
    };
    let Some(selections) = doc.view_selections(view) else {
        return Vec::new();
    };
    if selections.ranges().len() <= 1 {
        return Vec::new();
    }
    let primary = selections.primary();
    if binding.revision != doc.revision()
        || !world.has::<crate::Editor>(view)
        || buffer.caret != primary.head
        || buffer.anchor != primary.anchor
        || buffer.snapshot().as_ref() != doc.text()
        || doc.is_composing()
        || world.has::<crate::ImePreview>(view)
        || world
            .resource::<crate::Input>()
            .is_some_and(|i| !i.ime_preedit.is_empty())
    {
        return Vec::new();
    }
    let mut marks = Vec::new();
    for (index, selection) in selections.ranges().iter().enumerate() {
        if index == selections.primary_index() {
            continue;
        }
        let caret = selection.anchor == selection.head;
        let boxes = if caret {
            crate::source_folding::source_position_box(world, view, selection.head)
                .into_iter()
                .collect()
        } else {
            crate::source_folding::source_range_boxes(
                world,
                view,
                selection.anchor.min(selection.head),
                selection.anchor.max(selection.head),
            )
        };
        for b in boxes {
            marks.push(SecondarySelectionMark {
                x: b.x,
                y: b.y,
                w: b.w,
                h: b.h,
                caret,
            });
        }
    }
    marks
}

// A source position in the Label projection. Folds outside the current window
// must not be subtracted twice. Hidden interiors collapse to their seam.
fn offset(world: &World, view: Entity, at: usize, len: usize) -> Option<usize> {
    let base = crate::editor::window_base(world, view);
    if at < base {
        return None;
    }
    let removed: usize = crate::source_folding::current(world, view)
        .into_iter()
        .flat_map(|f| f.ranges.iter())
        .map(|r| at.min(r.end).saturating_sub(base.max(r.start)))
        .sum();
    let relative = at.saturating_sub(base).saturating_sub(removed);
    (relative <= len).then_some(relative)
}

/// Project currently live provider marks, without changing any World component.
///
/// Install the controller as a [`SourceExtensions`] World resource to opt in to
/// the standard painter. No update/layout system is required. Generic colored
/// decorations use a two-point underline (class is host-defined, never parsed).
/// Diagnostics use a severity-colored underline. Gutter annotations use a
/// three-point view-edge marker and preserve their full accessible description
/// here for host tooltip/accessibility adapters. Hidden-only/offscreen ranges
/// produce no geometry. Calling after retirement/detach/revision change returns
/// no obsolete fragments even without a controller refresh.
pub fn provider_marks(world: &World, view: Entity) -> Vec<ProviderMark> {
    let Some(binding) = world.get::<crate::editing::SourceBinding>(view) else {
        return Vec::new();
    };
    let Some(document) = world
        .resource::<crate::editing::SourceDocuments>()
        .and_then(|d| d.get(binding.document))
    else {
        return Vec::new();
    };
    if binding.revision != document.revision() {
        return Vec::new();
    }
    let Some(extensions) = world.resource::<SourceExtensions>() else {
        return Vec::new();
    };
    let Some(layout) = world.get::<TextLayout>(view) else {
        return Vec::new();
    };
    let Some(label) = world.get::<Label>(view) else {
        return Vec::new();
    };
    let len = label.text.len();
    let mut marks = Vec::new();
    let mut range = |provider, start: usize, end: usize, color, description: &str| {
        // Clip a span straddling the window rather than discarding it.
        let start = offset(
            world,
            view,
            start.max(crate::editor::window_base(world, view)),
            len,
        )
        .unwrap_or(len);
        let end = offset(world, view, end, len).unwrap_or_else(|| {
            if end < crate::editor::window_base(world, view) {
                0
            } else {
                len
            }
        });
        if start >= end {
            return;
        }
        for b in layout.selection_boxes(start, end) {
            marks.push(ProviderMark {
                provider,
                x: b.x,
                y: b.y + b.h - 2.0_f32.min(b.h),
                w: b.w.max(1.0),
                h: 2.0_f32.min(b.h),
                color,
                gutter: false,
                label: description.into(),
            });
        }
    };
    for (provider, decoration) in extensions.decorations(world, view) {
        range(
            provider,
            decoration.range.start,
            decoration.range.end,
            decoration.color,
            &decoration.class,
        );
    }
    for (provider, diagnostic) in extensions.diagnostics(world, view) {
        let color = match diagnostic.severity {
            DiagnosticSeverity::Error => Color::rgb(0.86, 0.28, 0.24),
            DiagnosticSeverity::Warning => Color::rgb(0.85, 0.62, 0.18),
            DiagnosticSeverity::Information => Color::rgb(0.30, 0.60, 0.90),
            DiagnosticSeverity::Hint => Color::rgb(0.45, 0.65, 0.55),
        };
        range(
            provider,
            diagnostic.range.start,
            diagnostic.range.end,
            color,
            &diagnostic.message,
        );
    }
    for (provider, marker) in extensions.gutter_markers(world, view) {
        // A hidden source marker must not incorrectly attach to the next row.
        if crate::source_folding::current(world, view)
            .is_some_and(|f| f.ranges.iter().any(|r| r.contains(&marker.at)))
        {
            continue;
        }
        let Some(at) = offset(world, view, marker.at, len) else {
            continue;
        };
        if let Some(line) = layout
            .lines
            .iter()
            .find(|line| line.start <= at && at <= line.end)
        {
            marks.push(ProviderMark {
                provider,
                x: 0.0,
                y: line.y,
                w: 3.0,
                h: line.h,
                color: Color::rgb(0.40, 0.65, 0.90),
                gutter: true,
                label: marker.label.clone(),
            });
        }
    }
    marks
}
