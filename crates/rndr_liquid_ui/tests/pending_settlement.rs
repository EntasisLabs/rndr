//! Public feed-settlement regressions from independent review of 5b2feb8a.
use rndr_core::*;
use rndr_liquid_ui::*;
use rndr_ui::{apply_theme_bridge, UiTheme};
use std::sync::{Arc, Mutex};

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
const FEED5: &str = "```feed\nid: news\ndatatype: text\n```\n";
fn ready5(s: &str) -> FeedOutcome {
    FeedOutcome::Ready(FeedBody {
        datatype: FeedDatatype::Text,
        body: s.into(),
    })
}
#[test]
fn retired_pending_started_during_reentrant_loader_stays_charged_until_ack() {
    struct Host {
        owner: Arc<Mutex<Option<Entity>>>,
        calls: usize,
        contexts: Arc<Mutex<Vec<LiquidFeedContext>>>,
    }
    impl LiquidHost for Host {
        fn load_feed_for(
            &mut self,
            w: &mut World,
            c: &LiquidFeedContext,
            _: &FeedRequest,
        ) -> FeedOutcome {
            self.calls += 1;
            if self.calls % 2 == 1 {
                return ready5("initial");
            }
            self.contexts.lock().unwrap().push(c.clone());
            set_liquid_source(w, self.owner.lock().unwrap().unwrap(), "Removed\n");
            FeedOutcome::Pending
        }
    }
    let owner = Arc::new(Mutex::new(None));
    let contexts = Arc::new(Mutex::new(Vec::new()));
    let mut app = boot(
        Host {
            owner: owner.clone(),
            calls: 0,
            contexts: contexts.clone(),
        },
        LiquidLimits {
            scopes: 2,
            ..LiquidLimits::default()
        },
    );
    let a = liquid_view(app.world_mut(), FEED5);
    *owner.lock().unwrap() = Some(a);
    for _ in 0..6 {
        for _ in 0..3 {
            pump(&mut app);
        }
        if find(app.world(), a, "Refresh").is_some() {
            activate(app.world_mut(), a, "Refresh");
        }
        for _ in 0..3 {
            pump(&mut app);
        }
        set_liquid_source(app.world_mut(), a, FEED5);
    }
    let mut pending = contexts.lock().unwrap();
    let mut unique = Vec::new();
    for c in pending.iter() {
        if !unique.contains(c) {
            unique.push(c.clone());
        }
    }
    *pending = unique;
    assert!(pending.iter().all(|c| c.is_retired()));
    eprintln!("REENTRANT_UNACKNOWLEDGED={} scopes=2", pending.len());
    assert!(
        pending.len() <= 2,
        "unacknowledged Pending contexts exceeded scopes=2: {}",
        pending.len()
    );
    assert!(!pending.is_empty());
    let old = pending.clone();
    drop(pending);
    for c in &old {
        c.acknowledge_retirement();
    }
    for _ in 0..3 {
        pump(&mut app);
    }
    set_liquid_source(app.world_mut(), a, "Removed\n");
    for _ in 0..3 {
        pump(&mut app);
    }
    set_liquid_source(app.world_mut(), a, FEED5);
    for _ in 0..3 {
        pump(&mut app);
    }
    activate(app.world_mut(), a, "Refresh");
    for _ in 0..3 {
        pump(&mut app);
    }
    assert!(
        contexts.lock().unwrap().iter().any(|c| !old.contains(c)),
        "acknowledgment must restore admission"
    );
}

#[test]
fn normal_pending_retirement_backpressure_control() {
    struct Host {
        calls: usize,
        last: Option<LiquidFeedContext>,
        contexts: Arc<Mutex<Vec<LiquidFeedContext>>>,
    }
    impl LiquidHost for Host {
        fn load_feed_for(
            &mut self,
            _: &mut World,
            c: &LiquidFeedContext,
            _: &FeedRequest,
        ) -> FeedOutcome {
            if self.last.as_ref() != Some(c) {
                self.calls += 1;
                self.last = Some(c.clone());
            }
            if self.calls % 2 == 1 {
                return ready5("initial");
            }
            self.contexts.lock().unwrap().push(c.clone());
            FeedOutcome::Pending
        }
    }
    let contexts = Arc::new(Mutex::new(Vec::new()));
    let mut app = boot(
        Host {
            calls: 0,
            last: None,
            contexts: contexts.clone(),
        },
        LiquidLimits {
            scopes: 2,
            ..LiquidLimits::default()
        },
    );
    let a = liquid_view(app.world_mut(), FEED5);
    for _ in 0..6 {
        for _ in 0..3 {
            pump(&mut app);
        }
        if find(app.world(), a, "Refresh").is_some() {
            activate(app.world_mut(), a, "Refresh");
        }
        set_liquid_source(app.world_mut(), a, "Removed\n");
        for _ in 0..3 {
            pump(&mut app);
        }
        set_liquid_source(app.world_mut(), a, FEED5);
    }
    let mut cs = contexts.lock().unwrap();
    let mut unique = Vec::new();
    for c in cs.iter() {
        if !unique.contains(c) {
            unique.push(c.clone());
        }
    }
    *cs = unique;
    eprintln!("NORMAL_UNACKNOWLEDGED={} scopes=2", cs.len());
    assert!(!cs.is_empty());
    assert!(cs.len() <= 2);
    assert!(cs.iter().all(|c| c.is_retired()));
    assert_eq!(cs.len(), 2);
    let old = cs.clone();
    drop(cs);
    for c in &old {
        c.acknowledge_retirement();
    }
    for _ in 0..3 {
        pump(&mut app);
    }
    set_liquid_source(app.world_mut(), a, "Removed\n");
    for _ in 0..3 {
        pump(&mut app);
    }
    set_liquid_source(app.world_mut(), a, FEED5);
    for _ in 0..3 {
        pump(&mut app);
    }
    activate(app.world_mut(), a, "Refresh");
    for _ in 0..3 {
        pump(&mut app);
    }
    assert!(
        contexts.lock().unwrap().iter().any(|c| !old.contains(c)),
        "normal acknowledgment must restore admission"
    );
}

#[test]
fn reentrant_settlement_controls_do_not_leave_phantom_charges() {
    // Ready, Defer, Failed, early retirement acknowledgment, and a borrowed-host
    // refresh all settle without retaining unacknowledged async work.
    for mode in 0..5 {
        struct Host {
            mode: usize,
            calls: usize,
            owner: Arc<Mutex<Option<Entity>>>,
            contexts: Arc<Mutex<Vec<LiquidFeedContext>>>,
        }
        impl LiquidHost for Host {
            fn load_feed_for(
                &mut self,
                w: &mut World,
                c: &LiquidFeedContext,
                _: &FeedRequest,
            ) -> FeedOutcome {
                self.calls += 1;
                if self.calls % 2 == 1 {
                    return ready5("initial");
                }
                self.contexts.lock().unwrap().push(c.clone());
                let owner = self.owner.lock().unwrap().unwrap();
                if self.mode == 4 {
                    activate(w, owner, "Refresh");
                }
                set_liquid_source(w, owner, "Removed\n");
                if self.mode == 3 {
                    c.acknowledge_retirement();
                }
                match self.mode {
                    0 | 4 => ready5("obsolete synchronous body"),
                    1 => FeedOutcome::Defer,
                    2 => FeedOutcome::Failed {
                        message: "synchronous failure".into(),
                    },
                    _ => FeedOutcome::Pending,
                }
            }
        }
        let owner = Arc::new(Mutex::new(None));
        let contexts = Arc::new(Mutex::new(Vec::new()));
        let mut app = boot(
            Host {
                mode,
                calls: 0,
                owner: owner.clone(),
                contexts: contexts.clone(),
            },
            LiquidLimits {
                scopes: 3,
                ..LiquidLimits::default()
            },
        );
        let a = liquid_view(app.world_mut(), FEED5);
        *owner.lock().unwrap() = Some(a);
        for _ in 0..6 {
            for _ in 0..3 {
                pump(&mut app);
            }
            activate(app.world_mut(), a, "Refresh");
            for _ in 0..3 {
                pump(&mut app);
            }
            assert!(
                liquid_component_states(app.world(), a).is_empty(),
                "stale state mode={mode}"
            );
            assert!(find(app.world(), a, "obsolete synchronous body").is_none());
            set_liquid_source(app.world_mut(), a, FEED5);
        }
        assert_eq!(
            contexts.lock().unwrap().len(),
            6,
            "phantom charge mode={mode}"
        );
    }
}

#[test]
fn loader_unwind_retains_escaped_work_and_releases_emission_reservation() {
    struct Host {
        armed: Arc<std::sync::atomic::AtomicBool>,
        owner: Arc<Mutex<Option<Entity>>>,
        contexts: Arc<Mutex<Vec<LiquidFeedContext>>>,
    }
    impl LiquidHost for Host {
        fn load_feed_for(
            &mut self,
            w: &mut World,
            c: &LiquidFeedContext,
            _: &FeedRequest,
        ) -> FeedOutcome {
            if !self.armed.swap(false, std::sync::atomic::Ordering::SeqCst) {
                return ready5("initial");
            }
            self.contexts.lock().unwrap().push(c.clone());
            set_liquid_source(w, self.owner.lock().unwrap().unwrap(), "Removed\n");
            panic!("escaped worker then loader unwind");
        }
    }
    let armed = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let owner = Arc::new(Mutex::new(None));
    let contexts = Arc::new(Mutex::new(Vec::new()));
    let mut app = boot(
        Host {
            armed: armed.clone(),
            owner: owner.clone(),
            contexts: contexts.clone(),
        },
        LiquidLimits {
            scopes: 2,
            envelopes: 1,
            ..LiquidLimits::default()
        },
    );
    let a = liquid_view(app.world_mut(), FEED5);
    *owner.lock().unwrap() = Some(a);
    for _ in 0..6 {
        for _ in 0..3 {
            pump(&mut app);
        }
        if find(app.world(), a, "Refresh").is_some() {
            armed.store(true, std::sync::atomic::Ordering::SeqCst);
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                activate(app.world_mut(), a, "Refresh")
            }));
        }
        armed.store(false, std::sync::atomic::Ordering::SeqCst);
        for _ in 0..3 {
            pump(&mut app);
        }
        set_liquid_source(app.world_mut(), a, FEED5);
    }
    let old = contexts.lock().unwrap().clone();
    assert!(!old.is_empty());
    assert!(old.len() <= 2, "unwind must remain bounded");
    for c in &old {
        assert!(c.is_retired());
        c.acknowledge_retirement();
    }
    for _ in 0..3 {
        pump(&mut app);
    }
    set_liquid_source(app.world_mut(), a, "Removed\n");
    for _ in 0..3 {
        pump(&mut app);
    }
    set_liquid_source(app.world_mut(), a, FEED5);
    for _ in 0..3 {
        pump(&mut app);
    }
    assert!(
        !liquid_component_states(app.world(), a).is_empty(),
        "host restored and reserved envelope released after unwind"
    );
    armed.store(true, std::sync::atomic::Ordering::SeqCst);
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        activate(app.world_mut(), a, "Refresh")
    }));
    assert!(contexts.lock().unwrap().iter().any(|c| !old.contains(c)));
}

#[test]
fn newer_context_result_settles_even_when_enclosing_loader_returns_pending() {
    struct Host {
        calls: usize,
        owner: Arc<Mutex<Option<Entity>>>,
        contexts: Arc<Mutex<Vec<LiquidFeedContext>>>,
    }
    impl LiquidHost for Host {
        fn load_feed_for(
            &mut self,
            w: &mut World,
            c: &LiquidFeedContext,
            _: &FeedRequest,
        ) -> FeedOutcome {
            self.calls += 1;
            if self.calls % 2 == 1 {
                return FeedOutcome::Pending;
            }
            self.contexts.lock().unwrap().push(c.clone());
            let owner = self.owner.lock().unwrap().unwrap();
            let r = liquid_component_states(w, owner).remove(0);
            put_liquid_feed_result(w, c, r.revision, ready5("newer synchronous result")).unwrap();
            set_liquid_source(w, owner, "Removed\n");
            FeedOutcome::Pending
        }
    }
    let owner = Arc::new(Mutex::new(None));
    let contexts = Arc::new(Mutex::new(Vec::new()));
    let mut app = boot(
        Host {
            calls: 0,
            owner: owner.clone(),
            contexts: contexts.clone(),
        },
        LiquidLimits {
            scopes: 2,
            ..LiquidLimits::default()
        },
    );
    let a = liquid_view(app.world_mut(), FEED5);
    *owner.lock().unwrap() = Some(a);
    for _ in 0..6 {
        for _ in 0..5 {
            pump(&mut app);
        }
        assert!(liquid_component_states(app.world(), a).is_empty());
        set_liquid_source(app.world_mut(), a, FEED5);
    }
    assert_eq!(
        contexts.lock().unwrap().len(),
        6,
        "stale loader Pending must not undo newer result settlement"
    );
}

#[test]
fn same_revision_context_result_refuses_enclosing_loader_publication() {
    struct Host {
        calls: usize,
        owner: Arc<Mutex<Option<Entity>>>,
    }
    impl LiquidHost for Host {
        fn load_feed_for(
            &mut self,
            w: &mut World,
            c: &LiquidFeedContext,
            _: &FeedRequest,
        ) -> FeedOutcome {
            self.calls += 1;
            if self.calls == 2 {
                let owner = self.owner.lock().unwrap().unwrap();
                let r = liquid_component_states(w, owner).remove(0);
                put_liquid_feed_result(w, c, r.revision, FeedOutcome::Pending).unwrap();
                assert_eq!(
                    liquid_component_states(w, owner)[0].revision,
                    r.revision,
                    "identical state need not advance the component revision"
                );
                return ready5("obsolete enclosing result");
            }
            FeedOutcome::Pending
        }
    }
    let owner = Arc::new(Mutex::new(None));
    let mut app = boot(
        Host {
            calls: 0,
            owner: owner.clone(),
        },
        LiquidLimits::default(),
    );
    let a = liquid_view(app.world_mut(), FEED5);
    *owner.lock().unwrap() = Some(a);
    for _ in 0..5 {
        pump(&mut app);
    }
    let records = liquid_component_states(app.world(), a);
    assert_eq!(records.len(), 1);
    let LiquidComponentState::Feeds { slots } = &records[0].state else {
        panic!("wrong target state");
    };
    assert!(
        slots[0].body.is_none(),
        "newer exact result must win even at the same revision"
    );
    assert!(find(app.world(), a, "obsolete enclosing result").is_none());
    assert_eq!(records[0].revision, 1);
}
