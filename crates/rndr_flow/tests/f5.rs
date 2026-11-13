mod f5_support;
use f5_support::*;
use rndr_core::editing::EditError;
use rndr_flow::*;
use std::{cell::Cell, rc::Rc};
#[test]
fn cancel_preserves_exact_bytes_revision_history_selection_and_resources() {
    for _seed in [1, 7, 42] {
        let mut f = Fixture::new();
        let entry = f.push(1);
        let batch = f.batch();
        f.source_mut().commit(batch).unwrap();
        let before = f.snapshot();
        let id = f.close(vec![entry]);
        assert_eq!(f.history.phase(id), Some(Phase::AwaitingDecision));
        let duplicate = f
            .history
            .sender()
            .submit(Navigation::Close {
                entries: vec![entry],
            })
            .unwrap();
        assert_eq!(id, duplicate);
        let observer = f.history.decision(id).unwrap();
        let _ = observer;
        for _ in 0..4 {
            f.tick();
        }
        assert_eq!(f.history.phase(id), Some(Phase::AwaitingDecision));
        assert_eq!(f.snapshot(), before);
        f.choose(id, Choice::Cancel).unwrap();
        f.tick();
        assert_eq!(f.history.phase(id), Some(Phase::Cancelled));
        assert_eq!(f.snapshot(), before);
        assert!(!f.source().is_readonly());
    }
}
#[test]
fn confirmed_disposal_once_stale_duplicate_foreign_decisions_and_saves_refuse() {
    let mut f = Fixture::new();
    let e = f.push(1);
    let drops = Rc::new(Cell::new(0));
    let seen = drops.clone();
    f.flow
        .register_cleanup(e, CleanupKind::Lifetime, move |_| {
            seen.set(seen.get() + 1);
            Ok(())
        })
        .unwrap();
    let op = f.close(vec![e]);
    let decision = f.history.decision(op).unwrap();
    f.choose(op, Choice::Save).unwrap();
    let ticket = f.history.guard(op).unwrap().saves()[0];
    let batch = f.batch();
    assert_eq!(f.source_mut().commit(batch), Err(EditError::Readonly));
    assert_eq!(f.choose(op, Choice::Discard), Err(Error::Stale));
    let mut other = Fixture::new();
    let oe = other.push(2);
    let oo = other.close(vec![oe]);
    let mut worlds = [HostWorld::new(other.world, other.app.world_mut()).unwrap()];
    assert_eq!(
        other.history.choose(
            &mut other.host.pass(&other.flow, &mut worlds),
            decision,
            Choice::Discard
        ),
        Err(Error::Stale)
    );
    assert_eq!(other.save(oo, ticket, |_, _| Ok(())), Err(Error::Stale));
    f.save(op, ticket, |_, _| Ok(())).unwrap();
    assert_eq!(
        f.save(op, ticket, |_, _| panic!("duplicate must not invoke save")),
        Err(Error::Stale)
    );
    f.tick();
    assert_eq!(f.history.phase(op), Some(Phase::Committed));
    assert_eq!(f.history.cancel(op), Err(Error::Invalid));
    for _ in 0..10 {
        f.flow.pump_work(1);
    }
    f.flow.pump();
    assert_eq!(drops.get(), 1);
    assert!(f.history.entries(f.tab).unwrap().is_empty());
}
#[test]
fn failure_panic_and_lost_save_retain_freeze_until_explicit_cancel_or_discard() {
    for failure in [
        SaveFailure::Failed,
        SaveFailure::Panicked,
        SaveFailure::Lost,
    ] {
        let mut f = Fixture::new();
        let e = f.push(1);
        let before = f.snapshot();
        let op = f.close(vec![e]);
        f.choose(op, Choice::Save).unwrap();
        let ticket = f.history.guard(op).unwrap().saves()[0];
        if failure == SaveFailure::Panicked {
            f.save(op, ticket, |_, _| panic!("save panic")).unwrap();
        } else {
            let mut worlds = [HostWorld::new(f.world, f.app.world_mut()).unwrap()];
            f.host
                .pass(&f.flow, &mut worlds)
                .fail_save(f.history.guard_mut(op).unwrap(), ticket, failure)
                .unwrap();
        }
        f.tick();
        assert_eq!(f.history.phase(op), Some(Phase::Failed));
        assert!(f.source().is_readonly());
        assert_eq!(f.snapshot(), before);
        let old = f.history.decision(op).unwrap();
        f.choose(op, Choice::Save).unwrap();
        let retry = f.history.guard(op).unwrap().saves()[0];
        assert_ne!(retry, ticket);
        assert_eq!(f.save(op, ticket, |_, _| Ok(())), Err(Error::Stale));
        f.save(op, retry, |_, _| Err(SaveFailure::Failed)).unwrap();
        f.tick();
        assert_ne!(f.history.decision(op), Some(old));
        f.choose(op, Choice::Cancel).unwrap();
        f.tick();
        assert_eq!(f.snapshot(), before);
        assert!(!f.source().is_readonly());
        let op = f.close(vec![e]);
        f.choose(op, Choice::Discard).unwrap();
        f.tick();
        assert_eq!(f.history.phase(op), Some(Phase::Committed));
    }
}
#[test]
fn source_programmatic_undo_redo_reload_and_input_policy_cannot_bypass_save_freeze() {
    let mut f = Fixture::new();
    let e = f.push(1);
    let batch = f.batch();
    f.source_mut().commit(batch).unwrap();
    f.source_mut().undo().unwrap();
    let before = f.snapshot();
    let op = f.close(vec![e]);
    f.choose(op, Choice::Save).unwrap();
    let batch = f.batch();
    let revision = f.source().revision();
    f.source_mut().set_readonly(false).unwrap();
    assert!(f.source().is_readonly());
    assert_eq!(f.source_mut().commit(batch), Err(EditError::Readonly));
    assert_eq!(f.source_mut().undo(), Err(EditError::Readonly));
    assert_eq!(f.source_mut().redo(), Err(EditError::Readonly));
    assert_eq!(
        f.source_mut().reload(revision, "unavoidable reload"),
        Err(EditError::Readonly)
    );
    assert_eq!(f.snapshot(), before);
    f.history.cancel(op).unwrap();
    f.tick();
    let batch = f.batch();
    f.source_mut().commit(batch).unwrap();
    let op2 = f.close(vec![e]);
    assert_ne!(op, op2);
    assert_eq!(f.history.phase(op2), Some(Phase::AwaitingDecision));
}
#[test]
fn retained_back_forward_replace_and_forward_truncation_guard_exact_incarnations() {
    let mut f = Fixture::new();
    let a = f.push(1);
    let b = f.push(2);
    let c = f.push(3);
    let op = f
        .history
        .sender()
        .submit(f.history.back(f.tab).unwrap())
        .unwrap();
    f.tick();
    assert_eq!(f.history.phase(op), Some(Phase::Committed));
    assert_eq!(f.history.selected(f.tab).unwrap(), Some(b));
    let op = f
        .history
        .sender()
        .submit(f.history.forward(f.tab).unwrap())
        .unwrap();
    f.tick();
    assert_eq!(f.history.phase(op), Some(Phase::Committed));
    assert_eq!(f.history.selected(f.tab).unwrap(), Some(c));
    let op = f
        .history
        .sender()
        .submit(Navigation::Select {
            tab: f.tab,
            entry: a,
        })
        .unwrap();
    f.tick();
    assert_eq!(f.history.phase(op), Some(Phase::Committed));
    let new = f.entry(4);
    let new_id = new.id();
    let op = f
        .history
        .sender()
        .submit(Navigation::Push {
            tab: f.tab,
            after: Some(a),
            entry: new,
        })
        .unwrap();
    f.tick();
    assert_eq!(f.history.phase(op), Some(Phase::AwaitingDecision));
    assert_eq!(
        f.history
            .entries(f.tab)
            .unwrap()
            .iter()
            .map(|e| e.id())
            .collect::<Vec<_>>(),
        vec![a, b, c]
    );
    f.choose(op, Choice::Cancel).unwrap();
    f.tick();
    let new = f.entry(5);
    assert_ne!(new.id(), new_id);
    let replacement = new.id();
    let op = f
        .history
        .sender()
        .submit(Navigation::Replace {
            tab: f.tab,
            old: b,
            entry: new,
        })
        .unwrap();
    f.tick();
    f.choose(op, Choice::Discard).unwrap();
    f.tick();
    assert_eq!(
        f.history
            .entries(f.tab)
            .unwrap()
            .iter()
            .map(|e| e.id())
            .collect::<Vec<_>>(),
        vec![a, replacement, c]
    );
    assert_eq!(f.flow.state(b), State::Disposed);
    assert_ne!(f.flow.state(a), State::Disposed);
    assert_ne!(f.flow.state(c), State::Disposed);
}
#[test]
fn multi_entry_validation_is_atomic_on_unavoidable_participant_loss() {
    let mut f = Fixture::new();
    let a = f.push(1);
    let b = f.push(2);
    let op = f.close(vec![a, b]);
    f.choose(op, Choice::Save).unwrap();
    let ticket = f.history.guard(op).unwrap().saves()[0];
    f.save(op, ticket, |_, _| Ok(())).unwrap();
    // Explicit terminal scope loss invalidates the entire lease snapshot. The
    // already completed persistence is not rolled back; no other owner is disposed.
    f.flow.dispose(a).unwrap();
    f.tick();
    assert_eq!(f.history.phase(op), Some(Phase::Failed));
    assert_ne!(f.flow.state(b), State::Disposed);
    assert_eq!(f.history.entries(f.tab).unwrap().len(), 2);
    assert!(f.source().is_readonly());
    f.choose(op, Choice::Cancel).unwrap();
    f.tick();
}
#[test]
fn reentrant_submission_conflict_queue_and_unrelated_selection_progress() {
    let mut f = Fixture::new();
    let a = f.push(1);
    let b = f.push(2);
    let close = f.close(vec![b]);
    let select = f
        .history
        .sender()
        .submit(Navigation::Select {
            tab: f.tab,
            entry: a,
        })
        .unwrap();
    f.tick();
    assert_eq!(f.history.phase(select), Some(Phase::Queued));
    let tab_owner = f.flow.create_scope(f.world_scope()).unwrap();
    let other = f.history.add_tab(&f.flow, f.world, tab_owner).unwrap();
    let independent_target = f.new_target();
    let e = f.entry_for(other, 3, independent_target);
    let independent = f
        .history
        .sender()
        .submit(Navigation::Push {
            tab: other,
            after: None,
            entry: e,
        })
        .unwrap();
    f.tick();
    assert_eq!(f.history.phase(independent), Some(Phase::Committed));
    f.choose(close, Choice::Save).unwrap();
    let ticket = f.history.guard(close).unwrap().saves()[0];
    let sender = f.history.sender();
    let tab = f.tab;
    let mut queued = None;
    f.save(close, ticket, |_, _| {
        queued = Some(sender.submit(Navigation::Select { tab, entry: a }).unwrap());
        Ok(())
    })
    .unwrap();
    f.tick();
    f.tick();
    f.tick();
    assert_eq!(f.history.phase(select), Some(Phase::Committed));
    assert_eq!(f.history.phase(queued.unwrap()), Some(Phase::Committed));
}
#[test]
fn actual_source_save_lane_is_live_while_close_waits() {
    let mut f = Fixture::new();
    let a = f.push(1);
    let op = f.close(vec![a]);
    f.choose(op, Choice::Save).unwrap();
    let ticket = f.history.guard(op).unwrap().saves()[0];
    let path = std::env::temp_dir().join(format!(
        "rndr-f5-{}-{}.txt",
        std::process::id(),
        ticket.revision()
    ));
    let mut worlds = [HostWorld::new(f.world, f.app.world_mut()).unwrap()];
    f.host
        .pass(&f.flow, &mut worlds)
        .save_source(f.history.guard_mut(op).unwrap(), ticket, &path)
        .unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), f.source().text());
    std::fs::remove_file(path).unwrap();
    f.tick();
    assert_eq!(f.history.phase(op), Some(Phase::Committed));
}
#[test]
fn pending_ime_blocks_snapshot_without_discarding_preedit() {
    let mut f = Fixture::new();
    let a = f.push(1);
    f.app
        .world_mut()
        .resource_mut::<rndr_core::Input>()
        .unwrap()
        .ime_preedit = "未確定".into();
    let op = f.close(vec![a]);
    assert_eq!(f.history.phase(op), Some(Phase::Queued));
    assert_eq!(
        f.app
            .world()
            .resource::<rndr_core::Input>()
            .unwrap()
            .ime_preedit,
        "未確定"
    );
    assert_ne!(f.flow.state(a), State::Disposed);
    f.history.cancel(op).unwrap();
}
