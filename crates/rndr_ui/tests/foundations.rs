use rndr_core::*;
use rndr_ui::*;
#[path = "support/gallery.rs"]
mod gallery;

fn base() -> (App, Entity) {
    let mut app = App::new();
    app.add_plugin(Chrome)
        .add_plugin(UiPlugin)
        .add_plugin(UiPlugin);
    app.world_mut()
        .insert_resource(Viewport::new(800.0, 6000.0));
    let root = app.world_mut().spawn((Layout {
        width: Size::Fill,
        ..Layout::column()
    },));
    (app, root)
}
#[test]
fn install_and_unopted_legacy_appearance_are_unchanged() {
    let (mut app, root) = base();
    let explicit = Color::rgba(0.1, 0.2, 0.3, 0.0);
    let legacy = app.world_mut().spawn((
        Layout {
            width: Size::Px(80.0),
            height: Size::Px(30.0),
            ..Layout::column()
        },
        explicit,
        Label::new("legacy"),
    ));
    set_parent(app.world_mut(), legacy, root);
    app.update();
    let before = rndr_paint::paint(app.world());
    app.world_mut()
        .resource_mut::<UiTheme>()
        .unwrap()
        .appearance = Appearance::Dark;
    app.update();
    let after = rndr_paint::paint(app.world());
    assert_eq!(format!("{before:?}"), format!("{after:?}"));
    assert_eq!(app.world().get::<Color>(legacy), Some(&explicit));
    assert!(!app.world().has_resource::<Selection>());
    assert_eq!(app.world().resource::<Actions>().unwrap().bindings.len(), 3);
    assert!(app.world().resource::<Theme>().is_none());
}
#[test]
fn theme_override_ownership_transparency_takeover_and_detach() {
    let (mut app, root) = base();
    let handle = Card::mount(app.world_mut(), root, UiId(1), "Card").unwrap();
    let label = text(app.world_mut(), handle.content, "body", Typography::Reading).unwrap();
    handle.adopt(app.world_mut(), label).unwrap();
    app.update();
    let light = *app.world().get::<Color>(handle.root).unwrap();
    let label_id = access_id(label);
    app.world_mut()
        .resource_mut::<UiTheme>()
        .unwrap()
        .appearance = Appearance::Dark;
    app.update();
    assert_ne!(app.world().get::<Color>(handle.root), Some(&light));
    assert_eq!(access_id(label), label_id);
    let transparent = Color::rgba(0.8, 0.1, 0.2, 0.0);
    app.world_mut().insert(
        handle.root,
        UiStyle {
            background: Some(UiColor::Explicit(transparent)),
            ..UiStyle::default()
        },
    );
    assert_eq!(
        Card::mount(app.world_mut(), root, UiId(1), "updated").unwrap(),
        handle
    );
    app.update();
    assert_eq!(app.world().get::<Color>(handle.root), Some(&transparent));
    // Direct app mutation relinquishes only that property, not text or gap.
    let takeover = Color::rgb(0.3, 0.4, 0.5);
    app.world_mut().insert(handle.root, takeover);
    app.world_mut()
        .resource_mut::<UiTheme>()
        .unwrap()
        .appearance = Appearance::Light;
    app.update();
    assert_eq!(app.world().get::<Color>(handle.root), Some(&takeover));
    detach_style(app.world_mut(), handle.root);
    assert_eq!(app.world().get::<Color>(handle.root), Some(&takeover));
    assert!(app.world().get::<Radius>(handle.root).is_none());
    assert_eq!(app.world().get::<Layout>(handle.root).unwrap().padding, 0.0);
    detach_style(app.world_mut(), label);
    assert!(app.world().get::<TextStyle>(label).is_none());
    assert!(app.world().get::<Label>(label).unwrap().color.is_none());
}
#[test]
fn existing_primitives_win_and_property_reattach_is_fresh() {
    let (mut app, root) = base();
    let authored = Color::rgba(0.2, 0.3, 0.4, 0.0);
    let entity = app.world_mut().spawn((
        Layout::column(),
        authored,
        Label::new("explicit").with_color(authored),
        TextStyle {
            size: 19.0,
            ..TextStyle::default()
        },
        UiStyle {
            background: Some(Token::Content.into()),
            foreground: Some(Token::Text.into()),
            typography: Some(Typography::Label),
            ..UiStyle::default()
        },
    ));
    set_parent(app.world_mut(), entity, root);
    app.update();
    assert_eq!(app.world().get::<Color>(entity), Some(&authored));
    assert_eq!(
        app.world().get::<Label>(entity).unwrap().color,
        Some(authored)
    );
    assert_eq!(app.world().get::<TextStyle>(entity).unwrap().size, 19.0);
    let fresh = app.world_mut().spawn((
        Layout::column(),
        UiStyle {
            background: Some(Token::Raised.into()),
            ..UiStyle::default()
        },
    ));
    app.update();
    assert!(app.world().has::<Color>(fresh));
    app.world_mut()
        .get_mut::<UiStyle>(fresh)
        .unwrap()
        .background = None;
    app.update();
    assert!(!app.world().has::<Color>(fresh));
    app.world_mut().insert(fresh, authored);
    app.world_mut()
        .get_mut::<UiStyle>(fresh)
        .unwrap()
        .background = Some(Token::Canvas.into());
    app.update();
    assert_eq!(app.world().get::<Color>(fresh), Some(&authored));
}
#[test]
fn stable_lifecycle_borrowed_and_adopted_ownership_stale_handles() {
    let (mut app, root) = base();
    let handle = Card::mount(app.world_mut(), root, UiId(9), "before").unwrap();
    app.world_mut().insert(
        handle.content,
        Scroll {
            offset: [0.0, 37.0],
            ..Scroll::default()
        },
    );
    let borrowed = app
        .world_mut()
        .spawn((Layout::column(), Label::new("borrowed"), Focusable));
    set_parent(app.world_mut(), borrowed, handle.content);
    let owned = app
        .world_mut()
        .spawn((Layout::column(), Label::new("owned"), Focusable));
    handle.adopt(app.world_mut(), owned).unwrap();
    app.world_mut().insert_resource(Focus {
        entity: Some(borrowed),
    });
    let update = Card::mount(app.world_mut(), root, UiId(9), "after").unwrap();
    assert_eq!(update, handle);
    assert_eq!(
        app.world().get::<Scroll>(handle.content).unwrap().offset,
        [0.0, 37.0]
    );
    assert_eq!(
        app.world().resource::<Focus>().unwrap().entity,
        Some(borrowed)
    );
    assert_eq!(
        Callout::mount(app.world_mut(), root, UiId(9), Feedback::Info, "collision"),
        Err(UiError::IdentityConflict)
    );
    handle.destroy(app.world_mut()).unwrap();
    assert!(!app.world().is_alive(owned));
    assert!(app.world().is_alive(borrowed));
    assert!(app.world().get::<Parent>(borrowed).is_none());
    assert_eq!(
        app.world().resource::<Focus>().unwrap().entity,
        Some(borrowed)
    );
    assert_eq!(
        handle.set_title(app.world_mut(), "stale"),
        Err(UiError::Stale)
    );
    let replacement = Card::mount(app.world_mut(), root, UiId(9), "replacement").unwrap();
    assert_ne!(replacement, handle);
    request_focus(app.world_mut(), replacement.root); // unsupported passive surface, no focus stealing
    assert_eq!(
        app.world().resource::<Focus>().unwrap().entity,
        Some(borrowed)
    );
}
#[test]
fn owned_teardown_cleans_only_owned_focus_capture_queue_and_access_nodes() {
    let (mut app, root) = base();
    let handle = Card::mount(app.world_mut(), root, UiId(2), "container").unwrap();
    let owned = app
        .world_mut()
        .spawn((Layout::column(), Button::new("owned"), ReleaseActivation));
    handle.adopt(app.world_mut(), owned).unwrap();
    let other = app
        .world_mut()
        .spawn((Layout::column(), Button::new("other"), ReleaseActivation));
    request_focus(app.world_mut(), owned);
    try_activate(app.world_mut(), owned);
    try_activate(app.world_mut(), other);
    handle.destroy(app.world_mut()).unwrap();
    assert!(app.world().resource::<Focus>().unwrap().entity.is_none());
    let intents = take_action_intents(app.world_mut());
    assert_eq!(intents.len(), 1);
    assert_eq!(intents[0].entity, other);
    assert!(!access_tree(app.world())
        .nodes
        .iter()
        .any(|n| n.id == access_id(owned)));
}
fn rgb(color: Color) -> [f32; 3] {
    [color.r, color.g, color.b]
}
fn lum(c: [f32; 3]) -> f32 {
    fn linear(v: f32) -> f32 {
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    }
    linear(c[0]) * 0.2126 + linear(c[1]) * 0.7152 + linear(c[2]) * 0.0722
}
fn contrast(a: Color, b: Color) -> f32 {
    let a = lum(rgb(a));
    let b = lum(rgb(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}
#[test]
fn resolved_documented_pairs_have_required_contrast() {
    use Token::*;
    for appearance in [Appearance::Light, Appearance::Dark] {
        for mode in [Contrast::Standard, Contrast::High] {
            let theme = UiTheme {
                appearance,
                contrast: mode,
                ..UiTheme::default()
            };
            for foreground in [Text, Secondary] {
                for background in [Canvas, Navigation, Content, Raised, Overlay, Selected] {
                    assert!(
                        contrast(theme.color(foreground), theme.color(background)) >= 4.5,
                        "{appearance:?}/{mode:?}: {foreground:?}/{background:?}"
                    );
                }
            }
            for background in [Accent, Hover, Pressed] {
                assert!(contrast(theme.color(OnAccent), theme.color(background)) >= 4.5);
            }
            for (fg, bg, stroke) in [
                (InfoText, InfoFill, InfoStroke),
                (SuccessText, SuccessFill, SuccessStroke),
                (WarningText, WarningFill, WarningStroke),
                (DangerText, DangerFill, DangerStroke),
            ] {
                assert!(contrast(theme.color(fg), theme.color(bg)) >= 4.5);
                assert!(contrast(theme.color(stroke), theme.color(bg)) >= 3.0);
            }
            for bg in [Canvas, Navigation, Content, Raised, Overlay, Selected] {
                assert!(contrast(theme.color(Focus), theme.color(bg)) >= 3.0);
            }
        }
    }
}
#[test]
fn headless_gallery_emits_real_glyphs_fills_wrapped_layout_at_200_percent() {
    for appearance in [Appearance::Light, Appearance::Dark] {
        for mode in [Contrast::Standard, Contrast::High] {
            for density in [Density::Comfortable, Density::Compact] {
                for scale in [1.0, 2.0] {
                    for width in [280.0, 800.0] {
                        let theme = UiTheme {
                            appearance,
                            contrast: mode,
                            density,
                            text_scale: scale,
                            reading_scale: scale,
                            ..UiTheme::default()
                        };
                        let (mut app, card, body) = gallery::scene(theme.clone(), width);
                        let frame = rndr_paint::frame(&mut app);
                        assert!(frame.quads.iter().any(|q| q.uv == rndr_paint::solid_uv()));
                        assert!(
                            frame
                                .quads
                                .iter()
                                .filter(|q| q.uv != rndr_paint::solid_uv())
                                .count()
                                > 100
                        );
                        let fill = theme.color(Token::Raised);
                        assert!(frame.quads.iter().any(|q| q.color == fill));
                        let body_rect = *app.world().get::<Rect>(body).unwrap();
                        let card_rect = *app.world().get::<Rect>(card.root).unwrap();
                        assert!(body_rect.h >= theme.typography(Typography::Reading).size);
                        assert!(
                            body_rect.x >= card_rect.x
                                && body_rect.x + body_rect.w <= card_rect.x + card_rect.w + 0.01
                        );
                        for e in app.world().entities_with::<Rect>() {
                            let r = app.world().get::<Rect>(e).unwrap();
                            assert!(
                                r.x.is_finite()
                                    && r.y.is_finite()
                                    && r.w.is_finite()
                                    && r.h.is_finite()
                            );
                            assert!(r.w >= 0.0 && r.h >= 0.0);
                        }
                        let before = body_rect;
                        let focus_before = app.world().resource::<Focus>().unwrap().entity;
                        let updated = UiTheme {
                            appearance: if appearance == Appearance::Light {
                                Appearance::Dark
                            } else {
                                Appearance::Light
                            },
                            density: if density == Density::Compact {
                                Density::Comfortable
                            } else {
                                Density::Compact
                            },
                            ..theme.clone()
                        };
                        app.world_mut().insert_resource(updated);
                        let _ = rndr_paint::frame(&mut app);
                        assert!(card.validate(app.world()).is_ok());
                        assert_eq!(
                            focus_before,
                            app.world().resource::<Focus>().unwrap().entity
                        );
                        assert_eq!(
                            app.world().get::<TextStyle>(body).unwrap().size,
                            theme.typography(Typography::Reading).size
                        );
                        assert!(before.h > 0.0);
                    }
                }
            }
        }
    }
}
#[test]
fn density_does_not_shrink_type_scaling_and_invalid_preferences_are_bounded() {
    let normal = UiTheme::default();
    let compact = UiTheme {
        density: Density::Compact,
        ..normal.clone()
    };
    assert_eq!(
        normal.typography(Typography::Label),
        compact.typography(Typography::Label)
    );
    assert!(compact.space(Space::X16) < normal.space(Space::X16));
    assert_eq!(compact.target_min(), 28.0);
    let invalid = UiTheme {
        text_scale: f32::NAN,
        reading_scale: f32::INFINITY,
        ..normal
    };
    assert_eq!(invalid.typography(Typography::Label).size, 14.0);
    assert_eq!(reading_layout(-50.0, &invalid).width, Size::Px(0.0));
}
