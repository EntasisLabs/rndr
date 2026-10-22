//! Headless tray menu, event mapping, and tray-only lifecycle.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use rndr_core::tray::Entry;
use rndr_core::{
    displayed_label, events, map_report, phase, push_report, set_checked, set_enabled, set_icon,
    set_label, starts_hidden, toggle_on_click, App, Availability, FakeTray, IconError, Image,
    Input, Item, KeyBinding, Keymap, Keystroke, Menu, NamedKey, Platform, Schedule, TrayCommand,
    TrayConfig, TrayEvent, TrayIcon, TrayItemId, TrayMapped, TrayMenu, TrayPhase, TrayReport,
    TraySupport, Wake,
};

struct FlagWake(AtomicBool);

impl Wake for FlagWake {
    fn wake(&self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

fn icon() -> TrayIcon {
    TrayIcon::rgba(1, 1, vec![0, 0, 0, 255]).unwrap()
}

fn show_shortcut() -> String {
    KeyBinding::new("palette.show", [Keystroke::character('p').primary()]).shortcut(Platform::Mac)
}

#[test]
fn menu_ids_updates_and_nested_submenus() {
    let shortcut = show_shortcut();
    assert_eq!(shortcut, "\u{2318}P");

    let mut menu = Menu::new();
    let show = menu.add(Item::new("Show Palette").shortcut(shortcut.clone()));
    let settings = menu.add(Item::new("Settings").enabled(false));
    menu.separator();
    let mut child = TrayItemId::from_raw(0);
    let mut leaf = TrayItemId::from_raw(0);
    let more = menu.submenu("More", |menu| {
        child = menu.add(Item::new("Child").checked(true));
        menu.separator();
        let _deep = menu.submenu("Deep", |menu| {
            leaf = menu.add(Item::new("Leaf").enabled(false));
        });
    });
    let quit = menu.add(Item::new("Quit").quits());

    let mut ids = [show, settings, more, child, leaf, quit];
    ids.sort_by_key(|id| id.as_u32());
    let mut unique = ids.to_vec();
    unique.dedup();
    assert_eq!(unique.len(), ids.len(), "ids are unique across submenus");
    assert!(ids.iter().all(|id| id.as_u32() > 0));

    assert_eq!(menu.item(show).unwrap().shortcut, shortcut);
    assert_eq!(
        displayed_label("Show Palette", &shortcut),
        format!("Show Palette\t{shortcut}")
    );
    assert!(!menu.item(settings).unwrap().enabled);
    assert!(menu.item(child).unwrap().checkable && menu.item(child).unwrap().checked);
    assert!(menu.item(quit).unwrap().quits);
    assert!(!menu.item(leaf).unwrap().enabled);
    assert_eq!(menu.submenu_row(more).unwrap().label, "More");
    assert!(menu
        .entries()
        .iter()
        .any(|entry| matches!(entry, Entry::Separator)));
    let nested = menu.submenu_row(more).unwrap();
    assert!(nested
        .menu
        .entries()
        .iter()
        .any(|entry| matches!(entry, Entry::Separator)));

    assert!(menu.set_label(show, "Open Palette"));
    assert_eq!(menu.item(show).unwrap().label, "Open Palette");
    assert!(menu.set_enabled(settings, true));
    assert!(menu.item(settings).unwrap().enabled);
    assert!(menu.set_checked(child, false));
    assert!(!menu.item(child).unwrap().checked);
    assert!(menu.item(child).unwrap().checkable);
    assert!(menu.set_label(more, "Extra"));
    assert_eq!(menu.submenu_row(more).unwrap().label, "Extra");
    assert!(menu.set_enabled(leaf, true));
    assert!(menu.item(leaf).unwrap().enabled);
    assert!(!menu.set_label(TrayItemId::from_raw(999), "missing"));
    assert!(!menu.set_checked(more, true));

    let mut map = Keymap::default();
    map.bind(KeyBinding::new(
        "palette.show",
        [Keystroke::named(NamedKey::F(6))],
    ));
    let from_map = map.shortcut("palette.show", &[], Platform::Windows);
    assert_eq!(from_map, "F6");
    assert_eq!(displayed_label("Show", &from_map), "Show\tF6");
}

#[test]
fn icon_bytes_are_checked() {
    assert!(matches!(
        TrayIcon::rgba(0, 1, vec![0, 0, 0, 255]),
        Err(IconError::Empty)
    ));
    assert_eq!(
        TrayIcon::rgba(1, 1, vec![0, 0, 0]).unwrap_err(),
        IconError::Size
    );
    let icon = TrayIcon::rgba(1, 1, vec![1, 2, 3, 4])
        .unwrap()
        .template(true);
    assert!(icon.template);
    assert!(matches!(
        icon.image,
        Image::Rgba {
            width: 1,
            height: 1,
            ..
        }
    ));
    assert_eq!(
        TrayIcon::png(b"not a png".to_vec()).unwrap_err(),
        IconError::Png
    );
    assert!(TrayIcon::png(b"\x89PNG\r\n\x1a\nrest".to_vec()).is_ok());
}

#[test]
fn platform_support_matches_the_documented_table() {
    let mac = rndr_core::tray::platform_support(Platform::Mac);
    assert_eq!(mac.template, Availability::Supported);
    assert_eq!(mac.click, Availability::Supported);
    assert_eq!(mac.double_click, Availability::Ignored);
    assert_eq!(mac.menu, Availability::Supported);

    let windows = rndr_core::tray::platform_support(Platform::Windows);
    assert_eq!(windows.template, Availability::Ignored);
    assert_eq!(windows.double_click, Availability::Supported);

    let linux = TraySupport {
        icon: Availability::Supported,
        template: Availability::Ignored,
        tooltip: Availability::Supported,
        menu: Availability::Supported,
        click: Availability::Supported,
        double_click: Availability::Ignored,
    };
    assert_eq!(rndr_core::tray::platform_support(Platform::X11), linux);
    assert_eq!(rndr_core::tray::platform_support(Platform::Wayland), linux);
}

#[test]
fn fake_backend_maps_clicks_and_drops_hover() {
    assert_eq!(
        map_report(TrayReport::Click),
        Some(TrayMapped::Icon(TrayEvent::Click))
    );
    assert_eq!(
        map_report(TrayReport::DoubleClick),
        Some(TrayMapped::Icon(TrayEvent::DoubleClick))
    );
    assert_eq!(map_report(TrayReport::Hover), None);
    let show_id_for_map = TrayItemId::from_raw(3);
    assert_eq!(
        map_report(TrayReport::Activated(show_id_for_map)),
        Some(TrayMapped::Menu(TrayMenu::Activated(show_id_for_map)))
    );

    let mut menu = Menu::new();
    let show = menu.add(Item::new("Show"));
    let mut app = App::new();
    let flag = Arc::new(FlagWake(AtomicBool::new(false)));
    app.set_wake(flag.clone());
    app.world_mut().insert_resource(
        TrayConfig::new(icon(), menu)
            .tooltip("palette")
            .hide_window(false),
    );
    app.update();

    let mut backend = FakeTray::new();
    backend.sync(app.world_mut());
    match backend.commands.first() {
        Some(TrayCommand::Create(config)) => {
            assert_eq!(config.tooltip, "palette");
            assert!(config.menu.item(show).is_some());
        }
        other => panic!("startup queues a create, got {other:?}"),
    }

    flag.0.store(false, Ordering::SeqCst);
    let next = TrayIcon::rgba(1, 1, vec![9, 9, 9, 255]).unwrap();
    assert!(set_icon(app.world_mut(), next));
    assert!(
        flag.0.load(Ordering::SeqCst),
        "swapping the icon wakes the loop"
    );
    assert!(set_label(app.world_mut(), show, "Open"));
    assert!(set_enabled(app.world_mut(), show, false));
    assert!(set_checked(app.world_mut(), show, true));
    backend.commands.clear();
    backend.sync(app.world_mut());
    assert!(backend
        .commands
        .iter()
        .any(|command| matches!(command, TrayCommand::SetIcon(_))));
    assert!(backend.commands.iter().any(|command| matches!(
        command,
        TrayCommand::SetMenu(menu) if menu.item(show).is_some_and(|item| {
            item.label == "Open" && !item.enabled && item.checked && item.checkable
        })
    )));

    flag.0.store(false, Ordering::SeqCst);
    backend.emit(TrayReport::Hover);
    backend.emit(TrayReport::Click);
    backend.emit(TrayReport::DoubleClick);
    backend.emit(TrayReport::Activated(show));
    backend.sync(app.world_mut());
    assert!(
        flag.0.load(Ordering::SeqCst),
        "a click wakes the loop while the window is up"
    );

    app.update();
    assert_eq!(
        events::<TrayEvent>(app.world()),
        &[TrayEvent::Click, TrayEvent::DoubleClick]
    );
    assert_eq!(
        events::<TrayMenu>(app.world()),
        &[TrayMenu::Activated(show)]
    );
    assert!(!app.world().resource::<Input>().unwrap().quit);
}

#[test]
fn tray_only_stays_running_until_quit() {
    let mut menu = Menu::new();
    let _show = menu.add(Item::new("Show"));
    let quit = menu.add(Item::new("Quit").quits());
    let mut app = App::new();
    app.world_mut()
        .insert_resource(TrayConfig::new(icon(), menu).hide_window(true));
    assert!(starts_hidden(app.world()));

    app.update();
    assert_eq!(
        phase(app.world()),
        TrayPhase {
            installed: true,
            window_visible: false,
            running: true,
        }
    );

    push_report(app.world_mut(), TrayReport::Activated(quit));
    app.update();
    assert_eq!(
        phase(app.world()),
        TrayPhase {
            installed: true,
            window_visible: false,
            running: false,
        }
    );
    assert!(events::<TrayMenu>(app.world()).contains(&TrayMenu::Activated(quit)));
}

#[test]
fn left_click_toggles_the_hidden_window() {
    let mut menu = Menu::new();
    menu.add(Item::new("Show"));
    let mut app = App::new();
    app.world_mut()
        .insert_resource(TrayConfig::new(icon(), menu).hide_window(true));
    app.add_system(Schedule::Update, toggle_on_click);

    push_report(app.world_mut(), TrayReport::Click);
    app.update();
    assert!(phase(app.world()).window_visible);
    assert!(phase(app.world()).running);

    push_report(app.world_mut(), TrayReport::Click);
    app.update();
    assert!(!phase(app.world()).window_visible);
    assert!(phase(app.world()).running);
}
