//! Concrete native source/structured writer protection and CloseGuard composition.
//! Host calls `sync_native` before returning to the runner. Save completion runs
//! independently of retained navigation scheduling. No executor or GUI certification.
use crate::*;
use rndr_core::editing::{self, SourceDocuments};
use rndr_core::structured_native::{StructuredDocuments, StructuredHandle};
use rndr_core::{CloseGuard, CloseRequest, CloseState, DocumentFreeze, Input, World};
use std::{
    cell::Cell,
    panic::{catch_unwind, AssertUnwindSafe},
    path::Path,
    rc::Rc,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeDocument {
    Source(editing::DocumentId),
    Structured(StructuredHandle),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProtectedTarget {
    pub document: DocumentId,
    pub manager: WorldId,
    pub native: NativeDocument,
}
/// Exact registered World. Constructor checks the retained WorldOwner resource.
pub struct HostWorld<'a> {
    pub(crate) id: WorldId,
    pub(crate) world: &'a mut World,
}
impl<'a> HostWorld<'a> {
    pub fn new(id: WorldId, world: &'a mut World) -> Result<Self, Error> {
        if world.local::<WorldOwner>().is_none_or(|o| o.id() != id) {
            return Err(Error::Invalid);
        }
        Ok(Self { id, world })
    }
}
struct Record {
    target: ProtectedTarget,
    saved: Rc<Cell<Option<u64>>>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SaveTicket {
    operation: OperationId,
    round: u64,
    target: ProtectedTarget,
    revision: u64,
}
impl SaveTicket {
    pub fn target(self) -> ProtectedTarget {
        self.target
    }
    pub fn revision(self) -> u64 {
        self.revision
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SaveFailure {
    Failed,
    Panicked,
    Lost,
}
/// Owned save work for an existing host executor/thread. Actual loss is technical
/// failure, never a choice; dropping an observer SaveTicket has no effect.
pub struct SaveWork {
    ticket: SaveTicket,
    lost: std::sync::Arc<std::sync::atomic::AtomicBool>,
    armed: bool,
}
pub struct SavedWork {
    ticket: SaveTicket,
    lost: std::sync::Arc<std::sync::atomic::AtomicBool>,
    result: Result<(), SaveFailure>,
    armed: bool,
}
impl Drop for SaveWork {
    fn drop(&mut self) {
        if self.armed {
            self.lost.store(true, Ordering::Release);
        }
    }
}
impl Drop for SavedWork {
    fn drop(&mut self) {
        if self.armed {
            self.lost.store(true, Ordering::Release);
        }
    }
}
impl SaveWork {
    pub fn run<F>(mut self, save: F) -> SavedWork
    where
        F: FnOnce() -> Result<(), SaveFailure>,
    {
        let result = catch_unwind(AssertUnwindSafe(save)).unwrap_or(Err(SaveFailure::Panicked));
        self.armed = false;
        SavedWork {
            ticket: self.ticket,
            lost: self.lost.clone(),
            result,
            armed: true,
        }
    }
}
enum Protection {
    Choice,
    Waiting(SaveTicket),
    Safe,
    Failed,
}
struct FrozenDocument {
    target: ProtectedTarget,
    revision: u64,
    saved: Rc<Cell<Option<u64>>>,
    _writer: DocumentFreeze,
    leases: TopologyFreeze,
    state: Protection,
    lost: std::sync::Arc<std::sync::atomic::AtomicBool>,
    claimed: bool,
}
/// Owned freeze and exact revisions; no document bytes/history are stored here.
#[must_use]
pub struct FrozenDocuments {
    operation: OperationId,
    docs: Vec<FrozenDocument>,
    worlds: Option<TopologyFreeze>,
    native_world: Option<WorldId>,
}
impl FrozenDocuments {
    /// Mint at most one actual job per save revision/decision. Keep with the exact
    /// existing producer; dropping either job or undelivered result fails closed.
    pub fn own_save(&mut self, ticket: SaveTicket) -> Result<SaveWork, Error> {
        let d = self
            .docs
            .iter_mut()
            .find(|d| matches!(d.state,Protection::Waiting(t)if t==ticket) && !d.claimed)
            .ok_or(Error::Stale)?;
        d.claimed = true;
        Ok(SaveWork {
            ticket,
            lost: d.lost.clone(),
            armed: true,
        })
    }
    pub fn saves(&self) -> Vec<SaveTicket> {
        self.docs
            .iter()
            .filter_map(|d| {
                if let Protection::Waiting(t) = d.state {
                    Some(t)
                } else {
                    None
                }
            })
            .collect()
    }
}
struct NativeAttempt {
    world: WorldId,
    request: CloseRequest,
    operation: OperationId,
    round: u64,
    guard: FrozenDocuments,
    phase: Phase,
}
/// Bounded native document metadata only. Register each authoritative document
/// once; shared/hidden presentations use Coordinator presentation leases.
pub struct ProtectionHost {
    limit: usize,
    records: Vec<Record>,
    native: Vec<NativeAttempt>,
    authority: u64,
    serial: u64,
}
impl ProtectionHost {
    pub fn new(limit: usize) -> Result<Self, Error> {
        if limit == 0 {
            return Err(Error::Invalid);
        }
        Ok(Self {
            limit,
            records: vec![],
            native: vec![],
            authority: allocate_authority(&crate::AUTHORITIES)?,
            serial: 0,
        })
    }
    pub fn register(
        &mut self,
        target: ProtectedTarget,
        saved_revision: Option<u64>,
    ) -> Result<(), Error> {
        if self.records.len() >= self.limit {
            return Err(Error::Capacity);
        }
        if self
            .records
            .iter()
            .any(|r| r.target.document == target.document || r.target.native == target.native)
        {
            return Err(Error::Invalid);
        }
        self.records.push(Record {
            target,
            saved: Rc::new(Cell::new(saved_revision)),
        });
        Ok(())
    }
    /// Reclaim only after the native document owner has explicitly closed it.
    pub fn unregister(&mut self, flow: &Coordinator, target: ProtectedTarget) -> Result<(), Error> {
        if flow.live_scope(target.document.0).is_ok()
            || self
                .native
                .iter()
                .any(|n| n.guard.docs.iter().any(|d| d.target == target))
        {
            return Err(Error::InUse);
        }
        let at = self
            .records
            .iter()
            .position(|r| r.target == target)
            .ok_or(Error::Stale)?;
        self.records.remove(at);
        Ok(())
    }
    pub fn pass<'a, 'b>(
        &'a mut self,
        flow: &'a Coordinator,
        worlds: &'a mut [HostWorld<'b>],
    ) -> ProtectionPass<'a, 'b> {
        ProtectionPass {
            host: self,
            flow,
            worlds,
            native: false,
        }
    }
    /// Native decisions are bound to the exact request's operation and round.
    pub fn native_decision(&self, world: WorldId) -> Option<DecisionId> {
        self.native
            .iter()
            .find(|n| {
                n.world == world && matches!(n.phase, Phase::AwaitingDecision | Phase::Failed)
            })
            .map(|n| DecisionId {
                operation: n.operation,
                round: n.round,
            })
    }
    pub fn native_saves(&self) -> Vec<SaveTicket> {
        self.native.iter().flat_map(|n| n.guard.saves()).collect()
    }
    pub fn choose_native(
        &mut self,
        flow: &Coordinator,
        worlds: &mut [HostWorld<'_>],
        decision: DecisionId,
        choice: Choice,
    ) -> Result<(), Error> {
        let at = self
            .native
            .iter()
            .position(|n| {
                n.operation == decision.operation
                    && n.round == decision.round
                    && matches!(n.phase, Phase::AwaitingDecision | Phase::Failed)
            })
            .ok_or(Error::Stale)?;
        let round = self.native[at]
            .round
            .checked_add(1)
            .ok_or(Error::Exhausted)?;
        let world = self.native[at].world;
        if choice == Choice::Cancel {
            get_world_mut(worlds, world)?
                .resource_mut::<CloseGuard>()
                .ok_or(Error::Invalid)?
                .cancel();
            self.native.remove(at);
            return Ok(());
        }
        let mut attempt = self.native.remove(at);
        if choice == Choice::Save {
            let g = get_world_mut(worlds, world)?
                .resource_mut::<CloseGuard>()
                .ok_or(Error::Invalid)?;
            if let Some(request) = g.retry(attempt.request) {
                attempt.request = request;
            }
        }
        let phase = self
            .pass(flow, worlds)
            .choose(&mut attempt.guard, decision, choice);
        match phase {
            Ok(p) => {
                attempt.phase = p;
                attempt.round = round;
                self.native.insert(at, attempt);
                Ok(())
            }
            Err(e) => {
                self.native.insert(at, attempt);
                Err(e)
            }
        }
    }
    /// Run host-defined persistence under an owned panic boundary, then acknowledge
    /// only the exact current saved revision. A dropped/lost job never chooses discard.
    /// This synchronous lane does not await the quit/navigation operation it protects.
    pub fn save_native<F>(
        &mut self,
        flow: &Coordinator,
        worlds: &mut [HostWorld<'_>],
        ticket: SaveTicket,
        save: F,
    ) -> Result<(), Error>
    where
        F: FnOnce(&World, NativeDocument) -> Result<(), SaveFailure>,
    {
        let at = self
            .native
            .iter()
            .position(|n| n.operation == ticket.operation)
            .ok_or(Error::Stale)?;
        let mut attempt = self.native.remove(at);
        let result = self
            .pass(flow, worlds)
            .save(&mut attempt.guard, ticket, save);
        self.native.insert(at, attempt);
        result
    }
    pub fn own_native_save(&mut self, ticket: SaveTicket) -> Result<SaveWork, Error> {
        self.native
            .iter_mut()
            .find(|n| n.operation == ticket.operation)
            .ok_or(Error::Stale)?
            .guard
            .own_save(ticket)
    }
    pub fn complete_native_owned_save(
        &mut self,
        flow: &Coordinator,
        worlds: &mut [HostWorld<'_>],
        result: SavedWork,
    ) -> Result<(), Error> {
        let at = self
            .native
            .iter()
            .position(|n| n.operation == result.ticket.operation)
            .ok_or(Error::Stale)?;
        let mut attempt = self.native.remove(at);
        let accepted = self
            .pass(flow, worlds)
            .complete_owned_save(&mut attempt.guard, result);
        self.native.insert(at, attempt);
        accepted
    }
    pub fn fail_native_save(
        &mut self,
        ticket: SaveTicket,
        failure: SaveFailure,
    ) -> Result<(), Error> {
        let n = self
            .native
            .iter_mut()
            .find(|n| n.operation == ticket.operation)
            .ok_or(Error::Stale)?;
        fail_save(&mut n.guard, ticket, failure)
    }
    /// Install CloseGuard at window creation, not after close starts. Keep calling
    /// while Pending/Failed/Ready. Dynamic Worlds refuse at Coordinator::register_world
    /// while any native protection owns the participant freeze. Missing/foreign Worlds
    /// cannot complete protection. No entry is disposed on a World's Ready.
    pub fn sync_native(
        &mut self,
        flow: &Coordinator,
        worlds: &mut [HostWorld<'_>],
    ) -> Result<(), Error> {
        let expected: Vec<_> = flow
            .scopes
            .iter()
            .filter(|(_, s)| s.role == Role::World && s.life.live())
            .map(|(id, _)| WorldId(*id))
            .collect();
        let missing_native = worlds.iter().any(|w| {
            let sources = w.world.resource::<SourceDocuments>().is_some_and(|docs| {
                docs.ids().any(|id| {
                    !self.records.iter().any(|r| {
                        r.target.manager == w.id && r.target.native == NativeDocument::Source(id)
                    })
                })
            });
            let structured = w
                .world
                .resource::<StructuredDocuments>()
                .is_some_and(|docs| {
                    docs.handles().any(|id| {
                        !self.records.iter().any(|r| {
                            r.target.manager == w.id
                                && r.target.native == NativeDocument::Structured(id)
                        })
                    })
                });
            sources || structured
        });
        if missing_native
            || flow.leases.values().any(|l| {
                flow.held(l) && !self.records.iter().any(|r| r.target.document == l.document)
            })
        {
            invalidate_ready(worlds);
            return Err(Error::Pending);
        }
        if expected.len() != worlds.len()
            || expected
                .iter()
                .any(|id| worlds.iter().filter(|w| w.id == *id).count() != 1)
        {
            invalidate_ready(worlds);
            return Err(Error::Pending);
        }
        let mut at = 0;
        while at < self.native.len() {
            let n = &self.native[at];
            let state = get_world(worlds, n.world)?
                .resource::<CloseGuard>()
                .ok_or(Error::Invalid)?
                .state()
                .clone();
            if matches!(state, CloseState::Idle) {
                self.native.remove(at);
                continue;
            }
            let request = match state {
                CloseState::Pending(r) | CloseState::Failed(r, _) | CloseState::Ready(r) => r,
                CloseState::Idle => unreachable!(),
            };
            if request != n.request {
                // Native retry/invalidation keeps the existing freeze; stale save
                // tickets and displayed decisions cannot protect the new request.
                let n = &mut self.native[at];
                n.round = n.round.checked_add(1).ok_or(Error::Exhausted)?;
                n.request = request;
                for d in &mut n.guard.docs {
                    d.state = if d.saved.get() == Some(d.revision) {
                        Protection::Safe
                    } else {
                        Protection::Choice
                    };
                }
                n.phase = Phase::AwaitingDecision;
            }
            at += 1;
        }
        for id in expected {
            let state = get_world(worlds, id)?
                .resource::<CloseGuard>()
                .ok_or(Error::Invalid)?
                .state()
                .clone();
            let CloseState::Pending(request) = state else {
                continue;
            };
            if self.native.iter().any(|n| n.world == id) {
                continue;
            }
            if self.native.len() >= self.limit {
                return Err(Error::Capacity);
            }
            let serial = self.serial.checked_add(1).ok_or(Error::Exhausted)?;
            let operation = OperationId {
                authority: self.authority,
                serial,
            };
            let targets: Vec<_> = self
                .records
                .iter()
                .filter(|r| {
                    r.target.manager == id
                        || flow.leases.values().any(|l| {
                            l.document == r.target.document
                                && flow.held(l)
                                && scope_in_world(flow, l.scope, id)
                        })
                })
                .map(|r| (r.target.manager.0, r.target))
                .collect();
            let mut pass = self.pass(flow, worlds);
            pass.native = true;
            let mut guard = pass.protect(operation, &targets)?;
            guard.worlds = Some(flow.freeze_worlds()?);
            guard.native_world = Some(id);
            self.serial = serial;
            let phase = if guard
                .docs
                .iter()
                .any(|d| !matches!(d.state, Protection::Safe))
            {
                Phase::AwaitingDecision
            } else {
                Phase::Ready
            };
            self.native.push(NativeAttempt {
                world: id,
                request,
                operation,
                round: 1,
                guard,
                phase,
            });
        }
        let mut attempts = std::mem::take(&mut self.native);
        for n in &mut attempts {
            let mut pass = self.pass(flow, worlds);
            if n.phase == Phase::Saving {
                n.phase = pass.poll(&mut n.guard);
            }
            if pass.validate(&n.guard).is_err() {
                n.phase = Phase::Failed;
                let g = get_world_mut(worlds, n.world)?
                    .resource_mut::<CloseGuard>()
                    .ok_or(Error::Invalid)?;
                if matches!(g.state(), CloseState::Ready(_)) {
                    g.invalidate();
                    if let CloseState::Pending(r) = g.state() {
                        n.request = *r;
                    }
                }
            }
        }
        // First validate every owned attempt, then publish native readiness.
        // Destroying a manager World destroys its actual native document store:
        // another retained presentation (or application lease) must not outlive it.
        // Ready phases form the bounded jointly-prepared teardown cohort; no
        // second document store or implicit ownership transfer is introduced.
        let prepared: Vec<_> = attempts
            .iter()
            .filter(|n| n.phase == Phase::Ready)
            .map(|n| n.world)
            .collect();
        for n in &mut attempts {
            let manager_in_use = n
                .guard
                .docs
                .iter()
                .filter(|d| d.target.manager == n.world)
                .any(|d| {
                    flow.leases.values().any(|l| {
                        l.document == d.target.document
                            && flow.held(l)
                            && !scope_in_world(flow, l.scope, n.world)
                            && !prepared
                                .iter()
                                .any(|world| scope_in_world(flow, l.scope, *world))
                    })
                });
            let g = get_world_mut(worlds, n.world)?
                .resource_mut::<CloseGuard>()
                .ok_or(Error::Invalid)?;
            if n.phase == Phase::Ready && manager_in_use {
                // Cancellation/withdrawal of a participant revokes a previous
                // Ready before the runner can consume it. Keep the freeze; the
                // fresh request is reconciled at the next pass.
                if matches!(g.state(), CloseState::Ready(_)) {
                    g.invalidate();
                }
                continue;
            }
            match n.phase {
                Phase::Ready => {
                    if matches!(g.state(),CloseState::Failed(r,_)if *r==n.request) {
                        g.discard(n.request);
                    } else {
                        g.complete(n.request, Ok(()));
                    }
                }
                Phase::Failed => {
                    g.complete(
                        n.request,
                        Err("flow protection failed; retry, cancel or informed discard".into()),
                    );
                }
                _ => {}
            }
        }
        self.native = attempts;
        Ok(())
    }
    /// Only terminal WorldOwner loss releases native Ready protections. The runner,
    /// not flow, commits native destruction after *all* participating guards are Ready.
    pub fn reconcile_world_loss(&mut self, flow: &Coordinator) {
        self.native.retain(|n| flow.world_is_live(n.world));
    }
}
fn invalidate_ready(worlds: &mut [HostWorld<'_>]) {
    for w in worlds {
        if let Some(g) = w.world.resource_mut::<CloseGuard>() {
            if matches!(g.state(), CloseState::Ready(_)) {
                g.invalidate();
            }
        }
    }
}
fn scope_in_world(flow: &Coordinator, mut scope: ScopeId, world: WorldId) -> bool {
    loop {
        if scope == world.0 {
            return true;
        }
        let Some(parent) = flow.scopes.get(&scope).and_then(|s| s.parent) else {
            return false;
        };
        scope = parent;
    }
}
fn get_world<'a>(worlds: &'a [HostWorld<'_>], id: WorldId) -> Result<&'a World, Error> {
    worlds
        .iter()
        .find(|w| w.id == id)
        .map(|w| &*w.world)
        .ok_or(Error::Pending)
}
fn get_world_mut<'a, 'b>(
    worlds: &'a mut [HostWorld<'b>],
    id: WorldId,
) -> Result<&'a mut World, Error> {
    worlds
        .iter_mut()
        .find(|w| w.id == id)
        .map(|w| &mut *w.world)
        .ok_or(Error::Pending)
}
fn revision(world: &World, native: NativeDocument) -> Result<u64, Error> {
    match native {
        NativeDocument::Source(id) => world
            .resource::<SourceDocuments>()
            .and_then(|d| d.get(id))
            .map(|d| d.revision().0),
        NativeDocument::Structured(id) => world
            .resource::<StructuredDocuments>()
            .and_then(|d| d.get(id))
            .map(|d| d.revision().0),
    }
    .ok_or(Error::Stale)
}
fn freeze(world: &World, native: NativeDocument) -> Result<DocumentFreeze, Error> {
    // Earliest Input has not necessarily reached Chrome yet. Conservatively block
    // admission for pending IME in the manager World; never clear/drop preedit.
    if world
        .resource::<Input>()
        .is_some_and(|i| !i.ime_preedit.is_empty() || !i.ime_commit.is_empty())
    {
        return Err(Error::Pending);
    }
    match native {
        NativeDocument::Source(id) => world
            .resource::<SourceDocuments>()
            .and_then(|d| d.get(id))
            .ok_or(Error::Stale)?
            .freeze()
            .map_err(|_| Error::Pending),
        NativeDocument::Structured(id) => world
            .resource::<StructuredDocuments>()
            .and_then(|d| d.get(id))
            .ok_or(Error::Stale)?
            .freeze()
            .map_err(|_| Error::Pending),
    }
}
fn fail_save(
    guard: &mut FrozenDocuments,
    ticket: SaveTicket,
    _failure: SaveFailure,
) -> Result<(), Error> {
    let d = guard
        .docs
        .iter_mut()
        .find(|d| matches!(d.state,Protection::Waiting(t)if t==ticket))
        .ok_or(Error::Stale)?;
    d.state = Protection::Failed;
    Ok(())
}
pub struct ProtectionPass<'a, 'b> {
    host: &'a mut ProtectionHost,
    flow: &'a Coordinator,
    worlds: &'a mut [HostWorld<'b>],
    native: bool,
}
impl ProtectionPass<'_, '_> {
    pub fn complete_owned_save(
        &mut self,
        guard: &mut FrozenDocuments,
        mut result: SavedWork,
    ) -> Result<(), Error> {
        let status = result.result;
        let accepted = self.save_inner(guard, result.ticket, |_, _| status, true);
        if accepted.is_ok() {
            result.armed = false;
        }
        accepted
    }
    pub fn save<F>(
        &mut self,
        guard: &mut FrozenDocuments,
        ticket: SaveTicket,
        save: F,
    ) -> Result<(), Error>
    where
        F: FnOnce(&World, NativeDocument) -> Result<(), SaveFailure>,
    {
        self.save_inner(guard, ticket, save, false)
    }
    fn save_inner<F>(
        &mut self,
        guard: &mut FrozenDocuments,
        ticket: SaveTicket,
        save: F,
        owned: bool,
    ) -> Result<(), Error>
    where
        F: FnOnce(&World, NativeDocument) -> Result<(), SaveFailure>,
    {
        let d = guard
            .docs
            .iter_mut()
            .find(|d| {
                matches!(d.state,Protection::Waiting(t)if t==ticket)
                    && d.claimed == owned
                    && !d.lost.load(Ordering::Acquire)
            })
            .ok_or(Error::Stale)?;
        if !d.leases.valid()
            || revision(get_world(self.worlds, d.target.manager)?, d.target.native)?
                != ticket.revision
        {
            return Err(Error::Stale);
        }
        let world = get_world(self.worlds, ticket.target.manager)?;
        let result = catch_unwind(AssertUnwindSafe(|| save(world, ticket.target.native)))
            .unwrap_or(Err(SaveFailure::Panicked));
        match result {
            Ok(()) => {
                d.saved.set(Some(ticket.revision));
                d.state = Protection::Safe;
                Ok(())
            }
            Err(_) => {
                d.state = Protection::Failed;
                Ok(())
            }
        }
    }
    pub fn fail_save(
        &mut self,
        guard: &mut FrozenDocuments,
        ticket: SaveTicket,
        failure: SaveFailure,
    ) -> Result<(), Error> {
        fail_save(guard, ticket, failure)
    }
    /// Actual source durability lane: exact frozen bytes, write_all + sync_all.
    /// The host supplies its authorized destination. Does not claim transactionality
    /// of external effects or rollback of completed saves after cancellation.
    pub fn save_source(
        &mut self,
        guard: &mut FrozenDocuments,
        ticket: SaveTicket,
        path: &Path,
    ) -> Result<(), Error> {
        self.save(guard, ticket, |world, native| {
            use std::io::Write;
            let NativeDocument::Source(id) = native else {
                return Err(SaveFailure::Failed);
            };
            let doc = world
                .resource::<SourceDocuments>()
                .and_then(|d| d.get(id))
                .ok_or(SaveFailure::Failed)?;
            let mut file = std::fs::File::create(path).map_err(|_| SaveFailure::Failed)?;
            file.write_all(doc.text().as_bytes())
                .and_then(|_| file.sync_all())
                .map_err(|_| SaveFailure::Failed)
        })
    }
}
impl TransitionHost<ProtectedTarget> for ProtectionPass<'_, '_> {
    type Guard = FrozenDocuments;
    fn protect(
        &mut self,
        operation: OperationId,
        targets: &[(ScopeId, ProtectedTarget)],
    ) -> Result<FrozenDocuments, Error> {
        if !self.native
            && self.host.native.iter().any(|n| {
                n.guard
                    .docs
                    .iter()
                    .any(|d| targets.iter().any(|(_, t)| *t == d.target))
            })
        {
            return Err(Error::Pending);
        }
        let mut docs: Vec<FrozenDocument> = vec![];
        for (scope, target) in targets {
            // A shared document can have pending earliest-Input composition in
            // a presentation World other than its authoritative manager World.
            // Require every such participant before admitting the snapshot.
            for (id, s) in &self.flow.scopes {
                let world = WorldId(*id);
                if s.role == Role::World
                    && s.life.live()
                    && self.flow.leases.values().any(|l| {
                        l.document == target.document
                            && self.flow.held(l)
                            && scope_in_world(self.flow, l.scope, world)
                    })
                {
                    let participant = get_world(self.worlds, world)?;
                    if participant
                        .resource::<Input>()
                        .is_some_and(|i| !i.ime_preedit.is_empty() || !i.ime_commit.is_empty())
                    {
                        return Err(Error::Pending);
                    }
                }
            }
            if *scope != target.manager.0
                && !self.flow.leases.values().any(|l| {
                    l.scope == *scope && l.document == target.document && self.flow.held(l)
                })
            {
                return Err(Error::Invalid);
            }
            if docs.iter().any(|d| d.target == *target) {
                continue;
            }
            let record = self
                .host
                .records
                .iter()
                .find(|r| r.target == *target)
                .ok_or(Error::Invalid)?;
            let leases = self.flow.freeze_document_leases(target.document)?;
            let world = get_world(self.worlds, target.manager)?;
            let writer = freeze(world, target.native)?;
            let revision = revision(world, target.native)?;
            let state = if record.saved.get() == Some(revision) {
                Protection::Safe
            } else {
                Protection::Choice
            };
            docs.push(FrozenDocument {
                target: *target,
                revision,
                saved: record.saved.clone(),
                _writer: writer,
                leases,
                state,
                lost: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
                claimed: false,
            });
        }
        Ok(FrozenDocuments {
            operation,
            docs,
            worlds: None,
            native_world: None,
        })
    }
    fn dirty(&self, g: &FrozenDocuments) -> bool {
        g.docs.iter().any(|d| !matches!(d.state, Protection::Safe))
    }
    fn choose(
        &mut self,
        g: &mut FrozenDocuments,
        decision: DecisionId,
        choice: Choice,
    ) -> Result<Phase, Error> {
        if decision.operation != g.operation {
            return Err(Error::Stale);
        }
        self.validate(g)?;
        for d in &mut g.docs {
            if !matches!(d.state, Protection::Safe) {
                d.lost = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
                d.claimed = false;
                d.state = match choice {
                    Choice::Discard => Protection::Safe,
                    Choice::Save => Protection::Waiting(SaveTicket {
                        operation: g.operation,
                        round: decision.round,
                        target: d.target,
                        revision: d.revision,
                    }),
                    Choice::Cancel => return Err(Error::Invalid),
                };
            }
        }
        Ok(if choice == Choice::Save {
            Phase::Saving
        } else {
            Phase::Ready
        })
    }
    fn poll(&mut self, g: &mut FrozenDocuments) -> Phase {
        for d in &mut g.docs {
            if matches!(d.state, Protection::Waiting(_)) && d.lost.load(Ordering::Acquire) {
                d.state = Protection::Failed;
            }
        }
        if g.docs.iter().any(|d| matches!(d.state, Protection::Failed)) {
            Phase::Failed
        } else if g.docs.iter().all(|d| matches!(d.state, Protection::Safe)) {
            Phase::Ready
        } else {
            Phase::Saving
        }
    }
    fn validate(&self, g: &FrozenDocuments) -> Result<(), Error> {
        if let Some(world) = g.native_world {
            if self.host.records.iter().any(|r| {
                (r.target.manager == world
                    || self.flow.leases.values().any(|l| {
                        l.document == r.target.document
                            && self.flow.held(l)
                            && scope_in_world(self.flow, l.scope, world)
                    }))
                    && !g.docs.iter().any(|d| d.target == r.target)
            }) {
                return Err(Error::Stale);
            }
        }
        if g.worlds.as_ref().is_some_and(|w| !w.valid()) {
            return Err(Error::Stale);
        }
        for d in &g.docs {
            if !d.leases.valid()
                || revision(get_world(self.worlds, d.target.manager)?, d.target.native)?
                    != d.revision
            {
                return Err(Error::Stale);
            }
        }
        Ok(())
    }
}
