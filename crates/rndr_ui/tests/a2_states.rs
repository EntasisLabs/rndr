use rndr_core::*;
use rndr_ui::{
    controls::{self, *},
    *,
};
fn ratio(a: Color, b: Color) -> f32 {
    fn l(c: Color) -> f32 {
        fn f(v: f32) -> f32 {
            if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        }
        0.2126 * f(c.r) + 0.7152 * f(c.g) + 0.0722 * f(c.b)
    }
    let a = l(a);
    let b = l(b);
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}
#[test]
fn actual_resolved_control_states_contrast_focus_geometry_and_glyph_scale() {
    for appearance in [Appearance::Light, Appearance::Dark] {
        for contrast in [Contrast::Standard, Contrast::High] {
            for variant in [
                ButtonVariant::Primary,
                ButtonVariant::Secondary,
                ButtonVariant::Quiet,
                ButtonVariant::Destructive,
            ] {
                for state in 0..6 {
                    let theme = UiTheme {
                        appearance,
                        contrast,
                        ..UiTheme::default()
                    };
                    let mut a = App::new();
                    a.add_plugin(UiPlugin);
                    a.world_mut().insert_resource(theme.clone());
                    let root = a.world_mut().spawn((Layout {
                        width: Size::Px(240.0),
                        ..Layout::column()
                    },));
                    let mut p = ButtonProps::new("Operate", HostIntent::Command(1));
                    p.variant = variant;
                    p.enabled = state != 4;
                    p.readonly = state == 5;
                    let h = controls::Button::mount(a.world_mut(), root, UiId(1), p).unwrap();
                    a.update();
                    let r = *a.world().get::<Rect>(h.root).unwrap();
                    if state == 1 || state == 2 {
                        let i = a.world_mut().resource_mut::<Input>().unwrap();
                        i.pointer.position = [r.x + 12.0, r.y + 12.0];
                        if state == 2 {
                            i.pointer.pressed = true;
                            i.pointer.down = true;
                            i.pointer.press_at = Some(i.pointer.position);
                        }
                    }
                    if state == 3 {
                        assert!(request_focus(a.world_mut(), h.root));
                    }
                    let f = rndr_paint::frame(&mut a);
                    let face = a.world().get::<Parent>(h.label).unwrap().entity;
                    let border = a.world().get::<Parent>(face).unwrap().entity;
                    let gap = a.world().get::<Parent>(border).unwrap().entity;
                    let fill = *a.world().get::<Color>(face).unwrap();
                    let ink = a.world().get::<Label>(h.label).unwrap().color.unwrap();
                    assert!(
                        ratio(fill, ink) >= 4.5,
                        "actual resolved {appearance:?} {contrast:?} {variant:?} state{state}: {}",
                        ratio(fill, ink)
                    );
                    assert!(f
                        .quads
                        .iter()
                        .any(|q| q.uv == rndr_paint::solid_uv() && q.color == fill));
                    assert!(f
                        .quads
                        .iter()
                        .any(|q| q.uv != rndr_paint::solid_uv() && q.color == ink));
                    if state == 3 {
                        let gr = *a.world().get::<Rect>(gap).unwrap();
                        let br = *a.world().get::<Rect>(border).unwrap();
                        assert!((gr.x - r.x - 2.0).abs() < 0.01);
                        assert!((br.x - gr.x - 2.0).abs() < 0.01);
                        assert!(f
                            .quads
                            .iter()
                            .any(|q| q.rect == r && q.color == theme.color(Token::Focus)));
                        assert!(f
                            .quads
                            .iter()
                            .any(|q| q.rect == gr && q.color == theme.color(Token::Canvas)));
                        assert!(
                            ratio(theme.color(Token::Focus), theme.color(Token::Canvas)) >= 3.0
                        );
                    }
                    assert_eq!(interaction_pressed(a.world(), h.root), state == 2);
                }
            }
        }
    }
    let mut heights = vec![];
    for scale in [1.0, 2.0] {
        let mut a = App::new();
        a.add_plugin(UiPlugin);
        a.world_mut().insert_resource(UiTheme {
            text_scale: scale,
            ..UiTheme::default()
        });
        let root = a.world_mut().spawn((Layout {
            width: Size::Px(400.0),
            ..Layout::column()
        },));
        controls::Button::mount(
            a.world_mut(),
            root,
            UiId(1),
            ButtonProps::new("MMMM", HostIntent::Command(1)),
        )
        .unwrap();
        let f = rndr_paint::frame(&mut a);
        heights.push(
            f.quads
                .iter()
                .find(|q| q.uv != rndr_paint::solid_uv())
                .unwrap()
                .rect
                .h,
        );
    }
    assert!((heights[1] - heights[0] * 2.0).abs() < 0.01);
}
#[test]
fn style_override_detach_and_theme_changes_do_not_reconstruct_controls() {
    let mut a = App::new();
    a.add_plugin(UiPlugin);
    let root = a.world_mut().spawn((Layout {
        width: Size::Px(320.0),
        ..Layout::column()
    },));
    let h = controls::Button::mount(
        a.world_mut(),
        root,
        UiId(1),
        ButtonProps::new("Action", HostIntent::Command(1)),
    )
    .unwrap();
    let custom = Color::rgb(0.2, 0.3, 0.4);
    a.world_mut().insert(
        h.root,
        UiStyle {
            background: Some(UiColor::Explicit(custom)),
            ..UiStyle::default()
        },
    );
    a.update();
    assert_eq!(a.world().get::<Color>(h.root), Some(&custom));
    a.world_mut().insert_resource(UiTheme {
        appearance: Appearance::Dark,
        ..UiTheme::default()
    });
    a.update();
    assert_eq!(a.world().get::<Color>(h.root), Some(&custom));
    h.validate(a.world()).unwrap();
    a.world_mut().remove::<UiStyle>(h.root);
    a.update();
    assert_eq!(
        *a.world().get::<Color>(h.root).unwrap(),
        // Inactive focus layers no longer stamp canvas onto raised surfaces.
        Color::rgba(0.0, 0.0, 0.0, 0.0)
    );
    assert!(request_focus(a.world_mut(), h.root));
    a.update();
    assert_eq!(
        *a.world().get::<Color>(h.root).unwrap(),
        a.world().resource::<UiTheme>().unwrap().color(Token::Focus)
    );
    h.destroy(a.world_mut()).unwrap();
    assert_eq!(h.validate(a.world()), Err(UiError::Stale));
}
