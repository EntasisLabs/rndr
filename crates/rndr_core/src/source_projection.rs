//! Source-preserving staged rendered editing, not a Markdown codec.
//!
//! Supported contract: capture a live literal target, compose locally, then apply
//! through the same source session. A target never rebases itself. Unsupported
//! edits reveal source; no serializer silently takes over. The narrow grammar
//! below is intentional, not a claim of CommonMark or inline WYSIWYG support.
//!
//! ```
//! use rndr_core::editing::{EditOrigin, SourceDocuments};
//! use rndr_core::source_projection::{Projection, ProjectionSelection};
//!
//! let mut documents = SourceDocuments::default();
//! let id = documents.open("A *quiet* line  \r\n");
//! let source = documents.get_mut(id).unwrap();
//! let projection = Projection::build(&source.snapshot());
//! let target = projection.capture(source, ProjectionSelection::inward(2, 7))?;
//! assert_eq!(target.original_text(), "quiet");
//! target.replace(source, "calm", EditOrigin::Host(1))?;
//! assert_eq!(source.text(), "A *calm* line  \r\n");
//! source.undo()?;
//! assert_eq!(source.text(), "A *quiet* line  \r\n");
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! A deliberately narrow line grammar supports plain text, single `*emphasis*`,
//! and `- [ ]` / `- [x]` tasks. Unknown syntax is an opaque source-reveal atom.
//! The immutable map owns source byte ranges; display offsets are never reused
//! as source offsets. Only a splice wholly inside one literal segment is
//! editable. Crossing hidden syntax/atoms requests source reveal instead.
use crate::editing::{
    Affinity, ChangeEvent, DocumentId, EditError, EditOrigin, Revision, SourceBatch, SourceChange,
    SourceSelection, SourceSession, SourceSnapshot,
};
use std::ops::Range;

/// How one monotonic source region appears in the rendered buffer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SegmentKind {
    /// Exact UTF-8 text, editable without serialization.
    Text,
    /// Zero-width source syntax (BOM or emphasis delimiter).
    Hidden,
    /// An atomic task marker, changed only by an explicit toggle command.
    Task,
    /// Unsupported raw source; reveal it in a source editor to edit.
    Opaque,
}
/// Explicit source/display byte ranges in a single captured revision.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectionSegment {
    /// Original source range, including hidden bytes.
    pub source: Range<usize>,
    /// Display range; empty for hidden syntax.
    pub display: Range<usize>,
    /// Mapping and edit policy.
    pub kind: SegmentKind,
}
/// A logical display byte offset plus syntax-boundary affinity.
/// Bidi/resize geometry must first resolve to a logical byte via the text engine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProjectionPoint {
    /// UTF-8 byte offset in the captured display, never a source offset.
    pub offset: usize,
    /// Side of zero-width syntax at this position.
    pub affinity: Affinity,
}
/// Directed endpoints; reversed selections retain their direction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProjectionSelection {
    /// Fixed endpoint.
    pub anchor: ProjectionPoint,
    /// Moving endpoint.
    pub head: ProjectionPoint,
}
impl ProjectionSelection {
    /// Select literal content inward at both ends; crossing syntax still refuses edits.
    pub fn inward(anchor: usize, head: usize) -> Self {
        let (a, h) = if anchor <= head {
            (Affinity::After, Affinity::Before)
        } else {
            (Affinity::Before, Affinity::After)
        };
        Self {
            anchor: ProjectionPoint {
                offset: anchor,
                affinity: a,
            },
            head: ProjectionPoint {
                offset: head,
                affinity: if anchor == head { a } else { h },
            },
        }
    }
}
/// Explicit clipboard policy; rendered text is not a lossless source codec.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CopyPolicy {
    /// Exact source bytes between mapped directed endpoints, including internal syntax.
    Source,
    /// Display text, including visible checkbox/opaque labels; never parsed on paste.
    Rendered,
}
/// A refused projection operation has no document/history/selection side effects.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProjectionError {
    /// Source-session admission failed (including stale revision and readonly).
    Source(EditError),
    /// Invalid UTF-8 endpoint (source or display, according to the operation).
    InvalidPoint,
    /// Operation crosses syntax/atom boundaries or would introduce unsupported syntax.
    /// The host should switch to source mode and select this original range.
    RevealSource(Range<usize>),
}
impl From<EditError> for ProjectionError {
    fn from(value: EditError) -> Self {
        Self::Source(value)
    }
}
impl std::fmt::Display for ProjectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Source(error) => write!(f, "source admission refused: {error}"),
            Self::InvalidPoint => f.write_str("invalid projection byte endpoint"),
            Self::RevealSource(range) => write!(
                f,
                "edit source bytes {}..{} instead",
                range.start, range.end
            ),
        }
    }
}
impl std::error::Error for ProjectionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Source(error) => Some(error),
            _ => None,
        }
    }
}
/// Immutable map and text derived from exactly one source identity/revision.
#[derive(Clone, Debug)]
pub struct Projection {
    source: SourceSnapshot,
    text: String,
    segments: Vec<ProjectionSegment>,
}
impl Projection {
    /// Build without changing source bytes, revision, history, or selection.
    pub fn build(source: &SourceSnapshot) -> Self {
        let mut result = Self {
            source: source.clone(),
            text: String::new(),
            segments: Vec::new(),
        };
        let mut offset = 0;
        if source.text.starts_with('\u{feff}') {
            result.push(0..3, SegmentKind::Hidden, "");
            offset = 3;
        }
        let mut fence: Option<(u8, usize, usize)> = None;
        let mut opaque_start = None;
        for line in source.text[offset..].split_inclusive('\n') {
            let start = offset;
            offset += line.len();
            let body = line.trim_end_matches(['\r', '\n']);
            let trimmed = body.trim_start_matches(' ');
            let indent = body.len() - trimmed.len();
            let marker = trimmed.as_bytes().first().copied().unwrap_or(0);
            let count = trimmed.bytes().take_while(|b| *b == marker).count();
            if let Some((open, length, _)) = fence {
                if indent <= 3
                    && marker == open
                    && count >= length
                    && trimmed[count..].trim().is_empty()
                {
                    result.opaque(opaque_start.take().unwrap()..offset);
                    fence = None;
                }
                continue;
            }
            if indent <= 3 && matches!(marker, b'`' | b'~') && count >= 3 {
                fence = Some((marker, count, start));
                opaque_start = Some(start);
                continue;
            }
            result.line(start, body, line.len() - body.len());
        }
        if let Some(start) = opaque_start {
            result.opaque(start..source.text.len());
        }
        if result.segments.is_empty()
            || result
                .segments
                .iter()
                .all(|s| s.kind == SegmentKind::Hidden)
        {
            result.literal(source.text.len()..source.text.len());
        }
        result
    }
    fn push(&mut self, source: Range<usize>, kind: SegmentKind, display: &str) {
        let start = self.text.len();
        self.text.push_str(display);
        self.segments.push(ProjectionSegment {
            source,
            display: start..self.text.len(),
            kind,
        });
    }
    fn literal(&mut self, source: Range<usize>) {
        let text = self.source.text[source.clone()].to_owned();
        self.push(source, SegmentKind::Text, &text);
    }
    fn opaque(&mut self, source: Range<usize>) {
        let newline = if self.source.text[source.clone()].ends_with('\n') {
            "\n"
        } else {
            ""
        };
        self.push(
            source,
            SegmentKind::Opaque,
            &format!("[Unsupported source — reveal to edit]{newline}"),
        );
    }
    fn line(&mut self, start: usize, body: &str, newline: usize) {
        let task =
            body.starts_with("- [ ] ") || body.starts_with("- [x] ") || body.starts_with("- [X] ");
        let skip = if task { 6 } else { 0 };
        let content = &body[skip..];
        // No nesting, escaping, links, inline code, HTML, lists, headings,
        // tables, underscore emphasis, controls, or multiline rich grammar.
        // Conservative source reveal is preferable to a false syntax claim.
        let stars: Vec<_> = content.match_indices('*').map(|(at, _)| at).collect();
        let emphasis = stars.len() == 2
            && stars[1] > stars[0] + 1
            && !content[stars[0] + 1..stars[1]].starts_with(char::is_whitespace)
            && !content[stars[0] + 1..stars[1]].ends_with(char::is_whitespace);
        let unsupported = content.chars().any(|c| {
            matches!(
                c,
                '`' | '~' | '_' | '[' | ']' | '<' | '>' | '#' | '|' | '\\' | '\r'
            ) || c.is_control()
        }) || content.starts_with([' ', '\t', '-', '+'])
            || content.as_bytes().first().is_some_and(u8::is_ascii_digit)
            || (!stars.is_empty() && !emphasis);
        if unsupported {
            self.opaque(start..start + body.len() + newline);
            return;
        }
        if task {
            // ASCII fallback is deliberate: no platform glyph/resource is
            // required to distinguish the task's checked state.
            let display = if &body[3..4] == " " { "[ ] " } else { "[x] " };
            self.push(start..start + skip, SegmentKind::Task, display);
        }
        let base = start + skip;
        if emphasis {
            let a = stars[0];
            let b = stars[1];
            if a > 0 {
                self.literal(base..base + a);
            }
            self.push(base + a..base + a + 1, SegmentKind::Hidden, "");
            self.literal(base + a + 1..base + b);
            self.push(base + b..base + b + 1, SegmentKind::Hidden, "");
            if b + 1 < content.len() {
                self.literal(base + b + 1..base + content.len());
            }
        } else if !content.is_empty() {
            self.literal(base..base + content.len());
        }
        if newline > 0 {
            self.literal(start + body.len()..start + body.len() + newline);
        }
        // Empty final documents/lines need a legal insertion target.
        if content.is_empty() && newline == 0 {
            self.literal(base..base);
        }
    }
    /// Captured document identity.
    pub fn document(&self) -> DocumentId {
        self.source.document
    }
    /// Captured revision; never refreshed in place after a commit.
    pub fn revision(&self) -> Revision {
        self.source.revision
    }
    /// Logical rendered text. Hosts shape it using their ordinary text engine.
    pub fn text(&self) -> &str {
        &self.text
    }
    /// Complete monotonic source partition, including zero-width syntax.
    pub fn segments(&self) -> &[ProjectionSegment] {
        &self.segments
    }
    /// Ensure cached projection still describes this exact live authority.
    pub fn validate(&self, session: &SourceSession) -> Result<(), ProjectionError> {
        let snapshot = session.snapshot();
        if snapshot.document != self.source.document {
            return Err(EditError::MissingDocument.into());
        }
        if snapshot.revision != self.source.revision || snapshot.text != self.source.text {
            return Err(EditError::StaleRevision.into());
        }
        Ok(())
    }
    /// Map a scalar display point with explicit hidden-boundary affinity.
    /// Interior atomic points request source reveal, never proportional mapping.
    pub fn point_to_source(&self, point: ProjectionPoint) -> Result<usize, ProjectionError> {
        let at = point.offset;
        if !self.text.is_char_boundary(at) {
            return Err(ProjectionError::InvalidPoint);
        }
        let mut candidates = Vec::new();
        for s in &self.segments {
            if s.display.start <= at && at <= s.display.end {
                if s.kind == SegmentKind::Text {
                    candidates.push(s.source.start + at - s.display.start);
                } else if s.display.is_empty() {
                    candidates.extend([s.source.start, s.source.end]);
                } else if at == s.display.start {
                    candidates.push(s.source.start);
                } else if at == s.display.end {
                    candidates.push(s.source.end);
                } else {
                    return Err(ProjectionError::RevealSource(s.source.clone()));
                }
            }
        }
        if candidates.is_empty() && self.source.text.is_empty() && at == 0 {
            return Ok(0);
        }
        match point.affinity {
            Affinity::Before => candidates.into_iter().min(),
            Affinity::After => candidates.into_iter().max(),
        }
        .ok_or(ProjectionError::InvalidPoint)
    }
    /// Map an exact captured source endpoint back into logical display geometry.
    /// Hidden boundaries recover their syntax affinity. Interior task/opaque
    /// positions reveal source rather than snapping to a guessed display caret.
    /// This is not a cross-revision map or a bidi visual-affinity conversion.
    pub fn point_from_source(&self, offset: usize) -> Result<ProjectionPoint, ProjectionError> {
        if !self.source.text.is_char_boundary(offset) {
            return Err(ProjectionError::InvalidPoint);
        }
        // Prefer hidden boundaries over adjacent literals, recovering the side
        // needed for an exact round trip through point_to_source.
        for s in &self.segments {
            if s.kind == SegmentKind::Hidden && s.source.start <= offset && offset <= s.source.end {
                return Ok(ProjectionPoint {
                    offset: s.display.start,
                    affinity: if offset == s.source.start {
                        Affinity::Before
                    } else {
                        Affinity::After
                    },
                });
            }
        }
        for s in &self.segments {
            if s.source.start <= offset && offset <= s.source.end {
                let display = if s.kind == SegmentKind::Text {
                    s.display.start + offset - s.source.start
                } else if offset == s.source.start {
                    s.display.start
                } else if offset == s.source.end {
                    s.display.end
                } else {
                    return Err(ProjectionError::RevealSource(s.source.clone()));
                };
                return Ok(ProjectionPoint {
                    offset: display,
                    affinity: Affinity::After,
                });
            }
        }
        Err(ProjectionError::InvalidPoint)
    }
    /// Map directed source endpoints in this revision without changing selection.
    /// A source selection inside an opaque atom must remain in the source pane.
    pub fn selection_from_source(
        &self,
        selection: SourceSelection,
    ) -> Result<ProjectionSelection, ProjectionError> {
        Ok(ProjectionSelection {
            anchor: self.point_from_source(selection.anchor)?,
            head: self.point_from_source(selection.head)?,
        })
    }
    /// Map directed endpoints without changing the source session selection.
    pub fn selection_to_source(
        &self,
        selection: ProjectionSelection,
    ) -> Result<SourceSelection, ProjectionError> {
        Ok(SourceSelection {
            anchor: self.point_to_source(selection.anchor)?,
            head: self.point_to_source(selection.head)?,
        })
    }
    /// Copy captured bytes with an explicit policy. A host must validate this
    /// projection against the live session before using a live selection.
    pub fn copy(
        &self,
        selection: ProjectionSelection,
        policy: CopyPolicy,
    ) -> Result<String, ProjectionError> {
        if !self.text.is_char_boundary(selection.anchor.offset)
            || !self.text.is_char_boundary(selection.head.offset)
        {
            return Err(ProjectionError::InvalidPoint);
        }
        if policy == CopyPolicy::Rendered {
            let a = selection.anchor.offset.min(selection.head.offset);
            let b = selection.anchor.offset.max(selection.head.offset);
            return Ok(self.text[a..b].to_owned());
        }
        let s = self.selection_to_source(selection)?;
        Ok(self.source.text[s.anchor.min(s.head)..s.anchor.max(s.head)].to_owned())
    }
    /// Copy a live selection only after identity, revision and byte validation.
    /// Use `copy` instead only when intentionally copying a historical snapshot.
    pub fn copy_live(
        &self,
        session: &SourceSession,
        selection: ProjectionSelection,
        policy: CopyPolicy,
    ) -> Result<String, ProjectionError> {
        self.validate(session)?;
        self.copy(selection, policy)
    }
    /// Capture a checked literal target for staged native field/IME replacement.
    /// Capture is read-only: it does not reserve composition or change selection.
    /// Apply rechecks source authority and current admission; no automatic rebase.
    pub fn capture(
        &self,
        session: &SourceSession,
        selection: ProjectionSelection,
    ) -> Result<ProjectionTarget, ProjectionError> {
        self.validate(session)?;
        let original = self.copy(selection, CopyPolicy::Source)?;
        self.replacement_batch(selection, &original, EditOrigin::Host(0))?;
        Ok(ProjectionTarget {
            projection: self.clone(),
            selection,
            original,
        })
    }
    /// Translate a splice only if both endpoints and every selected byte belong
    /// to one literal segment. New syntax/newlines must be edited in source mode.
    /// This is a snapshot-only planner, not live admission. Prefer `replace` or
    /// `ProjectionTarget::replace`: they also validate exact live source bytes.
    pub fn replacement_batch(
        &self,
        selection: ProjectionSelection,
        replacement: &str,
        origin: EditOrigin,
    ) -> Result<SourceBatch, ProjectionError> {
        let mapped = self.selection_to_source(selection)?;
        let a = mapped.anchor.min(mapped.head);
        let b = mapped.anchor.max(mapped.head);
        let literal = self.segments.iter().any(|s| {
            s.kind == SegmentKind::Text
                && s.source.start <= a
                && b <= s.source.end
                && !self.source.text[a..b].contains(['\r', '\n'])
        });
        let syntax = replacement.chars().any(|c| {
            c.is_control()
                || matches!(
                    c,
                    '*' | '_' | '`' | '~' | '[' | ']' | '<' | '>' | '#' | '|' | '\\'
                )
        });
        if !literal || syntax {
            return Err(ProjectionError::RevealSource(a..b));
        }
        // Editing may make previously literal text become syntax (or empty an
        // emphasis span). Reparse locally before admitting, then require every
        // nonliteral region's kind and exact raw bytes to remain unchanged.
        // Full reparse is intentionally O(document); no incremental/large-file claim.
        let mut candidate = self.source.clone();
        candidate.text.replace_range(a..b, replacement);
        let rebuilt = Self::build(&candidate);
        if !self.custody().eq(rebuilt.custody()) {
            return Err(ProjectionError::RevealSource(a..b));
        }
        Ok(SourceBatch {
            document: self.document(),
            base: self.revision(),
            origin,
            changes: vec![SourceChange {
                range: a..b,
                replacement: replacement.to_owned(),
            }],
        })
    }
    fn custody(&self) -> impl Iterator<Item = (SegmentKind, &str)> {
        self.segments
            .iter()
            .filter(|s| s.kind != SegmentKind::Text)
            .map(|s| (s.kind, &self.source.text[s.source.clone()]))
    }
    /// Commit through the current source session/history; stale maps refuse.
    pub fn replace(
        &self,
        session: &mut SourceSession,
        selection: ProjectionSelection,
        replacement: &str,
        origin: EditOrigin,
    ) -> Result<Option<ChangeEvent>, ProjectionError> {
        self.validate(session)?;
        let batch = self.replacement_batch(selection, replacement, origin)?;
        Ok(session.commit(batch)?)
    }
    /// Toggle an atomic task using its captured source marker, never its label.
    pub fn toggle_task(
        &self,
        session: &mut SourceSession,
        offset: usize,
        origin: EditOrigin,
    ) -> Result<Option<ChangeEvent>, ProjectionError> {
        self.validate(session)?;
        if !self.text.is_char_boundary(offset) {
            return Err(ProjectionError::InvalidPoint);
        }
        let s = self
            .segments
            .iter()
            .find(|s| s.kind == SegmentKind::Task && s.display.contains(&offset))
            .ok_or(ProjectionError::InvalidPoint)?;
        let marker = s.source.start + 3;
        let replacement = if &self.source.text[marker..marker + 1] == " " {
            "x"
        } else {
            " "
        };
        Ok(session.commit(SourceBatch {
            document: self.document(),
            base: self.revision(),
            origin,
            changes: vec![SourceChange {
                range: marker..marker + 1,
                replacement: replacement.into(),
            }],
        })?)
    }
    /// Capture an IME target. Preedit stays local and cannot mutate source.
    pub fn begin_composition(
        &self,
        selection: ProjectionSelection,
    ) -> Result<ProjectionComposition, ProjectionError> {
        let unchanged = self.copy(selection, CopyPolicy::Source)?;
        self.replacement_batch(selection, &unchanged, EditOrigin::Ime)?;
        Ok(ProjectionComposition {
            projection: self.clone(),
            selection,
            preedit: String::new(),
        })
    }
}
/// Reusable, revision-bound literal target. Fields are private so hosts cannot
/// accidentally pair a new map with an old selection. Clone retains the old
/// revision; a successful mutation makes every clone stale. No-op Apply leaves
/// the revision and redo intact. Drop cancels a draft without touching source.
#[derive(Clone, Debug)]
pub struct ProjectionTarget {
    projection: Projection,
    selection: ProjectionSelection,
    original: String,
}
impl ProjectionTarget {
    /// Captured document authority.
    pub fn document(&self) -> DocumentId {
        self.projection.document()
    }
    /// Captured monotonic revision.
    pub fn revision(&self) -> Revision {
        self.projection.revision()
    }
    /// Original directed display selection, including explicit syntax affinity.
    pub fn selection(&self) -> ProjectionSelection {
        self.selection
    }
    /// Exact selected source text to seed a native replacement field.
    pub fn original_text(&self) -> &str {
        &self.original
    }
    /// Recheck live identity, revision and bytes without applying a draft.
    pub fn validate(&self, session: &SourceSession) -> Result<(), ProjectionError> {
        self.projection.validate(session)
    }
    /// Apply through source admission/history. Unsupported or stale drafts have
    /// no side effects and are never silently normalized or recaptured.
    pub fn replace(
        &self,
        session: &mut SourceSession,
        replacement: &str,
        origin: EditOrigin,
    ) -> Result<Option<ChangeEvent>, ProjectionError> {
        self.projection
            .replace(session, self.selection, replacement, origin)
    }
}
/// Local projection IME draft, invalidated by any source revision change.
/// Cancel by dropping it. Mode changes must cancel before exposing another view.
#[derive(Clone, Debug)]
pub struct ProjectionComposition {
    projection: Projection,
    selection: ProjectionSelection,
    preedit: String,
}
impl ProjectionComposition {
    /// Replace uncommitted preedit; has no source/history effect.
    pub fn set_preedit(&mut self, text: impl Into<String>) {
        self.preedit = text.into();
    }
    /// Read the local preedit, never used as source offset geometry.
    pub fn preedit(&self) -> &str {
        &self.preedit
    }
    /// Native commit is exactly one source transaction; stale/unsupported refuses.
    pub fn commit(
        self,
        session: &mut SourceSession,
        text: &str,
    ) -> Result<Option<ChangeEvent>, ProjectionError> {
        self.projection
            .replace(session, self.selection, text, EditOrigin::Ime)
    }
}
