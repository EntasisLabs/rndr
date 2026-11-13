mod f5_support;
use f5_support::*;
use rndr_flow::*;
#[test]
fn actual_save_job_or_result_loss_fails_closed_and_late_retry_cannot_replace_it() {
    for lose_result in [false, true] {
        let mut f = Fixture::new();
        let a = f.push(1);
        let before = f.snapshot();
        let op = f.close(vec![a]);
        f.choose(op, Choice::Save).unwrap();
        let ticket = f.history.guard(op).unwrap().saves()[0];
        let job = f.history.guard_mut(op).unwrap().own_save(ticket).unwrap();
        assert!(f.history.guard_mut(op).unwrap().own_save(ticket).is_err());
        assert_eq!(
            f.save(op, ticket, |_, _| panic!(
                "owned producer cannot be bypassed"
            )),
            Err(Error::Stale)
        );
        if lose_result {
            let result = std::thread::spawn(move || job.run(|| Ok(())))
                .join()
                .unwrap();
            drop(result);
        } else {
            drop(job);
        }
        assert_eq!(f.save(op, ticket, |_, _| Ok(())), Err(Error::Stale));
        f.tick();
        assert_eq!(f.history.phase(op), Some(Phase::Failed));
        assert_eq!(f.snapshot(), before);
        assert!(f.source().is_readonly());
        f.choose(op, Choice::Save).unwrap();
        let ticket2 = f.history.guard(op).unwrap().saves()[0];
        let job = f.history.guard_mut(op).unwrap().own_save(ticket2).unwrap();
        let result = std::thread::spawn(move || job.run(|| Ok(())))
            .join()
            .unwrap();
        let mut worlds = [HostWorld::new(f.world, f.app.world_mut()).unwrap()];
        f.host
            .pass(&f.flow, &mut worlds)
            .complete_owned_save(f.history.guard_mut(op).unwrap(), result)
            .unwrap();
        f.tick();
        assert_eq!(f.history.phase(op), Some(Phase::Committed));
    }
}
#[test]
fn owned_save_panic_is_failed_not_discard_and_result_after_cancel_is_foreign() {
    let mut f = Fixture::new();
    let a = f.push(1);
    let op = f.close(vec![a]);
    f.choose(op, Choice::Save).unwrap();
    let ticket = f.history.guard(op).unwrap().saves()[0];
    let work = f.history.guard_mut(op).unwrap().own_save(ticket).unwrap();
    let result = work.run(|| panic!("actual job panic"));
    {
        let mut worlds = [HostWorld::new(f.world, f.app.world_mut()).unwrap()];
        f.host
            .pass(&f.flow, &mut worlds)
            .complete_owned_save(f.history.guard_mut(op).unwrap(), result)
            .unwrap();
    }
    f.tick();
    assert_eq!(f.history.phase(op), Some(Phase::Failed));
    assert!(f.source().is_readonly());
    f.choose(op, Choice::Save).unwrap();
    let ticket = f.history.guard(op).unwrap().saves()[0];
    let work = f.history.guard_mut(op).unwrap().own_save(ticket).unwrap();
    f.history.cancel(op).unwrap();
    f.tick();
    let result = work.run(|| Ok(()));
    let op2 = f.close(vec![a]);
    f.choose(op2, Choice::Save).unwrap();
    let mut worlds = [HostWorld::new(f.world, f.app.world_mut()).unwrap()];
    assert_eq!(
        f.host
            .pass(&f.flow, &mut worlds)
            .complete_owned_save(f.history.guard_mut(op2).unwrap(), result),
        Err(Error::Stale)
    );
    assert_ne!(f.flow.state(a), State::Disposed);
}
