//! Native tray icon.
//!
//! [`tray-icon`](https://docs.rs/tray-icon) 0.26 (MIT OR Apache-2.0) draws the
//! status item. The `ksni` feature is on and the default GTK AppIndicator
//! backend is off. On Linux that is a StatusNotifierItem on the session bus,
//! serviced from a worker thread. Clicks call the winit proxy, so the loop can
//! stay on `ControlFlow::Wait` while the window is hidden. A GTK tray would
//! have to share this thread and would either spin or miss the click.
//!
//! macOS and Windows emit a press and a release for one click. The press is
//! the [`TrayReport::Click`]. StatusNotifierItem reports the activation as a
//! release, and that release is the click. A mouse-up on macOS or Windows is
//! not a second toggle.

use std::io::Cursor;

use rndr_core::tray::{
    menu_id, menu_label, parse_menu_id, Entry, Image, Menu, MenuItem, Submenu, TrayCommand,
    TrayConfig, TrayIcon as CoreIcon, TrayReport,
};
use rndr_core::Platform;
use tray_icon::menu::{
    CheckMenuItem, IsMenuItem, MenuItem as NativeItem, PredefinedMenuItem, Submenu as NativeSub,
};
use tray_icon::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder};

/// Which edge of a left click becomes [`TrayReport::Click`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ClickEdge {
    /// Press. macOS and Windows also emit a release for the same click.
    Press,
    /// Release. StatusNotifierItem's activation is this edge.
    Release,
}

pub(crate) fn host_edge() -> ClickEdge {
    if cfg!(any(
        target_os = "linux",
        target_os = "dragonfly",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd"
    )) {
        ClickEdge::Release
    } else {
        ClickEdge::Press
    }
}

pub(crate) fn map_click(
    button: MouseButton,
    state: MouseButtonState,
    edge: ClickEdge,
) -> Option<TrayReport> {
    if button != MouseButton::Left {
        return None;
    }
    let hit = matches!(
        (edge, state),
        (ClickEdge::Press, MouseButtonState::Down) | (ClickEdge::Release, MouseButtonState::Up)
    );
    hit.then_some(TrayReport::Click)
}

pub(crate) fn map_icon_event(event: &tray_icon::TrayIconEvent) -> Option<TrayReport> {
    match event {
        tray_icon::TrayIconEvent::Click {
            button,
            button_state,
            ..
        } => map_click(*button, *button_state, host_edge()),
        tray_icon::TrayIconEvent::DoubleClick { button, .. } if *button == MouseButton::Left => {
            Some(TrayReport::DoubleClick)
        }
        _ => None,
    }
}

pub(crate) fn map_menu_event(event: &tray_icon::menu::MenuEvent) -> Option<TrayReport> {
    parse_menu_id(event.id.as_ref()).map(TrayReport::Activated)
}

pub(crate) struct Session {
    icon: Option<TrayIcon>,
}

impl Session {
    pub(crate) fn new() -> Self {
        Self { icon: None }
    }

    pub(crate) fn apply(&mut self, config: &TrayConfig, commands: &[TrayCommand]) {
        for command in commands {
            match command {
                TrayCommand::Create(_) => self.create(config),
                TrayCommand::SetIcon(_) => {
                    if self.icon.is_none() {
                        self.create(config);
                    } else {
                        self.replace_icon(&config.icon);
                    }
                }
                TrayCommand::SetTooltip(_) => self.replace_tooltip(&config.tooltip),
                TrayCommand::SetMenu(_) => self.replace_menu(&config.menu),
            }
        }
    }

    fn create(&mut self, config: &TrayConfig) {
        let Some(icon) = raster(&config.icon) else {
            eprintln!("rndr: tray icon bytes could not be decoded");
            return;
        };
        let menu = native_menu(&config.menu);
        let id = if config.id.is_empty() {
            "rndr".to_string()
        } else {
            config.id.clone()
        };
        let mut builder = TrayIconBuilder::new()
            .with_id(id.clone())
            .with_autosave_name(id)
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(false)
            .with_tooltip(config.tooltip.clone());
        if !config.title.is_empty() {
            builder = builder.with_title(config.title.clone());
        }
        let builder = install_icon(builder, icon, config.icon.template);
        match builder.build() {
            Ok(icon) => self.icon = Some(icon),
            Err(err) => eprintln!("rndr: tray icon was not created: {err}"),
        }
    }

    fn replace_icon(&mut self, icon: &CoreIcon) {
        let Some(native) = raster(icon) else {
            return;
        };
        let Some(tray) = self.icon.as_ref() else {
            return;
        };
        let result = replace_native_icon(tray, native, icon.template);
        if let Err(err) = result {
            eprintln!("rndr: tray icon was not updated: {err}");
        }
    }

    fn replace_tooltip(&mut self, tooltip: &str) {
        if let Some(tray) = self.icon.as_ref() {
            if let Err(err) = tray.set_tooltip(Some(tooltip)) {
                eprintln!("rndr: tray tooltip was not updated: {err}");
            }
        }
    }

    fn replace_menu(&mut self, menu: &Menu) {
        if let Some(tray) = self.icon.as_ref() {
            tray.set_menu(Some(Box::new(native_menu(menu))));
        }
    }
}

fn install_icon(
    builder: TrayIconBuilder,
    icon: tray_icon::Icon,
    template: bool,
) -> TrayIconBuilder {
    #[cfg(target_os = "macos")]
    if template {
        return builder.with_icon_templated(icon);
    }
    #[cfg(not(target_os = "macos"))]
    let _ = template;
    builder.with_icon(icon)
}

fn replace_native_icon(
    tray: &TrayIcon,
    icon: tray_icon::Icon,
    template: bool,
) -> tray_icon::Result<()> {
    #[cfg(target_os = "macos")]
    if template {
        return tray.set_icon_templated(Some(icon));
    }
    #[cfg(not(target_os = "macos"))]
    let _ = template;
    tray.set_icon(Some(icon))
}

fn native_menu(menu: &Menu) -> tray_icon::menu::Menu {
    let native = tray_icon::menu::Menu::new();
    append_all(&native, menu);
    native
}

fn append_all(parent: &dyn Append, menu: &Menu) {
    for entry in menu.entries() {
        match entry {
            Entry::Separator => parent.push(&PredefinedMenuItem::separator()),
            Entry::Item(item) => push_item(parent, item),
            Entry::Submenu(sub) => push_submenu(parent, sub),
        }
    }
}

fn push_item(parent: &dyn Append, item: &MenuItem) {
    let (text, accelerator) = row_presentation(&item.label, &item.shortcut, host_linux());
    let id = menu_id(item.id);
    if item.checkable {
        parent.push(&CheckMenuItem::with_id(
            id,
            text,
            item.enabled,
            item.checked,
            accelerator,
        ));
    } else {
        parent.push(&NativeItem::with_id(id, text, item.enabled, accelerator));
    }
}

fn push_submenu(parent: &dyn Append, sub: &Submenu) {
    let (text, _) = row_presentation(&sub.label, "", host_linux());
    let child = NativeSub::with_id(menu_id(sub.id), text, sub.enabled);
    append_all(&child, &sub.menu);
    parent.push(&child);
}

fn host_linux() -> bool {
    cfg!(any(
        target_os = "linux",
        target_os = "dragonfly",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd"
    ))
}

/// Menu text and the accelerator passed to the native item.
///
/// A parsed chord is the accelerator. macOS and Windows draw that column, so
/// the chord is left out of the label. Linux does not draw it, so the chord
/// stays in the label. A shortcut that does not parse stays in the label on
/// every platform.
pub(crate) fn row_presentation(
    label: &str,
    shortcut: &str,
    linux: bool,
) -> (String, Option<tray_icon::menu::accelerator::Accelerator>) {
    let accelerator = shortcut.parse().ok();
    let platform = if linux { Platform::X11 } else { Platform::Mac };
    let shown = if linux || accelerator.is_none() {
        shortcut
    } else {
        ""
    };
    let text = menu_label(label, shown, platform).replace('&', "&&");
    (text, accelerator)
}

trait Append {
    fn push(&self, item: &dyn IsMenuItem);
}

impl Append for tray_icon::menu::Menu {
    fn push(&self, item: &dyn IsMenuItem) {
        let _ = self.append(item);
    }
}

impl Append for NativeSub {
    fn push(&self, item: &dyn IsMenuItem) {
        let _ = self.append(item);
    }
}

fn raster(icon: &CoreIcon) -> Option<tray_icon::Icon> {
    let (width, height, rgba) = match &icon.image {
        Image::Rgba {
            width,
            height,
            rgba,
        } => (*width, *height, rgba.clone()),
        Image::Png(bytes) => decode_png(bytes)?,
    };
    tray_icon::Icon::from_rgba(rgba, width, height).ok()
}

fn decode_png(bytes: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()?];
    let frame = reader.next_frame(&mut buf).ok()?;
    let data = &buf[..frame.buffer_size()];
    let rgba = match frame.color_type {
        png::ColorType::Rgba => data.to_vec(),
        png::ColorType::Rgb => expand_rgb(data),
        png::ColorType::Grayscale => expand_gray(data),
        png::ColorType::GrayscaleAlpha => expand_gray_alpha(data),
        _ => return None,
    };
    Some((frame.width, frame.height, rgba))
}

fn expand_rgb(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() / 3 * 4);
    for chunk in data.as_chunks::<3>().0 {
        out.extend_from_slice(&[chunk[0], chunk[1], chunk[2], 255]);
    }
    out
}

fn expand_gray(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() * 4);
    for gray in data {
        out.extend_from_slice(&[*gray, *gray, *gray, 255]);
    }
    out
}

fn expand_gray_alpha(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() / 2 * 4);
    for chunk in data.as_chunks::<2>().0 {
        out.extend_from_slice(&[chunk[0], chunk[0], chunk[0], chunk[1]]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{decode_png, map_click, ClickEdge};
    use rndr_core::TrayReport;
    use tray_icon::{MouseButton, MouseButtonState};

    #[test]
    fn left_press_is_one_click_and_right_click_is_not() {
        assert_eq!(
            map_click(MouseButton::Left, MouseButtonState::Down, ClickEdge::Press),
            Some(TrayReport::Click)
        );
        assert_eq!(
            map_click(MouseButton::Left, MouseButtonState::Up, ClickEdge::Press),
            None
        );
        assert_eq!(
            map_click(MouseButton::Left, MouseButtonState::Up, ClickEdge::Release),
            Some(TrayReport::Click)
        );
        assert_eq!(
            map_click(
                MouseButton::Left,
                MouseButtonState::Down,
                ClickEdge::Release
            ),
            None
        );
        assert_eq!(
            map_click(MouseButton::Right, MouseButtonState::Down, ClickEdge::Press),
            None
        );
        assert_eq!(
            map_click(
                MouseButton::Middle,
                MouseButtonState::Up,
                ClickEdge::Release
            ),
            None
        );
    }

    #[test]
    fn png_bytes_become_rgba() {
        let mut encoded = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut encoded, 1, 1);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(&[8, 16, 32, 64]).unwrap();
        }
        let (width, height, rgba) = decode_png(&encoded).unwrap();
        assert_eq!((width, height), (1, 1));
        assert_eq!(rgba, vec![8, 16, 32, 64]);
    }

    #[test]
    fn menu_rows_show_the_hotkey_on_linux_and_keep_other_accelerators() {
        let (text, accel) = super::row_presentation("Show Palette", "Alt+Space", true);
        assert_eq!(text, "Show Palette  Alt+Space");
        assert!(accel.is_some());
        let (text, accel) = super::row_presentation("Show Palette", "Ctrl+Alt+Space", true);
        assert_eq!(text, "Show Palette  Ctrl+Alt+Space");
        assert!(accel.is_some());
        let (text, accel) = super::row_presentation("Show Palette", "Cmd+Space", true);
        assert_eq!(text, "Show Palette  Cmd+Space");
        assert!(accel.is_some());
        let (text, accel) = super::row_presentation("Show Palette", "⌥Space", true);
        assert_eq!(text, "Show Palette  ⌥Space");
        assert!(accel.is_none());
        let (text, accel) = super::row_presentation("Show & Palette", "Alt+Space", false);
        assert_eq!(text, "Show && Palette");
        assert!(accel.is_some());
        let (text, accel) = super::row_presentation("Show Palette", "Cmd+K", false);
        assert_eq!(text, "Show Palette");
        assert!(accel.is_some());
        let (text, accel) = super::row_presentation("Show Palette", "⌥Space", false);
        assert_eq!(text, "Show Palette\t⌥Space");
        assert!(accel.is_none());
        let (text, accel) = super::row_presentation("Quit", "", false);
        assert_eq!(text, "Quit");
        assert!(accel.is_none());
    }

    #[test]
    fn host_edge_matches_the_backend() {
        let edge = super::host_edge();
        if cfg!(any(
            target_os = "linux",
            target_os = "dragonfly",
            target_os = "freebsd",
            target_os = "netbsd",
            target_os = "openbsd"
        )) {
            assert_eq!(edge, ClickEdge::Release);
        } else {
            assert_eq!(edge, ClickEdge::Press);
        }
    }
}
