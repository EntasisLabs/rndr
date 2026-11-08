use rndr_core::*;

fn bar() -> (Menu, TrayItemId, TrayItemId, TrayItemId) {
    let mut bar = Menu::new();
    let (mut new, mut wrap, mut file) = (None, None, TrayItemId::from_raw(0));
    file = {
        let id = bar.submenu("File", |m| {
            new = Some(m.add(Item::new("New").shortcut("Ctrl+N")));
        });
        let _ = file;
        id
    };
    bar.submenu("View", |m| {
        wrap = Some(m.add(Item::new("Wrap").checked(true)));
    });
    (bar, new.unwrap(), wrap.unwrap(), file)
}

#[test]
fn the_runner_sees_each_change_once_and_picks_become_events() {
    let mut app = App::new();
    assert!(take_app_menu_change(app.world_mut()).is_none());
    let (menu, new, wrap, file) = bar();
    assert_ne!(new, wrap, "ids are unique across the bar");
    set_app_menu(app.world_mut(), menu.clone());
    assert_eq!(take_app_menu_change(app.world_mut()), Some(menu.clone()));
    assert!(take_app_menu_change(app.world_mut()).is_none());

    push_app_menu_pick(app.world_mut(), wrap);
    push_app_menu_pick(app.world_mut(), file);
    push_app_menu_pick(app.world_mut(), TrayItemId::from_raw(999));
    assert!(app_menu_pending(app.world()));
    app.update();
    assert_eq!(
        events::<AppMenuPicked>(app.world()).to_vec(),
        vec![AppMenuPicked { item: wrap }],
        "submenu rows and unknown ids are dropped"
    );
    app.update();
    assert!(events::<AppMenuPicked>(app.world()).is_empty());

    set_app_menu(app.world_mut(), menu);
    assert!(take_app_menu_change(app.world_mut()).is_some());
}

#[test]
fn ids_round_trip_and_stay_apart_from_tray_ids() {
    let id = TrayItemId::from_raw(7);
    assert_eq!(parse_app_menu_id(&app_menu_id(id)), Some(id));
    assert_eq!(parse_app_menu_id("rndr-tray-7"), None);
    assert_eq!(parse_menu_id(&app_menu_id(id)), None);
    assert_eq!(app_menu_support(Platform::X11), None);
    assert_eq!(app_menu_support(Platform::Wayland), None);
    assert_eq!(
        app_menu_support(Platform::Mac),
        Some(Availability::Unverified)
    );
}
