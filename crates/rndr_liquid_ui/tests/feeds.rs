//! Async feed loads: phases, the five data types, refresh, revisions, recycling.
use rndr_core::*;
use rndr_liquid_ui::*;
use rndr_ui::{apply_theme_bridge, UiPlugin, UiTheme};
use std::sync::{Arc, Mutex};
use std::time::Duration;

struct Gate {
    outcome: Arc<Mutex<FeedOutcome>>,
    events: Arc<Mutex<Vec<LiquidEvent>>>,
}

impl LiquidHost for Gate {
    fn event(&mut self, _: &mut World, event: LiquidEvent) {
        self.events.lock().unwrap().push(event);
    }
    fn load_feed(&mut self, _: &mut World, request: &FeedRequest) -> FeedOutcome {
        match &*self.outcome.lock().unwrap() {
            FeedOutcome::Ready(body) => FeedOutcome::Ready(FeedBody {
                datatype: request.datatype,
                body: body.body.clone(),
            }),
            other => other.clone(),
        }
    }
}

struct ById {
    bodies: Arc<Mutex<Vec<(String, FeedDatatype, String)>>>,
}

impl LiquidHost for ById {
    fn load_feed(&mut self, _: &mut World, request: &FeedRequest) -> FeedOutcome {
        let bodies = self.bodies.lock().unwrap();
        let Some((_, datatype, body)) = bodies.iter().find(|(id, _, _)| id == &request.feed_id)
        else {
            return FeedOutcome::Failed {
                message: "missing".into(),
            };
        };
        FeedOutcome::Ready(FeedBody {
            datatype: *datatype,
            body: body.clone(),
        })
    }
}

struct EnvelopeLog(Arc<Mutex<Vec<LiquidInteractionEnvelope>>>);
impl LiquidInteractionHandler for EnvelopeLog {
    fn on_interaction(&mut self, _: &mut World, envelope: &LiquidInteractionEnvelope) {
        self.0.lock().unwrap().push(envelope.clone());
    }
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

fn open(source: &str, host: impl LiquidHost + 'static) -> (App, Entity) {
    let mut app = App::new();
    app.add_plugin(UiPlugin);
    app.add_plugin(MarkdownPlugin);
    app.add_plugin(LiquidUiPlugin::new(host));
    app.world_mut().insert_resource(Viewport::new(960.0, 640.0));
    apply_theme_bridge(app.world_mut(), UiTheme::default());
    let view = liquid_view(app.world_mut(), source);
    app.world_mut()
        .get_mut::<MarkdownView>(view)
        .unwrap()
        .set_follow(false);
    (app, view)
}

fn slot(world: &World, view: Entity, instance: &str) -> FeedSlot {
    liquid_component_states(world, view)
        .into_iter()
        .find_map(|record| match record.state {
            LiquidComponentState::Feeds { slots } => {
                slots.into_iter().find(|slot| slot.instance == instance)
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("no feed slot {instance}"))
}

fn record(world: &World, view: Entity) -> LiquidComponentRecord {
    liquid_component_states(world, view)
        .into_iter()
        .find(|record| record.component_type == "feed")
        .expect("feed record")
}

const FEED: &str = "\
```feed
id: news
datatype: text
title: News
empty: Nothing yet
```
";

#[test]
fn feed_load_moves_through_loading_ready_error_and_stale() {
    let outcome = Arc::new(Mutex::new(FeedOutcome::Pending));
    let (mut app, view) = open(
        FEED,
        Gate {
            outcome: Arc::clone(&outcome),
            events: Arc::new(Mutex::new(Vec::new())),
        },
    );
    rndr_paint::frame(&mut app);
    assert!(has(&app, view, "Loading feed…"));
    assert_eq!(slot(app.world(), view, "news").phase, FeedPhase::Loading);
    *outcome.lock().unwrap() = FeedOutcome::Ready(FeedBody {
        datatype: FeedDatatype::Text,
        body: "Kept body".into(),
    });
    rndr_paint::frame(&mut app);
    assert!(has(&app, view, "Kept body"));
    assert_eq!(slot(app.world(), view, "news").phase, FeedPhase::Ready);
    *outcome.lock().unwrap() = FeedOutcome::Failed {
        message: "offline".into(),
    };
    press(&mut app, view, "Refresh");
    assert!(has(&app, view, "Kept body"));
    assert!(has(&app, view, "Feed error: offline"));
    assert_eq!(slot(app.world(), view, "news").phase, FeedPhase::Stale);
    let fresh = "\
```feed
id: missing
datatype: text
title: Missing
```
";
    *outcome.lock().unwrap() = FeedOutcome::Failed {
        message: "gone".into(),
    };
    let (mut app, view) = open(
        fresh,
        Gate {
            outcome,
            events: Arc::new(Mutex::new(Vec::new())),
        },
    );
    rndr_paint::frame(&mut app);
    rndr_paint::frame(&mut app);
    assert!(!has(&app, view, "Kept body"));
    assert!(has(&app, view, "Feed error: gone"));
    assert_eq!(slot(app.world(), view, "missing").phase, FeedPhase::Error);
}

#[test]
fn each_feed_datatype_renders_its_body() {
    let bodies = Arc::new(Mutex::new(vec![
        ("notes".into(), FeedDatatype::Md, "Markdown body".into()),
        ("plain".into(), FeedDatatype::Text, "Plain line".into()),
        ("payload".into(), FeedDatatype::Json, "{\"n\":1}".into()),
        (
            "sheet".into(),
            FeedDatatype::Csv,
            "Name,Score\nAda,9".into(),
        ),
        (
            "picture".into(),
            FeedDatatype::Image,
            "https://example.test/plot.png".into(),
        ),
    ]));
    let source = "\
```feed
id: notes
datatype: md
title: Notes
```

```feed
id: plain
datatype: text
title: Plain
```

```feed
id: payload
datatype: json
title: Payload
```

```feed
id: sheet
datatype: csv
title: Sheet
```

```feed
id: picture
datatype: image
title: Picture
```
";
    let (mut app, view) = open(source, ById { bodies });
    rndr_paint::frame(&mut app);
    rndr_paint::frame(&mut app);
    for want in [
        "Markdown body",
        "Plain line",
        "{\"n\":1}",
        "Name",
        "Ada",
        "https://example.test/plot.png",
    ] {
        assert!(has(&app, view, want), "missing {want}");
    }
    assert_eq!(slot(app.world(), view, "notes").datatype, "md");
    assert_eq!(slot(app.world(), view, "plain").datatype, "text");
    assert_eq!(slot(app.world(), view, "payload").datatype, "json");
    assert_eq!(slot(app.world(), view, "sheet").datatype, "csv");
    assert_eq!(slot(app.world(), view, "picture").datatype, "image");
}

#[test]
fn manual_and_interval_refresh_publish_envelopes() {
    let outcome = Arc::new(Mutex::new(FeedOutcome::Ready(FeedBody {
        datatype: FeedDatatype::Text,
        body: "first".into(),
    })));
    let envelopes = Arc::new(Mutex::new(Vec::new()));
    let events = Arc::new(Mutex::new(Vec::new()));
    let clock = Arc::new(ManualClock::new());
    let mut app = App::new();
    app.set_clock(Arc::clone(&clock) as Arc<dyn rndr_core::clock::Clock>);
    app.add_plugin(UiPlugin);
    app.add_plugin(MarkdownPlugin);
    app.add_plugin(LiquidUiPlugin::new(Gate {
        outcome: Arc::clone(&outcome),
        events: Arc::clone(&events),
    }));
    app.world_mut().insert_resource(Viewport::new(960.0, 640.0));
    apply_theme_bridge(app.world_mut(), UiTheme::default());
    subscribe_liquid_interactions(app.world_mut(), EnvelopeLog(Arc::clone(&envelopes)));
    let view = liquid_view(
        app.world_mut(),
        "\
```feed
id: news
datatype: text
title: News
```

```dashboard
title: Ops

---
label: Users
value: 10
feed: metrics
field: users
refresh: both
interval: 1s
---
label: Errors
value: 3
```
",
    );
    app.world_mut()
        .get_mut::<MarkdownView>(view)
        .unwrap()
        .set_follow(false);
    rndr_paint::frame(&mut app);
    rndr_paint::frame(&mut app);
    assert!(has(&app, view, "first"));
    *outcome.lock().unwrap() = FeedOutcome::Ready(FeedBody {
        datatype: FeedDatatype::Text,
        body: "second".into(),
    });
    press(&mut app, view, "Refresh");
    assert!(has(&app, view, "second"));
    assert!(events.lock().unwrap().iter().any(|event| {
        matches!(event, LiquidEvent::RefreshFeed { feed_id } if feed_id == "news")
    }));
    *outcome.lock().unwrap() = FeedOutcome::Ready(FeedBody {
        datatype: FeedDatatype::Text,
        body: "1,204".into(),
    });
    clock.advance(Duration::from_secs(1));
    rndr_paint::frame(&mut app);
    assert!(
        has(&app, view, "1,204"),
        "interval did not refresh the tile"
    );
    let logged = envelopes.lock().unwrap().clone();
    assert!(logged.iter().any(|envelope| {
        envelope.event == LiquidInteraction::FeedRefresh
            && envelope.event.name() == "feed_refresh"
            && envelope.instance_id == "news"
    }));
    assert!(logged.iter().any(|envelope| {
        envelope.event == LiquidInteraction::FeedRefresh && envelope.node_id == "feed"
    }));
}

#[test]
fn a_stale_feed_revision_is_rejected() {
    let outcome = Arc::new(Mutex::new(FeedOutcome::Ready(FeedBody {
        datatype: FeedDatatype::Text,
        body: "kept".into(),
    })));
    let (mut app, view) = open(
        FEED,
        Gate {
            outcome,
            events: Arc::new(Mutex::new(Vec::new())),
        },
    );
    rndr_paint::frame(&mut app);
    rndr_paint::frame(&mut app);
    let current = record(app.world(), view);
    assert!(current.revision >= 1);
    let stale = put_liquid_component_state(
        app.world_mut(),
        view,
        current.source,
        0,
        LiquidComponentState::Feeds { slots: Vec::new() },
    );
    assert_eq!(
        stale,
        Err(LiquidRevisionError {
            expected: 0,
            actual: current.revision
        })
    );
    assert!(has(&app, view, "kept"));
    assert_eq!(record(app.world(), view).state, current.state);
}

#[test]
fn feed_last_good_survives_recycling() {
    let outcome = Arc::new(Mutex::new(FeedOutcome::Ready(FeedBody {
        datatype: FeedDatatype::Text,
        body: "Kept body".into(),
    })));
    let source = format!(
        "{FEED}\n{}",
        (0..80)
            .map(|i| format!("Filler paragraph {i} alpha beta gamma delta.\n\n"))
            .collect::<String>()
    );
    let (mut app, view) = open(
        &source,
        Gate {
            outcome: Arc::clone(&outcome),
            events: Arc::new(Mutex::new(Vec::new())),
        },
    );
    rndr_paint::frame(&mut app);
    rndr_paint::frame(&mut app);
    assert!(has(&app, view, "Kept body"));
    *outcome.lock().unwrap() = FeedOutcome::Failed {
        message: "offline".into(),
    };
    let bottom = {
        let scroll = app.world().get::<Scroll>(view).unwrap();
        let rect = app.world().get::<Rect>(view).unwrap();
        (rect.h - scroll.content[1]).min(0.0)
    };
    app.world_mut().get_mut::<Scroll>(view).unwrap().offset[1] = bottom;
    rndr_paint::frame(&mut app);
    assert!(!has(&app, view, "Kept body"), "the feed stayed mounted");
    assert_eq!(slot(app.world(), view, "news").phase, FeedPhase::Ready);
    app.world_mut().get_mut::<Scroll>(view).unwrap().offset[1] = 0.0;
    rndr_paint::frame(&mut app);
    assert!(has(&app, view, "Kept body"));
    assert!(!has(&app, view, "Feed error: offline"));
}
