//! Optional whole-line folding and small language-neutral source hooks.
//!
//! Folded bytes remain source: copying a selection across a fold includes them.
//! Navigation or selection into a hidden line expands its fold. A source commit
//! maps each range through that commit's [`crate::editing::SourceMap`]. A range
//! that no longer covers whole lines is dropped; the rest stay collapsed.
use crate::editing::{DocumentId, Revision, SourceBinding, SourceDocuments};
pub use crate::editor::{source_position_box, source_range_boxes};
use crate::{Buffer, Component, Entity, TextLayout, TextSpan, World};
use std::ops::Range;

/// Per-view collapsed line ranges. Construct through [`set_source_folds`].
#[derive(Clone, Debug)]
pub struct SourceFolds {
    document: DocumentId,
    revision: Revision,
    pub(crate) ranges: Vec<Range<usize>>,
}
impl Component for SourceFolds {}
impl SourceFolds {
    /// Collapsed source byte ranges, sorted and disjoint.
    pub fn ranges(&self) -> &[Range<usize>] {
        &self.ranges
    }
}
/// Why a fold publication was refused without modifying the existing folds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FoldError {
    /// View is detached, rebound or its projected revision is not current.
    Stale,
    /// Range is empty, overlapping, hides the first line, or splits a line.
    InvalidRange,
}
/// Publish collapsed whole logical lines for one attached source view.
///
/// `start` must follow a newline (never zero); `end` must follow a newline or
/// equal text length. Keep the opening/header line outside the hidden range.
/// Nested/overlapping ranges must be resolved by the provider before publication.
/// A selection endpoint in a collapsed range expands that range on the next pass.
/// Later commits map these ranges; they do not have to be published again.
pub fn set_source_folds(
    world: &mut World,
    view: Entity,
    document: DocumentId,
    revision: Revision,
    mut ranges: Vec<Range<usize>>,
) -> Result<(), FoldError> {
    let binding = world.get::<SourceBinding>(view).ok_or(FoldError::Stale)?;
    let doc = world
        .resource::<SourceDocuments>()
        .and_then(|d| d.get(document))
        .ok_or(FoldError::Stale)?;
    if binding.document != document || binding.revision != revision || doc.revision() != revision {
        return Err(FoldError::Stale);
    }
    let text = doc.text();
    ranges.sort_by_key(|r| r.start);
    let mut end = 0;
    for r in &ranges {
        if r.start < end || !crate::editing::is_whole_line_fold(text, r.start, r.end) {
            return Err(FoldError::InvalidRange);
        }
        end = r.end;
    }
    if let Some(doc) = world
        .resource_mut::<SourceDocuments>()
        .and_then(|docs| docs.get_mut(document))
    {
        doc.set_view_folds(view, ranges.clone());
    }
    world.insert(
        view,
        SourceFolds {
            document,
            revision,
            ranges,
        },
    );
    Ok(())
}
/// Expand all folds. Source bytes and history are untouched.
pub fn clear_source_folds(world: &mut World, view: Entity) {
    if let Some(document) = world.get::<SourceBinding>(view).map(|b| b.document) {
        if let Some(doc) = world
            .resource_mut::<SourceDocuments>()
            .and_then(|docs| docs.get_mut(document))
        {
            doc.clear_view_folds(view);
        }
    }
    world.remove::<SourceFolds>(view);
}

pub(crate) fn current(world: &World, view: Entity) -> Option<&SourceFolds> {
    let folds = world.get::<SourceFolds>(view)?;
    let binding = world.get::<SourceBinding>(view)?;
    let doc = world.resource::<SourceDocuments>()?.get(binding.document)?;
    (folds.document == binding.document
        && folds.revision == binding.revision
        && folds.revision == doc.revision())
    .then_some(folds)
}
pub(crate) fn prepare(world: &mut World, view: Entity) {
    // The binding still names the pre-commit revision. Leave the component
    // alone until project_source publishes the buffer for the mapped ranges.
    if let Some(binding) = world.get::<SourceBinding>(view).copied() {
        let current_revision = world
            .resource::<SourceDocuments>()
            .and_then(|docs| docs.get(binding.document))
            .map(|doc| doc.revision());
        if current_revision.is_some_and(|revision| revision != binding.revision) {
            return;
        }
    }
    adopt_mapped_folds(world, view);
    if !world.has::<SourceFolds>(view) {
        return;
    }
    if current(world, view).is_none() {
        clear_source_folds(world, view);
        return;
    }
    let Some(buffer) = world.get::<Buffer>(view) else {
        clear_source_folds(world, view);
        return;
    };
    let mut endpoints = vec![buffer.caret, buffer.anchor];
    if let Some(binding) = world.get::<SourceBinding>(view) {
        if let Some(selections) = world
            .resource::<SourceDocuments>()
            .and_then(|docs| docs.get(binding.document))
            .and_then(|doc| doc.view_selections(view))
        {
            for selection in selections.ranges() {
                endpoints.extend([selection.anchor, selection.head]);
            }
        }
    }
    if let Some(folds) = world.get_mut::<SourceFolds>(view) {
        folds
            .ranges
            .retain(|r| !endpoints.iter().any(|at| r.contains(at)));
    }
    let ranges = world
        .get::<SourceFolds>(view)
        .map(|folds| folds.ranges.clone())
        .unwrap_or_default();
    if let Some(document) = world
        .get::<SourceBinding>(view)
        .map(|binding| binding.document)
    {
        if let Some(doc) = world
            .resource_mut::<SourceDocuments>()
            .and_then(|docs| docs.get_mut(document))
        {
            doc.set_view_folds(view, ranges);
        }
    }
    if world
        .get::<SourceFolds>(view)
        .is_some_and(|folds| folds.ranges.is_empty())
    {
        clear_source_folds(world, view);
    }
}

/// Copy commit-mapped folds onto the view once its binding matches the document.
/// A binding that is still on the previous revision keeps its component untouched
/// so old coordinates are not applied to the new text, and are not cleared either.
fn adopt_mapped_folds(world: &mut World, view: Entity) {
    let Some(binding) = world.get::<SourceBinding>(view).copied() else {
        return;
    };
    let Some(doc_revision) = world
        .resource::<SourceDocuments>()
        .and_then(|docs| docs.get(binding.document))
        .map(|doc| doc.revision())
    else {
        return;
    };
    if binding.revision != doc_revision {
        return;
    }
    let component_revision = world.get::<SourceFolds>(view).map(|folds| folds.revision);
    let component_document = world.get::<SourceFolds>(view).map(|folds| folds.document);
    if component_document.is_some_and(|document| document != binding.document) {
        clear_source_folds(world, view);
        return;
    }
    if component_revision == Some(doc_revision) {
        return;
    }
    let mapped = world
        .resource::<SourceDocuments>()
        .and_then(|docs| docs.get(binding.document))
        .and_then(|doc| doc.view_folds(view).map(|ranges| ranges.to_vec()));
    if let Some(ranges) = mapped.filter(|ranges| !ranges.is_empty()) {
        world.insert(
            view,
            SourceFolds {
                document: binding.document,
                revision: doc_revision,
                ranges,
            },
        );
    } else if world.has::<SourceFolds>(view) {
        clear_source_folds(world, view);
    }
}
/// Remove folded rows from full-document geometry while preserving document
/// byte indices. This is also the accessibility projection contract. Only
/// whole-line source folds are supported, never inline/markup elision.
/// Glyph/highlight geometry must not be painted after this projection; paint
/// uses the editor's separately projected visible Label.
pub fn project_fold_layout(world: &World, view: Entity, layout: &mut TextLayout) {
    let Some(folds) = current(world, view) else {
        return;
    };
    if folds.ranges.is_empty() {
        return;
    }
    layout.lines.retain(|line| {
        let next = folds.ranges.partition_point(|r| r.end <= line.start);
        !folds
            .ranges
            .get(next)
            .is_some_and(|r| r.contains(&line.start))
    });
    let mut y = 0.0;
    for line in &mut layout.lines {
        line.y = y;
        y += line.h;
    }
    layout.height = y;
    layout.glyphs.clear();
    layout.highlights.clear();
}

/// One indentation suggestion, in logical columns (tabs use provider policy).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceIndent {
    /// Logical line's first byte.
    pub line: usize,
    /// Suggested indentation columns, not an automatic source edit.
    pub columns: usize,
}
/// Optional source-analysis result. Publication and source edits remain separate.
#[derive(Clone, Debug, Default)]
pub struct SourceAnalysis {
    /// Document-relative syntax spans.
    pub syntax: Vec<TextSpan>,
    /// Indentation suggestions, never automatically inserted.
    pub indents: Vec<SourceIndent>,
    /// Matched opening/closing ASCII bracket byte offsets.
    pub brackets: Vec<(usize, usize)>,
    /// Candidate hidden whole-line ranges; nested candidates may overlap.
    pub folds: Vec<Range<usize>>,
}
/// Optional synchronous language hook; applications may compute elsewhere and
/// publish through revision-fenced provider APIs. No language transport required.
pub trait SourceLanguageHooks: Send + Sync {
    /// Analyze one immutable source revision. All positions are UTF-8 bytes.
    fn analyze(&self, text: &str) -> SourceAnalysis;
}
/// Representative brace-source provider, not a language parser. Recognizes
/// ASCII brackets and numbers, double-quoted strings with backslash escapes,
/// and `//` comments; other syntax (raw strings, block comments, interpolation)
/// is intentionally not interpreted. Indentation uses four spaces per brace.
#[derive(Clone, Copy, Debug, Default)]
pub struct BraceSourceHooks;
impl SourceLanguageHooks for BraceSourceHooks {
    fn analyze(&self, text: &str) -> SourceAnalysis {
        let mut result = SourceAnalysis::default();
        let mut stack = Vec::<(u8, usize)>::new();
        let bytes = text.as_bytes();
        let mut i = 0;
        let mut line = 0;
        let mut depth = 0usize;
        result.indents.push(SourceIndent { line, columns: 0 });
        while i < bytes.len() {
            let b = bytes[i];
            if b == b'\n' {
                line = i + 1;
                let closing = text[line..].trim_start().starts_with('}');
                result.indents.push(SourceIndent {
                    line,
                    columns: depth.saturating_sub(usize::from(closing)) * 4,
                });
            } else if b == b'/' && bytes.get(i + 1) == Some(&b'/') {
                let start = i;
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
                result
                    .syntax
                    .push(TextSpan::new(start, i, crate::Color::rgb(0.4, 0.6, 0.4)));
                continue;
            } else if b == b'"' {
                let start = i;
                i += 1;
                while i < bytes.len() && bytes[i] != b'\n' {
                    if bytes[i] == b'\\' {
                        i = (i + 2).min(bytes.len());
                    } else if bytes[i] == b'"' {
                        i += 1;
                        break;
                    } else {
                        i += 1;
                    }
                }
                // ASCII scanning can stop after an escaped non-ASCII leading byte.
                while !text.is_char_boundary(i) {
                    i += 1;
                }
                result
                    .syntax
                    .push(TextSpan::new(start, i, crate::Color::rgb(0.7, 0.5, 0.3)));
                continue;
            } else if b.is_ascii_digit() {
                let start = i;
                while i < bytes.len() && bytes[i].is_ascii_digit() {
                    i += 1;
                }
                result
                    .syntax
                    .push(TextSpan::new(start, i, crate::Color::rgb(0.4, 0.5, 0.8)));
                continue;
            } else if b"{([".contains(&b) {
                stack.push((b, i));
                if b == b'{' {
                    depth += 1;
                }
            } else if b"})]".contains(&b) {
                let expected = match b {
                    b'}' => b'{',
                    b')' => b'(',
                    _ => b'[',
                };
                if stack.last().is_some_and(|(open, _)| *open == expected) {
                    let (_, at) = stack.pop().unwrap();
                    result.brackets.push((at, i));
                    if b == b'}' {
                        depth = depth.saturating_sub(1);
                        if let Some(nl) = text[at..i].find('\n') {
                            let start = at + nl + 1;
                            if start < line {
                                result.folds.push(start..line);
                            }
                        }
                    }
                }
            }
            i += 1;
        }
        result
    }
}
