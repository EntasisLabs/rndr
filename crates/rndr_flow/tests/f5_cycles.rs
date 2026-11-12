mod f5_support;
use f5_support::*;
use rndr_flow::*;
#[test]
fn retained_guard_cycles_return_to_exact_bounded_resource_baseline() {
    for seed in [1, 7, 42] {
        let mut f = Fixture::new();
        f.flow.pump();
        let baseline = f.flow.counts();
        let work = f.flow.work_counts();
        let mut history_baseline = None;
        for cycle in 0..184 {
            let e = f.push(cycle);
            let before = f.snapshot();
            let op = f.close(vec![e]);
            f.choose(op, Choice::Cancel).unwrap();
            f.tick();
            assert_eq!(f.snapshot(), before);
            let op = f.close(vec![e]);
            f.choose(op, Choice::Discard).unwrap();
            f.tick();
            for _ in 0..128 {
                f.flow.pump_work(1);
            }
            f.flow.pump();
            assert_eq!(f.flow.counts(), baseline);
            assert_eq!(f.flow.work_counts(), work);
            let h = f.history.counts();
            assert_eq!(h.tabs, 1);
            assert_eq!((h.entries, h.pending, h.queued), (0, 0, 0));
            assert!(h.outcomes <= 64);
            if cycle >= 64 {
                assert_eq!(h, *history_baseline.get_or_insert(h));
                println!("F5 baseline seed={seed} cycle={} counts={baseline:?} work={work:?} history={h:?}",cycle-64);
            }
        }
        drop(f.history);
        drop(f.app);
        drop(f.root);
        for _ in 0..512 {
            f.flow.pump_work(1);
        }
        f.flow.pump();
        assert_eq!(f.flow.counts(), Counts::default());
        assert_eq!(f.flow.work_counts(), WorkCounts::default());
    }
}
