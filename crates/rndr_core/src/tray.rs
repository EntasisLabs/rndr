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
//! Shortcut text on an item is the chord shown as a menu accelerator when the
//! tray can parse it (`Ctrl+K`, `Alt+Space`, `Cmd+Space`). A symbol string such
//! as `⌘K` stays in the label on macOS and Windows and is omitted on Linux.

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
    /// The raw id [`Menu`] assigned, or the value passed to [`Self::from_raw`].
    pub fn as_u32(self) -> u32 {
        self.0
    }

    /// Rebuild an id without checking that a [`Menu`] issued it.
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
    /// Unpacked RGBA8 pixels, four bytes per pixel in row order.
    Rgba {
        /// Width in pixels. [`TrayIcon::rgba`] rejects zero.
        width: u32,
        /// Height in pixels. [`TrayIcon::rgba`] rejects zero.
        height: u32,
        /// Channel bytes. The length is `width * height * 4`.
        rgba: Vec<u8>,
    },
    /// PNG file bytes. [`TrayIcon::png`] checks the signature only.
    Png(Vec<u8>),
}

/// Icon shown in the menu bar or notification area.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrayIcon {
    /// Pixels the status item draws.
    pub image: Image,
    /// macOS draws this as a template image so it follows the menu bar color.
    /// Other platforms ignore the flag. See [`platform_support`].
    pub template: bool,
}

impl TrayIcon {
    /// An icon from RGBA8 pixels, with the template flag clear.
    ///
    /// Returns [`IconError::Empty`] when `width`, `height`, or `rgba` is
    /// empty. Returns [`IconError::Size`] when `rgba.len()` is not
    /// `width * height * 4`, including when that product overflows.
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

    /// Set the macOS template flag. Other platforms ignore it.
    /// See [`platform_support`].
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
    /// An enabled row with this label, no shortcut, and no check mark.
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

    /// Chord for the menu accelerator. [`crate::Keystroke::label`] parses
    /// (`Alt+Space`, `Cmd+K`). A symbol string is shown in the label where the
    /// tray has no accelerator column.
    pub fn shortcut(mut self, text: impl Into<String>) -> Self {
        self.shortcut = text.into();
        self
    }

    /// Grey the row out when `enabled` is false. A new item starts enabled.
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
    /// Id from [`Menu::add`]. It stays put when the label or flags change.
    pub id: TrayItemId,
    /// Row text, without the shortcut.
    pub label: String,
    /// Accelerator text. Empty when the row has none. See [`Item::shortcut`].
    pub shortcut: String,
    /// False when the row is greyed out.
    pub enabled: bool,
    /// Whether the check mark is on. Meaningful once [`Self::checkable`] is set.
    pub checked: bool,
    /// True after [`Item::checked`] or [`Menu::set_checked`] made this a check row.
    pub checkable: bool,
    /// True when activating the row sets [`Input::quit`]. See [`Item::quits`].
    pub quits: bool,
}

/// A nested menu. Its id is for the submenu row itself (label and enabled).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Submenu {
    /// Id of this submenu's own row. Child rows have their own ids.
    pub id: TrayItemId,
    /// Text on the row that opens the nested menu.
    pub label: String,
    /// False when that row is greyed out. Children keep their own flags.
    pub enabled: bool,
    /// Nested rows. Their ids continue the parent [`Menu`]'s counter.
    pub menu: Menu,
}

/// One slot in a [`Menu`]: a command row, a divider, or a nested menu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Entry {
    /// A command row. [`Menu::item`] finds these.
    Item(MenuItem),
    /// A divider. It has no id, and [`Menu::item`] skips it.
    Separator,
    /// A nested menu. [`Menu::submenu_row`] finds these.
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
    /// An empty menu. The next [`Self::add`] or [`Self::submenu`] gets id 1.
    pub fn new() -> Self {
        Self {
            next: 1,
            entries: Vec::new(),
        }
    }

    /// Rows in insertion order. A nested menu is one [`Entry::Submenu`].
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// Append `item` and return its id.
    ///
    /// Ids start at 1 and stay unique across this menu and every submenu
    /// built from it.
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

    /// Append a divider. The divider has no id.
    pub fn separator(&mut self) {
        self.entries.push(Entry::Separator);
    }

    /// `build` adds children. Their ids continue this menu's counter.
    ///
    /// Returns the submenu row's id. That row starts enabled. The id is
    /// taken before `build` runs, so a panic inside `build` skips the id
    /// and leaves the menu without that row.
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

    /// The command row `id`, searching nested menus.
    ///
    /// Returns `None` for a submenu row, a separator, or an unknown id.
    pub fn item(&self, id: TrayItemId) -> Option<&MenuItem> {
        find_item(&self.entries, id)
    }

    /// The submenu whose own row is `id`, searching nested menus.
    ///
    /// Returns `None` for a command row.
    pub fn submenu_row(&self, id: TrayItemId) -> Option<&Submenu> {
        find_sub(&self.entries, id)
    }

    /// Replace the label of the command or submenu row `id`.
    ///
    /// Returns false when `id` is not one of those rows.
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

    /// Enable or disable the command or submenu row `id`.
    ///
    /// Returns false when `id` is not one of those rows. Children of a
    /// submenu keep the flags they already have.
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
    ///
    /// Returns false when `id` is not a command row. A submenu id is left
    /// unchanged.
    pub fn set_checked(&mut self, id: TrayItemId, checked: bool) -> bool {
        let Some(item) = find_item_mut(&mut self.entries, id) else {
            return false;
        };
        item.checkable = true;
        item.checked = checked;
        true
    }

    /// True when the command row `id` was built with [`Item::quits`].
    ///
    /// A submenu id and an unknown id return false.
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

/// Label for a native menu row.
///
/// X11 and Wayland draw the whole label as one string, so a tab would show
/// up literally. Those platforms leave the shortcut out of the label; a
/// parseable chord is passed separately as the item accelerator. Other
/// platforms keep a tab before a shortcut the accelerator parser rejects.
pub fn menu_label(label: &str, shortcut: &str, platform: Platform) -> String {
    if shortcut.is_empty() || matches!(platform, Platform::X11 | Platform::Wayland) {
        label.to_string()
    } else {
        displayed_label(label, shortcut)
    }
}

/// What the app asks the runner to show.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrayConfig {
    /// Icon created with the status item.
    pub icon: TrayIcon,
    /// Hover text. Empty leaves the tooltip unset.
    pub tooltip: String,
    /// Menu opened from the icon.
    pub menu: Menu,
    /// The window starts hidden and the process keeps running. Quit still exits.
    pub hide_window: bool,
    /// Status-item id. Empty uses `rndr` when the icon is created.
    pub id: String,
    /// Title for trays that show one. Empty leaves the title unset.
    pub title: String,
}

impl TrayConfig {
    /// Icon and menu, tooltip and title empty, id `rndr`, window left visible.
    pub fn new(icon: TrayIcon, menu: Menu) -> Self {
        Self {
            icon,
            tooltip: String::new(),
            menu,
            hide_window: false,
            id: "rndr".to_string(),
            title: String::new(),
        }
    }

    /// Hover text. An empty string leaves the tooltip unset.
    pub fn tooltip(mut self, text: impl Into<String>) -> Self {
        self.tooltip = text.into();
        self
    }

    /// Start the window hidden when `hide` is true. The process keeps running.
    pub fn hide_window(mut self, hide: bool) -> Self {
        self.hide_window = hide;
        self
    }

    /// Status-item id. An empty string uses `rndr` when the icon is created.
    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }

    /// Title for trays that show one. An empty string leaves the title unset.
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }
}

/// What the runner should do. The menu resource is the source of truth;
/// the command says which part changed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TrayCommand {
    /// Create the status item. Startup queues this once when a
    /// [`TrayConfig`] is present.
    Create(TrayConfig),
    /// Replace the icon. The tooltip and menu stay.
    SetIcon(TrayIcon),
    /// Replace the hover text.
    SetTooltip(String),
    /// Replace the whole menu, ids included.
    SetMenu(Menu),
}

/// A report from the platform, before it becomes an [`Event`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrayReport {
    /// Left click on the icon.
    Click,
    /// Second click. Windows emits this. macOS and StatusNotifierItem do not.
    DoubleClick,
    /// The user chose the menu row `id`.
    Activated(TrayItemId),
    /// The pointer moved onto the icon. [`map_report`] drops it.
    Hover,
}

/// Left click or double click on the icon.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrayEvent {
    /// A primary click on the icon.
    Click,
    /// A second click. Windows emits this. macOS and StatusNotifierItem do not.
    DoubleClick,
}

impl Event for TrayEvent {}

/// A menu row was chosen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrayMenu {
    /// The command row the user chose.
    Activated(TrayItemId),
}

impl Event for TrayMenu {}

/// An app-facing event from [`map_report`]. Hover is dropped before this.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrayMapped {
    /// An icon click, as a [`TrayEvent`].
    Icon(TrayEvent),
    /// A chosen menu row, as a [`TrayMenu`].
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
    /// A status icon can be shown.
    pub icon: Availability,
    /// macOS template images. [`Availability::Ignored`] on other platforms.
    pub template: Availability,
    /// Hover text.
    pub tooltip: Availability,
    /// A menu on the icon.
    pub menu: Availability,
    /// A primary click on the icon.
    pub click: Availability,
    /// A second click. Supported on Windows. Ignored on macOS, X11, and Wayland.
    pub double_click: Availability,
}

/// Support for `platform`.
///
/// Icon, tooltip, menu, and click are supported everywhere. Template images
/// are honored on macOS and ignored elsewhere. Double-click is honored on
/// Windows and ignored elsewhere. On X11 and Wayland the icon still needs a
/// StatusNotifierItem watcher on the session bus.
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
    /// True after startup queued [`TrayCommand::Create`] for a [`TrayConfig`].
    pub installed: bool,
    /// [`window::is_visible`] when [`phase`] was called.
    pub window_visible: bool,
    /// False once [`Input::quit`] is set. A quit menu row does that.
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

/// Installed flag, window visibility, and whether the process is still running.
///
/// `installed` is true once startup queued a create for a [`TrayConfig`].
/// `window_visible` is [`window::is_visible`], the applied phase, so a
/// [`window::hide`] that is still queued leaves this true. `running` is
/// false when [`Input::quit`] is set. It stays true when [`Input`] is absent.
pub fn phase(world: &World) -> TrayPhase {
    TrayPhase {
        installed: world
            .resource::<TrayState>()
            .is_some_and(|state| state.installed),
        window_visible: window::is_visible(world),
        running: !world.resource::<Input>().is_some_and(|input| input.quit),
    }
}

/// True when [`push_report`] has queued a report the frame has not mapped.
///
/// False when the tray state is absent or the inbox is empty.
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

/// Take the queued [`TrayCommand`]s and leave the queue empty.
///
/// Returns an empty vec when the tray state was never installed, or when
/// nothing has been queued since the last drain. The runner performs these
/// after the frame.
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

/// Set row `id`'s label on the [`TrayConfig`] and queue a menu update.
///
/// Returns false when that resource is absent or `id` is not a command or
/// submenu row. On success the full menu is queued as [`TrayCommand::SetMenu`]
/// and the loop is woken. That wake panics when the runtime is missing;
/// [`crate::App`] installs it.
pub fn set_label(world: &mut World, id: TrayItemId, label: impl Into<String>) -> bool {
    let label = label.into();
    mutate_menu(world, |menu| menu.set_label(id, label))
}

/// Enable or disable row `id` on the [`TrayConfig`] and queue a menu update.
///
/// Returns false when that resource is absent or `id` is not a command or
/// submenu row. On success the loop is woken. That wake panics when the
/// runtime is missing; [`crate::App`] installs it.
pub fn set_enabled(world: &mut World, id: TrayItemId, enabled: bool) -> bool {
    mutate_menu(world, |menu| menu.set_enabled(id, enabled))
}

/// Check or uncheck the command row `id` and queue a menu update.
///
/// The row becomes a check item when it was not one yet, matching
/// [`Menu::set_checked`]. Returns false when no [`TrayConfig`] is present or
/// `id` is not a command row. On success the loop is woken. That wake panics
/// when the runtime is missing; [`crate::App`] installs it.
pub fn set_checked(world: &mut World, id: TrayItemId, checked: bool) -> bool {
    mutate_menu(world, |menu| menu.set_checked(id, checked))
}

/// Replace the icon on the [`TrayConfig`] and queue [`TrayCommand::SetIcon`].
///
/// Returns false when that resource is absent. On success the loop is woken.
/// That wake panics when the runtime is missing; [`crate::App`] installs it.
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

/// Replace the tooltip on the [`TrayConfig`] and queue [`TrayCommand::SetTooltip`].
///
/// An empty string clears the hover text. Returns false when that resource
/// is absent. On success the loop is woken. That wake panics when the
/// runtime is missing; [`crate::App`] installs it.
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

/// Replace the menu on the [`TrayConfig`] and queue [`TrayCommand::SetMenu`].
///
/// `menu` becomes the whole menu, ids included. Returns false when that
/// resource is absent. On success the loop is woken. That wake panics when
/// the runtime is missing; [`crate::App`] installs it.
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
    /// Commands taken by [`Self::sync`], oldest first.
    pub commands: Vec<TrayCommand>,
    reports: Vec<TrayReport>,
}

impl FakeTray {
    /// No commands collected and no reports waiting.
    pub fn new() -> Self {
        Self::default()
    }

    /// Queue `report` until the next [`Self::sync`]. The world is unchanged
    /// until then.
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

/// Parse a string from [`menu_id`].
///
/// Returns `None` when the `rndr-tray-` prefix is missing or the rest is not
/// a decimal `u32`. The id is not checked against a [`Menu`].
pub fn parse_menu_id(text: &str) -> Option<TrayItemId> {
    let raw = text.strip_prefix("rndr-tray-")?.parse().ok()?;
    Some(TrayItemId(raw))
}
