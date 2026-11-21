//! Actual editor surface, synthetic public content, offscreen pixels (not native input proof).
use rndr_core::*;
use rndr_markdown_ui::*;
use rndr_ui::{Appearance, Contrast, Density, Token, UiId, UiTheme};
use std::sync::Arc;

const FIELD_NOTES: &str = "# Field notes\n\nBody **bold**, *italic*, `inline code`, $x+y$, ==highlight== and [link](https://example.com).\n\n{{red|Red}} {{orange|Orange}} {{yellow|Yellow}} {{green|Green}} {{blue|Blue}} {{purple|Purple}} {{pink|Pink}}\n\n- [ ] Unchecked task\n- [x] Completed task\n- [ ] Long task label wraps onto the next line to test first-line checkbox alignment and document geometry in narrow windows.\n\n> [!info] Information\n> A neutral body with **bold** and `code`.\n\n> [!warning] Warning\n> Read the warning before continuing.\n\n```rust\nlet quiet = true;\n```\n\n| Name | Value |\n| --- | --- |\n| Reading | 14/22 |\n| Last row | Visible |\n";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let has = |flag: &str| args.iter().any(|arg| arg == flag);
    let prefix = args
        .iter()
        .position(|arg| arg == "--capture-prefix")
        .and_then(|i| args.get(i + 1))
        .ok_or("--capture-prefix required")?;
    let scale = if has("--200") { 2.0 } else { 1.0 };
    let theme = UiTheme {
        appearance: if has("--dark") {
            Appearance::Dark
        } else {
            Appearance::Light
        },
        contrast: if has("--hc") {
            Contrast::High
        } else {
            Contrast::Standard
        },
        density: if has("--compact") {
            Density::Compact
        } else {
            Density::Comfortable
        },
        text_scale: scale,
        reading_scale: scale,
        ..Default::default()
    };
    let width = if has("--narrow") { 380.0 } else { 900.0 };
    let mut app = App::new();
    app.add_plugin(rndr_markdown_ui::MarkdownPlugin);
    app.world_mut()
        .insert_resource(Arc::new(rndr_render::ParleyText::bundled()) as Arc<dyn TextEngine>);
    app.world_mut()
        .insert_resource(Viewport::new(width, 1400.0));
    app.world_mut().insert_resource(theme.clone());
    let root = app.world_mut().spawn((
        Layout {
            width: Size::Fill,
            height: Size::Fill,
            ..Layout::column()
        },
        theme.color(Token::Content),
    ));
    let document = app
        .world_mut()
        .resource_mut::<MarkdownDocuments>()
        .unwrap()
        .open(FIELD_NOTES);
    // Keep caret outside styled content so source-reveal does not distort the fixture.
    app.world_mut()
        .resource_mut::<MarkdownDocuments>()
        .unwrap()
        .get_mut(document)
        .unwrap()
        .move_caret(FIELD_NOTES.len(), FIELD_NOTES.len())
        .expect("fixture caret");
    MarkdownSurface::mount(
        app.world_mut(),
        root,
        UiId(1),
        MarkdownSurfaceProps::new(document, "Field notes"),
    )
    .expect("fixture surface");
    for _ in 0..3 {
        rndr_paint::frame(&mut app);
    }
    use rndr_render::{
        capture::{capture, Target},
        encode_png,
        observe::ObservationContent,
        Exporter,
    };
    app.world_mut().insert_resource(ObservationContent::Public);
    let mut exporter = Exporter::new()?;
    let result = capture(&mut app, &mut exporter, Target::Entity(root), |_, _| None)?;
    std::fs::write(format!("{prefix}.png"), encode_png(&result.image)?)?;
    std::fs::write(
        format!("{prefix}.json"),
        result.metadata.to_json().to_string(),
    )?;
    println!(
        "{}x{} pixels, {} same-frame nodes",
        result.image.width,
        result.image.height,
        result.metadata.nodes.len()
    );
    Ok(())
}
