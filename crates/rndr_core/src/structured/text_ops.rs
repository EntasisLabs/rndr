//! Text mutation algorithms. The parent dispatch owns normalization after each operation;
//! these helpers retain the original validation and identity-allocation order.
use super::*;

pub(super) fn set_range_marks(
    state: &mut State,
    range: TextSelection,
    marks: Marks,
) -> Result<(), StructuredError> {
    valid_marks(&marks)?;
    let (first, last) = ordered(state, range)?;
    for (block_index, block) in state
        .blocks
        .iter_mut()
        .enumerate()
        .take(last.0 + 1)
        .skip(first.0)
    {
        let start = if block_index == first.0 { first.1 } else { 0 };
        let end = if block_index == last.0 {
            last.1
        } else {
            block.len()
        };
        let mut runs = slice(block, 0, start);
        let mut middle = slice(block, start, end);
        for run in &mut middle {
            run.marks = marks.clone();
        }
        runs.extend(middle);
        runs.extend(slice(block, end, block.len()));
        block.runs = runs;
    }
    Ok(())
}

#[cfg(test)]
#[test]
fn replacement_mapping_matches_original_endpoint_rules() {
    for start in 0..=8 {
        for end in start..=8 {
            for inserted_len in 0..=5 {
                let edit = ReplacementMap {
                    start,
                    end,
                    inserted_len,
                };
                for byte in 0..=12 {
                    for affinity in [Affinity::Before, Affinity::After] {
                        // Original operation's inline mapping, kept independent.
                        let expected = if byte < start {
                            byte
                        } else if byte > end {
                            byte - (end - start) + inserted_len
                        } else if byte == end && end > start {
                            start + inserted_len
                        } else {
                            start
                                + if affinity == Affinity::After {
                                    inserted_len
                                } else {
                                    0
                                }
                        };
                        assert_eq!(edit.map(byte, affinity), expected);
                    }
                }
            }
        }
    }
}

/// Absolute byte mapping for precisely this replacement, not inferred from text equality.
struct ReplacementMap {
    start: usize,
    end: usize,
    inserted_len: usize,
}
impl ReplacementMap {
    fn map(&self, byte: usize, affinity: Affinity) -> usize {
        if byte < self.start {
            byte
        } else if byte > self.end {
            byte - (self.end - self.start) + self.inserted_len
        } else if byte == self.end && self.end > self.start {
            self.start + self.inserted_len
        } else {
            self.start
                + if affinity == Affinity::After {
                    self.inserted_len
                } else {
                    0
                }
        }
    }
}

pub(super) fn replace_text_range(
    state: &mut State,
    next: &mut u64,
    range: TextSelection,
    text: String,
) -> Result<(), StructuredError> {
    let text = normalize_plain_input(&text);
    let (first, last) = ordered(state, range)?;
    let start = offset(
        state,
        Position {
            block: state.blocks[first.0].id,
            offset: first.1,
            affinity: Affinity::Before,
        },
    )?;
    let end = offset(
        state,
        Position {
            block: state.blocks[last.0].id,
            offset: last.1,
            affinity: Affinity::After,
        },
    )?;
    let original_selection = state.selection;
    let mapped_selection = match original_selection {
        StructuredSelection::Text(selection) => Some([
            (offset(state, selection.anchor)?, selection.anchor.affinity),
            (offset(state, selection.head)?, selection.head.affinity),
        ]),
        _ => None,
    };
    let new_blocks = build_replacement_blocks(state, next, first, last, &text)?;
    let selected_node_replaced = match original_selection {
        StructuredSelection::Node(id) => state.blocks[first.0..=last.0]
            .iter()
            .any(|block| block.id == id),
        _ => false,
    };
    state.blocks.splice(first.0..=last.0, new_blocks);
    restore_replacement_selection(
        state,
        original_selection,
        mapped_selection,
        selected_node_replaced,
        range,
        &ReplacementMap {
            start,
            end,
            inserted_len: text.len(),
        },
    )
}

// The first leaf retains its identity/kind/attributes. Later leaves allocate in
// source order; task-item containers allocate before their corresponding leaves.
fn build_replacement_blocks(
    state: &mut State,
    next: &mut u64,
    first: (usize, usize),
    last: (usize, usize),
    text: &str,
) -> Result<Vec<Block>, StructuredError> {
    let marks = insertion_marks(&state.blocks[first.0], first.1);
    let prefix = slice(&state.blocks[first.0], 0, first.1);
    let suffix = slice(&state.blocks[last.0], last.1, state.blocks[last.0].len());
    let parts: Vec<_> = text.split('\n').collect();
    let mut new_blocks = vec![];
    let original_parent = state.blocks[first.0].parent;
    let item = original_parent.filter(|id| {
        matches!(
            container(state, *id).map(|container| container.kind),
            Ok(ContainerKind::Item { .. })
        )
    });
    let mut last_parent = original_parent;
    for (part_index, part) in parts.iter().enumerate() {
        let mut runs = if part_index == 0 {
            prefix.clone()
        } else {
            vec![]
        };
        if !part.is_empty() {
            runs.push(Run {
                text: (*part).into(),
                marks: marks.clone(),
            });
        }
        if part_index + 1 == parts.len() {
            runs.extend(suffix.clone());
        }
        let parent = if part_index > 0 {
            if let Some(item) = item {
                let container = container(state, item)?.clone();
                let checked = match container.kind {
                    ContainerKind::Item { checked } => checked.map(|_| false),
                    _ => None,
                };
                Some(fresh_container(
                    state,
                    next,
                    container.parent,
                    ContainerKind::Item { checked },
                ))
            } else {
                original_parent
            }
        } else {
            original_parent
        };
        last_parent = parent;
        new_blocks.push(Block {
            id: if part_index == 0 {
                state.blocks[first.0].id
            } else {
                fresh(next)
            },
            kind: if part_index == 0 {
                state.blocks[first.0].kind
            } else {
                BlockKind::Paragraph
            },
            parent,
            attributes: if part_index == 0 {
                state.blocks[first.0].attributes.clone()
            } else {
                Attributes::new()
            },
            runs,
        });
    }
    if parts.len() > 1 && item.is_some() {
        // Descendant lists follow the rightmost split item, not a new sibling.
        for container in &mut state.containers {
            if container.parent == item {
                container.parent = last_parent;
            }
        }
    }
    Ok(new_blocks)
}

fn restore_replacement_selection(
    state: &mut State,
    original: StructuredSelection,
    mapped: Option<[(usize, Affinity); 2]>,
    selected_node_replaced: bool,
    active_range: TextSelection,
    edit: &ReplacementMap,
) -> Result<(), StructuredError> {
    if let Some([anchor, head]) = mapped {
        state.selection = StructuredSelection::Text(TextSelection {
            anchor: position_at(state, edit.map(anchor.0, anchor.1), anchor.1)?,
            head: position_at(state, edit.map(head.0, head.1), head.1)?,
        });
    } else if let StructuredSelection::Node(id) = original {
        if selected_node_replaced || block_index(state, id).is_err() {
            let position = position_at(state, edit.start + edit.inserted_len, Affinity::After)?;
            state.selection = StructuredSelection::Text(TextSelection {
                anchor: position,
                head: position,
            });
        }
    }
    // Replacing the active directed range collapses at the inserted text's end.
    if original == StructuredSelection::Text(active_range) {
        let position = position_at(state, edit.start + edit.inserted_len, Affinity::After)?;
        state.selection = StructuredSelection::Text(TextSelection {
            anchor: position,
            head: position,
        });
    }
    Ok(())
}
