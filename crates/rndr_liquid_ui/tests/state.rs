//! Component records and interaction envelopes survive recycling and appends.
use rndr_core::*;
use rndr_liquid_ui::*;
use rndr_ui::{apply_theme_bridge, UiPlugin, UiTheme};
use std::sync::{Arc, Mutex};

fn boot(source: &str) -> (App, Entity) {
    let mut app = App::new();
    app.add_plugin(UiPlugin);
    app.add_plugin(MarkdownPlugin);
    app.add_plugin(LiquidUiPlugin::new(NoHost));
    app.world_mut().insert_resource(Viewport::new(960.0, 640.0));
    apply_theme_bridge(app.world_mut(), UiTheme::default());
    let view = liquid_view(app.world_mut(), source);
    app.world_mut()
        .get_mut::<MarkdownView>(view)
        .unwrap()
        .set_follow(false);
    rndr_paint::frame(&mut app);
    rndr_paint::frame(&mut app);
    (app, view)
}

fn prose(n: usize) -> String {
    (0..n)
        .map(|i| format!("Filler paragraph {i} alpha beta gamma delta.\n\n"))
        .collect()
}

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

fn scroll_to(app: &mut App, view: Entity, offset: f32) {
    app.world_mut().get_mut::<Scroll>(view).unwrap().offset[1] = offset;
    rndr_paint::frame(app);
}

fn bottom(app: &App, view: Entity) -> f32 {
    let scroll = app.world().get::<Scroll>(view).unwrap();
    let rect = app.world().get::<Rect>(view).unwrap();
    (rect.h - scroll.content[1]).min(0.0)
}

struct Log(Arc<Mutex<Vec<LiquidInteractionEnvelope>>>);

impl LiquidInteractionHandler for Log {
    fn on_interaction(&mut self, _: &mut World, envelope: &LiquidInteractionEnvelope) {
        self.0.lock().unwrap().push(envelope.clone());
    }
}

const ACCORDION: &str = "\
```accordion
title: FAQ

---
label: One
body: First body
---
label: Two
body: Second body
```
";

#[test]
fn accordion_state_is_recorded_and_survives_release_and_append() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let (mut app, view) = boot(&format!("{ACCORDION}\n{}", prose(80)));
    subscribe_liquid_interactions(app.world_mut(), Log(Arc::clone(&events)));
    press(&mut app, view, "+ One");
    let record = liquid_component_states(app.world(), view)
        .into_iter()
        .find(|record| record.component_type == "accordion")
        .expect("accordion record");
    assert_eq!(record.component_type, "accordion");
    assert_eq!(record.revision, 1);
    assert_eq!(
        record.state,
        LiquidComponentState::Accordion { open: vec![0] }
    );
    let logged = events.lock().unwrap().clone();
    assert!(
        logged.iter().any(|envelope| {
            envelope.event == LiquidInteraction::Expand
                && envelope.disposition == LiquidDisposition::LocalState
                && envelope.instance_id == "0"
                && envelope.version == 1
        }),
        "{logged:?}"
    );
    let parked_at = bottom(&app, view);
    scroll_to(&mut app, view, parked_at);
    assert!(!has(&app, view, "First body"));
    let parked = liquid_component_state(app.world(), view, record.source).unwrap();
    assert_eq!(parked.state, record.state);
    scroll_to(&mut app, view, 0.0);
    rndr_paint::frame(&mut app);
    assert!(has(&app, view, "First body"));
    push_liquid_str(app.world_mut(), view, "\n\nTail after the accordion.\n");
    rndr_paint::frame(&mut app);
    rndr_paint::frame(&mut app);
    assert!(has(&app, view, "First body"));
    assert!(
        app.world()
            .get::<MarkdownView>(view)
            .unwrap()
            .source()
            .contains("Tail after the accordion."),
        "the append was dropped"
    );
    assert_eq!(
        liquid_component_state(app.world(), view, record.source)
            .unwrap()
            .state,
        record.state
    );
    let stale = put_liquid_component_state(
        app.world_mut(),
        view,
        record.source,
        0,
        LiquidComponentState::Accordion { open: vec![] },
    );
    assert_eq!(
        stale,
        Err(LiquidRevisionError {
            expected: 0,
            actual: record.revision
        })
    );
}

#[test]
fn tabs_and_checklists_round_trip_through_the_record() {
    let source = format!(
        "```tabs\n---\nlabel: Alpha\nbody: Alpha body.\n---\nlabel: Beta\nbody: Beta body.\n```\n\n\
```steps\ntitle: Build\n\n---\nlabel: Mix\nbody: Stir\n---\nlabel: Bake\nbody: Heat\n```\n\n{}",
        prose(80)
    );
    let (mut app, view) = boot(&source);
    press(&mut app, view, "Beta");
    press(&mut app, view, "[ ]");
    let records = liquid_component_states(app.world(), view);
    let tabs = records
        .iter()
        .find(|record| record.component_type == "tabs")
        .expect("tabs")
        .clone();
    let steps = records
        .iter()
        .find(|record| record.component_type == "steps")
        .expect("steps")
        .clone();
    assert_eq!(tabs.state, LiquidComponentState::Tabs { selected: 1 });
    assert_eq!(
        steps.state,
        LiquidComponentState::Checklist { checked: vec![0] }
    );
    let parked_at = bottom(&app, view);
    scroll_to(&mut app, view, parked_at);
    assert!(!has(&app, view, "Beta body."));
    scroll_to(&mut app, view, 0.0);
    rndr_paint::frame(&mut app);
    assert_eq!(tab_selected(app.world(), view), Some(1));
    assert!(has(&app, view, "[x]"));
    assert_eq!(
        liquid_component_state(app.world(), view, tabs.source)
            .unwrap()
            .state,
        tabs.state
    );
}

fn tab_selected(world: &World, root: Entity) -> Option<u64> {
    let mut stack = vec![root];
    while let Some(entity) = stack.pop() {
        if let Some(tabs) = world.get::<Tabs>(entity) {
            return Some(tabs.selected);
        }
        stack.extend(children_of(world, entity));
    }
    None
}
