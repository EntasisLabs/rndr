//! Headless window config, visibility, and focus/blur order.

use std::time::Duration;

use rndr_core::{
    events, spawn, window, Availability, Dismiss, Event, Input, Key, KeyEvent, LogicalSize,
    Modifiers, NamedKey, Platform, Position, Resized, ScaleChanged, Schedule, Sim, Viewport,
    WindowConfig, WindowEffect, WindowFocus, WindowKind, WindowMetrics, WindowPhase, WindowSignal,
    WindowSupport,
};

fn near(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.001
}

#[test]
fn defaults_match_the_hard_coded_window() {
    let config = WindowConfig::default();
    assert_eq!(config.title, "rndr");
    assert!(near(config.size.width, 800.0));
    assert!(near(config.size.height, 600.0));
    assert!(config.min_size.is_none());
    assert_eq!(config.position, Position::Platform);
    assert!(config.resizable);
    assert!(config.decorations);
    assert!(!config.transparent);
    assert!(!config.always_on_top);
    assert!(!config.skip_taskbar);
    assert_eq!(config.kind, WindowKind::Normal);
    assert!(!config.hide_on_blur);
    assert_eq!(config.dismiss, Dismiss::Ignore);
    assert!(!config.accessory);
    assert!(near(WindowMetrics::default().width, 800.0));
    assert!(near(WindowMetrics::default().scale, 1.0));
    let phase = WindowPhase::default();
    assert!(phase.visible);
    assert!(!phase.focused);
    assert!(!phase.armed);
}

#[test]
fn platform_support_matches_the_documented_table() {
    let mac = window::platform_support(Platform::Mac);
    assert_eq!(mac.position, Availability::Supported);
    assert_eq!(mac.hide, Availability::Supported);
    assert_eq!(mac.focus, Availability::Supported);
    assert_eq!(mac.always_on_top, Availability::Supported);
    assert_eq!(mac.skip_taskbar, Availability::Ignored);
    assert_eq!(mac.panel_above_fullscreen, Availability::Supported);
    assert_eq!(mac.panel_all_spaces, Availability::Supported);
    assert_eq!(mac.panel_nonactivating, Availability::Supported);
    assert_eq!(mac.accessory, Availability::Supported);

    let windows = window::platform_support(Platform::Windows);
    assert_eq!(windows.skip_taskbar, Availability::Supported);
    assert_eq!(windows.panel_above_fullscreen, Availability::Ignored);
    assert_eq!(windows.hide, Availability::Supported);

    let x11 = window::platform_support(Platform::X11);
    assert_eq!(x11.position, Availability::Supported);
    assert_eq!(x11.skip_taskbar, Availability::Supported);
    assert_eq!(x11.always_on_top, Availability::Supported);
    assert_eq!(x11.panel_nonactivating, Availability::Ignored);

    let wayland = window::platform_support(Platform::Wayland);
    let ignored = WindowSupport {
        position: Availability::Ignored,
        hide: Availability::Ignored,
        focus: Availability::Ignored,
        always_on_top: Availability::Ignored,
        skip_taskbar: Availability::Ignored,
        panel_above_fullscreen: Availability::Ignored,
        panel_all_spaces: Availability::Ignored,
        panel_nonactivating: Availability::Ignored,
        accessory: Availability::Ignored,
    };
    assert_eq!(wayland, ignored);
}

#[test]
fn toggle_hides_then_shows_and_focuses() {
    let config = WindowConfig::default();
    let hidden = window::transition(WindowPhase::default(), &config, WindowSignal::Toggle);
    assert!(!hidden.phase.visible);
    assert!(hidden.events.is_empty());
    assert_eq!(hidden.effects, vec![WindowEffect::Hide]);

    let shown = window::transition(hidden.phase, &config, WindowSignal::Toggle);
    assert!(shown.phase.visible);
    assert!(shown.phase.focused);
    assert_eq!(shown.events, vec![WindowFocus::Focused]);
    assert_eq!(shown.effects, vec![WindowEffect::Show, WindowEffect::Focus]);

    let again = window::transition(shown.phase, &config, WindowSignal::Toggle);
    assert!(!again.phase.visible);
    assert!(!again.phase.focused);
    assert_eq!(again.events, vec![WindowFocus::Blurred]);
    assert_eq!(again.effects, vec![WindowEffect::Hide]);
}

#[test]
fn focus_while_hidden_does_nothing() {
    let hidden = window::transition(
        WindowPhase::default(),
        &WindowConfig::default(),
        WindowSignal::Hide,
    );
    let focused = window::transition(hidden.phase, &WindowConfig::default(), WindowSignal::Focus);
    assert!(!focused.phase.visible);
    assert!(!focused.phase.focused);
    assert!(focused.events.is_empty());
    assert!(focused.effects.is_empty());
}

#[test]
fn show_and_focus_are_idempotent() {
    let config = WindowConfig::default();
    let phase = WindowPhase::focused();
    let shown = window::transition(phase, &config, WindowSignal::Show);
    let focused = window::transition(phase, &config, WindowSignal::Focus);
    assert!(shown.events.is_empty() && shown.effects.is_empty());
    assert!(focused.events.is_empty() && focused.effects.is_empty());
    assert!(shown.phase.focused && focused.phase.visible);
}

#[test]
fn blur_before_the_first_focus_is_ignored() {
    let config = WindowConfig {
        hide_on_blur: true,
        ..WindowConfig::default()
    };
    let lost = window::transition(WindowPhase::default(), &config, WindowSignal::LostFocus);
    assert!(lost.phase.visible);
    assert!(lost.events.is_empty());
    assert!(lost.effects.is_empty());
}

#[test]
fn hide_on_blur_emits_blur_then_hides() {
    let config = WindowConfig {
        hide_on_blur: true,
        ..WindowConfig::default()
    };
    let gained = window::transition(WindowPhase::default(), &config, WindowSignal::GainedFocus);
    assert_eq!(gained.events, vec![WindowFocus::Focused]);
    let lost = window::transition(gained.phase, &config, WindowSignal::LostFocus);
    assert_eq!(lost.events, vec![WindowFocus::Blurred]);
    assert_eq!(lost.effects, vec![WindowEffect::Hide]);
    assert!(!lost.phase.visible);
    assert!(!lost.phase.focused);
}

#[test]
fn focus_and_blur_stay_in_order_without_hiding() {
    let config = WindowConfig::default();
    let mut phase = WindowPhase::focused();
    let mut log = Vec::new();
    for signal in [
        WindowSignal::LostFocus,
        WindowSignal::GainedFocus,
        WindowSignal::LostFocus,
    ] {
        let step = window::transition(phase, &config, signal);
        log.extend(step.events);
        phase = step.phase;
        assert!(phase.visible);
    }
    assert_eq!(
        log,
        vec![
            WindowFocus::Blurred,
            WindowFocus::Focused,
            WindowFocus::Blurred
        ]
    );
    assert!(!phase.focused);
}

#[test]
fn queued_focus_blur_focus_arrive_in_one_queue() {
    let mut sim = Sim::new();
    window::focus(sim.world());
    window::platform_blurred(sim.world());
    window::focus(sim.world());
    sim.update();
    assert_eq!(
        events::<WindowFocus>(sim.world()),
        &[
            WindowFocus::Focused,
            WindowFocus::Blurred,
            WindowFocus::Focused
        ]
    );
    assert!(window::is_visible(sim.world()));
    assert!(window::is_focused(sim.world()));
    assert_eq!(
        window::drain_effects(sim.world()),
        vec![WindowEffect::Focus, WindowEffect::Focus]
    );
}

#[test]
fn hide_on_blur_reaches_app_code_as_one_blur() {
    let mut sim = Sim::new();
    sim.world().insert_resource(WindowConfig {
        hide_on_blur: true,
        ..WindowConfig::default()
    });
    window::focus(sim.world());
    sim.update();
    assert_eq!(events::<WindowFocus>(sim.world()), &[WindowFocus::Focused]);
    let _ = window::drain_effects(sim.world());

    window::platform_blurred(sim.world());
    sim.update();
    assert_eq!(events::<WindowFocus>(sim.world()), &[WindowFocus::Blurred]);
    assert!(!window::is_visible(sim.world()));
    assert!(!window::is_focused(sim.world()));
    assert_eq!(window::drain_effects(sim.world()), vec![WindowEffect::Hide]);
}

#[test]
fn escape_is_left_alone_unless_dismiss_is_set() {
    let mut sim = Sim::new();
    sim.world().resource_mut::<Input>().unwrap().key = Some(Key::Escape);
    sim.update();
    let input = sim.world().resource::<Input>().unwrap();
    assert_eq!(input.key, Some(Key::Escape));
    assert!(!input.quit);
    assert!(window::is_visible(sim.world()));
}

#[derive(Clone, Copy)]
struct SawEscape(bool);

fn record_escape(world: &mut rndr_core::World) {
    let saw = world.resource::<Input>().unwrap().key == Some(Key::Escape);
    world.insert_resource(SawEscape(saw));
}

#[test]
fn dismiss_runs_before_other_input_systems() {
    let mut sim = Sim::new();
    sim.world().insert_resource(WindowConfig {
        dismiss: Dismiss::Hide,
        ..WindowConfig::default()
    });
    window::focus(sim.world());
    sim.update();
    let _ = window::drain_effects(sim.world());
    sim.app.add_system(Schedule::Input, record_escape);

    sim.world().resource_mut::<Input>().unwrap().key = Some(Key::Escape);
    sim.update();
    assert!(!sim.world().resource::<SawEscape>().unwrap().0);
    assert!(sim.world().resource::<Input>().unwrap().key.is_none());
    assert_eq!(events::<WindowFocus>(sim.world()), &[WindowFocus::Blurred]);
    assert!(!window::is_visible(sim.world()));
}

#[test]
fn platform_focus_is_readable_in_input() {
    let mut sim = Sim::new();
    sim.app.add_system(Schedule::Input, record_focus);
    window::platform_focused(sim.world());
    sim.update();
    assert!(sim.world().resource::<SawFocus>().unwrap().0);
}

#[derive(Clone, Copy)]
struct SawFocus(bool);

fn record_focus(world: &mut rndr_core::World) {
    let saw = events::<WindowFocus>(world).contains(&WindowFocus::Focused);
    world.insert_resource(SawFocus(saw));
}

#[test]
fn scale_is_readable_in_input() {
    let mut sim = Sim::new();
    sim.app.add_system(Schedule::Input, layout_reads_metrics);
    window::publish_metrics(sim.world(), 1600, 1200, 2.0);
    sim.update();
    let seen = *sim.world().resource::<SeenMetrics>().unwrap();
    assert!(near(seen.scale, 2.0));
    assert!(seen.event_scale.is_some_and(|scale| near(scale, 2.0)));
    assert!(seen.event_width.is_none());
}

#[test]
fn dismiss_hide_consumes_escape_before_systems() {
    let mut sim = Sim::new();
    sim.world().insert_resource(WindowConfig {
        dismiss: Dismiss::Hide,
        ..WindowConfig::default()
    });
    window::focus(sim.world());
    sim.update();
    let _ = window::drain_effects(sim.world());
    sim.app.add_system(Schedule::Update, record_escape);

    sim.world().resource_mut::<Input>().unwrap().key = Some(Key::Escape);
    sim.update();
    assert!(!sim.world().resource::<SawEscape>().unwrap().0);
    assert!(sim.world().resource::<Input>().unwrap().key.is_none());
    assert!(!sim.world().resource::<Input>().unwrap().quit);
    assert!(!window::is_visible(sim.world()));
    assert_eq!(events::<WindowFocus>(sim.world()), &[WindowFocus::Blurred]);
    assert_eq!(window::drain_effects(sim.world()), vec![WindowEffect::Hide]);
}

#[test]
fn dismiss_close_sets_quit() {
    let mut sim = Sim::new();
    sim.world().insert_resource(WindowConfig {
        dismiss: Dismiss::Close,
        ..WindowConfig::default()
    });
    sim.app.add_system(Schedule::Update, record_escape);
    {
        let input = sim.world().resource_mut::<Input>().unwrap();
        input.key = Some(Key::Escape);
        input
            .keys
            .push(KeyEvent::named(NamedKey::Escape, Modifiers::default()));
    }
    sim.update();
    assert!(sim.world().resource::<Input>().unwrap().quit);
    assert!(sim.world().resource::<Input>().unwrap().key.is_none());
    assert!(sim.world().resource::<Input>().unwrap().keys.is_empty());
    assert!(!sim.world().resource::<SawEscape>().unwrap().0);
    assert!(window::is_visible(sim.world()));
}

#[test]
fn modified_escape_does_not_dismiss() {
    let mut sim = Sim::new();
    sim.world().insert_resource(WindowConfig {
        dismiss: Dismiss::Hide,
        ..WindowConfig::default()
    });
    let input = sim.world().resource_mut::<Input>().unwrap();
    input.key = Some(Key::Escape);
    input.ctrl = true;
    sim.update();
    assert!(window::is_visible(sim.world()));
    assert!(!sim.world().resource::<Input>().unwrap().quit);
    assert_eq!(
        sim.world().resource::<Input>().unwrap().key,
        Some(Key::Escape)
    );
}

fn hide_on_down(world: &mut rndr_core::World) {
    if world
        .resource::<Input>()
        .is_some_and(|input| input.key == Some(Key::Down))
    {
        window::hide(world);
        world.resource_mut::<Input>().unwrap().key = None;
    }
}

#[test]
fn a_system_can_hide_and_the_blur_follows() {
    let mut sim = Sim::new();
    window::focus(sim.world());
    sim.update();
    let _ = window::drain_effects(sim.world());
    sim.app.add_system(Schedule::Update, hide_on_down);

    sim.world().resource_mut::<Input>().unwrap().key = Some(Key::Down);
    sim.update();
    assert!(!window::is_visible(sim.world()));
    assert!(events::<WindowFocus>(sim.world()).is_empty());
    assert_eq!(window::drain_effects(sim.world()), vec![WindowEffect::Hide]);

    sim.update();
    assert_eq!(events::<WindowFocus>(sim.world()), &[WindowFocus::Blurred]);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Ack;

impl Event for Ack {}

#[test]
fn a_task_can_hide_after_await() {
    let mut sim = Sim::new();
    window::focus(sim.world());
    sim.update();
    let _ = window::drain_effects(sim.world());

    let ctl = window::handle(sim.world());
    let handle = spawn(sim.world(), async move {
        rndr_core::sleep(Duration::from_millis(5)).await;
        ctl.hide();
        Ack
    });
    sim.update();
    assert!(window::is_visible(sim.world()));

    sim.advance(Duration::from_millis(5));
    sim.update();
    assert!(!window::is_visible(sim.world()));
    assert_eq!(events::<WindowFocus>(sim.world()), &[WindowFocus::Blurred]);
    assert_eq!(events::<Ack>(sim.world()), &[Ack]);
    drop(handle);
}

#[test]
fn another_thread_can_toggle() {
    let mut sim = Sim::new();
    let ctl = window::handle(sim.world());
    std::thread::spawn(move || ctl.toggle())
        .join()
        .expect("toggle");
    sim.update();
    assert!(!window::is_visible(sim.world()));
    assert_eq!(window::drain_effects(sim.world()), vec![WindowEffect::Hide]);

    let ctl = window::handle(sim.world());
    std::thread::spawn(move || {
        ctl.toggle();
    })
    .join()
    .expect("show");
    sim.update();
    assert!(window::is_visible(sim.world()));
    assert!(window::is_focused(sim.world()));
    assert_eq!(events::<WindowFocus>(sim.world()), &[WindowFocus::Focused]);
    assert_eq!(
        window::drain_effects(sim.world()),
        vec![WindowEffect::Show, WindowEffect::Focus]
    );
}

#[derive(Clone, Copy)]
struct SeenMetrics {
    width: f32,
    height: f32,
    scale: f32,
    event_scale: Option<f32>,
    event_width: Option<f32>,
}

fn layout_reads_metrics(world: &mut rndr_core::World) {
    let viewport = world.resource::<Viewport>().copied().unwrap();
    let event_scale = events::<ScaleChanged>(world)
        .first()
        .map(|event| event.scale);
    let event_width = events::<Resized>(world).first().map(|event| event.width);
    world.insert_resource(SeenMetrics {
        width: viewport.width,
        height: viewport.height,
        scale: viewport.scale,
        event_scale,
        event_width,
    });
}

#[test]
fn scale_and_resize_are_visible_to_layout() {
    let mut sim = Sim::new();
    sim.app.add_system(Schedule::Layout, layout_reads_metrics);

    window::publish_metrics(sim.world(), 1600, 1200, 2.0);
    assert!(near(
        sim.world().resource::<Viewport>().unwrap().width,
        800.0
    ));
    assert!(near(sim.world().resource::<Viewport>().unwrap().scale, 2.0));
    sim.update();
    let seen = *sim.world().resource::<SeenMetrics>().unwrap();
    assert!(near(seen.width, 800.0));
    assert!(near(seen.height, 600.0));
    assert!(near(seen.scale, 2.0));
    assert!(seen.event_scale.is_some_and(|scale| near(scale, 2.0)));
    assert!(seen.event_width.is_none());

    window::publish_metrics(sim.world(), 1000, 400, 2.0);
    sim.update();
    let seen = *sim.world().resource::<SeenMetrics>().unwrap();
    assert!(near(seen.width, 500.0));
    assert!(near(seen.height, 200.0));
    assert!(seen.event_width.is_some_and(|width| near(width, 500.0)));
    assert!(seen.event_scale.is_none());
    let resized = events::<Resized>(sim.world())[0];
    assert!(near(resized.scale, 2.0));
}

#[test]
fn a_zero_physical_size_keeps_the_logical_size() {
    let change = window::advance_metrics(WindowMetrics::default(), 0, 0, 2.0);
    assert!(change.scale_changed);
    assert!(!change.resized);
    assert!(near(change.metrics.width, 800.0));
    assert!(near(change.metrics.height, 600.0));
    assert!(near(change.metrics.scale, 2.0));
}

#[test]
fn publishing_the_same_size_is_quiet() {
    let mut sim = Sim::new();
    window::publish_metrics(sim.world(), 800, 600, 1.0);
    sim.update();
    assert!(events::<ScaleChanged>(sim.world()).is_empty());
    assert!(events::<Resized>(sim.world()).is_empty());
    assert!(near(
        sim.world().resource::<Viewport>().unwrap().width,
        800.0
    ));
}

#[test]
fn logical_size_constructor() {
    let size = LogicalSize::new(480.0, 270.0);
    assert!(near(size.width, 480.0));
    assert!(near(size.height, 270.0));
}
