//! Typed retained metadata. Documents, editor history and UI trees stay native.
use crate::{allocate_authority, Error, Rejected, ScopeId, ScopeOwner, WorldId};
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    rc::Rc,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OperationId {
    pub(crate) authority: u64,
    pub(crate) serial: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DecisionId {
    pub(crate) operation: OperationId,
    pub(crate) round: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    Save,
    Discard,
    Cancel,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Queued,
    AwaitingDecision,
    Saving,
    Failed,
    Ready,
    Committed,
    Cancelled,
}
#[derive(Clone, Copy, Debug)]
pub struct HistoryLimits {
    pub tabs: usize,
    pub entries: usize,
    pub operations: usize,
    pub outcomes: usize,
}
impl Default for HistoryLimits {
    fn default() -> Self {
        Self {
            tabs: 64,
            entries: 512,
            operations: 64,
            outcomes: 64,
        }
    }
}
/// Single retained presentation owner; route is typed application data, not a URI.
/// Local model/history is owned by the host under `owner`, never copied into flow.
pub struct RetainedEntry<R, D> {
    pub route: R,
    pub document: D,
    pub owner: ScopeOwner,
    pub lease: crate::PresentationLease,
}
impl<R, D> RetainedEntry<R, D> {
    pub fn id(&self) -> ScopeId {
        self.owner.id()
    }
}
struct Tab<R, D> {
    owner: ScopeOwner,
    world: WorldId,
    history: Vec<RetainedEntry<R, D>>,
    selected: Option<ScopeId>,
}
/// All contextual destinations are resolved by constructors against exact retained IDs
/// before enqueue. Queued commands never reinterpret “current tab” or a URI.
pub enum Navigation<R, D> {
    Push {
        tab: ScopeId,
        after: Option<ScopeId>,
        entry: RetainedEntry<R, D>,
    },
    Replace {
        tab: ScopeId,
        old: ScopeId,
        entry: RetainedEntry<R, D>,
    },
    Select {
        tab: ScopeId,
        entry: ScopeId,
    },
    Close {
        entries: Vec<ScopeId>,
    },
}
impl<R, D> Navigation<R, D> {
    fn new_entry(&self) -> Option<&RetainedEntry<R, D>> {
        match self {
            Self::Push { entry, .. } | Self::Replace { entry, .. } => Some(entry),
            _ => None,
        }
    }
}
struct Inbox<R, D> {
    authority: u64,
    serial: Cell<u64>,
    capacity: usize,
    entry_limit: usize,
    live: Cell<bool>,
    charged: Cell<usize>,
    coalesced: RefCell<Vec<(OperationId, Vec<ScopeId>)>>,
    queue: RefCell<VecDeque<(OperationId, Navigation<R, D>)>>,
}
/// Callback-safe bounded submission. Drop/timeout is not a decision.
pub struct NavigationSender<R, D>(Rc<Inbox<R, D>>);
impl<R, D> Clone for NavigationSender<R, D> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}
impl<R, D> NavigationSender<R, D> {
    pub fn submit(
        &self,
        command: Navigation<R, D>,
    ) -> Result<OperationId, Rejected<Navigation<R, D>>> {
        if !self.0.live.get() {
            return Err(Rejected {
                reason: Error::Stale,
                value: command,
            });
        }
        if let Navigation::Close { entries } = &command {
            if entries.len() > self.0.entry_limit {
                return Err(Rejected {
                    reason: Error::Capacity,
                    value: command,
                });
            }
            if entries.is_empty()
                || entries
                    .iter()
                    .enumerate()
                    .any(|(i, id)| entries[..i].contains(id))
            {
                return Err(Rejected {
                    reason: Error::Invalid,
                    value: command,
                });
            }
            if let Some((id, _)) =
                self.0.coalesced.borrow().iter().find(|(_, e)| {
                    e.len() == entries.len() && e.iter().all(|id| entries.contains(id))
                })
            {
                return Ok(*id);
            }
        }
        let serial = match self.0.serial.get().checked_add(1) {
            Some(n) => n,
            None => {
                return Err(Rejected {
                    reason: Error::Exhausted,
                    value: command,
                })
            }
        };
        let mut q = self.0.queue.borrow_mut();
        if self.0.charged.get() >= self.0.capacity {
            return Err(Rejected {
                reason: Error::Capacity,
                value: command,
            });
        }
        let id = OperationId {
            authority: self.0.authority,
            serial,
        };
        self.0.serial.set(serial);
        self.0.charged.set(self.0.charged.get() + 1);
        if let Navigation::Close { entries } = &command {
            self.0.coalesced.borrow_mut().push((id, entries.clone()));
        }
        q.push_back((id, command));
        Ok(id)
    }
}
struct Pending<R, D, G> {
    id: OperationId,
    command: Navigation<R, D>,
    affected: Vec<ScopeId>,
    tabs: Vec<ScopeId>,
    phase: Phase,
    round: u64,
    guard: Option<G>,
}
/// Native protection belongs to a concrete host adapter. No callbacks run inside
/// validate/commit. Freeze remains owned by G through failure and until commit/cancel.
pub trait TransitionHost<D> {
    type Guard;
    fn protect(
        &mut self,
        operation: OperationId,
        targets: &[(ScopeId, D)],
    ) -> Result<Self::Guard, Error>;
    fn dirty(&self, guard: &Self::Guard) -> bool;
    fn choose(
        &mut self,
        guard: &mut Self::Guard,
        decision: DecisionId,
        choice: Choice,
    ) -> Result<Phase, Error>;
    fn poll(&mut self, guard: &mut Self::Guard) -> Phase;
    fn validate(&self, guard: &Self::Guard) -> Result<(), Error>;
    /// Observational, callback-free validation of the incoming native identity and
    /// its exact presentation lease. Required even when no outgoing target exists.
    fn validate_entry(
        &self,
        scope: ScopeId,
        document: &D,
        lease: &crate::PresentationLease,
    ) -> Result<(), Error>;
}
/// Bounded retained history plus bounded owned operations/outcomes. Not an executor,
/// document store, UI tree, navigation service or implicit resource eviction policy.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HistoryCounts {
    pub tabs: usize,
    pub entries: usize,
    pub pending: usize,
    pub queued: usize,
    pub outcomes: usize,
}
pub struct Retained<R, D, G> {
    limits: HistoryLimits,
    tabs: Vec<Tab<R, D>>,
    pending: Vec<Pending<R, D, G>>,
    outcomes: VecDeque<(OperationId, Phase)>,
    sender: NavigationSender<R, D>,
}
impl<R, D, G> Drop for Retained<R, D, G> {
    fn drop(&mut self) {
        self.sender.0.live.set(false);
        let queued = std::mem::take(&mut *self.sender.0.queue.borrow_mut());
        drop(queued);
    }
}
#[cfg(test)]
#[path = "history_tests.rs"]
mod bounds_tests;
impl<R, D: Clone + Eq, G> Retained<R, D, G> {
    pub fn new(limits: HistoryLimits) -> Result<Self, Error> {
        if [
            limits.tabs,
            limits.entries,
            limits.operations,
            limits.outcomes,
        ]
        .contains(&0)
        {
            return Err(Error::Invalid);
        }
        let authority = allocate_authority(&crate::AUTHORITIES)?;
        Ok(Self {
            limits,
            tabs: vec![],
            pending: vec![],
            outcomes: VecDeque::new(),
            sender: NavigationSender(Rc::new(Inbox {
                authority,
                serial: Cell::new(0),
                capacity: limits.operations,
                entry_limit: limits.entries,
                live: Cell::new(true),
                charged: Cell::new(0),
                coalesced: RefCell::new(vec![]),
                queue: RefCell::new(VecDeque::new()),
            })),
        })
    }
    /// Terminal World loss, never a native Ready observation. Queued commands
    /// keep their exact IDs and refuse on the next tick; affected owned attempts
    /// are explicitly abandoned before their dead tab resources are released.
    pub fn reconcile_world_loss(&mut self, flow: &crate::Coordinator) {
        let dead: Vec<_> = self
            .tabs
            .iter()
            .filter(|t| !flow.world_is_live(t.world))
            .map(|t| t.owner.id())
            .collect();
        let operations: Vec<_> = self
            .pending
            .iter()
            .filter(|p| p.tabs.iter().any(|id| dead.contains(id)))
            .map(|p| p.id)
            .collect();
        for id in operations {
            let at = self.pending.iter().position(|p| p.id == id).unwrap();
            let p = self.pending.remove(at);
            self.finish(id, Phase::Cancelled);
            drop(p);
        }
        let mut retired = vec![];
        let mut at = 0;
        while at < self.tabs.len() {
            if dead.contains(&self.tabs[at].owner.id()) {
                retired.push(self.tabs.remove(at));
            } else {
                at += 1;
            }
        }
        drop(retired);
    }
    pub fn counts(&self) -> HistoryCounts {
        HistoryCounts {
            tabs: self.tabs.len(),
            entries: self.tabs.iter().map(|t| t.history.len()).sum(),
            pending: self.pending.len(),
            queued: self.sender.0.queue.borrow().len(),
            outcomes: self.outcomes.len(),
        }
    }
    pub fn sender(&self) -> NavigationSender<R, D> {
        self.sender.clone()
    }
    pub fn add_tab(
        &mut self,
        flow: &crate::Coordinator,
        world: WorldId,
        owner: ScopeOwner,
    ) -> Result<ScopeId, Rejected<ScopeOwner>> {
        if self.tabs.len() >= self.limits.tabs {
            return Err(Rejected {
                reason: Error::Capacity,
                value: owner,
            });
        }
        if !flow.world_is_live(world)
            || flow
                .scopes
                .get(&owner.id())
                .is_none_or(|s| s.parent != Some(world.0))
            || !owner.liveness().is_live()
            || self.tabs.iter().any(|t| t.owner.id() == owner.id())
        {
            return Err(Rejected {
                reason: Error::Invalid,
                value: owner,
            });
        }
        let id = owner.id();
        self.tabs.push(Tab {
            owner,
            world,
            history: vec![],
            selected: None,
        });
        Ok(id)
    }
    pub fn entries(&self, tab: ScopeId) -> Result<&[RetainedEntry<R, D>], Error> {
        Ok(&self.tab(tab)?.history)
    }
    pub fn selected(&self, tab: ScopeId) -> Result<Option<ScopeId>, Error> {
        Ok(self.tab(tab)?.selected)
    }
    pub fn world(&self, tab: ScopeId) -> Result<WorldId, Error> {
        Ok(self.tab(tab)?.world)
    }
    fn tab(&self, id: ScopeId) -> Result<&Tab<R, D>, Error> {
        self.tabs
            .iter()
            .find(|t| t.owner.id() == id && t.owner.liveness().is_live())
            .ok_or(Error::Stale)
    }
    pub fn back(&self, tab: ScopeId) -> Result<Navigation<R, D>, Error> {
        self.move_by(tab, false)
    }
    pub fn forward(&self, tab: ScopeId) -> Result<Navigation<R, D>, Error> {
        self.move_by(tab, true)
    }
    fn move_by(&self, tab: ScopeId, forward: bool) -> Result<Navigation<R, D>, Error> {
        let t = self.tab(tab)?;
        let at = t
            .history
            .iter()
            .position(|e| Some(e.id()) == t.selected)
            .ok_or(Error::Stale)?;
        let at = if forward {
            at.checked_add(1)
        } else {
            at.checked_sub(1)
        }
        .ok_or(Error::Invalid)?;
        Ok(Navigation::Select {
            tab,
            entry: t.history.get(at).ok_or(Error::Invalid)?.id(),
        })
    }
    pub fn phase(&self, id: OperationId) -> Option<Phase> {
        self.pending
            .iter()
            .find(|p| p.id == id)
            .map(|p| p.phase)
            .or_else(|| {
                self.outcomes
                    .iter()
                    .find(|(i, _)| *i == id)
                    .map(|(_, p)| *p)
            })
            .or_else(|| {
                self.sender
                    .0
                    .queue
                    .borrow()
                    .iter()
                    .find(|(i, _)| *i == id)
                    .map(|_| Phase::Queued)
            })
    }
    pub fn decision(&self, id: OperationId) -> Option<DecisionId> {
        self.pending
            .iter()
            .find(|p| p.id == id && matches!(p.phase, Phase::AwaitingDecision | Phase::Failed))
            .map(|p| DecisionId {
                operation: id,
                round: p.round,
            })
    }
    pub fn guard(&self, id: OperationId) -> Option<&G> {
        self.pending
            .iter()
            .find(|p| p.id == id)
            .and_then(|p| p.guard.as_ref())
    }
    pub fn guard_mut(&mut self, id: OperationId) -> Option<&mut G> {
        self.pending
            .iter_mut()
            .find(|p| p.id == id)
            .and_then(|p| p.guard.as_mut())
    }
    fn finish(&mut self, id: OperationId, phase: Phase) {
        self.sender.0.charged.set(self.sender.0.charged.get() - 1);
        self.sender
            .0
            .coalesced
            .borrow_mut()
            .retain(|(i, _)| *i != id);
        if self.outcomes.len() == self.limits.outcomes {
            self.outcomes.pop_front();
        }
        self.outcomes.push_back((id, phase));
    }
    pub fn choose<H: TransitionHost<D, Guard = G>>(
        &mut self,
        host: &mut H,
        decision: DecisionId,
        choice: Choice,
    ) -> Result<(), Error> {
        let p = self
            .pending
            .iter_mut()
            .find(|p| p.id == decision.operation)
            .ok_or(Error::Stale)?;
        if p.round != decision.round || !matches!(p.phase, Phase::AwaitingDecision | Phase::Failed)
        {
            return Err(Error::Stale);
        }
        let next = p.round.checked_add(1).ok_or(Error::Exhausted)?;
        if choice == Choice::Cancel {
            p.phase = Phase::Cancelled;
            p.guard = None;
        } else {
            p.phase = host.choose(p.guard.as_mut().ok_or(Error::Invalid)?, decision, choice)?;
        }
        p.round = next;
        Ok(())
    }
    /// Cancellation is explicit and wins in every precommit phase. Observers carry no ownership.
    pub fn cancel(&mut self, id: OperationId) -> Result<(), Error> {
        if let Some(p) = self.pending.iter_mut().find(|p| p.id == id) {
            p.phase = Phase::Cancelled;
            p.guard = None;
            return Ok(());
        }
        let mut q = self.sender.0.queue.borrow_mut();
        if let Some(at) = q.iter().position(|(i, _)| *i == id) {
            let removed = q.remove(at);
            drop(q);
            drop(removed);
            self.finish(id, Phase::Cancelled);
            return Ok(());
        }
        if self.phase(id) == Some(Phase::Committed) {
            Err(Error::Invalid)
        } else {
            Err(Error::Stale)
        }
    }
    fn incoming_entry<'a>(&'a self, c: &'a Navigation<R, D>) -> Option<&'a RetainedEntry<R, D>> {
        if let Navigation::Select { tab, entry } = c {
            self.tab(*tab)
                .ok()?
                .history
                .iter()
                .find(|e| e.id() == *entry)
        } else {
            c.new_entry()
        }
    }
    fn resolve(&self, c: &Navigation<R, D>) -> Result<(Vec<ScopeId>, Vec<ScopeId>), Error> {
        let mut affected = vec![];
        let mut tabs = vec![];
        match c {
            Navigation::Select { tab, entry } => {
                let t = self.tab(*tab)?;
                if !t
                    .history
                    .iter()
                    .any(|e| e.id() == *entry && e.owner.liveness().is_live())
                {
                    return Err(Error::Stale);
                }
                tabs.push(*tab);
            }
            Navigation::Push { tab, after, .. } => {
                let t = self.tab(*tab)?;
                let at = match after {
                    Some(id) => {
                        t.history
                            .iter()
                            .position(|e| e.id() == *id)
                            .ok_or(Error::Stale)?
                            + 1
                    }
                    None => {
                        if !t.history.is_empty() {
                            return Err(Error::Stale);
                        }
                        0
                    }
                };
                affected.extend(t.history[at..].iter().map(|e| e.id()));
                tabs.push(*tab);
            }
            Navigation::Replace { tab, old, .. } => {
                let t = self.tab(*tab)?;
                if !t.history.iter().any(|e| e.id() == *old) {
                    return Err(Error::Stale);
                }
                affected.push(*old);
                tabs.push(*tab);
            }
            Navigation::Close { entries } => {
                if entries.is_empty() {
                    return Err(Error::Invalid);
                }
                for id in entries {
                    if affected.contains(id) {
                        return Err(Error::Invalid);
                    }
                    let t = self
                        .tabs
                        .iter()
                        .find(|t| {
                            t.owner.liveness().is_live() && t.history.iter().any(|e| e.id() == *id)
                        })
                        .ok_or(Error::Stale)?;
                    affected.push(*id);
                    if !tabs.contains(&t.owner.id()) {
                        tabs.push(t.owner.id());
                    }
                }
            }
        }
        if let Some(e) = c.new_entry() {
            if !e.owner.liveness().is_live()
                || !tabs.iter().any(|id| {
                    self.tab(*id).is_ok_and(|t| {
                        e.owner
                            .life
                            .parent
                            .as_ref()
                            .is_some_and(|p| std::sync::Arc::ptr_eq(p, &t.owner.life))
                    })
                })
                || self
                    .tabs
                    .iter()
                    .any(|t| t.history.iter().any(|old| old.id() == e.id()))
            {
                return Err(Error::Invalid);
            }
            let total: usize = self.tabs.iter().map(|t| t.history.len()).sum();
            if total + 1 - affected.len() > self.limits.entries {
                return Err(Error::Capacity);
            }
        }
        Ok((affected, tabs))
    }
    /// One bounded scheduling pass. Reentrant submissions wait for the next pass;
    /// conflicting tabs serialize while unrelated entries can progress. No host callback
    /// or await between final all-target validation and metadata/owner removal.
    pub fn tick<H: TransitionHost<D, Guard = G>>(&mut self, host: &mut H) {
        while self.pending.len() < self.limits.operations {
            let Some((id, command)) = self.sender.0.queue.borrow_mut().pop_front() else {
                break;
            };
            match self.resolve(&command) {
                Ok((affected, tabs)) => self.pending.push(Pending {
                    id,
                    command,
                    affected,
                    tabs,
                    phase: Phase::Queued,
                    round: 1,
                    guard: None,
                }),
                Err(_) => self.finish(id, Phase::Failed),
            }
        }
        for at in 0..self.pending.len() {
            if self.pending[at].phase == Phase::Cancelled {
                continue;
            }
            if self.pending[at].phase == Phase::Queued {
                if self.pending[..at].iter().any(|p| {
                    !matches!(p.phase, Phase::Cancelled | Phase::Committed)
                        && p.tabs.iter().any(|t| self.pending[at].tabs.contains(t))
                }) {
                    continue;
                }
                // Revalidate exact IDs at start, including forward truncation set.
                let valid = self.resolve(&self.pending[at].command).is_ok_and(|(a, t)| {
                    a == self.pending[at].affected && t == self.pending[at].tabs
                });
                if !valid {
                    self.pending[at].phase = Phase::Cancelled;
                    continue;
                }
                let targets: Vec<_> = self
                    .tabs
                    .iter()
                    .flat_map(|t| t.history.iter())
                    .filter(|e| self.pending[at].affected.contains(&e.id()))
                    .map(|e| (e.id(), e.document.clone()))
                    .collect();
                if self
                    .incoming_entry(&self.pending[at].command)
                    .is_some_and(|e| host.validate_entry(e.id(), &e.document, &e.lease).is_err())
                {
                    self.pending[at].phase = Phase::Failed;
                    continue;
                }
                match host.protect(self.pending[at].id, &targets) {
                    Ok(g) => {
                        self.pending[at].phase = if host.dirty(&g) {
                            Phase::AwaitingDecision
                        } else {
                            Phase::Ready
                        };
                        self.pending[at].guard = Some(g);
                    }
                    Err(Error::Pending) => {
                        continue;
                    }
                    Err(_) => {
                        self.pending[at].phase = Phase::Failed;
                        continue;
                    }
                }
            }
            let p = &mut self.pending[at];
            if p.phase == Phase::Saving {
                p.phase = host.poll(p.guard.as_mut().expect("saving owns protection"));
            }
            if p.phase == Phase::Ready {
                if host
                    .validate(p.guard.as_ref().expect("ready owns protection"))
                    .is_err()
                {
                    p.phase = Phase::Failed;
                    continue;
                }
                // Incoming owners can be lost while the outgoing documents wait
                // for a decision/save, including a different-document replacement.
                // Re-resolve every identity and the exact truncation set, and check
                // the incoming native authority/lease at this final boundary too.
                let p = &self.pending[at];
                let valid = self
                    .resolve(&p.command)
                    .is_ok_and(|(a, t)| a == p.affected && t == p.tabs)
                    && self
                        .incoming_entry(&p.command)
                        .is_none_or(|e| host.validate_entry(e.id(), &e.document, &e.lease).is_ok());
                if !valid {
                    self.pending[at].phase = Phase::Failed;
                    continue;
                }
                // No callbacks between final validation and metadata mutation.
                let p = self.pending.remove(at);
                self.commit(p);
                return; // removal shifts indexes; next pass resumes
            }
        }
        let mut at = 0;
        while at < self.pending.len() {
            if self.pending[at].phase == Phase::Cancelled {
                let p = self.pending.remove(at);
                self.finish(p.id, Phase::Cancelled);
            } else {
                at += 1;
            }
        }
    }
    fn commit(&mut self, p: Pending<R, D, G>) {
        let replacement_at = if let Navigation::Replace { tab, old, .. } = &p.command {
            self.tab(*tab)
                .ok()
                .and_then(|t| t.history.iter().position(|e| e.id() == *old))
        } else {
            None
        };
        let Pending {
            id,
            command,
            affected,
            guard,
            ..
        } = p;
        // All owners move out of authoritative metadata before destruction.
        let mut retired = vec![];
        for t in &mut self.tabs {
            let mut i = 0;
            while i < t.history.len() {
                if affected.contains(&t.history[i].id()) {
                    retired.push(t.history.remove(i));
                } else {
                    i += 1;
                }
            }
            if t.selected.is_some_and(|id| affected.contains(&id)) {
                t.selected = t.history.last().map(|e| e.id());
            }
        }
        match command {
            Navigation::Push { tab, entry, .. } => {
                let t = self.tabs.iter_mut().find(|t| t.owner.id() == tab).unwrap();
                t.selected = Some(entry.id());
                t.history.push(entry);
            }
            Navigation::Replace { tab, entry, .. } => {
                let t = self.tabs.iter_mut().find(|t| t.owner.id() == tab).unwrap();
                t.selected = Some(entry.id());
                t.history.insert(replacement_at.unwrap(), entry);
            }
            Navigation::Select { tab, entry } => {
                self.tabs
                    .iter_mut()
                    .find(|t| t.owner.id() == tab)
                    .unwrap()
                    .selected = Some(entry)
            }
            Navigation::Close { .. } => {}
        }
        self.finish(id, Phase::Committed);
        drop(retired);
        drop(guard);
    }
}
