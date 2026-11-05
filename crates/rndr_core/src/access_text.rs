//! Experimental source/Buffer accessibility ranges. Positions are UTF-8 bytes;
//! platform adapters must translate their character units against this snapshot.
use crate::editing::{attached_source, DocumentId, Revision, SourceDocuments, SourceSelection};
use crate::{Buffer, Entity, Focus, Input, Rect, TextLayout, World};

/// Exact text identity captured when a platform text range was published.
/// A source revision prevents ABA (edit then undo); legacy buffers compare bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccessTextState {
    entity: Entity,
    text: String,
    source: Option<(DocumentId, Revision)>,
    structured: Option<(crate::structured_native::StructuredHandle, Revision)>,
}
impl AccessTextState {
    /// Full document, never the editor's clipped paint Label or IME preview.
    pub fn text(&self) -> &str {
        &self.text
    }
}

/// Current source, directed selection and full-document shaped geometry.
/// Layout coordinates plus `origin` are window-logical points. `clip` is the
/// editor viewport, not a claim that offscreen text has visible bounds.
#[derive(Clone, Debug)]
pub struct AccessTextSnapshot {
    /// Freshness fence to echo with selection actions.
    pub state: AccessTextState,
    /// Document byte offsets, preserving anchor/head direction.
    pub selection: SourceSelection,
    /// Same text engine, width, style and spans used by the editor.
    pub layout: TextLayout,
    /// Visual row of the moving caret, retaining the editor's wrap affinity.
    pub caret_row: usize,
    /// Window-logical origin including current vertical viewport translation.
    pub origin: (f32, f32),
    /// Viewport bounds in window-logical points.
    pub clip: Rect,
    /// Document or inherited readonly policy. Selection remains available.
    pub readonly: bool,
    /// Selection actions may currently be dispatched (focused, not composing).
    pub selectable: bool,
}

impl AccessTextSnapshot {
    /// Unclipped caret rectangle in window-logical points. Intersect with clip
    /// for visible geometry; offscreen source positions remain addressable.
    pub fn caret_bounds(&self) -> Rect {
        let (x, y) = self
            .layout
            .caret_xy_on_line(self.selection.head, self.caret_row);
        let h = self
            .layout
            .lines
            .get(self.caret_row)
            .map_or(0.0, |line| line.h);
        Rect {
            x: self.origin.0 + x,
            y: self.origin.1 + y,
            w: 1.0,
            h,
        }
    }
    /// Unclipped shaped selection rectangles in window-logical points.
    pub fn selection_bounds(&self) -> Vec<Rect> {
        self.layout
            .selection_boxes(self.selection.anchor, self.selection.head)
            .into_iter()
            .map(|b| Rect {
                x: self.origin.0 + b.x,
                y: self.origin.1 + b.y,
                w: b.w,
                h: b.h,
            })
            .collect()
    }
}

fn state(world: &World, entity: Entity) -> Option<AccessTextState> {
    if !world.has::<crate::Editor>(entity) || !crate::access_details(world, entity).focus {
        return None;
    }
    let buffer = world.get::<Buffer>(entity)?;
    let text = buffer.text();
    let source = if let Some(document) = attached_source(world, entity) {
        let doc = world.resource::<SourceDocuments>()?.get(document)?;
        // Host changes may precede the next projection pass. Do not expose old
        // geometry/selection with a new document revision or retarget a view.
        let binding = world.get::<crate::editing::SourceBinding>(entity)?;
        if binding.revision != doc.revision()
            || doc.text() != text
            || doc.view_selection(entity)
                != Some(SourceSelection {
                    anchor: buffer.anchor,
                    head: buffer.caret,
                })
        {
            return None;
        }
        Some((document, doc.revision()))
    } else {
        None
    };
    let structured = if world.has::<crate::structured_native::StructuredBinding>(entity) {
        let display = crate::structured_native::structured_display(world, entity)?;
        Some((display.document, display.revision))
    } else {
        None
    };
    Some(AccessTextState {
        entity,
        text,
        source,
        structured,
    })
}
fn composing(world: &World, entity: Entity) -> bool {
    world
        .resource::<Input>()
        .is_some_and(|i| !i.ime_preedit.is_empty() || !i.ime_commit.is_empty())
        || world
            .get::<crate::ImePreview>(entity)
            .is_some_and(|p| !p.text.is_empty())
        || attached_source(world, entity)
            .and_then(|id| world.resource::<SourceDocuments>()?.get(id))
            .is_some_and(|doc| doc.is_composing())
        || crate::structured_native::is_composing(world, entity)
}
fn selectable(world: &World, entity: Entity) -> bool {
    world.resource::<Focus>().and_then(|f| f.entity) == Some(entity) && !composing(world, entity)
}

/// Snapshot an available editor, including readonly editors. Hidden, disabled,
/// modal-blocked and unprojected source views return None. This experimental
/// path shapes full text on demand; E3 performance certification is separate.
pub fn access_text(world: &World, entity: Entity) -> Option<AccessTextSnapshot> {
    let state = state(world, entity)?;
    let buffer = world.get::<Buffer>(entity)?;
    let clip = *world.get::<Rect>(entity)?;
    let style = crate::text::style_for(world, entity);
    let engine = crate::text::text_engine(world);
    let width = crate::font::field_text_width(clip.w, engine.unit_advance(&style));
    let spans = world
        .get::<crate::Spans>(entity)
        .map_or(&[][..], |s| s.runs.as_slice());
    let mut layout = engine.layout(state.text(), &style, spans, width);
    crate::source_folding::project_fold_layout(world, entity, &mut layout);
    let offset = world.get::<crate::Editor>(entity)?.offset;
    let offset = if offset.is_finite() {
        offset.max(0.0)
    } else {
        0.0
    };
    let top = layout
        .lines
        .iter()
        .rfind(|l| l.y <= offset)
        .map_or(0.0, |l| l.y);
    let readonly = crate::access_details(world, entity).readonly
        || state
            .source
            .and_then(|(id, _)| world.resource::<SourceDocuments>()?.get(id))
            .is_some_and(|doc| doc.is_readonly());
    let caret_row = crate::editor::access_caret_row(world, entity, &layout, buffer.caret);
    Some(AccessTextSnapshot {
        state,
        selection: SourceSelection {
            anchor: buffer.anchor,
            head: buffer.caret,
        },
        caret_row,
        layout,
        origin: (
            clip.x + crate::TEXT_ORIGIN_X,
            clip.y + crate::TEXT_ORIGIN_Y - top,
        ),
        clip,
        readonly,
        selectable: selectable(world, entity),
    })
}

pub(crate) fn set_selection(
    world: &mut World,
    entity: Entity,
    expected: AccessTextState,
    selection: SourceSelection,
) -> crate::AccessCommandResult {
    if !selectable(world, entity)
        || state(world, entity).as_ref() != Some(&expected)
        || !expected.text.is_char_boundary(selection.anchor)
        || !expected.text.is_char_boundary(selection.head)
    {
        return crate::AccessCommandResult::Rejected;
    }
    let selection = SourceSelection {
        anchor: crate::edit::floor_grapheme(&expected.text, selection.anchor),
        head: crate::edit::floor_grapheme(&expected.text, selection.head),
    };
    if let Some((_, revision)) = expected.structured {
        if crate::structured_native::set_display_selection(
            world,
            entity,
            revision,
            selection.anchor,
            selection.head,
        )
        .is_err()
        {
            return crate::AccessCommandResult::Rejected;
        }
    } else if let Some((id, revision)) = expected.source {
        let Some(doc) = world
            .resource_mut::<SourceDocuments>()
            .and_then(|docs| docs.get_mut(id))
        else {
            return crate::AccessCommandResult::Rejected;
        };
        if doc.set_view_selection(entity, revision, selection).is_err() {
            return crate::AccessCommandResult::Rejected;
        }
        crate::editing::project_source(world, entity);
    } else {
        if let Some(history) = world.get_mut::<crate::typing::EditHistory>(entity) {
            history.break_run();
        }
        let buffer = world.get_mut::<Buffer>(entity).expect("validated buffer");
        buffer.caret = selection.head;
        buffer.anchor = selection.anchor;
    }
    crate::editor::reveal_access_selection(world, entity);
    crate::AccessCommandResult::Applied
}
