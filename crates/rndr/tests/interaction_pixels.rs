//! Settled hover, press, focus, and truncation pixels in light and dark.
#![cfg(feature = "parley")]
use rndr_core::*;
use rndr_render::{encode_png, Exporter};
use rndr_ui::controls::{Button, ButtonProps, HostIntent};
use rndr_ui::{Appearance, InteractionMotion, UiId, UiPlugin, UiTheme};

fn scene(light: bool) -> App {
    let mut app = App::new();
    app.add_plugin(UiPlugin);
    app.world_mut().insert_resource(InteractionMotion::snap());
    let mut viewport = Viewport::new(360.0, 160.0);
    viewport.scale = 2.0;
    app.world_mut().insert_resource(viewport);
    let engine: std::sync::Arc<dyn TextEngine> =
        std::sync::Arc::new(rndr_render::ParleyText::bundled());
    app.world_mut().insert_resource(engine);
    let theme = UiTheme {
        appearance: if light {
            Appearance::Light
        } else {
            Appearance::Dark
        },
        ..UiTheme::default()
    };
    let canvas = theme.color(rndr_ui::Token::Canvas);
    app.world_mut().insert_resource(theme);
    let root = app.world_mut().spawn((
        Layout {
            width: Size::Fill,
            height: Size::Fill,
            gap: 8.0,
            padding: 12.0,
            ..Layout::column()
        },
        canvas,
    ));
    let hover = Button::mount(
        app.world_mut(),
        root,
        UiId(1),
        ButtonProps::new("Hover me", HostIntent::Command(1)),
    )
    .unwrap();
    let narrow = Button::mount(
        app.world_mut(),
        root,
        UiId(2),
        ButtonProps::new(
            "Truncated label that does not fit this control",
            HostIntent::Command(2),
        ),
    )
    .unwrap();
    app.world_mut()
        .get_mut::<Layout>(narrow.root)
        .unwrap()
        .width = Size::Px(96.0);
    app.world_mut().insert_resource(Scene {
        hover: hover.root,
        hover_label: hover.label,
        narrow_label: narrow.label,
    });
    app
}

#[derive(Clone, Copy)]
struct Scene {
    hover: Entity,
    hover_label: Entity,
    narrow_label: Entity,
}
impl Component for Scene {}

fn sample(image: &rndr_render::RgbaImage, x: f32, y: f32, scale: f32) -> [u8; 3] {
    let px = (x * scale) as u32;
    let py = (y * scale) as u32;
    let i = ((py * image.width + px) * 4) as usize;
    [image.rgba[i], image.rgba[i + 1], image.rgba[i + 2]]
}

fn near(pixel: [u8; 3], color: Color) -> bool {
    let channel = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as i32;
    let expected = [channel(color.r), channel(color.g), channel(color.b)];
    pixel
        .iter()
        .zip(expected)
        .all(|(got, want)| (*got as i32 - want).abs() <= 2)
}

#[test]
fn hover_press_focus_and_truncation_pixels_in_light_and_dark() {
    let mut gpu = Exporter::new().expect("a real headless graphics adapter");
    for light in [true, false] {
        let mut app = scene(light);
        let theme = app.world().resource::<UiTheme>().cloned().unwrap();
        app.update();
        let scene = *app.world().resource::<Scene>().unwrap();
        let hover_rect = *app.world().get::<Rect>(scene.hover).unwrap();
        let at = [hover_rect.x + 16.0, hover_rect.y + 10.0];
        {
            let input = app.world_mut().resource_mut::<Input>().unwrap();
            input.pointer.position = at;
        }
        let frame = rndr_paint::frame(&mut app);
        let image = gpu
            .render_frame(Some(app.world()), &frame, 720, 320, 2.0)
            .unwrap();
        let face = app.world().get::<Parent>(scene.hover_label).unwrap().entity;
        let face_rect = *app.world().get::<Rect>(face).unwrap();
        let hover_pixel = sample(
            &image,
            face_rect.x + face_rect.w * 0.5,
            face_rect.y + face_rect.h * 0.5,
            2.0,
        );
        assert!(
            near(hover_pixel, theme.color(rndr_ui::Token::ControlHover)),
            "hover {hover_pixel:?} light={light}"
        );

        {
            let input = app.world_mut().resource_mut::<Input>().unwrap();
            input.pointer.position = at;
            input.pointer.pressed = true;
            input.pointer.down = true;
            input.pointer.press_at = Some(at);
        }
        let frame = rndr_paint::frame(&mut app);
        let image = gpu
            .render_frame(Some(app.world()), &frame, 720, 320, 2.0)
            .unwrap();
        let press_pixel = sample(
            &image,
            face_rect.x + face_rect.w * 0.5,
            face_rect.y + face_rect.h * 0.5,
            2.0,
        );
        assert!(
            near(press_pixel, theme.color(rndr_ui::Token::ControlPressed)),
            "press {press_pixel:?} light={light}"
        );
        assert_ne!(hover_pixel, press_pixel);

        {
            let input = app.world_mut().resource_mut::<Input>().unwrap();
            input.pointer.pressed = false;
            input.pointer.down = false;
            input.pointer.position = [-10.0, -10.0];
        }
        assert!(request_focus(app.world_mut(), scene.hover));
        let frame = rndr_paint::frame(&mut app);
        let image = gpu
            .render_frame(Some(app.world()), &frame, 720, 320, 2.0)
            .unwrap();
        let ring = sample(
            &image,
            hover_rect.x + hover_rect.w * 0.5,
            hover_rect.y + 1.0,
            2.0,
        );
        assert!(
            near(ring, theme.color(rndr_ui::Token::Focus)),
            "focus {ring:?} light={light}"
        );

        let label = app.world().get::<Label>(scene.narrow_label).unwrap();
        let layout = app.world().get::<TextLayout>(scene.narrow_label).unwrap();
        let rect = *app.world().get::<Rect>(scene.narrow_label).unwrap();
        assert_eq!(label.overflow, TextOverflow::Ellipsis);
        assert_eq!(layout.lines.len(), 1);
        assert!(layout.width <= rect.w + 0.5);
        let style = style_for(app.world(), scene.narrow_label);
        let natural = text_engine(app.world())
            .measure(&label.text, &style, &[], f32::INFINITY)
            .width;
        assert!(natural > layout.width + 4.0, "light={light}");

        if let Some(dir) = std::env::var_os("RNDR_DIFF_CAPTURE_DIR") {
            let dir = std::path::PathBuf::from(dir);
            std::fs::create_dir_all(&dir).unwrap();
            let name = if light { "light" } else { "dark" };
            std::fs::write(
                dir.join(format!("interaction-{name}.png")),
                encode_png(&image).unwrap(),
            )
            .unwrap();
        }
    }
}
