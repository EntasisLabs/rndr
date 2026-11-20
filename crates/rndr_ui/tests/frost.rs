//! Translucent surfaces, host planes, tree washes, and frosted overlays.
use rndr_core::*;
use rndr_ui::controls::{Button, ButtonProps, ButtonVariant, HostIntent};
use rndr_ui::inputs::{Toast, ToastProps};
use rndr_ui::menu::{open_menu_with, MenuAt, MenuEntry, MenuItem};
use rndr_ui::palette::{CommandPalette, PaletteProps};
use rndr_ui::selection::{SelectOption, SelectionProps, SingleSelect};
use rndr_ui::split::{PaneFill, SplitSash, SplitSashProps};
use rndr_ui::tree::{NodeId, TreeNode, TreeProps, VirtualTree};
use rndr_ui::*;

fn app(theme: UiTheme) -> (App, Entity) {
    let mut app = App::new();
    app.add_plugin(UiPlugin);
    app.world_mut().insert_resource(Viewport::new(800.0, 600.0));
    app.world_mut().insert_resource(theme);
    let root = app.world_mut().spawn((Layout {
        width: Size::Px(800.0),
        height: Size::Px(600.0),
        ..Layout::column()
    },));
    (app, root)
}

#[test]
fn glass_material_and_panel_are_translucent_in_both_themes_and_opaque_in_high_contrast() {
    for appearance in [Appearance::Light, Appearance::Dark] {
        let theme = UiTheme {
            appearance,
            ..UiTheme::default()
        };
        for token in [Token::Glass, Token::Material, Token::Panel] {
            let color = theme.color(token);
            assert!(
                color.a < 1.0 && color.a > 0.4,
                "{appearance:?} {token:?} alpha {}",
                color.a
            );
        }
        assert!(theme.color(Token::Material).a < theme.color(Token::Glass).a);
    }
    for appearance in [Appearance::Light, Appearance::Dark] {
        let theme = UiTheme {
            appearance,
            contrast: Contrast::High,
            ..UiTheme::default()
        };
        for token in [Token::Glass, Token::Material, Token::Panel] {
            assert_eq!(theme.color(token).a, 1.0, "{appearance:?} {token:?}");
        }
    }
}

#[test]
fn split_panes_follow_a_theme_surface_or_stay_clear() {
    for appearance in [Appearance::Light, Appearance::Dark] {
        let theme = UiTheme {
            appearance,
            ..UiTheme::default()
        };
        let (mut app, root) = app(theme.clone());
        let mut props = SplitSashProps::new("Panes");
        props.leading_fill = PaneFill::Theme(Surface::Glass);
        props.trailing_fill = PaneFill::Transparent;
        let handle = SplitSash::mount(app.world_mut(), root, UiId(1), props, 0.5).unwrap();
        app.update();
        assert_eq!(
            *app.world().get::<Color>(handle.leading).unwrap(),
            theme.color(Token::Glass)
        );
        assert_eq!(app.world().get::<Color>(handle.trailing).unwrap().a, 0.0);
        let mut props = SplitSashProps::new("Panes");
        props.leading_fill = PaneFill::Theme(Surface::Material);
        props.trailing_fill = PaneFill::Theme(Surface::Panel);
        let handle = SplitSash::mount(app.world_mut(), root, UiId(1), props, 0.5).unwrap();
        app.update();
        assert_eq!(
            *app.world().get::<Color>(handle.leading).unwrap(),
            theme.color(Token::Material)
        );
        assert_eq!(
            *app.world().get::<Color>(handle.trailing).unwrap(),
            theme.color(Token::Panel)
        );
    }
}

#[test]
fn quiet_controls_and_focus_gaps_follow_a_translucent_host() {
    for appearance in [Appearance::Light, Appearance::Dark] {
        let theme = UiTheme {
            appearance,
            ..UiTheme::default()
        };
        let (mut app, root) = app(theme.clone());
        let surface = Surface::Glass
            .mount(app.world_mut(), root, UiId(1))
            .unwrap();
        let mut props = ButtonProps::new("Quiet", HostIntent::Command(1));
        props.variant = ButtonVariant::Quiet;
        let button = Button::mount(app.world_mut(), surface.content, UiId(2), props).unwrap();
        app.update();
        let glass = theme.color(Token::Glass);
        let face = app.world().get::<Parent>(button.label).unwrap().entity;
        assert_eq!(*app.world().get::<Color>(face).unwrap(), glass);
        request_focus(app.world_mut(), button.root);
        app.update();
        let border = app.world().get::<Parent>(face).unwrap().entity;
        let gap = app.world().get::<Parent>(border).unwrap().entity;
        assert_eq!(*app.world().get::<Color>(gap).unwrap(), glass);
        assert_ne!(
            *app.world().get::<Color>(gap).unwrap(),
            theme.color(Token::Canvas)
        );
    }
}

#[test]
fn translucent_tree_rows_use_a_theme_wash() {
    for appearance in [Appearance::Light, Appearance::Dark] {
        let theme = UiTheme {
            appearance,
            ..UiTheme::default()
        };
        let (mut app, root) = app(theme.clone());
        let nodes = [TreeNode::new(1, "Endpoints")];
        let tree = VirtualTree::mount(
            app.world_mut(),
            root,
            UiId(1),
            TreeProps::new("Groups").translucent(),
            &nodes,
            &[],
        )
        .unwrap();
        if let Some(layout) = app.world_mut().get_mut::<Layout>(tree.root) {
            layout.width = Size::Px(320.0);
            layout.height = Size::Px(160.0);
        }
        app.update();
        app.update();
        tree.select(app.world_mut(), Some(NodeId(1))).unwrap();
        app.update();
        let row = tree.row(app.world(), NodeId(1)).unwrap();
        let color = *app.world().get::<Color>(row).unwrap();
        assert!(color.a > 0.05 && color.a < 0.3, "{color:?}");
        if appearance == Appearance::Dark {
            assert!(color.r > 0.9, "dark wash should be light: {color:?}");
        } else {
            assert!(color.r < 0.3, "light wash should be dark: {color:?}");
        }
        let recipe = app.world().get::<UiRecipe>(row).unwrap();
        match recipe.0.background {
            Some(UiColor::Explicit(explicit)) => assert_eq!(explicit, color),
            other => panic!("opaque token recipe {other:?}"),
        }
    }
}

#[test]
fn menus_palette_and_popovers_use_the_overlay_radius_and_optional_frost() {
    let (mut app, root) = app(UiTheme::default());
    let anchor = app.world_mut().spawn((Layout::column(), Focusable));
    set_parent(app.world_mut(), anchor, root);
    let menu = open_menu_with(
        app.world_mut(),
        anchor,
        MenuAt::Below,
        &[MenuEntry::from(MenuItem::new(1, "Contained"))],
        OverlayChrome::frost(),
    )
    .unwrap();
    app.update();
    assert_eq!(
        app.world().get::<Radius>(menu.root).map(|radius| radius.0),
        Some(Corner::Overlay.points())
    );
    assert!(app.world().get::<BackdropBlur>(menu.root).unwrap().sigma() > 0.0);
    assert!(!app.world().get::<Shadows>(menu.root).unwrap().0.is_empty());

    let mut props = PaletteProps::new("Commands");
    props.chrome = OverlayChrome::frost();
    let palette = CommandPalette::mount(app.world_mut(), anchor, UiId(3), props).unwrap();
    assert!(palette.open(app.world_mut()));
    app.update();
    assert_eq!(
        app.world()
            .get::<Radius>(palette.root)
            .map(|radius| radius.0),
        Some(Corner::Overlay.points())
    );
    assert!(app.world().get::<BackdropBlur>(palette.root).is_some());

    let mut select = SelectionProps::new("Status");
    select.chrome = OverlayChrome::frost();
    let control = SingleSelect::mount(
        app.world_mut(),
        root,
        UiId(4),
        select,
        &[SelectOption::new(1, "Contained")],
        None,
    )
    .unwrap();
    app.update();
    let popup = control.popup(app.world()).unwrap();
    assert_eq!(
        app.world().get::<Radius>(popup).map(|radius| radius.0),
        Some(Corner::Overlay.points())
    );

    let mut toast_props = ToastProps::new("Saved");
    toast_props.chrome = OverlayChrome::frost();
    let toast = Toast::mount(app.world_mut(), root, UiId(5), toast_props).unwrap();
    app.update();
    assert_eq!(
        app.world().get::<Radius>(toast.root).map(|radius| radius.0),
        Some(Corner::Overlay.points())
    );
    assert!(app.world().get::<BackdropBlur>(toast.root).is_some());
}
