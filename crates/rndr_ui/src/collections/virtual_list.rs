//! Uniform-height semantic list, sharing core windowing and Scroll geometry.
use super::{
    CollectionEvent, CollectionEventKind, Item, ItemId, ListView, RowSlot, SelectionMode, ViewProps,
};
use crate::{UiError, UiId};
use rndr_core::*;
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    sync::Arc,
};

/// Row renderer. Called for realized rows during layout; update children in place.
/// Row subtrees are ephemeral: on recycling they are destroyed before rebinding.
/// Keep application/editor state outside rows, keyed by ItemId. Nested controls
/// own focus/activation exclusively; their input never selects/activates the row.
pub type VirtualCompose = fn(&mut World, RowSlot, &Item);
/// Explicit optional command-palette behavior. Default semantic navigation never
/// selects and pointer activation never emits host Activated. Install on the root
/// only when the host intentionally wants these legacy palette conventions.
#[derive(Clone, Copy, Debug, Default)]
pub struct VirtualListPolicy {
    /// Pointer movement over passive row content changes the navigation key.
    pub hover_navigation: bool,
    /// Navigation (keyboard/hover) also selects. Off by default.
    pub selection_follows_navigation: bool,
    /// Two accepted row clicks within 400ms activate the stable key. Nested
    /// control clicks are excluded, as are cancelled drags/releases.
    pub double_click_activate: bool,
    /// External canonical focus owner for passive rows/viewport. Logical row
    /// navigation remains keyed here; embedded controls keep their own focus.
    /// When valid, rows and viewport leave Tab order (the owner participates).
    pub focus_owner: Option<Entity>,
}
impl Component for VirtualListPolicy {}
#[derive(Clone)]
struct VirtualView {
    key: UiId,
    props: ViewProps,
    items: Arc<[Item]>,
    indices: Arc<HashMap<ItemId, usize>>,
    stride: f32,
    viewport_height: f32,
    compose: VirtualCompose,
    rows: BTreeMap<ItemId, RowSlot>,
    selected: Option<ItemId>,
    active: Option<ItemId>,
    reported: Option<ItemId>,
    top: Entity,
    bottom: Entity,
    empty: Entity,
    pointer: Option<[f32; 2]>,
    last_click: Option<(ItemId, u64, u64)>,
}
impl Component for VirtualView {}
#[derive(Clone, Copy)]
struct Owner {
    view: Entity,
    id: ItemId,
}
impl Component for Owner {}

/// Virtual list handle. Logical identity is stable; realized entities are not.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VirtualViewHandle {
    /// Core Scroll viewport; set height/width, but keep gap/padding zero.
    pub root: Entity,
}
impl ListView {
    /// Mount a uniform-height, viewport-bounded semantic list with no 1024-row
    /// limit. Metadata/index reconciliation is O(n); steady layout/row rendering
    /// is bounded by the visible window (not metadata count). Retained variable-height `mount` and TreeView are unchanged.
    /// Invalid input is rejected before mutation. Supports at most u32::MAX rows.
    pub fn mount_virtual(
        w: &mut World,
        parent: Entity,
        key: UiId,
        props: ViewProps,
        items: &[Item],
        row_height: f32,
        compose: VirtualCompose,
    ) -> Result<VirtualViewHandle, UiError> {
        if !w.is_alive(parent) || !w.has::<Layout>(parent) {
            return Err(UiError::Stale);
        }
        if props.name.trim().is_empty()
            || props.empty.trim().is_empty()
            || !row_height.is_finite()
            || row_height <= 0.0
            || items.len() > u32::MAX as usize
            || !(row_height * items.len() as f32).is_finite()
        {
            return Err(UiError::InvalidProps);
        }
        let mut indices = HashMap::with_capacity(items.len());
        for (i, item) in items.iter().enumerate() {
            if item.name.trim().is_empty() || indices.insert(item.id, i).is_some() {
                return Err(UiError::InvalidProps);
            }
        }
        let existing = children_of(w, parent).into_iter().find(|e| {
            super::key_at(w, *e)
                .or_else(|| crate::controls::key_at(w, *e))
                .or_else(|| crate::surfaces::key_at(w, *e))
                == Some(key)
        });
        let root = if let Some(e) = existing {
            if !w.has::<VirtualView>(e) {
                return Err(UiError::IdentityConflict);
            }
            validate(w, e)?;
            e
        } else {
            let e = w.spawn((
                Layout {
                    width: Size::Fill,
                    height: Size::Px(280.),
                    ..Layout::column()
                },
                Scroll::default(),
                Focusable,
                KeyNavigation(navigate),
                FocusTarget(focus_target),
                ViewportRows(realize),
            ));
            set_parent(w, e, parent);
            let top = spacer(w, e);
            let bottom = spacer(w, e);
            let empty = crate::text(w, e, &props.empty, crate::Typography::Secondary)?;
            w.insert(
                e,
                VirtualView {
                    key,
                    props: props.clone(),
                    items: Arc::from([]),
                    indices: Arc::new(HashMap::new()),
                    stride: row_height,
                    viewport_height: 0.,
                    compose,
                    rows: BTreeMap::new(),
                    selected: None,
                    active: None,
                    reported: None,
                    top,
                    bottom,
                    empty,
                    pointer: None,
                    last_click: None,
                },
            );
            e
        };
        let mut v = w.get::<VirtualView>(root).unwrap().clone();
        let focused = logical_focus(w, root, &v);
        let canonical_focus = w.resource::<Focus>().and_then(|f| f.entity);
        let old_index = focused
            .and_then(|id| v.indices.get(&id).copied())
            .unwrap_or(0);
        // Cancel queued/armed requests for removed or newly disabled rows before
        // replacing metadata. Rebinding performs the same retirement recursively.
        for (id, slot) in &v.rows {
            if !indices
                .get(id)
                .is_some_and(|i| items[*i].enabled && props.enabled)
            {
                retire(w, slot.root);
            }
        }
        if !std::ptr::fn_addr_eq(v.compose, compose) {
            for slot in v.rows.values() {
                retire(w, slot.root);
                for child in children_of(w, slot.content) {
                    despawn(w, child);
                }
            }
        }
        v.last_click = None;
        v.props = props;
        v.items = Arc::from(items);
        v.indices = Arc::new(indices);
        v.stride = row_height;
        v.compose = compose;
        if (v.selected.is_some_and(|id| !item_enabled(&v, id))
            || v.props.selection == SelectionMode::None
            || !v.props.enabled)
            && v.selected.take().is_some()
        {
            emit(w, root, CollectionEventKind::Selected(None));
        }
        if !v.active.is_some_and(|id| item_enabled(&v, id)) {
            v.active = v
                .items
                .iter()
                .skip(old_index.min(v.items.len().saturating_sub(1)))
                .chain(v.items.iter())
                .find(|i| i.enabled)
                .map(|i| i.id);
        }
        w.get_mut::<Label>(v.empty).unwrap().text = v.props.empty.clone();
        w.insert(root, v);
        realize(w, root);
        if focused.is_some()
            && !canonical_focus.is_some_and(|f| {
                w.is_alive(f)
                    && access_details(w, f).focus
                    && owner(w, f).is_some_and(|o| {
                        Some(o.id) == focused
                            && item_enabled(w.get::<VirtualView>(root).unwrap(), o.id)
                    })
            })
        {
            if let Some(id) = w.get::<VirtualView>(root).and_then(|v| v.active) {
                if w.get::<VirtualView>(root).unwrap().props.enabled {
                    VirtualViewHandle { root }.focus(w, id)?;
                } else {
                    request_focus(w, root);
                }
            } else {
                request_focus(w, root);
            }
        }
        Ok(VirtualViewHandle { root })
    }
}
fn spacer(w: &mut World, parent: Entity) -> Entity {
    let e = w.spawn((Layout {
        width: Size::Fill,
        height: Size::Px(0.),
        ..Layout::column()
    },));
    set_parent(w, e, parent);
    e
}
pub(super) fn key_at(w: &World, e: Entity) -> Option<UiId> {
    w.get::<VirtualView>(e).map(|v| v.key)
}
pub(super) fn owns(w: &World, e: Entity) -> bool {
    ancestor_with::<VirtualView>(w, e).is_some()
}
fn emit(w: &mut World, view: Entity, kind: CollectionEventKind) {
    send(w, CollectionEvent { view, kind });
}
fn item_enabled(v: &VirtualView, id: ItemId) -> bool {
    v.indices.get(&id).is_some_and(|i| v.items[*i].enabled)
}
fn owner(w: &World, e: Entity) -> Option<Owner> {
    ancestor_with::<Owner>(w, e).and_then(|e| w.get::<Owner>(e).copied())
}
fn logical_focus(w: &World, root: Entity, v: &VirtualView) -> Option<ItemId> {
    let f = w.resource::<Focus>()?.entity?;
    if f == root {
        v.active
    } else {
        owner(w, f).filter(|o| o.view == root).map(|o| o.id)
    }
}
fn subtree(w: &World, e: Entity, out: &mut Vec<Entity>) {
    out.push(e);
    for child in children_of(w, e) {
        subtree(w, child, out);
    }
}
fn retire(w: &mut World, e: Entity) {
    let mut entities = Vec::new();
    subtree(w, e, &mut entities);
    forget_interaction_targets(w, &entities);
}
fn validate(w: &World, root: Entity) -> Result<(), UiError> {
    let v = w.get::<VirtualView>(root).ok_or(UiError::Stale)?;
    if !w.has::<Scroll>(root)
        || !w.has::<Layout>(root)
        || !w.has::<Label>(v.empty)
        || [v.top, v.bottom, v.empty]
            .iter()
            .any(|e| !w.has::<Layout>(*e) || w.get::<Parent>(*e).map(|p| p.entity) != Some(root))
        || v.rows.values().any(|s| {
            !w.has::<Layout>(s.root)
                || !w.has::<Layout>(s.content)
                || w.get::<Parent>(s.root).map(|p| p.entity) != Some(root)
                || w.get::<Parent>(s.content).map(|p| p.entity) != Some(s.root)
        })
    {
        return Err(UiError::Stale);
    }
    Ok(())
}
fn realize(w: &mut World, root: Entity) -> bool {
    if validate(w, root).is_err() {
        return false;
    }
    let Some(mut v) = w.get::<VirtualView>(root).cloned() else {
        return false;
    };
    let h = w.get::<Rect>(root).map(|r| r.h).unwrap_or(0.);
    let max = (v.items.len() as f32 * v.stride - h).max(0.);
    let mut offset = w
        .get::<Scroll>(root)
        .map(|s| -s.offset[1])
        .unwrap_or(0.)
        .clamp(0., max);
    if h != v.viewport_height {
        // A genuine viewport resize may hide a canonical row that still owns
        // focus. Reveal only at this geometry transition; a deliberate scroll
        // parks focus on the viewport and must never cause per-frame snap-back.
        let canonical = w.resource::<Focus>().and_then(|f| f.entity);
        if let Some(id) = canonical
            .and_then(|f| owner(w, f))
            .filter(|o| o.view == root)
            .map(|o| o.id)
        {
            if let Some(index) = v.indices.get(&id) {
                let y = *index as f32 * v.stride;
                offset = if y < offset {
                    y
                } else if y + v.stride > offset + h {
                    (y + v.stride - h).max(0.)
                } else {
                    offset
                }
                .clamp(0., max);
            }
        }
        v.viewport_height = h;
    }
    if let Some(s) = w.get_mut::<Scroll>(root) {
        s.offset[1] = -offset;
    }
    let (first, len) = list_window(offset, h, v.stride, v.items.len() as u32);
    let wanted: Vec<_> = v
        .items
        .iter()
        .skip(first as usize)
        .take(len as usize)
        .map(|i| i.id)
        .collect();
    let wanted_set: HashSet<_> = wanted.iter().copied().collect();
    let old: Vec<_> = v.rows.keys().copied().collect();
    let mut spare = Vec::new();
    let mut changed = w
        .get::<VirtualView>(root)
        .is_some_and(|old| old.viewport_height != h);
    for id in old {
        if !wanted_set.contains(&id) {
            let slot = v.rows.remove(&id).unwrap();
            // Park focus on the viewport, preserving the logical stable ID.
            if w.resource::<Focus>()
                .and_then(|f| f.entity)
                .is_some_and(|f| ancestor_with::<Owner>(w, f) == Some(slot.root))
            {
                if item_enabled(&v, id) {
                    v.active = Some(id);
                }
                request_focus(w, root);
            }
            retire(w, slot.root);
            for child in children_of(w, slot.content) {
                despawn(w, child);
            }
            spare.push(slot);
            changed = true;
        }
    }
    for id in &wanted {
        if !v.rows.contains_key(id) {
            let slot = spare.pop().unwrap_or_else(|| {
                let row = w.spawn((Layout::column(), Focusable, ReleaseActivation));
                set_parent(w, row, root);
                let content = spacer(w, row);
                w.get_mut::<Layout>(content).unwrap().height = Size::Fill;
                RowSlot { root: row, content }
            });
            w.insert(
                slot.root,
                Owner {
                    view: root,
                    id: *id,
                },
            );
            v.rows.insert(*id, slot);
            changed = true;
        }
    }
    for slot in spare {
        despawn(w, slot.root);
    }
    let top_h = first as f32 * v.stride;
    let bottom_h = (v.items.len() as u32 - first - len) as f32 * v.stride;
    for (e, height) in [(v.top, top_h), (v.bottom, bottom_h)] {
        let l = w.get_mut::<Layout>(e).unwrap();
        changed |= l.height != Size::Px(height);
        l.height = Size::Px(height);
    }
    w.insert(
        root,
        Children {
            list: std::iter::once(v.top)
                .chain(wanted.iter().map(|id| v.rows[id].root))
                .chain([v.bottom, v.empty])
                .collect(),
        },
    );
    if v.items.is_empty() {
        w.remove::<Hidden>(v.empty);
    } else {
        w.insert(v.empty, Hidden);
    }
    w.insert(
        root,
        TabStop(
            (v.rows.is_empty() || !v.active.is_some_and(|id| v.rows.contains_key(&id)))
                && !external_focus_owner(w, root),
        ),
    );
    w.insert(root, Semantics { name: Some(v.props.name.clone()), description: "Arrows move focus. Space/click selects. Enter activates. Embedded controls own their input.".into(), readonly: false });
    w.insert(
        root,
        CollectionAccess {
            role: AccessRole::List,
            expanded: None,
            level: None,
            parent: None,
        },
    );
    if v.props.enabled {
        w.remove::<ControlDisabled>(root);
    } else {
        w.insert(root, ControlDisabled);
    }
    // Store before callbacks: renderers and nested component mounting can query
    // the canonical selection and key ownership without borrowing model state.
    w.insert(root, v.clone());
    for id in wanted {
        let slot = v.rows[&id];
        let item = &v.items[v.indices[&id]];
        w.insert(
            slot.root,
            Layout {
                width: Size::Fill,
                height: Size::Px(v.stride),
                ..Layout::column()
            },
        );
        w.insert(
            slot.root,
            Button {
                action: "rndr-ui:select".into(),
                enabled: item.enabled && v.props.enabled,
            },
        );
        w.insert(slot.root, KeyNavigation(navigate));
        w.insert(slot.root, FocusTarget(focus_target));
        w.insert(slot.root, ActivationHandler(activate));
        w.insert(
            slot.root,
            TabStop(v.active == Some(id) && !external_focus_owner(w, root)),
        );
        w.insert(slot.root, SelectedState(v.selected == Some(id)));
        let focused = w
            .resource::<Focus>()
            .and_then(|f| f.entity)
            .is_some_and(|f| owner(w, f).is_some_and(|o| o.view == root && o.id == id));
        w.insert(
            slot.root,
            crate::UiRecipe(crate::UiStyle {
                background: Some(
                    if focused {
                        crate::Token::Focus
                    } else if v.selected == Some(id) {
                        crate::Token::Selected
                    } else {
                        crate::Token::Content
                    }
                    .into(),
                ),
                corner: Some(crate::Corner::Compact),
                ..Default::default()
            }),
        );
        w.insert(
            slot.root,
            Semantics {
                name: Some(item.name.clone()),
                ..Semantics::default()
            },
        );
        w.insert(
            slot.root,
            CollectionAccess {
                role: AccessRole::ListItem,
                expanded: None,
                level: None,
                parent: None,
            },
        );
        // Disabled ancestry is inherited by embedded controls too.
        if item.enabled {
            w.remove::<ControlDisabled>(slot.root);
        } else {
            w.insert(slot.root, ControlDisabled);
        }
        (v.compose)(w, slot, item);
    }
    crate::style::resolve_subtree_styles(w, root);
    changed
}
fn focus_target(w: &mut World, entity: Entity) -> Entity {
    let row = w.get::<Owner>(entity).copied();
    let root = row.map(|o| o.view).unwrap_or(entity);
    let external = w
        .get::<VirtualListPolicy>(root)
        .and_then(|p| p.focus_owner)
        .filter(|owner| access_details(w, *owner).focus);
    if let Some(external) = external {
        if let Some(row) = row {
            if let Some(v) = w.get_mut::<VirtualView>(root) {
                v.active = Some(row.id);
            }
        }
        external
    } else {
        entity
    }
}
fn external_focus_owner(w: &World, root: Entity) -> bool {
    w.get::<VirtualListPolicy>(root)
        .and_then(|p| p.focus_owner)
        .is_some_and(|e| access_details(w, e).focus)
}
fn activate(w: &mut World, e: Entity) {
    if let Some(o) = w.get::<Owner>(e).copied() {
        let _ = (VirtualViewHandle { root: o.view }).select(w, Some(o.id));
        if w.get::<VirtualListPolicy>(o.view)
            .is_some_and(|p| p.double_click_activate)
            && w.resource::<Input>().is_some_and(|i| i.pointer.released)
        {
            let (frame, ms) = w
                .resource::<Time>()
                .map(|t| (t.frame, t.now.as_millis() as u64))
                .unwrap_or((0, 0));
            let v = w.get_mut::<VirtualView>(o.view).unwrap();
            let double = v.last_click.is_some_and(|(id, old_frame, old_ms)| {
                id == o.id && frame > old_frame && ms.saturating_sub(old_ms) <= 400
            });
            v.last_click = if double {
                None
            } else {
                Some((o.id, frame, ms))
            };
            if double {
                emit(w, o.view, CollectionEventKind::Activated(o.id));
            }
        }
    }
}
fn navigate(w: &mut World, e: Entity, key: &KeyEvent) -> bool {
    navigate_impl(w, e, key, false)
}
fn navigate_impl(w: &mut World, e: Entity, key: &KeyEvent, external: bool) -> bool {
    let root = w.get::<Owner>(e).map(|o| o.view).unwrap_or(e);
    let Some(v) = w.get::<VirtualView>(root).cloned() else {
        return false;
    };
    // This hook is installed ONLY on the viewport/row, never on embedded controls.
    let LogicalKey::Named(k) = key.logical else {
        return false;
    };
    if !matches!(
        k,
        NamedKey::ArrowUp
            | NamedKey::ArrowDown
            | NamedKey::Home
            | NamedKey::End
            | NamedKey::PageUp
            | NamedKey::PageDown
            | NamedKey::Enter
    ) || key.modifiers.command()
        || key.modifiers.alt
        || key.modifiers.shift
    {
        return false;
    }
    if events::<WindowFocus>(w).contains(&WindowFocus::Blurred) {
        return true;
    }
    if key.released
        || key.synthetic
        || !v.props.enabled
        || w.resource::<Input>()
            .is_some_and(|i| i.ime_enabled || !i.ime_preedit.is_empty() || !i.ime_commit.is_empty())
    {
        return true;
    }
    let id = if external
        || w.get::<VirtualListPolicy>(root)
            .is_some_and(|p| p.hover_navigation)
    {
        v.active.or_else(|| logical_focus(w, root, &v))
    } else {
        logical_focus(w, root, &v).or(v.active)
    };
    if k == NamedKey::Enter {
        if !key.repeat {
            if let Some(id) = id.filter(|id| item_enabled(&v, *id)) {
                emit(w, root, CollectionEventKind::Activated(id));
            }
        }
        return true;
    }
    if v.items.is_empty() {
        return true;
    }
    let at = id.and_then(|id| v.indices.get(&id).copied()).unwrap_or(0);
    let page = (w.get::<Rect>(root).map(|r| r.h).unwrap_or(v.stride) / v.stride)
        .floor()
        .max(1.) as usize;
    let target = match k {
        NamedKey::Home => v.items.iter().find(|i| i.enabled),
        NamedKey::End => v.items.iter().rev().find(|i| i.enabled),
        NamedKey::ArrowUp | NamedKey::PageUp => v.items[..at
            .saturating_sub(if k == NamedKey::PageUp { page } else { 1 })
            .saturating_add(1)]
            .iter()
            .rev()
            .find(|i| i.enabled),
        _ => v
            .items
            .iter()
            .skip(
                (at + if k == NamedKey::PageDown { page } else { 1 })
                    .min(v.items.len().saturating_sub(1)),
            )
            .find(|i| i.enabled),
    }
    .map(|i| i.id);
    if let Some(id) = target {
        if external {
            let _ = (VirtualViewHandle { root }).reveal(w, id);
            w.get_mut::<VirtualView>(root).unwrap().active = Some(id);
        } else {
            let _ = (VirtualViewHandle { root }).focus(w, id);
        }
        if w.get::<VirtualListPolicy>(root)
            .is_some_and(|p| p.selection_follows_navigation)
        {
            let _ = (VirtualViewHandle { root }).select(w, Some(id));
        }
    }
    true
}
/// Report canonical focus changes once per Input, not every layout refresh.
pub(super) fn requests(w: &mut World) {
    for root in w.entities_with::<VirtualView>() {
        let mut v = w.get::<VirtualView>(root).unwrap().clone();
        if w.get::<VirtualListPolicy>(root)
            .is_some_and(|p| p.hover_navigation)
        {
            w.insert_resource(MotionWatch { active: true });
        }
        let pointer = w.resource::<Input>().map(|i| i.pointer.position);
        if let Some(position) = pointer {
            let moved = v.pointer.is_some_and(|previous| previous != position);
            v.pointer = Some(position);
            if moved
                && v.props.enabled
                && w.get::<VirtualListPolicy>(root)
                    .is_some_and(|p| p.hover_navigation)
            {
                if let Some(hit) = hit_test(w, position[0], position[1]) {
                    if let Some(o) =
                        owner(w, hit).filter(|o| o.view == root && item_enabled(&v, o.id))
                    {
                        let row = v.rows[&o.id].root;
                        // The nearest control owns its input. Passive label hits
                        // belong to the row; nested Buttons/Checkboxes never do.
                        if ancestor_with::<Button>(w, hit) == Some(row) {
                            v.active = Some(o.id);
                            if w.get::<VirtualListPolicy>(root)
                                .is_some_and(|p| p.selection_follows_navigation)
                                && v.selected != Some(o.id)
                            {
                                v.selected = Some(o.id);
                                emit(w, root, CollectionEventKind::Selected(Some(o.id)));
                            }
                        }
                    }
                }
            }
        }
        let f = logical_focus(w, root, &v);
        if f != v.reported {
            if let Some(id) = f {
                v.active = Some(id);
            }
        }
        if f != v.reported {
            emit(w, root, CollectionEventKind::Focused(f));
            v.reported = f;
        }
        w.insert(root, v);
    }
}
impl VirtualViewHandle {
    /// Stable row identity for a currently mounted embedded control/event target.
    /// Returns None for stale entities or entities owned by another list.
    pub fn item_for_entity(self, w: &World, entity: Entity) -> Option<ItemId> {
        owner(w, entity)
            .filter(|o| o.view == self.root)
            .map(|o| o.id)
    }
    /// Current selected stable key, independent of focus and realization.
    pub fn selected(self, w: &World) -> Option<ItemId> {
        w.get::<VirtualView>(self.root).and_then(|v| v.selected)
    }
    /// Canonical/logical focus, including a row parked on its viewport by scroll.
    pub fn focused(self, w: &World) -> Option<ItemId> {
        w.get::<VirtualView>(self.root)
            .and_then(|v| logical_focus(w, self.root, v))
    }
    /// A currently realized slot, NOT a persistent entity identity.
    pub fn row(self, w: &World, id: ItemId) -> Option<RowSlot> {
        w.get::<VirtualView>(self.root)?.rows.get(&id).copied()
    }
    /// Metadata row count (not realized entity count).
    pub fn len(self, w: &World) -> usize {
        w.get::<VirtualView>(self.root)
            .map(|v| v.items.len())
            .unwrap_or(0)
    }
    /// True when no logical records are present.
    pub fn is_empty(self, w: &World) -> bool {
        self.len(w) == 0
    }
    /// Select without focus/reveal/activation. None clears selection.
    pub fn select(self, w: &mut World, id: Option<ItemId>) -> Result<(), UiError> {
        validate(w, self.root)?;
        let v = w.get_mut::<VirtualView>(self.root).ok_or(UiError::Stale)?;
        if !v.props.enabled
            || id.is_some_and(|id| v.props.selection == SelectionMode::None || !item_enabled(v, id))
        {
            return Err(UiError::InvalidProps);
        }
        if v.selected != id {
            v.selected = id;
            emit(w, self.root, CollectionEventKind::Selected(id));
        }
        Ok(())
    }
    /// Reveal an enabled logical row, realizing it synchronously. No focus change.
    pub fn reveal(self, w: &mut World, id: ItemId) -> Result<(), UiError> {
        validate(w, self.root)?;
        let v = w.get::<VirtualView>(self.root).ok_or(UiError::Stale)?;
        if !v.props.enabled || !item_enabled(v, id) {
            return Err(UiError::InvalidProps);
        }
        let y = v.indices[&id] as f32 * v.stride;
        let h = w.get::<Rect>(self.root).map(|r| r.h).unwrap_or(0.);
        let stride = v.stride;
        let s = w.get_mut::<Scroll>(self.root).ok_or(UiError::Stale)?;
        let offset = -s.offset[1];
        s.offset[1] = -if y < offset {
            y
        } else if y + stride > offset + h {
            (y + stride - h).max(0.)
        } else {
            offset
        };
        layout_propagate(w);
        Ok(())
    }
    /// Move focus and reveal without selecting or activating. If a row's nested
    /// control already owns focus, explicitly calling this transfers it to row.
    pub fn focus(self, w: &mut World, id: ItemId) -> Result<(), UiError> {
        self.reveal(w, id)?;
        let row = w
            .get::<VirtualView>(self.root)
            .and_then(|v| v.rows.get(&id))
            .map(|s| s.root)
            .unwrap_or(self.root);
        w.get_mut::<VirtualView>(self.root).unwrap().active = Some(id);
        request_focus(w, row);
        // Canonical request_focus reveals using logical rects; refresh them first
        // on the next layout. Our explicit uniform reveal already set the offset.
        Ok(())
    }
    /// Destroy this list and all ephemeral row content, retiring queued input.
    pub fn unmount(self, w: &mut World) -> Result<(), UiError> {
        if !w.has::<VirtualView>(self.root) {
            return Err(UiError::Stale);
        }
        retire(w, self.root);
        despawn(w, self.root);
        Ok(())
    }
    /// Forward a key from an external search field without transferring focus.
    /// Intended for command palettes. Navigation changes logical focus/reveal;
    /// Enter activates that key. No selection-follows-focus is implied.
    pub fn forward_key(self, w: &mut World, key: &KeyEvent) -> bool {
        navigate_impl(w, self.root, key, true)
    }
    /// Current logical navigation key, even while focus is in an external field.
    pub fn active(self, w: &World) -> Option<ItemId> {
        w.get::<VirtualView>(self.root).and_then(|v| v.active)
    }
}
