//! Shared Multiple and Range policy for list and table rows.
//!
//! `None` and `Single` keep a separate optional id. This module does not emit
//! events; the caller does, and a set change never carries the ids.

use super::{ItemId, SelectionMode};
use std::collections::BTreeSet;

/// Item ids selected in `Multiple` or `Range`, plus the range anchor.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Membership {
    pub ids: BTreeSet<ItemId>,
    /// Range origin. Plain range gestures move it. Shift extensions do not.
    /// `None` means the next Shift gesture has no origin yet.
    pub anchor: Option<ItemId>,
}

/// What the caller should publish. `Set` has no id payload.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum MembershipChange {
    /// No observable selection change.
    Quiet,
    /// `Single` or `None` changed its one id.
    Single(Option<ItemId>),
    /// `Multiple` or `Range` membership changed. Read the set from the handle.
    Set,
}

/// Selection gesture for one enabled row. Disabled ids and unknown ids do nothing.
///
/// `shift` extends a `Range` from the anchor through `id` in `order`. Disabled
/// ids inside that span are skipped. A Shift gesture with no anchor selects
/// `id` and sets the anchor; that is the empty-anchor repair, not an extension.
pub(super) fn gesture(
    mode: SelectionMode,
    single: &mut Option<ItemId>,
    membership: &mut Membership,
    id: ItemId,
    shift: bool,
    order: &[ItemId],
    mut enabled: impl FnMut(ItemId) -> bool,
) -> MembershipChange {
    if !order.contains(&id) || !enabled(id) {
        return MembershipChange::Quiet;
    }
    match mode {
        SelectionMode::None => MembershipChange::Quiet,
        SelectionMode::Single => {
            if *single == Some(id) {
                MembershipChange::Quiet
            } else {
                *single = Some(id);
                MembershipChange::Single(Some(id))
            }
        }
        SelectionMode::Multiple => {
            if !membership.ids.remove(&id) {
                membership.ids.insert(id);
            }
            MembershipChange::Set
        }
        SelectionMode::Range if !shift || membership.anchor.is_none() => {
            let unchanged = membership.ids.len() == 1
                && membership.ids.contains(&id)
                && membership.anchor == Some(id);
            membership.ids.clear();
            membership.ids.insert(id);
            membership.anchor = Some(id);
            if unchanged {
                MembershipChange::Quiet
            } else {
                MembershipChange::Set
            }
        }
        SelectionMode::Range => {
            let Some(anchor) = membership.anchor else {
                return MembershipChange::Quiet;
            };
            let Some(set) = span(order, anchor, id, &mut enabled) else {
                return MembershipChange::Quiet;
            };
            if set == membership.ids {
                MembershipChange::Quiet
            } else {
                membership.ids = set;
                MembershipChange::Set
            }
        }
    }
}

/// Drop ids that are gone or disabled. Repair a missing or disabled range anchor
/// to the next enabled survivor in the previous order, then the previous, then none.
/// The set is not rebuilt from the repaired anchor.
pub(super) fn reconcile(
    mode: SelectionMode,
    membership: &mut Membership,
    old_order: &[ItemId],
    new_order: &[ItemId],
    mut enabled: impl FnMut(ItemId) -> bool,
) -> MembershipChange {
    if !matches!(mode, SelectionMode::Multiple | SelectionMode::Range) {
        membership.ids.clear();
        membership.anchor = None;
        return MembershipChange::Quiet;
    }
    let before = membership.ids.clone();
    membership
        .ids
        .retain(|id| new_order.contains(id) && enabled(*id));
    if mode == SelectionMode::Range {
        membership.anchor = repair_anchor(old_order, new_order, membership.anchor, &mut enabled);
    } else {
        membership.anchor = None;
    }
    if membership.ids == before {
        MembershipChange::Quiet
    } else {
        MembershipChange::Set
    }
}

fn span(
    order: &[ItemId],
    anchor: ItemId,
    end: ItemId,
    enabled: &mut impl FnMut(ItemId) -> bool,
) -> Option<BTreeSet<ItemId>> {
    let start = order.iter().position(|id| *id == anchor)?;
    let finish = order.iter().position(|id| *id == end)?;
    let (low, high) = if start <= finish {
        (start, finish)
    } else {
        (finish, start)
    };
    Some(
        order[low..=high]
            .iter()
            .copied()
            .filter(|id| enabled(*id))
            .collect(),
    )
}

fn repair_anchor(
    old_order: &[ItemId],
    new_order: &[ItemId],
    anchor: Option<ItemId>,
    enabled: &mut impl FnMut(ItemId) -> bool,
) -> Option<ItemId> {
    let anchor = anchor?;
    if new_order.contains(&anchor) && enabled(anchor) {
        return Some(anchor);
    }
    let index = old_order.iter().position(|id| *id == anchor)?;
    if let Some(id) = old_order[index + 1..]
        .iter()
        .copied()
        .find(|&id| new_order.contains(&id) && enabled(id))
    {
        return Some(id);
    }
    old_order[..index]
        .iter()
        .copied()
        .rev()
        .find(|&id| new_order.contains(&id) && enabled(id))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(values: &[u64]) -> Vec<ItemId> {
        values.iter().copied().map(ItemId).collect()
    }

    #[test]
    fn range_skips_disabled_and_shift_keeps_the_anchor() {
        let order = ids(&[1, 2, 3, 4]);
        let mut single = None;
        let mut membership = Membership::default();
        let enabled = |id: ItemId| id != ItemId(3);
        assert_eq!(
            gesture(
                SelectionMode::Range,
                &mut single,
                &mut membership,
                ItemId(1),
                false,
                &order,
                enabled,
            ),
            MembershipChange::Set
        );
        assert_eq!(membership.anchor, Some(ItemId(1)));
        assert_eq!(
            gesture(
                SelectionMode::Range,
                &mut single,
                &mut membership,
                ItemId(4),
                true,
                &order,
                enabled,
            ),
            MembershipChange::Set
        );
        assert_eq!(
            membership.ids.iter().copied().collect::<Vec<_>>(),
            vec![ItemId(1), ItemId(2), ItemId(4)]
        );
        assert_eq!(membership.anchor, Some(ItemId(1)));
    }

    #[test]
    fn removed_anchor_repairs_to_the_next_enabled_then_previous() {
        let old = ids(&[1, 2, 3, 4]);
        let mut membership = Membership {
            ids: [ItemId(2), ItemId(4)].into_iter().collect(),
            anchor: Some(ItemId(2)),
        };
        let new = ids(&[1, 3, 4]);
        assert_eq!(
            reconcile(SelectionMode::Range, &mut membership, &old, &new, |id| id
                != ItemId(3),),
            MembershipChange::Set
        );
        assert_eq!(membership.anchor, Some(ItemId(4)));
        assert!(!membership.ids.contains(&ItemId(2)));
        membership.anchor = Some(ItemId(4));
        membership.ids.insert(ItemId(4));
        let tail = ids(&[1]);
        assert_eq!(
            reconcile(SelectionMode::Range, &mut membership, &new, &tail, |_| {
                true
            }),
            MembershipChange::Set
        );
        assert_eq!(membership.anchor, Some(ItemId(1)));
    }
}
