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

#[test]
fn mixed_physical_semantic_child_relocation_does_not_duplicate() {
    let mut app = boot(NoHost, LiquidLimits::default());
    let a = open(&mut app, ACC);
    activate(app.world_mut(), a, "+ One");
    pump(&mut app);
    let child = app
        .world()
        .entities_with::<LiquidView>()
        .into_iter()
        .find(|e| *e != a && find(app.world(), *e, "First body").is_some())
        .unwrap();
    let handle = liquid_surface_handle(app.world(), child).unwrap();
    let b = open(&mut app, "Independent B\n");
    let attachment = find(app.world(), a, "− One").unwrap();
    set_parent(app.world_mut(), b, attachment);
    set_parent(app.world_mut(), child, b);
    activate(app.world_mut(), a, "− One");
    pump(&mut app);
    activate(app.world_mut(), a, "+ One");
    for _ in 0..4 {
        pump(&mut app);
    }
    let children: Vec<_> = app
        .world()
        .entities_with::<LiquidView>()
        .into_iter()
        .filter(|e| *e != a && *e != b && find(app.world(), *e, "First body").is_some())
        .collect();
    assert_eq!(
        children.len(),
        1,
        "owned child must not duplicate behind independent boundary: {children:?}"
    );
    assert_eq!(
        liquid_surface_handle(app.world(), children[0]).unwrap(),
        handle
    );
}

#[test]
fn public_feed_reservation_releases_after_loader_panic_and_stale_owner() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    struct FaultHost {
        mode: usize,
        calls: Arc<AtomicUsize>,
        victim: Arc<Mutex<Option<Entity>>>,
    }
    impl LiquidHost for FaultHost {
        fn load_feed(&mut self, w: &mut World, _: &FeedRequest) -> FeedOutcome {
            let n = self.calls.fetch_add(1, Ordering::SeqCst);
            if n == 0 {
                if self.mode == 0 {
                    panic!("deliberate public loader unwind");
                }
                w.remove::<LiquidView>(self.victim.lock().unwrap().unwrap());
            }
            FeedOutcome::Ready(FeedBody {
                datatype: FeedDatatype::Text,
                body: "capacity released".into(),
            })
        }
    }
    for mode in 0..2 {
        let calls = Arc::new(AtomicUsize::new(0));
        let victim = Arc::new(Mutex::new(None));
        let mut app = boot(
            FaultHost {
                mode,
                calls: calls.clone(),
                victim: victim.clone(),
            },
            LiquidLimits {
                envelopes: 1,
                callbacks: 1,
                ..LiquidLimits::default()
            },
        );
        let a = liquid_view(
            app.world_mut(),
            "```feed\nid: victim\ndatatype: text\n```\n",
        );
        *victim.lock().unwrap() = Some(a);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            pump(&mut app);
        }));
        assert_eq!(result.is_err(), mode == 0);
        for _ in 0..4 {
            pump(&mut app);
        }
        let b = open(&mut app, "```feed\nid: next\ndatatype: text\n```\n");
        for _ in 0..4 {
            pump(&mut app);
        }
        assert!(
            calls.load(Ordering::SeqCst) >= 2,
            "reservation leaked mode={mode}"
        );
        assert!(liquid_component_states(app.world(), b).iter().any(|r| matches!(&r.state, LiquidComponentState::Feeds { slots } if slots.iter().any(|s| s.body.as_deref()==Some("capacity released")))), "later feed could not publish mode={mode}");
    }
}

#[test]
fn mixed_ownership_terminal_controls_keep_independent_and_retire_owned_child() {
    for mode in 0..3 {
        let mut app = boot(NoHost, LiquidLimits::default());
        let a = open(&mut app, ACC);
        activate(app.world_mut(), a, "+ One");
        pump(&mut app);
        let c = app
            .world()
            .entities_with::<LiquidView>()
            .into_iter()
            .find(|e| *e != a && find(app.world(), *e, "First body").is_some())
            .unwrap();
        let ch = liquid_surface_handle(app.world(), c).unwrap();
        let b = open(&mut app, "Independent B\n");
        let bh = liquid_surface_handle(app.world(), b).unwrap();
        let anchor = find(app.world(), a, "− One").unwrap();
        set_parent(app.world_mut(), b, anchor);
        set_parent(app.world_mut(), c, b);
        match mode {
            0 => {
                app.world_mut().despawn_entity(a);
            }
            1 => {
                app.world_mut().remove::<LiquidView>(a);
            }
            _ => {
                set_liquid_source(app.world_mut(), a, "Removed\n");
            }
        }
        for _ in 0..4 {
            pump(&mut app);
        }
        assert_eq!(
            liquid_surface_handle(app.world(), b).unwrap(),
            bh,
            "independent B mode={mode}"
        );
        assert!(
            app.world()
                .entities_with::<LiquidView>()
                .into_iter()
                .all(|e| liquid_surface_handle(app.world(), e).ok() != Some(ch)),
            "owned C retained mode={mode}"
        );
        assert!(find(app.world(), b, "Independent B").is_some());
    }
}

#[test]
fn loader_reentrant_prepend_cannot_publish_feed_state_into_new_callout() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    const FEED: &str = "```feed\nid: news\ndatatype: text\n```\n";
    struct MoveHost {
        view: Arc<Mutex<Option<Entity>>>,
        calls: Arc<AtomicUsize>,
    }
    impl LiquidHost for MoveHost {
        fn load_feed_for(
            &mut self,
            w: &mut World,
            _: &LiquidFeedContext,
            _: &FeedRequest,
        ) -> FeedOutcome {
            let call = self.calls.fetch_add(1, Ordering::SeqCst);
            if call == 0 {
                return FeedOutcome::Pending;
            }
            if call == 1 {
                let a = self.view.lock().unwrap().unwrap();
                set_liquid_source(
                    w,
                    a,
                    &format!("```callout\ntitle: Prefix\nbody: Added\n```\n\n{FEED}"),
                );
            }
            FeedOutcome::Ready(FeedBody {
                datatype: FeedDatatype::Text,
                body: "right feed outcome".into(),
            })
        }
    }
    let owner = Arc::new(Mutex::new(None));
    let calls = Arc::new(AtomicUsize::new(0));
    let mut app = boot(
        MoveHost {
            view: owner.clone(),
            calls: calls.clone(),
        },
        LiquidLimits::default(),
    );
    let log = Arc::new(Mutex::new(Vec::new()));
    subscribe_liquid_interactions(app.world_mut(), Log(log.clone()));
    let a = liquid_view(app.world_mut(), FEED);
    *owner.lock().unwrap() = Some(a);
    for _ in 0..5 {
        pump(&mut app);
    }
    assert!(calls.load(Ordering::SeqCst) >= 2);
    let records = liquid_component_states(app.world(), a);
    assert_eq!(records.len(), 1, "retained feed eventually publishes");
    let r = &records[0];
    assert!(
        matches!(&r.state, LiquidComponentState::Feeds { slots } if slots.iter().any(|s| s.body.as_deref() == Some("right feed outcome")))
    );
    assert_eq!(
        r.revision, 2,
        "obsolete attempt must not consume a revision"
    );
    let log = log.lock().unwrap();
    assert_eq!(log.len(), 2, "only Pending and current Ready publish");
    assert_eq!(log[1].source, r.source);
    assert_eq!(log[1].revision, r.revision);
    for r in records {
        assert!(
            matches!(
                app.world()
                    .get::<LiquidView>(a)
                    .unwrap()
                    .blocks()
                    .get(r.block),
                Some(rndr_liquid::LiquidBlock::Feed(_))
            ),
            "feed state published to non-feed block after host prepend: {r:?}"
        );
    }
}

#[test]
fn semantic_child_reuse_ignores_nested_foreign_and_outside_physical_placement() {
    for outside in [false, true] {
        let mut app = boot(NoHost, LiquidLimits::default());
        let a = open(&mut app, ACC);
        activate(app.world_mut(), a, "+ One");
        pump(&mut app);
        let c = app
            .world()
            .entities_with::<LiquidView>()
            .into_iter()
            .find(|e| *e != a)
            .unwrap();
        let ch = liquid_surface_handle(app.world(), c).unwrap();
        let b = open(
            &mut app,
            &ACC.replace("First body", "Independent nested body"),
        );
        activate(app.world_mut(), b, "+ One");
        pump(&mut app);
        let d = app
            .world()
            .entities_with::<LiquidView>()
            .into_iter()
            .find(|e| *e != a && *e != b && *e != c)
            .unwrap();
        let bh = liquid_surface_handle(app.world(), b).unwrap();
        let dh = liquid_surface_handle(app.world(), d).unwrap();
        let records = liquid_component_states(app.world(), b);
        if !outside {
            let anchor = find(app.world(), a, "− One").unwrap();
            set_parent(app.world_mut(), b, anchor);
        }
        set_parent(app.world_mut(), c, d);
        for _ in 0..3 {
            activate(app.world_mut(), a, "− One");
            pump(&mut app);
            assert_eq!(liquid_surface_handle(app.world(), b).unwrap(), bh);
            assert_eq!(liquid_surface_handle(app.world(), d).unwrap(), dh);
            assert!(find(app.world(), d, "Independent nested body").is_some());
            assert_eq!(liquid_component_states(app.world(), b), records);
            activate(app.world_mut(), a, "+ One");
            for _ in 0..4 {
                pump(&mut app);
            }
            let children: Vec<_> = app
                .world()
                .entities_with::<LiquidView>()
                .into_iter()
                .filter(|e| liquid_surface_handle(app.world(), *e).ok() == Some(ch))
                .collect();
            assert_eq!(children.len(), 1, "reuse outside={outside}");
            assert!(find(app.world(), children[0], "First body").is_some());
            assert_eq!(app.world().entities_with::<LiquidView>().len(), 4);
            set_parent(app.world_mut(), children[0], d);
        }
    }
}

#[test]
fn reentrant_feed_removal_replacement_and_newer_result_refuse_obsolete_outcome() {
    const FEED: &str = "```feed\nid: news\ndatatype: text\n```\n";
    struct ReentrantHost {
        mode: usize,
        calls: usize,
        owner: Arc<Mutex<Option<Entity>>>,
        old: Arc<Mutex<Option<LiquidFeedContext>>>,
    }
    impl LiquidHost for ReentrantHost {
        fn load_feed_for(
            &mut self,
            w: &mut World,
            c: &LiquidFeedContext,
            _: &FeedRequest,
        ) -> FeedOutcome {
            self.calls += 1;
            if self.calls == 1 {
                *self.old.lock().unwrap() = Some(c.clone());
                return FeedOutcome::Pending;
            }
            if self.calls == 2 {
                let a = self.owner.lock().unwrap().unwrap();
                match self.mode {
                    0 => set_liquid_source(w, a, "Removed\n"),
                    1 => set_liquid_source(w, a, &FEED.replace("news", "replacement")),
                    2 => {
                        let r = liquid_component_states(w, a).remove(0);
                        put_liquid_feed_result(
                            w,
                            c,
                            r.revision,
                            FeedOutcome::Ready(FeedBody {
                                datatype: FeedDatatype::Text,
                                body: "newer result".into(),
                            }),
                        )
                        .unwrap();
                    }
                    _ => activate(w, a, "Refresh"),
                }
                return FeedOutcome::Ready(FeedBody {
                    datatype: FeedDatatype::Text,
                    body: "obsolete outcome".into(),
                });
            }
            FeedOutcome::Ready(FeedBody {
                datatype: FeedDatatype::Text,
                body: "current outcome".into(),
            })
        }
        fn retire_feed_observation(&mut self, _: &mut World, c: &LiquidFeedContext) {
            c.acknowledge_retirement();
        }
    }
    for mode in 0..4 {
        let owner = Arc::new(Mutex::new(None));
        let old = Arc::new(Mutex::new(None));
        let log = Arc::new(Mutex::new(Vec::new()));
        let mut app = boot(
            ReentrantHost {
                mode,
                calls: 0,
                owner: owner.clone(),
                old: old.clone(),
            },
            LiquidLimits::default(),
        );
        subscribe_liquid_interactions(app.world_mut(), Log(log.clone()));
        let a = liquid_view(app.world_mut(), FEED);
        *owner.lock().unwrap() = Some(a);
        for _ in 0..8 {
            pump(&mut app);
        }
        let records = liquid_component_states(app.world(), a);
        if mode == 0 {
            assert!(records.is_empty());
        } else {
            assert_eq!(records.len(), 1);
            let LiquidComponentState::Feeds { slots } = &records[0].state else {
                panic!("wrong type")
            };
            assert_eq!(
                slots[0].body.as_deref(),
                Some(if mode == 2 {
                    "newer result"
                } else {
                    "current outcome"
                })
            );
            assert_eq!(records[0].revision, if mode == 1 { 1 } else { 2 });
            assert!(log.lock().unwrap().iter().any(|e| e.view == a
                && e.source == records[0].source
                && e.revision == records[0].revision));
        }
        let context = old.lock().unwrap().clone().unwrap();
        let before = records.clone();
        let events = log.lock().unwrap().len();
        assert_eq!(
            put_liquid_feed_result(app.world_mut(), &context, 1, FeedOutcome::Pending),
            Err(LiquidUpdateError::StaleOwner)
        );
        assert_eq!(liquid_component_states(app.world(), a), before);
        assert_eq!(log.lock().unwrap().len(), events);
        assert_eq!(context.is_retired(), mode != 2);
    }
}
