//! Experimental retained structured editor. Tree transactions, not source text,
//! own content and undo. Install `UiPlugin`; the core Editor supplies shaping,
//! pointer selection, keyboard, scrolling, native IME and text accessibility.
use crate::{Token, Typography, UiError, UiId, UiRecipe, UiStyle};
use rndr_core::editing::{Affinity, EditOrigin};
use rndr_core::structured::{
    Attributes, BlockKind, ContainerKind, ListKind, Operation, StructuredSelection, TextSelection,
    TextStyle, Transaction,
};
use rndr_core::structured_native::*;
use rndr_core::*;

/// Controlled identity and accessible metadata. Readonly belongs to the document.
#[derive(Clone, Debug)]
pub struct StructuredSurfaceProps {
    /// Tree retained in `StructuredDocuments`.
    pub document: StructuredHandle,
    /// Required persistent accessible name.
    pub name: String,
    /// Additional accessible help.
    pub description: String,
}
impl StructuredSurfaceProps {
    /// Name an existing retained tree.
    pub fn new(document: StructuredHandle, name: impl Into<String>) -> Self {
        Self {
            document,
            name: name.into(),
            description: String::new(),
        }
    }
}
/// Refusal leaves tree/history and retained identity intact.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StructuredSurfaceError {
    /// Invalid parent, duplicate incompatible key or stale handle.
    Ui(UiError),
    /// Native ownership/composition or typed transaction refusal.
    View(StructuredViewError),
    /// Inline formatting requires nonempty text selection, not a node/caret.
    SelectText,
}
impl From<UiError> for StructuredSurfaceError {
    fn from(error: UiError) -> Self {
        Self::Ui(error)
    }
}
impl From<StructuredViewError> for StructuredSurfaceError {
    fn from(error: StructuredViewError) -> Self {
        Self::View(error)
    }
}
#[derive(Clone, Copy)]
struct Node {
    id: UiId,
    document: StructuredHandle,
    editor: Entity,
    gutter: Entity,
}
impl Component for Node {}
pub(crate) fn key_at(world: &World, entity: Entity) -> Option<UiId> {
    world.get::<Node>(entity).map(|n| n.id)
}
// Color/typography recipes resolve in Layout after native input. Reproject in
// that same pass so the first paint and theme changes do not use stale spans.
pub(crate) fn prepare_projection(world: &mut World) {
    for entity in world.entities_with::<StructuredBinding>() {
        project(world, entity);
    }
}
/// Retained structure-aware surface entry point, separate from EditorSurface.
pub struct StructuredSurface;
/// Retained root and canonical native input target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StructuredSurfaceHandle {
    /// Owned layout container.
    pub root: Entity,
    /// Native editor projection; do not raw-write its Buffer.
    pub editor: Entity,
}
impl StructuredSurface {
    /// Mount/update under a parent-scoped key. Changing documents requires
    /// explicit destroy/remount; no implicit history/selection replacement.
    pub fn mount(
        world: &mut World,
        parent: Entity,
        id: UiId,
        props: StructuredSurfaceProps,
    ) -> Result<StructuredSurfaceHandle, StructuredSurfaceError> {
        if !world.is_alive(parent) || !world.has::<Layout>(parent) {
            return Err(UiError::Stale.into());
        }
        if props.name.trim().is_empty() {
            return Err(UiError::InvalidProps.into());
        }
        if world
            .resource::<StructuredDocuments>()
            .and_then(|d| d.get(props.document))
            .is_none()
        {
            return Err(StructuredViewError::Unavailable.into());
        }
        let existing = children_of(world, parent)
            .into_iter()
            .find(|e| crate::identity(world, *e) == Some(id));
        let handle = if let Some(root) = existing {
            let node = world
                .get::<Node>(root)
                .copied()
                .ok_or(UiError::IdentityConflict)?;
            if node.document != props.document {
                return Err(UiError::InvalidProps.into());
            }
            let handle = StructuredSurfaceHandle {
                root,
                editor: node.editor,
            };
            handle.validate(world)?;
            handle
        } else {
            let root = world.spawn((Layout {
                width: Size::Fill,
                height: Size::Fill,
                ..Layout::row()
            },));
            // Keep only a small retained margin; paragraph indentation owns
            // marker space. Chrome never enters input or clipboard bytes.
            let gutter = world.spawn((Layout {
                width: Size::Px(4.0),
                height: Size::Fill,
                ..Layout::column()
            },));
            set_parent(world, gutter, root);
            let editor = world.spawn((
                Layout {
                    width: Size::Fill,
                    height: Size::Fill,
                    ..Layout::column()
                },
                Label::new(""),
                Buffer::from_str(""),
                Editor::default(),
                CapturesKeys,
                Focusable,
            ));
            set_parent(world, editor, root);
            world.insert(
                gutter,
                rndr_core::structured_chrome::StructuredChromeTarget(editor),
            );
            world.insert(
                editor,
                rndr_core::structured_chrome::StructuredChromeGutter { width: 4.0 },
            );
            world.insert(
                editor,
                UiRecipe(UiStyle {
                    typography: Some(Typography::Reading),
                    foreground: Some(Token::Text.into()),
                    background: Some(Token::Content.into()),
                    ..UiStyle::editor()
                }),
            );
            if let Err(error) = attach_structured(world, editor, props.document) {
                despawn(world, root);
                return Err(error.into());
            }
            set_parent(world, root, parent);
            world.insert(
                root,
                Node {
                    id,
                    document: props.document,
                    editor,
                    gutter,
                },
            );
            StructuredSurfaceHandle { root, editor }
        };
        let readonly = world
            .get::<Semantics>(handle.editor)
            .is_some_and(|s| s.readonly);
        world.insert(
            handle.editor,
            Semantics {
                name: Some(props.name),
                description: props.description,
                readonly,
            },
        );
        project(world, handle.editor);
        Ok(handle)
    }
}
/// Explicit toolbar intent. Selection stays in the document when focus moves.
#[derive(Clone, Debug)]
pub enum StructuredCommand {
    /// Insert a real document grid at a collapsed caret.
    Table {
        /// Positive row count.
        rows: usize,
        /// Positive column count.
        columns: usize,
        /// Header semantics for the first row.
        header: bool,
    },
    /// Insert a row after the active cell, or before it when true.
    InsertRow(bool),
    /// Delete the active cell's row, repairing selection.
    DeleteRow,
    /// Insert a column after the active cell, or before it when true.
    InsertColumn(bool),
    /// Delete the active cell's column, repairing selection.
    DeleteColumn,
    /// Select the active cell as a typed structural selection.
    SelectCell,
    /// Extend cell selection to the complete containing table.
    SelectTable,
    /// Insert a permission-free atomic host extension.
    Atomic(rndr_core::structured::AtomicPayload),
    /// Toggle strong on selected runs, preserving emphasis and link.
    Strong,
    /// Toggle emphasis on selected runs, preserving strong and link.
    Emphasis,
    /// Apply/remove a validated link, preserving other marks. No URL navigation.
    Link(Option<String>),
    /// Paragraph/heading conversion, preserving identity and text selection.
    Block(BlockKind),
    /// Wrap selected sibling blocks/subtrees in an explicit quote container.
    Quote,
    /// Wrap selected sibling blocks in a schema-owned list and item containers.
    List(ListKind),
    /// Nest the moving endpoint's entire item under its preceding sibling.
    Indent,
    /// Promote the moving endpoint's item or exit its outer list/quote.
    Outdent,
    /// Toggle the moving endpoint's task item; checked state is model history.
    ToggleTask,
    /// Insert a nontext rule at a collapsed text caret.
    HorizontalRule,
    /// Replace validated inert metadata on the moving endpoint's block.
    Attributes(Attributes),
    /// Replace color/family styles on selected text, preserving semantic marks.
    TextStyle(TextStyle),
    /// Document-owned undo.
    Undo,
    /// Document-owned redo.
    Redo,
    /// Select the entire block containing the moving text endpoint.
    SelectBlock,
    /// Delete a selected structural node, leaving an editable paragraph if last.
    DeleteBlock,
}
impl StructuredSurfaceHandle {
    /// Validate retained key, parent, editor and exact document attachment.
    pub fn validate(self, world: &World) -> Result<(), UiError> {
        let node = world.get::<Node>(self.root).ok_or(UiError::Stale)?;
        if node.editor != self.editor
            || !world.has::<Editor>(self.editor)
            || !world.has::<Buffer>(self.editor)
            || world.get::<Parent>(self.editor).map(|p| p.entity) != Some(self.root)
            || attached_structured(world, self.editor) != Some(node.document)
        {
            return Err(UiError::Stale);
        }
        Ok(())
    }
    /// Dispatch against the saved typed selection, even while a toolbar has
    /// focus. Refuses active IME instead of silently cancelling its capture.
    pub fn command(
        self,
        world: &mut World,
        command: StructuredCommand,
    ) -> Result<bool, StructuredSurfaceError> {
        self.validate(world)?;
        let id = command_ready(world, self.editor)?;
        // A view/ancestor readonly policy must also protect toolbar-origin edits.
        let readonly = rndr_core::access_details(world, self.editor).readonly;
        let doc = world
            .resource_mut::<StructuredDocuments>()
            .and_then(|d| d.get_mut(id))
            .ok_or(StructuredViewError::Unavailable)?;
        let select_only = matches!(
            command,
            StructuredCommand::SelectBlock
                | StructuredCommand::SelectCell
                | StructuredCommand::SelectTable
        );
        if readonly && !select_only {
            return Err(StructuredViewError::Model(
                rndr_core::structured::StructuredError::Readonly,
            )
            .into());
        }
        let reveal = !matches!(command, StructuredCommand::ToggleTask);
        let origin = EditOrigin::Host(0xE6);
        let result = match command {
            StructuredCommand::Table {
                rows,
                columns,
                header,
            } => doc.insert_table(rows, columns, header, origin),
            StructuredCommand::Atomic(payload) => doc.insert_atomic(payload, origin),
            command @ (StructuredCommand::InsertRow(_)
            | StructuredCommand::DeleteRow
            | StructuredCommand::InsertColumn(_)
            | StructuredCommand::DeleteColumn
            | StructuredCommand::SelectCell
            | StructuredCommand::SelectTable) => {
                let block = match doc.selection() {
                    StructuredSelection::Text(s) => s.head.block,
                    StructuredSelection::Node(b) => b,
                    StructuredSelection::Cells { head, .. } => doc
                        .cell_first_block(head)
                        .map_err(StructuredViewError::from)?,
                };
                let a = doc
                    .cell_address(block)
                    .map_err(StructuredViewError::from)?
                    .ok_or(StructuredSurfaceError::SelectText)?;
                if matches!(
                    command,
                    StructuredCommand::SelectCell | StructuredCommand::SelectTable
                ) {
                    let (anchor, head) = if matches!(command, StructuredCommand::SelectTable) {
                        let grid = doc
                            .table_cells(a.table)
                            .map_err(StructuredViewError::from)?;
                        (grid[0][0], *grid.last().unwrap().last().unwrap())
                    } else {
                        (a.cell, a.cell)
                    };
                    doc.set_selection(StructuredSelection::Cells { anchor, head })
                        .map(|_| true)
                } else {
                    let operation = match command {
                        StructuredCommand::InsertRow(before) => Operation::InsertRow {
                            table: a.table,
                            index: a.row + usize::from(!before),
                        },
                        StructuredCommand::DeleteRow => Operation::DeleteRow {
                            table: a.table,
                            index: a.row,
                        },
                        StructuredCommand::InsertColumn(before) => Operation::InsertColumn {
                            table: a.table,
                            index: a.column + usize::from(!before),
                        },
                        _ => Operation::DeleteColumn {
                            table: a.table,
                            index: a.column,
                        },
                    };
                    doc.apply(Transaction {
                        document: doc.id(),
                        base: doc.revision(),
                        origin,
                        group: None,
                        operations: vec![operation],
                    })
                }
            }
            StructuredCommand::Undo => doc.undo(),
            StructuredCommand::Redo => doc.redo(),
            StructuredCommand::Block(kind) => doc.set_block_kind(kind, origin),
            command @ (StructuredCommand::Quote
            | StructuredCommand::List(_)
            | StructuredCommand::Indent
            | StructuredCommand::Outdent
            | StructuredCommand::ToggleTask
            | StructuredCommand::HorizontalRule
            | StructuredCommand::Attributes(_)) => {
                let block = match doc.selection() {
                    StructuredSelection::Text(s) => s.head.block,
                    StructuredSelection::Node(id) => id,
                    StructuredSelection::Cells { head, .. } => doc
                        .cell_first_block(head)
                        .map_err(StructuredViewError::from)?,
                };
                let operation = match command {
                    StructuredCommand::Quote | StructuredCommand::List(_) => {
                        let range = doc
                            .text_selection()
                            .ok_or(StructuredSurfaceError::SelectText)?;
                        let kind = match command {
                            StructuredCommand::List(kind) => ContainerKind::List(kind),
                            _ => ContainerKind::Quote,
                        };
                        Operation::Wrap { range, kind }
                    }
                    StructuredCommand::Indent => Operation::Indent { block },
                    StructuredCommand::Outdent => Operation::Outdent { block },
                    StructuredCommand::ToggleTask => Operation::ToggleTask { block },
                    StructuredCommand::Attributes(attributes) => {
                        Operation::SetAttributes { block, attributes }
                    }
                    StructuredCommand::HorizontalRule => {
                        let range = doc
                            .text_selection()
                            .ok_or(StructuredSurfaceError::SelectText)?;
                        if range.anchor.block != range.head.block
                            || range.anchor.offset != range.head.offset
                        {
                            return Err(StructuredSurfaceError::SelectText);
                        }
                        Operation::InsertHorizontalRule { at: range.head }
                    }
                    _ => unreachable!(),
                };
                doc.apply(Transaction {
                    document: doc.id(),
                    base: doc.revision(),
                    origin,
                    group: None,
                    operations: vec![operation],
                })
            }
            StructuredCommand::SelectBlock => {
                let block = match doc.selection() {
                    StructuredSelection::Text(s) => s.head.block,
                    StructuredSelection::Node(id) => id,
                    StructuredSelection::Cells { head, .. } => doc
                        .cell_first_block(head)
                        .map_err(StructuredViewError::from)?,
                };
                doc.set_selection(StructuredSelection::Node(block))
                    .map(|_| true)
            }
            StructuredCommand::DeleteBlock => {
                let StructuredSelection::Node(block) = doc.selection() else {
                    return Err(StructuredSurfaceError::SelectText);
                };
                doc.apply(Transaction {
                    document: doc.id(),
                    base: doc.revision(),
                    origin,
                    group: None,
                    operations: vec![Operation::DeleteNode { block }],
                })
            }
            command => {
                let Some(selection) = doc.text_selection() else {
                    return Err(StructuredSurfaceError::SelectText);
                };
                let a = doc
                    .display_offset(selection.anchor)
                    .map_err(StructuredViewError::from)?;
                let h = doc
                    .display_offset(selection.head)
                    .map_err(StructuredViewError::from)?;
                let range = a.min(h)..a.max(h);
                if range.is_empty() {
                    return Err(StructuredSurfaceError::SelectText);
                }
                let mut offset = 0;
                let mut selected = Vec::new();
                for block in doc.blocks() {
                    for run in &block.runs {
                        let start = offset.max(range.start);
                        let end = (offset + run.text.len()).min(range.end);
                        if start < end {
                            selected.push((start, end, run.marks.clone()));
                        }
                        offset += run.text.len();
                    }
                    offset += 1;
                }
                let enable = match command {
                    StructuredCommand::Strong => !selected.iter().all(|(_, _, m)| m.strong),
                    StructuredCommand::Emphasis => !selected.iter().all(|(_, _, m)| m.emphasis),
                    _ => true,
                };
                let mut operations = Vec::new();
                for (start, end, mut marks) in selected {
                    match &command {
                        StructuredCommand::Strong => marks.strong = enable,
                        StructuredCommand::Emphasis => marks.emphasis = enable,
                        StructuredCommand::Link(link) => marks.link = link.clone(),
                        StructuredCommand::TextStyle(style) => marks.text_style = style.clone(),
                        _ => unreachable!(),
                    }
                    operations.push(Operation::SetMarks {
                        range: TextSelection {
                            anchor: doc
                                .position_at(start, Affinity::After)
                                .map_err(StructuredViewError::from)?,
                            head: doc
                                .position_at(end, Affinity::After)
                                .map_err(StructuredViewError::from)?,
                        },
                        marks,
                    });
                }
                doc.apply(Transaction {
                    document: doc.id(),
                    base: doc.revision(),
                    origin,
                    group: None,
                    operations,
                })
            }
        }
        .map_err(StructuredViewError::from)?;
        project(world, self.editor);
        if result && reveal {
            rndr_core::structured_native::reveal_selection(world, self.editor);
        }
        Ok(result)
    }
    /// Detach and free only owned entities; document/history remain retained.
    pub fn destroy(self, world: &mut World) -> Result<(), StructuredSurfaceError> {
        self.validate(world)?;
        detach_structured(world, self.editor)?;
        let gutter = world.get::<Node>(self.root).ok_or(UiError::Stale)?.gutter;
        despawn(world, gutter);
        for child in children_of(world, self.root) {
            if child != self.editor {
                world.remove::<Parent>(child);
                if let Some(children) = world.get_mut::<Children>(self.root) {
                    children.list.retain(|e| *e != child);
                }
            }
        }
        forget_interaction_targets(world, &[self.root, self.editor]);
        despawn(world, self.root);
        Ok(())
    }
}
