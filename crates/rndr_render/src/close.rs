//! Runner close coordination, kept independent of native/GPU objects for tests.
// Existing flow/reviewer tests include this entire module but use only `step`.
#![cfg_attr(test, allow(dead_code))]
use rndr_core::{CloseGuard, CloseState, Input, World};

/// One live Session window at the final close boundary. Metadata is runner-owned.
/// Worlds include hidden, unrelated, unguarded and dynamically admitted windows.
/// The borrow cannot escape the validator. `closing()` is authoritative native
/// participation; a manually requested CloseGuard does not establish it.
pub struct CloseWorld<'a> {
    id: rndr_core::WindowId,
    primary: bool,
    closing: bool,
    world: &'a mut World,
}

impl CloseWorld<'_> {
    /// Stable Desktop window identity for this Session window.
    pub fn id(&self) -> rndr_core::WindowId {
        self.id
    }
    /// Whether this is the Session's primary window.
    pub fn primary(&self) -> bool {
        self.primary
    }
    /// Whether this window actually participates in the pending native close.
    pub fn closing(&self) -> bool {
        self.closing
    }
    /// Inspect the live World.
    pub fn world(&self) -> &World {
        self.world
    }
    /// Validate/protect this live World using the application's existing owners.
    pub fn world_mut(&mut self) -> &mut World {
        self.world
    }
}

/// Session-owned final validator. Errors and unwinding panics refuse the entire
/// destructive pass. It must validate *all* supplied Worlds and actual closing
/// participation, not just the windows whose guards happen to be requested.
/// No user callback or World/topology mutation runs after success and before
/// guard consumption/removal. This is not a persistence or native GUI guarantee.
pub type CloseValidator = dyn for<'a> FnMut(&mut [CloseWorld<'a>]) -> Result<(), String>;

#[derive(Default)]
pub(crate) struct CloseSlot {
    pub requested: bool,
    pub active: bool,
    pub closed: bool,
    pub needs_frame: bool,
}

/// The native Handler and deterministic headless participants share this owner.
pub(crate) trait CloseParticipant {
    fn parts(&mut self) -> (&mut World, &mut CloseSlot, bool, rndr_core::WindowId);
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum CloseOutcome {
    Continue,
    Deferred,
    Exit,
}

#[derive(Default)]
pub(crate) struct CloseDrain {
    exiting: bool,
}

impl CloseDrain {
    /// Primary window close and quit from any window drain *all* windows.
    /// No world is destroyed until every participating guard is Ready.
    #[cfg(test)]
    pub fn step<'a>(
        &mut self,
        slots: impl IntoIterator<Item = (&'a mut World, &'a mut CloseSlot, bool)>,
    ) -> bool {
        let mut slots: Vec<_> = slots.into_iter().collect();
        self.prepare(slots.iter_mut().map(|(w, c, p)| (&mut **w, &mut **c, *p)));
        self.consume(slots)
    }

    /// The actual renderer close ordering, independent of GPU/event-loop objects.
    /// Both event paths call this after their bounded build/update batch.
    pub fn reconcile<T: CloseParticipant>(
        &mut self,
        slots: &mut [T],
        mut frame: impl FnMut(&mut T),
        validator: Option<&mut CloseValidator>,
    ) -> CloseOutcome {
        self.prepare(slots.iter_mut().map(|s| {
            let (w, c, p, _) = s.parts();
            (w, c, p)
        }));
        for slot in &mut *slots {
            if std::mem::take(&mut slot.parts().1.needs_frame) {
                frame(slot);
            }
        }
        // Hidden callbacks may invalidate, cancel or quit. New intent requires
        // another bounded frame pass, never a post-validation callback.
        self.prepare(slots.iter_mut().map(|s| {
            let (w, c, p, _) = s.parts();
            (w, c, p)
        }));
        if slots.iter_mut().any(|s| s.parts().1.needs_frame) {
            return CloseOutcome::Deferred;
        }
        if self.finish(slots.iter_mut().map(|s| s.parts()), validator) {
            CloseOutcome::Exit
        } else {
            CloseOutcome::Continue
        }
    }

    /// Request/settle intent only. Never publishes a destructive decision.
    pub fn prepare<'a>(
        &mut self,
        slots: impl IntoIterator<Item = (&'a mut World, &'a mut CloseSlot, bool)>,
    ) {
        let mut slots: Vec<_> = slots.into_iter().collect();
        for (_, close, _) in &mut slots {
            close.closed = false;
        }
        for (world, close, primary) in &mut slots {
            let quit = world.resource_mut::<Input>().is_some_and(|input| {
                let quit = input.quit;
                input.quit = false; // consume the edge; Cancel must not immediately re-arm
                quit
            });
            self.exiting |= quit || (*primary && close.requested);
        }
        for (world, close, _) in &mut slots {
            if (self.exiting || close.requested) && !close.active {
                close.active = true;
                close.needs_frame = true;
                if let Some(guard) = world.resource_mut::<CloseGuard>() {
                    guard.request();
                }
            }
            close.requested = false;
        }
        let cancelled = slots.iter().any(|(world, close, _)| {
            close.active
                && world
                    .resource::<CloseGuard>()
                    .is_some_and(|g| matches!(g.state(), CloseState::Idle))
        });
        if self.exiting && cancelled {
            // A cancel in any window cancels the whole process-exit transaction.
            self.exiting = false;
            for (world, close, _) in &mut slots {
                close.active = false;
                close.needs_frame = true;
                if let Some(guard) = world.resource_mut::<CloseGuard>() {
                    guard.cancel();
                }
            }
            return;
        }
        if !self.exiting {
            for (world, close, _) in &mut slots {
                if close.active
                    && world
                        .resource::<CloseGuard>()
                        .is_some_and(|g| matches!(g.state(), CloseState::Idle))
                {
                    close.active = false;
                }
            }
        }
    }

    /// Final callback-free validation and consumption, shared by both native
    /// event paths and headless tests. Caller must destroy immediately afterwards
    /// without updates, builds or changes to the admitted slot set.
    pub fn finish<'a>(
        &mut self,
        slots: impl IntoIterator<Item = (&'a mut World, &'a mut CloseSlot, bool, rndr_core::WindowId)>,
        validator: Option<&mut CloseValidator>,
    ) -> bool {
        let mut slots: Vec<_> = slots.into_iter().collect();
        for (_, close, _, _) in &mut slots {
            close.closed = false;
        }
        if let Some(validator) = validator {
            // Fail closed even before invoking user code on fabricated requests.
            if slots.iter().any(|(w, c, _, _)| {
                !c.active
                    && w.resource::<CloseGuard>()
                        .is_some_and(|g| !matches!(g.state(), CloseState::Idle))
            }) {
                return false;
            }
            if slots.iter().any(|(_, c, _, _)| c.active) {
                let mut worlds: Vec<_> = slots
                    .iter_mut()
                    .map(|(w, c, p, id)| CloseWorld {
                        id: *id,
                        primary: *p,
                        closing: c.active,
                        world: w,
                    })
                    .collect();
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    validator(&mut worlds)
                }));
                // Do not cancel guards, release freezes, or manufacture success.
                match result {
                    Ok(Ok(())) => {}
                    Ok(Err(_)) => return false,
                    Err(payload) => {
                        // A panic_any payload can have a panicking destructor.
                        // Drop it under a second fence, not while returning from
                        // this one. Forget only a secondary panic payload to
                        // avoid unbounded recursive panics during normalization.
                        if let Err(secondary) =
                            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| drop(payload)))
                        {
                            if secondary.is::<String>() || secondary.is::<&'static str>() {
                                drop(secondary);
                            } else {
                                std::mem::forget(secondary);
                            }
                        }
                        return false;
                    }
                }
            }
        }
        self.consume(slots.into_iter().map(|(w, c, p, _)| (w, c, p)))
    }

    fn consume<'a>(
        &mut self,
        slots: impl IntoIterator<Item = (&'a mut World, &'a mut CloseSlot, bool)>,
    ) -> bool {
        let mut slots: Vec<_> = slots.into_iter().collect();
        // Decisions are recomputed, never sticky across later invalidation.
        for (_, close, _) in &mut slots {
            close.closed = false;
        }
        // A requested guard is not authority to destroy its manager. Refuse the
        // entire pass if participation changed or a callback requested new work.
        if slots.iter().any(|(w, c, _)| {
            c.requested
                || w.resource::<Input>().is_some_and(|i| i.quit)
                || w.resource::<CloseGuard>().is_some_and(|g| {
                    if c.active {
                        matches!(g.state(), CloseState::Idle)
                    } else {
                        !matches!(g.state(), CloseState::Idle)
                    }
                })
        }) {
            return false;
        }
        let ready = |world: &World| {
            world
                .resource::<CloseGuard>()
                .is_none_or(|g| matches!(g.state(), CloseState::Ready(_)))
        };
        if self.exiting {
            return slots.iter().all(|(world, _, _)| ready(world));
        }
        for (world, close, _) in &mut slots {
            if close.active {
                if world
                    .resource::<CloseGuard>()
                    .is_some_and(|g| matches!(g.state(), CloseState::Idle))
                {
                    close.active = false;
                } else if ready(world) {
                    close.closed = true;
                }
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rndr_core::{App, CloseRequest};

    fn protected() -> App {
        let mut app = App::new();
        app.world_mut().insert_resource(CloseGuard::default());
        app
    }
    fn pending(app: &App) -> CloseRequest {
        match app.world().resource::<CloseGuard>().unwrap().state() {
            CloseState::Pending(r) => *r,
            state => panic!("expected pending, got {state:?}"),
        }
    }
    fn step(d: &mut CloseDrain, app: &mut App, c: &mut CloseSlot, primary: bool) -> bool {
        d.step([(app.world_mut(), c, primary)])
    }

    #[test]
    fn failure_never_exits_or_destroys_in_memory_text_retry_drains() {
        let mut app = protected();
        let text = "unsaved α\nexact bytes".to_string();
        app.world_mut().insert_resource(text.clone());
        let mut c = CloseSlot {
            requested: true,
            ..Default::default()
        };
        let mut d = CloseDrain::default();
        assert!(!step(&mut d, &mut app, &mut c, true));
        let r = pending(&app);
        // Inject actual IO failure without touching live notes: a file cannot be a directory.
        let path = std::env::temp_dir().join(format!("rndr-close-{}-{:?}", std::process::id(), r));
        std::fs::write(&path, b"not a directory").unwrap();
        let result =
            std::fs::write(path.join("recovery"), text.as_bytes()).map_err(|e| e.to_string());
        std::fs::remove_file(path).unwrap();
        assert!(result.is_err());
        app.world_mut()
            .resource_mut::<CloseGuard>()
            .unwrap()
            .complete(r, result);
        for _ in 0..3 {
            c.requested = true;
            assert!(!step(&mut d, &mut app, &mut c, true));
            assert!(!c.closed);
            assert_eq!(app.world().resource::<String>(), Some(&text));
        }
        let guard = app.world_mut().resource_mut::<CloseGuard>().unwrap();
        let retry = guard.retry(r).unwrap();
        assert!(!guard.complete(r, Ok(())));
        assert!(!step(&mut d, &mut app, &mut c, true));
        app.world_mut()
            .resource_mut::<CloseGuard>()
            .unwrap()
            .complete(retry, Ok(()));
        assert!(step(&mut d, &mut app, &mut c, true));
    }

    #[test]
    fn pending_cancel_quit_is_consumed_and_stale_success_rejected() {
        let mut app = protected();
        app.world_mut().resource_mut::<Input>().unwrap().quit = true;
        let mut c = CloseSlot::default();
        let mut d = CloseDrain::default();
        assert!(!step(&mut d, &mut app, &mut c, true));
        let r = pending(&app);
        assert!(!step(&mut d, &mut app, &mut c, true));
        app.world_mut()
            .resource_mut::<CloseGuard>()
            .unwrap()
            .cancel();
        assert!(!step(&mut d, &mut app, &mut c, true));
        assert!(!c.active);
        assert!(!app
            .world_mut()
            .resource_mut::<CloseGuard>()
            .unwrap()
            .complete(r, Ok(())));
        assert!(!step(&mut d, &mut app, &mut c, true));
        c.requested = true;
        assert!(!step(&mut d, &mut app, &mut c, true));
        assert_ne!(pending(&app), r);
    }

    #[test]
    fn secondary_failure_explicit_discard_closes_only_that_window() {
        let mut app = protected();
        let mut c = CloseSlot {
            requested: true,
            ..Default::default()
        };
        let mut d = CloseDrain::default();
        assert!(!step(&mut d, &mut app, &mut c, false));
        let r = pending(&app);
        let g = app.world_mut().resource_mut::<CloseGuard>().unwrap();
        g.complete(r, Err("no space".into()));
        assert!(!step(&mut d, &mut app, &mut c, false));
        assert!(!c.closed);
        assert!(app
            .world_mut()
            .resource_mut::<CloseGuard>()
            .unwrap()
            .discard(r));
        assert!(!step(&mut d, &mut app, &mut c, false));
        assert!(c.closed);
    }

    #[test]
    fn process_exit_waits_for_secondary_and_cancel_resets_all() {
        let mut a = protected();
        let mut b = protected();
        let mut ca = CloseSlot {
            requested: true,
            ..Default::default()
        };
        let mut cb = CloseSlot::default();
        let mut d = CloseDrain::default();
        assert!(!d.step([
            (a.world_mut(), &mut ca, true),
            (b.world_mut(), &mut cb, false)
        ]));
        let ra = pending(&a);
        let rb = pending(&b);
        a.world_mut()
            .resource_mut::<CloseGuard>()
            .unwrap()
            .complete(ra, Ok(()));
        assert!(!d.step([
            (a.world_mut(), &mut ca, true),
            (b.world_mut(), &mut cb, false)
        ]));
        b.world_mut().resource_mut::<CloseGuard>().unwrap().cancel();
        assert!(!d.step([
            (a.world_mut(), &mut ca, true),
            (b.world_mut(), &mut cb, false)
        ]));
        assert!(!ca.active && !cb.active);
        assert!(matches!(
            a.world().resource::<CloseGuard>().unwrap().state(),
            CloseState::Idle
        ));
        assert!(!b
            .world_mut()
            .resource_mut::<CloseGuard>()
            .unwrap()
            .complete(rb, Ok(())));
        ca.requested = true;
        assert!(!d.step([
            (a.world_mut(), &mut ca, true),
            (b.world_mut(), &mut cb, false)
        ]));
        for app in [&mut a, &mut b] {
            let r = pending(app);
            app.world_mut()
                .resource_mut::<CloseGuard>()
                .unwrap()
                .complete(r, Ok(()));
        }
        assert!(d.step([
            (a.world_mut(), &mut ca, true),
            (b.world_mut(), &mut cb, false)
        ]));
    }

    #[test]
    fn mutation_after_completion_invalidates_ready_before_destruction() {
        let mut app = protected();
        app.world_mut().insert_resource("draft v1".to_string());
        let mut c = CloseSlot {
            requested: true,
            ..Default::default()
        };
        let mut d = CloseDrain::default();
        assert!(!step(&mut d, &mut app, &mut c, true));
        let old = pending(&app);
        app.world_mut()
            .resource_mut::<CloseGuard>()
            .unwrap()
            .complete(old, Ok(()));
        // A later editing system in the same frame must invalidate the snapshot.
        *app.world_mut().resource_mut::<String>().unwrap() = "draft v2".into();
        app.world_mut()
            .resource_mut::<CloseGuard>()
            .unwrap()
            .invalidate();
        assert!(!step(&mut d, &mut app, &mut c, true));
        assert_eq!(app.world().resource::<String>().unwrap(), "draft v2");
        let new = pending(&app);
        assert_ne!(old, new);
        let g = app.world_mut().resource_mut::<CloseGuard>().unwrap();
        assert!(!g.complete(old, Ok(())));
        assert!(g.complete(new, Ok(())));
        assert!(step(&mut d, &mut app, &mut c, true));
    }

    #[test]
    fn unguarded_primary_cannot_destroy_guarded_secondary_or_new_window() {
        for quit_from_secondary in [false, true] {
            let mut a = App::new();
            let mut b = protected();
            let mut ca = CloseSlot {
                requested: !quit_from_secondary,
                ..Default::default()
            };
            let mut cb = CloseSlot::default();
            b.world_mut().resource_mut::<Input>().unwrap().quit = quit_from_secondary;
            let mut d = CloseDrain::default();
            assert!(!d.step([
                (a.world_mut(), &mut ca, true),
                (b.world_mut(), &mut cb, false)
            ]));
            let r = pending(&b);
            b.world_mut()
                .resource_mut::<CloseGuard>()
                .unwrap()
                .complete(r, Ok(()));
            // A window opened during drain must also be protected.
            let mut late = protected();
            let mut cl = CloseSlot::default();
            assert!(!d.step([
                (a.world_mut(), &mut ca, true),
                (b.world_mut(), &mut cb, false),
                (late.world_mut(), &mut cl, false)
            ]));
            let r = pending(&late);
            late.world_mut()
                .resource_mut::<CloseGuard>()
                .unwrap()
                .complete(r, Ok(()));
            assert!(d.step([
                (a.world_mut(), &mut ca, true),
                (b.world_mut(), &mut cb, false),
                (late.world_mut(), &mut cl, false)
            ]));
        }
    }

    #[test]
    fn ordinary_apps_keep_primary_secondary_and_quit_behavior() {
        for (primary, quit, exit) in [
            (true, false, true),
            (false, false, false),
            (false, true, true),
        ] {
            let mut app = App::new();
            app.world_mut().resource_mut::<Input>().unwrap().quit = quit;
            let mut c = CloseSlot {
                requested: !quit,
                ..Default::default()
            };
            let mut d = CloseDrain::default();
            assert_eq!(step(&mut d, &mut app, &mut c, primary), exit);
            assert_eq!(c.closed, !exit);
        }
    }
}
