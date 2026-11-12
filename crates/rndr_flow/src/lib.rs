//! Opt-in headless ownership metadata, not a document store or executor.
//!
//! Keep [`Coordinator`] in the host, outside its participating Worlds. Bind a
//! [`WorldOwner`] to each World and optionally bind [`ScopeOwner`] to an entity.
//! Replacement/destruction drops the token and immediately closes its entire
//! subtree's admission. For `World::remove`, drop the returned owner immediately:
//! retaining/moving it is not a supported detach/reparent operation. Call [`Coordinator::pump`] before result consumers
//! and after callback batches to drain physical cleanup. Explicit disposal uses
//! the same flags and drain; core hierarchy order is irrelevant. No other
//! World-destruction reconciliation mode is supported.
//!
//! Drop only signals. In particular dropping the coordinator never invokes host
//! callbacks: signal owners, pump to completion, then drop it for orderly cleanup.
//! Forced host loss releases captures but cannot guarantee unsubscribe/IO drain.
//! Callbacks may capture non-Send values; commands are bounded and cannot borrow
//! the coordinator. No backend cancellation or worker acknowledgement is implied.

mod transition;

use rndr_core::{Entity, World};
use std::collections::{BTreeMap, VecDeque};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

static AUTHORITIES: AtomicU64 = AtomicU64::new(0);

fn allocate_authority(counter: &AtomicU64) -> Result<u64, Error> {
    counter
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |old| {
            old.checked_add(1)
        })
        .map(|old| old + 1)
        .map_err(|_| Error::Exhausted)
}

fn next(counter: &mut u64) -> Result<u64, Error> {
    let value = counter.checked_add(1).ok_or(Error::Exhausted)?;
    *counter = value;
    Ok(value)
}

/// Process-local identity; never persisted authority or a domain target.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ScopeId {
    authority: u64,
    serial: u64,
}

/// A distinct World registration, not a raw entity ID.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorldId(ScopeId);

/// Manager-owned document lifetime. No document bytes or revision live in flow.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DocumentId(ScopeId);
impl DocumentId {
    /// For cleanup/state observation only; direct `dispose` refuses documents.
    pub fn scope(self) -> ScopeId {
        self.0
    }
}

/// Checked, nonreused registration identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CleanupId {
    authority: u64,
    serial: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Visible,
    Hidden,
    Suspended,
    Disposed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Capacity,
    Exhausted,
    Stale,
    Suspended,
    Pending,
    InUse,
    Invalid,
}

/// Refusal retains the untouched submitted value. The caller owns its cleanup.
#[must_use = "recover and clean the refused value"]
pub struct Rejected<T> {
    pub reason: Error,
    pub value: T,
}
impl<T> std::fmt::Debug for Rejected<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Rejected")
            .field("reason", &self.reason)
            .finish_non_exhaustive()
    }
}

/// Payload-free, bounded diagnostics. Errors never become save success.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Ok,
    Error,
    Panicked,
    Disposed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub scope: ScopeId,
    pub cleanup: Option<CleanupId>,
    pub outcome: Outcome,
}

/// F3 budgets only. Lanes, history and guarded transitions belong to later slices.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub scopes: usize,
    pub cleanups_per_scope: usize,
    pub leases: usize,
    pub commands: usize,
    pub cleanups_per_pump: usize,
    pub diagnostics: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            scopes: 4096,
            cleanups_per_scope: 256,
            leases: 4096,
            commands: 1024,
            cleanups_per_pump: 256,
            diagnostics: 512,
        }
    }
}

struct Life {
    dead: AtomicBool,
    parent: Option<Arc<Life>>,
}
impl Life {
    fn live(&self) -> bool {
        let mut current = Some(self);
        while let Some(life) = current {
            if life.dead.load(Ordering::Acquire) {
                return false;
            }
            current = life.parent.as_deref();
        }
        true
    }
}

/// Cloneable read-only atomic admission flag; includes ancestor terminal loss.
#[derive(Clone)]
pub struct Liveness(Arc<Life>);
impl Liveness {
    pub fn is_live(&self) -> bool {
        self.0.live()
    }
}

/// The single, nonclone disposal owner. Drop signals; it never calls host code.
#[must_use = "retain or bind the owner; drop logically disposes its subtree"]
pub struct ScopeOwner {
    id: ScopeId,
    life: Arc<Life>,
}
impl ScopeOwner {
    pub fn id(&self) -> ScopeId {
        self.id
    }
    pub fn liveness(&self) -> Liveness {
        Liveness(self.life.clone())
    }
    /// Bind exactly one owner to a live entity. Dropping the removed component
    /// is terminal; retaining/moving a removed owner is not supported.
    pub fn bind(self, world: &mut World, entity: Entity) -> Result<(), Rejected<Self>> {
        if !self.life.live() {
            return Err(Rejected {
                reason: Error::Stale,
                value: self,
            });
        }
        if !world.is_alive(entity) || world.has::<Self>(entity) {
            return Err(Rejected {
                reason: Error::Invalid,
                value: self,
            });
        }
        world.insert(entity, self);
        Ok(())
    }
}
impl rndr_core::Component for ScopeOwner {}
impl Drop for ScopeOwner {
    fn drop(&mut self) {
        self.life.dead.store(true, Ordering::Release);
    }
}

/// A World has one registration. Store it in that World's local slot, not in
/// another World. Dropping the World signals all of this registration's scopes.
#[must_use]
pub struct WorldOwner(ScopeOwner);
impl WorldOwner {
    pub fn id(&self) -> WorldId {
        WorldId(self.0.id)
    }
    pub fn scope(&self) -> ScopeId {
        self.0.id
    }
    pub fn bind(self, world: &mut World) -> Result<(), Rejected<Self>> {
        if !self.0.life.live() {
            return Err(Rejected {
                reason: Error::Stale,
                value: self,
            });
        }
        if world.has_local::<Self>() {
            return Err(Rejected {
                reason: Error::InUse,
                value: self,
            });
        }
        world.insert_local(self);
        Ok(())
    }
}

/// A presentation holds a lease, never a second disposal-parent edge.
#[must_use = "retain the presentation lease"]
pub struct PresentationLease(Arc<AtomicBool>);
impl Drop for PresentationLease {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}
struct Lease {
    scope: ScopeId,
    document: DocumentId,
    held: Arc<AtomicBool>,
}

/// Recovery is a host assertion: resume can reconstruct from durable state.
/// Never use for an unrecoverable stream. Hidden does not imply suspension.
#[derive(Clone, Copy, Debug)]
pub enum Recovery {
    ReconcileFromDurableState,
}

/// F3 scope activity incarnation, not an F4 lane or a completion authorization.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActivityId {
    scope: ScopeId,
    generation: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CleanupKind {
    Activity,
    Lifetime,
}

/// Callback failure is payload-free so retained diagnostics cannot retain prose.
#[derive(Clone, Copy, Debug)]
pub struct CleanupError;

type Callback = Box<dyn FnOnce(&mut Commands<'_>) -> Result<(), CleanupError>>;
struct Cleanup {
    id: CleanupId,
    kind: CleanupKind,
    callback: Callback,
}

/// Only deferred disposal is supported during F3 callbacks. Queue overflow is
/// explicit, never silently discarded. Requests are revalidated FIFO after each
/// callback; no callback can recursively borrow the authoritative registry.
pub struct Commands<'a> {
    queue: &'a mut VecDeque<ScopeId>,
    limit: usize,
}
impl Commands<'_> {
    pub fn dispose(&mut self, scope: ScopeId) -> Result<(), Error> {
        if self.queue.len() == self.limit {
            return Err(Error::Capacity);
        }
        self.queue.push_back(scope);
        Ok(())
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Role {
    Application,
    World,
    Scope,
    Document,
}
struct Scope {
    parent: Option<ScopeId>,
    role: Role,
    life: Arc<Life>,
    state: State,
    generation: u64,
    cleanups: Vec<Cleanup>,
}

/// Bounded current storage counts, not claims of cooperative/backend quiescence.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counts {
    pub scopes: usize,
    pub cleanups: usize,
    pub leases: usize,
    pub active_leases: usize,
    pub pending_scopes: usize,
    pub pending_cleanups: usize,
    pub queued_commands: usize,
}

/// UI-thread metadata owner. It is deliberately not cloneable or behind a lock.
/// Documents are directly application/document-manager owned in F3; only
/// `close_document` can dispose one separately, and refuses held presentations.
/// Guarded revision/lease snapshots for F5 are not implemented here.
pub struct Coordinator {
    authority: u64,
    serial: u64,
    cleanup_serial: u64,
    lease_serial: u64,
    root: ScopeId,
    limits: Limits,
    scopes: BTreeMap<ScopeId, Scope>,
    leases: BTreeMap<u64, Lease>,
    commands: VecDeque<ScopeId>,
    diagnostics: VecDeque<Diagnostic>,
    dropped_diagnostics: u64,
}

impl Coordinator {
    pub fn new(limits: Limits) -> Result<(Self, ScopeOwner), Error> {
        if [
            limits.scopes,
            limits.cleanups_per_scope,
            limits.leases,
            limits.commands,
            limits.cleanups_per_pump,
            limits.diagnostics,
        ]
        .contains(&0)
        {
            return Err(Error::Invalid);
        }
        let authority = allocate_authority(&AUTHORITIES)?;
        Self::with_authority(limits, authority)
    }
    fn with_authority(limits: Limits, authority: u64) -> Result<(Self, ScopeOwner), Error> {
        let root = ScopeId {
            authority,
            serial: 1,
        };
        let life = Arc::new(Life {
            dead: AtomicBool::new(false),
            parent: None,
        });
        let owner = ScopeOwner {
            id: root,
            life: life.clone(),
        };
        let mut scopes = BTreeMap::new();
        scopes.insert(
            root,
            Scope {
                parent: None,
                role: Role::Application,
                life,
                state: State::Visible,
                generation: 0,
                cleanups: Vec::new(),
            },
        );
        Ok((
            Self {
                authority,
                serial: 1,
                cleanup_serial: 0,
                lease_serial: 0,
                root,
                limits,
                scopes,
                leases: BTreeMap::new(),
                commands: VecDeque::new(),
                diagnostics: VecDeque::new(),
                dropped_diagnostics: 0,
            },
            owner,
        ))
    }
    pub fn root(&self) -> ScopeId {
        self.root
    }
    fn live_scope(&self, id: ScopeId) -> Result<&Scope, Error> {
        self.scopes
            .get(&id)
            .filter(|s| s.life.live())
            .ok_or(Error::Stale)
    }
    pub fn state(&self, id: ScopeId) -> State {
        self.live_scope(id).map_or(State::Disposed, |s| s.state)
    }
    pub fn liveness(&self, id: ScopeId) -> Result<Liveness, Error> {
        Ok(Liveness(self.live_scope(id)?.life.clone()))
    }
    fn create_metadata(
        &mut self,
        parent: ScopeId,
        role: Role,
    ) -> Result<(ScopeId, Arc<Life>), Error> {
        let p = self.live_scope(parent)?;
        if p.role == Role::Document {
            return Err(Error::Invalid);
        }
        if self.scopes.len() >= self.limits.scopes {
            return Err(Error::Capacity);
        }
        let life = Arc::new(Life {
            dead: AtomicBool::new(false),
            parent: Some(p.life.clone()),
        });
        let id = ScopeId {
            authority: self.authority,
            serial: next(&mut self.serial)?,
        };
        self.scopes.insert(
            id,
            Scope {
                parent: Some(parent),
                role,
                life: life.clone(),
                state: State::Visible,
                generation: 0,
                cleanups: Vec::new(),
            },
        );
        Ok((id, life))
    }
    fn create(&mut self, parent: ScopeId, role: Role) -> Result<ScopeOwner, Error> {
        self.create_metadata(parent, role)
            .map(|(id, life)| ScopeOwner { id, life })
    }
    pub fn create_scope(&mut self, parent: ScopeId) -> Result<ScopeOwner, Error> {
        self.create(parent, Role::Scope)
    }
    pub fn register_world(&mut self) -> Result<WorldOwner, Error> {
        self.create(self.root, Role::World).map(WorldOwner)
    }
    pub fn create_document(&mut self) -> Result<DocumentId, Error> {
        let (id, _) = self.create_metadata(self.root, Role::Document)?;
        Ok(DocumentId(id))
    }
    pub fn document_is_live(&self, document: DocumentId) -> bool {
        self.live_scope(document.0).is_ok()
    }
    fn held(&self, lease: &Lease) -> bool {
        lease.held.load(Ordering::Acquire) && self.live_scope(lease.scope).is_ok()
    }
    fn sweep_leases(&mut self) {
        let retired: Vec<_> = self
            .leases
            .iter()
            .filter(|(_, l)| !self.held(l))
            .map(|(id, _)| *id)
            .collect();
        for id in retired {
            self.leases.remove(&id);
        }
    }
    pub fn lease(
        &mut self,
        scope: ScopeId,
        document: DocumentId,
    ) -> Result<PresentationLease, Error> {
        self.live_scope(scope)?;
        self.live_scope(document.0)?;
        if self.live_scope(scope)?.role == Role::Document {
            return Err(Error::Invalid);
        }
        self.sweep_leases();
        if self.leases.len() >= self.limits.leases {
            return Err(Error::Capacity);
        }
        let serial = next(&mut self.lease_serial)?;
        let held = Arc::new(AtomicBool::new(true));
        self.leases.insert(
            serial,
            Lease {
                scope,
                document,
                held: held.clone(),
            },
        );
        Ok(PresentationLease(held))
    }
    pub fn close_document(&mut self, document: DocumentId) -> Result<(), Error> {
        if self
            .leases
            .values()
            .any(|l| l.document == document && self.held(l))
        {
            return Err(Error::InUse);
        }
        self.dispose_internal(document.0)
    }
    pub fn set_visibility(&mut self, id: ScopeId, next: State) -> Result<(), Error> {
        let state = transition::visibility(self.live_scope(id)?.state, next)?;
        self.scopes.get_mut(&id).unwrap().state = state;
        Ok(())
    }
    pub fn activity(&self, id: ScopeId) -> Result<ActivityId, Error> {
        let scope = self.live_scope(id)?;
        if scope.state == State::Suspended {
            return Err(Error::Suspended);
        }
        Ok(ActivityId {
            scope: id,
            generation: scope.generation,
        })
    }
    pub fn activity_is_current(&self, activity: ActivityId) -> bool {
        self.activity(activity.scope) == Ok(activity)
    }
    pub fn suspend(&mut self, id: ScopeId, _: Recovery) -> Result<(), Error> {
        let scope = self.live_scope(id)?;
        let generation = transition::suspend(scope.state, scope.generation)?;
        let scope = self.scopes.get_mut(&id).unwrap();
        scope.generation = generation;
        scope.state = State::Suspended;
        Ok(())
    }
    /// Resume only after activity cleanup was attempted. Lifetime resources stay.
    pub fn resume(&mut self, id: ScopeId, visible: bool) -> Result<ActivityId, Error> {
        let scope = self.live_scope(id)?;
        if scope.state != State::Suspended {
            return Err(Error::Invalid);
        }
        if scope
            .cleanups
            .iter()
            .any(|c| c.kind == CleanupKind::Activity)
        {
            return Err(Error::Pending);
        }
        self.scopes.get_mut(&id).unwrap().state = if visible {
            State::Visible
        } else {
            State::Hidden
        };
        self.activity(id)
    }
    pub fn register_cleanup<F>(
        &mut self,
        id: ScopeId,
        kind: CleanupKind,
        callback: F,
    ) -> Result<CleanupId, Rejected<F>>
    where
        F: FnOnce(&mut Commands<'_>) -> Result<(), CleanupError> + 'static,
    {
        let refusal = self.live_scope(id).and_then(|scope| {
            if kind == CleanupKind::Activity && scope.state == State::Suspended {
                return Err(Error::Suspended);
            }
            if scope.cleanups.len() >= self.limits.cleanups_per_scope {
                return Err(Error::Capacity);
            }
            Ok(())
        });
        if let Err(reason) = refusal {
            return Err(Rejected {
                reason,
                value: callback,
            });
        }
        let serial = match next(&mut self.cleanup_serial) {
            Ok(serial) => serial,
            Err(reason) => {
                return Err(Rejected {
                    reason,
                    value: callback,
                })
            }
        };
        let cleanup = CleanupId {
            authority: self.authority,
            serial,
        };
        self.scopes.get_mut(&id).unwrap().cleanups.push(Cleanup {
            id: cleanup,
            kind,
            callback: Box::new(callback),
        });
        Ok(cleanup)
    }
    /// Idempotent logical disposal; old/expired terminal IDs are still dead.
    /// Direct document disposal is intentionally unavailable through this path.
    pub fn dispose(&mut self, id: ScopeId) -> Result<(), Error> {
        if self
            .scopes
            .get(&id)
            .is_some_and(|s| s.role == Role::Document)
        {
            return Err(Error::Invalid);
        }
        self.dispose_internal(id)
    }
    fn dispose_internal(&mut self, id: ScopeId) -> Result<(), Error> {
        if id.authority != self.authority || id.serial > self.serial {
            return Err(Error::Stale);
        }
        if let Some(scope) = self.scopes.get(&id) {
            scope.life.dead.store(true, Ordering::Release);
        }
        // Descendant liveness closes synchronously through the ancestor flag.
        self.reconcile_dead();
        Ok(())
    }
    fn reconcile_dead(&mut self) {
        for scope in self.scopes.values_mut() {
            if !scope.life.live() {
                scope.state = State::Disposed;
            }
        }
        self.sweep_leases();
    }
    fn order(&self) -> Vec<ScopeId> {
        let mut result = Vec::new();
        let mut stack = vec![(self.root, false)];
        while let Some((id, visited)) = stack.pop() {
            if visited {
                result.push(id);
                continue;
            }
            if !self.scopes.contains_key(&id) {
                continue;
            }
            stack.push((id, true));
            for (child, _) in self
                .scopes
                .iter()
                .rev()
                .filter(|(_, s)| s.parent == Some(id))
            {
                stack.push((*child, false));
            }
        }
        result
    }
    fn record(&mut self, event: Diagnostic) {
        if self.diagnostics.len() == self.limits.diagnostics {
            self.diagnostics.pop_front();
            self.dropped_diagnostics = self.dropped_diagnostics.saturating_add(1);
        }
        self.diagnostics.push_back(event);
    }
    /// Invoke at most the configured callback budget. Registrations are removed
    /// before invocation, no registry borrow survives a callback, and unwinding
    /// callbacks are recorded and independent cleanup continues. `panic=abort`
    /// and arbitrary destructor/double panics are not recoverable.
    pub fn pump(&mut self) -> Counts {
        self.reconcile_dead();
        let mut remaining = self.limits.cleanups_per_pump;
        loop {
            let mut selected = None;
            // Recompute after each callback: reentrant requests may have killed
            // an ancestor whose previously-live children were already visited.
            for id in self.order() {
                let scope = self.scopes.get_mut(&id).unwrap();
                let index = scope.cleanups.iter().rposition(|c| {
                    scope.state == State::Disposed
                        || (scope.state == State::Suspended && c.kind == CleanupKind::Activity)
                });
                if let Some(index) = index {
                    if remaining == 0 {
                        return self.counts();
                    }
                    selected = Some((id, scope.cleanups.remove(index)));
                    break;
                }
                if scope.state == State::Disposed {
                    self.scopes.remove(&id);
                    self.record(Diagnostic {
                        scope: id,
                        cleanup: None,
                        outcome: Outcome::Disposed,
                    });
                }
            }
            let Some((id, cleanup)) = selected else { break };
            remaining -= 1;
            let outcome = match catch_unwind(AssertUnwindSafe(|| {
                (cleanup.callback)(&mut Commands {
                    queue: &mut self.commands,
                    limit: self.limits.commands,
                })
            })) {
                Ok(Ok(())) => Outcome::Ok,
                Ok(Err(_)) => Outcome::Error,
                Err(_) => Outcome::Panicked,
            };
            self.record(Diagnostic {
                scope: id,
                cleanup: Some(cleanup.id),
                outcome,
            });
            while let Some(target) = self.commands.pop_front() {
                if self.dispose(target).is_err() {
                    self.record(Diagnostic {
                        scope: target,
                        cleanup: None,
                        outcome: Outcome::Error,
                    });
                }
            }
            self.reconcile_dead();
        }
        self.counts()
    }
    pub fn counts(&self) -> Counts {
        Counts {
            scopes: self.scopes.len(),
            cleanups: self.scopes.values().map(|s| s.cleanups.len()).sum(),
            leases: self.leases.len(),
            active_leases: self.leases.values().filter(|l| self.held(l)).count(),
            pending_scopes: self.scopes.values().filter(|s| !s.life.live()).count(),
            pending_cleanups: self
                .scopes
                .values()
                .map(|s| {
                    s.cleanups
                        .iter()
                        .filter(|c| {
                            !s.life.live()
                                || (s.state == State::Suspended && c.kind == CleanupKind::Activity)
                        })
                        .count()
                })
                .sum(),
            queued_commands: self.commands.len(),
        }
    }
    pub fn diagnostics(&self) -> impl Iterator<Item = &Diagnostic> {
        self.diagnostics.iter()
    }
    /// Saturating telemetry, never used as an identity/admission counter.
    pub fn dropped_diagnostics(&self) -> u64 {
        self.dropped_diagnostics
    }
}
impl Drop for Coordinator {
    fn drop(&mut self) {
        for scope in self.scopes.values() {
            scope.life.dead.store(true, Ordering::Release);
        }
    }
}

#[cfg(test)]
mod tests;
