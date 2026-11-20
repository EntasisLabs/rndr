use rndr_core::{App, FrameStage, FrameTimings, Layout};

#[test]
fn paint_timing_is_available_at_snapshot_and_separates_live_updates() {
    let mut app = App::new();
    app.world_mut().spawn(Layout::column());
    // Default path does not create a recorder.
    rndr_paint::frame(&mut app);
    assert!(app.world().resource::<FrameTimings>().is_none());
    app.world_mut().insert_resource(FrameTimings::new(4));
    rndr_paint::frame_with_snapshot_frame(&mut app, |world, _| {
        let timings = world.resource::<FrameTimings>().unwrap();
        assert_eq!(timings.summary(FrameStage::Paint).unwrap().count, 1);
        assert_eq!(timings.summary(FrameStage::Update).unwrap().count, 1);
    });
    rndr_paint::frame_live_with_snapshot(&mut app, |world| {
        let timings = world.resource::<FrameTimings>().unwrap();
        assert_eq!(timings.summary(FrameStage::Paint).unwrap().count, 2);
        assert_eq!(timings.summary(FrameStage::Update).unwrap().count, 1);
        assert_eq!(timings.summary(FrameStage::LiveUpdate).unwrap().count, 1);
    });
}
