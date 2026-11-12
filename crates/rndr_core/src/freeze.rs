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
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| n.checked_add(1))
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
