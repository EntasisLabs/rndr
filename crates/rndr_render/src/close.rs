//! Runner close coordination, kept independent of native/GPU objects for tests.
use rndr_core::{CloseGuard, CloseState, Input, World};

#[derive(Default)]
pub(crate) struct CloseSlot {
    pub requested: bool,
    pub active: bool,
    pub closed: bool,
    pub needs_frame: bool,
}

#[derive(Default)]
pub(crate) struct CloseDrain {
    exiting: bool,
}

impl CloseDrain {
    /// Primary window close and quit from any window drain *all* windows.
    /// No world is destroyed until every participating guard is Ready.
    pub fn step<'a>(
        &mut self,
        slots: impl IntoIterator<Item = (&'a mut World, &'a mut CloseSlot, bool)>,
    ) -> bool {
        let mut slots: Vec<_> = slots.into_iter().collect();
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
