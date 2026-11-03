#[path = "../examples/support/markdown_gallery.rs"]
mod gallery;
use rndr_core::*;
use rndr_ui::*;
#[test]
fn retained_core_pane_updates_owned_fills_without_rebuilding_or_clobbering_overrides() {
    let mut app = App::new();
    app.add_plugin(UiPlugin);
    app.add_system(Schedule::PostUpdate, apply_pane);
    let selected = app
        .world_mut()
        .spawn((Label::new("Record"), Body::new("Body")));
    app.world_mut().insert_resource(Selection {
        entity: Some(selected),
    });
    let pane = app.world_mut().spawn(detail_pane());
    apply_theme_bridge(app.world_mut(), UiTheme::default());
    rndr_paint::frame(&mut app);
    let rows = app.world().get::<Pane>(pane).unwrap().spawned.clone();
    app.world_mut().get_mut::<Label>(rows[1]).unwrap().text = "Uncommitted draft".into();
    // A takeover equal to the NEXT palette must not be accidentally reacquired.
    let custom = UiTheme {
        appearance: Appearance::Dark,
        ..Default::default()
    }
    .core_theme()
    .status;
    app.world_mut().insert(rows[2], custom);
    let dark = UiTheme {
        appearance: Appearance::Dark,
        ..Default::default()
    };
    apply_theme_bridge(app.world_mut(), dark.clone());
    rndr_paint::frame(&mut app);
    assert_eq!(app.world().get::<Pane>(pane).unwrap().spawned, rows);
    assert_eq!(
        app.world().get::<Label>(rows[1]).unwrap().text,
        "Uncommitted draft"
    );
    assert_eq!(
        app.world().get::<Color>(rows[0]).copied(),
        Some(dark.core_theme().title)
    );
    assert_eq!(
        app.world().get::<Color>(rows[1]).copied(),
        Some(dark.core_theme().body)
    );
    assert_eq!(app.world().get::<Color>(rows[2]).copied(), Some(custom));
    apply_theme_bridge(app.world_mut(), UiTheme::default());
    rndr_paint::frame(&mut app);
    assert_eq!(app.world().get::<Color>(rows[2]).copied(), Some(custom));
    set_selection(app.world_mut(), None);
    rndr_paint::frame(&mut app);
    let empty = app.world().get::<Pane>(pane).unwrap().spawned[0];
    apply_theme_bridge(app.world_mut(), UiTheme::default());
    rndr_paint::frame(&mut app);
    assert_eq!(app.world().get::<Pane>(pane).unwrap().spawned, vec![empty]);
    assert_eq!(
        app.world().get::<Color>(empty).copied(),
        Some(UiTheme::default().core_theme().empty)
    );
}

fn key(app: &mut App, named: NamedKey) {
    app.world_mut().resource_mut::<Input>().unwrap().keys =
        vec![KeyEvent::named(named, Modifiers::default())];
    rndr_paint::frame(app);
}
#[test]
fn gallery_streams_while_keyboard_operates_retained_controls_and_theme() {
    let mut app = gallery::create(760., 660., UiTheme::default());
    let before = app.world().resource::<gallery::Gallery>().unwrap().clone();
    request_focus(app.world_mut(), before.increment.root);
    key(&mut app, NamedKey::Enter);
    assert_eq!(app.world().resource::<gallery::Gallery>().unwrap().count, 1);
    key(&mut app, NamedKey::Tab);
    assert_eq!(
        app.world().resource::<Focus>().unwrap().entity,
        Some(before.theme_button.root)
    );
    key(&mut app, NamedKey::Space);
    assert_eq!(
        app.world().resource::<UiTheme>().unwrap().appearance,
        Appearance::Light
    );
    app.world_mut().resource_mut::<Input>().unwrap().keys =
        vec![KeyEvent::named(NamedKey::Space, Modifiers::default()).released()];
    rndr_paint::frame(&mut app);
    assert_eq!(
        app.world().resource::<UiTheme>().unwrap().appearance,
        Appearance::Dark
    );
    while app.world().resource::<gallery::Gallery>().unwrap().cursor < gallery::TAIL.len() {
        gallery::tick(app.world_mut());
        rndr_paint::frame(&mut app);
        assert!(before.value_slot.is_mounted(app.world()));
        assert!(before.controls_slot.is_mounted(app.world()));
        assert_eq!(
            app.world().resource::<Focus>().unwrap().entity,
            Some(before.theme_button.root)
        );
    }
    let g = app.world().resource::<gallery::Gallery>().unwrap();
    assert_eq!(g.count, 1);
    assert!(app
        .world()
        .get::<Label>(g.value)
        .unwrap()
        .text
        .contains("Live value: 1"));
    assert!(app
        .world()
        .get::<MarkdownView>(g.view)
        .unwrap()
        .source()
        .ends_with(gallery::TAIL));
    let theme = app.world().resource::<UiTheme>().unwrap();
    assert_eq!(
        app.world().get::<Color>(g.view).copied(),
        Some(theme.color(Token::Content))
    );
    app.world_mut().insert_resource(Viewport::new(340., 440.));
    rndr_paint::frame(&mut app);
    assert_eq!(
        app.world()
            .get::<Rect>(before.controls_slot.root)
            .unwrap()
            .w,
        308.
    );
    assert_eq!(
        app.world().resource::<Focus>().unwrap().entity,
        Some(before.theme_button.root)
    );
}
#[test]
fn theme_bridge_reaches_core_editor_default_labels_and_explicit_overrides() {
    let mut app = App::new();
    app.add_plugin(UiPlugin);
    app.world_mut().insert_resource(Viewport::new(400., 200.));
    let root = app.world_mut().spawn((Layout {
        width: Size::Fill,
        height: Size::Fill,
        ..Layout::column()
    },));
    let editor = app.world_mut().spawn((
        Layout {
            width: Size::Fill,
            height: Size::Px(100.),
            ..Layout::column()
        },
        Buffer::from_str("Editor text"),
        Editor::default(),
        Label::new(""),
    ));
    set_parent(app.world_mut(), editor, root);
    let plain = app.world_mut().spawn((
        Layout {
            width: Size::Fill,
            height: Size::Px(30.),
            ..Layout::column()
        },
        Label::new("Core widget"),
    ));
    set_parent(app.world_mut(), plain, root);
    let explicit = Color::rgb(1., 0., 0.);
    let override_label = app.world_mut().spawn((
        Layout {
            width: Size::Fill,
            height: Size::Px(30.),
            ..Layout::column()
        },
        Label::new("Override").with_color(explicit),
        TextStyle {
            size: 12.,
            ..Default::default()
        },
    ));
    set_parent(app.world_mut(), override_label, root);
    for appearance in [Appearance::Light, Appearance::Dark] {
        let theme = UiTheme {
            appearance,
            reading_scale: 1.25,
            ..Default::default()
        };
        apply_theme_bridge(app.world_mut(), theme.clone());
        let frame = rndr_paint::frame(&mut app);
        assert_eq!(
            app.world().get::<Label>(editor).unwrap().text,
            "Editor text"
        );
        assert_eq!(style_for(app.world(), editor).size, 20.);
        assert_eq!(style_for(app.world(), plain).size, 20.);
        assert_eq!(style_for(app.world(), override_label).size, 12.);
        assert_eq!(
            app.world().get::<Label>(override_label).unwrap().color,
            Some(explicit)
        );
        let rect = *app.world().get::<Rect>(editor).unwrap();
        assert!(frame
            .quads
            .iter()
            .any(|q| q.color == theme.color(Token::Text)
                && q.rect.y >= rect.y
                && q.rect.y < rect.y + rect.h));
        assert!(frame.quads.iter().any(|q| q.color == explicit));
    }
    // Resource updates, not a hidden process-global: another world is unchanged.
    let other = App::new();
    assert!(!other.world().has_resource::<Theme>());
    assert!(!other.world().has_resource::<TextStyle>());
}
