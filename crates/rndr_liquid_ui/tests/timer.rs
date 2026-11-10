//! Recipe timers keep their phase across recycling and publish envelope events.
use rndr_core::*;
use rndr_liquid_ui::*;
use rndr_ui::{apply_theme_bridge, UiPlugin, UiTheme};
use std::sync::{Arc, Mutex};
use std::time::Duration;

fn has(app: &App, view: Entity, text: &str) -> bool {
    fn walk(world: &World, entity: Entity, out: &mut Vec<Entity>) {
        for child in children_of(world, entity) {
            out.push(child);
            walk(world, child, out);
        }
    }
    let mut all = Vec::new();
    walk(app.world(), view, &mut all);
    all.iter().any(|entity| {
        app.world()
            .get::<Label>(*entity)
            .is_some_and(|label| label.text == text)
    })
}

fn press(app: &mut App, root: Entity, label: &str) {
    fn walk(world: &World, entity: Entity, out: &mut Vec<Entity>) {
        for child in children_of(world, entity) {
            out.push(child);
            walk(world, child, out);
        }
    }
    let mut all = Vec::new();
    walk(app.world(), root, &mut all);
    let mut at = all
        .into_iter()
        .find(|entity| {
            app.world()
                .get::<Label>(*entity)
                .is_some_and(|l| l.text == label)
        })
        .unwrap_or_else(|| panic!("no {label}"));
    while try_activate(app.world_mut(), at) != ActivationResult::Accepted {
        at = app.world().get::<Parent>(at).unwrap().entity;
    }
    for request in take_action_intents(app.world_mut()) {
        dispatch(app.world_mut(), &request);
    }
    rndr_paint::frame(app);
}

struct EnvelopeLog(Arc<Mutex<Vec<LiquidInteractionEnvelope>>>);
impl LiquidInteractionHandler for EnvelopeLog {
    fn on_interaction(&mut self, _: &mut World, envelope: &LiquidInteractionEnvelope) {
        self.0.lock().unwrap().push(envelope.clone());
    }
}

struct HostLog(Arc<Mutex<Vec<LiquidEvent>>>);
impl LiquidHost for HostLog {
    fn event(&mut self, _: &mut World, event: LiquidEvent) {
        self.0.lock().unwrap().push(event);
    }
}

#[test]
fn a_recipe_timer_pauses_resumes_and_survives_recycling() {
    let envelopes = Arc::new(Mutex::new(Vec::new()));
    let host_events = Arc::new(Mutex::new(Vec::new()));
    let clock = Arc::new(ManualClock::new());
    let mut app = App::new();
    app.set_clock(Arc::clone(&clock) as Arc<dyn rndr_core::clock::Clock>);
    app.add_plugin(UiPlugin);
    app.add_plugin(MarkdownPlugin);
    app.add_plugin(LiquidUiPlugin::new(HostLog(Arc::clone(&host_events))));
    app.world_mut().insert_resource(Viewport::new(960.0, 640.0));
    apply_theme_bridge(app.world_mut(), UiTheme::default());
    let source = format!(
        "```recipe\ntitle: Soup\n\n---\nlabel: Simmer\nbody: Stir\nduration: 5s\n```\n\n{}",
        (0..80)
            .map(|i| format!("Filler paragraph {i} alpha beta gamma delta.\n\n"))
            .collect::<String>()
    );
    let view = liquid_view(app.world_mut(), &source);
    app.world_mut()
        .get_mut::<MarkdownView>(view)
        .unwrap()
        .set_follow(false);
    subscribe_liquid_interactions(app.world_mut(), EnvelopeLog(Arc::clone(&envelopes)));
    rndr_paint::frame(&mut app);
    rndr_paint::frame(&mut app);
    assert!(has(&app, view, "Start"));
    press(&mut app, view, "Start");
    let timer = liquid_timers(app.world(), view)
        .into_iter()
        .next()
        .expect("timer");
    assert_eq!(timer.status, LiquidTimerStatus::Running);
    assert_eq!(timer.duration_ms, 5_000);
    assert!(envelopes.lock().unwrap().iter().any(|envelope| {
        envelope.event == LiquidInteraction::TimerStart && envelope.node_id == "timer"
    }));
    assert!(host_events.lock().unwrap().iter().any(|event| {
        matches!(
            event,
            LiquidEvent::StartTimer {
                duration_ms: 5_000,
                ..
            }
        )
    }));
    clock.advance(Duration::from_secs(2));
    rndr_paint::frame(&mut app);
    press(&mut app, view, "Pause");
    let paused = liquid_timer(app.world(), view, timer.source, timer.step).unwrap();
    assert_eq!(paused.status, LiquidTimerStatus::Paused);
    assert!(paused.remaining_ms <= 3_000 && paused.remaining_ms >= 2_000);
    let bottom = {
        let scroll = app.world().get::<Scroll>(view).unwrap();
        let rect = app.world().get::<Rect>(view).unwrap();
        (rect.h - scroll.content[1]).min(0.0)
    };
    app.world_mut().get_mut::<Scroll>(view).unwrap().offset[1] = bottom;
    rndr_paint::frame(&mut app);
    assert!(!has(&app, view, "Resume"), "the recipe stayed mounted");
    assert_eq!(
        liquid_timer(app.world(), view, timer.source, timer.step)
            .unwrap()
            .status,
        LiquidTimerStatus::Paused
    );
    app.world_mut().get_mut::<Scroll>(view).unwrap().offset[1] = 0.0;
    rndr_paint::frame(&mut app);
    rndr_paint::frame(&mut app);
    assert!(has(&app, view, "Resume"));
    press(&mut app, view, "Reset");
    let reset = liquid_timer(app.world(), view, timer.source, timer.step).unwrap();
    assert_eq!(reset.status, LiquidTimerStatus::Idle);
    assert_eq!(reset.remaining_ms, 5_000);
    press(&mut app, view, "Start");
    clock.advance(Duration::from_secs(5));
    rndr_paint::frame(&mut app);
    let done = liquid_timer(app.world(), view, timer.source, timer.step).unwrap();
    assert_eq!(done.status, LiquidTimerStatus::Complete);
    assert!(has(&app, view, "Done"));
    let logged = envelopes.lock().unwrap().clone();
    assert!(logged
        .iter()
        .any(|envelope| envelope.event == LiquidInteraction::TimerPause));
    assert!(logged
        .iter()
        .any(|envelope| envelope.event == LiquidInteraction::TimerReset));
    assert!(logged
        .iter()
        .any(|envelope| envelope.event == LiquidInteraction::TimerComplete));
}
