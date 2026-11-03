//! Synthetic UI polish reproduction. Headless pixels are not native input certification.
#[path = "../../rndr_ui/examples/support/selection_gallery.rs"]
mod selection;
#[path = "../../rndr_ui/examples/support/data_gallery.rs"]
mod data;
#[path = "../../rndr_ui/examples/support/data_editors_gallery.rs"]
mod editors;
use rndr_core::*;
use rndr_ui::*;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let has = |s: &str| args.iter().any(|a| a == s);
    let value = |s: &str| args.iter().position(|a| a == s).and_then(|i| args.get(i+1));
    let width = if has("--narrow") { 380. } else { 1000. };
    let height = 960.;
    let theme = UiTheme { appearance: if has("--dark") { Appearance::Dark } else { Appearance::Light }, ..Default::default() };
    let scene = value("--scene").map(String::as_str).unwrap_or("selection");
    let mut app = match scene {
        "data" => data::create(width, height, theme),
        "editors" => editors::create(width, height, theme),
        _ => selection::create(width, height, theme, &args),
    };
    #[cfg(feature="parley")]
    if !has("--builtin") { let engine: std::sync::Arc<dyn TextEngine> = std::sync::Arc::new(rndr_render::ParleyText::bundled()); app.world_mut().insert_resource(engine); }
    if has("--compact") { app.world_mut().resource_mut::<UiTheme>().unwrap().density = Density::Compact; }
    if has("--2x") { app.world_mut().resource_mut::<Viewport>().unwrap().scale = 2.; }
    rndr_paint::frame(&mut app);
    if has("--focus-view") {
        let e = match scene {
            "data" => app.world().resource::<data::Gallery>().unwrap().tree.root,
            "editors" => app.world().resource::<editors::Gallery>().unwrap().table.unwrap().rows.root,
            _ => app.world().resource::<selection::Gallery>().unwrap().checkbox.unwrap().root,
        };
        request_focus(app.world_mut(), e);
    }
    if has("--focus") {
        if let Some(e) = app.world().entities_with::<Focusable>().into_iter().find(|e| access_details(app.world(), *e).focus) {
            request_focus(app.world_mut(), e);
        }
    }
    if let Some(label) = value("--focus-label") {
        if let Some(e) = app.world().entities_with::<Label>().into_iter().find(|e| app.world().get::<Label>(*e).is_some_and(|l| &l.text == label)).and_then(|e| ancestor_with::<Focusable>(app.world(),e)) {
            request_focus(app.world_mut(),e);
        } else { return Err("focus label not found".into()); }
    }
    if let Some(prefix) = value("--capture-prefix") {
        #[cfg(feature="observe")]
        {
            use rndr_render::{capture::{capture, Target}, observe::ObservationContent, Exporter, encode_png};
            use std::io::Write;
            app.world_mut().insert_resource(ObservationContent::Public);
            let root = match scene {
                "data" => app.world().resource::<data::Gallery>().unwrap().root,
                "editors" => app.world().resource::<editors::Gallery>().unwrap().root,
                _ => app.world().resource::<selection::Gallery>().unwrap().root,
            };
            let mut exporter = Exporter::new()?;
            let result = capture(&mut app, &mut exporter, Target::Entity(root), |w,e| identity(w,e).map(|id| id.0))?;
            for (ext, bytes) in [("png",encode_png(&result.image)?), ("json",result.metadata.to_json().to_string().into_bytes())] {
                std::fs::OpenOptions::new().write(true).create_new(true).open(format!("{prefix}.{ext}"))?.write_all(&bytes)?;
            }
            println!("headless {scene}: {}x{} scale {}",result.image.width,result.image.height,result.metadata.scale);
            return Ok(());
        }
        #[cfg(not(feature="observe"))]
        return Err("capture requires observe".into());
    }
    app.world_mut().insert_resource(WindowConfig { title: "rndr / UI polish retest".into(), size: LogicalSize::new(width,height), ..Default::default() });
    rndr::run(app);
    Ok(())
}
