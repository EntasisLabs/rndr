//! Native synthetic Checkbox/SingleSelect/MultiSelect + PropertyInspector gallery.
#[path = "../../rndr_ui/examples/support/selection_gallery.rs"]
mod gallery;
use rndr_core::*;
use rndr_ui::*;
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let has = |s: &str| args.iter().any(|a| a == s);
    let width = if has("--narrow") { 380. } else { 1180. };
    let height = 960.;
    let theme = UiTheme {
        appearance: if has("--dark") {
            Appearance::Dark
        } else {
            Appearance::Light
        },
        density: if has("--compact") {
            Density::Compact
        } else {
            Density::Comfortable
        },
        contrast: if has("--hc") {
            Contrast::High
        } else {
            Contrast::Standard
        },
        text_scale: if has("--200") { 2. } else { 1. },
        reading_scale: if has("--200") { 2. } else { 1. },
        ..Default::default()
    };
    let mut app = gallery::create(width, height, theme, &args);
    // Source-defined synthetic state, not physical input evidence. Prime ordinary
    // layout so the initial observation already includes the requested popup.
    if has("--single-open") || has("--multi-open") || has("--inspector-open") {
        for _ in 0..2 {
            rndr_paint::frame(&mut app);
        }
        let g = app.world().resource::<gallery::Gallery>().unwrap();
        let target = if has("--inspector-open") {
            g.inspector
                .unwrap()
                .slot(app.world(), rndr_ui::data::PropertyId(3))
                .unwrap()
                .editor
        } else if has("--single-open") {
            g.single.unwrap().root
        } else {
            g.multi.unwrap().root
        };
        request_focus(app.world_mut(), target);
        reveal_entity(app.world_mut(), target);
        rndr_paint::frame(&mut app);
        try_activate(app.world_mut(), target);
        rndr_paint::frame(&mut app);
    }
    app.world_mut().insert_resource(WindowConfig {
        title: "rndr / Selection controls".into(),
        size: LogicalSize::new(width, height),
        min_size: Some(LogicalSize::new(320., 400.)),
        ..Default::default()
    });
    if has("--observe") {
        #[cfg(feature = "observe")]
        {
            use rndr::observe::{ObservationContent, ObservationSession};
            app.world_mut().insert_resource(ObservationContent::Public);
            let descriptor =
                ObservationSession::enable(app.world_mut(), "synthetic selection gallery")
                    .expect("observation session");
            eprintln!("observation descriptor: {}", descriptor.display());
        }
        #[cfg(not(feature = "observe"))]
        {
            eprintln!("--observe requires --features observe");
            std::process::exit(2);
        }
    }
    #[cfg(feature = "observe")]
    let _exit = if has("--observe") && has("--observe-exit-after-15s") {
        #[derive(Clone)]
        struct Exit;
        impl Event for Exit {}
        app.add_system(Schedule::Update, |w| {
            if !events::<Exit>(w).is_empty() {
                w.resource_mut::<Input>().unwrap().quit = true;
            }
        });
        Some(after(
            app.world_mut(),
            std::time::Duration::from_secs(15),
            Exit,
        ))
    } else {
        None
    };
    rndr::run(app);
}
