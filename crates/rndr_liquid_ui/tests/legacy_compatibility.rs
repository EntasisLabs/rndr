//! Compile-time exhaustive legacy construction, plus default-host behavior.
use rndr_core::{App, Plugin, World};
use rndr_liquid_ui::*;

struct Legacy;
impl LiquidHost for Legacy {
    fn load_feed(&mut self, _: &mut World, _: &FeedRequest) -> FeedOutcome {
        FeedOutcome::Pending
    }
}
#[test]
fn legacy_v1_literals_and_context_free_host_still_compile() {
    let request = FeedRequest {
        feed_id: "f".into(),
        datatype: FeedDatatype::Md,
        field: None,
        instance: "f".into(),
    };
    let FeedRequest {
        feed_id,
        datatype,
        field,
        instance,
    } = request;
    assert_eq!(
        (feed_id.as_str(), datatype, field, instance.as_str()),
        ("f", FeedDatatype::Md, None, "f")
    );
    let slot = FeedSlot {
        instance: "f".into(),
        feed_id: "f".into(),
        datatype: "md".into(),
        phase: FeedPhase::Loading,
        body: None,
        error: None,
        inflight: true,
    };
    let state = LiquidComponentState::Feeds { slots: vec![slot] };
    let record = LiquidComponentRecord {
        schema_version: 1,
        source: 0,
        block: 0,
        component_type: "feed".into(),
        revision: 1,
        state,
    };
    let LiquidComponentRecord {
        schema_version,
        source,
        block,
        component_type,
        revision,
        state: _,
    } = record;
    assert_eq!(
        (
            schema_version,
            source,
            block,
            component_type.as_str(),
            revision
        ),
        (1, 0, 0, "feed", 1)
    );
    let timer = LiquidTimerRecord {
        schema_version: 1,
        source: 0,
        step: 0,
        revision: 1,
        status: LiquidTimerStatus::Idle,
        duration_ms: 1000,
        remaining_ms: 1000,
        deadline_ms: None,
    };
    let LiquidTimerRecord {
        schema_version,
        source: _,
        step: _,
        revision: _,
        status: _,
        duration_ms: _,
        remaining_ms: _,
        deadline_ms: _,
    } = timer;
    assert_eq!(schema_version, 1);
    let mut world = World::new();
    let view = liquid_view(&mut world, "ordinary");
    let envelope = LiquidInteractionEnvelope {
        version: 1,
        view,
        source: 0,
        node_id: "feed".into(),
        instance_id: "f".into(),
        event: LiquidInteraction::FeedRefresh,
        disposition: LiquidDisposition::ContextOnly,
        revision: 1,
    };
    let LiquidInteractionEnvelope {
        version,
        view: _,
        source: _,
        node_id: _,
        instance_id: _,
        event: _,
        disposition: _,
        revision: _,
    } = envelope;
    assert_eq!(version, 1);
    assert!(matches!(
        Legacy.load_feed(
            &mut world,
            &FeedRequest {
                feed_id: "f".into(),
                datatype: FeedDatatype::Md,
                field: None,
                instance: "f".into()
            }
        ),
        FeedOutcome::Pending
    ));
}
#[test]
fn limits_are_validated_and_installed_at_plugin_construction() {
    let limits = LiquidLimits {
        callbacks: 0,
        ..LiquidLimits::default()
    };
    assert!(matches!(
        LiquidUiPlugin::new(Legacy).with_limits(limits),
        Err(LiquidUpdateError::Capacity)
    ));
    let mut app = App::new();
    LiquidUiPlugin::new(Legacy)
        .with_limits(LiquidLimits {
            callbacks: 2,
            scopes: 3,
            ..LiquidLimits::default()
        })
        .unwrap_or_else(|_| panic!("positive limits"))
        .build(&mut app);
    assert_eq!(app.world().resource::<LiquidLimits>().unwrap().callbacks, 2);
    assert_eq!(app.world().resource::<LiquidLimits>().unwrap().scopes, 3);
}
