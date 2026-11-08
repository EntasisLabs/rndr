//! The native application menu bar on macOS and Windows.
//!
//! Build the bar as a [`Menu`] whose top-level rows are submenus, one per
//! named menu ("File", "Edit", ...), and pass it to [`set_app_menu`]. Item ids
//! come from that one [`Menu`], so they are unique across the whole bar.
//! Shortcut text that parses as a chord (`Ctrl+N`, `Cmd+Shift+Z`) becomes a
//! real accelerator. On macOS the runner adds the standard application menu
//! (About, Services, Hide, Hide Others, Show All, Quit) in front of the bar.
//!
//! A picked item is queued by the runner, which wakes the loop.
//! [`crate::App::update`] turns the queue into [`AppMenuPicked`] before Input.
//! Linux and the browser have no native bar: [`app_menu_support`] returns
//! `None` there, and apps draw menus with `rndr_ui::menu`.
use crate::event::{self, Event};
use crate::keys::Platform;
use crate::tray::{Menu, TrayItemId};
use crate::window::Availability;
use crate::world::World;

/// An item of the app menu bar was picked. Readable in Input and Update.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AppMenuPicked {
    /// Id that [`Menu::add`] returned for the item.
    pub item: TrayItemId,
}
impl Event for AppMenuPicked {}

#[derive(Default)]
struct AppMenuState {
    menu: Option<Menu>,
    revision: u64,
    applied: u64,
    inbox: Vec<TrayItemId>,
}

/// Replace the app menu bar. The runner installs it before the next frame is drawn.
pub fn set_app_menu(world: &mut World, menu: Menu) {
    if world.resource::<AppMenuState>().is_none() {
        world.insert_resource(AppMenuState::default());
    }
    let state = world.resource_mut::<AppMenuState>().unwrap();
    state.menu = Some(menu);
    state.revision += 1;
}

/// The bar last passed to [`set_app_menu`].
pub fn app_menu(world: &World) -> Option<&Menu> {
    world.resource::<AppMenuState>()?.menu.as_ref()
}

/// How the runner handles the app menu bar on `platform`; `None` when the
/// platform has no native bar. macOS and Windows are compiled but have not
/// been run in this repository's checks, so they report
/// [`Availability::Unverified`].
pub fn app_menu_support(platform: Platform) -> Option<Availability> {
    match platform {
        Platform::Mac | Platform::Windows => Some(Availability::Unverified),
        Platform::X11 | Platform::Wayland => None,
    }
}

/// Runner half: the bar to install when it changed since the last call.
pub fn take_app_menu_change(world: &mut World) -> Option<Menu> {
    let state = world.resource_mut::<AppMenuState>()?;
    if state.applied == state.revision {
        return None;
    }
    state.applied = state.revision;
    state.menu.clone()
}

/// Runner half: queue a pick from the native bar. Unknown ids are dropped.
pub fn push_app_menu_pick(world: &mut World, item: TrayItemId) {
    let known = app_menu(world).is_some_and(|m| m.item(item).is_some());
    if let (true, Some(state)) = (known, world.resource_mut::<AppMenuState>()) {
        state.inbox.push(item);
    }
}

/// Whether picks are queued for the next update.
pub fn app_menu_pending(world: &World) -> bool {
    world
        .resource::<AppMenuState>()
        .is_some_and(|s| !s.inbox.is_empty())
}

/// Native id of an app menu row; distinct from tray ids.
pub fn app_menu_id(id: TrayItemId) -> String {
    format!("rndr-app-menu-{}", id.as_u32())
}

/// Parse a string from [`app_menu_id`].
pub fn parse_app_menu_id(text: &str) -> Option<TrayItemId> {
    let raw = text.strip_prefix("rndr-app-menu-")?.parse().ok()?;
    Some(TrayItemId::from_raw(raw))
}

pub(crate) fn drive(world: &mut World) {
    let picks = world
        .resource_mut::<AppMenuState>()
        .map(|s| std::mem::take(&mut s.inbox))
        .unwrap_or_default();
    for item in picks {
        event::send(world, AppMenuPicked { item });
    }
}
