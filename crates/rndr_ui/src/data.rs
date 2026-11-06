//! Application-owned, bounded data compositions. See `docs/data-editors.md` for
//! identity, row-only focus, resizing limits, transaction and reconciliation policy.
//! No reflection, persistence, virtualization, spreadsheet editing or app undo.
#![doc = include_str!("../../../docs/data-editors.md")]

// Public paths stay here. Family modules own their retained state and policy;
// only unchanged entity/identity mechanics are shared through this parent.
mod inspector;
mod table;

pub(crate) use inspector::{inspector_blur, styles};
pub use inspector::{
    CancelReason, Choice, GroupId, InspectorHandle, InspectorProps, ObjectId, Property,
    PropertyDraft, PropertyEvent, PropertyEventKind, PropertyGroup, PropertyId, PropertyInspector,
    PropertyKind, PropertySlot, PropertyValidator, PropertyValue, MAX_PROPERTIES,
};
pub(crate) use table::{measure_cells, TableRow, TABLE_INSET};
pub use table::{
    CellAlignment, CellSlot, Column, ColumnId, DataTable, SortDirection, SortRequested, SortState,
    TableHandle, TableProps, MAX_TABLE_COLUMNS, MAX_TABLE_ROWS,
};

use crate::{controls, Typography, UiError, UiId};
use rndr_core::*;

pub(crate) fn key_at(world: &World, e: Entity) -> Option<UiId> {
    table::key_at(world, e).or_else(|| inspector::key_at(world, e))
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
