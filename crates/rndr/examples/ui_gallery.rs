//! Native launcher for the same synthetic, headless-tested operating gallery.
//! This does not adopt UI components in a legacy/default application.
#[path = "../../rndr_ui/examples/support/operating.rs"]
mod operating;
use rndr_core::{LogicalSize, WindowConfig};

// Synthetic fixture inputs only, not desktop automation or OS input proof.
struct StateFixture {
    pressed: bool,
    sent: bool,
}
impl rndr_core::Component for StateFixture {}
fn fixture_input(world: &mut rndr_core::World) {
    use rndr_core::*;
    let fixture = world.resource::<StateFixture>().unwrap();
    let pressed = fixture.pressed;
    let sent = fixture.sent;
    let demo = world.resource::<operating::Demo>().unwrap();
    let target = demo.commands[usize::from(pressed)].root;
    let Some(rect) = world.get::<Rect>(target).copied() else {
        return;
    };
    let position = [rect.x + rect.w * 0.5, rect.y + rect.h * 0.5];
    let pointer = &mut world.resource_mut::<Input>().unwrap().pointer;
    *pointer = Pointer {
        position,
        pressed: pressed && !sent,
        down: pressed,
        press_at: if pressed && !sent {
            Some(position)
        } else {
            None
        },
        ..Pointer::default()
    };
    world.resource_mut::<StateFixture>().unwrap().sent = true;
}
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
        title: "rndr — Component gallery (synthetic)".into(),
        size: LogicalSize::new(width, 700.0),
        min_size: Some(LogicalSize::new(280.0, 240.0)),
        ..WindowConfig::default()
    });
    // Capture lower content at enlarged type without an automation endpoint.
    // This uses the same canonical focus-reveal path as keyboard navigation.
    let demo = app.world().resource::<operating::Demo>().unwrap();
    let focus = if has("--focus-source") {
        Some(demo.source.root)
    } else if has("--focus-editor") {
        Some(demo.editor)
    } else {
        None
    };
    if let Some(entity) = focus {
        rndr_core::request_focus(app.world_mut(), entity);
    }
    if has("--preview") {
        let target = app
            .world()
            .resource::<operating::Demo>()
            .unwrap()
            .selector
            .tabs[1]
            .root;
        assert_eq!(
            rndr_core::try_activate(app.world_mut(), target),
            rndr_core::ActivationResult::Accepted
        );
    }
    if has("--states") || has("--pressed") {
        use rndr_core::*;
        use rndr_ui::controls::{self, ButtonProps, ButtonVariant, HostIntent};
        let demo = app.world().resource::<operating::Demo>().unwrap().clone();
        let mut props = ButtonProps::new("Unavailable", HostIntent::Command(2));
        props.variant = ButtonVariant::Quiet;
        props.enabled = false;
        controls::Button::mount(app.world_mut(), demo.toolbar.root, UiId(12), props).unwrap();
        request_focus(app.world_mut(), demo.nav[1].root);
        app.world_mut().insert_resource(StateFixture {
            pressed: has("--pressed"),
            sent: false,
        });
        app.add_system_front(Schedule::Input, fixture_input);
    }
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
