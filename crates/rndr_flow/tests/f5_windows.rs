mod f5_support;
use f5_support::*;
use rndr_core::App;
use rndr_flow::*;
use std::{cell::Cell, rc::Rc};
#[test]
fn terminal_window_loss_reclaims_tabs_without_closing_shared_document() {
    for seed in [1, 7, 42] {
        let mut f = Fixture::new();
        f.flow.pump();
        let baseline = f.flow.counts();
        let work = f.flow.work_counts();
        let attempts = Rc::new(Cell::new(0));
        for cycle in 0..120 {
            let owner = f.flow.register_world().unwrap();
            let world = owner.id();
            let parent = owner.scope();
            let mut app = App::new();
            owner.bind(app.world_mut()).unwrap();
            let tab_owner = f.flow.create_scope(parent).unwrap();
            let tab = f.history.add_tab(&f.flow, world, tab_owner).unwrap();
            let entry = f.entry_for(tab, cycle, f.target);
            let id = entry.id();
            let seen = attempts.clone();
            f.flow
                .register_cleanup(id, CleanupKind::Lifetime, move |_| {
                    seen.set(seen.get() + 1);
                    Ok(())
                })
                .unwrap();
            let op = f
                .history
                .sender()
                .submit(Navigation::Push {
                    tab,
                    after: None,
                    entry,
                })
                .unwrap();
            f.tick();
            assert_eq!(f.history.phase(op), Some(Phase::Committed));
            drop(app);
            assert_eq!(f.flow.state(id), State::Disposed);
            f.history.reconcile_world_loss(&f.flow);
            f.host.reconcile_world_loss(&f.flow);
            f.flow.pump();
            assert_eq!(f.flow.counts(), baseline);
            assert_eq!(f.flow.work_counts(), work);
            let h = f.history.counts();
            assert_eq!((h.tabs, h.entries, h.pending, h.queued), (1, 0, 0, 0));
            assert!(h.outcomes <= 64);
            assert_eq!(attempts.get(), cycle + 1);
            assert!(f.flow.document_is_live(f.target.document));
        }
        println!("F5 windows seed={seed} cycles=120 exact_baseline=true shared_document_live=true exactly_once=120");
    }
}
