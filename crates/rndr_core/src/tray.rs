//! Menu-bar and notification-area icon.
//!
//! The menu, the icon bytes, and the events live here. No windowing crate does.
//! The desktop runner turns a [`TrayConfig`] into a status item and pushes
//! [`TrayReport`]s back. Headless tests use [`FakeTray`] for that half.
//!
//! A click or a menu activation is queued from the platform callback, which
//! wakes the loop. [`crate::App::update`] maps the queue into [`TrayEvent`] and
//! [`TrayMenu`] before Input, so an Update system reads them on that frame,
//! including while the window is hidden.
//!
//! Shortcut text is display only. Pass [`crate::Keymap::shortcut`] or
//! [`crate::Keystroke::display`]. A chord can be more than one key, so it is
//! not registered as a menu accelerator.

use std::sync::{Mutex, MutexGuard};

use crate::event::{self, Event};
use crate::input::Input;
use crate::keys::Platform;
use crate::runtime;
use crate::window::{self, Availability};
use crate::world::World;

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|err| err.into_inner())
}

/// Id of a menu row. Assigned by [`Menu`] and stable across label updates.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TrayItemId(u32);

impl TrayItemId {
    pub fn as_u32(self) -> u32 {
        self.0
    }

    pub fn from_raw(id: u32) -> Self {
        Self(id)
    }
}

/// Why an icon was rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IconError {
    /// Width, height, or the buffer is empty.
    Empty,
    /// `rgba.len()` is not `width * height * 4`.
    Size,
    /// The buffer does not start with a PNG signature.
    Png,
}

/// RGBA pixels or a PNG file. The runner decodes PNG.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Image {
    Rgba {
        width: u32,
        height: u32,
        rgba: Vec<u8>,
    },
    Png(Vec<u8>),
}

/// Icon shown in the menu bar or notification area.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrayIcon {
    pub image: Image,
    /// macOS draws this as a template image so it follows the menu bar color.
    /// Other platforms ignore the flag. See [`platform_support`].
    pub template: bool,
}

impl TrayIcon {
    pub fn rgba(width: u32, height: u32, rgba: Vec<u8>) -> Result<Self, IconError> {
        if width == 0 || height == 0 || rgba.is_empty() {
            return Err(IconError::Empty);
        }
        let pixels = (width as usize)
            .checked_mul(height as usize)
            .and_then(|n| n.checked_mul(4));
        if pixels != Some(rgba.len()) {
            return Err(IconError::Size);
        }
        Ok(Self {
            image: Image::Rgba {
                width,
                height,
                rgba,
            },
            template: false,
        })
    }

    /// `bytes` must be a PNG file. The signature is checked here; the runner
    /// decodes the rest.
    pub fn png(bytes: Vec<u8>) -> Result<Self, IconError> {
        const SIG: &[u8] = b"\x89PNG\r\n\x1a\n";
        if bytes.len() < SIG.len() || !bytes.starts_with(SIG) {
            return Err(IconError::Png);
        }
        Ok(Self {
            image: Image::Png(bytes),
            template: false,
        })
    }

    pub fn template(mut self, template: bool) -> Self {
        self.template = template;
        self
    }
}

/// One row before it is inserted into a [`Menu`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    label: String,
    shortcut: String,
    enabled: bool,
    checked: bool,
    checkable: bool,
    quits: bool,
}

impl Item {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            shortcut: String::new(),
            enabled: true,
            checked: false,
            checkable: false,
            quits: false,
        }
    }

    /// Display text from [`crate::Keymap::shortcut`] or [`crate::Keystroke::display`].
    pub fn shortcut(mut self, text: impl Into<String>) -> Self {
        self.shortcut = text.into();
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// A check row. `false` still reserves the check mark so it can be toggled later.
    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self.checkable = true;
        self
    }

    /// Activating this row sets [`Input::quit`]. The runner exits, including
    /// when the window is hidden.
    pub fn quits(mut self) -> Self {
        self.quits = true;
        self
    }
}

/// A row that already has an id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuItem {
    pub id: TrayItemId,
    pub label: String,
    pub shortcut: String,
    pub enabled: bool,
    pub checked: bool,
    pub checkable: bool,
    pub quits: bool,
}

/// A nested menu. Its id is for the submenu row itself (label and enabled).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Submenu {
    pub id: TrayItemId,
    pub label: String,
    pub enabled: bool,
    pub menu: Menu,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Entry {
    Item(MenuItem),
    Separator,
    Submenu(Submenu),
}

/// The tray menu. Ids are unique across nested submenus.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Menu {
    next: u32,
    entries: Vec<Entry>,
}

impl Default for Menu {
    fn default() -> Self {
        Self::new()
    }
}

impl Menu {
    pub fn new() -> Self {
        Self {
            next: 1,
            entries: Vec::new(),
        }
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn add(&mut self, item: Item) -> TrayItemId {
        let id = self.alloc();
        self.entries.push(Entry::Item(MenuItem {
            id,
            label: item.label,
            shortcut: item.shortcut,
            enabled: item.enabled,
            checked: item.checked,
            checkable: item.checkable,
            quits: item.quits,
        }));
        id
    }

    pub fn separator(&mut self) {
        self.entries.push(Entry::Separator);
    }

    /// `build` adds children. Their ids continue this menu's counter.
    pub fn submenu(
        &mut self,
        label: impl Into<String>,
        build: impl FnOnce(&mut Menu),
    ) -> TrayItemId {
        let id = self.alloc();
        let mut child = Menu {
            next: self.next,
            entries: Vec::new(),
        };
        build(&mut child);
        self.next = child.next;
        self.entries.push(Entry::Submenu(Submenu {
            id,
            label: label.into(),
            enabled: true,
            menu: child,
        }));
        id
    }

    pub fn item(&self, id: TrayItemId) -> Option<&MenuItem> {
        find_item(&self.entries, id)
    }

    pub fn submenu_row(&self, id: TrayItemId) -> Option<&Submenu> {
        find_sub(&self.entries, id)
    }

    pub fn set_label(&mut self, id: TrayItemId, label: impl Into<String>) -> bool {
        let label = label.into();
        if let Some(item) = find_item_mut(&mut self.entries, id) {
            item.label = label;
            return true;
        }
        if let Some(sub) = find_sub_mut(&mut self.entries, id) {
            sub.label = label;
            return true;
        }
        false
    }

    pub fn set_enabled(&mut self, id: TrayItemId, enabled: bool) -> bool {
        if let Some(item) = find_item_mut(&mut self.entries, id) {
            item.enabled = enabled;
            return true;
        }
        if let Some(sub) = find_sub_mut(&mut self.entries, id) {
            sub.enabled = enabled;
            return true;
        }
        false
    }

    /// Turns the row into a check item when it was not one yet.
    pub fn set_checked(&mut self, id: TrayItemId, checked: bool) -> bool {
        let Some(item) = find_item_mut(&mut self.entries, id) else {
            return false;
        };
        item.checkable = true;
        item.checked = checked;
        true
    }

    pub fn quits(&self, id: TrayItemId) -> bool {
        self.item(id).is_some_and(|item| item.quits)
    }

    fn alloc(&mut self) -> TrayItemId {
        let id = TrayItemId(self.next);
        self.next = self.next.saturating_add(1);
        id
    }
}

fn find_item(entries: &[Entry], id: TrayItemId) -> Option<&MenuItem> {
    for entry in entries {
        match entry {
            Entry::Item(item) if item.id == id => return Some(item),
            Entry::Submenu(sub) => {
                if let Some(item) = find_item(sub.menu.entries(), id) {
                    return Some(item);
                }
            }
            _ => {}
        }
    }
    None
}

fn find_item_mut(entries: &mut [Entry], id: TrayItemId) -> Option<&mut MenuItem> {
    for entry in entries {
        match entry {
            Entry::Item(item) if item.id == id => return Some(item),
            Entry::Submenu(sub) => {
                if let Some(item) = find_item_mut(&mut sub.menu.entries, id) {
                    return Some(item);
                }
            }
            _ => {}
        }
    }
    None
}

fn find_sub(entries: &[Entry], id: TrayItemId) -> Option<&Submenu> {
    for entry in entries {
        if let Entry::Submenu(sub) = entry {
            if sub.id == id {
                return Some(sub);
            }
            if let Some(found) = find_sub(sub.menu.entries(), id) {
                return Some(found);
            }
        }
    }
    None
}

fn find_sub_mut(entries: &mut [Entry], id: TrayItemId) -> Option<&mut Submenu> {
    for entry in entries {
        if let Entry::Submenu(sub) = entry {
            if sub.id == id {
                return Some(sub);
            }
            if let Some(found) = find_sub_mut(&mut sub.menu.entries, id) {
                return Some(found);
            }
        }
    }
    None
}

/// Label plus shortcut, for the native row. A tab separates them so the
/// shortcut sits in the accelerator column where the platform honors it.
pub fn displayed_label(label: &str, shortcut: &str) -> String {
    if shortcut.is_empty() {
        label.to_string()
    } else {
        format!("{label}\t{shortcut}")
    }
}

/// What the app asks the runner to show.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrayConfig {
    pub icon: TrayIcon,
    pub tooltip: String,
    pub menu: Menu,
    /// The window starts hidden and the process keeps running. Quit still exits.
    pub hide_window: bool,
}

impl TrayConfig {
    pub fn new(icon: TrayIcon, menu: Menu) -> Self {
        Self {
            icon,
            tooltip: String::new(),
            menu,
            hide_window: false,
        }
    }

    pub fn tooltip(mut self, text: impl Into<String>) -> Self {
        self.tooltip = text.into();
        self
    }

    pub fn hide_window(mut self, hide: bool) -> Self {
        self.hide_window = hide;
        self
    }
}

/// What the runner should do. The menu resource is the source of truth;
/// the command says which part changed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TrayCommand {
    Create(TrayConfig),
    SetIcon(TrayIcon),
    SetTooltip(String),
    SetMenu(Menu),
}

/// A report from the platform, before it becomes an [`Event`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrayReport {
    /// Left click on the icon.
    Click,
    /// Second click. Windows emits this. macOS and StatusNotifierItem do not.
    DoubleClick,
    Activated(TrayItemId),
    /// The pointer moved onto the icon. [`map_report`] drops it.
    Hover,
}

/// Left click or double click on the icon.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrayEvent {
    Click,
    DoubleClick,
}

impl Event for TrayEvent {}

/// A menu row was chosen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrayMenu {
    Activated(TrayItemId),
}

impl Event for TrayMenu {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrayMapped {
    Icon(TrayEvent),
    Menu(TrayMenu),
}

/// Map one backend report. [`TrayReport::Hover`] is not an app event.
pub fn map_report(report: TrayReport) -> Option<TrayMapped> {
    Some(match report {
        TrayReport::Click => TrayMapped::Icon(TrayEvent::Click),
        TrayReport::DoubleClick => TrayMapped::Icon(TrayEvent::DoubleClick),
        TrayReport::Activated(id) => TrayMapped::Menu(TrayMenu::Activated(id)),
        TrayReport::Hover => return None,
    })
}

/// Per-platform support. X11 and Wayland share one StatusNotifierItem backend.
/// The icon shows when a watcher is running on the session bus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TraySupport {
    pub icon: Availability,
    pub template: Availability,
    pub tooltip: Availability,
    pub menu: Availability,
    pub click: Availability,
    pub double_click: Availability,
}

pub fn platform_support(platform: Platform) -> TraySupport {
    let supported = TraySupport {
        icon: Availability::Supported,
        template: Availability::Ignored,
        tooltip: Availability::Supported,
        menu: Availability::Supported,
        click: Availability::Supported,
        double_click: Availability::Ignored,
    };
    match platform {
        Platform::Mac => TraySupport {
            template: Availability::Supported,
            ..supported
        },
        Platform::Windows => TraySupport {
            double_click: Availability::Supported,
            ..supported
        },
        Platform::X11 | Platform::Wayland => supported,
    }
}

/// Snapshot after a frame. `running` is false once a quit item (or anything
/// else) has set [`Input::quit`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TrayPhase {
    pub installed: bool,
    pub window_visible: bool,
    pub running: bool,
}

#[derive(Default)]
struct TrayState {
    installed: bool,
    started: bool,
    commands: Vec<TrayCommand>,
    inbox: Vec<TrayReport>,
}

pub(crate) fn install(world: &mut World) {
    world.insert_resource(TrayState::default());
}

/// The config asks to start hidden. The runner reads this before creating the window.
pub fn starts_hidden(world: &World) -> bool {
    world
        .resource::<TrayConfig>()
        .is_some_and(|config| config.hide_window)
}

/// Mark the window hidden without a platform effect. Used when the window is
/// created already hidden, so the first frame does not flash it.
pub fn mark_hidden(world: &mut World) {
    if let Some(phase) = world.resource_mut::<window::WindowPhase>() {
        phase.visible = false;
    }
}

pub fn phase(world: &World) -> TrayPhase {
    TrayPhase {
        installed: world
            .resource::<TrayState>()
            .is_some_and(|state| state.installed),
        window_visible: window::is_visible(world),
        running: !world.resource::<Input>().is_some_and(|input| input.quit),
    }
}

pub fn pending(world: &World) -> bool {
    world
        .resource::<TrayState>()
        .is_some_and(|state| !state.inbox.is_empty())
}

/// Queue a platform report and wake the loop. Safe while the window is hidden.
pub fn push_report(world: &mut World, report: TrayReport) {
    if let Some(state) = world.resource_mut::<TrayState>() {
        state.inbox.push(report);
    }
    wake(world);
}

pub fn drain_commands(world: &mut World) -> Vec<TrayCommand> {
    world
        .resource_mut::<TrayState>()
        .map(|state| std::mem::take(&mut state.commands))
        .unwrap_or_default()
}

fn wake(world: &World) {
    let slot = runtime::wake_slot(world);
    lock(&slot).wake();
}

fn queue(world: &mut World, command: TrayCommand) {
    if let Some(state) = world.resource_mut::<TrayState>() {
        state.commands.push(command);
    }
    wake(world);
}

/// After Startup systems. Inserting a [`TrayConfig`] there is enough.
pub(crate) fn startup(world: &mut World) {
    let Some(config) = world.resource::<TrayConfig>().cloned() else {
        if let Some(state) = world.resource_mut::<TrayState>() {
            state.started = true;
        }
        return;
    };
    let hide = config.hide_window;
    if let Some(state) = world.resource_mut::<TrayState>() {
        state.started = true;
        if !state.installed {
            state.installed = true;
            state.commands.push(TrayCommand::Create(config));
        }
    }
    if hide && window::is_visible(world) {
        window::hide(world);
    }
}

pub(crate) fn drive(world: &mut World) {
    let reports = world
        .resource_mut::<TrayState>()
        .map(|state| std::mem::take(&mut state.inbox))
        .unwrap_or_default();
    let mut quit_ids = Vec::new();
    for report in reports {
        match map_report(report) {
            Some(TrayMapped::Icon(event)) => event::send(world, event),
            Some(TrayMapped::Menu(TrayMenu::Activated(id))) => {
                if world
                    .resource::<TrayConfig>()
                    .is_some_and(|config| config.menu.quits(id))
                {
                    quit_ids.push(id);
                }
                event::send(world, TrayMenu::Activated(id));
            }
            None => {}
        }
    }
    if !quit_ids.is_empty() {
        if let Some(input) = world.resource_mut::<Input>() {
            input.quit = true;
        }
    }
}

fn mutate_menu(world: &mut World, change: impl FnOnce(&mut Menu) -> bool) -> bool {
    let menu = {
        let Some(config) = world.resource_mut::<TrayConfig>() else {
            return false;
        };
        if !change(&mut config.menu) {
            return false;
        }
        config.menu.clone()
    };
    queue(world, TrayCommand::SetMenu(menu));
    true
}

pub fn set_label(world: &mut World, id: TrayItemId, label: impl Into<String>) -> bool {
    let label = label.into();
    mutate_menu(world, |menu| menu.set_label(id, label))
}

pub fn set_enabled(world: &mut World, id: TrayItemId, enabled: bool) -> bool {
    mutate_menu(world, |menu| menu.set_enabled(id, enabled))
}

pub fn set_checked(world: &mut World, id: TrayItemId, checked: bool) -> bool {
    mutate_menu(world, |menu| menu.set_checked(id, checked))
}

pub fn set_icon(world: &mut World, icon: TrayIcon) -> bool {
    {
        let Some(config) = world.resource_mut::<TrayConfig>() else {
            return false;
        };
        config.icon = icon.clone();
    }
    queue(world, TrayCommand::SetIcon(icon));
    true
}

pub fn set_tooltip(world: &mut World, tooltip: impl Into<String>) -> bool {
    let tooltip = tooltip.into();
    {
        let Some(config) = world.resource_mut::<TrayConfig>() else {
            return false;
        };
        config.tooltip.clone_from(&tooltip);
    }
    queue(world, TrayCommand::SetTooltip(tooltip));
    true
}

pub fn set_menu(world: &mut World, menu: Menu) -> bool {
    {
        let Some(config) = world.resource_mut::<TrayConfig>() else {
            return false;
        };
        config.menu = menu.clone();
    }
    queue(world, TrayCommand::SetMenu(menu));
    true
}

/// On [`TrayEvent::Click`], queue [`window::toggle`].
///
/// Put it on Update. The click is already readable there. One click toggles
/// once; the runner drops the matching mouse-up so a press is not two events.
pub fn toggle_on_click(world: &mut World) {
    let click = event::events::<TrayEvent>(world)
        .iter()
        .any(|event| matches!(event, TrayEvent::Click));
    if click {
        window::toggle(world);
    }
}

/// In-memory backend. Records commands and feeds reports through [`push_report`].
#[derive(Clone, Debug, Default)]
pub struct FakeTray {
    pub commands: Vec<TrayCommand>,
    reports: Vec<TrayReport>,
}

impl FakeTray {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn emit(&mut self, report: TrayReport) {
        self.reports.push(report);
    }

    /// Take queued commands, then deliver queued reports into the app inbox.
    pub fn sync(&mut self, world: &mut World) {
        self.commands.extend(drain_commands(world));
        for report in self.reports.drain(..) {
            push_report(world, report);
        }
    }
}

/// Id string the runner puts on a native menu row.
pub fn menu_id(id: TrayItemId) -> String {
    format!("rndr-tray-{}", id.as_u32())
}

pub fn parse_menu_id(text: &str) -> Option<TrayItemId> {
    let raw = text.strip_prefix("rndr-tray-")?.parse().ok()?;
    Some(TrayItemId(raw))
}
