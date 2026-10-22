// Copyright 2022-2022 Tauri Programme within The Commons Conservancy
// SPDX-License-Identifier: Apache-2.0
// SPDX-License-Identifier: MIT

use std::{io::Cursor, sync::Arc};

use muda::{MenuItemKindSnapshot, SnapshotIcon};

use super::StatusNotifierTray;

pub(super) fn into_ksni_menu(
    items: &[MenuItemKindSnapshot],
) -> Vec<ksni::MenuItem<StatusNotifierTray>> {
    items.iter().filter_map(into_ksni_item).collect()
}

fn into_ksni_item(item: &MenuItemKindSnapshot) -> Option<ksni::MenuItem<StatusNotifierTray>> {
    match item {
        MenuItemKindSnapshot::MenuItem(item) => {
            let activate = Arc::clone(&item.activate);
            let (label, shortcut) = label_and_shortcut(&item.text());
            Some(
                ksni::menu::StandardItem {
                    label,
                    shortcut,
                    enabled: item.is_enabled(),
                    activate: Box::new(move |_| activate()),
                    ..Default::default()
                }
                .into(),
            )
        }
        MenuItemKindSnapshot::Check(item) => {
            let activate = Arc::clone(&item.activate);
            let (label, shortcut) = label_and_shortcut(&item.text());
            Some(
                ksni::menu::CheckmarkItem {
                    label,
                    shortcut,
                    enabled: item.is_enabled(),
                    checked: item.is_checked(),
                    activate: Box::new(move |_| activate()),
                    ..Default::default()
                }
                .into(),
            )
        }
        MenuItemKindSnapshot::Submenu(item) => {
            let (icon_name, icon_data) = icon_fields(item.icon().as_ref());
            Some(
                ksni::menu::SubMenu {
                    label: to_ksni_mnemonic(&item.text()),
                    enabled: item.is_enabled(),
                    icon_name,
                    icon_data,
                    submenu: into_ksni_menu(&item.items()),
                    ..Default::default()
                }
                .into(),
            )
        }
        MenuItemKindSnapshot::Predefined(item) if item.is_separator() => {
            Some(ksni::MenuItem::Separator)
        }
        MenuItemKindSnapshot::Icon(item) => {
            let (icon_name, icon_data) = icon_fields(item.icon().as_ref());
            let activate = Arc::clone(&item.activate);
            let (label, shortcut) = label_and_shortcut(&item.text());
            Some(
                ksni::menu::StandardItem {
                    label,
                    shortcut,
                    enabled: item.is_enabled(),
                    icon_name,
                    icon_data,
                    activate: Box::new(move |_| activate()),
                    ..Default::default()
                }
                .into(),
            )
        }
        // Items with no action on this platform are created disabled.
        MenuItemKindSnapshot::Predefined(item) => {
            let activate = Arc::clone(&item.activate);
            let (label, shortcut) = label_and_shortcut(&item.text());
            Some(
                ksni::menu::StandardItem {
                    label,
                    shortcut,
                    enabled: item.is_enabled(),
                    activate: Box::new(move |_| activate()),
                    ..Default::default()
                }
                .into(),
            )
        }
    }
}

/// Record separator between a visible label and a dbusmenu accelerator.
///
/// The menu snapshot does not expose the accelerator, so the caller appends
/// the keys after U+001E. Keys are separated by U+001F. The visible label is
/// everything before the mark.
const ACCEL_MARK: char = '\u{1e}';
const ACCEL_KEY: char = '\u{1f}';

fn label_and_shortcut(text: &str) -> (String, Vec<Vec<String>>) {
    match text.split_once(ACCEL_MARK) {
        Some((label, rest)) => {
            let keys: Vec<String> = rest
                .split(ACCEL_KEY)
                .filter(|part| !part.is_empty())
                .map(str::to_string)
                .collect();
            let shortcut = if keys.is_empty() {
                Vec::new()
            } else {
                vec![keys]
            };
            (to_ksni_mnemonic(label), shortcut)
        }
        None => (to_ksni_mnemonic(text), Vec::new()),
    }
}

fn to_ksni_mnemonic(text: &str) -> String {
    let mut converted = String::with_capacity(text.len());
    let mut characters = text.chars().peekable();
    while let Some(character) = characters.next() {
        match character {
            '_' => converted.push_str("__"),
            '&' if characters.peek() == Some(&'&') => {
                characters.next();
                converted.push('&');
            }
            '&' => converted.push('_'),
            _ => converted.push(character),
        }
    }
    converted
}

fn icon_fields(icon: Option<&SnapshotIcon>) -> (String, Vec<u8>) {
    match icon {
        Some(SnapshotIcon::Rgba {
            rgba,
            width,
            height,
        }) => (
            String::new(),
            encode_png(rgba, *width, *height).unwrap_or_default(),
        ),
        Some(SnapshotIcon::Native(icon)) => (icon.freedesktop_name().to_string(), Vec::new()),
        _ => Default::default(),
    }
}

fn encode_png(rgba: &[u8], width: u32, height: u32) -> Result<Vec<u8>, png::EncodingError> {
    let mut data = Vec::new();
    {
        let mut encoder = png::Encoder::new(Cursor::new(&mut data), width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header()?;
        writer.write_image_data(rgba)?;
    }
    Ok(data)
}
