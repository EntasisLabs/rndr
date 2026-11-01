//! Native launcher for the same synthetic, headless-tested operating gallery.
//! This does not adopt UI components in a legacy/default application.
#[path = "../../rndr_ui/examples/support/operating.rs"]
mod operating;
use rndr_core::{LogicalSize, WindowConfig};
use rndr_ui::*;
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let has = |s: &str| args.iter().any(|a| a == s);
    let width = if has("--narrow") { 320.0 } else { 1000.0 };
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
        ..UiTheme::default()
    };
    let mut app = operating::create(width, 700.0, theme.clone(), has("--long"));
    app.world_mut().insert_resource(WindowConfig {
        title: "rndr A2 — synthetic host (not Blooey)".into(),
        size: LogicalSize::new(width, 700.0),
        min_size: Some(LogicalSize::new(280.0, 240.0)),
        ..WindowConfig::default()
    });
    if has("--observe") {
        #[cfg(feature = "observe")]
        {
            // This launcher contains only audited synthetic fixtures. Other apps
            // must make their own whole-frame public-content decision.
            use rndr::observe::{ObservationContent, ObservationSession};
            app.world_mut().insert_resource(ObservationContent::Public);
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
    // Bounded native validation without desktop automation or an interaction
    // endpoint. Timer only requests normal synthetic-window shutdown.
    #[cfg(feature = "observe")]
    let _exit_timer = if has("--observe") && has("--observe-exit-after-15s") {
        #[derive(Clone)]
        struct Exit;
        impl rndr_core::Event for Exit {}
        app.add_system(rndr_core::Schedule::Update, |world| {
            if !rndr_core::events::<Exit>(world).is_empty() {
                world.resource_mut::<rndr_core::Input>().unwrap().quit = true;
            }
        });
        Some(rndr_core::after(
            app.world_mut(),
            std::time::Duration::from_secs(15),
            Exit,
        ))
    } else {
        None
    };
    rndr::run(app);
}
