//! Identity is independent of projection lifetimes and resource positions.

#[test]
fn colliding_generated_child_ids_never_alias_independent_request_bindings() {
    let src="```dashboard\ntitle: Collisions\n---\nlabel: A\nvalue: 0\nfeed: alpha\n---\nlabel: A\nvalue: 0\nfeed: beta\n---\nlabel: A-1\nvalue: 0\nfeed: gamma\n```\n";
    let model = Arc::new(Mutex::new(Model::default()));
    let mut a = app(Host(model.clone()));
    let v = liquid_view(a.world_mut(), src);
    let s = liquid_surface_handle(a.world(), v).unwrap();
    set(&mut a, s, src, &[Some("dashboard")]);
    let rndr_liquid::LiquidBlock::Dashboard(props) =
        &a.world().get::<LiquidView>(v).unwrap().blocks()[0]
    else {
        panic!("dashboard fixture");
    };
    assert_eq!(props.tiles.len(), 3);
    assert_eq!(
        props
            .tiles
            .iter()
            .map(|t| &t.id)
            .collect::<std::collections::HashSet<_>>()
            .len(),
        2,
        "fixture must exercise colliding generated IDs, not a parser change"
    );
    pump(&mut a);
    let contexts = model.lock().unwrap().contexts.clone();
    assert_eq!(
        contexts.len(),
        3,
        "each full request binding owns a distinct live context"
    );
    let h = liquid_component_handle(a.world(), s, 0).unwrap();
    for (ctx, name) in contexts.iter().zip(["alpha", "beta", "gamma"]) {
        let revision = liquid_component_state_for(a.world(), h)
            .unwrap()
            .unwrap()
            .revision;
        put_liquid_feed_result_for(
            a.world_mut(),
            ctx,
            revision,
            FeedOutcome::Ready(FeedBody {
                datatype: FeedDatatype::Text,
                body: format!("result-{name}"),
            }),
        )
        .unwrap();
    }
    let record = liquid_component_state_for(a.world(), h).unwrap().unwrap();
    let LiquidComponentState::Feeds { slots } = record.state else {
        panic!("feed slots");
    };
    assert_eq!(slots.len(), 3);
    assert_eq!(
        slots
            .iter()
            .map(|s| &s.instance)
            .collect::<std::collections::HashSet<_>>()
            .len(),
        3
    );
    for slot in &slots {
        assert_eq!(
            slot.body.as_deref(),
            Some(format!("result-{}", slot.feed_id).as_str())
        );
    }
    set(
        &mut a,
        s,
        &src.replace("feed: beta", "feed: changed"),
        &[Some("dashboard")],
    );
    assert!(!contexts[0].is_retired());
    assert!(contexts[1].is_retired());
    assert!(!contexts[2].is_retired());
    let before = liquid_component_state_for(a.world(), h).unwrap();
    assert_eq!(
        put_liquid_feed_result_for(
            a.world_mut(),
            &contexts[1],
            before.as_ref().unwrap().revision,
            FeedOutcome::Pending
        ),
        Err(LiquidF2Error::StaleRequest)
    );
    assert_eq!(liquid_component_state_for(a.world(), h).unwrap(), before);
}

#[test]
fn parked_nested_handles_query_write_and_reset_without_a_physical_view() {
    let filler: String = (0..90).map(|i| format!("Paragraph {i}.\n\n")).collect();
    let source = format!("{NESTED}{filler}");
    let mut a = app(NoHost);
    let v = liquid_view(a.world_mut(), &source);
    a.world_mut()
        .get_mut::<MarkdownView>(v)
        .unwrap()
        .set_follow(false);
    pump(&mut a);
    let (nested, h) = inner(&a, v);
    let surface = liquid_surface_handle(a.world(), nested).unwrap();
    put_liquid_component_state_for(
        a.world_mut(),
        h,
        0,
        LiquidComponentState::Accordion { open: vec![0] },
    )
    .unwrap();
    let bottom = (a.world().get::<Rect>(v).unwrap().h
        - a.world().get::<Scroll>(v).unwrap().content[1])
        .min(0.0);
    a.world_mut().get_mut::<Scroll>(v).unwrap().offset[1] = bottom;
    pump(&mut a);
    assert!(
        !a.world().is_alive(nested),
        "test must remove the nested physical view"
    );
    assert_eq!(
        liquid_component_state_for(a.world(), h)
            .unwrap()
            .unwrap()
            .revision,
        1
    );
    put_liquid_component_state_for(
        a.world_mut(),
        h,
        1,
        LiquidComponentState::Accordion { open: vec![] },
    )
    .unwrap();
    let fresh = reset_liquid_block(a.world_mut(), h, 2).unwrap();
    assert_eq!(liquid_component_state_for(a.world(), fresh).unwrap(), None);
    let revision = liquid_source_revision(a.world(), surface).unwrap();
    let new_surface = reset_liquid_surface(a.world_mut(), surface, revision).unwrap();
    assert_eq!(
        liquid_source_revision(a.world(), surface),
        Err(LiquidF2Error::StaleOwner)
    );
    assert!(liquid_component_state_for(a.world(), fresh).is_err());
    a.world_mut().get_mut::<Scroll>(v).unwrap().offset[1] = 0.0;
    pump(&mut a);
    let (nested, new_handle) = inner(&a, v);
    assert_eq!(
        liquid_surface_handle(a.world(), nested).unwrap(),
        new_surface
    );
    assert_eq!(
        liquid_component_state_for(a.world(), new_handle).unwrap(),
        None
    );
}
#[test]
fn parked_nested_feed_accepts_current_context_and_preserves_captured_envelope_handles() {
    let source = format!(
        "```report\ntitle: Feed parent\n\n{FEED}```\n\n{}",
        (0..90)
            .map(|i| format!("Paragraph {i}.\n\n"))
            .collect::<String>()
    );
    let model = Arc::new(Mutex::new(Model::default()));
    let mut a = app(Host(model.clone()));
    let v = liquid_view(a.world_mut(), &source);
    a.world_mut()
        .get_mut::<MarkdownView>(v)
        .unwrap()
        .set_follow(false);
    pump(&mut a);
    let context = model.lock().unwrap().contexts.last().unwrap().clone();
    let handle = context.block_handle();
    let bottom = (a.world().get::<Rect>(v).unwrap().h
        - a.world().get::<Scroll>(v).unwrap().content[1])
        .min(0.0);
    a.world_mut().get_mut::<Scroll>(v).unwrap().offset[1] = bottom;
    pump(&mut a);
    assert!(a.world().get::<LiquidView>(v).unwrap().mounted().is_empty());
    struct Events(Arc<Mutex<Vec<LiquidInteractionEnvelopeFor>>>);
    impl LiquidInteractionHandlerFor for Events {
        fn on_interaction(&mut self, _: &mut World, e: &LiquidInteractionEnvelopeFor) {
            self.0.lock().unwrap().push(e.clone());
        }
    }
    let events = Arc::new(Mutex::new(Vec::new()));
    let _subscription = subscribe_liquid_interactions_owned_for(
        a.world_mut(),
        LiquidListenerOwner::World,
        Events(events.clone()),
    )
    .unwrap();
    let r = liquid_component_state_for(a.world(), handle)
        .unwrap()
        .unwrap()
        .revision;
    put_liquid_feed_result_for(
        a.world_mut(),
        &context,
        r,
        FeedOutcome::Ready(FeedBody {
            datatype: FeedDatatype::Text,
            body: "offscreen result".into(),
        }),
    )
    .unwrap();
    assert_eq!(
        liquid_component_state_for(a.world(), handle)
            .unwrap()
            .unwrap()
            .revision,
        r + 1
    );
    assert_eq!(events.lock().unwrap().len(), 1);
    let event = events.lock().unwrap()[0].clone();
    assert_eq!(event.block, handle);
    assert_eq!(event.surface, context.surface_handle());
    a.world_mut().get_mut::<Scroll>(v).unwrap().offset[1] = 0.0;
    pump(&mut a);
    assert!(!context.is_retired());
    assert_eq!(
        liquid_component_state_for(a.world(), handle)
            .unwrap()
            .unwrap()
            .revision,
        r + 1
    );
}
#[test]
fn one_changed_dashboard_binding_does_not_retire_an_unrelated_tile_lane() {
    let src="```dashboard\ntitle: Metrics\n---\nid: left\nlabel: Left\nvalue: 0\nfeed: alpha\nfield: value\n---\nid: right\nlabel: Right\nvalue: 0\nfeed: beta\nfield: value\n```\n\n";
    let model = Arc::new(Mutex::new(Model::default()));
    let mut a = app(Host(model.clone()));
    let v = liquid_view(a.world_mut(), src);
    let s = liquid_surface_handle(a.world(), v).unwrap();
    set(&mut a, s, src, &[Some("dashboard")]);
    pump(&mut a);
    let contexts = model.lock().unwrap().contexts.clone();
    assert_eq!(contexts.len(), 2);
    let h = contexts[0].block_handle();
    let before = liquid_component_state_for(a.world(), h)
        .unwrap()
        .unwrap()
        .revision;
    set(
        &mut a,
        s,
        &src.replace("feed: alpha", "feed: gamma"),
        &[Some("dashboard")],
    );
    assert!(contexts[0].is_retired());
    assert!(!contexts[1].is_retired());
    let record = liquid_component_state_for(a.world(), h).unwrap().unwrap();
    assert_eq!(record.revision, before + 1);
    let LiquidComponentState::Feeds { slots } = record.state else {
        panic!("feeds")
    };
    assert_eq!(slots.len(), 1);
    assert_eq!(slots[0].instance, "tile-right");
    assert_eq!(slots[0].feed_id, "beta");
    pump(&mut a);
    assert_eq!(model.lock().unwrap().contexts.len(), 3);
    let r = liquid_component_state_for(a.world(), h)
        .unwrap()
        .unwrap()
        .revision;
    put_liquid_feed_result_for(
        a.world_mut(),
        &contexts[1],
        r,
        FeedOutcome::Ready(FeedBody {
            datatype: FeedDatatype::Text,
            body: "unrelated lane still valid".into(),
        }),
    )
    .unwrap();
}

use rndr_core::*;
use rndr_liquid_ui::*;
use rndr_ui::{apply_theme_bridge, UiTheme};
use std::sync::{Arc, Mutex};
const NESTED: &str = "```report\ntitle: Parent\n\n```accordion\ntitle: Inner\n\n---\nlabel: Child\nbody: Child body\n```\n\n```\n\n";
const RECIPE: &str =
    "```recipe\ntitle: Soup\n---\nlabel: Simmer\nbody: Stir\nduration: 5s\n```\n\n";
const FEED: &str = "```feed\nid: news\ndatatype: text\ntitle: News\n```\n\n";
fn pump(app: &mut App) {
    for _ in 0..3 {
        rndr_paint::frame(app);
    }
}
fn app(host: impl LiquidHost + 'static) -> App {
    let mut a = App::new();
    a.add_plugin(MarkdownPlugin);
    a.add_plugin(LiquidUiPlugin::new(host));
    a.world_mut().insert_resource(Viewport::new(960.0, 640.0));
    apply_theme_bridge(a.world_mut(), UiTheme::default());
    a
}
fn set(a: &mut App, s: LiquidSurfaceHandle, source: &str, names: &[Option<&str>]) {
    let rev = liquid_source_revision(a.world(), s).unwrap();
    set_liquid_source_keyed(
        a.world_mut(),
        s,
        rev,
        source,
        &LiquidKeyMap {
            keys: names.iter().map(|k| k.map(str::to_owned)).collect(),
        },
    )
    .unwrap();
}
fn press(a: &mut App, label: &str) {
    let mut e = a
        .world()
        .entities_with::<Label>()
        .into_iter()
        .find(|e| a.world().get::<Label>(*e).is_some_and(|l| l.text == label))
        .unwrap_or_else(|| panic!("no {label}"));
    while try_activate(a.world_mut(), e) != ActivationResult::Accepted {
        e = a.world().get::<Parent>(e).unwrap().entity;
    }
    for intent in take_action_intents(a.world_mut()) {
        assert!(dispatch(a.world_mut(), &intent));
    }
}
fn inner(a: &App, top: Entity) -> (Entity, LiquidBlockHandle) {
    a.world()
        .entities_with::<LiquidView>()
        .into_iter()
        .filter(|v| *v != top)
        .find_map(|v| {
            let s = liquid_surface_handle(a.world(), v).ok()?;
            let h = liquid_component_handle(a.world(), s, 0).ok()?;
            a.world()
                .get::<LiquidView>(v)?
                .blocks()
                .iter()
                .any(|b| b.kind() == "accordion")
                .then_some((v, h))
        })
        .expect("nested accordion")
}
#[test]
fn nested_parse_index_shift_is_not_semantic_edit_but_same_index_new_child_is() {
    for keyed in [false, true] {
        let mut a = app(NoHost);
        let v = liquid_view(a.world_mut(), NESTED);
        let s = liquid_surface_handle(a.world(), v).unwrap();
        if keyed {
            set(&mut a, s, NESTED, &[Some("parent")]);
        }
        pump(&mut a);
        let parent = liquid_component_handle(a.world(), s, 0).unwrap();
        let (_, child) = inner(&a, v);
        put_liquid_component_state_for(
            a.world_mut(),
            child,
            0,
            LiquidComponentState::Accordion { open: vec![0] },
        )
        .unwrap();
        let prefix = "```card\ntitle: unrelated\n```\n\n";
        let moved = format!("{prefix}{NESTED}");
        if keyed {
            set(&mut a, s, &moved, &[None, Some("parent")]);
        } else {
            set_liquid_source(a.world_mut(), v, &moved);
        }
        pump(&mut a);
        assert_eq!(liquid_component_handle(a.world(), s, 1).unwrap(), parent);
        assert_eq!(
            inner(&a, v).1,
            child,
            "parse-index-only shift, keyed={keyed}"
        );
        assert_eq!(
            liquid_component_state_for(a.world(), child)
                .unwrap()
                .unwrap()
                .revision,
            1
        );
        if keyed {
            let cosmetic = moved.replace("title: Parent", "title: Cosmetic");
            set(&mut a, s, &cosmetic, &[None, Some("parent")]);
            pump(&mut a);
            assert_eq!(inner(&a, v).1, child);
        }
        let edited = moved.replace("Child body", "Different child body");
        if keyed {
            set(&mut a, s, &edited, &[None, Some("parent")]);
        } else {
            set_liquid_source(a.world_mut(), v, &edited);
        }
        assert!(liquid_component_state_for(a.world(), child).is_err());
        pump(&mut a);
        assert_ne!(
            inner(&a, v).1,
            child,
            "same token index must not mask child edit"
        );
        assert_eq!(
            liquid_component_handle(a.world(), s, 1).unwrap() == parent,
            keyed
        );
    }
}
#[test]
fn keyed_recipe_cosmetic_reorder_preserves_deadline_schema_edit_clears_it() {
    let mut a = app(NoHost);
    let v = liquid_view(a.world_mut(), RECIPE);
    let s = liquid_surface_handle(a.world(), v).unwrap();
    set(&mut a, s, RECIPE, &[Some("recipe")]);
    pump(&mut a);
    let h = liquid_component_handle(a.world(), s, 0).unwrap();
    press(&mut a, "Start");
    let old = liquid_timers(a.world(), v).remove(0);
    assert_eq!(old.status, LiquidTimerStatus::Running);
    let moved = format!("Prefix\n\n{}", RECIPE.replace("Soup", "Title"));
    set(&mut a, s, &moved, &[Some("recipe")]);
    pump(&mut a);
    assert_eq!(liquid_component_handle(a.world(), s, 0).unwrap(), h);
    let current = liquid_timers(a.world(), v).remove(0);
    assert_eq!(current.revision, old.revision);
    assert_eq!(current.deadline_ms, old.deadline_ms);
    set(&mut a, s, &moved.replace("5s", "8s"), &[Some("recipe")]);
    assert!(liquid_timers(a.world(), v).is_empty());
    pump(&mut a);
    assert_ne!(
        liquid_timer(
            a.world(),
            v,
            liquid_component_state_for(a.world(), h)
                .unwrap()
                .unwrap()
                .source,
            0
        )
        .map(|t| t.deadline_ms),
        Some(old.deadline_ms)
    );
}
#[derive(Default)]
struct Model {
    contexts: Vec<LiquidFeedContext>,
    retired: usize,
}
struct Host(Arc<Mutex<Model>>);
impl LiquidHost for Host {
    fn load_feed_for(
        &mut self,
        _: &mut World,
        c: &LiquidFeedContext,
        _: &FeedRequest,
    ) -> FeedOutcome {
        let mut model = self.0.lock().unwrap();
        if !model.contexts.contains(c) {
            model.contexts.push(c.clone());
        }
        FeedOutcome::Pending
    }
    fn retire_feed_observation(&mut self, _: &mut World, c: &LiquidFeedContext) {
        self.0.lock().unwrap().retired += 1;
        c.acknowledge_retirement();
    }
}
#[test]
fn feed_binding_retains_cosmetic_context_and_retires_old_lane_with_zero_stale_effects() {
    let model = Arc::new(Mutex::new(Model::default()));
    let mut a = app(Host(model.clone()));
    let v = liquid_view(a.world_mut(), FEED);
    let s = liquid_surface_handle(a.world(), v).unwrap();
    set(&mut a, s, FEED, &[Some("feed")]);
    pump(&mut a);
    let h = liquid_component_handle(a.world(), s, 0).unwrap();
    let context = model.lock().unwrap().contexts.last().unwrap().clone();
    let ready = || {
        FeedOutcome::Ready(FeedBody {
            datatype: FeedDatatype::Text,
            body: "last good".into(),
        })
    };
    let r = liquid_component_state_for(a.world(), h).unwrap().unwrap();
    put_liquid_feed_result_for(a.world_mut(), &context, r.revision, ready()).unwrap();
    let moved = format!("Prefix\n\n{}", FEED.replace("title: News", "title: Title"));
    set(&mut a, s, &moved, &[Some("feed")]);
    assert!(!context.is_retired());
    assert_eq!(
        liquid_component_state_for(a.world(), h)
            .unwrap()
            .unwrap()
            .revision,
        r.revision + 1
    );
    let changed = moved.replace("id: news", "id: other");
    set(&mut a, s, &changed, &[Some("feed")]);
    assert!(context.is_retired());
    let before = liquid_component_state_for(a.world(), h).unwrap();
    assert_eq!(
        put_liquid_feed_result_for(
            a.world_mut(),
            &context,
            before.as_ref().unwrap().revision,
            ready()
        ),
        Err(LiquidF2Error::StaleRequest)
    );
    assert_eq!(liquid_component_state_for(a.world(), h).unwrap(), before);
    pump(&mut a);
    assert!(model.lock().unwrap().retired >= 1);
    let fresh = model.lock().unwrap().contexts.last().unwrap().clone();
    assert_ne!(fresh, context);
    let before = liquid_component_state_for(a.world(), h).unwrap();
    assert!(matches!(
        put_liquid_feed_result_for(a.world_mut(), &fresh, 0, ready()),
        Err(LiquidF2Error::Revision { .. })
    ));
    assert_eq!(liquid_component_state_for(a.world(), h).unwrap(), before);
    let actual = before.unwrap().revision;
    put_liquid_feed_result_for(a.world_mut(), &fresh, actual, ready()).unwrap();
    let _fresh = reset_liquid_block(a.world_mut(), h, actual + 1).unwrap();
    let before = liquid_component_states(a.world(), v);
    assert_eq!(
        put_liquid_feed_result_for(a.world_mut(), &fresh, 0, ready()),
        Err(LiquidF2Error::StaleBlock)
    );
    assert_eq!(liquid_component_states(a.world(), v), before);
}
#[test]
fn missing_zero_revision_is_fenced_by_keyed_schema_reset() {
    let mut a = app(NoHost);
    let src = "```accordion\n---\nlabel: One\nbody: Old\n```\n";
    let v = liquid_view(a.world_mut(), src);
    let s = liquid_surface_handle(a.world(), v).unwrap();
    set(&mut a, s, src, &[Some("a")]);
    let h = liquid_component_handle(a.world(), s, 0).unwrap();
    assert_eq!(liquid_component_state_for(a.world(), h).unwrap(), None);
    set(&mut a, s, &src.replace("Old", "New"), &[Some("a")]);
    assert!(matches!(
        put_liquid_component_state_for(
            a.world_mut(),
            h,
            0,
            LiquidComponentState::Accordion { open: vec![0] }
        ),
        Err(LiquidF2Error::Revision { actual: 1, .. })
    ));
}
