//! Application-owned, bounded data compositions. See `docs/data-editors.md` for
//! identity, row-only focus, resizing limits, transaction and reconciliation policy.
//! No reflection, persistence, virtualization, spreadsheet editing or app undo.
#![doc = include_str!("../../../docs/data-editors.md")]
use crate::collections::Item;
use crate::{collections::*, controls, *};
use rndr_core::*;
use std::collections::{BTreeMap, BTreeSet};

/// Maximum supplied rows in one table mount; page larger models in the host.
pub const MAX_TABLE_ROWS: usize = 256;
/// Maximum columns in one table mount.
pub const MAX_TABLE_COLUMNS: usize = 32;
/// Maximum properties in one inspector mount.
pub const MAX_PROPERTIES: usize = 128;
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

pub(crate) fn key_at(world: &World, e: Entity) -> Option<UiId> {
    world
        .get::<Table>(e)
        .map(|s| s.key)
        .or_else(|| world.get::<Inspector>(e).map(|s| s.key))
}
fn existing(world: &World, parent: Entity, key: UiId) -> Result<Option<Entity>, UiError> {
    if !world.has::<Layout>(parent) {
        return Err(UiError::Stale);
    }
    Ok(children_of(world, parent).into_iter().find(|e| {
        controls::key_at(world, *e).or_else(|| crate::surfaces::key_at(world, *e)) == Some(key)
    }))
}
fn node(world: &mut World, parent: Entity, layout: Layout) -> Entity {
    let e = world.spawn((layout,));
    set_parent(world, e, parent);
    e
}
fn owned_entities(world: &World, e: Entity, out: &mut Vec<Entity>) {
    out.push(e);
    for c in children_of(world, e) {
        owned_entities(world, c, out);
    }
}
fn destroy(world: &mut World, e: Entity) {
    let mut all = Vec::new();
    owned_entities(world, e, &mut all);
    forget_interaction_targets(world, &all);
    rndr_core::despawn(world, e);
}
fn reorder(world: &mut World, parent: Entity, children: &[Entity]) {
    for &e in children {
        set_parent(world, e, parent);
    }
    let mut list = children.to_vec();
    for e in children_of(world, parent) {
        if !list.contains(&e) {
            list.push(e);
        }
    }
    world.insert(parent, Children { list });
}
fn label(world: &mut World, parent: Entity, text: &str, typography: Typography) -> Entity {
    crate::text(world, parent, text, typography).expect("live owned slot")
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

// Inspector: typed draft transactions over existing TextField and command controls.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
/// Stable nonzero property identity, scoped to an inspector object.
pub struct PropertyId(pub u64);
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
/// Stable nonzero property-group identity.
pub struct GroupId(pub u64);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Stable nonzero host-object identity; change it on selection transitions.
pub struct ObjectId(pub u64);
#[derive(Clone, Debug, PartialEq)]
/// Explicit typed property value, never reflected from an arbitrary application object.
pub enum PropertyValue {
    /// Committed or parsed text.
    Text(String),
    /// Finite committed or parsed number.
    Number(f64),
    /// Committed or parsed boolean.
    Boolean(bool),
    /// Stable choice ID, not its label.
    Choice(u64),
    /// Stable multiple-choice IDs; empty is valid. Canonical order is numeric ID.
    Choices(Vec<u64>),
    /// Explicit caller-defined kind name or custom string value.
    Custom(String),
}
#[derive(Clone, Debug, PartialEq)]
/// Stable choice identity and independent visible label.
pub struct Choice {
    /// Stable nonzero identity in the containing owner namespace.
    pub id: u64,
    /// Human-readable visible label.
    pub label: String,
}
#[derive(Clone, Debug, PartialEq)]
/// Explicit builtin or caller-composed editor kind and validation bounds.
pub enum PropertyKind {
    /// Text editor with Unicode scalar-count limit.
    Text {
        /// Positive character limit, at most 65,536.
        max_chars: usize,
    },
    /// Finite numeric editor with inclusive finite bounds.
    Number {
        /// Inclusive lower bound.
        min: f64,
        /// Inclusive upper bound, not less than min.
        max: f64,
    },
    /// Boolean draft toggled by canonical activation.
    Boolean,
    /// Nonempty bounded set of stable choices.
    Choice(Vec<Choice>),
    /// Bounded multiple-choice draft editor; selection does not commit records.
    MultiChoice(Vec<Choice>),
    /// Explicit caller-defined kind name or custom string value.
    Custom(String),
}
/// Pure deterministic property validation after parsing and again at commit.
pub type PropertyValidator = fn(&PropertyValue) -> Result<(), String>;

#[derive(Clone, Debug)]
/// Host-owned committed property definition and validation policy.
pub struct Property {
    /// Stable nonzero identity in the containing owner namespace.
    pub id: PropertyId,
    /// Stable group identity, which must exist in the supplied groups.
    pub group: GroupId,
    /// Human-readable visible label.
    pub label: String,
    /// Additional editor help, also exposed to accessibility.
    pub description: String,
    /// Explicit editor kind and builtin bounds.
    pub kind: PropertyKind,
    /// Host committed value, or parsed draft value when this is PropertyDraft.
    pub value: PropertyValue,
    /// Interaction permission; false disables owned actionable controls.
    pub enabled: bool,
    /// Forbids edits/commits but allows text selection/copy.
    pub readonly: bool,
    /// Host-supplied validation error; blocks commit until reconciled away.
    pub validation: Option<String>,
    /// Pure deterministic validator after parsing and again at commit.
    pub validate: Option<PropertyValidator>,
}
#[derive(Clone, Debug)]
/// Ordered, labelled group of properties.
pub struct PropertyGroup {
    /// Stable nonzero identity in the containing owner namespace.
    pub id: GroupId,
    /// Human-readable visible label.
    pub label: String,
}
#[derive(Clone, Debug)]
/// Inspector identity, interaction and empty-state policy.
pub struct InspectorProps {
    /// Nonempty accessible name.
    pub name: String,
    /// Original host object identity; not a row position.
    pub object: ObjectId,
    /// Interaction permission; false disables owned actionable controls.
    pub enabled: bool,
    /// Nonempty feedback when no records or properties are supplied.
    pub empty: String,
}
impl InspectorProps {
    /// Construct enabled defaults with a nonempty accessible name.
    pub fn new(name: impl Into<String>, object: ObjectId) -> Self {
        Self {
            name: name.into(),
            object,
            enabled: true,
            empty: "No properties supplied".into(),
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
/// Local editable draft, distinct from the host committed value.
pub struct PropertyDraft {
    /// Editable raw string, including invalid intermediate input.
    pub raw: String,
    /// Host committed value, or parsed draft value when this is PropertyDraft.
    pub value: Option<PropertyValue>,
    /// Validation error when the draft is not committable.
    pub error: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
/// Why a local draft was explicitly discarded.
pub enum CancelReason {
    /// Explicit Escape or Cancel action.
    Escape,
    /// Focus left the property or the native window blurred.
    Blur,
    /// Host identity, committed value or editor contract changed.
    Reconciled,
    /// The property was removed.
    Removed,
    /// The whole inspector was unmounted.
    Unmounted,
}
#[derive(Clone, Debug, PartialEq)]
/// Draft, requested commit, or explicit cancellation. Does not mutate host data.
pub enum PropertyEventKind {
    /// The local draft changed; no model mutation.
    DraftChanged(PropertyDraft),
    /// Valid explicit commit request, to be accepted or rejected by the host.
    CommitRequested {
        /// Host committed value at request time.
        before: PropertyValue,
        /// Valid requested value; host decides whether to apply it.
        after: PropertyValue,
    },
    /// An uncommitted local draft was discarded.
    Cancelled(CancelReason),
}
#[derive(Clone, Debug, PartialEq)]
/// Typed transaction carrying the original inspector/object/property identity.
pub struct PropertyEvent {
    /// Originating managed inspector entity.
    pub inspector: Entity,
    /// Original host object identity; not a row position.
    pub object: ObjectId,
    /// Stable property identity within the original object.
    pub property: PropertyId,
    /// Explicit editor kind and builtin bounds.
    pub kind: PropertyEventKind,
}
impl Event for PropertyEvent {}
#[derive(Clone, Copy, Debug)]
/// Managed property controls. Compose custom controls only beneath editor.
pub struct PropertySlot {
    /// Component-owned root; do not reparent or modify its managed children.
    pub root: Entity,
    /// Canonical builtin editor, or caller-owned custom-editor descendants.
    pub editor: Entity,
    /// Explicit canonical Commit control.
    pub commit: Entity,
    /// Explicit canonical Cancel control.
    pub cancel: Entity,
    /// Component-owned draft/validation feedback label.
    pub feedback: Entity,
}
#[derive(Clone)]
struct PropertyState {
    property: Property,
    slot: PropertySlot,
    draft: PropertyDraft,
    // Outstanding requests survive later edits until acknowledged or cancelled.
    pending: Vec<PropertyValue>,
    dirty: bool,
}
#[derive(Clone)]
struct Inspector {
    key: UiId,
    props: InspectorProps,
    groups: BTreeMap<GroupId, Entity>,
    fields: BTreeMap<PropertyId, PropertyState>,
    empty: Entity,
}
impl Component for Inspector {}
#[derive(Clone, Copy)]
struct PropertyOwner {
    inspector: Entity,
    property: PropertyId,
    action: u8,
}
impl Component for PropertyOwner {}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Validated handle for explicit draft/commit/cancel and lifecycle operations.
pub struct InspectorHandle {
    /// Component-owned root; do not reparent or modify its managed children.
    pub root: Entity,
}
/// Reusable grouped typed-editor composition over canonical field/control primitives.
pub struct PropertyInspector;
fn display(v: &PropertyValue) -> String {
    match v {
        PropertyValue::Text(s) | PropertyValue::Custom(s) => s.clone(),
        PropertyValue::Number(n) => n.to_string(),
        PropertyValue::Boolean(b) => b.to_string(),
        PropertyValue::Choice(id) => id.to_string(),
        PropertyValue::Choices(ids) => ids.iter().map(u64::to_string).collect::<Vec<_>>().join(","),
    }
}
fn parsed(p: &Property, raw: &str) -> PropertyDraft {
    let parsed = match &p.kind {
        PropertyKind::Text { max_chars } => {
            if raw.chars().count() <= *max_chars {
                Ok(PropertyValue::Text(raw.into()))
            } else {
                Err(format!("Maximum {max_chars} characters"))
            }
        }
        PropertyKind::Number { min, max } => match raw.parse::<f64>() {
            Ok(n) if n.is_finite() && n >= *min && n <= *max => Ok(PropertyValue::Number(n)),
            _ => Err(format!("Enter a finite number from {min} to {max}")),
        },
        PropertyKind::Boolean => match raw {
            "true" => Ok(PropertyValue::Boolean(true)),
            "false" => Ok(PropertyValue::Boolean(false)),
            _ => Err("Choose true or false".into()),
        },
        PropertyKind::Choice(options) => raw
            .parse::<u64>()
            .ok()
            .filter(|id| options.iter().any(|o| o.id == *id))
            .map(PropertyValue::Choice)
            .ok_or("Choose an available value".into()),
        PropertyKind::MultiChoice(options) => {
            let ids: Result<Vec<u64>, _> = if raw.trim().is_empty() {
                Ok(vec![])
            } else {
                raw.split(',').map(str::parse::<u64>).collect()
            };
            ids.ok()
                .filter(|ids| {
                    ids.len() <= 64
                        && ids.iter().all(|id| options.iter().any(|o| o.id == *id))
                        && ids.iter().collect::<BTreeSet<_>>().len() == ids.len()
                })
                .map(|mut ids| {
                    ids.sort();
                    PropertyValue::Choices(ids)
                })
                .ok_or("Choose available values without duplicates".into())
        }
        PropertyKind::Custom(_) => Ok(PropertyValue::Custom(raw.into())),
    };
    match parsed.and_then(|v| {
        if let Some(validate) = p.validate {
            validate(&v)?;
        }
        Ok(v)
    }) {
        Ok(v) => PropertyDraft {
            raw: raw.into(),
            value: Some(v),
            error: None,
        },
        Err(error) => PropertyDraft {
            raw: raw.into(),
            value: None,
            error: Some(error),
        },
    }
}
// Choices are sets at the inspector boundary. Keep host records (and all
// other PropertyValue equality/raw-draft contracts) untouched.
fn same_value(a: &PropertyValue, b: &PropertyValue) -> bool {
    match (a, b) {
        (PropertyValue::Choices(a), PropertyValue::Choices(b)) => {
            let (mut a, mut b) = (a.clone(), b.clone());
            a.sort_unstable();
            b.sort_unstable();
            a == b
        }
        _ => a == b,
    }
}
fn draft_dirty(p: &Property, d: &PropertyDraft) -> bool {
    if matches!(p.kind, PropertyKind::MultiChoice(_)) {
        // App validation may reject a well-typed set. Equality is still about
        // selected identities; malformed raw drafts remain dirty.
        let mut unvalidated = p.clone();
        unvalidated.validate = None;
        parsed(&unvalidated, &d.raw)
            .value
            .is_none_or(|v| !same_value(&v, &p.value))
    } else {
        d.raw != display(&p.value)
    }
}
fn base(p: &Property) -> PropertyDraft {
    let mut d = parsed(p, &display(&p.value));
    if let Some(e) = &p.validation {
        d.error = Some(e.clone());
    }
    d
}
fn inspector_valid(props: &InspectorProps, groups: &[PropertyGroup], fields: &[Property]) -> bool {
    if props.name.trim().is_empty()
        || props.empty.trim().is_empty()
        || props.object.0 == 0
        || groups.len() > 32
        || fields.len() > MAX_PROPERTIES
    {
        return false;
    }
    let ids: BTreeSet<_> = groups.iter().map(|g| g.id).collect();
    if ids.len() != groups.len()
        || groups
            .iter()
            .any(|g| g.id.0 == 0 || g.label.trim().is_empty())
        || fields.iter().map(|p| p.id).collect::<BTreeSet<_>>().len() != fields.len()
    {
        return false;
    }
    fields.iter().all(|p| {
        if p.id.0 == 0 || p.label.trim().is_empty() || !ids.contains(&p.group) {
            return false;
        }
        match (&p.kind, &p.value) {
            (PropertyKind::Text { max_chars }, PropertyValue::Text(s)) => {
                *max_chars > 0 && *max_chars <= 65536 && s.chars().count() <= *max_chars
            }
            (PropertyKind::Number { min, max }, PropertyValue::Number(n)) => {
                min.is_finite()
                    && max.is_finite()
                    && min <= max
                    && n.is_finite()
                    && n >= min
                    && n <= max
            }
            (PropertyKind::Boolean, PropertyValue::Boolean(_)) => true,
            (PropertyKind::Choice(options), PropertyValue::Choice(id)) => {
                !options.is_empty()
                    && options.len() <= 64
                    && options
                        .iter()
                        .all(|o| o.id != 0 && !o.label.trim().is_empty())
                    && options.iter().map(|o| o.id).collect::<BTreeSet<_>>().len() == options.len()
                    && options.iter().any(|o| o.id == *id)
            }
            (PropertyKind::MultiChoice(options), PropertyValue::Choices(ids)) => {
                options.len() <= 64
                    && ids.len() <= 64
                    && options
                        .iter()
                        .all(|o| o.id != 0 && !o.label.trim().is_empty())
                    && options.iter().map(|o| o.id).collect::<BTreeSet<_>>().len() == options.len()
                    && ids.iter().collect::<BTreeSet<_>>().len() == ids.len()
                    && ids.iter().all(|id| options.iter().any(|o| o.id == *id))
            }
            (PropertyKind::Custom(name), PropertyValue::Custom(s)) => {
                !name.trim().is_empty() && s.len() <= 65536
            }
            _ => false,
        }
    })
}
fn emit(w: &mut World, root: Entity, i: &Inspector, id: PropertyId, kind: PropertyEventKind) {
    send(
        w,
        PropertyEvent {
            inspector: root,
            object: i.props.object,
            property: id,
            kind,
        },
    );
}
fn owner(w: &World, e: Entity) -> Option<PropertyOwner> {
    w.get::<PropertyOwner>(e).copied()
}
fn field_signal(w: &mut World, e: Entity, s: FieldTransaction) {
    let Some(o) = owner(w, e) else {
        return;
    };
    let h = InspectorHandle { root: o.inspector };
    reveal_entity(w, e);
    match s {
        FieldTransaction::Changed(raw) => {
            if h.set_draft(w, o.property, &raw).is_err() {
                if let Some(d) = h.draft(w, o.property) {
                    if let Some(slot) = h.slot(w, o.property) {
                        reset_field_draft(w, slot.editor, &d.raw);
                    }
                }
            }
        }
        FieldTransaction::Submit(_) => {
            let _ = h.commit(w, o.property);
        }
        FieldTransaction::Cancel => {
            let _ = h.cancel(w, o.property, CancelReason::Escape);
        }
        // The property's FocusBoundary handles all descendants, not just TextFields.
        FieldTransaction::FocusLost(_) => {}
    }
}
fn property_departure(w: &mut World, e: Entity, _: Option<Entity>) {
    if let Some(o) = owner(w, e) {
        let _ = InspectorHandle { root: o.inspector }.cancel(w, o.property, CancelReason::Blur);
    }
}
fn property_key(w: &mut World, e: Entity, k: &KeyEvent) -> bool {
    if k.logical == LogicalKey::Named(NamedKey::Escape) && !k.repeat && !k.released && !k.synthetic
    {
        if let Some(o) = owner(w, e) {
            let _ =
                InspectorHandle { root: o.inspector }.cancel(w, o.property, CancelReason::Escape);
            return true;
        }
    }
    false
}
fn mount_selection(
    w: &mut World,
    parent: Entity,
    p: &Property,
    value: &PropertyValue,
    enabled: bool,
) -> Result<Entity, UiError> {
    use selection::*;
    let mut props = SelectionProps::new(&p.label);
    props.description = p.description.clone();
    props.enabled = enabled;
    props.readonly = p.readonly;
    let options = match &p.kind {
        PropertyKind::Choice(o) | PropertyKind::MultiChoice(o) => o
            .iter()
            .map(|o| SelectOption::new(o.id, &o.label))
            .collect::<Vec<_>>(),
        _ => vec![],
    };
    let h = match (&p.kind, value) {
        (PropertyKind::Boolean, PropertyValue::Boolean(b)) => {
            Checkbox::mount(w, parent, UiId(2), props, *b)?
        }
        (PropertyKind::Choice(_), PropertyValue::Choice(id)) => {
            SingleSelect::mount(w, parent, UiId(2), props, &options, Some(OptionId(*id)))?
        }
        (PropertyKind::MultiChoice(_), PropertyValue::Choices(ids)) => MultiSelect::mount(
            w,
            parent,
            UiId(2),
            props,
            &options,
            &ids.iter().map(|id| OptionId(*id)).collect::<Vec<_>>(),
        )?,
        _ => return Err(UiError::InvalidProps),
    };
    Ok(h.root)
}
fn property_selection(w: &mut World, e: Entity, value: &selection::SelectionValue) {
    let Some(o) = owner(w, e) else { return };
    let raw = match value {
        selection::SelectionValue::Checked(b) => b.to_string(),
        selection::SelectionValue::Selected(ids) => ids
            .iter()
            .map(|id| id.0.to_string())
            .collect::<Vec<_>>()
            .join(","),
    };
    let _ = (InspectorHandle { root: o.inspector }).set_draft(w, o.property, &raw);
}
fn property_selection_key(w: &mut World, e: Entity, k: &KeyEvent) -> bool {
    selection::navigate(w, e, k) || property_key(w, e, k)
}
fn property_activate(w: &mut World, e: Entity) {
    let Some(o) = owner(w, e) else {
        return;
    };
    let h = InspectorHandle { root: o.inspector };
    match o.action {
        1 => {
            let _ = h.commit(w, o.property);
        }
        2 => {
            let _ = h.cancel(w, o.property, CancelReason::Escape);
        }
        _ => {} // Selection editors use their typed ChangeHandler; custom editors own drafts.
    }
}
fn update_field(w: &mut World, i: &Inspector, s: &PropertyState) {
    let p = &s.property;
    let enabled = i.props.enabled && p.enabled;
    let editing = enabled && !p.readonly;
    let shown = match (&p.kind, &s.draft.value) {
        (PropertyKind::Choice(options), Some(PropertyValue::Choice(id))) => options
            .iter()
            .find(|c| c.id == *id)
            .map(|c| c.label.clone())
            .unwrap_or_default(),
        (PropertyKind::MultiChoice(options), _) => {
            let mut unvalidated = p.clone();
            unvalidated.validate = None;
            match parsed(&unvalidated, &s.draft.raw).value {
                Some(PropertyValue::Choices(ids)) if ids.is_empty() => "None selected".into(),
                Some(PropertyValue::Choices(ids)) => options
                    .iter()
                    .filter(|o| ids.contains(&o.id))
                    .map(|o| o.label.as_str())
                    .collect::<Vec<_>>()
                    .join(", "),
                _ => format!("Invalid selection: {}", s.draft.raw),
            }
        }
        _ => s.draft.raw.clone(),
    };
    if matches!(
        p.kind,
        PropertyKind::Text { .. } | PropertyKind::Number { .. }
    ) {
        if let Some(l) = w.get_mut::<Label>(s.slot.editor) {
            l.text = shown.clone();
        }
    } else if matches!(
        p.kind,
        PropertyKind::Boolean | PropertyKind::Choice(_) | PropertyKind::MultiChoice(_)
    ) {
        // A validator can reject an otherwise well-typed selection. Still show
        // the requested draft (including empty multi-selection), not the old model.
        let mut unvalidated = p.clone();
        unvalidated.validate = None;
        let candidate = parsed(&unvalidated, &s.draft.raw).value;
        let value = s
            .draft
            .value
            .as_ref()
            .or(candidate.as_ref())
            .unwrap_or(&p.value);
        let _ = mount_selection(w, s.slot.root, p, value, enabled);
        w.insert(s.slot.editor, selection::ChangeHandler(property_selection));
        w.insert(s.slot.editor, KeyNavigation(property_selection_key));
    }
    if enabled {
        w.remove::<ControlDisabled>(s.slot.editor);
    } else {
        w.insert(s.slot.editor, ControlDisabled);
    }
    w.insert(
        s.slot.editor,
        Semantics {
            name: Some(
                if matches!(
                    p.kind,
                    PropertyKind::Boolean | PropertyKind::Choice(_) | PropertyKind::MultiChoice(_)
                ) {
                    format!("{}: {}", p.label, shown)
                } else {
                    p.label.clone()
                },
            ),
            description: format!(
                "{} {}",
                p.description,
                if p.readonly {
                    "Read only"
                } else {
                    "Draft; Enter commits text/number, Escape cancels; leaving property cancels"
                }
            ),
            readonly: p.readonly,
        },
    );
    let message = if let Some(e) = &s.draft.error {
        format!("Invalid: {e}")
    } else if !s.pending.is_empty() {
        "Commit requested; awaiting model reconcile".into()
    } else if s.dirty {
        "Draft — not committed".into()
    } else if p.readonly {
        "Read only".into()
    } else {
        "Committed value".into()
    };
    w.get_mut::<Label>(s.slot.feedback).unwrap().text = message;
    for (entity, key, title, ok) in [
        (
            s.slot.commit,
            1,
            "Commit",
            editing
                && s.dirty
                && s.draft.error.is_none()
                && s.pending.last() != s.draft.value.as_ref(),
        ),
        (s.slot.cancel, 2, "Cancel", editing && s.dirty),
    ] {
        // Do not strand focus on an action that becomes disabled after a request.
        // Staying inside this property preserves later drafts/pending acknowledgements.
        if editing && !ok && w.resource::<Focus>().and_then(|f| f.entity) == Some(entity) {
            request_focus(w, s.slot.editor);
        }
        if let Some(parent) = w.get::<Parent>(entity).map(|p| p.entity) {
            let mut bp = controls::ButtonProps::new(title, controls::HostIntent::Command(p.id.0));
            bp.enabled = ok;
            let _ = controls::Button::mount(w, parent, UiId(key), bp);
            w.insert(entity, ActivationHandler(property_activate));
        }
    }
}
impl PropertyInspector {
    /// Parent retains committed values. Reconcile preserves a draft only while
    /// object/property/kind/value/enable/read-only/validator identities agree;
    /// changed or removed identities explicitly cancel before any replacement.
    pub fn mount(
        w: &mut World,
        parent: Entity,
        key: UiId,
        props: InspectorProps,
        groups: &[PropertyGroup],
        fields: &[Property],
        mut custom: impl FnMut(&mut World, PropertySlot, &Property),
    ) -> Result<InspectorHandle, UiError> {
        if !inspector_valid(&props, groups, fields) {
            return Err(UiError::InvalidProps);
        }
        let (root, mut inspector) = Self::resolve_inspector(w, parent, key, &props)?;
        Self::retire_fields_and_groups(w, root, &mut inspector, &props, groups, fields);
        let old_enabled = inspector.props.enabled;
        let group_order = Self::project_groups(w, root, &mut inspector, &props, groups, fields);
        Self::reconcile_fields(w, root, &mut inspector, old_enabled, fields)?;
        Self::order_fields(w, root, &inspector, groups, fields, &group_order);
        // Custom callbacks follow identity order and observe the previous publication.
        for state in inspector.fields.values() {
            update_field(w, &inspector, state);
            if matches!(state.property.kind, PropertyKind::Custom(_)) {
                custom(w, state.slot, &state.property);
            }
        }
        w.insert(root, inspector);
        Ok(InspectorHandle { root })
    }

    fn resolve_inspector(
        w: &mut World,
        parent: Entity,
        key: UiId,
        props: &InspectorProps,
    ) -> Result<(Entity, Inspector), UiError> {
        let found = existing(w, parent, key)?;
        Ok(if let Some(root) = found {
            let i = w
                .get::<Inspector>(root)
                .cloned()
                .ok_or(UiError::IdentityConflict)?;
            InspectorHandle { root }.validate(w)?;
            (root, i)
        } else {
            let root = node(
                w,
                parent,
                Layout {
                    width: Size::Fill,
                    height: Size::Fill,
                    padding: 8.0,
                    gap: 8.0,
                    ..Layout::column()
                },
            );
            w.insert(root, Scroll::default());
            w.insert(
                root,
                DataAccess {
                    role: AccessRole::Group,
                    headers: vec![],
                },
            );
            let empty = label(w, root, &props.empty, Typography::Secondary);
            (
                root,
                Inspector {
                    key,
                    props: props.clone(),
                    groups: BTreeMap::new(),
                    fields: BTreeMap::new(),
                    empty,
                },
            )
        })
    }

    fn retire_fields_and_groups(
        world: &mut World,
        root: Entity,
        inspector: &mut Inspector,
        props: &InspectorProps,
        groups: &[PropertyGroup],
        fields: &[Property],
    ) {
        let object_changed = inspector.props.object != props.object;
        let field_ids: BTreeSet<_> = fields.iter().map(|property| property.id).collect();
        let removed: Vec<_> = inspector
            .fields
            .keys()
            .copied()
            .filter(|id| !field_ids.contains(id) || object_changed)
            .collect();
        for id in removed {
            let state = inspector.fields.remove(&id).unwrap();
            if state.dirty {
                emit(
                    world,
                    root,
                    inspector,
                    id,
                    PropertyEventKind::Cancelled(if object_changed {
                        CancelReason::Reconciled
                    } else {
                        CancelReason::Removed
                    }),
                );
            }
            destroy(world, state.slot.root);
        }
        let group_ids: BTreeSet<_> = groups.iter().map(|group| group.id).collect();
        // A removed group may still contain retained fields moving to another
        // group. Reparent those first, then destroy the now-empty old group.
        for state in inspector.fields.values() {
            set_parent(world, state.slot.root, root);
        }
        inspector.groups.retain(|id, entity| {
            let keep = group_ids.contains(id);
            if !keep {
                destroy(world, *entity);
            }
            keep
        });
    }

    fn project_groups(
        w: &mut World,
        root: Entity,
        i: &mut Inspector,
        props: &InspectorProps,
        groups: &[PropertyGroup],
        fields: &[Property],
    ) -> Vec<Entity> {
        i.props = props.clone();
        w.insert(
            root,
            Semantics {
                name: Some(props.name.clone()),
                description:
                    "Application-owned typed properties; draft edits are not model mutations."
                        .into(),
                readonly: false,
            },
        );
        w.get_mut::<Label>(i.empty).unwrap().text = props.empty.clone();
        if fields.is_empty() {
            w.remove::<Hidden>(i.empty);
        } else {
            w.insert(i.empty, Hidden);
        }
        let mut group_order = vec![i.empty];
        for g in groups {
            let e = if let Some(&e) = i.groups.get(&g.id) {
                e
            } else {
                let e = node(
                    w,
                    root,
                    Layout {
                        width: Size::Fill,
                        gap: 8.0,
                        ..Layout::column()
                    },
                );
                w.insert(
                    e,
                    DataAccess {
                        role: AccessRole::Group,
                        headers: vec![],
                    },
                );
                label(w, e, &g.label, Typography::Label);
                i.groups.insert(g.id, e);
                e
            };
            if let Some(&title) = children_of(w, e).first() {
                if let Some(l) = w.get_mut::<Label>(title) {
                    l.text = g.label.clone();
                }
            }
            w.insert(
                e,
                Semantics {
                    name: Some(g.label.clone()),
                    ..Semantics::default()
                },
            );
            group_order.push(e);
        }
        group_order
    }

    fn reconcile_fields(
        w: &mut World,
        root: Entity,
        i: &mut Inspector,
        old_enabled: bool,
        fields: &[Property],
    ) -> Result<(), UiError> {
        for property in fields {
            Self::reconcile_property_draft(w, root, i, old_enabled, property);
            Self::create_missing_property(w, root, i, property)?;
            Self::project_property(w, root, i, property);
        }
        Ok(())
    }

    fn reconcile_property_draft(
        world: &mut World,
        root: Entity,
        inspector: &mut Inspector,
        old_enabled: bool,
        property: &Property,
    ) {
        if let Some(state) = inspector.fields.get_mut(&property.id) {
            let same_validator = match (state.property.validate, property.validate) {
                (None, None) => true,
                (Some(previous), Some(current)) => std::ptr::fn_addr_eq(previous, current),
                _ => false,
            };
            let contract_changed = state.property.kind != property.kind
                || state.property.readonly != property.readonly
                || state.property.enabled != property.enabled
                || state.property.validation != property.validation
                || old_enabled != inspector.props.enabled
                || !same_validator;
            let acknowledgement = (!contract_changed
                && !same_value(&state.property.value, &property.value))
            .then(|| {
                state
                    .pending
                    .iter()
                    .position(|value| same_value(value, &property.value))
            })
            .flatten();
            if let Some(accepted_index) = acknowledgement {
                // An earlier request was accepted. Later edits are still local:
                // do not replace the label, caret, IME state or typing history.
                state.pending.drain(..=accepted_index);
                state.dirty = draft_dirty(property, &state.draft);
            } else if contract_changed || !same_value(&state.property.value, &property.value) {
                if state.dirty {
                    send(
                        world,
                        PropertyEvent {
                            inspector: root,
                            object: inspector.props.object,
                            property: property.id,
                            kind: PropertyEventKind::Cancelled(CancelReason::Reconciled),
                        },
                    );
                }
                state.draft = base(property);
                state.pending.clear();
                state.dirty = false;
                world.remove::<Caret>(state.slot.editor);
                reset_field_draft(world, state.slot.editor, &state.draft.raw);
            }
            if state.property.kind != property.kind {
                destroy(world, state.slot.root);
                inspector.fields.remove(&property.id);
            }
        }
    }

    fn create_missing_property(
        w: &mut World,
        root: Entity,
        i: &mut Inspector,
        p: &Property,
    ) -> Result<(), UiError> {
        if !i.fields.contains_key(&p.id) {
            let r = node(
                w,
                i.groups[&p.group],
                Layout {
                    width: Size::Fill,
                    gap: 4.0,
                    padding: 8.0,
                    ..Layout::column()
                },
            );
            w.insert(
                r,
                DataAccess {
                    role: AccessRole::Group,
                    headers: vec![],
                },
            );
            label(w, r, &p.label, Typography::Label);
            label(w, r, &p.description, Typography::Secondary);
            let editor = match p.kind {
                PropertyKind::Text { .. } | PropertyKind::Number { .. } => {
                    let e = node(
                        w,
                        r,
                        Layout {
                            width: Size::Fill,
                            height: Size::Px(40.0),
                            ..Layout::column()
                        },
                    );
                    w.insert(e, Label::new(display(&p.value)));
                    w.insert(e, TextField);
                    w.insert(e, Focusable);
                    w.insert(e, CapturesKeys);
                    w.insert(e, FieldTransactions(field_signal));
                    w.insert(e, UiStyle::editor());
                    e
                }
                PropertyKind::Boolean | PropertyKind::Choice(_) | PropertyKind::MultiChoice(_) => {
                    mount_selection(w, r, p, &p.value, i.props.enabled && p.enabled)?
                }
                PropertyKind::Custom(_) => node(
                    w,
                    r,
                    Layout {
                        width: Size::Fill,
                        ..Layout::column()
                    },
                ),
            };
            let actions = node(
                w,
                r,
                Layout {
                    width: Size::Fill,
                    gap: 4.0,
                    ..Layout::row()
                },
            );
            let commit = controls::Button::mount(
                w,
                actions,
                UiId(1),
                controls::ButtonProps::new("Commit", controls::HostIntent::Command(p.id.0)),
            )?
            .root;
            let cancel = controls::Button::mount(
                w,
                actions,
                UiId(2),
                controls::ButtonProps::new("Cancel", controls::HostIntent::Command(p.id.0)),
            )?
            .root;
            for (e, action) in [(editor, 0), (commit, 1), (cancel, 2)] {
                w.insert(
                    e,
                    PropertyOwner {
                        inspector: root,
                        property: p.id,
                        action,
                    },
                );
                if e != editor {
                    w.insert(e, KeyNavigation(property_key));
                    w.insert(e, ActivationHandler(property_activate));
                } else if matches!(
                    p.kind,
                    PropertyKind::Boolean | PropertyKind::Choice(_) | PropertyKind::MultiChoice(_)
                ) {
                    w.insert(e, selection::ChangeHandler(property_selection));
                    w.insert(e, KeyNavigation(property_selection_key));
                } else {
                    w.insert(e, KeyNavigation(property_key));
                    if matches!(p.kind, PropertyKind::Custom(_)) {
                        w.insert(e, ActivationHandler(property_activate));
                    }
                }
            }
            let feedback = label(w, r, "", Typography::Secondary);
            i.fields.insert(
                p.id,
                PropertyState {
                    property: p.clone(),
                    slot: PropertySlot {
                        root: r,
                        editor,
                        commit,
                        cancel,
                        feedback,
                    },
                    draft: base(p),
                    pending: Vec::new(),
                    dirty: false,
                },
            );
        }
        Ok(())
    }

    fn project_property(w: &mut World, root: Entity, i: &mut Inspector, p: &Property) {
        let s = i.fields.get_mut(&p.id).unwrap();
        s.property = p.clone();
        w.insert(
            s.slot.root,
            PropertyOwner {
                inspector: root,
                property: p.id,
                action: 0,
            },
        );
        w.insert(s.slot.root, FocusBoundary(property_departure));
        set_parent(w, s.slot.root, i.groups[&p.group]);
        let children = children_of(w, s.slot.root);
        w.get_mut::<Label>(children[0]).unwrap().text = p.label.clone();
        w.get_mut::<Label>(children[1]).unwrap().text = p.description.clone();
    }

    fn order_fields(
        w: &mut World,
        root: Entity,
        i: &Inspector,
        groups: &[PropertyGroup],
        fields: &[Property],
        group_order: &[Entity],
    ) {
        for g in groups {
            let e = i.groups[&g.id];
            let mut order = children_of(w, e)
                .into_iter()
                .filter(|e| w.has::<Label>(*e))
                .take(1)
                .collect::<Vec<_>>();
            order.extend(
                fields
                    .iter()
                    .filter(|p| p.group == g.id)
                    .map(|p| i.fields[&p.id].slot.root),
            );
            reorder(w, e, &order);
        }
        reorder(w, root, group_order);
    }
}
impl InspectorHandle {
    /// Check managed entity generations, required components and parentage without mutation.
    pub fn validate(self, w: &World) -> Result<(), UiError> {
        let i = w.get::<Inspector>(self.root).ok_or(UiError::Stale)?;
        if !w.has::<Layout>(self.root)
            || !w.has::<Scroll>(self.root)
            || !w.has::<Label>(i.empty)
            || w.get::<Parent>(i.empty).map(|p| p.entity) != Some(self.root)
        {
            return Err(UiError::Stale);
        }
        for &g in i.groups.values() {
            let kids = children_of(w, g);
            if !w.has::<Layout>(g)
                || w.get::<Parent>(g).map(|p| p.entity) != Some(self.root)
                || kids.first().is_none_or(|e| !w.has::<Label>(*e))
            {
                return Err(UiError::Stale);
            }
        }
        for (id, s) in &i.fields {
            if [
                s.slot.root,
                s.slot.editor,
                s.slot.commit,
                s.slot.cancel,
                s.slot.feedback,
            ]
            .iter()
            .any(|e| !w.has::<Layout>(*e))
                || w.get::<Parent>(s.slot.root).map(|p| p.entity)
                    != i.groups.get(&s.property.group).copied()
            {
                return Err(UiError::Stale);
            }
            let kids = children_of(w, s.slot.root);
            if kids.len() != 5
                || !w.has::<Label>(kids[0])
                || !w.has::<Label>(kids[1])
                || kids[2] != s.slot.editor
                || kids[4] != s.slot.feedback
                || !w.has::<Label>(s.slot.feedback)
                || !w.has::<Layout>(kids[3])
            {
                return Err(UiError::Stale);
            }
            if w.get::<PropertyOwner>(s.slot.editor)
                .is_none_or(|o| o.inspector != self.root || o.property != *id)
            {
                return Err(UiError::Stale);
            }
            for e in [s.slot.editor, s.slot.feedback] {
                if w.get::<Parent>(e).map(|p| p.entity) != Some(s.slot.root) {
                    return Err(UiError::Stale);
                }
            }
            for e in [s.slot.commit, s.slot.cancel] {
                if !w.has::<Button>(e) || w.get::<Parent>(e).map(|p| p.entity) != Some(kids[3]) {
                    return Err(UiError::Stale);
                }
            }
            if matches!(
                s.property.kind,
                PropertyKind::Text { .. } | PropertyKind::Number { .. }
            ) && (!w.has::<TextField>(s.slot.editor)
                || !w.has::<FieldTransactions>(s.slot.editor)
                || !w.has::<Label>(s.slot.editor))
            {
                return Err(UiError::Stale);
            }
        }
        Ok(())
    }
    /// Get the live managed controls for a stable property identity.
    pub fn slot(self, w: &World, id: PropertyId) -> Option<PropertySlot> {
        self.validate(w).ok()?;
        Some(w.get::<Inspector>(self.root)?.fields.get(&id)?.slot)
    }
    /// Read a clone of the local draft; this is not the host committed value.
    pub fn draft(self, w: &World, id: PropertyId) -> Option<PropertyDraft> {
        self.validate(w).ok()?;
        Some(
            w.get::<Inspector>(self.root)?
                .fields
                .get(&id)?
                .draft
                .clone(),
        )
    }
    /// Update a local draft and validate it, without committing host data.
    /// Disabled/read-only or over-65,536-byte drafts are rejected.
    pub fn set_draft(self, w: &mut World, id: PropertyId, raw: &str) -> Result<(), UiError> {
        self.validate(w)?;
        let mut i = w.get::<Inspector>(self.root).unwrap().clone();
        let s = i.fields.get_mut(&id).ok_or(UiError::InvalidProps)?;
        if !i.props.enabled || !s.property.enabled || s.property.readonly || raw.len() > 65536 {
            return Err(UiError::InvalidProps);
        }
        if s.draft.raw == raw {
            return Ok(());
        }
        s.draft = parsed(&s.property, raw);
        if let Some(e) = &s.property.validation {
            s.draft.error = Some(e.clone());
        }
        s.dirty = draft_dirty(&s.property, &s.draft);
        let draft = s.draft.clone();
        update_field(w, &i, &i.fields[&id]);
        emit(w, self.root, &i, id, PropertyEventKind::DraftChanged(draft));
        w.insert(self.root, i);
        Ok(())
    }
    /// Request a valid changed value. False means invalid, unchanged or already pending.
    /// The host must process PropertyEvent and reconcile accepted committed values.
    pub fn commit(self, w: &mut World, id: PropertyId) -> Result<bool, UiError> {
        self.validate(w)?;
        let mut i = w.get::<Inspector>(self.root).unwrap().clone();
        let s = i.fields.get_mut(&id).ok_or(UiError::InvalidProps)?;
        if !i.props.enabled || !s.property.enabled || s.property.readonly {
            return Err(UiError::InvalidProps);
        }
        // Revalidate deterministically at the transaction boundary, including app validator.
        let d = parsed(&s.property, &s.draft.raw);
        s.draft = d;
        if let Some(e) = &s.property.validation {
            s.draft.error = Some(e.clone());
        }
        if s.draft.error.is_some() {
            update_field(w, &i, &i.fields[&id]);
            w.insert(self.root, i);
            return Ok(false);
        }
        let after = s.draft.value.clone().unwrap();
        if same_value(&after, &s.property.value)
            || s.pending.last().is_some_and(|v| same_value(v, &after))
        {
            return Ok(false);
        }
        let before = s.property.value.clone();
        s.pending.push(after.clone());
        update_field(w, &i, &i.fields[&id]);
        emit(
            w,
            self.root,
            &i,
            id,
            PropertyEventKind::CommitRequested { before, after },
        );
        w.insert(self.root, i);
        Ok(true)
    }
    /// Restore the host value and discard local typing history; emits cancellation if dirty.
    pub fn cancel(
        self,
        w: &mut World,
        id: PropertyId,
        reason: CancelReason,
    ) -> Result<(), UiError> {
        self.validate(w)?;
        let mut i = w.get::<Inspector>(self.root).unwrap().clone();
        let s = i.fields.get_mut(&id).ok_or(UiError::InvalidProps)?;
        let dirty = s.dirty;
        s.draft = base(&s.property);
        s.dirty = false;
        s.pending.clear();
        w.remove::<Caret>(s.slot.editor);
        clear_typing_history(w, s.slot.editor);
        update_field(w, &i, &i.fields[&id]);
        if dirty {
            emit(w, self.root, &i, id, PropertyEventKind::Cancelled(reason));
        }
        w.insert(self.root, i);
        Ok(())
    }
    /// Retire owned descendants and armed interactions, cancelling any dirty drafts.
    pub fn unmount(self, w: &mut World) -> Result<(), UiError> {
        self.validate(w)?;
        let i = w.get::<Inspector>(self.root).unwrap().clone();
        for (id, s) in &i.fields {
            if s.dirty {
                emit(
                    w,
                    self.root,
                    &i,
                    *id,
                    PropertyEventKind::Cancelled(CancelReason::Unmounted),
                );
            }
        }
        destroy(w, self.root);
        Ok(())
    }
}
fn clear_typing_history(w: &mut World, e: Entity) {
    // replacement is draft reset, not an undoable property commit
    let text = w
        .get::<Label>(e)
        .map(|l| l.text.clone())
        .unwrap_or_default();
    reset_field_draft(w, e, &text);
}
pub(crate) fn inspector_blur(w: &mut World) {
    let focus = w.resource::<Focus>().and_then(|f| f.entity);
    for root in w.entities_with::<Inspector>() {
        let Some(i) = w.get::<Inspector>(root).cloned() else {
            continue;
        };
        let mut cancellations = vec![];
        for (id, s) in &i.fields {
            if !s.dirty {
                continue;
            }
            let mut target = focus;
            let mut inside = false;
            for _ in 0..64 {
                let Some(e) = target else {
                    break;
                };
                if e == s.slot.root {
                    inside = true;
                    break;
                }
                target = w
                    .get::<FocusOwner>(e)
                    .map(|o| o.0)
                    .or_else(|| w.get::<Parent>(e).map(|p| p.entity));
            }
            if !inside {
                cancellations.push(*id);
            }
        }
        for id in cancellations {
            let _ = InspectorHandle { root }.cancel(w, id, CancelReason::Blur);
        }
    }
}

/// Refresh builtin field state using the shared semantic theme, not a parallel
/// text or layout engine. Custom editor subtrees remain application-owned.
pub(crate) fn styles(w: &mut World) {
    let theme = w.resource::<UiTheme>().cloned().unwrap_or_default();
    let focus = w.resource::<Focus>().and_then(|f| f.entity);
    for root in w.entities_with::<Inspector>() {
        let Some(i) = w.get::<Inspector>(root).cloned() else {
            continue;
        };
        for s in i.fields.values() {
            if w.has::<TextField>(s.slot.editor) {
                let mut style = UiStyle::editor();
                if !i.props.enabled || !s.property.enabled {
                    style.foreground = Some(Token::Disabled.into());
                } else if focus == Some(s.slot.editor) {
                    style.background = Some(Token::InfoFill.into());
                    style.foreground = Some(Token::InfoText.into());
                }
                w.insert(s.slot.editor, style);
                if let Some(l) = w.get_mut::<Layout>(s.slot.editor) {
                    l.height = Size::Px(
                        (theme
                            .typography(Typography::Code)
                            .line_box()
                            .unwrap_or(theme.typography(Typography::Code).size * 1.4)
                            + 16.)
                            .max(40.),
                    );
                }
            }
            w.insert(
                s.slot.feedback,
                UiStyle {
                    foreground: Some(
                        if s.draft.error.is_some() {
                            Token::DangerText
                        } else {
                            Token::Secondary
                        }
                        .into(),
                    ),
                    ..UiStyle::default()
                },
            );
        }
    }
}
