//! Pixel check of the table grid, same offscreen path as the diff fixture.
use std::sync::Arc;

use rndr_core::*;
use rndr_markdown_ui::*;
use rndr_render::{encode_png, Exporter, ParleyText};
use rndr_ui::{Appearance, Token, UiId, UiTheme};

fn fixture() -> (App, MarkdownSurfaceHandle) {
    let mut app = App::new();
    app.add_plugin(rndr_markdown_ui::MarkdownPlugin);
    let mut viewport = Viewport::new(720.0, 420.0);
    viewport.scale = 2.0;
    app.world_mut().insert_resource(viewport);
    let engine: Arc<dyn TextEngine> = Arc::new(ParleyText::bundled());
    app.world_mut().insert_resource(engine);
    let theme = UiTheme {
        appearance: Appearance::Dark,
        ..UiTheme::default()
    };
    app.world_mut().insert_resource(theme.clone());
    let id = app
        .world_mut()
        .resource_mut::<MarkdownDocuments>()
        .unwrap()
        .open("| **Left** | Right |\n| :--- | ---: |\n| link | 1234 |\n");
    let parent = app.world_mut().spawn((
        Layout {
            width: Size::Px(720.0),
            height: Size::Px(420.0),
            ..Layout::column()
        },
        theme.color(Token::Canvas),
    ));
    let handle = MarkdownSurface::mount(
        app.world_mut(),
        parent,
        UiId(4),
        MarkdownSurfaceProps::new(id, "Table"),
    )
    .unwrap();
    let caret = app
        .world()
        .resource::<MarkdownDocuments>()
        .unwrap()
        .get(id)
        .unwrap()
        .bytes()
        .find("1234")
        .unwrap();
    app.world_mut()
        .resource_mut::<MarkdownDocuments>()
        .unwrap()
        .get_mut(id)
        .unwrap()
        .move_caret(caret, caret)
        .unwrap();
    app.world_mut().resource_mut::<Focus>().unwrap().entity = Some(handle.root);
    app.update();
    app.update();
    (app, handle)
}

#[test]
fn table_grid_paints_header_fill_alignment_and_borders() {
    let mut gpu = Exporter::new().expect("a real headless graphics adapter");
    let (mut app, _handle) = fixture();
    let theme = app.world().resource::<UiTheme>().unwrap().clone();
    let header = theme.color(Token::Raised);
    let body = theme.color(Token::Content);
    let stroke = theme.color(Token::StrongStroke);
    let world = app.world();
    let left = world
        .entities_with::<Label>()
        .into_iter()
        .find(|entity| {
            world
                .get::<Label>(*entity)
                .is_some_and(|label| label.text == "Left")
        })
        .expect("header cell");
    let style = world.get::<TextStyle>(left).unwrap();
    assert_eq!(style.weight, FONT_WEIGHT_BOLD);
    assert_eq!(style.align, TextAlign::Start);
    let right = world
        .entities_with::<Label>()
        .into_iter()
        .find(|entity| {
            world
                .get::<Label>(*entity)
                .is_some_and(|label| label.text == "1234")
        })
        .expect("right aligned cell");
    assert_eq!(world.get::<TextStyle>(right).unwrap().align, TextAlign::End);
    assert!(world.get::<Border>(left).unwrap().width >= 1.0);
    let rect = *world.get::<Rect>(left).unwrap();
    let frame = rndr_paint::frame(&mut app);
    let image = gpu
        .render_frame(Some(app.world()), &frame, 1440, 840, 2.0)
        .unwrap();
    let fill = sample(
        &image.rgba,
        image.width,
        rect.x + rect.w * 0.5,
        rect.y + rect.h * 0.5,
        2.0,
    );
    let edge = sample(
        &image.rgba,
        image.width,
        rect.x + rect.w * 0.5,
        rect.y + 0.5,
        2.0,
    );
    assert!(
        nearer(&fill, header, body),
        "header fill {fill:?} should follow the raised token, not the body"
    );
    assert!(
        nearer(&edge, stroke, header),
        "cell edge {edge:?} should follow the strong stroke"
    );
    if let Some(dir) = std::env::var_os("RNDR_TABLE_CAPTURE_DIR") {
        let dir = std::path::PathBuf::from(dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("table-grid.png"), encode_png(&image).unwrap()).unwrap();
    }
}

fn sample(rgba: &[u8], width: u32, x: f32, y: f32, scale: f32) -> [u8; 3] {
    let x = (x * scale) as u32;
    let y = (y * scale) as u32;
    let i = ((y * width + x) * 4) as usize;
    [rgba[i], rgba[i + 1], rgba[i + 2]]
}

fn nearer(pixel: &[u8; 3], want: Color, other: Color) -> bool {
    distance(pixel, want) + 8.0 < distance(pixel, other)
}

fn distance(pixel: &[u8; 3], color: Color) -> f32 {
    let channel = |value: f32, byte: u8| value * 255.0 - f32::from(byte);
    channel(color.r, pixel[0]).powi(2)
        + channel(color.g, pixel[1]).powi(2)
        + channel(color.b, pixel[2]).powi(2)
}
