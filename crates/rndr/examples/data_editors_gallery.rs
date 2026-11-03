//! Standalone interactive DataTable/PropertyInspector gallery; synthetic data only.
#[path = "../../rndr_ui/examples/support/data_editors_gallery.rs"]
mod gallery;
use rndr_core::*;
use rndr_ui::*;
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let has = |s: &str| args.iter().any(|a| a == s);
    let width = if has("--narrow") { 380.0 } else { 1180.0 };
    let height = if has("--narrow") { 920.0 } else { 960.0 };
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
        text_scale: if has("--200") { 2.0 } else { 1.0 },
        reading_scale: if has("--200") { 2.0 } else { 1.0 },
        ..UiTheme::default()
    };
    let mut app = gallery::create(width, height, theme);
    {
        let g = app.world_mut().resource_mut::<gallery::Gallery>().unwrap();
        if has("--empty") {
            g.records.clear();
            g.selected = None;
        }
        if has("--filtered") {
            g.filtered = true;
        }
        if has("--disabled") {
            g.disabled = true;
        }
        if has("--readonly") {
            g.readonly = true;
        }
        gallery::reconcile(app.world_mut());
        if has("--invalid") {
            let h = app
                .world()
                .resource::<gallery::Gallery>()
                .unwrap()
                .inspector
                .unwrap();
            h.set_draft(app.world_mut(), rndr_ui::data::PropertyId(2), "-")
                .unwrap();
            let e = h
                .slot(app.world(), rndr_ui::data::PropertyId(2))
                .unwrap()
                .editor;
            request_focus(app.world_mut(), e);
        }
    }
    if has("--inspector-scroll") {
        app.add_system(Schedule::PostUpdate, |w| {
            let root = w.resource::<gallery::Gallery>().unwrap().root;
            w.get_mut::<Scroll>(root).unwrap().offset[1] = -1200.;
        });
    }
    if has("--inspector-bottom") {
        app.add_system(Schedule::PostUpdate, |w| {
            let h = w.resource::<gallery::Gallery>().unwrap().inspector.unwrap();
            w.get_mut::<Scroll>(h.root).unwrap().offset[1] = -900.;
        });
    }
    app.world_mut().insert_resource(WindowConfig {
        title: "rndr / Data editors".into(),
        size: LogicalSize::new(width, height),
        min_size: Some(LogicalSize::new(320.0, 400.0)),
        ..WindowConfig::default()
    });
    if has("--observe") {
        #[cfg(feature = "observe")]
        {
            use rndr::observe::{ObservationContent, ObservationSession};
            // This app has no private data sources or arbitrary-content loading.
            app.world_mut().insert_resource(ObservationContent::Public);
            let theme = app.world().resource::<UiTheme>().unwrap().clone();
            let descriptor = ObservationSession::enable(app.world_mut(), &format!("{theme:?}"))
                .expect("private observation session");
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
