use rndr_core::*;
use rndr_ui::{
    controls::{self, *},
    *,
};

fn ratio(a: Color, b: Color) -> f32 {
    fn luminance(c: Color) -> f32 {
        fn linear(v: f32) -> f32 {
            if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        }
        0.2126 * linear(c.r) + 0.7152 * linear(c.g) + 0.0722 * linear(c.b)
    }
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

#[test]
fn content_sized_actions_survive_reflow_scale_density_focus_and_activation() {
    for density in [Density::Comfortable, Density::Compact] {
        for scale in [1.0, 1.25, 2.0] {
            for text_scale in [1.0, 2.0] {
                let mut app = App::new();
                app.add_plugin(UiPlugin);
                app.world_mut().insert_resource(Viewport {
                    scale,
                    ..Viewport::new(1800., 2400.)
                });
                app.world_mut().insert_resource(UiTheme {
                    density,
                    text_scale,
                    ..UiTheme::default()
                });
                let host = app.world_mut().spawn((Layout {
                    width: Size::Px(1600.),
                    ..Layout::column()
                },));
                let toolbar = Toolbar::mount(app.world_mut(), host, UiId(1), 1600.).unwrap();
                let mut actions = Vec::new();
                for (i, label) in ["Add", "A longer desktop action", "+"].iter().enumerate() {
                    let h = controls::Button::mount(
                        app.world_mut(),
                        toolbar.root,
                        UiId(i as u64 + 2),
                        ButtonProps::new(*label, HostIntent::Command(i as u64)),
                    )
                    .unwrap();
                    app.world_mut().insert(h.root, ToolbarContentWidth);
                    actions.push(h);
                }
                app.update();
                assert!(request_focus(app.world_mut(), actions[0].root));
                for width in [1600., 120., 1600., 120., 1600.] {
                    app.world_mut().get_mut::<Layout>(host).unwrap().width = Size::Px(width);
                    Toolbar::mount(app.world_mut(), host, UiId(1), width).unwrap();
                    let frame = rndr_paint::frame(&mut app);
                    assert!(!frame.quads.is_empty());
                    let row = width == 1600.;
                    assert_eq!(
                        app.world().get::<Layout>(toolbar.root).unwrap().axis,
                        if row { Axis::Row } else { Axis::Column }
                    );
                    for h in &actions {
                        h.validate(app.world()).unwrap();
                        assert!(!app.world().has::<Hidden>(h.root));
                        let root = app.world().get::<Rect>(h.root).unwrap();
                        assert!(root.h >= app.world().resource::<UiTheme>().unwrap().target_min());
                        assert!(root.w <= width);
                        if row {
                            let style = style_for(app.world(), h.label);
                            let label = app.world().get::<Label>(h.label).unwrap();
                            let natural = text_engine(app.world()).layout(
                                &label.text,
                                &style,
                                &[],
                                f32::INFINITY,
                            );
                            let actual = app.world().get::<TextLayout>(h.label).unwrap();
                            // Compare final engine geometry, not our width formula.
                            assert_eq!(actual.lines, natural.lines);
                            assert_eq!(actual.glyphs, natural.glyphs);
                        } else {
                            assert_eq!(
                                app.world().get::<Layout>(h.root).unwrap().width,
                                Size::Fill
                            );
                        }
                    }
                    if row {
                        let a = app.world().get::<Rect>(actions[0].root).unwrap();
                        let b = app.world().get::<Rect>(actions[1].root).unwrap();
                        assert!(a.w < b.w && b.w < width / 2.);
                    }
                    assert_eq!(
                        app.world().resource::<Focus>().unwrap().entity,
                        Some(actions[0].root)
                    );
                    assert_eq!(
                        try_activate(app.world_mut(), actions[1].root),
                        ActivationResult::Accepted
                    );
                    let intents = take_action_intents(app.world_mut());
                    assert_eq!(intents.len(), 1);
                    assert_eq!(
                        host_intent(app.world(), &intents[0]),
                        Some(HostIntent::Command(1))
                    );
                }
                // Opt-in sizing never changes unmarked authored widths.
                app.world_mut()
                    .remove::<ToolbarContentWidth>(actions[0].root);
                app.world_mut()
                    .get_mut::<Layout>(actions[0].root)
                    .unwrap()
                    .width = Size::Px(93.);
                app.update();
                assert_eq!(
                    app.world().get::<Layout>(actions[0].root).unwrap().width,
                    Size::Px(93.)
                );
            }
        }
    }
}

#[test]
fn semantic_type_and_editor_recipe_resolve_readably_in_every_palette() {
    for appearance in [Appearance::Light, Appearance::Dark] {
        for contrast in [Contrast::Standard, Contrast::High] {
            let theme = UiTheme {
                appearance,
                contrast,
                ..UiTheme::default()
            };
            assert_eq!(theme.typography(Typography::Label).family, "Inter");
            assert_eq!(theme.typography(Typography::Label).size, 14.);
            assert_eq!(theme.typography(Typography::Emphasis).weight, 600);
            assert!(
                theme.typography(Typography::Title).size
                    > theme.typography(Typography::Heading).size
            );
            for background in [
                Token::Content,
                Token::Canvas,
                Token::ControlHover,
                Token::ControlPressed,
                Token::Selected,
            ] {
                for foreground in [Token::Text, Token::Secondary] {
                    assert!(
                        ratio(theme.color(background), theme.color(foreground)) >= 4.5,
                        "{appearance:?} {contrast:?} {background:?}/{foreground:?}"
                    );
                }
            }
            let mut app = App::new();
            app.add_plugin(UiPlugin);
            app.world_mut().insert_resource(theme.clone());
            let root = app.world_mut().spawn((Layout {
                width: Size::Px(600.),
                ..Layout::column()
            },));
            let secondary = text(
                app.world_mut(),
                root,
                "Supporting information",
                Typography::Secondary,
            )
            .unwrap();
            let editor = app.world_mut().spawn((
                Layout {
                    width: Size::Fill,
                    height: Size::Px(100.),
                    ..Layout::column()
                },
                Editor::default(),
                Buffer::from_str("Readable source"),
                Label::new(""),
                UiRecipe(UiStyle::editor()),
            ));
            set_parent(app.world_mut(), editor, root);
            let frame = rndr_paint::frame(&mut app);
            let foreground = app.world().get::<Label>(editor).unwrap().color.unwrap();
            assert_eq!(foreground, theme.color(Token::Text));
            assert_eq!(
                app.world().get::<Color>(editor),
                Some(&theme.color(Token::Content))
            );
            assert!(ratio(foreground, theme.color(Token::Content)) >= 4.5);
            assert_eq!(
                app.world().get::<Label>(secondary).unwrap().color,
                Some(theme.color(Token::Secondary))
            );
            assert_eq!(
                app.world().get::<TextStyle>(editor).unwrap().family,
                theme.code_family
            );
            assert!(frame
                .quads
                .iter()
                .any(|q| q.uv != rndr_paint::solid_uv() && q.color == foreground));
        }
    }
}

#[test]
fn hover_is_neutral_focus_is_reserved_and_selection_remains_distinct() {
    let mut app = App::new();
    app.add_plugin(UiPlugin);
    let theme = UiTheme::default();
    let root = app.world_mut().spawn((Layout {
        width: Size::Px(300.),
        ..Layout::column()
    },));
    let mut props = ButtonProps::new("Quiet action", HostIntent::Command(1));
    props.variant = ButtonVariant::Quiet;
    let h = controls::Button::mount(app.world_mut(), root, UiId(1), props).unwrap();
    let nav = NavigationRow::mount(
        app.world_mut(),
        root,
        UiId(2),
        ButtonProps::new("Selected note", HostIntent::Command(2)),
        NavigationMeta {
            selected: true,
            ..NavigationMeta::default()
        },
    )
    .unwrap();
    app.update();
    let face = app.world().get::<Parent>(h.label).unwrap().entity;
    let nav_face = app.world().get::<Parent>(nav.label).unwrap().entity;
    assert_eq!(
        app.world().get::<Color>(face),
        Some(&theme.color(Token::Canvas))
    );
    assert_eq!(
        app.world().get::<Color>(nav_face),
        Some(&theme.color(Token::Selected))
    );
    let r = *app.world().get::<Rect>(h.root).unwrap();
    app.world_mut()
        .resource_mut::<Input>()
        .unwrap()
        .pointer
        .position = [r.x + 12., r.y + 12.];
    let frame = rndr_paint::frame(&mut app);
    assert_eq!(
        app.world().get::<Color>(face),
        Some(&theme.color(Token::ControlHover))
    );
    assert_eq!(app.world().get::<Color>(h.root).unwrap().a, 0.);
    assert!(request_focus(app.world_mut(), h.root));
    let focused = rndr_paint::frame(&mut app);
    assert!(focused
        .quads
        .iter()
        .any(|q| q.rect == r && q.color == theme.color(Token::Focus)));
    assert!(!frame
        .quads
        .iter()
        .any(|q| q.rect == r && q.color == theme.color(Token::Focus)));
}
