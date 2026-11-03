//! Native streaming markdown with retained live UI, plus optional offscreen evidence.
#[path = "../../rndr_ui/examples/support/markdown_gallery.rs"]
mod gallery;
use rndr_core::*;
use rndr_ui::{Appearance, UiTheme};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let has = |s: &str| args.iter().any(|a| a == s);
    let width = if has("--narrow") { 340. } else { 760. };
    let height = if has("--narrow") { 440. } else { 660. };
    let theme = UiTheme {
        appearance: if has("--dark") {
            Appearance::Dark
        } else {
            Appearance::Light
        },
        ..Default::default()
    };
    let mut app = gallery::create(width, height, theme);
    if has("--complete") {
        while app.world().resource::<gallery::Gallery>().unwrap().cursor < gallery::TAIL.len() {
            gallery::tick(app.world_mut());
            rndr_paint::frame(&mut app);
        }
    }
    if let Some(index) = args.iter().position(|arg| arg == "--capture-prefix") {
        let prefix = args
            .get(index + 1)
            .ok_or("--capture-prefix needs an output prefix")?;
        #[cfg(feature = "observe")]
        {
            use rndr_render::{
                capture::{capture, Error, Target},
                encode_png,
                observe::{ObservationContent, ObservationSensitive},
                Exporter,
            };
            use std::io::Write;
            app.world_mut().insert_resource(ObservationContent::Public); // Entire synthetic scene audited.
            let mut exporter = Exporter::new()?;
            let g = app.world().resource::<gallery::Gallery>().unwrap().clone();
            request_focus(app.world_mut(), g.increment.root);
            let ids = |world: &World, entity| rndr_ui::identity(world, entity).map(|id| id.0);
            let result = capture(&mut app, &mut exporter, Target::Entity(g.view), ids)?;
            assert!(result.metadata.nodes.iter().any(|n| n.ui_id == Some(10)));
            assert!(!result
                .metadata
                .to_json()
                .to_string()
                .contains("Increment live value"));
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(format!("{prefix}.png"))?;
            file.write_all(&encode_png(&result.image)?)?;
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(format!("{prefix}.json"))?;
            file.write_all(result.metadata.to_json().to_string().as_bytes())?;
            println!(
                "captured {}x{} actual offscreen pixels; {} same-frame nodes",
                result.image.width,
                result.image.height,
                result.metadata.nodes.len()
            );
            // Sensitive host controls use the unchanged whole-frame fail-closed
            // gate. Test root, control descendant, hidden content and unrelated target.
            for entity in [g.controls_slot.root, g.increment.label] {
                app.world_mut().insert(entity, ObservationSensitive);
                for target in [g.view, g.value_slot.root, g.increment.root] {
                    assert!(matches!(
                        capture(&mut app, &mut exporter, Target::Entity(target), ids),
                        Err(Error::Sensitive)
                    ));
                }
                app.world_mut().insert(entity, Hidden);
                assert!(matches!(
                    capture(&mut app, &mut exporter, Target::Entity(g.view), ids),
                    Err(Error::Sensitive)
                ));
                app.world_mut().remove::<ObservationSensitive>(entity);
                app.world_mut().remove::<Hidden>(entity);
            }
            app.world_mut().insert_resource(ObservationContent::Deny);
            assert!(matches!(
                capture(&mut app, &mut exporter, Target::Entity(g.view), ids),
                Err(Error::Sensitive)
            ));
            println!("privacy: 9 embedded-control/hidden/unrelated-target/deny capture checks passed; no partial output");
            return Ok(());
        }
        #[cfg(not(feature = "observe"))]
        {
            let _ = prefix;
            return Err("--capture-prefix requires --features observe".into());
        }
    }
    app.world_mut().insert_resource(WindowConfig {
        title: "rndr / Markdown host slots".into(),
        size: LogicalSize::new(width, height),
        min_size: Some(LogicalSize::new(300., 260.)),
        ..Default::default()
    });
    if has("--observe") {
        #[cfg(feature = "observe")]
        {
            use rndr_render::observe::{ObservationContent, ObservationSession};
            app.world_mut().insert_resource(ObservationContent::Public);
            let descriptor = ObservationSession::enable(
                app.world_mut(),
                "synthetic markdown host slot gallery",
            )?;
            eprintln!("observation descriptor: {}", descriptor.display());
        }
        #[cfg(not(feature = "observe"))]
        {
            return Err("--observe requires --features observe".into());
        }
    }
    let _stream = every(
        app.world_mut(),
        std::time::Duration::from_millis(100),
        gallery::Tick,
    );
    rndr::run(app);
    Ok(())
}
