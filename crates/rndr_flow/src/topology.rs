//! Explicit synchronous topology protection, outside retirement work budgets.
use crate::*;
/// Freezes new admission; existing participant loss invalidates the snapshot.
#[must_use]
pub struct TopologyFreeze {
    counter: Arc<AtomicU64>,
    participants: Vec<(Liveness, Option<Arc<AtomicBool>>)>,
    owner: Liveness,
}
impl TopologyFreeze {
    pub fn valid(&self) -> bool {
        self.owner.is_live()
            && self.participants.iter().all(|(life, held)| {
                life.is_live() && held.as_ref().is_none_or(|h| h.load(Ordering::Acquire))
            })
    }
}
impl Drop for TopologyFreeze {
    fn drop(&mut self) {
        self.counter.fetch_sub(1, Ordering::AcqRel);
    }
}
impl Coordinator {
    fn topology_freeze(
        &self,
        id: ScopeId,
        participants: Vec<(Liveness, Option<Arc<AtomicBool>>)>,
    ) -> Result<TopologyFreeze, Error> {
        let owner = self.liveness(id)?;
        let counter = self.live_scope(id)?.topology_freeze.clone();
        counter
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| n.checked_add(1))
            .map_err(|_| Error::Exhausted)?;
        Ok(TopologyFreeze {
            counter,
            participants,
            owner,
        })
    }
    /// All held presentations, including hidden and other Worlds. New leases
    /// refuse at `lease`; lost leases/ancestors invalidate final validation.
    pub fn freeze_document_leases(&self, document: DocumentId) -> Result<TopologyFreeze, Error> {
        self.live_scope(document.0)?;
        let participants = self
            .leases
            .values()
            .filter(|l| l.document == document && self.held(l))
            .map(|l| {
                (
                    Liveness(self.scopes[&l.scope].life.clone()),
                    Some(l.held.clone()),
                )
            })
            .collect();
        self.topology_freeze(document.0, participants)
    }
    /// Freeze native participant admission through Ready consumption. Existing
    /// World loss is terminal UI loss, not successful save/protection.
    pub fn freeze_worlds(&self) -> Result<TopologyFreeze, Error> {
        let participants = self
            .scopes
            .values()
            .filter(|s| s.role == Role::World && s.life.live())
            .map(|s| (Liveness(s.life.clone()), None))
            .collect();
        self.topology_freeze(self.root, participants)
    }
    pub fn world_is_live(&self, id: WorldId) -> bool {
        self.live_scope(id.0).is_ok_and(|s| s.role == Role::World)
    }
}
