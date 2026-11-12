//! Public consumer compile test. Does not create an event loop or native window.
use rndr_core::{App, CloseGuard, CloseState};
use rndr_render::{CloseValidator, CloseWorld, Session};

#[test]
fn external_consumer_can_configure_opt_in_hook_without_native_objects() {
    let validator: Box<CloseValidator> = Box::new(|worlds: &mut [CloseWorld<'_>]| {
        for window in worlds {
            let _stable_id = window.id();
            let _primary = window.primary();
            if !window.closing() {
                continue;
            }
            // This compile example is observational, not a durability adapter.
            // Never invent success for a Pending or Failed guard.
            if window
                .world()
                .resource::<CloseGuard>()
                .is_some_and(|g| !matches!(g.state(), CloseState::Ready(_)))
            {
                return Err("protection remains pending".into());
            }
            let _borrowed_world = window.world_mut();
        }
        Ok(())
    });
    let _session = Session::new(App::new())
        .window(App::new())
        .close_validator(validator);
}
