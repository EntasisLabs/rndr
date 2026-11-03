//! Standalone interactive ListView/TreeView gallery; synthetic data only.
#[path = "../../rndr_ui/examples/support/data_gallery.rs"]
mod gallery;
use rndr_core::*;
use rndr_ui::*;
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let has = |s: &str| args.iter().any(|a| a == s);
    let width = if has("--narrow") { 380.0 } else { 1040.0 };
    let height = if has("--narrow") { 920.0 } else { 820.0 };
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
        ..UiTheme::default()
    };
    let mut app = gallery::create(width, height, theme);
    if has("--empty") || has("--filtered") || has("--disabled") || has("--large") {
        let g = app.world_mut().resource_mut::<gallery::Gallery>().unwrap();
        if has("--empty") {
            g.records.clear();
        }
        if has("--filtered") {
            g.query = "no matching project".into();
        }
        if has("--disabled") {
            g.disabled = true;
        }
        if has("--large") {
            g.records = (1..=10000)
                .map(|i| rndr_ui::collections::Item::new(i, format!("Dataset project {i:05}")))
                .collect();
            g.next_id = 10001;
        }
        gallery::reconcile(app.world_mut());
        let g = app.world().resource::<gallery::Gallery>().unwrap().clone();
        app.world_mut().get_mut::<Label>(g.filter).unwrap().text = g.query;
    }
    if has("--tree-focus") {
        let tree = app.world().resource::<gallery::Gallery>().unwrap().tree;
        let _ = tree.select(app.world_mut(), Some(rndr_ui::collections::ItemId(3)));
        let _ = tree.focus(app.world_mut(), rndr_ui::collections::ItemId(1));
    }
    app.world_mut().insert_resource(WindowConfig {
        title: "rndr / Data components".into(),
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
