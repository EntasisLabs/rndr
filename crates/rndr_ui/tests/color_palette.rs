use rndr_core::*;
use rndr_ui::{controls::*, *};

fn ratio(a: Color, b: Color) -> f32 {
    fn lum(c: Color) -> f32 {
        let linear = |v: f32| {
            if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        linear(c.r) * 0.2126 + linear(c.g) * 0.7152 + linear(c.b) * 0.0722
    }
    let (a, b) = (lum(a), lum(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

#[test]
fn text_tiers_descend_and_stay_legible_on_shell_surfaces() {
    use Token::*;
    for appearance in [Appearance::Light, Appearance::Dark] {
        let theme = UiTheme {
            appearance,
            ..UiTheme::default()
        };
        for surface in [Canvas, Navigation, Content, Raised] {
            let on = |ink| ratio(theme.color(ink), theme.color(surface));
            let ladder = [Text, Secondary, Tertiary, Quiet, Faint].map(on);
            assert!(
                ladder.windows(2).all(|pair| pair[0] > pair[1]),
                "{appearance:?} on {surface:?}: {ladder:?}"
            );
            assert!(ladder[2] >= 4.5, "{appearance:?} Tertiary on {surface:?}");
            assert!(ladder[4] >= 3.0, "{appearance:?} Faint on {surface:?}");
        }
        assert_eq!(theme.color(Link), theme.color(Accent));
        assert_eq!(theme.markdown_theme().link, theme.color(Link));
    }
}

fn selected_row(theme: UiTheme) -> (App, ControlHandle) {
    let mut a = App::new();
    a.add_plugin(UiPlugin);
    a.world_mut().insert_resource(InteractionMotion::snap());
    a.world_mut().insert_resource(Viewport::new(240., 800.));
    a.world_mut().insert_resource(theme);
    let root = a.world_mut().spawn((Layout {
        width: Size::Px(240.),
        ..Layout::column()
    },));
    let row = NavigationRow::mount(
        a.world_mut(),
        root,
        UiId(1),
        ButtonProps::new("Project notes", HostIntent::Command(1)),
        NavigationMeta {
            selected: true,
            dirty: false,
            secondary: Some("Draft".into()),
        },
    )
    .unwrap();
    a.update();
    (a, row)
}

fn bar_and_meta(a: &App, row: &ControlHandle) -> (Color, Color) {
    let bar = children_of(a.world(), row.root)
        .into_iter()
        .find(|e| a.world().has::<Absolute>(*e))
        .unwrap();
    let face = a.world().get::<Parent>(row.label).unwrap().entity;
    let meta = children_of(a.world(), face)
        .into_iter()
        .find(|e| *e != row.label)
        .unwrap();
    (
        *a.world().get::<Color>(bar).unwrap(),
        a.world().get::<Label>(meta).unwrap().color.unwrap(),
    )
}

#[test]
fn custom_palette_reaches_builtin_controls_and_follows_appearance() {
    let light_accent = Color::rgb(0.8, 0.1, 0.5);
    let light_meta = Color::rgb(0.3, 0.3, 0.2);
    let dark_accent = Color::rgb(0.9, 0.6, 0.2);
    let dark_meta = Color::rgb(0.7, 0.7, 0.6);
    let palette = ColorPalette::default()
        .with(Appearance::Light, Token::Accent, light_accent)
        .with(Appearance::Light, Token::Secondary, light_meta)
        .with(Appearance::Dark, Token::Accent, dark_accent)
        .with(Appearance::Dark, Token::Secondary, dark_meta);
    let theme = UiTheme {
        palette,
        ..UiTheme::default()
    };
    let (mut a, row) = selected_row(theme.clone());
    assert_eq!(bar_and_meta(&a, &row), (light_accent, light_meta));

    a.world_mut().insert_resource(UiTheme {
        appearance: Appearance::Dark,
        ..theme.clone()
    });
    a.update();
    assert_eq!(bar_and_meta(&a, &row), (dark_accent, dark_meta));

    // Increased contrast is an accessibility guarantee, not an app color.
    let high = UiTheme {
        contrast: Contrast::High,
        ..theme
    };
    a.world_mut().insert_resource(high.clone());
    a.update();
    let standard = UiTheme {
        contrast: Contrast::High,
        ..UiTheme::default()
    };
    assert_eq!(
        bar_and_meta(&a, &row),
        (
            standard.color(Token::Accent),
            standard.color(Token::Secondary)
        )
    );
    assert_eq!(high.core_theme().pane, standard.core_theme().pane);
}

#[test]
fn bridge_carries_palette_into_core_and_markdown_themes() {
    let pane = Color::rgb(0.05, 0.05, 0.08);
    let link = Color::rgb(0.6, 0.4, 0.9);
    let theme = UiTheme {
        appearance: Appearance::Dark,
        palette: ColorPalette::default()
            .with(Appearance::Dark, Token::Content, pane)
            .with(Appearance::Dark, Token::Link, link),
        ..UiTheme::default()
    };
    let mut world = World::new();
    apply_theme_bridge(&mut world, theme);
    assert_eq!(world.resource::<Theme>().unwrap().pane, pane);
    let markdown = world.resource::<MarkdownTheme>().unwrap();
    assert_eq!((markdown.surface, markdown.link), (pane, link));
}
