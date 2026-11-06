//! Table identity, retained rows/cells, header requests and intrinsic measurement.
use super::{destroy, existing, node, reorder};
use crate::collections::Item;
use crate::{collections::*, controls, *};
use rndr_core::*;
use std::collections::{BTreeMap, BTreeSet};

/// Maximum supplied rows in one table mount; page larger models in the host.
pub const MAX_TABLE_ROWS: usize = 256;
/// Maximum columns in one table mount.
pub const MAX_TABLE_COLUMNS: usize = 32;
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
/// Stable nonzero column identity, independent of display position.
pub struct ColumnId(pub u64);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Horizontal alignment for passive custom cell content.
pub enum CellAlignment {
    /// Fill the available cell width.
    Start,
    /// Center intrinsically sized passive content.
    Center,
    /// Align intrinsically sized passive content to the trailing edge.
    End,
}
#[derive(Clone, Debug)]
/// Host-owned column definition. Width is finite logical points (48–2048).
pub struct Column {
    /// Stable nonzero identity in the containing owner namespace.
    pub id: ColumnId,
    /// Human-readable visible label.
    pub label: String,
    /// Requested logical-point width; declarative host-owned resizing.
    pub width: f32,
    /// Passive cell-content alignment.
    pub alignment: CellAlignment,
    /// Whether canonical header activation may request a sort.
    pub sortable: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Host-accepted ordering direction. The table never sorts model records itself.
pub enum SortDirection {
    /// Ascending host ordering.
    Ascending,
    /// Descending host ordering.
    Descending,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Host-accepted current sort, also used in a sort request.
pub struct SortState {
    /// Stable column identity.
    pub column: ColumnId,
    /// Requested or host-accepted direction.
    pub direction: SortDirection,
}
#[derive(Clone, Debug)]
/// Table presentation and interaction policy; row/cell data remain application-owned.
pub struct TableProps {
    /// Nonempty accessible name.
    pub name: String,
    /// Interaction permission; false disables owned actionable controls.
    pub enabled: bool,
    /// Existing collection single/no-selection policy.
    pub selection: SelectionMode,
    /// Host-accepted sort indicator; does not reorder host data.
    pub sort: Option<SortState>,
    /// Bounded table-body viewport height in logical points (80–2000).
    pub body_height: f32,
    /// Nonempty feedback when no records or properties are supplied.
    pub empty: String,
}
impl TableProps {
    /// Construct enabled defaults with a nonempty accessible name.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            enabled: true,
            selection: SelectionMode::Single,
            sort: None,
            body_height: 280.0,
            empty: "No rows supplied".into(),
        }
    }
}
#[derive(Clone, Debug)]
/// Canonical header activation requesting a host-owned sort.
pub struct SortRequested {
    /// Originating managed table entity.
    pub table: Entity,
    /// Host-accepted sort indicator; does not reorder host data.
    pub sort: SortState,
}
impl Event for SortRequested {}
#[derive(Clone, Copy, Debug)]
/// Stable passive-cell composition slot. Only content descendants are caller-owned.
pub struct CellSlot {
    /// Component-owned root; do not reparent or modify its managed children.
    pub root: Entity,
    /// Caller-owned passive descendants are composed here.
    pub content: Entity,
}
#[derive(Clone)]
struct Table {
    key: UiId,
    props: TableProps,
    columns: Vec<Column>,
    view: ViewHandle,
    header: Entity,
    headers: BTreeMap<ColumnId, controls::ControlHandle>,
    cells: BTreeMap<(ItemId, ColumnId), CellSlot>,
}
impl Component for Table {}
#[derive(Clone, Copy)]
struct HeaderOwner {
    table: Entity,
    column: ColumnId,
}
impl Component for HeaderOwner {}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Reusable table handle and its canonical row-collection behavior.
pub struct TableHandle {
    /// Component-owned root; do not reparent or modify its managed children.
    pub root: Entity,
    /// Canonical ListView handle: focus, selection, activation and row access.
    pub rows: ViewHandle,
}
/// Bounded table composition over the existing ListView and controls.
pub struct DataTable;
// Grid chrome is independent of collection density: header and body share
// ten-point outer insets and exact column widths. Collection rows reserve
// three points for their focus outline; the remaining seven belong to content.
pub(crate) const TABLE_INSET: f32 = 10.0;
const CELL_INSET: f32 = 5.0; // Same as header root/gap/border chrome (2+2+1).
#[derive(Clone, Copy)]
pub(crate) struct TableRow;
impl Component for TableRow {}

pub(super) fn key_at(world: &World, e: Entity) -> Option<UiId> {
    world.get::<Table>(e).map(|s| s.key)
}
fn header_activate(world: &mut World, entity: Entity) {
    let Some(owner) = world.get::<HeaderOwner>(entity).copied() else {
        return;
    };
    let Some(table) = world.get::<Table>(owner.table) else {
        return;
    };
    if !table.props.enabled
        || !table
            .columns
            .iter()
            .any(|c| c.id == owner.column && c.sortable)
    {
        return;
    }
    let direction = if table.props.sort
        == Some(SortState {
            column: owner.column,
            direction: SortDirection::Ascending,
        }) {
        SortDirection::Descending
    } else {
        SortDirection::Ascending
    };
    send(
        world,
        SortRequested {
            table: owner.table,
            sort: SortState {
                column: owner.column,
                direction,
            },
        },
    );
}
// Fill labels in an intrinsically aligned subtree would measure to zero.
// Only passive label widths are normalized; custom cells should size intrinsically.
#[derive(Clone, Copy)]
struct IntrinsicCellLabel {
    minimum: f32,
    last: f32,
}
impl Component for IntrinsicCellLabel {}
fn intrinsic_labels(w: &mut World, e: Entity) {
    if w.has::<Label>(e) {
        if let Some(l) = w.get_mut::<Layout>(e) {
            if matches!(l.width, Size::Fill) {
                l.width = Size::Hug;
            }
        }
        if let Some(l) = w.get::<Layout>(e).filter(|l| matches!(l.width, Size::Hug)) {
            let min = l.min_width;
            if !w.has::<IntrinsicCellLabel>(e) {
                w.insert(
                    e,
                    IntrinsicCellLabel {
                        minimum: min,
                        last: min,
                    },
                );
            }
        }
    }
    for child in children_of(w, e) {
        intrinsic_labels(w, child);
    }
}
// Canonical layout snaps extents to device pixels. A snapped-down intrinsic
// width can soft-wrap the last numeric token onto a clipped second line. Reuse
// the active canonical engine after theme resolution, reserving one guard point.
pub(crate) fn measure_cells(w: &mut World) {
    let engine = text_engine(w);
    // After theme resolution, apply the same declared column alignment to
    // header text as body content. The header's existing button keeps all
    // focus, activation, semantics and rounded chrome.
    for root in w.entities_with::<Table>() {
        let table = w.get::<Table>(root).unwrap().clone();
        for column in &table.columns {
            let label = table.headers[&column.id].label;
            if let Some(style) = w.get_mut::<TextStyle>(label) {
                style.align = match column.alignment {
                    CellAlignment::Start => TextAlign::Start,
                    CellAlignment::Center => TextAlign::Center,
                    CellAlignment::End => TextAlign::End,
                };
            }
        }
    }
    for e in w.entities_with::<IntrinsicCellLabel>() {
        let Some(label) = w.get::<Label>(e) else {
            continue;
        };
        let Some(layout) = w.get::<Layout>(e) else {
            continue;
        };
        if !matches!(layout.width, Size::Hug) {
            continue;
        }
        let mut owner = *w.get::<IntrinsicCellLabel>(e).unwrap();
        if (layout.min_width - owner.last).abs() > 0.01 {
            owner.minimum = layout.min_width;
        }
        let runs = w.get::<Spans>(e).map(|s| s.runs.as_slice()).unwrap_or(&[]);
        let measured = engine.measure(&label.text, &style_for(w, e), runs, f32::INFINITY);
        owner.last = (measured.width + 2. * label_box(w, e).inset_x)
            .ceil()
            .max(owner.minimum)
            + 1.;
        w.get_mut::<Layout>(e).unwrap().min_width = owner.last;
        w.insert(e, owner);
    }
}
fn valid_columns(columns: &[Column], props: &TableProps, rows: &[Item]) -> bool {
    let ids: BTreeSet<_> = columns.iter().map(|c| c.id).collect();
    !props.name.trim().is_empty()
        && props.body_height.is_finite()
        && (80.0..=2000.0).contains(&props.body_height)
        && !props.empty.trim().is_empty()
        && !columns.is_empty()
        && columns.len() <= MAX_TABLE_COLUMNS
        && ids.len() == columns.len()
        && columns.iter().all(|c| {
            c.id.0 != 0
                && !c.label.trim().is_empty()
                && c.width.is_finite()
                && (48.0..=2048.0).contains(&c.width)
        })
        && props
            .sort
            .is_none_or(|s| columns.iter().any(|c| c.id == s.column && c.sortable))
        && rows.len() <= MAX_TABLE_ROWS
        && rows
            .iter()
            .all(|r| r.id.0 != 0 && !r.name.trim().is_empty())
        && rows.iter().map(|r| r.id).collect::<BTreeSet<_>>().len() == rows.len()
}
impl DataTable {
    /// Reconcile keyed rows, columns and cells. `compose` updates passive content
    /// only; it must be infallible, respect slot ownership, and not mutate peers.
    /// Width changes are host-owned declarative resize; no drag handle is offered.
    pub fn mount(
        world: &mut World,
        parent: Entity,
        key: UiId,
        props: TableProps,
        columns: &[Column],
        rows: &[Item],
        mut compose: impl FnMut(&mut World, CellSlot, &Item, &Column),
    ) -> Result<TableHandle, UiError> {
        if !valid_columns(columns, &props, rows) {
            return Err(UiError::InvalidProps);
        }
        let (root, mut table) = Self::resolve_table(world, parent, key, &props)?;
        Self::retire_table_entries(world, &mut table, columns, rows);
        table.props = props.clone();
        table.columns = columns.to_vec();
        let width = columns.iter().map(|column| column.width).sum::<f32>() + TABLE_INSET * 2.0;
        Self::project_table_chrome(world, root, &table, &props, width);
        Self::reconcile_headers(world, root, &mut table, &props, columns)?;
        Self::reconcile_rows(world, root, &mut table, &props, columns, rows, &mut compose)?;
        Self::project_table_body(world, &table, &props, width);
        let handle = TableHandle {
            root,
            rows: table.view,
        };
        // Publish only after composition, preserving the callback-visible state.
        world.insert(root, table);
        Ok(handle)
    }

    fn resolve_table(
        world: &mut World,
        parent: Entity,
        key: UiId,
        props: &TableProps,
    ) -> Result<(Entity, Table), UiError> {
        let found = existing(world, parent, key)?;
        Ok(if let Some(root) = found {
            let table = world
                .get::<Table>(root)
                .cloned()
                .ok_or(UiError::IdentityConflict)?;
            TableHandle {
                root,
                rows: table.view,
            }
            .validate(world)?;
            (root, table)
        } else {
            let root = node(
                world,
                parent,
                Layout {
                    width: Size::Fill,
                    height: Size::Hug,
                    ..Layout::column()
                },
            );
            world.insert(root, Scroll::default());
            world.insert(
                root,
                DataAccess {
                    role: AccessRole::Group,
                    headers: vec![],
                },
            );
            let header = node(world, root, Layout::row());
            let view = ListView::mount(
                world,
                root,
                UiId(1),
                ViewProps::new(&props.name),
                &[],
                |_, _, _| {},
            )?;
            (
                root,
                Table {
                    key,
                    props: props.clone(),
                    columns: vec![],
                    view,
                    header,
                    headers: BTreeMap::new(),
                    cells: BTreeMap::new(),
                },
            )
        })
    }

    fn retire_table_entries(
        world: &mut World,
        table: &mut Table,
        columns: &[Column],
        rows: &[Item],
    ) {
        let ids: BTreeSet<_> = columns.iter().map(|c| c.id).collect();
        let row_ids: BTreeSet<_> = rows.iter().map(|r| r.id).collect();
        table.cells.retain(|(r, c), slot| {
            let keep = row_ids.contains(r) && ids.contains(c);
            if !keep {
                destroy(world, slot.root);
            }
            keep
        });
        table.headers.retain(|id, h| {
            let keep = ids.contains(id);
            if !keep {
                destroy(world, h.root);
            }
            keep
        });
    }

    fn project_table_chrome(
        world: &mut World,
        root: Entity,
        table: &Table,
        props: &TableProps,
        width: f32,
    ) {
        world.insert(
            root,
            Layout {
                width: Size::Fill,
                height: Size::Px(props.body_height + 60.0),
                ..Layout::column()
            },
        );
        world.insert(root, Semantics { name: Some(props.name.clone()), description: "Row focus only; Arrow keys move rows, Space selects, Enter activates. Headers request application-owned sorting. Horizontal overflow scrolls; no spreadsheet editing or drag resizing.".into(), readonly: false });
        world.insert(
            root,
            DataAccess {
                role: AccessRole::Table,
                headers: vec![],
            },
        );
        world.insert(
            table.header,
            Layout {
                width: Size::Px(width),
                height: Size::Hug,
                padding: TABLE_INSET,
                ..Layout::row()
            },
        );
    }

    fn reconcile_headers(
        world: &mut World,
        root: Entity,
        table: &mut Table,
        props: &TableProps,
        columns: &[Column],
    ) -> Result<(), UiError> {
        let mut order = vec![];
        for c in columns {
            let mark = props
                .sort
                .filter(|s| s.column == c.id)
                .map(|s| {
                    if s.direction == SortDirection::Ascending {
                        " ↑"
                    } else {
                        " ↓"
                    }
                })
                .unwrap_or("");
            let mut p = controls::ButtonProps::new(
                format!("{}{mark}", c.label),
                controls::HostIntent::Command(c.id.0),
            );
            p.enabled = props.enabled && c.sortable;
            p.readonly = false;
            p.name = Some(format!(
                "{}, {}",
                c.label,
                if c.sortable {
                    if mark.is_empty() {
                        "not sorted; activate to sort"
                    } else if mark == " ↑" {
                        "ascending; activate for descending"
                    } else {
                        "descending; activate for ascending"
                    }
                } else {
                    "not sortable"
                }
            ));
            let h = controls::Button::mount(world, table.header, UiId(c.id.0), p)?;
            world.insert(
                h.root,
                Layout {
                    width: Size::Px(c.width),
                    height: Size::Hug,
                    ..Layout::column()
                },
            );
            world.insert(
                h.root,
                HeaderOwner {
                    table: root,
                    column: c.id,
                },
            );
            world.insert(h.root, ActivationHandler(header_activate));
            // Header chrome already supplies CELL_INSET; no second label inset.
            world.insert(
                h.label,
                LabelBox {
                    inset_x: 0.,
                    inset_y: 2.,
                    min_height: 0.,
                },
            );
            world.insert(
                h.root,
                DataAccess {
                    role: AccessRole::ColumnHeader,
                    headers: vec![],
                },
            );
            order.push(h.root);
            table.headers.insert(c.id, h);
        }
        reorder(world, table.header, &order);
        world.insert(
            table.header,
            DataAccess {
                role: AccessRole::Row,
                headers: vec![],
            },
        );
        Ok(())
    }

    fn reconcile_rows(
        world: &mut World,
        root: Entity,
        table: &mut Table,
        props: &TableProps,
        columns: &[Column],
        rows: &[Item],
        compose: &mut impl FnMut(&mut World, CellSlot, &Item, &Column),
    ) -> Result<(), UiError> {
        let mut vp = ViewProps::new(&props.name);
        vp.enabled = props.enabled;
        vp.selection = props.selection;
        vp.empty = props.empty.clone();
        let old_cells = table.cells.clone();
        let mut cells = BTreeMap::new();
        table.view = ListView::mount(world, root, UiId(1), vp, rows, |w, row, item| {
            w.insert(row.root, TableRow);
            w.insert(
                row.root,
                DataAccess {
                    role: AccessRole::Row,
                    headers: vec![],
                },
            );
            w.insert(
                row.content,
                Layout {
                    width: Size::Fill,
                    height: Size::Hug,
                    ..Layout::row()
                },
            );
            let mut order = vec![];
            for c in columns {
                let slot = old_cells.get(&(item.id, c.id)).copied().unwrap_or_else(|| {
                    let r = node(w, row.content, Layout::row());
                    let content = node(w, r, Layout::column());
                    CellSlot { root: r, content }
                });
                w.insert(
                    slot.root,
                    Layout {
                        width: Size::Px(c.width),
                        height: Size::Hug,
                        padding: CELL_INSET,
                        ..Layout::row()
                    },
                );
                // Rebuild only alignment spacers, never the caller's content slot.
                for child in children_of(w, slot.root) {
                    if child != slot.content {
                        destroy(w, child);
                    }
                }
                let mut aligned = vec![];
                if c.alignment != CellAlignment::Start {
                    aligned.push(node(
                        w,
                        slot.root,
                        Layout {
                            width: Size::Fill,
                            ..Layout::column()
                        },
                    ));
                }
                w.insert(
                    slot.content,
                    Layout {
                        width: if c.alignment == CellAlignment::Start {
                            Size::Fill
                        } else {
                            Size::Hug
                        },
                        height: Size::Hug,
                        ..Layout::column()
                    },
                );
                aligned.push(slot.content);
                if c.alignment == CellAlignment::Center {
                    aligned.push(node(
                        w,
                        slot.root,
                        Layout {
                            width: Size::Fill,
                            ..Layout::column()
                        },
                    ));
                }
                reorder(w, slot.root, &aligned);
                w.insert(
                    slot.root,
                    DataAccess {
                        role: AccessRole::Cell,
                        headers: vec![table.headers[&c.id].root],
                    },
                );
                w.insert(
                    slot.root,
                    Semantics {
                        name: None,
                        description: format!("Column: {}", c.label),
                        readonly: true,
                    },
                );
                compose(w, slot, item, c);
                if c.alignment != CellAlignment::Start {
                    intrinsic_labels(w, slot.content);
                }
                cells.insert((item.id, c.id), slot);
                order.push(slot.root);
            }
            reorder(w, row.content, &order);
        })?;
        table.cells = cells;
        Ok(())
    }

    fn project_table_body(world: &mut World, table: &Table, props: &TableProps, width: f32) {
        world.insert(
            table.view.root,
            Layout {
                width: Size::Px(width),
                height: Size::Px(props.body_height),
                ..Layout::column()
            },
        );
        // The backing list is an implementation detail, not an extra table/list role.
        world.insert(
            table.view.root,
            DataAccess {
                role: AccessRole::Group,
                headers: vec![],
            },
        );
    }
}
impl TableHandle {
    /// Check managed entity generations, required components and parentage without mutation.
    pub fn validate(self, w: &World) -> Result<(), UiError> {
        let t = w.get::<Table>(self.root).ok_or(UiError::Stale)?;
        if t.view != self.rows
            || !w.has::<Layout>(self.root)
            || !w.has::<Layout>(t.header)
            || !w.has::<Scroll>(self.root)
            || w.get::<Parent>(t.header).map(|p| p.entity) != Some(self.root)
        {
            return Err(UiError::Stale);
        }
        self.rows.validate(w)?;
        for h in t.headers.values() {
            h.validate(w)?;
            if w.get::<Parent>(h.root).map(|p| p.entity) != Some(t.header)
                || !w.has::<Button>(h.root)
            {
                return Err(UiError::Stale);
            }
        }
        for ((row, _), s) in &t.cells {
            if !w.has::<Layout>(s.root)
                || !w.has::<Layout>(s.content)
                || w.get::<Parent>(s.content).map(|p| p.entity) != Some(s.root)
                || w.get::<Parent>(s.root).map(|p| p.entity)
                    != self.rows.row(w, *row).map(|r| r.content)
            {
                return Err(UiError::Stale);
            }
        }
        Ok(())
    }
    /// Get a live stable cell slot by row and column identity.
    pub fn cell(self, w: &World, row: ItemId, column: ColumnId) -> Option<CellSlot> {
        self.validate(w).ok()?;
        w.get::<Table>(self.root)?
            .cells
            .get(&(row, column))
            .copied()
    }
    /// Get a live column-header entity for focus or observation.
    pub fn header(self, w: &World, column: ColumnId) -> Option<Entity> {
        self.validate(w).ok()?;
        w.get::<Table>(self.root)?
            .headers
            .get(&column)
            .map(|h| h.root)
    }
    /// Retire owned descendants and armed interactions, cancelling any dirty drafts.
    pub fn unmount(self, w: &mut World) -> Result<(), UiError> {
        self.validate(w)?;
        destroy(w, self.root);
        Ok(())
    }
}
