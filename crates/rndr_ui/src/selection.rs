//! Host-owned two-state checkbox and stable-ID popup selectors.
//! See docs/selection-controls.md for acknowledgement, dismissal and limits.
use crate::{controls, text, Corner, Token, Typography, UiError, UiId, UiStyle};
use rndr_core::*;
use std::collections::{BTreeMap, BTreeSet};

/// Maximum retained options per selector (not virtualization).
pub const MAX_OPTIONS: usize = 256;
/// Stable application identity, never a presentation index.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OptionId(pub u64);
/// A visible option. Disabled options remain exposed but cannot focus/toggle.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectOption {
    /// Stable identity scoped to one control.
    pub id: OptionId,
    /// Persistent visible/accessible label.
    pub label: String,
    /// Whether the option can be requested by user input.
    pub enabled: bool,
}
impl SelectOption {
    /// Enabled option.
    pub fn new(id: u64, label: impl Into<String>) -> Self {
        Self {
            id: OptionId(id),
            label: label.into(),
            enabled: true,
        }
    }
}
/// Shared persistent label and access policy.
#[derive(Clone, Debug)]
pub struct SelectionProps {
    /// Nonblank visible and accessible name.
    pub label: String,
    /// Supplementary help or error description (not a live announcement).
    pub description: String,
    /// Disabled controls cannot focus or act.
    pub enabled: bool,
    /// Readonly controls can focus but cannot open/toggle.
    pub readonly: bool,
    /// Empty/no-results content, must not be blank.
    pub empty: String,
}
impl SelectionProps {
    /// Enabled mutable control.
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            description: String::new(),
            enabled: true,
            readonly: false,
            empty: "No options available".into(),
        }
    }
}
/// Complete desired value in an explicit change request, never a record mutation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SelectionValue {
    /// Two-state checkbox request.
    Checked(bool),
    /// Zero/one for SingleSelect, zero/many for MultiSelect. Stable ID order.
    Selected(Vec<OptionId>),
}
/// Accepted canonical user activation, delivered on Update.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectionChange {
    /// Live generation-checked control root; host validates before accepting.
    pub control: Entity,
    /// Complete desired state, not a delta tied to a row index.
    pub value: SelectionValue,
}
impl Event for SelectionChange {}
/// Optional synchronous draft adapter at the canonical activation boundary.
/// Does not replace input dispatch. Hosts should use this OR event consumption,
/// never apply the same change twice. Inspector installs it on managed editors.
#[derive(Clone, Copy)]
pub struct ChangeHandler(pub fn(&mut World, Entity, &SelectionValue));
impl Component for ChangeHandler {}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Check,
    Single,
    Multi,
}
#[derive(Clone)]
struct Selection {
    key: UiId,
    kind: Kind,
    props: SelectionProps,
    value: SelectionValue,
    requested: SelectionValue,
    label: Entity,
    box_face: Option<Entity>,
    mark: Option<Entity>,
    popup: Option<Entity>,
    rows: BTreeMap<OptionId, Entity>,
    options: Vec<SelectOption>,
    empty: Option<Entity>,
}
impl Component for Selection {}
#[derive(Clone, Copy)]
struct OptionOwner {
    control: Entity,
    id: OptionId,
}
impl Component for OptionOwner {}
/// Stable root. Internal slots are library-owned; update only through mount.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SelectionHandle {
    /// Canonical interactive trigger/checkbox.
    pub root: Entity,
}
impl SelectionHandle {
    /// Retained popup root (Hidden while closed), absent for Checkbox.
    pub fn popup(self, world: &World) -> Option<Entity> {
        world.get::<Selection>(self.root)?.popup
    }
    /// Stable canonical option target, useful for synthetic tests/host observation.
    pub fn option(self, world: &World, id: OptionId) -> Option<Entity> {
        world.get::<Selection>(self.root)?.rows.get(&id).copied()
    }
    /// Destroy all owned slots including the detached popup, without host mutation.
    pub fn unmount(self, world: &mut World) -> Result<(), UiError> {
        let s = world
            .get::<Selection>(self.root)
            .cloned()
            .ok_or(UiError::Stale)?;
        if let Some(p) = s.popup {
            close_popup(world, p, false);
        }
        fn collect(w: &World, e: Entity, out: &mut Vec<Entity>) {
            out.push(e);
            for c in children_of(w, e) {
                collect(w, c, out);
            }
        }
        let mut all = Vec::new();
        collect(world, self.root, &mut all);
        if let Some(p) = s.popup {
            collect(world, p, &mut all);
        }
        forget_interaction_targets(world, &all);
        if let Some(p) = s.popup {
            despawn(world, p);
        }
        despawn(world, self.root);
        Ok(())
    }
    /// Close without reverting previously requested state.
    pub fn close(self, world: &mut World) {
        if let Some(p) = self.popup(world) {
            close_popup(world, p, true);
        }
    }
}
/// Recognizable themed square/check with label. Mixed state deliberately unsupported.
pub struct Checkbox;
/// Noneditable popup selector with zero/one selected stable identity.
pub struct SingleSelect;
/// Popup selector retaining multiple choices; toggling does not close the popup.
pub struct MultiSelect;
impl Checkbox {
    /// Reconcile authoritative checked state. Does not send a change request.
    pub fn mount(
        world: &mut World,
        parent: Entity,
        key: UiId,
        props: SelectionProps,
        checked: bool,
    ) -> Result<SelectionHandle, UiError> {
        mount(
            world,
            parent,
            key,
            props,
            Kind::Check,
            &[],
            SelectionValue::Checked(checked),
        )
    }
}
impl SingleSelect {
    /// Reconcile host-owned selection; stale IDs display as no selection.
    pub fn mount(
        world: &mut World,
        parent: Entity,
        key: UiId,
        props: SelectionProps,
        options: &[SelectOption],
        selected: Option<OptionId>,
    ) -> Result<SelectionHandle, UiError> {
        mount(
            world,
            parent,
            key,
            props,
            Kind::Single,
            options,
            SelectionValue::Selected(selected.into_iter().collect()),
        )
    }
}
impl MultiSelect {
    /// Reconcile host-owned selected identities; unknown/removed IDs are ignored.
    pub fn mount(
        world: &mut World,
        parent: Entity,
        key: UiId,
        props: SelectionProps,
        options: &[SelectOption],
        selected: &[OptionId],
    ) -> Result<SelectionHandle, UiError> {
        if selected.len() > MAX_OPTIONS {
            return Err(UiError::InvalidProps);
        }
        mount(
            world,
            parent,
            key,
            props,
            Kind::Multi,
            options,
            SelectionValue::Selected(selected.to_vec()),
        )
    }
}
pub(crate) fn key_at(world: &World, e: Entity) -> Option<UiId> {
    world.get::<Selection>(e).map(|s| s.key)
}
fn valid(props: &SelectionProps, options: &[SelectOption]) -> bool {
    let mut seen = BTreeSet::new();
    !props.label.trim().is_empty()
        && !props.empty.trim().is_empty()
        && options.len() <= MAX_OPTIONS
        && options
            .iter()
            .all(|o| !o.label.trim().is_empty() && seen.insert(o.id))
}
fn node(w: &mut World, parent: Option<Entity>, layout: Layout) -> Entity {
    let e = w.spawn((layout,));
    if let Some(p) = parent {
        set_parent(w, e, p);
    }
    e
}
fn projection(role: AccessRole) -> SelectionAccess {
    SelectionAccess {
        role,
        checked: None,
        expanded: None,
        multiselect: false,
        controls: None,
        labelled_by: None,
    }
}
fn mount(
    w: &mut World,
    parent: Entity,
    key: UiId,
    props: SelectionProps,
    kind: Kind,
    options: &[SelectOption],
    mut value: SelectionValue,
) -> Result<SelectionHandle, UiError> {
    if !w.is_alive(parent) || !w.has::<Layout>(parent) {
        return Err(UiError::Stale);
    }
    if !valid(&props, options) {
        return Err(UiError::InvalidProps);
    }
    let existing = children_of(w, parent).into_iter().find(|e| {
        crate::controls::key_at(w, *e).or_else(|| crate::surfaces::key_at(w, *e)) == Some(key)
    });
    if let Some(e) = existing {
        if !w.get::<Selection>(e).is_some_and(|s| s.kind == kind) {
            return Err(UiError::InvalidProps);
        }
    }
    if let SelectionValue::Selected(ids) = &mut value {
        ids.retain(|id| options.iter().any(|o| o.id == *id));
        ids.sort();
        ids.dedup();
    }
    let (root, mut s) = if let Some(e) = existing {
        (e, w.get::<Selection>(e).unwrap().clone())
    } else {
        let (e, label, face, mark) = if kind == Kind::Check {
            let e = node(
                w,
                Some(parent),
                Layout {
                    width: Size::Fill,
                    padding: 8.0,
                    gap: 8.0,
                    ..Layout::row()
                },
            );
            w.insert(e, Button::new("checkbox"));
            w.insert(e, ReleaseActivation);
            w.insert(e, Focusable);
            let border = node(
                w,
                Some(e),
                Layout {
                    width: Size::Px(22.0),
                    height: Size::Px(22.0),
                    padding: 2.0,
                    ..Layout::column()
                },
            );
            w.insert(
                border,
                UiStyle {
                    background: Some(Token::StrongStroke.into()),
                    corner: Some(Corner::Compact),
                    ..UiStyle::default()
                },
            );
            let face = node(
                w,
                Some(border),
                Layout {
                    width: Size::Fill,
                    height: Size::Fill,
                    ..Layout::column()
                },
            );
            let mark = node(
                w,
                Some(face),
                Layout {
                    width: Size::Fill,
                    height: Size::Fill,
                    ..Layout::column()
                },
            );
            w.insert(
                mark,
                CrossMark {
                    color: Color::rgb(1., 1., 1.),
                },
            );
            let label = text(w, e, &props.label, Typography::Label)?;
            w.get_mut::<Layout>(label).unwrap().width = Size::Fill;
            (e, label, Some(face), Some(mark))
        } else {
            let h = controls::Button::mount(
                w,
                parent,
                key,
                controls::ButtonProps::new(&props.label, controls::HostIntent::Command(key.0)),
            )?;
            (h.root, h.label, None, None)
        };
        let popup = if kind != Kind::Check {
            let p = node(
                w,
                None,
                Layout {
                    width: Size::Fill,
                    height: Size::Hug,
                    padding: 4.0,
                    gap: 2.0,
                    ..Layout::column()
                },
            );
            w.insert(p, Scroll::default());
            w.insert(p, Hidden);
            w.insert(p, Focusable);
            w.insert(p, TabStop(false));
            w.insert(p, KeyNavigation(navigate));
            w.insert(
                p,
                AnchoredPopup {
                    anchor: e,
                    max_height: 320.0,
                    closed,
                },
            );
            w.insert(p, FocusOwner(e));
            w.insert(
                p,
                UiStyle {
                    background: Some(Token::Overlay.into()),
                    corner: Some(Corner::Compact),
                    ..UiStyle::default()
                },
            );
            Some(p)
        } else {
            None
        };
        (
            e,
            Selection {
                key,
                kind,
                props: props.clone(),
                value: value.clone(),
                requested: value.clone(),
                label,
                box_face: face,
                mark,
                popup,
                rows: BTreeMap::new(),
                options: Vec::new(),
                empty: None,
            },
        )
    };
    let old_focus = w.resource::<Focus>().and_then(|f| f.entity);
    let removed_focus = old_focus.is_some_and(|f| {
        s.rows
            .iter()
            .any(|(id, e)| *e == f && !options.iter().any(|o| o.id == *id && o.enabled))
    });
    s.props = props;
    s.value = value.clone();
    s.requested = value;
    s.options = options.to_vec();
    if let Some(popup) = s.popup {
        // Normalize focus before destroying its old row; focus boundaries remain linked.
        if removed_focus {
            request_focus(w, popup);
        }
        let removed: Vec<_> = s
            .rows
            .keys()
            .filter(|id| !options.iter().any(|o| o.id == **id))
            .copied()
            .collect();
        for id in removed {
            let e = s.rows.remove(&id).unwrap();
            forget_interaction_targets(w, &[e]);
            despawn(w, e);
        }
        for o in options {
            let mut bp =
                controls::ButtonProps::new(&o.label, controls::HostIntent::Command(o.id.0));
            bp.enabled = o.enabled;
            bp.readonly = s.props.readonly;
            let h = controls::NavigationRow::mount(
                w,
                popup,
                UiId(o.id.0),
                bp,
                controls::NavigationMeta::default(),
            )?;
            s.rows.insert(o.id, h.root);
            w.insert(
                h.root,
                OptionOwner {
                    control: root,
                    id: o.id,
                },
            );
            w.insert(h.root, ActivationHandler(activate_option));
            w.insert(h.root, KeyNavigation(navigate));
            w.insert(h.root, TabStop(false));
        }
        // Presentation order changes without replacing stable option entities.
        let order: Vec<_> = options
            .iter()
            .map(|o| s.rows[&o.id])
            .chain(s.empty)
            .collect();
        w.insert(popup, Children { list: order });
        if options.is_empty() {
            if s.empty.is_none() {
                s.empty = Some(text(w, popup, &s.props.empty, Typography::Secondary)?);
            }
            if let Some(e) = s.empty {
                w.get_mut::<Label>(e).unwrap().text = s.props.empty.clone();
                w.remove::<Hidden>(e);
            }
        } else if let Some(e) = s.empty {
            w.insert(e, Hidden);
        }
    }
    w.insert(root, s);
    w.insert(root, KeyNavigation(navigate));
    w.insert(root, ActivationHandler(activate));
    refresh(w, root);
    if let Some(popup) = w.get::<Selection>(root).and_then(|s| s.popup) {
        if !w.has::<Hidden>(popup) {
            if !access_details(w, popup).activate
                && (w.get::<Selection>(root).unwrap().props.readonly
                    || !w.get::<Selection>(root).unwrap().props.enabled)
            {
                close_popup(w, popup, true);
            } else if removed_focus {
                focus_first(w, root, false);
            }
        }
    }
    Ok(SelectionHandle { root })
}
fn chosen(value: &SelectionValue, id: OptionId) -> bool {
    matches!(value, SelectionValue::Selected(ids) if ids.contains(&id))
}
fn refresh(w: &mut World, root: Entity) {
    let Some(s) = w.get::<Selection>(root).cloned() else {
        return;
    };
    if s.kind == Kind::Check {
        let checked = s.value == SelectionValue::Checked(true);
        w.get_mut::<Label>(s.label).unwrap().text = s.props.label.clone();
        let mut a = projection(AccessRole::CheckBox);
        a.checked = Some(checked);
        w.insert(root, a);
        let focused = w.resource::<Focus>().and_then(|f| f.entity) == Some(root);
        let available = access_details(w, root).focus;
        let scale = w
            .resource::<crate::UiTheme>()
            .map(|t| t.text_scale)
            .unwrap_or(1.0)
            .clamp(0.75, 3.0);
        w.insert(
            root,
            UiStyle {
                background: Some(Token::Content.into()),
                corner: Some(Corner::Control),
                ..UiStyle::default()
            },
        );
        w.get_mut::<Layout>(root).unwrap().min_height = 44.0;
        w.insert(
            s.label,
            UiStyle {
                foreground: Some(
                    if available {
                        Token::Text
                    } else {
                        Token::Disabled
                    }
                    .into(),
                ),
                typography: Some(Typography::Label),
                ..UiStyle::default()
            },
        );
        if let Some(border) = s
            .box_face
            .and_then(|e| w.get::<Parent>(e).map(|p| p.entity))
        {
            let layout = w.get_mut::<Layout>(border).unwrap();
            layout.width = Size::Px(22.0 * scale);
            layout.height = Size::Px(22.0 * scale);
            layout.padding = 2.0 * scale;
            // Indicator geometry is relative to the face, not a font baseline.
            w.insert(
                border,
                UiStyle {
                    background: Some(
                        if focused {
                            Token::Focus
                        } else {
                            Token::StrongStroke
                        }
                        .into(),
                    ),
                    corner: Some(Corner::Compact),
                    ..UiStyle::default()
                },
            );
        }
        if let Some(face) = s.box_face {
            w.insert(
                face,
                UiStyle {
                    background: Some(
                        if checked {
                            Token::Accent
                        } else {
                            Token::Content
                        }
                        .into(),
                    ),
                    ..UiStyle::default()
                },
            );
        }
        if let Some(mark) = s.mark {
            if checked {
                w.remove::<Hidden>(mark);
            } else {
                w.insert(mark, Hidden);
            }
            let theme = w.resource::<crate::UiTheme>().cloned().unwrap_or_default();
            w.insert(
                mark,
                CrossMark {
                    color: theme.color(Token::OnAccent),
                },
            );
        }
        if let Some(b) = w.get_mut::<Button>(root) {
            b.enabled = s.props.enabled;
        }
    } else {
        let summary: Vec<_> = s
            .options
            .iter()
            .filter(|o| chosen(&s.value, o.id))
            .map(|o| o.label.as_str())
            .collect();
        let summary = if summary.is_empty() {
            "None selected".into()
        } else if summary.len() > 3 {
            format!(
                "{}, +{} more ({} selected)",
                summary[..3].join(", "),
                summary.len() - 3,
                summary.len()
            )
        } else {
            summary.join(", ")
        };
        let mut bp = controls::ButtonProps::new(
            format!("{}: {}  v", s.props.label, summary),
            controls::HostIntent::Command(s.key.0),
        );
        bp.enabled = s.props.enabled;
        bp.readonly = s.props.readonly;
        bp.name = Some(format!("{}: {}", s.props.label, summary));
        bp.description = s.props.description.clone();
        if let Some(parent) = w.get::<Parent>(root).map(|p| p.entity) {
            let _ = controls::Button::mount(w, parent, s.key, bp);
        }
        let popup = s.popup.unwrap();
        let open = !w.has::<Hidden>(popup);
        let mut a = projection(AccessRole::ComboBox);
        a.expanded = Some(open);
        a.controls = open.then_some(popup);
        w.insert(root, a);
        let mut a = projection(AccessRole::ListBox);
        a.multiselect = s.kind == Kind::Multi;
        a.labelled_by = Some(root);
        w.insert(popup, a);
        w.insert(
            popup,
            Semantics {
                name: Some(s.props.label.clone()),
                ..Semantics::default()
            },
        );
        for o in &s.options {
            let e = s.rows[&o.id];
            let selected = chosen(&s.value, o.id);
            let mut bp = controls::ButtonProps::new(
                if s.kind == Kind::Multi {
                    format!("[{}] {}", if selected { "x" } else { " " }, o.label)
                } else {
                    o.label.clone()
                },
                controls::HostIntent::Command(o.id.0),
            );
            bp.name = Some(o.label.clone());
            bp.enabled = o.enabled;
            bp.readonly = s.props.readonly;
            let _ = controls::NavigationRow::mount(
                w,
                popup,
                UiId(o.id.0),
                bp,
                controls::NavigationMeta {
                    selected,
                    ..Default::default()
                },
            );
            w.insert(e, projection(AccessRole::Option));
            w.insert(e, SelectedState(selected));
        }
    }
    if s.props.enabled {
        w.remove::<ControlDisabled>(root);
    } else {
        w.insert(root, ControlDisabled);
    }
    let current_name = w.get::<Semantics>(root).and_then(|s| s.name.clone());
    w.insert(
        root,
        Semantics {
            name: if s.kind == Kind::Check {
                Some(s.props.label)
            } else {
                current_name
            },
            description: s.props.description,
            readonly: s.props.readonly,
        },
    );
}
fn emit(w: &mut World, root: Entity, value: SelectionValue) {
    if let Some(s) = w.get_mut::<Selection>(root) {
        s.requested = value.clone();
    }
    if let Some(h) = w.get::<ChangeHandler>(root).copied() {
        (h.0)(w, root, &value);
    } else {
        send(
            w,
            SelectionChange {
                control: root,
                value,
            },
        );
    }
}
fn activate(w: &mut World, root: Entity) {
    let Some(s) = w.get::<Selection>(root).cloned() else {
        return;
    };
    if s.kind == Kind::Check {
        emit(
            w,
            root,
            SelectionValue::Checked(s.requested != SelectionValue::Checked(true)),
        );
    } else if let Some(p) = s.popup {
        if w.has::<Hidden>(p) {
            if open_popup(w, p) {
                focus_first(w, root, false);
                refresh(w, root);
            }
        } else {
            close_popup(w, p, true);
        }
    }
}
fn activate_option(w: &mut World, e: Entity) {
    let Some(o) = w.get::<OptionOwner>(e).copied() else {
        return;
    };
    let Some(s) = w.get::<Selection>(o.control).cloned() else {
        return;
    };
    if !s.options.iter().any(|i| i.id == o.id && i.enabled) {
        return;
    }
    let value = if s.kind == Kind::Single {
        SelectionValue::Selected(vec![o.id])
    } else {
        let mut ids = match s.requested {
            SelectionValue::Selected(ids) => ids,
            _ => vec![],
        };
        if ids.contains(&o.id) {
            ids.retain(|id| *id != o.id);
        } else {
            ids.push(o.id);
        }
        ids.sort();
        SelectionValue::Selected(ids)
    };
    emit(w, o.control, value);
    if s.kind == Kind::Single {
        if let Some(p) = s.popup {
            close_popup(w, p, true);
        }
    }
}
fn closed(w: &mut World, popup: Entity) {
    if let Some(p) = w.get::<AnchoredPopup>(popup) {
        let root = p.anchor;
        refresh(w, root);
    }
}
fn focus_first(w: &mut World, root: Entity, last: bool) {
    let Some(s) = w.get::<Selection>(root).cloned() else {
        return;
    };
    let enabled: Vec<_> = s.options.iter().filter(|o| o.enabled).collect();
    let target = if last {
        enabled.last().copied()
    } else {
        enabled
            .iter()
            .find(|o| chosen(&s.value, o.id))
            .copied()
            .or_else(|| enabled.first().copied())
    };
    if let Some(o) = target {
        request_focus(w, s.rows[&o.id]);
    } else if let Some(p) = s.popup {
        request_focus(w, p);
    }
}
pub(crate) fn navigate(w: &mut World, e: Entity, key: &KeyEvent) -> bool {
    let root = if w.has::<Selection>(e) {
        e
    } else if let Some(o) = w.get::<OptionOwner>(e) {
        o.control
    } else if let Some(p) = w.get::<AnchoredPopup>(e) {
        p.anchor
    } else {
        return false;
    };
    let Some(s) = w.get::<Selection>(root).cloned() else {
        return false;
    };
    let action = matches!(
        key.logical,
        LogicalKey::Named(
            NamedKey::Enter
                | NamedKey::Space
                | NamedKey::ArrowUp
                | NamedKey::ArrowDown
                | NamedKey::Home
                | NamedKey::End
        )
    );
    if action
        && (key.synthetic
            || key.repeat
            || w.resource::<Input>().is_some_and(|i| {
                i.ime_enabled || !i.ime_preedit.is_empty() || !i.ime_commit.is_empty()
            }))
    {
        return true;
    }
    if key.released {
        return false;
    }
    let LogicalKey::Named(
        k @ (NamedKey::ArrowUp | NamedKey::ArrowDown | NamedKey::Home | NamedKey::End),
    ) = key.logical
    else {
        return false;
    };
    if s.kind == Kind::Check {
        return false;
    }
    let popup = s.popup.unwrap();
    if w.has::<Hidden>(popup) {
        if !access_details(w, root).activate {
            return true;
        }
        if open_popup(w, popup) {
            focus_first(w, root, matches!(k, NamedKey::ArrowUp | NamedKey::End));
            refresh(w, root);
        }
        return true;
    }
    let rows: Vec<_> = s
        .options
        .iter()
        .filter(|o| o.enabled)
        .map(|o| s.rows[&o.id])
        .collect();
    if rows.is_empty() {
        return true;
    }
    let at = rows.iter().position(|r| *r == e).unwrap_or(0);
    let at = match k {
        NamedKey::Home => 0,
        NamedKey::End => rows.len() - 1,
        NamedKey::ArrowUp => at.saturating_sub(1),
        _ => (at + 1).min(rows.len() - 1),
    };
    request_focus(w, rows[at]);
    reveal_entity(w, rows[at]);
    true
}
/// Theme/focus refresh only; does not dispatch input or alter selection.
pub(crate) fn styles(w: &mut World) {
    for e in w.entities_with::<Selection>() {
        if w.get::<Selection>(e).is_some_and(|s| s.kind == Kind::Check) {
            refresh(w, e);
        }
    }
}
