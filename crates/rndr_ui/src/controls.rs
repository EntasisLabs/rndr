//! Operating controls on core's canonical input/intent path. See README contracts.
use crate::{Token, Typography, UiError, UiId, UiRecipe, UiStyle, UiTheme};
use rndr_core::{Button as CoreButton, *};

/// Emphasis, not an additional action or checked state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonVariant {
    /// Accent command.
    Primary,
    /// Bordered neutral command.
    Secondary,
    /// Quiet neutral command.
    Quiet,
    /// Explicitly named destructive command; no confirmation is implied.
    Destructive,
}
/// Opaque host-owned source location. The UI never interprets text or offsets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceLocator {
    /// Host document identity.
    pub document: u64,
    /// Host revision token.
    pub revision: u64,
    /// Host anchor token, not an executable string or assumed byte offset.
    pub anchor: u64,
}
/// Typed request only. Hosts validate identity/revision/permission before use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostIntent {
    /// Application-local command token; no dispatch implementation in the UI.
    Command(u64),
    /// Request source reveal, never mutation or executable Liquid.
    RevealSource(SourceLocator),
}
/// Button properties, updated in place. Busy/validation/checked are unsupported.
#[derive(Clone, Debug, PartialEq)]
pub struct ButtonProps {
    /// Visible wrapped label (must not be blank).
    pub label: String,
    /// Persistent accessible name; otherwise the full visible label is used.
    pub name: Option<String>,
    /// Supplementary description.
    pub description: String,
    /// Opaque request carried through the reviewed core FIFO64.
    pub intent: HostIntent,
    /// Visual emphasis only.
    pub variant: ButtonVariant,
    /// Disabled controls cannot focus or activate.
    pub enabled: bool,
    /// Readonly controls can focus, but cannot activate.
    pub readonly: bool,
}
impl ButtonProps {
    /// Enabled secondary command with a visible name.
    pub fn new(label: impl Into<String>, intent: HostIntent) -> Self {
        Self {
            label: label.into(),
            name: None,
            description: String::new(),
            intent,
            variant: ButtonVariant::Secondary,
            enabled: true,
            readonly: false,
        }
    }
}
/// Optional navigation metadata, part of the persistent accessible name.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NavigationMeta {
    /// Host-selected state; activation does not silently change selection.
    pub selected: bool,
    /// Optional secondary label, retained visibly and accessibly.
    pub secondary: Option<String>,
    /// Explicit textual unsaved marker; no claim about a real save journal.
    pub dirty: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Button,
    Icon,
    Navigation,
    Source,
    Tab,
}
#[derive(Clone)]
struct Control {
    id: UiId,
    kind: Kind,
    props: ButtonProps,
    meta: NavigationMeta,
    gap: Entity,
    border: Entity,
    face: Entity,
    label: Entity,
}
impl Component for Control {}
#[derive(Clone, Copy)]
struct OwnedBy(Entity);
impl Component for OwnedBy {}

/// Stable generation-checked root and visible-label slot. No interactive children.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ControlHandle {
    /// Canonical Button or Tab entity, not a wrapper activation target.
    pub root: Entity,
    /// Owned wrapped Label. Do not remove/reparent; use mount to update.
    pub label: Entity,
}
/// Primary/secondary/quiet/destructive command.
pub struct Button;
/// Icon command requiring a persistent nonempty accessible name.
pub struct IconButton;
/// Navigation command with host-owned selected and textual metadata.
pub struct NavigationRow;
/// Compact visible labelled source request; Tab then Enter/Space is its key route.
pub struct SourceAction;
impl Button {
    /// Mount/update under a caller-stable key; never replaces focus or history.
    pub fn mount(
        world: &mut World,
        parent: Entity,
        id: UiId,
        props: ButtonProps,
    ) -> Result<ControlHandle, UiError> {
        mount_control(
            world,
            parent,
            id,
            Kind::Button,
            props,
            NavigationMeta::default(),
        )
    }
}
impl IconButton {
    /// `glyph` is caller-provided text/art, not a licensed OS-symbol asset.
    /// Name is required; no generic icon fallback is silently announced.
    pub fn mount(
        world: &mut World,
        parent: Entity,
        id: UiId,
        glyph: &str,
        name: &str,
        mut props: ButtonProps,
    ) -> Result<ControlHandle, UiError> {
        if name.trim().is_empty() {
            return Err(UiError::InvalidProps);
        }
        props.label = glyph.into();
        props.name = Some(name.into());
        mount_control(
            world,
            parent,
            id,
            Kind::Icon,
            props,
            NavigationMeta::default(),
        )
    }
}
impl NavigationRow {
    /// Selected and focused are independent. Metadata is visible and announced.
    pub fn mount(
        world: &mut World,
        parent: Entity,
        id: UiId,
        mut props: ButtonProps,
        meta: NavigationMeta,
    ) -> Result<ControlHandle, UiError> {
        props.variant = ButtonVariant::Quiet;
        mount_control(world, parent, id, Kind::Navigation, props, meta)
    }
}
impl SourceAction {
    /// Visible "Show source" label; host validates every locator at consumption.
    pub fn mount(
        world: &mut World,
        parent: Entity,
        id: UiId,
        locator: SourceLocator,
        enabled: bool,
    ) -> Result<ControlHandle, UiError> {
        let mut props = ButtonProps::new("Show source", HostIntent::RevealSource(locator));
        props.variant = ButtonVariant::Quiet;
        props.enabled = enabled;
        props.description = "Tab to Show source; Enter or Space requests source reveal".into();
        mount_control(
            world,
            parent,
            id,
            Kind::Source,
            props,
            NavigationMeta::default(),
        )
    }
}
fn action(props: &ButtonProps) -> String {
    format!("rndr-ui:{:?}", props.intent)
}
fn parent_valid(world: &World, parent: Entity) -> Result<(), UiError> {
    if world.is_alive(parent) && world.has::<Layout>(parent) {
        Ok(())
    } else {
        Err(UiError::Stale)
    }
}
/// Ownership guard shared with passive adoption; includes all internal slots,
/// not just keyed component roots. Borrowed caller subtrees remain unmarked.
pub(crate) fn owns_entity(world: &World, entity: Entity) -> bool {
    world.has::<OwnedBy>(entity)
}

/// Shared identity check also used by passive surfaces; no global registry.
pub(crate) fn key_at(world: &World, entity: Entity) -> Option<UiId> {
    world
        .get::<Control>(entity)
        .map(|c| c.id)
        .or_else(|| world.get::<Selector>(entity).map(|s| s.id))
        .or_else(|| world.get::<ToolbarNode>(entity).map(|s| s.id))
}
fn find(world: &World, parent: Entity, id: UiId) -> Option<Entity> {
    children_of(world, parent)
        .into_iter()
        .find(|e| key_at(world, *e).or_else(|| crate::surfaces::key_at(world, *e)) == Some(id))
}
fn primitive(world: &mut World, parent: Entity, owner: Entity, padding: f32) -> Entity {
    let e = world.spawn((
        Layout {
            width: Size::Fill,
            padding,
            ..Layout::column()
        },
        OwnedBy(owner),
        ResolvedColor,
    ));
    set_parent(world, e, parent);
    e
}
fn mount_control(
    world: &mut World,
    parent: Entity,
    id: UiId,
    kind: Kind,
    props: ButtonProps,
    meta: NavigationMeta,
) -> Result<ControlHandle, UiError> {
    parent_valid(world, parent)?;
    if props.label.trim().is_empty() || props.name.as_ref().is_some_and(|n| n.trim().is_empty()) {
        return Err(UiError::InvalidProps);
    }
    let h = if let Some(root) = find(world, parent, id) {
        let c = world
            .get::<Control>(root)
            .ok_or(UiError::IdentityConflict)?;
        if c.kind != kind {
            return Err(UiError::IdentityConflict);
        }
        let h = ControlHandle {
            root,
            label: c.label,
        };
        h.validate(world)?;
        h
    } else {
        let root = world.spawn((
            Layout {
                width: Size::Fill,
                padding: 2.0,
                ..Layout::column()
            },
            Focusable,
            ReleaseActivation,
            ResolvedColor,
        ));
        world.insert(root, OwnedBy(root));
        set_parent(world, root, parent);
        let gap = primitive(world, root, root, 2.0);
        let border = primitive(world, gap, root, 1.0);
        let face = primitive(world, border, root, 4.0);
        let label = primitive(world, face, root, 0.0);
        world.insert(label, Label::new(&props.label).overflow(TextOverflow::Wrap));
        world.insert(
            root,
            Control {
                id,
                kind,
                props: props.clone(),
                meta: meta.clone(),
                gap,
                border,
                face,
                label,
            },
        );
        ControlHandle { root, label }
    };
    // Changing action or availability cancels an in-flight gesture before update.
    let old = world.get::<Control>(h.root).unwrap();
    if (old.props.intent != props.intent
        || old.props.enabled != props.enabled
        || old.props.readonly != props.readonly)
        && interaction_pressed(world, h.root)
    {
        cancel_activation(world);
    }
    let c = world.get_mut::<Control>(h.root).unwrap();
    c.props = props.clone();
    c.meta = meta.clone();
    if kind != Kind::Tab {
        world.insert(
            h.root,
            CoreButton {
                action: action(&props),
                enabled: props.enabled,
            },
        );
    }
    if props.enabled {
        world.remove::<ControlDisabled>(h.root);
    } else {
        world.insert(h.root, ControlDisabled);
    }
    let mut name = props.name.clone().unwrap_or_else(|| props.label.clone());
    if kind == Kind::Navigation {
        if let Some(v) = &meta.secondary {
            name.push_str(&format!(", {v}"));
        }
        if meta.dirty {
            name.push_str(", unsaved");
        }
        world.insert(h.root, SelectedState(meta.selected));
    }
    world.insert(
        h.root,
        Semantics {
            name: Some(name),
            description: props.description,
            readonly: props.readonly,
        },
    );
    Ok(h)
}
impl ControlHandle {
    /// Reject stale generations, removed primitives and reparented owned slots.
    pub fn validate(self, world: &World) -> Result<(), UiError> {
        let c = world.get::<Control>(self.root).ok_or(UiError::Stale)?;
        if c.label != self.label
            || !world.has::<Layout>(self.root)
            || world.get::<OwnedBy>(self.root).map(|o| o.0) != Some(self.root)
            || !world.has::<ReleaseActivation>(self.root)
            || !world.has::<Label>(self.label)
        {
            return Err(UiError::Stale);
        }
        if c.kind == Kind::Tab {
            if !world.has::<Tab>(self.root) {
                return Err(UiError::Stale);
            }
        } else if !world.has::<CoreButton>(self.root) {
            return Err(UiError::Stale);
        }
        for (child, parent) in [
            (c.gap, self.root),
            (c.border, c.gap),
            (c.face, c.border),
            (c.label, c.face),
        ] {
            if world.get::<OwnedBy>(child).map(|o| o.0) != Some(self.root)
                || world.get::<Parent>(child).map(|p| p.entity) != Some(parent)
                || !world.has::<Layout>(child)
            {
                return Err(UiError::Stale);
            }
        }
        Ok(())
    }
    /// Destroy owned primitives only; any borrowed subtree is detached, alive.
    pub fn destroy(self, world: &mut World) -> Result<(), UiError> {
        self.validate(world)?;
        destroy_owned(world, self.root);
        Ok(())
    }
}
fn destroy_owned(world: &mut World, root: Entity) {
    fn collect(world: &mut World, e: Entity, root: Entity, out: &mut Vec<Entity>) {
        out.push(e);
        for child in children_of(world, e) {
            if world.get::<OwnedBy>(child).map(|o| o.0) == Some(root) {
                collect(world, child, root, out);
            } else {
                world.remove::<Parent>(child);
                if let Some(c) = world.get_mut::<Children>(e) {
                    c.list.retain(|v| *v != child);
                }
            }
        }
    }
    let mut owned = vec![];
    collect(world, root, root, &mut owned);
    forget_interaction_targets(world, &owned);
    despawn(world, root);
}
/// Validate a core request against its current UI identity and typed payload.
/// This consumes no queue and executes nothing. Drain only `take_action_intents`;
/// preserve order/multiplicity and apply host permission/revision checks next.
pub fn host_intent(world: &World, request: &ActionIntent) -> Option<HostIntent> {
    let c = world.get::<Control>(request.entity)?;
    let h = ControlHandle {
        root: request.entity,
        label: c.label,
    };
    (h.validate(world).is_ok()
        && access_details(world, h.root).activate
        && request.name == action(&c.props)
        && c.kind != Kind::Tab)
        .then_some(c.props.intent)
}

#[derive(Clone)]
struct Selector {
    id: UiId,
    group: Entity,
    tabs: Vec<ControlHandle>,
    panels: Vec<Entity>,
}
impl Component for Selector {}
/// Manual selector with actual panels; content is borrowed unless caller-owned
/// components are independently destroyed. No buffer or selection copying.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModeHandle {
    /// Stable component root.
    pub root: Entity,
    /// Named native tablist with explicit orientation.
    pub group: Entity,
    /// Canonical Tab targets in hierarchy order.
    pub tabs: Vec<ControlHandle>,
    /// Stable panel content slots, hidden/excluded while inactive.
    pub panels: Vec<Entity>,
}
/// Manual Source/Preview-style tabs; arrows focus, Enter/Space activate.
pub struct ModeSelector;
impl ModeSelector {
    /// Mount/update names without resetting active tab or document state.
    /// `initial` is used only at creation. Count changes require explicit destroy
    /// and remount; no silent content loss or heuristic reconciliation.
    pub fn mount(
        world: &mut World,
        parent: Entity,
        id: UiId,
        name: &str,
        labels: &[&str],
        orientation: TabOrientation,
        initial: usize,
    ) -> Result<ModeHandle, UiError> {
        parent_valid(world, parent)?;
        if name.trim().is_empty()
            || labels.is_empty()
            || initial >= labels.len()
            || labels.iter().any(|s| s.trim().is_empty())
        {
            return Err(UiError::InvalidProps);
        }
        let h = if let Some(root) = find(world, parent, id) {
            let s = world
                .get::<Selector>(root)
                .ok_or(UiError::IdentityConflict)?;
            let h = ModeHandle {
                root,
                group: s.group,
                tabs: s.tabs.clone(),
                panels: s.panels.clone(),
            };
            h.validate(world)?;
            if h.tabs.len() != labels.len() {
                return Err(UiError::IdentityConflict);
            }
            h
        } else {
            let root = world.spawn((Layout {
                width: Size::Fill,
                gap: 8.0,
                ..Layout::column()
            },));
            world.insert(root, OwnedBy(root));
            set_parent(world, root, parent);
            let group = primitive(world, root, root, 0.0);
            world.insert(
                group,
                Tabs {
                    selected: initial as u64,
                },
            );
            let mut tabs = vec![];
            let mut panels = vec![];
            for (i, label) in labels.iter().enumerate() {
                let panel = primitive(world, root, root, 0.0);
                world.insert(panel, TabPanel);
                let tab = mount_control(
                    world,
                    group,
                    UiId(i as u64),
                    Kind::Tab,
                    ButtonProps::new(*label, HostIntent::Command(0)),
                    NavigationMeta::default(),
                )?;
                world.insert(tab.root, Tab { id: i as u64 });
                link_tab_panel(world, tab.root, group, panel).map_err(|_| UiError::Stale)?;
                tabs.push(tab);
                panels.push(panel);
            }
            world.insert(
                root,
                Selector {
                    id,
                    group,
                    tabs: tabs.clone(),
                    panels: panels.clone(),
                },
            );
            ModeHandle {
                root,
                group,
                tabs,
                panels,
            }
        };
        world.insert(h.group, ManualTabs(orientation));
        world.insert(
            h.group,
            Semantics {
                name: Some(name.into()),
                ..Semantics::default()
            },
        );
        let layout = world.get_mut::<Layout>(h.group).unwrap();
        layout.axis = if orientation == TabOrientation::Horizontal {
            Axis::Row
        } else {
            Axis::Column
        };
        layout.gap = 4.0;
        for (i, label) in labels.iter().enumerate() {
            let previous = world.get::<Control>(h.tabs[i].root).unwrap().props.clone();
            let props = ButtonProps {
                label: (*label).into(),
                ..previous
            };
            mount_control(
                world,
                h.group,
                UiId(i as u64),
                Kind::Tab,
                props,
                NavigationMeta::default(),
            )?;
        }
        Ok(h)
    }
}
impl ModeHandle {
    /// Validate all generations, direct-parent relations and owned content slots.
    pub fn validate(&self, world: &World) -> Result<(), UiError> {
        let s = world.get::<Selector>(self.root).ok_or(UiError::Stale)?;
        if self.group != s.group
            || !world.has::<Layout>(self.root)
            || !world.has::<Layout>(self.group)
            || self.tabs != s.tabs
            || self.panels != s.panels
            || world.get::<OwnedBy>(self.root).map(|o| o.0) != Some(self.root)
            || world.get::<OwnedBy>(self.group).map(|o| o.0) != Some(self.root)
            || world.get::<Parent>(self.group).map(|p| p.entity) != Some(self.root)
            || !world.has::<Tabs>(self.group)
        {
            return Err(UiError::Stale);
        }
        for (tab, panel) in self.tabs.iter().zip(&self.panels) {
            tab.validate(world)?;
            if world.get::<Parent>(tab.root).map(|p| p.entity) != Some(self.group)
                || world.get::<Parent>(*panel).map(|p| p.entity) != Some(self.root)
                || world.get::<OwnedBy>(*panel).map(|o| o.0) != Some(self.root)
                || !world.has::<Layout>(*panel)
                || !world.has::<TabPanel>(*panel)
                || world
                    .get::<TabRelation>(tab.root)
                    .is_none_or(|r| r.panel != *panel || r.group != self.group)
            {
                return Err(UiError::Stale);
            }
        }
        Ok(())
    }
    /// Update availability/readonly without changing active panel or content.
    pub fn set_available(
        &self,
        world: &mut World,
        index: usize,
        enabled: bool,
        readonly: bool,
    ) -> Result<(), UiError> {
        self.validate(world)?;
        let h = *self.tabs.get(index).ok_or(UiError::InvalidProps)?;
        let c = world.get::<Control>(h.root).unwrap().clone();
        let mut props = c.props;
        props.enabled = enabled;
        props.readonly = readonly;
        mount_control(
            world,
            self.group,
            UiId(index as u64),
            Kind::Tab,
            props,
            c.meta,
        )?;
        Ok(())
    }
    /// Remove owned tabs/panels, detaching all borrowed panel children alive.
    pub fn destroy(self, world: &mut World) -> Result<(), UiError> {
        self.validate(world)?;
        for tab in self.tabs {
            tab.destroy(world)?;
        }
        destroy_owned(world, self.root);
        Ok(())
    }
}

#[derive(Clone, Copy)]
struct ToolbarNode {
    id: UiId,
    available: f32,
}
impl Component for ToolbarNode {}
/// Responsive toolbar recipe, not a menu. Controls stay visible in order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ToolbarHandle {
    /// Layout/content slot. Children are borrowed, not destroyed by teardown.
    pub root: Entity,
}
/// Fits a row only if every direct control's unwrapped measured label fits;
/// otherwise stacks every action vertically. No fake overflow button or hiding.
pub struct Toolbar;
impl Toolbar {
    /// Host supplies logical available width on resize, in place.
    pub fn mount(
        world: &mut World,
        parent: Entity,
        id: UiId,
        available: f32,
    ) -> Result<ToolbarHandle, UiError> {
        parent_valid(world, parent)?;
        if !available.is_finite() || available < 32.0 {
            return Err(UiError::InvalidProps);
        }
        let root = if let Some(root) = find(world, parent, id) {
            if !world.has::<ToolbarNode>(root) {
                return Err(UiError::IdentityConflict);
            }
            ToolbarHandle { root }.validate(world)?;
            root
        } else {
            let e = world.spawn((Layout::column(),));
            world.insert(e, OwnedBy(e));
            set_parent(world, e, parent);
            e
        };
        world.insert(root, ToolbarNode { id, available });
        if !world.has::<Semantics>(root) {
            world.insert(
                root,
                Semantics {
                    name: Some("Commands".into()),
                    ..Semantics::default()
                },
            );
        }
        Ok(ToolbarHandle { root })
    }
}
impl ToolbarHandle {
    /// Validate the toolbar's own root, not ownership of its borrowed children.
    pub fn validate(self, world: &World) -> Result<(), UiError> {
        if !world.has::<ToolbarNode>(self.root)
            || !world.has::<Layout>(self.root)
            || world.get::<OwnedBy>(self.root).map(|o| o.0) != Some(self.root)
        {
            return Err(UiError::Stale);
        }
        Ok(())
    }
    /// Remove toolbar only; controls and caller subtrees detach intact.
    pub fn destroy(self, world: &mut World) -> Result<(), UiError> {
        self.validate(world)?;
        destroy_owned(world, self.root);
        Ok(())
    }
}
fn background(world: &mut World, entity: Entity, token: Token) {
    world.insert(
        entity,
        UiRecipe(UiStyle {
            background: Some(token.into()),
            ..UiStyle::default()
        }),
    );
}
/// Recipe resolution runs before the existing style resolver/Layout/paint.
/// No input processing, queue draining or host command dispatch occurs here.
pub(crate) fn resolve_controls(world: &mut World) {
    let theme = world.resource::<UiTheme>().cloned().unwrap_or_default();
    let focused = world.resource::<Focus>().and_then(|f| f.entity);
    let pointer = world.resource::<Input>().map(|i| i.pointer.position);
    let hovered = pointer
        .and_then(|p| hit_test(world, p[0], p[1]))
        .and_then(|e| {
            if world.has::<Control>(e) {
                Some(e)
            } else {
                ancestor_with::<Control>(world, e)
            }
        });
    for root in world.entities_with::<Control>() {
        let c = world.get::<Control>(root).unwrap().clone();
        if (ControlHandle {
            root,
            label: c.label,
        })
        .validate(world)
        .is_err()
        {
            continue;
        }
        let selected = if c.kind == Kind::Tab {
            world
                .get::<Tab>(root)
                .zip(world.get::<TabRelation>(root))
                .is_some_and(|(t, r)| {
                    world
                        .get::<Tabs>(r.group)
                        .is_some_and(|g| g.selected == t.id)
                })
        } else {
            c.meta.selected
        };
        let pressed = interaction_pressed(world, root);
        let hover = hovered == Some(root) && access_details(world, root).activate;
        let (fill, ink) = if !c.props.enabled {
            (Token::Navigation, Token::Disabled)
        } else if pressed && theme.contrast == crate::Contrast::High {
            (Token::Text, Token::Canvas)
        } else if selected {
            (Token::Selected, Token::Text)
        } else {
            match c.props.variant {
                ButtonVariant::Primary => (
                    if pressed {
                        Token::Pressed
                    } else if hover {
                        Token::Hover
                    } else {
                        Token::Accent
                    },
                    Token::OnAccent,
                ),
                ButtonVariant::Destructive => {
                    if pressed {
                        (Token::DangerText, Token::DangerFill)
                    } else {
                        (Token::DangerFill, Token::DangerText)
                    }
                }
                _ => (
                    if pressed {
                        Token::Selected
                    } else if hover {
                        Token::Navigation
                    } else {
                        Token::Content
                    },
                    Token::Text,
                ),
            }
        };
        background(
            world,
            root,
            if focused == Some(root) {
                Token::Focus
            } else {
                Token::Canvas
            },
        );
        background(world, c.gap, Token::Canvas);
        background(
            world,
            c.border,
            if pressed || hover {
                Token::Focus
            } else if c.props.variant == ButtonVariant::Destructive {
                Token::DangerStroke
            } else if c.props.variant == ButtonVariant::Quiet && !hover && !selected {
                fill
            } else {
                Token::StrongStroke
            },
        );
        background(world, c.face, fill);
        let mut label = c.props.label.clone();
        if c.kind == Kind::Navigation {
            if let Some(v) = &c.meta.secondary {
                label.push_str(&format!(" — {v}"));
            }
            if c.meta.dirty {
                label.push_str(" (unsaved)");
            }
        }
        if selected {
            label = format!("| {label}");
        }
        if c.props.readonly {
            label.push_str(" (read only)");
        }
        world.get_mut::<Label>(c.label).unwrap().text = label;
        world.insert(
            c.label,
            UiRecipe(UiStyle {
                foreground: Some(ink.into()),
                typography: Some(if c.kind == Kind::Source {
                    Typography::Secondary
                } else {
                    Typography::Label
                }),
                ..UiStyle::default()
            }),
        );
        let l = world.get_mut::<Layout>(root).unwrap();
        l.min_height = theme.target_min();
        l.min_width = theme.target_min();
    }
}

/// Measure only after the current cascade/type scale has resolved. This stays
/// before core layout, avoiding a frame of horizontal overflow on type changes.
pub(crate) fn resolve_toolbars(world: &mut World) {
    let theme = world.resource::<UiTheme>().cloned().unwrap_or_default();
    // Measure with the actual installed text engine and resolved type style.
    // Unknown borrowed children force stacking rather than assuming a fake menu.
    let engine = text_engine(world);
    for root in world.entities_with::<ToolbarNode>() {
        let node = *world.get::<ToolbarNode>(root).unwrap();
        let children: Vec<_> = children_of(world, root)
            .into_iter()
            .filter(|e| !world.has::<Hidden>(*e))
            .collect();
        let mut required = 8.0 * children.len().saturating_sub(1) as f32;
        let mut known = true;
        for child in &children {
            if let Some(c) = world.get::<Control>(*child) {
                if (ControlHandle {
                    root: *child,
                    label: c.label,
                })
                .validate(world)
                .is_err()
                {
                    known = false;
                    continue;
                }
                let label = world.get::<Label>(c.label).unwrap();
                let style = world
                    .get::<TextStyle>(c.label)
                    .cloned()
                    .unwrap_or_else(|| theme.typography(Typography::Label));
                required += (engine
                    .measure(&label.text, &style, &[], f32::INFINITY)
                    .width
                    + 52.0)
                    .max(theme.target_min());
            } else {
                known = false;
            }
        }
        if let Some(l) = world.get_mut::<Layout>(root) {
            l.width = Size::Px(node.available);
            l.axis = if known && required <= node.available {
                Axis::Row
            } else {
                Axis::Column
            };
            l.gap = 8.0;
        }
    }
}
