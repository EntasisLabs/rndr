//! Native app menu bar through [`muda`](https://docs.rs/muda) 0.21 (MIT OR Apache-2.0).
//!
//! The core [`Menu`] becomes a muda menu bar. On macOS the standard
//! application menu comes first, so About, Hide and Quit behave as usual.
//! Picks arrive through the shared `MenuEvent` handler in `lib.rs`.
use muda::accelerator::Accelerator;
use muda::{CheckMenuItem, IsMenuItem, MenuItem as NativeItem, PredefinedMenuItem, Submenu};
use rndr_core::tray::{Entry, Menu, MenuItem};
use rndr_core::{app_menu_id, menu_label, Platform};

const PLATFORM: Platform = if cfg!(target_os = "macos") {
    Platform::Mac
} else {
    Platform::Windows
};

pub(crate) fn build(menu: &Menu, app_name: &str) -> muda::Menu {
    let native = muda::Menu::new();
    #[cfg(target_os = "macos")]
    {
        let name = if app_name.trim().is_empty() {
            "App"
        } else {
            app_name
        };
        let app = Submenu::new(name, true);
        let _ = app.append_items(&[
            &PredefinedMenuItem::about(None, None),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::services(None),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::hide(None),
            &PredefinedMenuItem::hide_others(None),
            &PredefinedMenuItem::show_all(None),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::quit(None),
        ]);
        let _ = native.append(&app);
    }
    #[cfg(not(target_os = "macos"))]
    let _ = app_name;
    append_all(&native, menu);
    native
}

trait Append {
    fn push(&self, item: &dyn IsMenuItem);
}

impl Append for muda::Menu {
    fn push(&self, item: &dyn IsMenuItem) {
        let _ = self.append(item);
    }
}

impl Append for Submenu {
    fn push(&self, item: &dyn IsMenuItem) {
        let _ = self.append(item);
    }
}

fn append_all(parent: &dyn Append, menu: &Menu) {
    for entry in menu.entries() {
        match entry {
            Entry::Separator => parent.push(&PredefinedMenuItem::separator()),
            Entry::Item(item) => push_item(parent, item),
            Entry::Submenu(sub) => {
                let child =
                    Submenu::with_id(app_menu_id(sub.id), text(&sub.label, ""), sub.enabled);
                append_all(&child, &sub.menu);
                parent.push(&child);
            }
        }
    }
}

fn push_item(parent: &dyn Append, item: &MenuItem) {
    let accelerator: Option<Accelerator> = item.shortcut.parse().ok();
    let shown = if accelerator.is_some() {
        ""
    } else {
        &item.shortcut
    };
    let label = text(&item.label, shown);
    let id = app_menu_id(item.id);
    if item.checkable {
        parent.push(&CheckMenuItem::with_id(
            id,
            label,
            item.enabled,
            item.checked,
            accelerator,
        ));
    } else {
        parent.push(&NativeItem::with_id(id, label, item.enabled, accelerator));
    }
}

/// Label with unparsed shortcut text, and `&` escaped for Windows mnemonics.
fn text(label: &str, shortcut: &str) -> String {
    menu_label(label, shortcut, PLATFORM).replace('&', "&&")
}

#[cfg(target_os = "windows")]
pub(crate) fn hwnd(window: &dyn winit::window::Window) -> Option<isize> {
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let handle = window.window_handle().ok()?;
    let RawWindowHandle::Win32(win) = handle.as_raw() else {
        return None;
    };
    Some(win.hwnd.get())
}
