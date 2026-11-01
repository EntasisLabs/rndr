//! Additive policies consumed by the existing generic interaction pass.
use crate::*;

/// Native tab-list orientation. Unannotated legacy groups keep their behavior.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabOrientation {
    /// Left/Right move focus; Up/Down remain unconsumed.
    Horizontal,
    /// Up/Down move focus; Left/Right remain unconsumed.
    Vertical,
}
/// Opt-in manual tab behavior and one roving Tab stop per group.
/// Entry uses the active enabled tab, or first available. While focus is in the
/// group, its focused tab is the stop, so Tab/Shift-Tab leave the group.
/// Arrows wrap; Home/End choose endpoints; Enter/Space alone activate.
#[derive(Clone, Copy, Debug)]
pub struct ManualTabs(pub TabOrientation);
impl Component for ManualTabs {}

/// Supplemental selected state for a command/navigation row, independent of focus.
/// Does not toggle on activation: selection remains host-owned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SelectedState(pub bool);
impl Component for SelectedState {}

/// Explicit disabled state for non-Button controls (not inherited by children).
/// Disabled targets advertise neither Focus nor Activate; readonly still focuses.
#[derive(Clone, Copy, Debug, Default)]
pub struct ControlDisabled;
impl Component for ControlDisabled {}

/// Preserve fully resolved semantic colors instead of legacy focus brightening.
/// Add to each styled primitive that owns its focus treatment. Legacy colors
/// without this marker retain the renderer's historic brightening policy.
#[derive(Clone, Copy, Debug, Default)]
pub struct ResolvedColor;
impl Component for ResolvedColor {}

pub(crate) fn roving_stop(world: &World, tab: Entity) -> bool {
    let Some(r) = crate::interaction::valid_tab_relation(world, tab) else {
        return true;
    };
    if !world.has::<ManualTabs>(r.group) {
        return true;
    }
    let tabs: Vec<_> = children_of(world, r.group)
        .into_iter()
        .filter(|e| {
            crate::interaction::valid_tab_relation(world, *e).is_some_and(|v| v.group == r.group)
                && access_details(world, *e).focus
        })
        .collect();
    let focused = world
        .resource::<Focus>()
        .and_then(|f| f.entity)
        .filter(|e| tabs.contains(e));
    let active = world.get::<Tabs>(r.group).and_then(|g| {
        tabs.iter()
            .copied()
            .find(|e| world.get::<Tab>(*e).is_some_and(|t| t.id == g.selected))
    });
    focused.or(active).or_else(|| tabs.first().copied()) == Some(tab)
}

pub(crate) fn manual_tab_key(world: &mut World, focus: Option<Entity>, event: &KeyEvent) -> bool {
    let Some(r) = focus.and_then(|f| crate::interaction::valid_tab_relation(world, f)) else {
        return false;
    };
    let Some(policy) = world.get::<ManualTabs>(r.group).copied() else {
        return false;
    };
    if event.released || event.synthetic || event.modifiers.shift {
        return false;
    }
    let movement = match (policy.0, &event.logical) {
        (TabOrientation::Horizontal, LogicalKey::Named(NamedKey::ArrowRight))
        | (TabOrientation::Vertical, LogicalKey::Named(NamedKey::ArrowDown)) => TabFocusMove::Next,
        (TabOrientation::Horizontal, LogicalKey::Named(NamedKey::ArrowLeft))
        | (TabOrientation::Vertical, LogicalKey::Named(NamedKey::ArrowUp)) => {
            TabFocusMove::Previous
        }
        (_, LogicalKey::Named(NamedKey::Home)) => TabFocusMove::First,
        (_, LogicalKey::Named(NamedKey::End)) => TabFocusMove::Last,
        _ => return false,
    };
    move_tab_focus(world, r.group, movement);
    true
}
