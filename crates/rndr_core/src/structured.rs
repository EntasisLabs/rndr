//! Experimental E5 structured authority, independent of source storage and history.
//!
//! Positions address stable blocks, not normalized runs. Offsets are UTF-8 scalar
//! boundaries; native views additionally snap interactive positions to graphemes.
//! Runs have value identity only: merging equal marks cannot invalidate positions.
//! Replacement inherits marks immediately before its start (first run at offset 0).
//! Split keeps the left identity/kind and creates a paragraph on the right; joining
//! keeps the left identity/kind. Newline paste follows the same rule. History uses
//! explicit consecutive group tokens; navigation and composition are barriers.
mod table_ops;
mod text_ops;
use table_ops::{edit_table, TableAxis, TableEdit};

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
    /// A whole-node nontext boundary; text positions inside it are invalid.
    HorizontalRule,
    /// Host-rendered whole-node boundary; inert metadata lives in attributes.
    Atomic,
}
/// Stable identity of a structural container, independent of editable leaf IDs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ContainerId(pub u64);
/// List numbering policy; starts are positive and explicit, never inferred from paint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListKind {
    /// Unordered list.
    Bullet,
    /// Ordered list with an explicit first ordinal.
    Ordered {
        /// Positive ordinal of the first item.
        start: u32,
    },
    /// List of independently checked items.
    Task,
}
/// Structural nodes own children through their validated parent identities.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContainerKind {
    /// Rectangular editable table containing only rows.
    Table,
    /// Table row containing only cells.
    Row,
    /// Editable cell; header semantics are explicit, not paint inferred.
    Cell {
        /// Whether this is a header cell.
        header: bool,
    },
    /// Quote containing leaves or further containers.
    Quote,
    /// List containing only item containers.
    List(ListKind),
    /// Item containing leaves and nested lists/quotes; checked is Some only in task lists.
    Item {
        /// Task checked state; None for ordinary lists.
        checked: Option<bool>,
    },
}
/// Canonical structural container. Sibling order is induced by descendant leaf order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Container {
    /// Stable document-local identity.
    pub id: ContainerId,
    /// Parent container; None is the document root.
    pub parent: Option<ContainerId>,
    /// Schema-validated semantics.
    pub kind: ContainerKind,
}
/// Host-owned inert attributes, sorted by key and validated before publication.
/// Keys are ASCII identifiers (1..=64 bytes); values are control-free, <=1024 bytes.
/// No URL, code, annotation or footnote execution is implied.
pub type Attributes = std::collections::BTreeMap<String, String>;
/// Typed text style values; no CSS strings or executable expressions.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TextStyle {
    /// Optional opaque RGB color.
    pub color: Option<[u8; 3]>,
    /// Optional literal family name, 1..=80 bytes, no controls or CSS delimiters.
    pub family: Option<String>,
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
    /// Underline decoration.
    pub underline: bool,
    /// Strikethrough decoration.
    pub strike: bool,
    /// Inline code.
    pub code: bool,
    /// Atomic mention pill: an inert host id (1..=256 bytes, no controls or HTML
    /// delimiters); the run text is its label. Positions inside a pill are invalid,
    /// so carets, selections and deletions treat it as one unit, and adjacent pills
    /// never merge.
    pub pill: Option<String>,
    /// Validated destination; None removes a link.
    pub link: Option<String>,
    /// Validated presentational text styling; defaults inherit from the view.
    pub text_style: TextStyle,
}
/// One toggleable mark for [`StructuredDocument::toggle_mark`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkKind {
    /// [`Marks::strong`].
    Strong,
    /// [`Marks::emphasis`].
    Emphasis,
    /// [`Marks::underline`].
    Underline,
    /// [`Marks::strike`].
    Strike,
    /// [`Marks::code`].
    Code,
}
impl MarkKind {
    fn flag(self, marks: &mut Marks) -> &mut bool {
        match self {
            Self::Strong => &mut marks.strong,
            Self::Emphasis => &mut marks.emphasis,
            Self::Underline => &mut marks.underline,
            Self::Strike => &mut marks.strike,
            Self::Code => &mut marks.code,
        }
    }
    /// Whether `marks` has this mark.
    pub fn get(self, marks: &Marks) -> bool {
        *self.flag(&mut marks.clone())
    }
    /// Turn this mark on or off in `marks`.
    pub fn set(self, marks: &mut Marks, on: bool) {
        *self.flag(marks) = on;
    }
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
    /// Explicit structural parent, or document root.
    pub parent: Option<ContainerId>,
    /// Inert host metadata in deterministic key order.
    pub attributes: Attributes,
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
/// Permission-free atomic host payload. No network, URL resolution or child execution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AtomicPayload {
    /// Host extension name (for example `image`), nonempty ASCII identifier.
    pub kind: String,
    /// Human-readable clipboard/accessibility fallback, control-free, <=1024 bytes.
    pub fallback: String,
    /// Inert extension data; `atomic-kind` and `atomic-fallback` are reserved.
    pub attributes: Attributes,
}
impl AtomicPayload {
    fn encode(&self) -> Result<Attributes, StructuredError> {
        if self.kind.is_empty()
            || self.kind.len() > 64
            || !self
                .kind
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
            || self.attributes.contains_key("atomic-kind")
            || self.attributes.contains_key("atomic-fallback")
        {
            return Err(StructuredError::InvalidSchema);
        }
        let mut a = self.attributes.clone();
        a.insert("atomic-kind".into(), self.kind.clone());
        a.insert("atomic-fallback".into(), self.fallback.clone());
        valid_attributes(&a)?;
        Ok(a)
    }
}
impl Block {
    /// Decode a validated inert host payload; non-atomic blocks return None.
    pub fn atomic_payload(&self) -> Option<AtomicPayload> {
        if self.kind != BlockKind::Atomic {
            return None;
        }
        let mut attributes = self.attributes.clone();
        let kind = attributes.remove("atomic-kind")?;
        let fallback = attributes.remove("atomic-fallback")?;
        Some(AtomicPayload {
            kind,
            fallback,
            attributes,
        })
    }
    /// Whether text/IME positions are forbidden within this whole-node boundary.
    pub fn is_atomic(&self) -> bool {
        matches!(self.kind, BlockKind::HorizontalRule | BlockKind::Atomic)
    }
    /// Plain clipboard/accessibility fallback without executable host semantics.
    pub fn copy_fallback(&self) -> String {
        self.atomic_payload()
            .map_or_else(|| self.text(), |p| p.fallback)
    }
}
/// A rectangular rich clipboard snapshot. IDs are values, never destination identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TableFragment {
    /// Nonempty equal-width rows of rich cells.
    pub rows: Vec<Vec<CellFragment>>,
}
/// Rich cell content retaining marks, nested structure and inert atom metadata.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CellFragment {
    /// Header semantics of the copied cell.
    pub header: bool,
    /// Descendant leaves; direct cell children have parent None in this snapshot.
    pub blocks: Vec<Block>,
    /// Descendant containers, with direct cell children parented to None.
    pub containers: Vec<Container>,
}
/// Canonical table membership and row-major coordinates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CellAddress {
    /// Stable containing table.
    pub table: ContainerId,
    /// Stable cell.
    pub cell: ContainerId,
    /// Zero-based row.
    pub row: usize,
    /// Zero-based column.
    pub column: usize,
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
    /// Directed rectangular cell selection; both endpoints belong to one table.
    Cells {
        /// Fixed cell identity.
        anchor: ContainerId,
        /// Moving cell identity.
        head: ContainerId,
    },
}
/// Operations are sequential, each in the candidate state produced by its predecessor.
#[derive(Clone, Debug)]
pub enum Operation {
    /// Insert a rectangular table between the split caret leaves.
    InsertTable {
        /// Editable insertion position, outside another table.
        at: Position,
        /// Positive row count, at most 256.
        rows: usize,
        /// Positive column count, at most 64.
        columns: usize,
        /// Whether the first row consists of header cells.
        header: bool,
    },
    /// Insert a row at index (including the trailing boundary).
    InsertRow {
        /// Stable table identity.
        table: ContainerId,
        /// Zero-based insertion index.
        index: usize,
    },
    /// Delete a row; deleting the last row removes the table.
    DeleteRow {
        /// Stable table identity.
        table: ContainerId,
        /// Zero-based row index.
        index: usize,
    },
    /// Insert a column at index (including the trailing boundary).
    InsertColumn {
        /// Stable table identity.
        table: ContainerId,
        /// Zero-based insertion index.
        index: usize,
    },
    /// Delete a column; deleting the last column removes the table.
    DeleteColumn {
        /// Stable table identity.
        table: ContainerId,
        /// Zero-based column index.
        index: usize,
    },
    /// Clear a selected cell rectangle and type plain text into its upper-left cell.
    ReplaceCells {
        /// Fixed rectangle endpoint.
        anchor: ContainerId,
        /// Moving rectangle endpoint.
        head: ContainerId,
        /// Plain text (newlines create paragraphs within the first cell).
        text: String,
    },
    /// Replace a fitting rectangle of cells with a rich inert fragment.
    PasteCells {
        /// Upper-left destination cell.
        at: ContainerId,
        /// Value snapshot, remapped to fresh descendant identities on paste.
        fragment: TableFragment,
    },
    /// Split at the caret and insert a selected nontext host boundary.
    InsertAtomic {
        /// Editable insertion position.
        at: Position,
        /// Permission-free host metadata, never executed by the model.
        payload: AtomicPayload,
    },
    /// Commit host/nested state through ordinary parent transaction history.
    UpdateAtomic {
        /// Stable atomic block identity.
        block: BlockId,
        /// Complete replacement metadata.
        payload: AtomicPayload,
    },
    /// Wrap selected sibling leaves/subtrees in a quote or list (not a bare item).
    Wrap {
        /// Leaf range; all intersecting leaves are included.
        range: TextSelection,
        /// Quote or List semantics.
        kind: ContainerKind,
    },
    /// Nest an entire item under its previous sibling; nonitems are refused.
    Indent {
        /// Any leaf in the item.
        block: BlockId,
    },
    /// Promote an entire item, or split its top-level list around the exited item.
    Outdent {
        /// Any leaf in the item/quote.
        block: BlockId,
    },
    /// Toggle a task item's checked state without modifying text or selection.
    ToggleTask {
        /// Any leaf within the nearest task item, including quoted content.
        block: BlockId,
    },
    /// Replace inert block metadata with schema-validated canonical attributes.
    SetAttributes {
        /// Target leaf.
        block: BlockId,
        /// Complete attribute set.
        attributes: Attributes,
    },
    /// Split at a text caret and insert a whole-node horizontal rule between leaves.
    InsertHorizontalRule {
        /// Editable insertion boundary.
        at: Position,
    },
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
    containers: Vec<Container>,
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
    policy_epoch: u64,
    atomic_lifetimes: std::collections::HashMap<BlockId, u64>,
    next_atomic_lifetime: u64,
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
                parent: None,
                attributes: Attributes::new(),
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
                containers: vec![],
                selection: StructuredSelection::Text(TextSelection { anchor: p, head: p }),
            },
            readonly: false,
            policy_epoch: 0,
            atomic_lifetimes: Default::default(),
            next_atomic_lifetime: 1,
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
    /// Structural containers. Parent links plus leaf order define the authoritative tree.
    pub fn containers(&self) -> &[Container] {
        &self.state.containers
    }
    /// Ancestors from immediate parent to root, useful for typed structural chrome.
    pub fn ancestors(&self, block: BlockId) -> Result<Vec<&Container>, StructuredError> {
        let mut parent = self.state.blocks[block_index(&self.state, block)?].parent;
        let mut result = vec![];
        while let Some(id) = parent {
            let c = container(&self.state, id)?;
            result.push(c);
            parent = c.parent;
        }
        Ok(result)
    }
    /// Ephemeral layout/clipboard text, with one newline between adjacent blocks.
    /// Horizontal rules contribute no editable bytes; consumers must fence them as nodes.
    pub fn plain_text(&self) -> String {
        self.state
            .blocks
            .iter()
            .map(Block::text)
            .collect::<Vec<_>>()
            .join("\n")
    }
    /// Ordered ordinal for the nearest containing item, or None for nonordered content.
    /// Explicit starts and sibling item order survive splits, outdents and undo.
    pub fn list_number(&self, block: BlockId) -> Result<Option<u32>, StructuredError> {
        block_index(&self.state, block)?;
        let Ok(item) = item_for(&self.state, block) else {
            return Ok(None);
        };
        let list = container(&self.state, item)?
            .parent
            .ok_or(StructuredError::InvalidSchema)?;
        let ContainerKind::List(ListKind::Ordered { start }) = container(&self.state, list)?.kind
        else {
            return Ok(None);
        };
        let first = subtree_bounds(&self.state, item)?.0;
        let count = self
            .state
            .containers
            .iter()
            .filter(|c| c.parent == Some(list))
            .filter(|c| subtree_bounds(&self.state, c.id).is_ok_and(|(n, _)| n < first))
            .count();
        Ok(Some(
            start
                .checked_add(count as u32)
                .ok_or(StructuredError::InvalidSchema)?,
        ))
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
        if self.readonly != value {
            self.policy_epoch += 1;
        }
        self.readonly = value;
        if value {
            self.cancel_composition();
        }
    }
    /// Monotonic policy/composition fence, outside document history.
    pub fn policy_epoch(&self) -> u64 {
        self.policy_epoch
    }
    /// Live retained incarnation of an atomic block. Undo restores model identity,
    /// never the previous retained child or its asynchronous callbacks.
    pub fn atomic_lifetime(&self, block: BlockId) -> Option<u64> {
        self.atomic_lifetimes.get(&block).copied()
    }
    fn refresh_atomic_lifetimes(&mut self) {
        self.atomic_lifetimes.retain(|id, _| {
            self.state
                .blocks
                .iter()
                .any(|b| b.id == *id && b.kind == BlockKind::Atomic)
        });
        for block in &self.state.blocks {
            if block.kind == BlockKind::Atomic && !self.atomic_lifetimes.contains_key(&block.id) {
                let next = self.next_atomic_lifetime;
                self.next_atomic_lifetime = next.checked_add(1).expect("atomic lifetime exhausted");
                self.atomic_lifetimes.insert(block.id, next);
            }
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
        if candidate.blocks == self.state.blocks && candidate.containers == self.state.containers {
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
        self.refresh_atomic_lifetimes();
        self.next_block = next;
        self.revision.0 += 1;
        self.redo.clear();
        self.group = tx.group;
        Ok(true)
    }
    fn command(
        &mut self,
        operations: Vec<Operation>,
        origin: EditOrigin,
    ) -> Result<bool, StructuredError> {
        self.apply(Transaction {
            document: self.id,
            base: self.revision,
            origin,
            group: None,
            operations,
        })
    }
    /// Wrap the selected sibling leaves in a quote or list. Nested selection must
    /// address complete direct siblings; partial subtrees are refused atomically.
    pub fn wrap_selection(
        &mut self,
        kind: ContainerKind,
        origin: EditOrigin,
    ) -> Result<bool, StructuredError> {
        let range = self
            .text_selection()
            .ok_or(StructuredError::InvalidPosition)?;
        self.command(vec![Operation::Wrap { range, kind }], origin)
    }
    /// Short command spelling for wrapping the active selection.
    pub fn wrap(
        &mut self,
        kind: ContainerKind,
        origin: EditOrigin,
    ) -> Result<bool, StructuredError> {
        self.wrap_selection(kind, origin)
    }
    /// Insert a nontext rule at the active collapsed text caret, selecting the rule.
    /// A noncollapsed range is refused; callers may replace it in an atomic transaction.
    pub fn insert_horizontal_rule(&mut self, origin: EditOrigin) -> Result<bool, StructuredError> {
        let range = self
            .text_selection()
            .ok_or(StructuredError::InvalidPosition)?;
        if range.anchor.block != range.head.block || range.anchor.offset != range.head.offset {
            return Err(StructuredError::InvalidPosition);
        }
        self.command(
            vec![Operation::InsertHorizontalRule { at: range.head }],
            origin,
        )
    }
    /// Indent the selected item's whole subtree under its preceding sibling.
    pub fn indent(&mut self, origin: EditOrigin) -> Result<bool, StructuredError> {
        let block = self.command_block()?;
        self.command(vec![Operation::Indent { block }], origin)
    }
    /// Outdent the selected item's subtree, or exit a quote/list at root level.
    pub fn outdent(&mut self, origin: EditOrigin) -> Result<bool, StructuredError> {
        let block = self.command_block()?;
        self.command(vec![Operation::Outdent { block }], origin)
    }
    /// Toggle the selected task item, preserving text and directed selection.
    pub fn toggle_task(&mut self, origin: EditOrigin) -> Result<bool, StructuredError> {
        let block = self.command_block()?;
        self.command(vec![Operation::ToggleTask { block }], origin)
    }
    fn command_block(&self) -> Result<BlockId, StructuredError> {
        Ok(match self.selection() {
            StructuredSelection::Node(id) => id,
            StructuredSelection::Cells { .. } => return Err(StructuredError::InvalidPosition),
            StructuredSelection::Text(r) => r.head.block,
        })
    }
    /// Enter splits an ordinary/list leaf; an empty item exits one nesting level.
    pub fn enter(&mut self, origin: EditOrigin) -> Result<bool, StructuredError> {
        let r = self
            .text_selection()
            .ok_or(StructuredError::InvalidPosition)?;
        let b = &self.state.blocks[block_index(&self.state, r.head.block)?];
        if r.anchor == r.head
            && b.is_empty()
            && b.parent.is_some()
            && !matches!(
                b.parent
                    .and_then(|p| container(&self.state, p).ok())
                    .map(|c| c.kind),
                Some(ContainerKind::Cell { .. })
            )
        {
            return self.outdent(origin);
        }
        self.command(
            vec![Operation::Replace {
                range: r,
                text: "\n".into(),
            }],
            origin,
        )
    }
    /// Backspace at a structural start outdents first. Adjacent HR is selected,
    /// never treated as an editable byte. Character deletion uses grapheme boundaries.
    pub fn backspace(&mut self, origin: EditOrigin) -> Result<bool, StructuredError> {
        let Some(r) = self.text_selection() else {
            return self.replace_selection("", origin);
        };
        if r.anchor != r.head {
            return self.replace_selection("", origin);
        }
        let (i, o) = local(&self.state, r.head)?;
        if o == 0 {
            if self.state.blocks[i].parent.is_some()
                && !matches!(
                    self.state.blocks[i]
                        .parent
                        .and_then(|p| container(&self.state, p).ok())
                        .map(|c| c.kind),
                    Some(ContainerKind::Cell { .. })
                )
            {
                return self.outdent(origin);
            }
            if i == 0 {
                return self.command(vec![], origin);
            }
            if self.state.blocks[i - 1].is_atomic() {
                if self.readonly {
                    return Err(StructuredError::Readonly);
                }
                self.set_selection(StructuredSelection::Node(self.state.blocks[i - 1].id))?;
                return Ok(false);
            }
            return self.command(
                vec![Operation::Join {
                    left: self.state.blocks[i - 1].id,
                }],
                origin,
            );
        }
        use unicode_segmentation::UnicodeSegmentation;
        let text = self.state.blocks[i].text();
        let start = text[..o]
            .grapheme_indices(true)
            .next_back()
            .map_or(0, |(n, _)| n);
        let start = pill_at(&self.state.blocks[i], start).map_or(start, |(a, _)| a);
        self.command(
            vec![Operation::Replace {
                range: TextSelection {
                    anchor: Position {
                        offset: start,
                        ..r.head
                    },
                    head: r.head,
                },
                text: String::new(),
            }],
            origin,
        )
    }
    /// Delete forward by one grapheme or join with the next text leaf. An
    /// adjacent HR receives whole-node selection before any destructive action.
    pub fn delete_forward(&mut self, origin: EditOrigin) -> Result<bool, StructuredError> {
        let Some(r) = self.text_selection() else {
            return self.replace_selection("", origin);
        };
        if r.anchor != r.head {
            return self.replace_selection("", origin);
        }
        let (i, o) = local(&self.state, r.head)?;
        if o == self.state.blocks[i].len() {
            if i + 1 == self.state.blocks.len() {
                return self.command(vec![], origin);
            }
            if self.state.blocks[i + 1].is_atomic() {
                if self.readonly {
                    return Err(StructuredError::Readonly);
                }
                self.set_selection(StructuredSelection::Node(self.state.blocks[i + 1].id))?;
                return Ok(false);
            }
            return self.command(
                vec![Operation::Join {
                    left: self.state.blocks[i].id,
                }],
                origin,
            );
        }
        use unicode_segmentation::UnicodeSegmentation;
        let text = self.state.blocks[i].text();
        let end = o + text[o..].graphemes(true).next().unwrap().len();
        let end = pill_at(&self.state.blocks[i], end).map_or(end, |(_, z)| z);
        self.command(
            vec![Operation::Replace {
                range: TextSelection {
                    anchor: r.head,
                    head: Position {
                        offset: end,
                        ..r.head
                    },
                },
                text: String::new(),
            }],
            origin,
        )
    }
    /// Replace active text or whole-node contents and collapse to an end caret.
    pub fn replace_selection(
        &mut self,
        text: &str,
        origin: EditOrigin,
    ) -> Result<bool, StructuredError> {
        let operations = match self.selection() {
            StructuredSelection::Cells { anchor, head } => vec![Operation::ReplaceCells {
                anchor,
                head,
                text: text.into(),
            }],
            StructuredSelection::Text(range) => vec![Operation::Replace {
                range,
                text: text.into(),
            }],
            StructuredSelection::Node(block)
                if self.blocks().iter().any(|b| b.id == block && b.is_atomic()) =>
            {
                // Whole HR replacement removes the node, then inserts into repaired caret.
                if self.readonly {
                    return Err(StructuredError::Readonly);
                }
                if self.is_composing() {
                    return Err(StructuredError::CompositionActive);
                }
                let mut candidate = self.state.clone();
                let mut next = self.next_block;
                apply_operation(&mut candidate, &mut next, Operation::DeleteNode { block })?;
                let StructuredSelection::Text(range) = candidate.selection else {
                    return Err(StructuredError::InvalidPosition);
                };
                return self.command(
                    vec![
                        Operation::DeleteNode { block },
                        Operation::Replace {
                            range,
                            text: text.into(),
                        },
                    ],
                    origin,
                );
            }
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
    /// Toggle one mark across the text selection, keeping every other mark
    /// (links, pills, styles) per run. It turns off only when every selected run has it.
    pub fn toggle_mark(&mut self, kind: MarkKind, origin: EditOrigin) -> Result<bool, StructuredError> {
        let range = self
            .text_selection()
            .ok_or(StructuredError::InvalidPosition)?;
        let (first, last) = ordered(&self.state, range)?;
        let mut segments = vec![];
        for block in &self.state.blocks[first.0..=last.0] {
            let index = block_index(&self.state, block.id)?;
            let start = if index == first.0 { first.1 } else { 0 };
            let end = if index == last.0 { last.1 } else { block.len() };
            let mut n = 0;
            for run in &block.runs {
                let (a, z) = (start.max(n), end.min(n + run.text.len()));
                if a < z {
                    segments.push((block.id, a, z, run.marks.clone()));
                }
                n += run.text.len();
            }
        }
        if segments.is_empty() {
            return Ok(false);
        }
        let on = !segments.iter().all(|(.., marks)| kind.get(marks));
        let at = |block, offset| Position {
            block,
            offset,
            affinity: Affinity::After,
        };
        let operations = segments
            .into_iter()
            .map(|(block, a, z, mut marks)| {
                kind.set(&mut marks, on);
                Operation::SetMarks {
                    range: TextSelection {
                        anchor: at(block, a),
                        head: at(block, z),
                    },
                    marks,
                }
            })
            .collect();
        self.command(operations, origin)
    }
    /// Replace the text selection with one atomic mention pill labelled `label`
    /// and leave the caret after it. Typing there does not extend the pill.
    pub fn insert_pill(
        &mut self,
        label: &str,
        id: &str,
        origin: EditOrigin,
    ) -> Result<bool, StructuredError> {
        if label.is_empty() || label.contains(['\n', '\r']) {
            return Err(StructuredError::InvalidSchema);
        }
        let range = self
            .text_selection()
            .ok_or(StructuredError::InvalidPosition)?;
        let (first, _) = ordered(&self.state, range)?;
        let block = self.state.blocks[first.0].id;
        let marks = Marks {
            pill: Some(id.into()),
            ..insertion_marks(&self.state.blocks[first.0], first.1)
        };
        let at = |offset| Position {
            block,
            offset,
            affinity: Affinity::After,
        };
        self.command(
            vec![
                Operation::Replace {
                    range,
                    text: label.into(),
                },
                Operation::SetMarks {
                    range: TextSelection {
                        anchor: at(first.1),
                        head: at(first.1 + label.len()),
                    },
                    marks,
                },
            ],
            origin,
        )
    }
    /// Set kind on every selected block, preserving directed selection.
    pub fn set_block_kind(
        &mut self,
        kind: BlockKind,
        origin: EditOrigin,
    ) -> Result<bool, StructuredError> {
        let ids = match self.selection() {
            StructuredSelection::Cells { .. } => return Err(StructuredError::InvalidPosition),
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
        self.refresh_atomic_lifetimes();
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
        self.policy_epoch += 1;
        self.composition = Some(Composition {
            base: self.revision,
            range,
        });
        self.group = None;
        Ok(())
    }
    /// Release composition capture without changing tree, revision or history.
    pub fn cancel_composition(&mut self) {
        if self.composition.is_some() {
            self.policy_epoch += 1;
        }
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
// Table order always derives from descendant leaf order, never creation/ID order.
fn children(s: &State, parent: ContainerId) -> Result<Vec<ContainerId>, StructuredError> {
    let mut children = s
        .containers
        .iter()
        .filter(|c| c.parent == Some(parent))
        .map(|c| Ok((subtree_bounds(s, c.id)?.0, c.id)))
        .collect::<Result<Vec<_>, StructuredError>>()?;
    children.sort_by_key(|c| c.0);
    Ok(children.into_iter().map(|c| c.1).collect())
}
fn table_grid(s: &State, table: ContainerId) -> Result<Vec<Vec<ContainerId>>, StructuredError> {
    if container(s, table)?.kind != ContainerKind::Table {
        return Err(StructuredError::InvalidSchema);
    }
    children(s, table)?
        .into_iter()
        .map(|r| children(s, r))
        .collect()
}
fn cell_for(s: &State, block: BlockId) -> Result<Option<ContainerId>, StructuredError> {
    let mut parent = s.blocks[block_index(s, block)?].parent;
    for _ in 0..=s.containers.len() {
        let Some(p) = parent else {
            return Ok(None);
        };
        let c = container(s, p)?;
        if matches!(c.kind, ContainerKind::Cell { .. }) {
            return Ok(Some(p));
        }
        parent = c.parent;
    }
    Err(StructuredError::InvalidSchema)
}
fn address(s: &State, cell: ContainerId) -> Result<CellAddress, StructuredError> {
    if !matches!(container(s, cell)?.kind, ContainerKind::Cell { .. }) {
        return Err(StructuredError::InvalidPosition);
    }
    let row = container(s, cell)?
        .parent
        .ok_or(StructuredError::InvalidSchema)?;
    let table = container(s, row)?
        .parent
        .ok_or(StructuredError::InvalidSchema)?;
    for (r, cells) in table_grid(s, table)?.iter().enumerate() {
        if let Some(column) = cells.iter().position(|c| *c == cell) {
            return Ok(CellAddress {
                table,
                cell,
                row: r,
                column,
            });
        }
    }
    Err(StructuredError::InvalidSchema)
}
type CellRectangle = (ContainerId, usize, usize, usize, usize);
fn cell_rectangle(
    s: &State,
    anchor: ContainerId,
    head: ContainerId,
) -> Result<CellRectangle, StructuredError> {
    let a = address(s, anchor)?;
    let h = address(s, head)?;
    if a.table != h.table {
        return Err(StructuredError::InvalidPosition);
    }
    Ok((
        a.table,
        a.row.min(h.row),
        a.column.min(h.column),
        a.row.max(h.row),
        a.column.max(h.column),
    ))
}
fn validate_tables(s: &State) -> Result<(), StructuredError> {
    for c in &s.containers {
        let parent = c
            .parent
            .map(|p| container(s, p).map(|p| p.kind))
            .transpose()?;
        match c.kind {
            ContainerKind::Row if parent != Some(ContainerKind::Table) => {
                return Err(StructuredError::InvalidSchema)
            }
            ContainerKind::Cell { .. } if parent != Some(ContainerKind::Row) => {
                return Err(StructuredError::InvalidSchema)
            }
            ContainerKind::Table => {
                // Nested tables are deliberately excluded in this schema slice.
                let mut p = c.parent;
                while let Some(id) = p {
                    let ancestor = container(s, id)?;
                    if matches!(
                        ancestor.kind,
                        ContainerKind::Table | ContainerKind::Cell { .. } | ContainerKind::Row
                    ) {
                        return Err(StructuredError::InvalidSchema);
                    }
                    p = ancestor.parent;
                }
                let grid = table_grid(s, c.id)?;
                if grid.is_empty()
                    || grid.len() > 256
                    || grid[0].is_empty()
                    || grid[0].len() > 64
                    || grid.iter().any(|r| r.len() != grid[0].len())
                {
                    return Err(StructuredError::InvalidSchema);
                }
            }
            _ => {}
        }
        if (parent == Some(ContainerKind::Table) && c.kind != ContainerKind::Row)
            || (parent == Some(ContainerKind::Row) && !matches!(c.kind, ContainerKind::Cell { .. }))
        {
            return Err(StructuredError::InvalidSchema);
        }
        if matches!(c.kind, ContainerKind::Cell { .. })
            && !s.blocks.iter().any(|b| descends(s, b.parent, c.id))
        {
            return Err(StructuredError::InvalidSchema);
        }
    }
    for b in &s.blocks {
        if let Some(p) = b.parent {
            if matches!(
                container(s, p)?.kind,
                ContainerKind::Table | ContainerKind::Row
            ) {
                return Err(StructuredError::InvalidSchema);
            }
        }
    }
    Ok(())
}
fn empty_leaf(next: &mut u64, parent: Option<ContainerId>) -> Block {
    Block {
        id: fresh(next),
        kind: BlockKind::Paragraph,
        parent,
        attributes: Attributes::new(),
        runs: vec![],
    }
}
fn select_cell(s: &mut State, cell: ContainerId) -> Result<(), StructuredError> {
    let b = s
        .blocks
        .iter()
        .find(|b| descends(s, b.parent, cell))
        .ok_or(StructuredError::InvalidSchema)?;
    s.selection = if b.is_atomic() {
        StructuredSelection::Node(b.id)
    } else {
        let p = Position {
            block: b.id,
            offset: 0,
            affinity: Affinity::After,
        };
        StructuredSelection::Text(TextSelection { anchor: p, head: p })
    };
    Ok(())
}
fn insert_table(
    s: &mut State,
    next: &mut u64,
    at: Position,
    rows: usize,
    columns: usize,
    header: bool,
) -> Result<(), StructuredError> {
    if rows == 0 || rows > 256 || columns == 0 || columns > 64 || cell_for(s, at.block)?.is_some() {
        return Err(StructuredError::InvalidSchema);
    }
    apply_operation(s, next, Operation::Split { at })?;
    let i = block_index(s, at.block)?;
    let table = fresh_container(s, next, s.blocks[i].parent, ContainerKind::Table);
    let mut leaves = vec![];
    let mut first = None;
    for r in 0..rows {
        let row = fresh_container(s, next, Some(table), ContainerKind::Row);
        for _ in 0..columns {
            let cell = fresh_container(
                s,
                next,
                Some(row),
                ContainerKind::Cell {
                    header: header && r == 0,
                },
            );
            first.get_or_insert(cell);
            leaves.push(empty_leaf(next, Some(cell)));
        }
    }
    s.blocks.splice(i + 1..i + 1, leaves);
    select_cell(s, first.unwrap())
}
fn repair_table_selection(
    s: &mut State,
    next: &mut u64,
    near: usize,
) -> Result<(), StructuredError> {
    if validate_selection(s, s.selection).is_ok() {
        return Ok(());
    }
    if !s.blocks.iter().any(|b| !b.is_atomic()) {
        s.blocks.push(empty_leaf(next, None));
    }
    let b = s
        .blocks
        .iter()
        .skip(near)
        .find(|b| !b.is_atomic())
        .or_else(|| s.blocks.iter().take(near).rev().find(|b| !b.is_atomic()))
        .ok_or(StructuredError::InvalidSchema)?;
    let p = Position {
        block: b.id,
        offset: 0,
        affinity: Affinity::After,
    };
    s.selection = StructuredSelection::Text(TextSelection { anchor: p, head: p });
    Ok(())
}
fn remove_subtrees(s: &mut State, ids: &[ContainerId]) {
    let leaves: std::collections::HashSet<_> = s
        .blocks
        .iter()
        .filter(|b| ids.iter().any(|id| descends(s, b.parent, *id)))
        .map(|b| b.id)
        .collect();
    let containers: std::collections::HashSet<_> = s
        .containers
        .iter()
        .filter(|c| ids.contains(&c.id) || ids.iter().any(|id| descends(s, c.parent, *id)))
        .map(|c| c.id)
        .collect();
    s.blocks.retain(|b| !leaves.contains(&b.id));
    s.containers.retain(|c| !containers.contains(&c.id));
}
fn paste_cells(
    s: &mut State,
    next: &mut u64,
    at: ContainerId,
    fragment: TableFragment,
) -> Result<(), StructuredError> {
    let a = address(s, at)?;
    let grid = table_grid(s, a.table)?;
    let height = fragment.rows.len();
    let width = fragment.rows.first().map_or(0, Vec::len);
    if height == 0
        || width == 0
        || fragment.rows.iter().any(|r| r.len() != width)
        || a.row + height > grid.len()
        || a.column + width > grid[0].len()
    {
        return Err(StructuredError::InvalidSchema);
    }
    // Validate source cells in isolation before allocating any published identities.
    for row in &fragment.rows {
        for cell in row {
            if cell.blocks.is_empty() {
                return Err(StructuredError::InvalidSchema);
            }
            let mut source = State {
                blocks: cell.blocks.clone(),
                containers: cell.containers.clone(),
                selection: StructuredSelection::Node(cell.blocks[0].id),
            };
            normalize(&mut source)?;
            if source.containers.len() != cell.containers.len()
                || source.containers.iter().any(|c| {
                    matches!(
                        c.kind,
                        ContainerKind::Table | ContainerKind::Row | ContainerKind::Cell { .. }
                    )
                })
            {
                return Err(StructuredError::InvalidSchema);
            }
        }
    }
    for (r, row) in fragment.rows.into_iter().enumerate().rev() {
        for (c, cell) in row.into_iter().enumerate().rev() {
            let destination = grid[a.row + r][a.column + c];
            let (start, end) = subtree_bounds(s, destination)?;
            let old_containers: std::collections::HashSet<_> = s
                .containers
                .iter()
                .filter(|c| descends(s, c.parent, destination))
                .map(|c| c.id)
                .collect();
            s.containers.retain(|c| !old_containers.contains(&c.id));
            let map: std::collections::HashMap<_, _> = cell
                .containers
                .iter()
                .map(|c| (c.id, ContainerId(fresh(next).0)))
                .collect();
            let remap = |p: Option<ContainerId>| -> Result<Option<ContainerId>, StructuredError> {
                Ok(Some(match p {
                    Some(id) => *map.get(&id).ok_or(StructuredError::InvalidSchema)?,
                    None => destination,
                }))
            };
            for c in cell.containers {
                s.containers.push(Container {
                    id: map[&c.id],
                    parent: remap(c.parent)?,
                    kind: c.kind,
                });
            }
            let mut blocks = vec![];
            for b in cell.blocks {
                blocks.push(Block {
                    id: fresh(next),
                    parent: remap(b.parent)?,
                    ..b
                });
            }
            s.blocks.splice(start..=end, blocks);
            container_mut(s, destination)?.kind = ContainerKind::Cell {
                header: cell.header,
            };
        }
    }
    s.selection = StructuredSelection::Cells {
        anchor: at,
        head: grid[a.row + height - 1][a.column + width - 1],
    };
    Ok(())
}
impl StructuredDocument {
    /// Return the nearest table cell membership of an existing block.
    pub fn cell_address(&self, block: BlockId) -> Result<Option<CellAddress>, StructuredError> {
        cell_for(&self.state, block)?
            .map(|c| address(&self.state, c))
            .transpose()
    }
    /// Stable cell identities in row-major rectangular order.
    pub fn table_cells(
        &self,
        table: ContainerId,
    ) -> Result<Vec<Vec<ContainerId>>, StructuredError> {
        table_grid(&self.state, table)
    }
    /// First leaf in a cell, suitable for native caret or whole-atom selection.
    pub fn cell_first_block(&self, cell: ContainerId) -> Result<BlockId, StructuredError> {
        address(&self.state, cell)?;
        Ok(self.state.blocks[subtree_bounds(&self.state, cell)?.0].id)
    }
    /// Tab/Shift-Tab within a table; returns false at its edges for host focus escape.
    /// Selection navigation does not mutate history, revision or readonly content.
    pub fn navigate_cell(&mut self, backward: bool) -> Result<bool, StructuredError> {
        if self.is_composing() {
            return Err(StructuredError::CompositionActive);
        }
        let cell = match self.selection() {
            StructuredSelection::Cells { head, .. } => Some(head),
            StructuredSelection::Node(b) => cell_for(&self.state, b)?,
            StructuredSelection::Text(r) => cell_for(&self.state, r.head.block)?,
        };
        let Some(cell) = cell else {
            return Ok(false);
        };
        let a = address(&self.state, cell)?;
        let cells: Vec<_> = table_grid(&self.state, a.table)?
            .into_iter()
            .flatten()
            .collect();
        let index = cells.iter().position(|c| *c == cell).unwrap();
        let next = if backward {
            index.checked_sub(1)
        } else {
            (index + 1 < cells.len()).then_some(index + 1)
        };
        let Some(next) = next else {
            return Ok(false);
        };
        select_cell(&mut self.state, cells[next])?;
        self.group = None;
        Ok(true)
    }
    /// Copy a rectangular cell selection as a rich value snapshot.
    pub fn copy_cells(
        &self,
        selection: StructuredSelection,
    ) -> Result<TableFragment, StructuredError> {
        let StructuredSelection::Cells { anchor, head } = selection else {
            return Err(StructuredError::InvalidPosition);
        };
        let (table, top, left, bottom, right) = cell_rectangle(&self.state, anchor, head)?;
        let grid = table_grid(&self.state, table)?;
        let mut rows = vec![];
        for row in grid.iter().take(bottom + 1).skip(top) {
            let mut cells = vec![];
            for &cell in row.iter().take(right + 1).skip(left) {
                let ContainerKind::Cell { header } = container(&self.state, cell)?.kind else {
                    unreachable!()
                };
                let (start, end) = subtree_bounds(&self.state, cell)?;
                let mut blocks = self.state.blocks[start..=end].to_vec();
                let mut containers: Vec<_> = self
                    .state
                    .containers
                    .iter()
                    .filter(|c| descends(&self.state, c.parent, cell))
                    .cloned()
                    .collect();
                for b in &mut blocks {
                    if b.parent == Some(cell) {
                        b.parent = None;
                    }
                }
                for c in &mut containers {
                    if c.parent == Some(cell) {
                        c.parent = None;
                    }
                }
                cells.push(CellFragment {
                    header,
                    blocks,
                    containers,
                });
            }
            rows.push(cells);
        }
        Ok(TableFragment { rows })
    }
    /// Plain TSV clipboard fallback for a rectangular selection; atoms use their fallback.
    pub fn cells_plain_text(
        &self,
        selection: StructuredSelection,
    ) -> Result<String, StructuredError> {
        Ok(self
            .copy_cells(selection)?
            .rows
            .iter()
            .map(|row| {
                row.iter()
                    .map(|cell| {
                        cell.blocks
                            .iter()
                            .map(Block::copy_fallback)
                            .collect::<Vec<_>>()
                            .join("\n")
                    })
                    .collect::<Vec<_>>()
                    .join("\t")
            })
            .collect::<Vec<_>>()
            .join("\n"))
    }
    /// Insert a table at the active collapsed text caret.
    pub fn insert_table(
        &mut self,
        rows: usize,
        columns: usize,
        header: bool,
        origin: EditOrigin,
    ) -> Result<bool, StructuredError> {
        let range = self
            .text_selection()
            .ok_or(StructuredError::InvalidPosition)?;
        if range.anchor.block != range.head.block || range.anchor.offset != range.head.offset {
            return Err(StructuredError::InvalidPosition);
        }
        self.command(
            vec![Operation::InsertTable {
                at: range.head,
                rows,
                columns,
                header,
            }],
            origin,
        )
    }
    /// Insert an inert host boundary at the active collapsed text caret.
    pub fn insert_atomic(
        &mut self,
        payload: AtomicPayload,
        origin: EditOrigin,
    ) -> Result<bool, StructuredError> {
        let range = self
            .text_selection()
            .ok_or(StructuredError::InvalidPosition)?;
        if range.anchor.block != range.head.block || range.anchor.offset != range.head.offset {
            return Err(StructuredError::InvalidPosition);
        }
        self.command(
            vec![Operation::InsertAtomic {
                at: range.head,
                payload,
            }],
            origin,
        )
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
    if s.blocks[i].is_atomic() {
        return Err(StructuredError::InvalidPosition);
    }
    let t = s.blocks[i].text();
    if p.offset > t.len() || !t.is_char_boundary(p.offset) || pill_at(&s.blocks[i], p.offset).is_some()
    {
        return Err(StructuredError::InvalidPosition);
    }
    Ok((i, p.offset))
}
type LocalPosition = (usize, usize);
fn ordered(s: &State, r: TextSelection) -> Result<(LocalPosition, LocalPosition), StructuredError> {
    let a = local(s, r.anchor)?;
    let b = local(s, r.head)?;
    let (a, b) = if a <= b { (a, b) } else { (b, a) };
    if cell_for(s, s.blocks[a.0].id)? != cell_for(s, s.blocks[b.0].id)?
        || s.blocks[a.0..=b.0].iter().any(Block::is_atomic)
    {
        return Err(StructuredError::InvalidPosition);
    }
    Ok((a, b))
}
fn offset(s: &State, p: Position) -> Result<usize, StructuredError> {
    let (i, o) = local(s, p)?;
    Ok(s.blocks[..i].iter().map(|b| b.len() + 1).sum::<usize>() + o)
}
fn position_at(s: &State, mut n: usize, affinity: Affinity) -> Result<Position, StructuredError> {
    for b in &s.blocks {
        if b.is_atomic() && n == 0 {
            return Err(StructuredError::InvalidPosition);
        }
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
        StructuredSelection::Cells { anchor, head } => {
            cell_rectangle(s, anchor, head)?;
        }
    }
    Ok(())
}
/// Byte range of the pill run that strictly contains `at`, if any.
fn pill_at(b: &Block, at: usize) -> Option<(usize, usize)> {
    let mut n = 0;
    for r in &b.runs {
        let end = n + r.text.len();
        if r.marks.pill.is_some() && n < at && at < end {
            return Some((n, end));
        }
        n = end;
    }
    None
}
fn valid_marks(m: &Marks) -> Result<(), StructuredError> {
    if m.pill.as_ref().is_some_and(|id| {
        id.is_empty()
            || id.len() > 256
            || id
                .chars()
                .any(|c| c.is_control() || matches!(c, '<' | '>' | '"' | '\'' | '&'))
    }) {
        return Err(StructuredError::InvalidSchema);
    }
    if m.text_style.family.as_ref().is_some_and(|s| {
        s.is_empty()
            || s.len() > 80
            || s.chars()
                .any(|c| c.is_control() || matches!(c, ';' | '{' | '}' | '<' | '>'))
    }) {
        return Err(StructuredError::InvalidSchema);
    }
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
fn container(s: &State, id: ContainerId) -> Result<&Container, StructuredError> {
    s.containers
        .iter()
        .find(|c| c.id == id)
        .ok_or(StructuredError::InvalidSchema)
}
fn container_mut(s: &mut State, id: ContainerId) -> Result<&mut Container, StructuredError> {
    s.containers
        .iter_mut()
        .find(|c| c.id == id)
        .ok_or(StructuredError::InvalidSchema)
}
fn descends(s: &State, mut parent: Option<ContainerId>, id: ContainerId) -> bool {
    for _ in 0..=s.containers.len() {
        let Some(p) = parent else { return false };
        if p == id {
            return true;
        }
        parent = s
            .containers
            .iter()
            .find(|c| c.id == p)
            .and_then(|c| c.parent);
    }
    false
}
fn fresh_container(
    s: &mut State,
    next: &mut u64,
    parent: Option<ContainerId>,
    kind: ContainerKind,
) -> ContainerId {
    let id = ContainerId(fresh(next).0);
    s.containers.push(Container { id, parent, kind });
    id
}
fn valid_attributes(attributes: &Attributes) -> Result<(), StructuredError> {
    if attributes.len() > 32
        || attributes.iter().any(|(k, v)| {
            k.is_empty()
                || k.len() > 64
                || !k
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
                || v.len() > 1024
                || v.chars().any(char::is_control)
        })
    {
        return Err(StructuredError::InvalidSchema);
    }
    Ok(())
}
fn normalize(s: &mut State) -> Result<(), StructuredError> {
    // Validate all relationships before pruning empty containers. Missing parents/cycles
    // are not silently repaired, even if the malformed node has no leaves.
    let mut ids = std::collections::HashSet::new();
    for c in &s.containers {
        if !ids.insert(c.id) {
            return Err(StructuredError::InvalidSchema);
        }
        let mut p = c.parent;
        let mut seen = std::collections::HashSet::from([c.id]);
        while let Some(id) = p {
            if !seen.insert(id) {
                return Err(StructuredError::InvalidSchema);
            }
            p = container(s, id)?.parent;
        }
        match c.kind {
            ContainerKind::List(ListKind::Ordered { start: 0 }) => {
                return Err(StructuredError::InvalidSchema)
            }
            ContainerKind::Item { checked } => {
                let parent = container(s, c.parent.ok_or(StructuredError::InvalidSchema)?)?;
                let ContainerKind::List(kind) = parent.kind else {
                    return Err(StructuredError::InvalidSchema);
                };
                if checked.is_some() != (kind == ListKind::Task) {
                    return Err(StructuredError::InvalidSchema);
                }
            }
            _ => {
                if let Some(p) = c.parent {
                    if matches!(container(s, p)?.kind, ContainerKind::List(_)) {
                        return Err(StructuredError::InvalidSchema);
                    }
                }
            }
        }
    }
    validate_tables(s)?;
    let mut block_ids = std::collections::HashSet::new();
    for b in &s.blocks {
        if !block_ids.insert(b.id) {
            return Err(StructuredError::InvalidSchema);
        }
        if let Some(p) = b.parent {
            if matches!(container(s, p)?.kind, ContainerKind::List(_)) {
                return Err(StructuredError::InvalidSchema);
            }
        }
    }
    let retained: std::collections::HashSet<_> = s
        .containers
        .iter()
        .filter(|c| s.blocks.iter().any(|b| descends(s, b.parent, c.id)))
        .map(|c| c.id)
        .collect();
    s.containers.retain(|c| retained.contains(&c.id));
    for c in &s.containers {
        if let ContainerKind::List(ListKind::Ordered { start }) = c.kind {
            let count = s
                .containers
                .iter()
                .filter(|child| child.parent == Some(c.id))
                .count();
            if start.checked_add(count.saturating_sub(1) as u32).is_none() {
                return Err(StructuredError::InvalidSchema);
            }
        }
        let positions: Vec<_> = s
            .blocks
            .iter()
            .enumerate()
            .filter(|(_, b)| descends(s, b.parent, c.id))
            .map(|(i, _)| i)
            .collect();
        if positions.last().unwrap() - positions[0] + 1 != positions.len() {
            return Err(StructuredError::InvalidSchema);
        }
    }
    for b in &mut s.blocks {
        valid_attributes(&b.attributes)?;
        if b.kind == BlockKind::Atomic {
            b.atomic_payload()
                .ok_or(StructuredError::InvalidSchema)?
                .encode()?;
        }
        if b.is_atomic() && !b.runs.is_empty() {
            return Err(StructuredError::InvalidSchema);
        }
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
            if let Some(last) = runs
                .last_mut()
                .filter(|last| last.marks == r.marks && r.marks.pill.is_none())
            {
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
            return Marks {
                pill: None,
                ..r.marks.clone()
            };
        }
    }
    Marks::default()
}
fn fresh(next: &mut u64) -> BlockId {
    let id = BlockId(*next);
    *next += 1;
    id
}
fn wrap(
    s: &mut State,
    next: &mut u64,
    range: TextSelection,
    kind: ContainerKind,
) -> Result<(), StructuredError> {
    if !matches!(kind, ContainerKind::Quote | ContainerKind::List(_)) {
        return Err(StructuredError::InvalidSchema);
    }
    let (a, z) = ordered(s, range)?;
    let parent = s.blocks[a.0].parent;
    if s.blocks[a.0..=z.0].iter().any(|b| b.parent != parent) {
        return Err(StructuredError::InvalidSchema);
    }
    let id = fresh_container(s, next, parent, kind);
    for i in a.0..=z.0 {
        s.blocks[i].parent = Some(match kind {
            ContainerKind::List(k) => fresh_container(
                s,
                next,
                Some(id),
                ContainerKind::Item {
                    checked: (k == ListKind::Task).then_some(false),
                },
            ),
            _ => id,
        });
    }
    Ok(())
}
fn item_for(s: &State, block: BlockId) -> Result<ContainerId, StructuredError> {
    let mut p = s.blocks[block_index(s, block)?].parent;
    while let Some(id) = p {
        let c = container(s, id)?;
        if matches!(c.kind, ContainerKind::Item { .. }) {
            return Ok(id);
        }
        p = c.parent;
    }
    Err(StructuredError::InvalidSchema)
}
fn subtree_bounds(s: &State, id: ContainerId) -> Result<(usize, usize), StructuredError> {
    let first = s
        .blocks
        .iter()
        .position(|b| descends(s, b.parent, id))
        .ok_or(StructuredError::InvalidSchema)?;
    let last = s
        .blocks
        .iter()
        .rposition(|b| descends(s, b.parent, id))
        .unwrap();
    Ok((first, last))
}
fn indent(s: &mut State, next: &mut u64, block: BlockId) -> Result<(), StructuredError> {
    let item = item_for(s, block)?;
    let list = container(s, item)?
        .parent
        .ok_or(StructuredError::InvalidSchema)?;
    let kind = container(s, list)?.kind;
    let (start, _) = subtree_bounds(s, item)?;
    let previous = s
        .containers
        .iter()
        .filter(|c| c.parent == Some(list) && c.id != item)
        .filter_map(|c| subtree_bounds(s, c.id).ok().map(|(_, end)| (end, c.id)))
        .filter(|(end, _)| *end < start)
        .max_by_key(|(end, _)| *end)
        .map(|(_, id)| id)
        .ok_or(StructuredError::InvalidSchema)?;
    let nested = s
        .containers
        .iter()
        .find(|c| c.parent == Some(previous) && c.kind == kind)
        .filter(|c| subtree_bounds(s, c.id).is_ok_and(|(_, end)| end + 1 == start))
        .map(|c| c.id);
    let nested = nested.unwrap_or_else(|| fresh_container(s, next, Some(previous), kind));
    container_mut(s, item)?.parent = Some(nested);
    Ok(())
}
fn outdent(s: &mut State, next: &mut u64, block: BlockId) -> Result<(), StructuredError> {
    let i = block_index(s, block)?;
    let direct = s.blocks[i].parent.ok_or(StructuredError::InvalidSchema)?;
    if container(s, direct)?.kind == ContainerKind::Quote {
        // Split the quote around this leaf; never reorder user text to repair a tree.
        let parent = container(s, direct)?.parent;
        let tail: Vec<_> = s
            .containers
            .iter()
            .filter(|c| c.parent == Some(direct))
            .filter(|c| subtree_bounds(s, c.id).is_ok_and(|(start, _)| start > i))
            .map(|c| c.id)
            .collect();
        if s.blocks[i + 1..].iter().any(|b| b.parent == Some(direct)) || !tail.is_empty() {
            let after = fresh_container(s, next, parent, ContainerKind::Quote);
            for b in &mut s.blocks[i + 1..] {
                if b.parent == Some(direct) {
                    b.parent = Some(after);
                }
            }
            for id in tail {
                container_mut(s, id)?.parent = Some(after);
            }
        }
        s.blocks[i].parent = parent;
        return Ok(());
    }
    let item = item_for(s, block)?;
    let list = container(s, item)?
        .parent
        .ok_or(StructuredError::InvalidSchema)?;
    let list_parent = container(s, list)?.parent;
    let (_, z) = subtree_bounds(s, item)?;
    let mut siblings: Vec<_> = s
        .containers
        .iter()
        .filter(|c| c.parent == Some(list))
        .filter_map(|c| subtree_bounds(s, c.id).ok().map(|(start, _)| (start, c.id)))
        .collect();
    siblings.sort_by_key(|(start, _)| *start);
    let ordinal = siblings
        .iter()
        .position(|(_, id)| *id == item)
        .ok_or(StructuredError::InvalidSchema)?;
    let following: Vec<_> = siblings
        .iter()
        .filter(|(start, _)| *start > z)
        .map(|(_, id)| *id)
        .collect();
    let kind = container(s, list)?.kind;
    let continuation = match kind {
        ContainerKind::List(ListKind::Ordered { start }) => {
            ContainerKind::List(ListKind::Ordered {
                start: start
                    .checked_add(ordinal as u32 + 1)
                    .ok_or(StructuredError::InvalidSchema)?,
            })
        }
        _ => kind,
    };
    if let Some(outer_item) = list_parent.filter(|id| {
        matches!(
            container(s, *id).map(|c| c.kind),
            Ok(ContainerKind::Item { .. })
        )
    }) {
        let outer_list = container(s, outer_item)?
            .parent
            .ok_or(StructuredError::InvalidSchema)?;
        let ContainerKind::List(outer_kind) = container(s, outer_list)?.kind else {
            return Err(StructuredError::InvalidSchema);
        };
        // Trailing nested siblings become children of the promoted item. This keeps
        // leaf order and the original item's descendant identities intact.
        if !following.is_empty() {
            let after = fresh_container(s, next, Some(item), continuation);
            for id in following {
                container_mut(s, id)?.parent = Some(after);
            }
        }
        let c = container_mut(s, item)?;
        c.parent = Some(outer_list);
        let checked = match c.kind {
            ContainerKind::Item { checked } => checked,
            _ => None,
        };
        c.kind = ContainerKind::Item {
            checked: if outer_kind == ListKind::Task {
                Some(checked.unwrap_or(false))
            } else {
                None
            },
        };
    } else {
        // Split a top-level list around the exited item. Ordered tail numbering
        // explicitly continues at its previous ordinal rather than resetting to 1.
        if !following.is_empty() {
            let after = fresh_container(s, next, list_parent, continuation);
            for id in following {
                container_mut(s, id)?.parent = Some(after);
            }
        }
        for b in &mut s.blocks {
            if b.parent == Some(item) {
                b.parent = list_parent;
            }
        }
        for c in &mut s.containers {
            if c.parent == Some(item) {
                c.parent = list_parent;
            }
        }
        s.containers.retain(|c| c.id != item);
    }
    Ok(())
}
fn apply_operation(s: &mut State, next: &mut u64, op: Operation) -> Result<(), StructuredError> {
    match op {
        Operation::InsertTable {
            at,
            rows,
            columns,
            header,
        } => insert_table(s, next, at, rows, columns, header)?,
        Operation::InsertRow { table, index } => {
            edit_table(s, next, table, index, TableAxis::Row, TableEdit::Insert)?
        }
        Operation::DeleteRow { table, index } => {
            edit_table(s, next, table, index, TableAxis::Row, TableEdit::Delete)?
        }
        Operation::InsertColumn { table, index } => {
            edit_table(s, next, table, index, TableAxis::Column, TableEdit::Insert)?
        }
        Operation::DeleteColumn { table, index } => {
            edit_table(s, next, table, index, TableAxis::Column, TableEdit::Delete)?
        }
        Operation::ReplaceCells { anchor, head, text } => {
            table_ops::replace_cell_rectangle(s, next, anchor, head, text)?
        }
        Operation::PasteCells { at, fragment } => paste_cells(s, next, at, fragment)?,
        Operation::InsertAtomic { at, payload } => {
            let attributes = payload.encode()?;
            apply_operation(s, next, Operation::InsertHorizontalRule { at })?;
            let StructuredSelection::Node(id) = s.selection else {
                unreachable!()
            };
            let i = block_index(s, id)?;
            s.blocks[i].kind = BlockKind::Atomic;
            s.blocks[i].attributes = attributes;
        }
        Operation::UpdateAtomic { block, payload } => {
            let i = block_index(s, block)?;
            if s.blocks[i].kind != BlockKind::Atomic {
                return Err(StructuredError::InvalidSchema);
            }
            s.blocks[i].attributes = payload.encode()?;
        }
        Operation::Wrap { range, kind } => wrap(s, next, range, kind)?,
        Operation::Indent { block } => indent(s, next, block)?,
        Operation::Outdent { block } => outdent(s, next, block)?,
        Operation::ToggleTask { block } => {
            let parent = item_for(s, block)?;
            let ContainerKind::Item {
                checked: Some(value),
            } = container(s, parent)?.kind
            else {
                return Err(StructuredError::InvalidSchema);
            };
            container_mut(s, parent)?.kind = ContainerKind::Item {
                checked: Some(!value),
            };
        }
        Operation::SetAttributes { block, attributes } => {
            valid_attributes(&attributes)?;
            let i = block_index(s, block)?;
            s.blocks[i].attributes = attributes;
        }
        Operation::InsertHorizontalRule { at } => {
            local(s, at)?;
            apply_operation(s, next, Operation::Split { at })?;
            let i = block_index(s, at.block)?;
            let id = fresh(next);
            s.blocks.insert(
                i + 1,
                Block {
                    id,
                    kind: BlockKind::HorizontalRule,
                    parent: s.blocks[i].parent,
                    attributes: Attributes::new(),
                    runs: vec![],
                },
            );
            s.selection = StructuredSelection::Node(id);
        }
        Operation::SetBlockKind { block, kind } => {
            let i = block_index(s, block)?;
            if matches!(kind, BlockKind::HorizontalRule | BlockKind::Atomic)
                || s.blocks[i].is_atomic()
            {
                return Err(StructuredError::InvalidSchema);
            }
            s.blocks[i].kind = kind;
        }
        Operation::SetMarks { range, marks } => text_ops::set_range_marks(s, range, marks)?,
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
        Operation::Replace { range, text } => text_ops::replace_text_range(s, next, range, text)?,
        Operation::DeleteNode { block } => {
            let i = block_index(s, block)?;
            let cell = cell_for(s, block)?;
            s.blocks.remove(i);
            if let Some(cell) = cell {
                if !s.blocks.iter().any(|b| descends(s, b.parent, cell)) {
                    s.blocks.insert(i, empty_leaf(next, Some(cell)));
                }
            }
            if !s.blocks.iter().any(|b| !b.is_atomic()) {
                s.blocks.push(Block {
                    id: fresh(next),
                    kind: BlockKind::Paragraph,
                    parent: None,
                    attributes: Attributes::new(),
                    runs: vec![],
                });
            }
            let j = (i..s.blocks.len())
                .find(|&j| !s.blocks[j].is_atomic())
                .or_else(|| {
                    (0..i.min(s.blocks.len()))
                        .rev()
                        .find(|&j| !s.blocks[j].is_atomic())
                })
                .ok_or(StructuredError::InvalidSchema)?;
            let b = &s.blocks[j];
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
mod operation_tests;

#[cfg(test)]
#[path = "structured_e6_tests.rs"]
mod e6_tests;

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
            ..Marks::default()
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

#[cfg(test)]
#[path = "structured_table_atom_tests.rs"]
mod table_atom_tests;
