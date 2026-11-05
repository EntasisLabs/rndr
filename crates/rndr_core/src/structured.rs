//! Experimental E5 structured authority, independent of source storage and history.
//!
//! Positions address stable blocks, not normalized runs. Offsets are UTF-8 scalar
//! boundaries; native views additionally snap interactive positions to graphemes.
//! Runs have value identity only: merging equal marks cannot invalidate positions.
//! Replacement inherits marks immediately before its start (first run at offset 0).
//! Split keeps the left identity/kind and creates a paragraph on the right; joining
//! keeps the left identity/kind. Newline paste follows the same rule. History uses
//! explicit consecutive group tokens; navigation and composition are barriers.
use crate::editing::{Affinity, EditOrigin, Revision};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_DOCUMENT: AtomicU64 = AtomicU64::new(1);
/// Process-unique structured authority, never a source-document identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct StructuredDocumentId(pub u64);
/// Document-local block identity, never reused even after undo.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BlockId(pub u64);
/// E5 block schema. Heading levels are 1 through 6.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockKind {
    /// Ordinary editable paragraph.
    Paragraph,
    /// Heading with a schema-validated level in 1..=6.
    Heading(u8),
}
/// Canonical mark set. Links accept http(s) DNS/IPv4 hosts (optional numeric
/// port), simple mailto addresses, and nonempty local anchors. Credentials,
/// whitespace, controls, backslashes and HTML delimiters are refused. This small
/// safe schema is deliberately not a browser URL parser (IPv6 is not in E5).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Marks {
    /// Strong semantic emphasis.
    pub strong: bool,
    /// Semantic emphasis.
    pub emphasis: bool,
    /// Validated destination; None removes a link.
    pub link: Option<String>,
}
/// Canonical nonempty inline run (empty blocks have no runs).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Run {
    /// Nonempty UTF-8 text without structural newlines.
    pub text: String,
    /// Canonical mark set for all text in this run.
    pub marks: Marks,
}
/// Immutable through the document's public accessors.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Block {
    /// Stable document-local identity.
    pub id: BlockId,
    /// Paragraph or heading semantics.
    pub kind: BlockKind,
    /// Canonical runs; empty for an empty editable block.
    pub runs: Vec<Run>,
}
impl Block {
    /// Concatenate this block's run text (no structural newline).
    pub fn text(&self) -> String {
        self.runs.iter().map(|r| r.text.as_str()).collect()
    }
    /// UTF-8 text length in bytes.
    pub fn len(&self) -> usize {
        self.runs.iter().map(|r| r.text.len()).sum()
    }
    /// Whether this block has no text.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
/// UTF-8 scalar-boundary byte offset local to one block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Position {
    /// Stable containing block.
    pub block: BlockId,
    /// UTF-8 scalar-boundary byte offset within that block's concatenated text.
    pub offset: usize,
    /// Which side of inserted/replaced text this endpoint follows.
    pub affinity: Affinity,
}
/// Directed endpoints; order is document order rather than numeric ID order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextSelection {
    /// Fixed endpoint.
    pub anchor: Position,
    /// Moving endpoint; may precede anchor in document order.
    pub head: Position,
}
/// Whole-node selection is not an ambiguous text offset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StructuredSelection {
    /// Directed text range or collapsed caret.
    Text(TextSelection),
    /// One whole block.
    Node(BlockId),
}
/// Operations are sequential, each in the candidate state produced by its predecessor.
#[derive(Clone, Debug)]
pub enum Operation {
    /// Replace text across runs/blocks. Newlines create paragraph boundaries.
    Replace {
        /// Directed range in the current candidate state.
        range: TextSelection,
        /// Plain text, not Markdown or an authoritative source buffer.
        text: String,
    },
    /// Split into left block and a new right paragraph, preserving text marks.
    Split {
        /// Scalar boundary where the split occurs.
        at: Position,
    },
    /// Join a block with its immediate successor, retaining left kind/identity.
    Join {
        /// Left block; it must have a successor.
        left: BlockId,
    },
    /// Change a block's semantics without changing its identity or text.
    SetBlockKind {
        /// Target block.
        block: BlockId,
        /// Schema-valid paragraph/heading kind.
        kind: BlockKind,
    },
    /// Replace marks on a text range, preserving directed selection.
    SetMarks {
        /// Range; collapsed ranges are no-ops (no implicit stored marks).
        range: TextSelection,
        /// Complete canonical mark set, not a toggle/delta.
        marks: Marks,
    },
    /// Delete a whole block; deleting the last block creates an empty paragraph.
    DeleteNode {
        /// Block to remove.
        block: BlockId,
    },
}
/// Atomic optimistic transaction. Equal adjacent group tokens coalesce history.
#[derive(Clone, Debug)]
pub struct Transaction {
    /// Exact structured authority identity.
    pub document: StructuredDocumentId,
    /// Optimistic revision fence.
    pub base: Revision,
    /// Mutation provenance; does not override readonly/schema checks.
    pub origin: EditOrigin,
    /// Explicit coalescence token; None always creates an independent undo step.
    pub group: Option<u64>,
    /// Sequential operations validated as one candidate before any publication.
    pub operations: Vec<Operation>,
}
/// Atomic refusal; tree, history, revision and selection remain unchanged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StructuredError {
    /// Transaction targets another structured authority.
    WrongDocument,
    /// Base revision differs from the current revision.
    StaleRevision,
    /// Mutation is prohibited for all origins.
    Readonly,
    /// Missing block, invalid scalar boundary or impossible structural command.
    InvalidPosition,
    /// Invalid heading level or tree structure.
    InvalidSchema,
    /// Link destination violates the documented E5 attribute schema.
    InvalidLink,
    /// A captured composition range owns the mutation/selection boundary.
    CompositionActive,
    /// No captured composition exists to commit.
    NoComposition,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct State {
    blocks: Vec<Block>,
    selection: StructuredSelection,
}
#[derive(Clone, Debug)]
struct History {
    before: State,
    after: State,
    origin: EditOrigin,
}
#[derive(Clone, Copy, Debug)]
struct Composition {
    base: Revision,
    range: TextSelection,
}
/// Tree + directed selection + revision + history owned by one authority.
#[derive(Debug)]
pub struct StructuredDocument {
    id: StructuredDocumentId,
    revision: Revision,
    state: State,
    next_block: u64,
    readonly: bool,
    undo: Vec<History>,
    redo: Vec<History>,
    group: Option<u64>,
    composition: Option<Composition>,
}
impl StructuredDocument {
    /// Create an independent authority from plain text; CRLF/CR/LF form paragraphs.
    pub fn new(text: &str) -> Self {
        let text = normalize_plain_input(text);
        let blocks: Vec<_> = text
            .split('\n')
            .enumerate()
            .map(|(i, t)| Block {
                id: BlockId(i as u64 + 1),
                kind: BlockKind::Paragraph,
                runs: if t.is_empty() {
                    vec![]
                } else {
                    vec![Run {
                        text: t.into(),
                        marks: Marks::default(),
                    }]
                },
            })
            .collect();
        let p = Position {
            block: blocks[0].id,
            offset: 0,
            affinity: Affinity::After,
        };
        Self {
            id: StructuredDocumentId(NEXT_DOCUMENT.fetch_add(1, Ordering::Relaxed)),
            revision: Revision(0),
            next_block: blocks.len() as u64 + 1,
            state: State {
                blocks,
                selection: StructuredSelection::Text(TextSelection { anchor: p, head: p }),
            },
            readonly: false,
            undo: vec![],
            redo: vec![],
            group: None,
            composition: None,
        }
    }
    /// Process-unique structured identity.
    pub fn id(&self) -> StructuredDocumentId {
        self.id
    }
    /// Monotonic content revision, including undo/redo (not selection changes).
    pub fn revision(&self) -> Revision {
        self.revision
    }
    /// Canonical blocks in document order.
    pub fn blocks(&self) -> &[Block] {
        &self.state.blocks
    }
    /// Ephemeral layout/clipboard text, with one newline between adjacent blocks.
    pub fn plain_text(&self) -> String {
        self.state
            .blocks
            .iter()
            .map(Block::text)
            .collect::<Vec<_>>()
            .join("\n")
    }
    /// Current directed text or whole-node selection.
    pub fn selection(&self) -> StructuredSelection {
        self.state.selection
    }
    /// Text selection, or None for whole-node selection.
    pub fn text_selection(&self) -> Option<TextSelection> {
        match self.state.selection {
            StructuredSelection::Text(s) => Some(s),
            _ => None,
        }
    }
    /// Whether all content/history mutations are refused.
    pub fn is_readonly(&self) -> bool {
        self.readonly
    }
    /// Set readonly; enabling it also cancels any transient composition capture.
    pub fn set_readonly(&mut self, value: bool) {
        self.readonly = value;
        if value {
            self.cancel_composition();
        }
    }
    /// Whether a view has captured the composition mutation boundary.
    pub fn is_composing(&self) -> bool {
        self.composition.is_some()
    }
    /// Validate and set selection, even in readonly mode. Breaks history grouping.
    pub fn set_selection(&mut self, selection: StructuredSelection) -> Result<(), StructuredError> {
        if self.is_composing() {
            return Err(StructuredError::CompositionActive);
        }
        validate_selection(&self.state, selection)?;
        self.state.selection = selection;
        self.group = None;
        Ok(())
    }
    /// Display coordinates are ephemeral: callers must also retain the revision.
    pub fn display_offset(&self, position: Position) -> Result<usize, StructuredError> {
        offset(&self.state, position)
    }
    /// Map ephemeral display bytes to a typed block-local scalar position.
    pub fn position_at(
        &self,
        byte: usize,
        affinity: Affinity,
    ) -> Result<Position, StructuredError> {
        position_at(&self.state, byte, affinity)
    }
    /// Validate/publish one atomic transaction; true means content changed.
    /// No-op transactions publish neither selection changes nor new revisions.
    pub fn apply(&mut self, tx: Transaction) -> Result<bool, StructuredError> {
        if tx.document != self.id {
            return Err(StructuredError::WrongDocument);
        }
        if tx.base != self.revision {
            return Err(StructuredError::StaleRevision);
        }
        if self.readonly {
            return Err(StructuredError::Readonly);
        }
        if self.is_composing() {
            return Err(StructuredError::CompositionActive);
        }
        let mut candidate = self.state.clone();
        let mut next = self.next_block;
        for op in tx.operations {
            apply_operation(&mut candidate, &mut next, op)?;
        }
        normalize(&mut candidate)?;
        validate_selection(&candidate, candidate.selection)?;
        if candidate.blocks == self.state.blocks {
            return Ok(false);
        }
        let before = self.state.clone();
        if tx.group.is_some()
            && tx.group == self.group
            && self.undo.last().is_some_and(|h| h.origin == tx.origin)
        {
            self.undo.last_mut().unwrap().after = candidate.clone();
        } else {
            self.undo.push(History {
                before,
                after: candidate.clone(),
                origin: tx.origin,
            });
        }
        self.state = candidate;
        self.next_block = next;
        self.revision.0 += 1;
        self.redo.clear();
        self.group = tx.group;
        Ok(true)
    }
    /// Replace active text or whole-node contents and collapse to an end caret.
    pub fn replace_selection(
        &mut self,
        text: &str,
        origin: EditOrigin,
    ) -> Result<bool, StructuredError> {
        let operations = match self.selection() {
            StructuredSelection::Text(range) => vec![Operation::Replace {
                range,
                text: text.into(),
            }],
            StructuredSelection::Node(block) => {
                let b = self
                    .blocks()
                    .iter()
                    .find(|b| b.id == block)
                    .ok_or(StructuredError::InvalidPosition)?;
                let a = Position {
                    block,
                    offset: 0,
                    affinity: Affinity::After,
                };
                let h = Position {
                    offset: b.len(),
                    ..a
                };
                vec![Operation::Replace {
                    range: TextSelection { anchor: a, head: h },
                    text: text.into(),
                }]
            }
        };
        self.apply(Transaction {
            document: self.id,
            base: self.revision,
            origin,
            group: None,
            operations,
        })
    }
    /// Set the complete mark set on current text selection (empty range is no-op).
    pub fn set_marks(&mut self, marks: Marks, origin: EditOrigin) -> Result<bool, StructuredError> {
        let range = self
            .text_selection()
            .ok_or(StructuredError::InvalidPosition)?;
        self.apply(Transaction {
            document: self.id,
            base: self.revision,
            origin,
            group: None,
            operations: vec![Operation::SetMarks { range, marks }],
        })
    }
    /// Set kind on every selected block, preserving directed selection.
    pub fn set_block_kind(
        &mut self,
        kind: BlockKind,
        origin: EditOrigin,
    ) -> Result<bool, StructuredError> {
        let ids = match self.selection() {
            StructuredSelection::Node(id) => vec![id],
            StructuredSelection::Text(s) => {
                let (a, b) = ordered(&self.state, s)?;
                self.state.blocks[a.0..=b.0].iter().map(|b| b.id).collect()
            }
        };
        self.apply(Transaction {
            document: self.id,
            base: self.revision,
            origin,
            group: None,
            operations: ids
                .into_iter()
                .map(|block| Operation::SetBlockKind { block, kind })
                .collect(),
        })
    }
    /// Restore the previous tree/selection snapshot with a fresh content revision.
    pub fn undo(&mut self) -> Result<bool, StructuredError> {
        self.history(false)
    }
    /// Restore the next tree/selection snapshot with a fresh content revision.
    pub fn redo(&mut self) -> Result<bool, StructuredError> {
        self.history(true)
    }
    fn history(&mut self, redo: bool) -> Result<bool, StructuredError> {
        if self.readonly {
            return Err(StructuredError::Readonly);
        }
        if self.is_composing() {
            return Err(StructuredError::CompositionActive);
        }
        let from = if redo { &mut self.redo } else { &mut self.undo };
        let Some(h) = from.pop() else {
            return Ok(false);
        };
        self.state = if redo {
            h.after.clone()
        } else {
            h.before.clone()
        };
        self.revision.0 += 1;
        self.group = None;
        if redo {
            self.undo.push(h);
        } else {
            self.redo.push(h);
        }
        Ok(true)
    }
    /// Capture typed range; preedit text belongs to the view, never this tree.
    pub fn begin_composition(&mut self) -> Result<(), StructuredError> {
        if self.readonly {
            return Err(StructuredError::Readonly);
        }
        if self.is_composing() {
            return Err(StructuredError::CompositionActive);
        }
        let range = self
            .text_selection()
            .ok_or(StructuredError::InvalidPosition)?;
        self.composition = Some(Composition {
            base: self.revision,
            range,
        });
        self.group = None;
        Ok(())
    }
    /// Release composition capture without changing tree, revision or history.
    pub fn cancel_composition(&mut self) {
        self.composition = None;
        self.group = None;
    }
    /// Replace the captured typed range once, using insertion marks at its start.
    /// Successful content commit is exactly one undo step; failures retain capture.
    pub fn commit_composition(&mut self, text: &str) -> Result<bool, StructuredError> {
        let c = self.composition.ok_or(StructuredError::NoComposition)?;
        if c.base != self.revision {
            return Err(StructuredError::StaleRevision);
        }
        self.composition = None;
        let result = self.apply(Transaction {
            document: self.id,
            base: c.base,
            origin: EditOrigin::Ime,
            group: None,
            operations: vec![Operation::Replace {
                range: c.range,
                text: text.into(),
            }],
        });
        if result.is_err() {
            self.composition = Some(c);
        }
        result
    }
}
// Plain native/host input has canonical structural line endings. This is not a codec.
pub(crate) fn normalize_plain_input(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}
fn block_index(s: &State, id: BlockId) -> Result<usize, StructuredError> {
    s.blocks
        .iter()
        .position(|b| b.id == id)
        .ok_or(StructuredError::InvalidPosition)
}
fn local(s: &State, p: Position) -> Result<(usize, usize), StructuredError> {
    let i = block_index(s, p.block)?;
    let t = s.blocks[i].text();
    if p.offset > t.len() || !t.is_char_boundary(p.offset) {
        return Err(StructuredError::InvalidPosition);
    }
    Ok((i, p.offset))
}
type LocalPosition = (usize, usize);
fn ordered(s: &State, r: TextSelection) -> Result<(LocalPosition, LocalPosition), StructuredError> {
    let a = local(s, r.anchor)?;
    let b = local(s, r.head)?;
    Ok(if a <= b { (a, b) } else { (b, a) })
}
fn offset(s: &State, p: Position) -> Result<usize, StructuredError> {
    let (i, o) = local(s, p)?;
    Ok(s.blocks[..i].iter().map(|b| b.len() + 1).sum::<usize>() + o)
}
fn position_at(s: &State, mut n: usize, affinity: Affinity) -> Result<Position, StructuredError> {
    for b in &s.blocks {
        if n <= b.len() {
            let p = Position {
                block: b.id,
                offset: n,
                affinity,
            };
            local(s, p)?;
            return Ok(p);
        }
        n -= b.len() + 1;
    }
    Err(StructuredError::InvalidPosition)
}
fn validate_selection(s: &State, selection: StructuredSelection) -> Result<(), StructuredError> {
    match selection {
        StructuredSelection::Text(t) => {
            local(s, t.anchor)?;
            local(s, t.head)?;
        }
        StructuredSelection::Node(id) => {
            block_index(s, id)?;
        }
    }
    Ok(())
}
fn valid_marks(m: &Marks) -> Result<(), StructuredError> {
    if let Some(link) = &m.link {
        let valid = ["https://", "http://", "mailto:", "#"]
            .iter()
            .any(|p| link.starts_with(p) && link.len() > p.len());
        if !valid
            || link.chars().any(|c| {
                c.is_whitespace() || c.is_control() || matches!(c, '\\' | '<' | '>' | '\"' | '\'')
            })
        {
            return Err(StructuredError::InvalidLink);
        }
        if let Some(rest) = link
            .strip_prefix("https://")
            .or_else(|| link.strip_prefix("http://"))
        {
            let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
            let (host, port) = authority
                .split_once(':')
                .map_or((authority, None), |(h, p)| (h, Some(p)));
            if host.is_empty()
                || host.split('.').any(|part| {
                    part.is_empty()
                        || part.starts_with('-')
                        || part.ends_with('-')
                        || !part.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
                })
                || port.is_some_and(|p| p.is_empty() || p.parse::<u16>().is_err())
            {
                return Err(StructuredError::InvalidLink);
            }
        } else if let Some(address) = link.strip_prefix("mailto:") {
            let Some((local, domain)) = address.split_once('@') else {
                return Err(StructuredError::InvalidLink);
            };
            if local.is_empty()
                || domain.is_empty()
                || domain.contains('@')
                || address.contains(['?', '#', '/'])
            {
                return Err(StructuredError::InvalidLink);
            }
        }
    }
    Ok(())
}
fn normalize(s: &mut State) -> Result<(), StructuredError> {
    for b in &mut s.blocks {
        if matches!(b.kind,BlockKind::Heading(n) if !(1..=6).contains(&n)) {
            return Err(StructuredError::InvalidSchema);
        }
        let mut runs: Vec<Run> = vec![];
        for r in b.runs.drain(..) {
            valid_marks(&r.marks)?;
            if r.text.contains('\n') {
                return Err(StructuredError::InvalidSchema);
            }
            if r.text.is_empty() {
                continue;
            }
            if let Some(last) = runs.last_mut().filter(|last| last.marks == r.marks) {
                last.text.push_str(&r.text);
            } else {
                runs.push(r);
            }
        }
        b.runs = runs;
    }
    Ok(())
}
fn slice(b: &Block, start: usize, end: usize) -> Vec<Run> {
    let mut n = 0;
    let mut out = vec![];
    for r in &b.runs {
        let a = start.max(n);
        let z = end.min(n + r.text.len());
        if a < z {
            out.push(Run {
                text: r.text[a - n..z - n].into(),
                marks: r.marks.clone(),
            });
        }
        n += r.text.len();
    }
    out
}
fn insertion_marks(b: &Block, at: usize) -> Marks {
    let mut n = 0;
    for r in &b.runs {
        n += r.text.len();
        if at <= n {
            return r.marks.clone();
        }
    }
    Marks::default()
}
fn fresh(next: &mut u64) -> BlockId {
    let id = BlockId(*next);
    *next += 1;
    id
}
fn apply_operation(s: &mut State, next: &mut u64, op: Operation) -> Result<(), StructuredError> {
    match op {
        Operation::SetBlockKind { block, kind } => {
            let i = block_index(s, block)?;
            s.blocks[i].kind = kind;
        }
        Operation::SetMarks { range, marks } => {
            valid_marks(&marks)?;
            let (a, z) = ordered(s, range)?;
            for i in a.0..=z.0 {
                let b = &s.blocks[i];
                let start = if i == a.0 { a.1 } else { 0 };
                let end = if i == z.0 { z.1 } else { b.len() };
                let mut runs = slice(b, 0, start);
                let mut middle = slice(b, start, end);
                for r in &mut middle {
                    r.marks = marks.clone();
                }
                runs.extend(middle);
                runs.extend(slice(b, end, b.len()));
                s.blocks[i].runs = runs;
            }
        }
        Operation::Split { at } => {
            apply_operation(
                s,
                next,
                Operation::Replace {
                    range: TextSelection {
                        anchor: at,
                        head: at,
                    },
                    text: "\n".into(),
                },
            )?;
        }
        Operation::Join { left } => {
            let i = block_index(s, left)?;
            if i + 1 >= s.blocks.len() {
                return Err(StructuredError::InvalidPosition);
            }
            let a = Position {
                block: left,
                offset: s.blocks[i].len(),
                affinity: Affinity::After,
            };
            let z = Position {
                block: s.blocks[i + 1].id,
                offset: 0,
                affinity: Affinity::After,
            };
            apply_operation(
                s,
                next,
                Operation::Replace {
                    range: TextSelection { anchor: a, head: z },
                    text: String::new(),
                },
            )?;
        }
        Operation::Replace { range, text } => {
            let text = normalize_plain_input(&text);
            let (a, z) = ordered(s, range)?;
            let start = offset(
                s,
                Position {
                    block: s.blocks[a.0].id,
                    offset: a.1,
                    affinity: Affinity::Before,
                },
            )?;
            let end = offset(
                s,
                Position {
                    block: s.blocks[z.0].id,
                    offset: z.1,
                    affinity: Affinity::After,
                },
            )?;
            let old_selection = s.selection;
            let mapped = match old_selection {
                StructuredSelection::Text(r) => Some([
                    (offset(s, r.anchor)?, r.anchor.affinity),
                    (offset(s, r.head)?, r.head.affinity),
                ]),
                _ => None,
            };
            let marks = insertion_marks(&s.blocks[a.0], a.1);
            let prefix = slice(&s.blocks[a.0], 0, a.1);
            let suffix = slice(&s.blocks[z.0], z.1, s.blocks[z.0].len());
            let parts: Vec<_> = text.split('\n').collect();
            let mut new_blocks = vec![];
            for (i, part) in parts.iter().enumerate() {
                let mut runs = if i == 0 { prefix.clone() } else { vec![] };
                if !part.is_empty() {
                    runs.push(Run {
                        text: (*part).into(),
                        marks: marks.clone(),
                    });
                }
                if i + 1 == parts.len() {
                    runs.extend(suffix.clone());
                }
                new_blocks.push(Block {
                    id: if i == 0 {
                        s.blocks[a.0].id
                    } else {
                        fresh(next)
                    },
                    kind: if i == 0 {
                        s.blocks[a.0].kind
                    } else {
                        BlockKind::Paragraph
                    },
                    runs,
                });
            }
            let selected_node_replaced = match old_selection {
                StructuredSelection::Node(id) => s.blocks[a.0..=z.0].iter().any(|b| b.id == id),
                _ => false,
            };
            s.blocks.splice(a.0..=z.0, new_blocks);
            if let Some(m) = mapped {
                let map = |(n, f)| {
                    if n < start {
                        n
                    } else if n > end {
                        n - (end - start) + text.len()
                    } else if n == end && end > start {
                        start + text.len()
                    } else {
                        start + if f == Affinity::After { text.len() } else { 0 }
                    }
                };
                s.selection = StructuredSelection::Text(TextSelection {
                    anchor: position_at(s, map(m[0]), m[0].1)?,
                    head: position_at(s, map(m[1]), m[1].1)?,
                });
            } else if let StructuredSelection::Node(id) = old_selection {
                if selected_node_replaced || block_index(s, id).is_err() {
                    let p = position_at(s, start + text.len(), Affinity::After)?;
                    s.selection = StructuredSelection::Text(TextSelection { anchor: p, head: p });
                }
            }
            // Replacing the active range collapses it at the inserted text's end.
            if old_selection == StructuredSelection::Text(range) {
                let p = position_at(s, start + text.len(), Affinity::After)?;
                s.selection = StructuredSelection::Text(TextSelection { anchor: p, head: p });
            }
        }
        Operation::DeleteNode { block } => {
            let i = block_index(s, block)?;
            s.blocks.remove(i);
            if s.blocks.is_empty() {
                s.blocks.push(Block {
                    id: fresh(next),
                    kind: BlockKind::Paragraph,
                    runs: vec![],
                });
            }
            let b = &s.blocks[i.min(s.blocks.len() - 1)];
            let fallback = Position {
                block: b.id,
                offset: 0,
                affinity: Affinity::After,
            };
            s.selection = match s.selection {
                StructuredSelection::Node(id) if id == block => {
                    StructuredSelection::Text(TextSelection {
                        anchor: fallback,
                        head: fallback,
                    })
                }
                StructuredSelection::Text(mut r) => {
                    if r.anchor.block == block {
                        r.anchor = fallback;
                    }
                    if r.head.block == block {
                        r.head = fallback;
                    }
                    StructuredSelection::Text(r)
                }
                other => other,
            };
        }
    }
    normalize(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn p(d: &StructuredDocument, n: usize) -> Position {
        d.position_at(n, Affinity::After).unwrap()
    }
    fn range(d: &StructuredDocument, a: usize, b: usize) -> TextSelection {
        TextSelection {
            anchor: p(d, a),
            head: p(d, b),
        }
    }
    fn select(d: &mut StructuredDocument, a: usize, b: usize) {
        let r = range(d, a, b);
        d.set_selection(StructuredSelection::Text(r)).unwrap();
    }
    fn tx(d: &StructuredDocument, operations: Vec<Operation>) -> Transaction {
        Transaction {
            document: d.id(),
            base: d.revision(),
            origin: EditOrigin::Keyboard,
            group: None,
            operations,
        }
    }
    fn run(d: &mut StructuredDocument, ops: Vec<Operation>) -> Result<bool, StructuredError> {
        let t = tx(d, ops);
        d.apply(t)
    }
    fn invariant(d: &StructuredDocument) {
        assert!(!d.blocks().is_empty());
        for b in d.blocks() {
            for r in &b.runs {
                assert!(!r.text.is_empty());
                assert!(!r.text.contains('\n'));
                valid_marks(&r.marks).unwrap();
            }
            for pair in b.runs.windows(2) {
                assert_ne!(pair[0].marks, pair[1].marks);
            }
        }
        validate_selection(&d.state, d.selection()).unwrap();
        let text = d.plain_text();
        for i in 0..=text.len() {
            if text.is_char_boundary(i) {
                let pos = d.position_at(i, Affinity::After).unwrap();
                assert_eq!(d.display_offset(pos), Ok(i));
            }
        }
    }
    #[test]
    fn structured_utf8_positions_and_direction() {
        let mut d = StructuredDocument::new("é😀\nz");
        assert_eq!(
            d.position_at(1, Affinity::After),
            Err(StructuredError::InvalidPosition)
        );
        select(&mut d, 8, 2);
        assert_eq!(d.display_offset(d.text_selection().unwrap().anchor), Ok(8));
        d.replace_selection("Q", EditOrigin::Keyboard).unwrap();
        assert_eq!(d.plain_text(), "éQ");
        invariant(&d);
    }
    #[test]
    fn structured_atomic_refusals_preserve_everything() {
        let mut d = StructuredDocument::new("abc");
        let original = d.state.clone();
        let r = range(&d, 0, 2);
        let id = d.blocks()[0].id;
        let t = tx(
            &d,
            vec![
                Operation::Replace {
                    range: r,
                    text: "Z".into(),
                },
                Operation::SetBlockKind {
                    block: id,
                    kind: BlockKind::Heading(9),
                },
            ],
        );
        assert_eq!(d.apply(t), Err(StructuredError::InvalidSchema));
        assert_eq!(d.state, original);
        assert_eq!(d.revision(), Revision(0));
        assert!(d.undo.is_empty());
        let bad = Marks {
            link: Some("javascript:bad".into()),
            ..Marks::default()
        };
        assert_eq!(
            run(
                &mut d,
                vec![Operation::SetMarks {
                    range: r,
                    marks: bad
                }]
            ),
            Err(StructuredError::InvalidLink)
        );
        let mut t = tx(&d, vec![]);
        t.base = Revision(4);
        assert_eq!(d.apply(t), Err(StructuredError::StaleRevision));
        let mut t = tx(&d, vec![]);
        t.document = StructuredDocumentId(0);
        assert_eq!(d.apply(t), Err(StructuredError::WrongDocument));
        d.set_readonly(true);
        for origin in [
            EditOrigin::Keyboard,
            EditOrigin::Ime,
            EditOrigin::Host(0),
            EditOrigin::Reload,
        ] {
            let mut t = tx(&d, vec![]);
            t.origin = origin;
            assert_eq!(d.apply(t), Err(StructuredError::Readonly));
        }
        assert_eq!(d.undo(), Err(StructuredError::Readonly));
        assert_eq!(d.redo(), Err(StructuredError::Readonly));
        assert_eq!(d.state, original);
        invariant(&d);
    }
    #[test]
    fn structured_crossblock_delete_identity_and_inverse() {
        let mut d = StructuredDocument::new("alpha\nbeta\ngamma");
        let ids: Vec<_> = d.blocks().iter().map(|b| b.id).collect();
        select(&mut d, 2, 13);
        let before = d.state.clone();
        d.replace_selection("", EditOrigin::Keyboard).unwrap();
        assert_eq!(d.plain_text(), "almma");
        assert_eq!(d.blocks()[0].id, ids[0]);
        assert!(d.undo().unwrap());
        assert_eq!(d.state, before);
        assert_eq!(d.revision(), Revision(2));
        assert!(d.redo().unwrap());
        assert_eq!(d.plain_text(), "almma");
        invariant(&d);
    }
    #[test]
    fn structured_split_join_and_marks_are_inverse() {
        let mut d = StructuredDocument::new("hello");
        let id = d.blocks()[0].id;
        let r = range(&d, 0, 5);
        let marks = Marks {
            strong: true,
            emphasis: true,
            link: Some("https://example.org".into()),
        };
        run(
            &mut d,
            vec![Operation::SetMarks {
                range: r,
                marks: marks.clone(),
            }],
        )
        .unwrap();
        let before = d.state.clone();
        let at = p(&d, 2);
        run(&mut d, vec![Operation::Split { at }]).unwrap();
        assert_eq!(d.plain_text(), "he\nllo");
        assert_eq!(d.blocks()[1].runs[0].marks, marks);
        run(&mut d, vec![Operation::Join { left: id }]).unwrap();
        assert_eq!(d.blocks(), before.blocks);
        assert_eq!(d.blocks()[0].runs.len(), 1);
        d.undo().unwrap();
        d.undo().unwrap();
        assert_eq!(d.state, before);
        invariant(&d);
    }
    #[test]
    fn structured_normalization_preserves_typed_selection() {
        let mut d = StructuredDocument::new("abcdef");
        select(&mut d, 5, 1);
        let selection = d.selection();
        let r = range(&d, 2, 4);
        run(
            &mut d,
            vec![Operation::SetMarks {
                range: r,
                marks: Marks {
                    strong: true,
                    ..Marks::default()
                },
            }],
        )
        .unwrap();
        assert_eq!(d.blocks()[0].runs.len(), 3);
        assert_eq!(d.selection(), selection);
        run(
            &mut d,
            vec![Operation::SetMarks {
                range: r,
                marks: Marks::default(),
            }],
        )
        .unwrap();
        assert_eq!(d.blocks()[0].runs.len(), 1);
        assert_eq!(d.selection(), selection);
        let revision = d.revision();
        assert!(!run(
            &mut d,
            vec![Operation::SetMarks {
                range: r,
                marks: Marks::default()
            }]
        )
        .unwrap());
        assert_eq!(d.revision(), revision);
        invariant(&d);
    }
    #[test]
    fn structured_node_delete_empty_and_undo_identity() {
        let mut d = StructuredDocument::new("abc");
        let id = d.blocks()[0].id;
        d.set_selection(StructuredSelection::Node(id)).unwrap();
        let before = d.state.clone();
        run(&mut d, vec![Operation::DeleteNode { block: id }]).unwrap();
        assert_eq!(d.plain_text(), "");
        assert_ne!(d.blocks()[0].id, id);
        let new_id = d.blocks()[0].id;
        d.undo().unwrap();
        assert_eq!(d.state, before);
        d.redo().unwrap();
        assert_eq!(d.blocks()[0].id, new_id);
        invariant(&d);
    }
    #[test]
    fn structured_explicit_grouping_and_selection_barriers() {
        let mut d = StructuredDocument::new("");
        for text in ["a", "b"] {
            let r = d.text_selection().unwrap();
            let mut t = tx(
                &d,
                vec![Operation::Replace {
                    range: r,
                    text: text.into(),
                }],
            );
            t.group = Some(1);
            d.apply(t).unwrap();
        }
        assert_eq!(d.plain_text(), "ab");
        assert_eq!(d.undo.len(), 1);
        d.undo().unwrap();
        assert_eq!(d.plain_text(), "");
        d.redo().unwrap();
        assert_eq!(d.plain_text(), "ab");
        select(&mut d, 0, 0);
        let r = d.text_selection().unwrap();
        let mut t = tx(
            &d,
            vec![Operation::Replace {
                range: r,
                text: "c".into(),
            }],
        );
        t.group = Some(1);
        d.apply(t).unwrap();
        assert_eq!(d.undo.len(), 2);
        assert!(d.redo.is_empty());
        invariant(&d);
    }
    #[test]
    fn structured_composition_capture_mark_boundary_and_readonly() {
        let mut d = StructuredDocument::new("abCD");
        let r = range(&d, 0, 2);
        run(
            &mut d,
            vec![Operation::SetMarks {
                range: r,
                marks: Marks {
                    strong: true,
                    ..Marks::default()
                },
            }],
        )
        .unwrap();
        select(&mut d, 1, 3);
        let before = d.state.clone();
        let rev = d.revision();
        d.begin_composition().unwrap();
        assert_eq!(d.revision(), rev);
        assert_eq!(d.state, before);
        let t = tx(&d, vec![]);
        assert_eq!(d.apply(t), Err(StructuredError::CompositionActive));
        assert_eq!(d.undo(), Err(StructuredError::CompositionActive));
        assert_eq!(
            d.set_marks(Marks::default(), EditOrigin::Host(1)),
            Err(StructuredError::CompositionActive)
        );
        d.commit_composition("é").unwrap();
        assert_eq!(d.plain_text(), "aéD");
        assert!(d.blocks()[0].runs[0].marks.strong);
        d.undo().unwrap();
        assert_eq!(d.state, before);
        d.begin_composition().unwrap();
        d.set_readonly(true);
        assert!(!d.is_composing());
        assert_eq!(d.begin_composition(), Err(StructuredError::Readonly));
        invariant(&d);
    }
    #[test]
    fn structured_paste_newline_and_deleted_endpoint_mapping() {
        let mut d = StructuredDocument::new("abc\ndef\nghi");
        select(&mut d, 10, 0);
        let old_last = d.blocks()[2].id;
        let r = range(&d, 1, 6);
        run(
            &mut d,
            vec![Operation::Replace {
                range: r,
                text: "X\nY".into(),
            }],
        )
        .unwrap();
        assert_eq!(d.plain_text(), "aX\nYf\nghi");
        assert_eq!(d.blocks()[2].id, old_last);
        invariant(&d);
    }
    #[test]
    fn structured_link_schema_rejects_invalid_attributes_atomically() {
        let mut d = StructuredDocument::new("text");
        select(&mut d, 0, 4);
        for link in [
            "",
            "https://",
            "https://?q",
            "http://a:bogus",
            "http://a:999999",
            "http://user@host",
            "javascript:alert(0)",
            "data:x",
            "mailto:nobody",
            "mailto:@host",
            "#",
            "https://a\\\\b",
            "https://a/\"x",
            "https://a/ x",
        ] {
            let state = d.state.clone();
            let rev = d.revision();
            assert_eq!(
                d.set_marks(
                    Marks {
                        link: Some(link.into()),
                        ..Marks::default()
                    },
                    EditOrigin::Keyboard
                ),
                Err(StructuredError::InvalidLink),
                "{link}"
            );
            assert_eq!(d.state, state);
            assert_eq!(d.revision(), rev);
        }
        for link in [
            "https://example.org/a?q=x#part",
            "http://localhost:8080",
            "mailto:me@example.org",
            "#section",
        ] {
            d.set_marks(
                Marks {
                    link: Some(link.into()),
                    ..Marks::default()
                },
                EditOrigin::Keyboard,
            )
            .unwrap();
            invariant(&d);
        }
    }
    #[test]
    fn structured_node_replacement_becomes_text_caret() {
        let mut d = StructuredDocument::new("abc");
        let id = d.blocks()[0].id;
        d.set_selection(StructuredSelection::Node(id)).unwrap();
        d.replace_selection("x", EditOrigin::Keyboard).unwrap();
        assert_eq!(d.text_selection().unwrap(), range(&d, 1, 1));
        d.replace_selection("y", EditOrigin::Keyboard).unwrap();
        assert_eq!(d.plain_text(), "xy");
    }
    #[test]
    fn structured_kind_changes_identity_and_no_reuse_after_undo() {
        let mut d = StructuredDocument::new("a\nb");
        select(&mut d, 3, 0);
        let before = d.state.clone();
        d.set_block_kind(BlockKind::Heading(2), EditOrigin::Host(1))
            .unwrap();
        assert!(d.blocks().iter().all(|b| b.kind == BlockKind::Heading(2)));
        assert_eq!(d.selection(), before.selection);
        d.undo().unwrap();
        assert_eq!(d.state, before);
        let at = p(&d, 1);
        run(&mut d, vec![Operation::Split { at }]).unwrap();
        let first = d.blocks()[1].id;
        d.undo().unwrap();
        let at = p(&d, 1);
        run(&mut d, vec![Operation::Split { at }]).unwrap();
        assert_ne!(d.blocks()[1].id, first);
        invariant(&d);
    }
    #[test]
    fn structured_invalid_position_after_valid_op_rolls_back() {
        let mut d = StructuredDocument::new("é");
        let state = d.state.clone();
        let r = range(&d, 0, 2);
        let invalid = Position {
            block: d.blocks()[0].id,
            offset: 1,
            affinity: Affinity::After,
        };
        let t = tx(
            &d,
            vec![
                Operation::Replace {
                    range: r,
                    text: "😀".into(),
                },
                Operation::Split { at: invalid },
            ],
        );
        assert_eq!(d.apply(t), Err(StructuredError::InvalidPosition));
        assert_eq!(d.state, state);
        assert!(d.undo.is_empty());
        assert_eq!(d.next_block, 2);
    }
    #[test]
    fn structured_many_unicode_splices_roundtrip_inverse() {
        for original in ["", "é😀", "a\nb\nc", "e\u{301}👩‍💻\nx"] {
            let source = StructuredDocument::new(original);
            let bytes: Vec<_> = (0..=original.len())
                .filter(|&n| original.is_char_boundary(n))
                .collect();
            for &a in &bytes {
                for &b in &bytes {
                    for replacement in ["", "Q", "é\n😀"] {
                        let mut d = StructuredDocument::new(original);
                        select(&mut d, a, b);
                        let before = d.state.clone();
                        let changed = d
                            .replace_selection(replacement, EditOrigin::Keyboard)
                            .unwrap();
                        invariant(&d);
                        let expected = format!(
                            "{}{}{}",
                            &original[..a.min(b)],
                            replacement,
                            &original[a.max(b)..]
                        );
                        assert_eq!(d.plain_text(), expected);
                        if changed {
                            let after = d.state.clone();
                            d.undo().unwrap();
                            assert_eq!(d.state, before);
                            d.redo().unwrap();
                            assert_eq!(d.state, after);
                        }
                        invariant(&d);
                    }
                }
            }
            invariant(&source);
        }
    }
}
