//! Drawn context and popup menus that work in native windows, headless and the browser.
//!
//! [`open_menu`] builds a transient menu on the core popup scope: at a point for
//! a [`SecondaryPress`], or below a button. Items carry a label, shortcut text,
//! a check mark, a disabled state and a danger style; separators, group labels
//! and submenus divide them. Arrows, Home/End, Enter/Space, Left/Right and Escape
//! drive the menu from the keyboard; hovering moves the highlight and opens
//! submenus. Picking an item sends [`MenuPicked`]; any other close sends
//! [`MenuDismissed`]. A closed menu is despawned during the next Layout.
use crate::{separator, text, Corner, Token, Typography, UiError, UiStyle};
use rndr_core::*;

/// Maximum entries in one menu, counting every submenu.
pub const MAX_ENTRIES: usize = 256;

/// A pickable menu item.
#[derive(Clone, Debug, PartialEq)]
pub struct MenuItem {
    /// Application identity sent in [`MenuPicked`].
    pub id: u64,
    /// Visible and accessible label; must not be blank.
    pub label: String,
    /// Shortcut text drawn at the trailing edge, such as `"Ctrl+C"`. Display only.
    pub shortcut: Option<String>,
    /// `Some` reserves a check mark, drawn when `true`.
    pub checked: Option<bool>,
    /// Disabled items are drawn dimmed and skipped by the keyboard.
    pub enabled: bool,
    /// Destructive action, drawn in the danger colour.
    pub danger: bool,
}
impl MenuItem {
    /// Enabled item without shortcut or check.
    pub fn new(id: u64, label: impl Into<String>) -> Self {
        Self {
            id,
            label: label.into(),
            shortcut: None,
            checked: None,
            enabled: true,
            danger: false,
        }
    }
    /// Show shortcut text.
    pub fn shortcut(mut self, text: impl Into<String>) -> Self {
        self.shortcut = Some(text.into());
        self
    }
    /// Show a check mark when `checked`.
    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = Some(checked);
        self
    }
    /// Set whether the item can be picked.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
    /// Draw as a destructive action.
    pub fn danger(mut self) -> Self {
        self.danger = true;
        self
    }
}

/// One line of a menu.
#[derive(Clone, Debug, PartialEq)]
pub enum MenuEntry {
    /// A pickable item.
    Item(MenuItem),
    /// A thin rule between groups.
    Separator,
    /// A dimmed heading for the items after it.
    Group(String),
    /// A row that opens a nested menu beside itself.
    Submenu {
        /// Visible label; must not be blank.
        label: String,
        /// Entries of the nested menu; must not be empty.
        entries: Vec<MenuEntry>,
    },
}
impl From<MenuItem> for MenuEntry {
    fn from(item: MenuItem) -> Self {
        Self::Item(item)
    }
}
impl MenuEntry {
    /// A submenu row.
    pub fn submenu(label: impl Into<String>, entries: Vec<MenuEntry>) -> Self {
        Self::Submenu {
            label: label.into(),
            entries,
        }
    }
}

/// Where [`open_menu`] places the menu.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MenuAt {
    /// At a window point, usually [`SecondaryPress::at`].
    Point([f32; 2]),
    /// Below the anchor, at least as wide as it, as for a menu button.
    Below,
}

/// The top-level popup of an open menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MenuHandle {
    /// Popup root; also the `menu` of [`MenuPicked`] and [`MenuDismissed`].
    pub root: Entity,
}
impl MenuHandle {
    /// Whether the menu is still open.
    pub fn is_open(self, world: &World) -> bool {
        world.is_alive(self.root)
            && !world.has::<Hidden>(self.root)
            && world.get::<MenuRoot>(self.root).is_some_and(|m| !m.retired)
    }
    /// Close the menu and its submenus, returning focus to the anchor.
    /// Sends [`MenuDismissed`].
    pub fn close(self, world: &mut World) {
        if self.is_open(world) {
            close_popup(world, self.root, true);
        }
    }
    /// The row of item `id`, in this menu or a submenu, for tests and hosts.
    pub fn item(self, world: &World, id: u64) -> Option<Entity> {
        world.entities_with::<MenuRow>().into_iter().find(|e| {
            let row = world.get::<MenuRow>(*e).unwrap();
            row.item == Some(id)
                && world.get::<MenuRoot>(row.menu).map(|m| m.top) == Some(self.root)
        })
    }
    /// The row that opens the submenu labelled `label`.
    pub fn submenu_row(self, world: &World, label: &str) -> Option<Entity> {
        world.entities_with::<MenuRow>().into_iter().find(|e| {
            let row = world.get::<MenuRow>(*e).unwrap();
            row.submenu.is_some()
                && world.get::<MenuRoot>(row.menu).map(|m| m.top) == Some(self.root)
                && world.get::<Semantics>(*e).and_then(|s| s.name.as_deref()) == Some(label)
        })
    }
}

/// An item was picked. Sent during Input, readable in Update.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MenuPicked {
    /// [`MenuHandle::root`] of the menu.
    pub menu: Entity,
    /// [`MenuItem::id`] of the picked item.
    pub item: u64,
}
impl Event for MenuPicked {}

/// The menu closed without a pick: Escape, an outside click, Tab, blur, or
/// [`MenuHandle::close`]. Also sent when another popup replaces it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MenuDismissed {
    /// [`MenuHandle::root`] of the menu.
    pub menu: Entity,
}
impl Event for MenuDismissed {}

#[derive(Clone, Copy)]
struct MenuRoot {
    top: Entity,
    picked: bool,
    retired: bool,
}
impl Component for MenuRoot {}

#[derive(Clone)]
struct MenuRow {
    menu: Entity,
    item: Option<u64>,
    submenu: Option<Entity>,
    danger: bool,
    texts: Vec<Entity>,
    shortcut: Option<Entity>,
}
impl Component for MenuRow {}

#[derive(Default)]
struct HoverAt(Option<[f32; 2]>);

/// Open a menu anchored to `anchor`, replacing any open popup. Focus moves to
/// the first enabled item and returns to `anchor` on close. Fails with
/// [`UiError::InvalidProps`] for blank labels, empty submenus or more than
/// [`MAX_ENTRIES`] entries, and [`UiError::Stale`] when `anchor` is gone,
/// disabled or blocked by a modal.
pub fn open_menu(
    world: &mut World,
    anchor: Entity,
    at: MenuAt,
    entries: &[MenuEntry],
) -> Result<MenuHandle, UiError> {
    if !world.is_alive(anchor) {
        return Err(UiError::Stale);
    }
    let mut count = 0;
    if entries.is_empty() || !valid(entries, &mut count) || count > MAX_ENTRIES {
        return Err(UiError::InvalidProps);
    }
    let placement = match at {
        MenuAt::Point(p) => PopupPlacement::At(p),
        MenuAt::Below => PopupPlacement::Below,
    };
    let root = popup(world, anchor, placement, None);
    build(world, root, root, entries)?;
    if !open_popup(world, root) {
        retire(world, root);
        return Err(UiError::Stale);
    }
    focus_first(world, root, false);
    let at = world.resource::<Input>().map(|i| i.pointer.position);
    world.insert_resource(HoverAt(at));
    world.insert_resource(MotionWatch { active: true });
    Ok(MenuHandle { root })
}

fn valid(entries: &[MenuEntry], count: &mut usize) -> bool {
    entries.iter().all(|entry| {
        *count += 1;
        match entry {
            MenuEntry::Item(item) => !item.label.trim().is_empty(),
            MenuEntry::Separator => true,
            MenuEntry::Group(label) => !label.trim().is_empty(),
            MenuEntry::Submenu { label, entries } => {
                !label.trim().is_empty() && !entries.is_empty() && valid(entries, count)
            }
        }
    })
}

fn popup(
    world: &mut World,
    anchor: Entity,
    placement: PopupPlacement,
    top: Option<Entity>,
) -> Entity {
    let p = world.spawn((Layout {
        width: Size::Hug,
        height: Size::Hug,
        padding: 4.0,
        min_width: 180.0,
        ..Layout::column()
    },));
    world.insert(p, Scroll::default());
    world.insert(p, Hidden);
    world.insert(p, Focusable);
    world.insert(p, TabStop(false));
    world.insert(p, KeyNavigation(navigate));
    world.insert(
        p,
        AnchoredPopup {
            anchor,
            max_height: 480.0,
            closed,
        },
    );
    world.insert(p, placement);
    world.insert(p, FocusOwner(anchor));
    world.insert(
        p,
        UiStyle {
            background: Some(Token::Overlay.into()),
            corner: Some(Corner::Compact),
            ..UiStyle::default()
        },
    );
    world.insert(
        p,
        MenuRoot {
            top: top.unwrap_or(p),
            picked: false,
            retired: false,
        },
    );
    p
}

fn build(
    world: &mut World,
    top: Entity,
    menu: Entity,
    entries: &[MenuEntry],
) -> Result<(), UiError> {
    for entry in entries {
        match entry {
            MenuEntry::Item(item) => {
                let r = row(world, menu, &item.label, item.enabled)?;
                let mark = world.spawn((Layout {
                    width: Size::Px(14.0),
                    ..Layout::column()
                },));
                set_parent(world, mark, r);
                world.get_mut::<Children>(r).unwrap().list.rotate_right(1);
                let mut texts = world.get::<MenuRow>(r).unwrap().texts.clone();
                if item.checked == Some(true) {
                    texts.push(hug(world, mark, "✓", Typography::Label)?);
                }
                let shortcut = match &item.shortcut {
                    Some(s) => {
                        spacer(world, r);
                        Some(hug(world, r, s, Typography::Secondary)?)
                    }
                    None => None,
                };
                let row = world.get_mut::<MenuRow>(r).unwrap();
                row.item = Some(item.id);
                row.danger = item.danger;
                row.texts = texts;
                row.shortcut = shortcut;
                if let Some(s) = &item.shortcut {
                    world.get_mut::<Semantics>(r).unwrap().description = s.clone();
                }
            }
            MenuEntry::Separator => {
                let gap = world.spawn((Layout {
                    width: Size::Fill,
                    padding: 4.0,
                    ..Layout::column()
                },));
                set_parent(world, gap, menu);
                separator(world, gap)?;
            }
            MenuEntry::Group(label) => {
                let t = hug(world, menu, label, Typography::Secondary)?;
                world.get_mut::<Layout>(t).unwrap().padding = 6.0;
            }
            MenuEntry::Submenu { label, entries } => {
                let r = row(world, menu, label, true)?;
                let mark = world.spawn((Layout {
                    width: Size::Px(14.0),
                    ..Layout::column()
                },));
                set_parent(world, mark, r);
                world.get_mut::<Children>(r).unwrap().list.rotate_right(1);
                spacer(world, r);
                let arrow = hug(world, r, "›", Typography::Label)?;
                let sub = popup(world, r, PopupPlacement::Beside, Some(top));
                build(world, top, sub, entries)?;
                let row = world.get_mut::<MenuRow>(r).unwrap();
                row.submenu = Some(sub);
                row.texts.push(arrow);
            }
        }
    }
    Ok(())
}

fn row(world: &mut World, menu: Entity, label: &str, enabled: bool) -> Result<Entity, UiError> {
    let r = world.spawn((Layout {
        width: Size::Fill,
        padding: 6.0,
        gap: 8.0,
        ..Layout::row()
    },));
    set_parent(world, r, menu);
    world.insert(r, Focusable);
    world.insert(r, TabStop(false));
    world.insert(
        r,
        Button {
            action: "menu".into(),
            enabled,
        },
    );
    world.insert(r, ReleaseActivation);
    world.insert(r, ActivationHandler(activate));
    world.insert(r, KeyNavigation(navigate));
    world.insert(
        r,
        Semantics {
            name: Some(label.to_string()),
            ..Semantics::default()
        },
    );
    let t = hug(world, r, label, Typography::Label)?;
    world.insert(
        r,
        MenuRow {
            menu,
            item: None,
            submenu: None,
            danger: false,
            texts: vec![t],
            shortcut: None,
        },
    );
    Ok(r)
}

fn hug(
    world: &mut World,
    parent: Entity,
    value: &str,
    role: Typography,
) -> Result<Entity, UiError> {
    let t = text(world, parent, value, role)?;
    world.get_mut::<Layout>(t).unwrap().width = Size::Hug;
    Ok(t)
}

fn spacer(world: &mut World, parent: Entity) {
    let s = world.spawn((Layout {
        width: Size::Fill,
        height: Size::Px(1.0),
        ..Layout::column()
    },));
    set_parent(world, s, parent);
}

fn rows(world: &World, menu: Entity) -> Vec<Entity> {
    children_of(world, menu)
        .into_iter()
        .filter(|e| world.has::<MenuRow>(*e) && access_details(world, *e).focus)
        .collect()
}

fn focus_first(world: &mut World, menu: Entity, last: bool) {
    let rows = rows(world, menu);
    let target = if last { rows.last() } else { rows.first() };
    request_focus(world, target.copied().unwrap_or(menu));
}

fn open_submenu(world: &mut World, sub: Entity, focus: bool) {
    if world.has::<Hidden>(sub) && !open_popup(world, sub) {
        return;
    }
    if focus {
        focus_first(world, sub, false);
    }
}

fn activate(world: &mut World, row: Entity) {
    let Some(r) = world.get::<MenuRow>(row).cloned() else {
        return;
    };
    if let Some(sub) = r.submenu {
        open_submenu(world, sub, true);
        return;
    }
    let (Some(item), Some(top)) = (r.item, world.get::<MenuRoot>(r.menu).map(|m| m.top)) else {
        return;
    };
    if let Some(m) = world.get_mut::<MenuRoot>(top) {
        m.picked = true;
    }
    send(world, MenuPicked { menu: top, item });
    close_popup(world, top, true);
}

fn closed(world: &mut World, popup: Entity) {
    let Some(m) = world.get::<MenuRoot>(popup).copied() else {
        return;
    };
    if m.top != popup || m.retired {
        return;
    }
    if !m.picked {
        send(world, MenuDismissed { menu: popup });
    }
    world.get_mut::<MenuRoot>(popup).unwrap().retired = true;
}

fn navigate(world: &mut World, e: Entity, key: &KeyEvent) -> bool {
    if key.released || key.synthetic {
        return false;
    }
    let menu = match world.get::<MenuRow>(e) {
        Some(r) => r.menu,
        None if world.has::<MenuRoot>(e) => e,
        None => return false,
    };
    let LogicalKey::Named(k) = key.logical else {
        return false;
    };
    let rows = rows(world, menu);
    let at = rows.iter().position(|r| *r == e);
    let n = rows.len();
    let next = match k {
        NamedKey::ArrowDown => at.map_or(0, |i| (i + 1) % n.max(1)),
        NamedKey::ArrowUp => at.map_or(n.saturating_sub(1), |i| (i + n - 1) % n),
        NamedKey::Home => 0,
        NamedKey::End => n.saturating_sub(1),
        NamedKey::ArrowRight => {
            if let Some(sub) = world.get::<MenuRow>(e).and_then(|r| r.submenu) {
                if access_details(world, e).activate {
                    open_submenu(world, sub, true);
                }
            }
            return true;
        }
        NamedKey::ArrowLeft => {
            if world.get::<MenuRoot>(menu).is_some_and(|m| m.top != menu) {
                close_popup(world, menu, true);
            }
            return true;
        }
        _ => return false,
    };
    if let Some(row) = rows.get(next).copied() {
        request_focus(world, row);
        reveal_entity(world, row);
    }
    true
}

fn open_menus(world: &World) -> Vec<Entity> {
    world
        .entities_with::<MenuRoot>()
        .into_iter()
        .filter(|e| !world.has::<Hidden>(*e))
        .collect()
}

/// Input: pointer motion highlights the row under it and opens its submenu.
pub(crate) fn hover(world: &mut World) {
    if open_menus(world).is_empty() {
        return;
    }
    world.insert_resource(MotionWatch { active: true });
    let Some(pos) = world.resource::<Input>().map(|i| i.pointer.position) else {
        return;
    };
    if world
        .resource::<HoverAt>()
        .is_some_and(|h| h.0 == Some(pos))
    {
        return;
    }
    world.insert_resource(HoverAt(Some(pos)));
    let Some(row) = hit_test(world, pos[0], pos[1]).and_then(|hit| {
        if world.has::<MenuRow>(hit) {
            Some(hit)
        } else {
            ancestor_with::<MenuRow>(world, hit)
        }
    }) else {
        return;
    };
    let r = world.get::<MenuRow>(row).cloned().unwrap();
    if world.has::<Hidden>(r.menu) || !access_details(world, row).focus {
        return;
    }
    request_focus(world, row);
    for other in children_of(world, r.menu) {
        let sub = world.get::<MenuRow>(other).and_then(|o| o.submenu);
        if let Some(sub) = sub.filter(|s| Some(*s) != r.submenu && !world.has::<Hidden>(*s)) {
            close_popup(world, sub, false);
        }
    }
    if let Some(sub) = r.submenu {
        open_submenu(world, sub, false);
    }
}

fn retire(world: &mut World, top: Entity) {
    let mut roots: Vec<_> = world
        .entities_with::<MenuRoot>()
        .into_iter()
        .filter(|e| world.get::<MenuRoot>(*e).is_some_and(|m| m.top == top))
        .collect();
    roots.sort_by_key(|e| *e != top);
    let mut all = Vec::new();
    fn collect(w: &World, e: Entity, out: &mut Vec<Entity>) {
        out.push(e);
        for c in children_of(w, e) {
            collect(w, c, out);
        }
    }
    for root in &roots {
        collect(world, *root, &mut all);
    }
    forget_interaction_targets(world, &all);
    for root in roots.into_iter().rev() {
        despawn(world, root);
    }
}

/// Layout: despawn closed menus, then project highlight, danger and disabled styles.
pub(crate) fn styles(world: &mut World) {
    for e in world.entities_with::<MenuRoot>() {
        if world
            .get::<MenuRoot>(e)
            .is_some_and(|m| m.retired && m.top == e)
        {
            retire(world, e);
        }
    }
    let focus = world.resource::<Focus>().and_then(|f| f.entity);
    for e in world.entities_with::<MenuRow>() {
        let r = world.get::<MenuRow>(e).cloned().unwrap();
        let available = access_details(world, e).activate;
        let open_sub = r.submenu.is_some_and(|s| !world.has::<Hidden>(s));
        let lit = available && (focus == Some(e) || open_sub);
        world.insert(
            e,
            UiStyle {
                background: lit.then_some(Token::Accent.into()),
                corner: Some(Corner::Compact),
                ..UiStyle::default()
            },
        );
        let ink = if !available {
            Token::Disabled
        } else if lit {
            Token::OnAccent
        } else if r.danger {
            Token::DangerText
        } else {
            Token::Text
        };
        for t in r.texts.iter().copied().chain(r.shortcut) {
            let shortcut = Some(t) == r.shortcut;
            world.insert(
                t,
                UiStyle {
                    foreground: Some(
                        if shortcut && ink == Token::Text {
                            Token::Secondary
                        } else {
                            ink
                        }
                        .into(),
                    ),
                    typography: Some(if shortcut {
                        Typography::Secondary
                    } else {
                        Typography::Label
                    }),
                    ..UiStyle::default()
                },
            );
        }
    }
}
