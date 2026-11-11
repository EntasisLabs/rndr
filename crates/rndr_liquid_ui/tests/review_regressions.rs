//! Public-path regressions from independent review of d2edd1ad.
use rndr_core::*;
use rndr_liquid_ui::*;
use rndr_ui::{apply_theme_bridge, UiTheme};
use std::sync::{Arc, Mutex};

const ACC: &str = "```accordion\ntitle: FAQ\n---\nlabel: One\nbody: First body\n```\n";
const TABS: &str =
    "```tabs\n---\nlabel: Alpha\nbody: Alpha body\n---\nlabel: Beta\nbody: Beta body\n```\n";
fn boot(host: impl LiquidHost + 'static, limits: LiquidLimits) -> App {
    let mut app = App::new();
    app.add_plugin(MarkdownPlugin);
    app.add_plugin(LiquidUiPlugin::new(host).with_limits(limits).unwrap());
    app.world_mut().insert_resource(Viewport::new(960.0, 640.0));
    apply_theme_bridge(app.world_mut(), UiTheme::default());
    app
}
fn pump(app: &mut App) {
    rndr_paint::frame(app);
}
fn open(app: &mut App, source: &str) -> Entity {
    let view = liquid_view(app.world_mut(), source);
    app.world_mut()
        .get_mut::<MarkdownView>(view)
        .unwrap()
        .set_follow(false);
    pump(app);
    pump(app);
    view
}
fn find(world: &World, root: Entity, text: &str) -> Option<Entity> {
    if world.get::<Label>(root).is_some_and(|l| l.text == text) {
        return Some(root);
    }
    children_of(world, root)
        .into_iter()
        .find_map(|e| find(world, e, text))
}
fn activate(world: &mut World, root: Entity, text: &str) {
    let mut at = find(world, root, text).unwrap_or_else(|| panic!("missing {text}"));
    while try_activate(world, at) != ActivationResult::Accepted {
        at = world.get::<Parent>(at).unwrap().entity;
    }
    for request in take_action_intents(world) {
        dispatch(world, &request);
    }
}
struct Log(Arc<Mutex<Vec<LiquidInteractionEnvelope>>>);
impl LiquidInteractionHandler for Log {
    fn on_interaction(&mut self, _: &mut World, e: &LiquidInteractionEnvelope) {
        self.0.lock().unwrap().push(e.clone());
    }
}
#[test]
fn independent_surface_survives_parent_collapse_rebuild_and_cleanup() {
    for cleanup in 0..5 {
        let context = Arc::new(Mutex::new(None));
        let retired = Arc::new(Mutex::new(0));
        let mut app = boot(
            PendingHost {
                context: context.clone(),
                retired: retired.clone(),
                capture: None,
            },
            LiquidLimits::default(),
        );
        let parent_source = format!("{ACC}\n{}", "Filler paragraph alpha beta.\n\n".repeat(100));
        let a = open(&mut app, &parent_source);
        activate(app.world_mut(), a, "+ One");
        pump(&mut app);
        let b = open(
            &mut app,
            &format!("{ACC}\n```feed\nid: news\ndatatype: text\ntitle: News\n```\n"),
        );
        activate(app.world_mut(), b, "+ One");
        pump(&mut app);
        let handle = liquid_surface_handle(app.world(), b).unwrap();
        let records = liquid_component_states(app.world(), b);
        let body = find(app.world(), a, "First body").unwrap();
        let wrapper = app.world_mut().spawn(Layout::column());
        set_parent(app.world_mut(), wrapper, body);
        set_parent(app.world_mut(), b, wrapper);
        // Also protect a nested unrelated Liquid subtree, not just B's shell.
        let c = open(&mut app, ACC);
        let body_b = find(app.world(), b, "First body").unwrap();
        set_parent(app.world_mut(), c, body_b);
        let c_handle = liquid_surface_handle(app.world(), c).unwrap();
        let native = MarkdownSlotHosts::native_projections(app.world(), b);
        let feed_context = context.lock().unwrap().clone().unwrap();
        let events = Arc::new(Mutex::new(Vec::new()));
        let _listener = subscribe_liquid_interactions_owned(
            app.world_mut(),
            LiquidListenerOwner::Surface(handle),
            Log(events.clone()),
        )
        .unwrap();
        match cleanup {
            0 => {
                activate(app.world_mut(), a, "− One");
                pump(&mut app);
                pump(&mut app);
                activate(app.world_mut(), a, "+ One");
            }
            1 => {
                set_liquid_source(app.world_mut(), a, "Replacement\n");
            }
            2 => {
                app.world_mut().despawn_entity(a);
            }
            3 => {
                app.world_mut().remove::<LiquidView>(a);
            }
            _ => {
                app.world_mut().get_mut::<Scroll>(a).unwrap().offset[1] = -5000.0;
                pump(&mut app);
                pump(&mut app);
                app.world_mut().get_mut::<Scroll>(a).unwrap().offset[1] = 0.0;
            }
        }
        for _ in 0..4 {
            pump(&mut app);
        }
        assert!(app.world().has::<LiquidView>(b), "cleanup={cleanup}");
        assert_eq!(liquid_surface_handle(app.world(), b).unwrap(), handle);
        assert_eq!(liquid_component_states(app.world(), b), records);
        assert_eq!(liquid_surface_handle(app.world(), c).unwrap(), c_handle);
        assert!(find(app.world(), b, "First body").is_some());
        assert_eq!(
            MarkdownSlotHosts::native_projections(app.world(), b),
            native
        );
        assert!(!feed_context.is_retired());
        assert_eq!(*retired.lock().unwrap(), 0);
        activate(app.world_mut(), b, "− One");
        pump(&mut app);
        assert_eq!(
            events
                .lock()
                .unwrap()
                .iter()
                .filter(|e| e.node_id == "accordion" && e.event == LiquidInteraction::Collapse)
                .count(),
            1,
            "independent listener remains eligible"
        );
        push_liquid_str(app.world_mut(), b, "\nTail\n");
        app.world_mut().despawn_entity(b);
        app.world_mut().despawn_entity(c);
        for _ in 0..4 {
            pump(&mut app);
        }
        assert!(liquid_surface_handle(app.world(), b).is_err());
        assert!(feed_context.is_retired());
        assert_eq!(*retired.lock().unwrap(), 1);
    }
}
#[test]
fn native_tabs_defer_atomically_until_envelope_capacity_is_available() {
    let mut app = boot(
        NoHost,
        LiquidLimits {
            callbacks: 1,
            envelopes: 1,
            ..LiquidLimits::default()
        },
    );
    let view = open(&mut app, &format!("{ACC}\n{TABS}"));
    let logs: Vec<_> = (0..3).map(|_| Arc::new(Mutex::new(Vec::new()))).collect();
    for log in &logs {
        subscribe_liquid_interactions(app.world_mut(), Log(log.clone()));
    }
    activate(app.world_mut(), view, "+ One");
    activate(app.world_mut(), view, "Beta");
    pump(&mut app);
    assert!(
        liquid_component_states(app.world(), view)
            .iter()
            .all(|r| r.component_type != "tabs"),
        "full queue must not commit a tab revision"
    );
    for _ in 0..8 {
        pump(&mut app);
    }
    let tabs = liquid_component_states(app.world(), view)
        .into_iter()
        .find(|r| r.component_type == "tabs")
        .unwrap();
    assert_eq!(tabs.state, LiquidComponentState::Tabs { selected: 1 });
    assert_eq!(tabs.revision, 1);
    for log in logs {
        let log = log.lock().unwrap();
        assert_eq!(
            log.iter()
                .filter(|e| e.node_id == "tabs" && e.revision == 1)
                .count(),
            1,
            "{log:?}"
        );
    }
}
struct PendingHost {
    context: Arc<Mutex<Option<LiquidFeedContext>>>,
    retired: Arc<Mutex<usize>>,
    capture: Option<Arc<()>>,
}
impl LiquidHost for PendingHost {
    fn load_feed_for(
        &mut self,
        _: &mut World,
        c: &LiquidFeedContext,
        _: &FeedRequest,
    ) -> FeedOutcome {
        *self.context.lock().unwrap() = Some(c.clone());
        FeedOutcome::Pending
    }
    fn retire_feed_observation(&mut self, _: &mut World, c: &LiquidFeedContext) {
        *self.retired.lock().unwrap() += 1;
        self.capture.take();
        c.acknowledge_retirement();
    }
}
struct Chain {
    view: Entity,
    calls: Arc<Mutex<usize>>,
}
impl LiquidInteractionHandler for Chain {
    fn on_interaction(&mut self, w: &mut World, _: &LiquidInteractionEnvelope) {
        *self.calls.lock().unwrap() += 1;
        let label = if find(w, self.view, "− One").is_some() {
            "− One"
        } else {
            "+ One"
        };
        activate(w, self.view, label);
    }
}
#[test]
fn retirement_and_listeners_both_progress_under_sustained_reentrant_load() {
    let context = Arc::new(Mutex::new(None));
    let retired = Arc::new(Mutex::new(0));
    let capture = Arc::new(());
    let weak = Arc::downgrade(&capture);
    let mut app = boot(
        PendingHost {
            context: context.clone(),
            retired: retired.clone(),
            capture: Some(capture),
        },
        LiquidLimits {
            callbacks: 1,
            ..LiquidLimits::default()
        },
    );
    let a = open(&mut app, ACC);
    let feed = open(
        &mut app,
        "```feed\nid: news\ndatatype: text\ntitle: News\n```\n",
    );
    let old = context
        .lock()
        .unwrap()
        .clone()
        .expect("pending observation");
    let calls = Arc::new(Mutex::new(0));
    subscribe_liquid_interactions(
        app.world_mut(),
        Chain {
            view: a,
            calls: calls.clone(),
        },
    );
    activate(app.world_mut(), a, "+ One");
    set_liquid_source(app.world_mut(), feed, "Removed\n");
    let before = *calls.lock().unwrap();
    for _ in 0..4 {
        let start = *calls.lock().unwrap() + *retired.lock().unwrap();
        pump(&mut app);
        assert!(*calls.lock().unwrap() + *retired.lock().unwrap() - start <= 1);
    }
    assert_eq!(*retired.lock().unwrap(), 1, "retirement must not starve");
    assert!(weak.upgrade().is_none(), "host capture released");
    assert!(old.is_retired());
    assert_eq!(
        put_liquid_feed_result(app.world_mut(), &old, 0, FeedOutcome::Pending),
        Err(LiquidUpdateError::StaleOwner)
    );
    assert!(
        *calls.lock().unwrap() > before,
        "listeners must not starve either"
    );
    for _ in 0..8 {
        pump(&mut app);
    }
    assert_eq!(*retired.lock().unwrap(), 1);
    assert!(*calls.lock().unwrap() >= before + 8);
}

#[test]
fn deferred_native_tab_intent_can_be_superseded_without_a_phantom_commit() {
    for replace in [false, true] {
        let mut app = boot(
            NoHost,
            LiquidLimits {
                callbacks: 1,
                envelopes: 1,
                ..LiquidLimits::default()
            },
        );
        let view = open(&mut app, &format!("{ACC}\n{TABS}"));
        let log = Arc::new(Mutex::new(Vec::new()));
        for _ in 0..3 {
            subscribe_liquid_interactions(app.world_mut(), Log(log.clone()));
        }
        activate(app.world_mut(), view, "+ One");
        activate(app.world_mut(), view, "Beta");
        pump(&mut app); // queue still full, so Beta has not been admitted
        if replace {
            set_liquid_source(app.world_mut(), view, "Replacement\n");
        } else {
            activate(app.world_mut(), view, "Alpha");
        }
        for _ in 0..8 {
            pump(&mut app);
        }
        assert!(log.lock().unwrap().iter().all(|e| e.node_id != "tabs"));
        assert!(liquid_component_states(app.world(), view)
            .iter()
            .all(|r| r.component_type != "tabs"));
    }
}

struct ReentrantTab {
    view: Entity,
    once: bool,
}
impl LiquidInteractionHandler for ReentrantTab {
    fn on_interaction(&mut self, world: &mut World, _: &LiquidInteractionEnvelope) {
        if !self.once {
            self.once = true;
            activate(world, self.view, "Beta");
        }
    }
}
#[test]
fn native_tab_activation_inside_partial_delivery_is_admitted_once() {
    let mut app = boot(
        NoHost,
        LiquidLimits {
            callbacks: 1,
            envelopes: 1,
            ..LiquidLimits::default()
        },
    );
    let view = open(&mut app, &format!("{ACC}\n{TABS}"));
    subscribe_liquid_interactions(app.world_mut(), ReentrantTab { view, once: false });
    let log = Arc::new(Mutex::new(Vec::new()));
    for _ in 0..2 {
        subscribe_liquid_interactions(app.world_mut(), Log(log.clone()));
    }
    activate(app.world_mut(), view, "+ One");
    pump(&mut app);
    assert!(liquid_component_states(app.world(), view)
        .iter()
        .all(|r| r.component_type != "tabs"));
    for _ in 0..8 {
        pump(&mut app);
    }
    let log = log.lock().unwrap();
    assert_eq!(
        log.iter()
            .filter(|e| e.node_id == "tabs" && e.revision == 1)
            .count(),
        2
    );
}

struct ReentrantFeed {
    view: Arc<Mutex<Option<Entity>>>,
    once: bool,
}
impl LiquidHost for ReentrantFeed {
    fn load_feed_for(
        &mut self,
        world: &mut World,
        _: &LiquidFeedContext,
        _: &FeedRequest,
    ) -> FeedOutcome {
        if !self.once {
            self.once = true;
            activate(world, self.view.lock().unwrap().unwrap(), "+ One");
        }
        FeedOutcome::Ready(FeedBody {
            datatype: FeedDatatype::Text,
            body: "Outcome".into(),
        })
    }
}
#[test]
fn feed_reserves_envelope_capacity_across_reentrant_loader() {
    let owner = Arc::new(Mutex::new(None));
    let mut app = boot(
        ReentrantFeed {
            view: owner.clone(),
            once: false,
        },
        LiquidLimits {
            callbacks: 1,
            envelopes: 1,
            ..LiquidLimits::default()
        },
    );
    let a = open(&mut app, ACC);
    *owner.lock().unwrap() = Some(a);
    let log = Arc::new(Mutex::new(Vec::new()));
    for _ in 0..3 {
        subscribe_liquid_interactions(app.world_mut(), Log(log.clone()));
    }
    let feed = liquid_view(
        app.world_mut(),
        "```feed\nid: news\ndatatype: text\ntitle: News\n```\n",
    );
    pump(&mut app);
    assert!(
        liquid_component_states(app.world(), a).is_empty(),
        "host reentry must not consume the feed's reserved capacity"
    );
    for _ in 0..10 {
        pump(&mut app);
    }
    assert!(find(app.world(), feed, "Outcome").is_some());
    assert_eq!(
        log.lock()
            .unwrap()
            .iter()
            .filter(|e| e.node_id == "feed")
            .count(),
        3
    );
}

#[test]
fn retirement_backlog_cannot_starve_reentrant_listener_traffic() {
    let context = Arc::new(Mutex::new(None));
    let retired = Arc::new(Mutex::new(0));
    let mut app = boot(
        PendingHost {
            context,
            retired: retired.clone(),
            capture: None,
        },
        LiquidLimits {
            callbacks: 1,
            ..LiquidLimits::default()
        },
    );
    let a = open(&mut app, ACC);
    let feeds = (0..8)
        .map(|i| format!("```feed\nid: news{i}\ndatatype: text\ntitle: News{i}\n```\n\n"))
        .collect::<String>();
    let feed = open(&mut app, &feeds);
    // Ensure every observation is mounted (rather than relying on viewport height).
    app.world_mut()
        .insert_resource(Viewport::new(960.0, 8000.0));
    pump(&mut app);
    pump(&mut app);
    let calls = Arc::new(Mutex::new(0));
    subscribe_liquid_interactions(
        app.world_mut(),
        Chain {
            view: a,
            calls: calls.clone(),
        },
    );
    activate(app.world_mut(), a, "+ One");
    set_liquid_source(app.world_mut(), feed, "Removed\n");
    let before = *calls.lock().unwrap();
    for tick in 0..16 {
        let start = *calls.lock().unwrap() + *retired.lock().unwrap();
        pump(&mut app);
        assert!(*calls.lock().unwrap() + *retired.lock().unwrap() - start <= 1);
        if tick == 1 {
            assert!(
                *calls.lock().unwrap() > before,
                "listener progress within two pumps"
            );
            assert!(
                *retired.lock().unwrap() > 0,
                "retirement progress within two pumps"
            );
        }
    }
    assert_eq!(*retired.lock().unwrap(), 8);
    assert!(*calls.lock().unwrap() >= before + 8);
    for _ in 0..4 {
        pump(&mut app);
    }
    assert_eq!(*retired.lock().unwrap(), 8, "exactly once per observation");
}

#[test]
fn recycling_refuses_unadmitted_native_tab_without_changing_revision() {
    let mut app = boot(
        NoHost,
        LiquidLimits {
            callbacks: 1,
            envelopes: 1,
            ..LiquidLimits::default()
        },
    );
    let tail = "Filler paragraph alpha beta gamma.\n\n".repeat(100);
    let view = open(&mut app, &format!("{ACC}\n{TABS}\n{tail}"));
    let log = Arc::new(Mutex::new(Vec::new()));
    for _ in 0..3 {
        subscribe_liquid_interactions(app.world_mut(), Log(log.clone()));
    }
    activate(app.world_mut(), view, "+ One");
    activate(app.world_mut(), view, "Beta");
    app.world_mut().get_mut::<Scroll>(view).unwrap().offset[1] = -5000.0;
    pump(&mut app);
    assert!(liquid_component_states(app.world(), view)
        .iter()
        .all(|r| r.component_type != "tabs"));
    for _ in 0..5 {
        pump(&mut app);
    }
    assert!(log.lock().unwrap().iter().all(|e| e.node_id != "tabs"));
    app.world_mut().get_mut::<Scroll>(view).unwrap().offset[1] = 0.0;
    pump(&mut app);
    pump(&mut app);
    // The refused presentation intent cannot reappear after remounting.
    assert!(liquid_component_states(app.world(), view)
        .iter()
        .all(|r| !matches!(r.state, LiquidComponentState::Tabs { selected: 1 })));
}

#[test]
fn retained_logical_child_parks_when_its_parent_occurrence_moves() {
    let mut app = boot(NoHost, LiquidLimits::default());
    let view = open(&mut app, ACC);
    activate(app.world_mut(), view, "+ One");
    pump(&mut app);
    let child = app
        .world()
        .entities_with::<LiquidView>()
        .into_iter()
        .find(|e| *e != view && find(app.world(), *e, "First body").is_some())
        .unwrap();
    let handle = liquid_surface_handle(app.world(), child).unwrap();
    set_liquid_source(
        app.world_mut(),
        view,
        &format!("```callout\ntitle: New prefix\nbody: New\n```\n\n{ACC}"),
    );
    for _ in 0..4 {
        pump(&mut app);
    }
    let children: Vec<_> = app
        .world()
        .entities_with::<LiquidView>()
        .into_iter()
        .filter(|e| *e != view && find(app.world(), *e, "First body").is_some())
        .collect();
    assert_eq!(children.len(), 1, "no leaked or duplicate logical body");
    assert_eq!(
        liquid_surface_handle(app.world(), children[0]).unwrap(),
        handle
    );
    set_liquid_source(app.world_mut(), view, "Removed\n");
    for _ in 0..4 {
        pump(&mut app);
    }
    assert!(app
        .world()
        .entities_with::<LiquidView>()
        .into_iter()
        .all(|e| liquid_surface_handle(app.world(), e).ok() != Some(handle)));
}
