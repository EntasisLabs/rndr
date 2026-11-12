//! Host close protection at native document writers, independent of readonly UI policy.
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

#[derive(Debug, Default)]
pub(crate) struct FreezeState(Arc<AtomicUsize>);
impl FreezeState {
    pub(crate) fn active(&self) -> bool {
        self.0.load(Ordering::Acquire) != 0
    }
    pub(crate) fn acquire(&self) -> Option<DocumentFreeze> {
        self.0
            .try_update(Ordering::AcqRel, Ordering::Acquire, |n| n.checked_add(1))
            .ok()?;
        Some(DocumentFreeze(self.0.clone()))
    }
}
/// Owned content/history admission freeze. Reads and selection remain available.
/// Dropping the final lease reopens writers without changing bytes/revisions/history.
/// Keep this with the host's protection attempt through failure and native Ready.
/// Not a durability receipt; cannot be forged, cloned or disabled by set_readonly.
#[derive(Debug)]
#[must_use = "retain through protection, including Failed and native Ready"]
pub struct DocumentFreeze(Arc<AtomicUsize>);
impl Drop for DocumentFreeze {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_leases_reopen_only_after_final_drop() {
        let state = FreezeState::default();
        assert!(!state.active());
        let first = state.acquire().unwrap();
        let second = state.acquire().unwrap();
        assert_eq!(state.0.load(Ordering::Acquire), 2);
        drop(first);
        assert!(state.active());
        drop(second);
        assert!(!state.active());
    }

    #[test]
    fn exhausted_count_refuses_without_wrapping() {
        let state = FreezeState(Arc::new(AtomicUsize::new(usize::MAX - 1)));
        let final_lease = state.acquire().unwrap();
        assert!(state.acquire().is_none());
        assert_eq!(state.0.load(Ordering::Acquire), usize::MAX);
        assert!(state.active());
        drop(final_lease);
        assert_eq!(state.0.load(Ordering::Acquire), usize::MAX - 1);
    }

    #[test]
    fn concurrent_leases_preserve_count() {
        let state = FreezeState::default();
        let barrier = std::sync::Barrier::new(8);
        std::thread::scope(|scope| {
            for _ in 0..8 {
                let state = &state;
                let barrier = &barrier;
                scope.spawn(move || {
                    let lease = state.acquire().unwrap();
                    barrier.wait();
                    assert_eq!(state.0.load(Ordering::Acquire), 8);
                    barrier.wait();
                    drop(lease);
                });
            }
        });
        assert!(!state.active());
    }
}
