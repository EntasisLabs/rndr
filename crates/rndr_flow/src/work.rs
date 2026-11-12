//! Bounded ownership of existing runtime handles, not a scheduler.
use crate::*;
use std::future::Future;
use std::pin::Pin;
use std::sync::Mutex;
use std::task::{Context, Poll};
use std::time::Duration;

#[derive(Clone, Copy, Debug)]
pub struct WorkLimits {
    pub lanes_per_scope: usize,
    /// Includes retired/unacknowledged work, independently of result eligibility.
    pub slots: usize,
    pub effects_per_world: usize,
}
impl Default for WorkLimits {
    fn default() -> Self {
        Self {
            lanes_per_scope: 32,
            slots: 4096,
            effects_per_world: 256,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Terminal {
    Running,
    Succeeded,
    Failed,
    Panicked,
    DroppedUnknown,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Observation {
    pub retired: bool,
    pub cancellation_signalled: bool,
    pub cleanup_attempted: bool,
    /// True only after the actual wrapped future/producer has exited/dropped.
    pub exit_acknowledged: bool,
    /// Reserved task outputs and queue/event deliveries whose final receipt
    /// has not been dropped. Tasks reserve BEFORE polling, not after exit.
    pub pending_deliveries: usize,
    pub terminal: Terminal,
}
impl Default for Observation {
    fn default() -> Self {
        Self {
            retired: false,
            cancellation_signalled: false,
            cleanup_attempted: false,
            exit_acknowledged: false,
            pending_deliveries: 0,
            terminal: Terminal::Running,
        }
    }
}
#[derive(Clone, Default, Debug)]
pub struct WorkStatus(Arc<Mutex<Observation>>);
impl WorkStatus {
    pub fn observation(&self) -> Observation {
        *self.0.lock().unwrap_or_else(|e| e.into_inner())
    }
    fn update(&self, f: impl FnOnce(&mut Observation)) {
        f(&mut self.0.lock().unwrap_or_else(|e| e.into_inner()));
    }
    fn retire(&self) {
        self.update(|s| {
            s.retired = true;
            s.cancellation_signalled = true;
        });
    }
}
#[derive(Debug)]
pub(crate) struct DeliveryReceipt(WorkStatus);
impl DeliveryReceipt {
    pub(crate) fn new(status: WorkStatus) -> Self {
        status.update(|s| {
            s.pending_deliveries = s
                .pending_deliveries
                .checked_add(1)
                .expect("resident delivery count overflow")
        });
        Self(status)
    }
}
impl Drop for DeliveryReceipt {
    fn drop(&mut self) {
        self.0.update(|s| s.pending_deliveries -= 1);
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskFailure {
    Failed,
    Panicked,
}

/// Poll normalization is identical on local/background/custom-executor paths.
/// Dropping this wrapper acknowledges the USER future only after its destructor
/// returned/unwound. It does not join arbitrary IO or certify external work.
struct Tracked<F> {
    inner: Option<Pin<Box<F>>>,
    status: WorkStatus,
}
impl<F> Tracked<F> {
    fn new(inner: F, status: WorkStatus) -> Self {
        Self {
            inner: Some(Box::pin(inner)),
            status,
        }
    }
    fn drop_inner(&mut self) {
        if let Some(inner) = self.inner.take() {
            let panic = catch_unwind(AssertUnwindSafe(|| drop(inner))).is_err();
            self.status.update(|s| {
                if panic {
                    s.terminal = Terminal::Panicked;
                } else if s.terminal == Terminal::Running {
                    s.terminal = Terminal::DroppedUnknown;
                }
                s.exit_acknowledged = true;
            });
        }
    }
}
impl<F> Drop for Tracked<F> {
    fn drop(&mut self) {
        self.drop_inner();
    }
}
impl<F, T> Future for Tracked<F>
where
    F: Future<Output = Result<T, TaskFailure>>,
{
    type Output = Result<T, TaskFailure>;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        let polled = catch_unwind(AssertUnwindSafe(|| {
            this.inner
                .as_mut()
                .expect("poll after Ready")
                .as_mut()
                .poll(cx)
        }));
        let value = match polled {
            Ok(Poll::Pending) => return Poll::Pending,
            Ok(Poll::Ready(value)) => value,
            Err(_) => Err(TaskFailure::Panicked),
        };
        this.status.update(|s| {
            if s.terminal != Terminal::Panicked {
                s.terminal = match &value {
                    Ok(_) => Terminal::Succeeded,
                    Err(TaskFailure::Failed) => Terminal::Failed,
                    Err(TaskFailure::Panicked) => Terminal::Panicked,
                };
            }
        });
        this.drop_inner();
        if this.status.observation().terminal == Terminal::Panicked {
            Poll::Ready(Err(TaskFailure::Panicked))
        } else {
            Poll::Ready(value)
        }
    }
}

/// Move this guard into a controlled producer BEFORE starting it. Explicit
/// finish records an outcome; dropping it reports unknown exit, never success.
/// Only the producer may finish; a UI cancellation signal is not an ack.
pub struct ProducerExit {
    status: Option<WorkStatus>,
}
impl ProducerExit {
    /// Run on the EXISTING producer's executor/thread. A blocked closure retains
    /// the charge. No public cancellation-as-ack setter is provided.
    pub fn run(
        mut self,
        producer: impl FnOnce() -> Result<(), TaskFailure>,
    ) -> Result<(), TaskFailure> {
        let status = self.status.take().unwrap();
        let result = catch_unwind(AssertUnwindSafe(producer)).unwrap_or(Err(TaskFailure::Panicked));
        status.update(|s| {
            s.terminal = match result {
                Ok(()) => Terminal::Succeeded,
                Err(TaskFailure::Failed) => Terminal::Failed,
                Err(TaskFailure::Panicked) => Terminal::Panicked,
            };
            s.exit_acknowledged = true;
        });
        result
    }
    /// Wrap an async producer without imposing Send or choosing an executor.
    pub fn track<F, T>(mut self, future: F) -> impl Future<Output = Result<T, TaskFailure>>
    where
        F: Future<Output = Result<T, TaskFailure>>,
    {
        Tracked::new(future, self.status.take().unwrap())
    }
}
impl Drop for ProducerExit {
    fn drop(&mut self) {
        if let Some(status) = &self.status {
            status.update(|s| {
                if !s.exit_acknowledged {
                    s.terminal = Terminal::DroppedUnknown;
                    s.exit_acknowledged = true;
                }
            });
        }
    }
}

trait FeedHandle {
    fn close(&self);
}
impl<T> FeedHandle for rndr_core::feed::Feed<T> {
    fn close(&self) {
        self.cancel();
    }
}
enum Handle {
    Task(rndr_core::TaskHandle),
    Timer(rndr_core::TimerHandle),
    Feed(Box<dyn FeedHandle>),
    Effect,
    Pending,
}
pub(crate) struct Slot {
    token: RequestToken,
    handle: Option<Handle>,
    status: WorkStatus,
}
#[derive(Default)]
pub(crate) struct Registry {
    pub limits: WorkLimits,
    serial: u64,
    slots: BTreeMap<u64, Slot>,
    after: u64,
    pub sweep_turn: bool,
}
impl Drop for Registry {
    fn drop(&mut self) {
        // Signal only; no World callback is run here. Concrete handle drops
        // request runtime cancellation, while actual exits remain observable.
        for slot in self.slots.values() {
            if slot.handle.is_some() {
                slot.status.retire();
            } else {
                slot.status.update(|s| s.retired = true);
            }
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WorkCounts {
    pub charged: usize,
    pub retired_unacknowledged: usize,
    pub pending_deliveries: usize,
    pub handles: usize,
}
impl Registry {
    #[cfg(test)]
    pub(crate) fn set_serial(&mut self, value: u64) {
        self.serial = value;
    }
    fn preflight(&self) -> Result<u64, Error> {
        let serial = self.serial.checked_add(1).ok_or(Error::Exhausted)?;
        if self.slots.len() >= self.limits.slots {
            return Err(Error::Capacity);
        }
        Ok(serial)
    }
    fn insert(&mut self, id: u64, token: RequestToken, handle: Handle, status: WorkStatus) {
        self.serial = id;
        self.slots.insert(
            id,
            Slot {
                token,
                handle: Some(handle),
                status,
            },
        );
    }
    pub fn has_slots(&self) -> bool {
        !self.slots.is_empty()
    }
    pub fn sweep(&mut self) {
        if let Some((&id, slot)) = self.slots.range((Excluded(self.after), Unbounded)).next() {
            self.after = id;
            let observation = slot.status.observation();
            if slot.handle.is_none()
                && observation.exit_acknowledged
                && observation.pending_deliveries == 0
            {
                self.slots.remove(&id);
            }
        } else {
            self.after = 0;
        }
    }
}
impl Coordinator {
    /// Configure before any work is admitted. No existing F3 Limits literals
    /// change; limits cannot be shrunk underneath admitted or retired work.
    pub fn configure_work(&mut self, limits: WorkLimits) -> Result<(), Error> {
        if [
            limits.lanes_per_scope,
            limits.slots,
            limits.effects_per_world,
        ]
        .contains(&0)
        {
            return Err(Error::Invalid);
        }
        if self.work.serial != 0 || self.scopes.values().any(|s| !s.lanes.is_empty()) {
            return Err(Error::InUse);
        }
        self.work.limits = limits;
        Ok(())
    }
    /// Exact synchronous observation, not part of pump_work's budget.
    pub fn work_counts(&self) -> WorkCounts {
        WorkCounts {
            charged: self.work.slots.len(),
            pending_deliveries: self
                .work
                .slots
                .values()
                .map(|s| s.status.observation().pending_deliveries)
                .sum(),
            retired_unacknowledged: self
                .work
                .slots
                .values()
                .filter(|s| {
                    let o = s.status.observation();
                    o.retired && (!o.exit_acknowledged || o.pending_deliveries != 0)
                })
                .count(),
            handles: self
                .work
                .slots
                .values()
                .filter(|s| s.handle.is_some())
                .count(),
        }
    }
    fn install_slot(&mut self, id: u64, token: RequestToken, handle: Handle, status: WorkStatus) {
        self.work.insert(id, token, handle, status);
        self.scopes
            .get_mut(&token.activity.scope)
            .unwrap()
            .work
            .insert(id);
        self.rescan = true;
    }
    /// One slot visit per traversal unit; no global scan. Called in child-first
    /// order before custom cleanup. Retired slots remain charged in Registry.
    pub(crate) fn handle_step(&mut self, scope: ScopeId, after: u64) -> Option<(u64, bool)> {
        let id = self.scopes[&scope]
            .work
            .range((Excluded(after), Unbounded))
            .next()
            .copied()?;
        let token = self.work.slots[&id].token;
        // Enter propagated ancestor death one node at a time. Do NOT call
        // request_is_current/Life::live here: those synchronous observations
        // walk ancestry and would hide depth work under a single pump unit.
        let owner = &self.scopes[&scope];
        let current = owner.state != State::Disposed
            && owner.state != State::Suspended
            && owner.generation == token.activity.generation
            && owner
                .lanes
                .get(&token.lane)
                .is_some_and(|l| l.active && l.generation == token.generation)
            && self
                .scopes
                .get(&token.world.0)
                .is_some_and(|w| w.state != State::Disposed);
        if !current {
            let slot = self.work.slots.get_mut(&id).unwrap();
            slot.status.retire();
            if matches!(slot.handle, Some(Handle::Effect))
                && !slot.status.observation().exit_acknowledged
            {
                // Unsubscribe must execute in its owning World before custom
                // cleanup. A lagging World is pending, not falsely drained.
                return Some((after, true));
            }
            let handle = slot.handle.take();
            // Move out of registry before invoking core cancellation/wake/drop.
            let outcome = catch_unwind(AssertUnwindSafe(|| match handle {
                Some(Handle::Task(task)) => {
                    slot.status.update(|s| s.cleanup_attempted = true);
                    task.cancel();
                    drop(task);
                }
                Some(Handle::Timer(timer)) => {
                    timer.cancel();
                    drop(timer);
                    slot.status.update(|s| {
                        s.cleanup_attempted = true;
                    });
                }
                Some(Handle::Feed(feed)) => {
                    feed.close();
                    drop(feed);
                    slot.status.update(|s| s.cleanup_attempted = true);
                }
                Some(Handle::Effect) | Some(Handle::Pending) | None => {}
            }));
            if outcome.is_err() {
                slot.status.update(|s| s.terminal = Terminal::Panicked);
            }
            self.scopes.get_mut(&scope).unwrap().work.remove(&id);
        }
        Some((id, false))
    }
    fn finish_spawn(
        &mut self,
        id: u64,
        token: RequestToken,
        result: std::thread::Result<rndr_core::TaskHandle>,
    ) {
        let slot = self.work.slots.get_mut(&id).unwrap();
        match result {
            Ok(handle) => slot.handle = Some(Handle::Task(handle)),
            Err(_) => {
                slot.handle = None;
                // A panicking executor may have retained its Job without
                // returning a cancellation handle. Do not fabricate a signal
                // or exit acknowledgment. Its reservation stays charged.
                slot.status.update(|s| {
                    s.retired = true;
                    s.terminal = Terminal::Panicked;
                });
                self.scopes
                    .get_mut(&token.activity.scope)
                    .unwrap()
                    .work
                    .remove(&id);
                self.scopes
                    .get_mut(&token.activity.scope)
                    .unwrap()
                    .lanes
                    .get_mut(&token.lane)
                    .unwrap()
                    .active = false;
            }
        }
    }
    /// Atomically preflight lane+slot before invalidating the old request.
    /// Cancellation is drained by pump_work; eligibility changes immediately.
    pub fn replace_local<D, T, F>(
        &mut self,
        world: &mut World,
        scope: ScopeId,
        key: LaneKey,
        identity: Identity<D>,
        mode: DeliveryMode,
        future: F,
    ) -> Result<(Request<D>, WorkStatus), Rejected<F>>
    where
        D: Clone + Eq + Send + Sync + 'static,
        T: Send + Sync + 'static,
        F: Future<Output = Result<T, TaskFailure>> + 'static,
    {
        let prepared = match self
            .prepare_request(world, scope, key)
            .and_then(|p| Ok((p, self.work.preflight()?)))
        {
            Ok(v) => v,
            Err(reason) => {
                return Err(Rejected {
                    reason,
                    value: future,
                })
            }
        };
        if mode != DeliveryMode::Once {
            return Err(Rejected {
                reason: Error::Invalid,
                value: future,
            });
        }
        let status = WorkStatus::default();
        let request = self.install_request(scope, key, identity, mode, prepared.0);
        self.install_slot(prepared.1, request.token, Handle::Pending, status.clone());
        let tracked = Tracked::new(future, status.clone());
        let reserved = request.completion(None, ()).with_receipt(status.clone());
        let result = catch_unwind(AssertUnwindSafe(|| {
            rndr_core::spawn_local(world, async move { reserved.with_value(tracked.await) })
        }));
        self.finish_spawn(prepared.1, request.token, result);
        Ok((request, status))
    }
    pub fn replace_background<D, T, F>(
        &mut self,
        world: &mut World,
        scope: ScopeId,
        key: LaneKey,
        identity: Identity<D>,
        future: F,
    ) -> Result<(Request<D>, WorkStatus), Rejected<F>>
    where
        D: Clone + Eq + Send + Sync + 'static,
        T: Send + Sync + 'static,
        F: Future<Output = Result<T, TaskFailure>> + Send + 'static,
    {
        let prepared = match self
            .prepare_request(world, scope, key)
            .and_then(|p| Ok((p, self.work.preflight()?)))
        {
            Ok(v) => v,
            Err(reason) => {
                return Err(Rejected {
                    reason,
                    value: future,
                })
            }
        };
        let status = WorkStatus::default();
        let request = self.install_request(scope, key, identity, DeliveryMode::Once, prepared.0);
        self.install_slot(prepared.1, request.token, Handle::Pending, status.clone());
        let tracked = Tracked::new(future, status.clone());
        let reserved = request.completion(None, ()).with_receipt(status.clone());
        let result = catch_unwind(AssertUnwindSafe(|| {
            rndr_core::spawn(world, async move { reserved.with_value(tracked.await) })
        }));
        self.finish_spawn(prepared.1, request.token, result);
        Ok((request, status))
    }
    pub fn replace_timer<D, T>(
        &mut self,
        world: &mut World,
        scope: ScopeId,
        key: LaneKey,
        identity: Identity<D>,
        delay: Duration,
        value: T,
    ) -> Result<(Request<D>, WorkStatus), Rejected<T>>
    where
        D: Clone + Eq + Send + Sync + 'static,
        T: Send + Sync + 'static,
    {
        let prepared = match self
            .prepare_request(world, scope, key)
            .and_then(|p| Ok((p, self.work.preflight()?)))
        {
            Ok(v) => v,
            Err(reason) => return Err(Rejected { reason, value }),
        };
        let status = WorkStatus::default();
        let request = self.install_request(scope, key, identity, DeliveryMode::Once, prepared.0);
        // An event-owned sentinel acknowledges actual event disposal, including
        // timer cancellation and already-published buffers. Never infer firing.
        let event = TimerCompletion {
            completion: Some(request.completion(None, value).with_receipt(status.clone())),
            status: status.clone(),
        };
        let handle = rndr_core::after(world, delay, event);
        self.install_slot(
            prepared.1,
            request.token,
            Handle::Timer(handle),
            status.clone(),
        );
        Ok((request, status))
    }
    pub fn replace_feed<D, T>(
        &mut self,
        world: &mut World,
        scope: ScopeId,
        key: LaneKey,
        identity: Identity<D>,
        mode: DeliveryMode,
        config: rndr_core::FeedConfig,
    ) -> Result<FeedStart<D, T>, Error>
    where
        D: Clone + Eq + Send + Sync + 'static,
        T: Send + Sync + 'static,
    {
        let prepared = self.prepare_request(world, scope, key)?;
        let id = self.work.preflight()?;
        let status = WorkStatus::default();
        let (feed, sender) = rndr_core::feed::open(world, config);
        // Captured BEFORE producer start. Each replacement uses a fresh feed;
        // no saturating Feed::advance and no per-chunk replacement is involved.
        let epoch = feed.epoch();
        let request = self.install_request(scope, key, identity, mode, prepared);
        self.install_slot(
            id,
            request.token,
            Handle::Feed(Box::new(feed)),
            status.clone(),
        );
        Ok((
            request.clone(),
            CapturedSender {
                request,
                sender,
                epoch,
                status: status.clone(),
            },
            ProducerExit {
                status: Some(status.clone()),
            },
            status,
        ))
    }
}

pub struct TimerCompletion<D, T> {
    completion: Option<Completion<D, T>>,
    status: WorkStatus,
}
impl<D: Send + Sync + 'static, T: Send + Sync + 'static> rndr_core::Event
    for TimerCompletion<D, T>
{
}
impl<D: Clone, T: Clone> TimerCompletion<D, T> {
    pub fn completion(&self) -> Completion<D, T> {
        self.completion.as_ref().expect("timer completion").clone()
    }
}
impl<D, T> Drop for TimerCompletion<D, T> {
    fn drop(&mut self) {
        self.status.update(|s| {
            s.exit_acknowledged = true;
            if s.terminal == Terminal::Running {
                s.terminal = Terminal::DroppedUnknown;
            }
        });
    }
}
/// Request, fixed-epoch sender, producer-exit wrapper and terminal observation.
pub type FeedStart<D, T> = (Request<D>, CapturedSender<D, T>, ProducerExit, WorkStatus);
pub struct CapturedSender<D, T> {
    request: Request<D>,
    sender: rndr_core::feed::Sender<Completion<D, T>>,
    epoch: rndr_core::feed::Epoch,
    status: WorkStatus,
}
impl<D: Clone, T> CapturedSender<D, T> {
    pub fn try_send(&self, sequence: Option<u64>, value: T) -> Result<(), rndr_core::FeedError> {
        self.sender.try_send_at(
            self.epoch,
            self.request
                .completion(sequence, value)
                .with_receipt(self.status.clone()),
        )
    }
    pub fn send(&self, sequence: Option<u64>, value: T) -> Result<(), rndr_core::FeedError> {
        self.sender.send_at(
            self.epoch,
            self.request
                .completion(sequence, value)
                .with_receipt(self.status.clone()),
        )
    }
    pub fn is_cancelled(&self) -> bool {
        self.sender.is_cancelled()
    }
    pub fn wait_until_cancelled(&self) {
        self.sender.wait_until_cancelled();
    }
}

/// Safe standalone refresh guard for existing feed owners. Caller must hold
/// exclusive refresh authority (never share Feed::advance with another writer).
/// Max is checked BEFORE core advance can clear queued values.
pub fn advance_feed<T>(
    feed: &mut rndr_core::feed::Feed<T>,
) -> Result<rndr_core::feed::Epoch, Error> {
    checked_epoch(feed.epoch().number())?;
    Ok(feed.advance())
}
fn checked_epoch(epoch: u64) -> Result<u64, Error> {
    epoch.checked_add(1).ok_or(Error::Exhausted)
}

// World-specific removal effects live ONLY in that World's local table. This
// is the consumer seam for e.g. explicit Liquid unsubscribe; no flow->UI edge.
type EffectFn = Box<dyn FnOnce(&mut World)>;
struct Effect {
    status: WorkStatus,
    callback: Option<EffectFn>,
}
impl Drop for Effect {
    fn drop(&mut self) {
        if let Some(callback) = self.callback.take() {
            let panic = catch_unwind(AssertUnwindSafe(|| drop(callback))).is_err();
            self.status.update(|s| {
                s.exit_acknowledged = true;
                s.terminal = if panic {
                    Terminal::Panicked
                } else {
                    Terminal::DroppedUnknown
                };
            });
        }
    }
}
#[derive(Default)]
struct WorldEffects {
    entries: BTreeMap<(u64, u64), Effect>,
    after: Option<(u64, u64)>,
}
impl Coordinator {
    pub fn own_world_effect<D: Clone + Eq + 'static, F: FnOnce(&mut World) + 'static>(
        &mut self,
        world: &mut World,
        request: &Request<D>,
        effect: F,
    ) -> Result<WorkStatus, Rejected<F>> {
        let valid = world.local::<WorldOwner>().map(WorldOwner::id) == Some(request.token.world)
            && self.request_is_current(request.token);
        let preflight = if !valid {
            Err(Error::Stale)
        } else if world
            .local::<WorldEffects>()
            .is_some_and(|e| e.entries.len() >= self.work.limits.effects_per_world)
        {
            Err(Error::Capacity)
        } else {
            self.work.preflight()
        };
        let id = match preflight {
            Ok(id) => id,
            Err(reason) => {
                return Err(Rejected {
                    reason,
                    value: effect,
                })
            }
        };
        let status = WorkStatus::default();
        if !world.has_local::<WorldEffects>() {
            world.insert_local(WorldEffects::default());
        }
        world.local_mut::<WorldEffects>().unwrap().entries.insert(
            (self.authority, id),
            Effect {
                status: status.clone(),
                callback: Some(Box::new(effect)),
            },
        );
        self.install_slot(id, request.token, Handle::Effect, status.clone());
        Ok(status)
    }
}
/// At most budget table visits/callbacks. Extract each callback before invoking
/// it: no local/resource borrow survives host code. Reentrant additions survive.
/// Wrong Worlds cannot see this table. World loss reports dropped/unknown.
pub fn drain_world_effects(world: &mut World, budget: usize) -> usize {
    let mut units = 0;
    while units < budget {
        units += 1;
        let effect = {
            let Some(table) = world.local_mut::<WorldEffects>() else {
                break;
            };
            let candidate = match table.after {
                Some(key) => table.entries.range((Excluded(key), Unbounded)).next(),
                None => table.entries.first_key_value(),
            };
            if let Some((&key, effect)) = candidate {
                table.after = Some(key);
                if effect.status.observation().retired {
                    table.entries.remove(&key)
                } else {
                    None
                }
            } else {
                table.after = None;
                None
            }
        };
        if let Some(mut effect) = effect {
            effect.status.update(|s| s.cleanup_attempted = true);
            let callback = effect.callback.take().unwrap();
            let result = catch_unwind(AssertUnwindSafe(|| callback(world)));
            effect.status.update(|s| {
                s.exit_acknowledged = true;
                s.terminal = if result.is_ok() {
                    Terminal::Succeeded
                } else {
                    Terminal::Panicked
                };
            });
        }
    }
    units
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn epoch_max_refuses_without_advancing() {
        assert_eq!(checked_epoch(u64::MAX), Err(Error::Exhausted));
        assert_eq!(checked_epoch(u64::MAX - 1), Ok(u64::MAX));
    }
}
