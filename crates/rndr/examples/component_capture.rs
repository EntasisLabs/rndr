//! No window/event loop: capture a mounted Card using its scoped UiId.
#[cfg(feature = "observe")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use rndr_core::{App, Layout, Size, Viewport};
    use rndr_render::{
        capture::{capture, Error, Target},
        encode_png,
        observe::ObservationContent,
        Exporter,
    };
    use rndr_ui::{Card, Typography, UiId, UiPlugin};
    let prefix = std::env::args_os()
        .nth(1)
        .ok_or("usage: component_capture OUTPUT_PREFIX")?;
    let mut app = App::new();
    app.add_plugin(UiPlugin);
    app.world_mut().insert_resource(Viewport::new(360.0, 240.0));
    app.world_mut().insert_resource(ObservationContent::Public); // Entire synthetic scene audited.
    let host = app.world_mut().spawn((Layout {
        width: Size::Px(320.0),
        height: Size::Px(220.0),
        ..Layout::column()
    },));
    let card = Card::mount(app.world_mut(), host, UiId(42), "Headless component").unwrap();
    rndr_ui::text(
        app.world_mut(),
        card.content,
        "Public synthetic content",
        Typography::Label,
    )
    .unwrap();
    let other_host = app.world_mut().spawn((Layout {
        width: Size::Px(24.0),
        height: Size::Px(24.0),
        ..Layout::column()
    },));
    Card::mount(app.world_mut(), other_host, UiId(42), "Other scope").unwrap();
    let mut exporter = Exporter::new()?;
    let ids = |w: &rndr_core::World, e| rndr_ui::identity(w, e).map(|id| id.0);
    assert_eq!(rndr_ui::identity(app.world(), card.root), Some(UiId(42)));
    assert!(matches!(
        capture(
            &mut app,
            &mut exporter,
            Target::UiId {
                id: 42,
                parent: None,
                expected: None
            },
            ids
        ),
        Err(Error::Ambiguous)
    ));
    let result = capture(
        &mut app,
        &mut exporter,
        Target::UiId {
            id: 42,
            parent: Some(host),
            expected: Some(card.root),
        },
        ids,
    )?;
    assert_eq!(result.metadata.target, card.root);
    assert_eq!(result.metadata.nodes[0].ui_id, Some(42));
    assert!(result.metadata.nodes.len() >= 4);
    assert!(result.image.rgba.chunks_exact(4).any(|p| p[3] > 0));
    let json = result.metadata.to_json().to_string();
    assert!(!json.contains("Public synthetic content"));
    assert!(!json.contains("Headless component"));
    let mut png_path = prefix.clone();
    png_path.push(".png");
    let mut json_path = prefix;
    json_path.push(".json");
    use std::io::Write;
    let mut png = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(png_path)?;
    png.write_all(&encode_png(&result.image)?)?;
    let mut metadata = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(json_path)?;
    metadata.write_all(json.as_bytes())?;
    println!(
        "UI key 42 resolved to {:?}; {} subtree nodes, {}x{} real offscreen pixels",
        card.root,
        result.metadata.nodes.len(),
        result.image.width,
        result.image.height
    );
    Ok(())
}
#[cfg(not(feature = "observe"))]
fn main() {
    eprintln!("component_capture requires --features observe");
    std::process::exit(2);
}
