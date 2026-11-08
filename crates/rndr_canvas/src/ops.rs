//! Edits expressed as data, applied all-or-nothing.

use crate::error::{invalid, Result};
use crate::schema::{Document, Node};
use crate::validate::validate_document;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Operation {
    /// Inserts the node at the front of the paint order, or replaces the
    /// node with the same id in place.
    PutNode {
        node: Box<Node>,
    },
    /// Removes the node and all of its descendants.
    RemoveNode {
        id: String,
    },
    /// Sets the paint order; `ids` lists every node exactly once.
    Reorder {
        ids: Vec<String>,
    },
    SetBackground {
        color: String,
    },
    Rename {
        title: String,
    },
    Pin {
        pinned: bool,
    },
}

impl Operation {
    pub fn put(node: Node) -> Self {
        Operation::PutNode {
            node: Box::new(node),
        }
    }

    pub fn remove(id: impl Into<String>) -> Self {
        Operation::RemoveNode { id: id.into() }
    }
}

/// A labelled batch of operations made against a known revision. Its id
/// makes retries idempotent.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Transaction {
    pub id: String,
    pub expected_revision: u64,
    /// Undo-menu label, for example "Move".
    pub label: String,
    pub operations: Vec<Operation>,
}

/// An undo, or a redo when `redo` is set, against a known revision. Like a
/// transaction, its id makes retries idempotent.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct HistoryRequest {
    pub id: String,
    pub expected_revision: u64,
    pub redo: bool,
}

pub const MAX_OPERATIONS: usize = 1000;

/// Applies `ops` to a copy of `doc` and validates the result. The revision
/// and timestamps are left for the caller to advance.
pub fn apply_operations(doc: &Document, ops: &[Operation]) -> Result<Document> {
    if ops.is_empty() || ops.len() > MAX_OPERATIONS {
        return Err(invalid("transaction must contain 1-1000 operations"));
    }
    let mut next = doc.clone();
    let mut cascaded = BTreeSet::new();
    for op in ops {
        match op {
            Operation::PutNode { node } => {
                if let Some(old) = next.scene.nodes.iter_mut().find(|n| n.id == node.id) {
                    *old = (**node).clone();
                } else {
                    next.scene.nodes.push((**node).clone());
                }
            }
            Operation::RemoveNode { id } => {
                if !next.scene.nodes.iter().any(|n| &n.id == id) {
                    // Explicit removals of descendants already cascaded away
                    // are accepted; a target that never existed is not.
                    if cascaded.contains(id) {
                        continue;
                    }
                    return Err(invalid("removed node does not exist"));
                }
                let removed = crate::geometry::descendants(&next.scene.nodes, [id.as_str()]);
                cascaded.extend(removed.iter().filter(|t| *t != id).cloned());
                next.scene.nodes.retain(|node| !removed.contains(&node.id));
            }
            Operation::Reorder { ids } => {
                let mut by_id: BTreeMap<_, _> = next
                    .scene
                    .nodes
                    .drain(..)
                    .map(|n| (n.id.clone(), n))
                    .collect();
                if ids.len() != by_id.len() {
                    return Err(invalid("reorder must include every node"));
                }
                for id in ids {
                    next.scene.nodes.push(
                        by_id
                            .remove(id)
                            .ok_or_else(|| invalid("reorder contains duplicate or unknown id"))?,
                    );
                }
            }
            Operation::SetBackground { color } => next.scene.background = color.clone(),
            Operation::Rename { title } => next.title = title.clone(),
            Operation::Pin { pinned } => next.pinned = *pinned,
        }
    }
    validate_document(&next)?;
    Ok(next)
}
