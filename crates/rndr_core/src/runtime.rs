//! Tasks, timers, and the frame pump.
//!
//! Background futures are `Send` and run on the [`Executor`] the runner
//! installed. Foreground futures run on the thread that calls
//! [`run_until_parked`] or [`crate::App::update`] and may hold non-`Send`
//! values across `.await`. Completions are delivered on that thread as
//! [`crate::Event`]s. Dropping a [`TaskHandle`] cancels the task and drops a
//! result that has not been committed yet. [`TaskHandle::detach`] opts out.

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, Weak};
use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};
use std::time::Duration;

use crate::clock::{Clock, SystemClock, Time};
use crate::event::{self, Event, EventHub};
use crate::world::World;

const MAX_STEPS: u32 = 10_000;
const TIMER_BURST: u8 = 64;

/// Wakes the window loop. The desktop runner sends a winit user event.
pub trait Wake: Send + Sync {
    fn wake(&self);
}

/// Wake target for headless runs. Does not schedule a redraw.
#[derive(Clone, Copy, Debug, Default)]
pub struct NopWake;

impl Wake for NopWake {
    fn wake(&self) {}
}

type WakeSlot = Arc<Mutex<Arc<dyn Wake>>>;
type Inbox = Arc<Mutex<Vec<Delivery>>>;
type ReadyQueue = Arc<Mutex<Vec<u64>>>;
type SendOnce = Box<dyn FnOnce(&mut World) + Send>;
type SendFn = Box<dyn Fn(&mut World) + Send>;
type LocalOnce = Box<dyn FnOnce(&mut World)>;

struct Delivery {
    shared: Arc<TaskShared>,
    emit: SendOnce,
}

struct RuntimeInner {
    clock: Mutex<Arc<dyn Clock>>,
    wake: WakeSlot,
    timers: Mutex<Vec<Timer>>,
    inbox: Inbox,
    local_ready: ReadyQueue,
    ids: AtomicU64,
}

/// Handle held by jobs so `sleep` can find the scheduler during `poll`.
/// Jobs keep a [`Weak`] so a parked task does not leak the runtime.
#[derive(Clone)]
pub(crate) struct Runtime {
    inner: Arc<RuntimeInner>,
}

impl Runtime {
    fn new() -> Self {
        Self {
            inner: Arc::new(RuntimeInner {
                clock: Mutex::new(Arc::new(SystemClock::new())),
                wake: Arc::new(Mutex::new(Arc::new(NopWake))),
                timers: Mutex::new(Vec::new()),
                inbox: Arc::new(Mutex::new(Vec::new())),
                local_ready: Arc::new(Mutex::new(Vec::new())),
                ids: AtomicU64::new(1),
            }),
        }
    }

    fn downgrade(&self) -> Weak<RuntimeInner> {
        Arc::downgrade(&self.inner)
    }

    fn elapsed(&self) -> Duration {
        lock(&self.inner.clock).elapsed()
    }

    fn alloc_id(&self) -> u64 {
        self.inner.ids.fetch_add(1, Ordering::Relaxed)
    }

    fn wake_app(&self) {
        lock(&self.inner.wake).wake();
    }

    fn wake_slot(&self) -> WakeSlot {
        Arc::clone(&self.inner.wake)
    }

    fn arm_wake(&self, when: Duration, waker: Waker, cancel: Arc<AtomicBool>) {
        {
            let mut timers = lock(&self.inner.timers);
            if let Some(timer) = timers
                .iter_mut()
                .find(|timer| Arc::ptr_eq(&timer.cancel, &cancel))
            {
                timer.when = when;
                timer.kind = TimerKind::Wake(Some(waker));
            } else {
                timers.push(Timer {
                    when,
                    period: None,
                    cancel,
                    kind: TimerKind::Wake(Some(waker)),
                    burst: 0,
                });
            }
        }
        self.wake_app();
    }

    fn soonest(&self) -> Option<Duration> {
        let mut timers = lock(&self.inner.timers);
        timers.retain(|timer| !timer.cancel.load(Ordering::Acquire));
        timers.iter().map(|timer| timer.when).min()
    }
}

fn runtime(world: &World) -> Runtime {
    world
        .resource::<Runtime>()
        .cloned()
        .expect("runtime installed by App")
}

thread_local! {
    static CURRENT: std::cell::RefCell<Option<Arc<RuntimeInner>>> = const { std::cell::RefCell::new(None) };
}

struct RtGuard {
    prev: Option<Arc<RuntimeInner>>,
}

impl Drop for RtGuard {
    fn drop(&mut self) {
        let prev = self.prev.take();
        CURRENT.with(|cell| *cell.borrow_mut() = prev);
    }
}

fn enter(inner: &Arc<RuntimeInner>) -> RtGuard {
    CURRENT.with(|cell| {
        let prev = cell.borrow_mut().replace(Arc::clone(inner));
        RtGuard { prev }
    })
}

fn current_runtime() -> Arc<RuntimeInner> {
    CURRENT.with(|cell| {
        cell.borrow()
            .as_ref()
            .cloned()
            .expect("sleep polled outside a rndr task")
    })
}

pub(crate) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|err| err.into_inner())
}

struct TaskShared {
    cancelled: AtomicBool,
    nudge: Mutex<Option<Arc<dyn Fn() + Send + Sync>>>,
}

impl TaskShared {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            cancelled: AtomicBool::new(false),
            nudge: Mutex::new(None),
        })
    }

    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    fn set_nudge(&self, nudge: Arc<dyn Fn() + Send + Sync>) {
        *lock(&self.nudge) = Some(nudge);
    }

    fn cancel_and_nudge(&self) {
        self.cancelled.store(true, Ordering::Release);
        if let Some(nudge) = lock(&self.nudge).as_ref() {
            nudge();
        }
    }
}

/// Cancels the task when dropped. [`detach`](Self::detach) keeps it running.
#[must_use = "dropping a TaskHandle cancels the task; call detach() to keep it"]
pub struct TaskHandle {
    shared: Arc<TaskShared>,
    detached: bool,
}

impl TaskHandle {
    fn new(shared: Arc<TaskShared>) -> Self {
        Self {
            shared,
            detached: false,
        }
    }

    /// Stop the task and drop a result that has not been committed.
    pub fn cancel(&self) {
        self.shared.cancel_and_nudge();
    }

    pub fn is_cancelled(&self) -> bool {
        self.shared.is_cancelled()
    }

    /// Run to completion even though this handle is going away.
    pub fn detach(mut self) {
        self.detached = true;
    }
}

impl Drop for TaskHandle {
    fn drop(&mut self) {
        if !self.detached {
            self.shared.cancel_and_nudge();
        }
    }
}

/// A `Send` future plus the scheduler it may sleep on.
///
/// Executors poll this on a worker and drop it to cancel. [`Job::poll`]
/// installs the scheduler for [`sleep`] and returns [`Poll::Ready`] once the
/// task has been cancelled.
pub struct Job {
    future: Pin<Box<dyn Future<Output = ()> + Send>>,
    runtime: Weak<RuntimeInner>,
    shared: Arc<TaskShared>,
}

impl Job {
    pub fn cancelled(&self) -> bool {
        self.shared.is_cancelled()
    }

    pub fn poll(&mut self, cx: &mut Context<'_>) -> Poll<()> {
        let Some(inner) = self.runtime.upgrade() else {
            return Poll::Ready(());
        };
        let _guard = enter(&inner);
        if self.shared.is_cancelled() {
            return Poll::Ready(());
        }
        match catch_unwind(AssertUnwindSafe(|| self.future.as_mut().poll(cx))) {
            Ok(poll) => poll,
            Err(_) => {
                self.shared.cancelled.store(true, Ordering::Release);
                Poll::Ready(())
            }
        }
    }

    fn wake_slot(&self) -> Option<WakeSlot> {
        self.runtime.upgrade().map(|inner| Arc::clone(&inner.wake))
    }

    pub(crate) fn set_nudge(&self, nudge: Arc<dyn Fn() + Send + Sync>) {
        self.shared.set_nudge(nudge);
    }
}

/// Polls `Send` futures. The desktop runner installs a thread pool.
/// [`poll_inline`](Executor::poll_inline) is for executors that share the
/// caller's thread; a pool returns `false` and polls on its workers.
pub trait Executor: Send + Sync {
    fn spawn(&self, job: Job);

    fn poll_inline(&self) -> bool {
        false
    }

    fn has_ready(&self) -> bool {
        false
    }
}

pub(crate) struct Exec(pub Arc<dyn Executor>);

struct SchedWaker {
    schedule: Arc<dyn Fn() + Send + Sync>,
}

pub(crate) fn waker_for(schedule: Arc<dyn Fn() + Send + Sync>) -> Waker {
    fn clone(ptr: *const ()) -> RawWaker {
        let arc = unsafe { Arc::<SchedWaker>::from_raw(ptr as *const SchedWaker) };
        let cloned = Arc::clone(&arc);
        std::mem::forget(arc);
        raw(cloned)
    }
    fn wake(ptr: *const ()) {
        let arc = unsafe { Arc::<SchedWaker>::from_raw(ptr as *const SchedWaker) };
        (arc.schedule)();
    }
    fn wake_by_ref(ptr: *const ()) {
        let arc = unsafe { Arc::<SchedWaker>::from_raw(ptr as *const SchedWaker) };
        (arc.schedule)();
        std::mem::forget(arc);
    }
    fn drop_waker(ptr: *const ()) {
        let _ = unsafe { Arc::<SchedWaker>::from_raw(ptr as *const SchedWaker) };
    }
    fn raw(arc: Arc<SchedWaker>) -> RawWaker {
        static VTABLE: RawWakerVTable = RawWakerVTable::new(clone, wake, wake_by_ref, drop_waker);
        let ptr = Arc::into_raw(arc) as *const ();
        RawWaker::new(ptr, &VTABLE)
    }
    let arc = Arc::new(SchedWaker { schedule });
    unsafe { Waker::from_raw(raw(arc)) }
}

struct InlineState {
    slots: HashMap<u64, InlineSlot>,
    rng: Option<XorShift64>,
    next: u64,
}

struct InlineSlot {
    job: Option<Job>,
    schedule: Arc<dyn Fn() + Send + Sync>,
}

/// Single-threaded executor. `seed` shuffles the ready batch so tests can
/// shake ordering. `None` keeps spawn order.
pub struct InlineExecutor {
    state: Mutex<InlineState>,
    ready: ReadyQueue,
}

impl InlineExecutor {
    pub fn new(seed: Option<u64>) -> Self {
        Self {
            state: Mutex::new(InlineState {
                slots: HashMap::new(),
                rng: seed.map(XorShift64::new),
                next: 1,
            }),
            ready: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn poll_one(&self, id: u64) {
        let (job, schedule) = {
            let mut state = lock(&self.state);
            let Some(slot) = state.slots.get_mut(&id) else {
                return;
            };
            let schedule = Arc::clone(&slot.schedule);
            (slot.job.take(), schedule)
        };
        let Some(mut job) = job else {
            return;
        };
        if job.cancelled() {
            let mut state = lock(&self.state);
            state.slots.remove(&id);
            return;
        }
        let waker = waker_for(schedule);
        let mut cx = Context::from_waker(&waker);
        match job.poll(&mut cx) {
            Poll::Ready(()) => {
                let mut state = lock(&self.state);
                state.slots.remove(&id);
            }
            Poll::Pending => {
                let mut state = lock(&self.state);
                if job.cancelled() {
                    state.slots.remove(&id);
                    return;
                }
                if let Some(slot) = state.slots.get_mut(&id) {
                    slot.job = Some(job);
                }
            }
        }
    }
}

impl Executor for InlineExecutor {
    fn spawn(&self, job: Job) {
        let id = {
            let mut state = lock(&self.state);
            let id = state.next;
            state.next = state.next.wrapping_add(1);
            id
        };
        let ready = Arc::clone(&self.ready);
        let wake = job.wake_slot();
        let schedule: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
            lock(&ready).push(id);
            if let Some(wake) = &wake {
                lock(wake).wake();
            }
        });
        job.set_nudge(Arc::clone(&schedule));
        {
            let mut state = lock(&self.state);
            state.slots.insert(
                id,
                InlineSlot {
                    job: Some(job),
                    schedule: Arc::clone(&schedule),
                },
            );
        }
        schedule();
    }

    fn poll_inline(&self) -> bool {
        let mut batch = mem_take_ready(&self.ready);
        if batch.is_empty() {
            return false;
        }
        {
            let mut state = lock(&self.state);
            if let Some(rng) = &mut state.rng {
                rng.shuffle(&mut batch);
            }
        }
        for id in batch {
            self.poll_one(id);
        }
        true
    }

    fn has_ready(&self) -> bool {
        !lock(&self.ready).is_empty()
    }
}

fn mem_take_ready(ready: &ReadyQueue) -> Vec<u64> {
    let mut batch = std::mem::take(&mut *lock(ready));
    dedup_ids(&mut batch);
    batch
}

fn dedup_ids(ids: &mut Vec<u64>) {
    let mut seen = HashSet::with_capacity(ids.len());
    ids.retain(|id| seen.insert(*id));
}

struct XorShift64(u64);

impl XorShift64 {
    fn new(seed: u64) -> Self {
        let mixed = seed
            .wrapping_mul(0x9E37_79B9_7F4A_7C15)
            .wrapping_add(0x6A09_E667_F3BC_C909);
        Self(if mixed == 0 {
            0xD1B5_4A32_D192_ED03
        } else {
            mixed
        })
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            let j = (self.next_u64() as usize) % (i + 1);
            items.swap(i, j);
        }
    }
}

#[derive(Default)]
struct LocalTasks {
    tasks: HashMap<u64, LocalTask>,
}

struct LocalTask {
    future: Option<Pin<Box<dyn Future<Output = ()>>>>,
    shared: Arc<TaskShared>,
    runtime: Weak<RuntimeInner>,
    schedule: Arc<dyn Fn() + Send + Sync>,
}

pub(crate) fn install(world: &mut World) {
    world.insert_resource(Time::default());
    world.insert_resource(EventHub::default());
    world.insert_resource(Runtime::new());
    world.insert_resource(Exec(Arc::new(InlineExecutor::new(None))));
    world.insert_local(LocalTasks::default());
}

pub(crate) fn set_clock(world: &World, clock: Arc<dyn Clock>) {
    *lock(&runtime(world).inner.clock) = clock;
}

pub(crate) fn set_wake(world: &World, wake: Arc<dyn Wake>) {
    *lock(&runtime(world).inner.wake) = wake;
}

pub(crate) fn set_executor(world: &mut World, executor: Arc<dyn Executor>) {
    world.insert_resource(Exec(executor));
}

fn exec(world: &World) -> Arc<dyn Executor> {
    Arc::clone(
        &world
            .resource::<Exec>()
            .expect("executor installed by App")
            .0,
    )
}

fn wrap_output<T, F>(
    runtime: &Runtime,
    shared: Arc<TaskShared>,
    future: F,
) -> impl Future<Output = ()>
where
    T: Event,
    F: Future<Output = T>,
{
    let inbox = Arc::clone(&runtime.inner.inbox);
    let wake = Arc::clone(&runtime.inner.wake);
    let shared2 = Arc::clone(&shared);
    async move {
        let value = future.await;
        if shared2.is_cancelled() {
            return;
        }
        lock(&inbox).push(Delivery {
            shared: Arc::clone(&shared2),
            emit: Box::new(move |world| event::send(world, value)),
        });
        lock(&wake).wake();
    }
}

/// Spawn `future` on the background [`Executor`].
/// The output is committed as an event on the UI thread unless the handle
/// was dropped or [`TaskHandle::cancel`]led first.
pub fn spawn<T, F>(world: &mut World, future: F) -> TaskHandle
where
    T: Event,
    F: Future<Output = T> + Send + 'static,
{
    let runtime = runtime(world);
    let shared = TaskShared::new();
    let wrapped = wrap_output(&runtime, Arc::clone(&shared), future);
    let job = Job {
        future: Box::pin(wrapped),
        runtime: runtime.downgrade(),
        shared: Arc::clone(&shared),
    };
    exec(world).spawn(job);
    TaskHandle::new(shared)
}

/// Spawn `future` on the thread that pumps the app.
/// The future may own non-`Send` state. The output still has to be an [`Event`].
pub fn spawn_local<T, F>(world: &mut World, future: F) -> TaskHandle
where
    T: Event,
    F: Future<Output = T> + 'static,
{
    let runtime = runtime(world);
    let shared = TaskShared::new();
    let wrapped = wrap_output(&runtime, Arc::clone(&shared), future);
    let id = runtime.alloc_id();
    let ready = Arc::clone(&runtime.inner.local_ready);
    let wake = runtime.wake_slot();
    let schedule: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
        lock(&ready).push(id);
        lock(&wake).wake();
    });
    shared.set_nudge(Arc::clone(&schedule));
    world
        .local_mut::<LocalTasks>()
        .expect("local tasks")
        .tasks
        .insert(
            id,
            LocalTask {
                future: Some(Box::pin(wrapped)),
                shared: Arc::clone(&shared),
                runtime: runtime.downgrade(),
                schedule: Arc::clone(&schedule),
            },
        );
    schedule();
    TaskHandle::new(shared)
}

fn poll_local(world: &mut World) -> bool {
    let runtime = runtime(world);
    let batch = mem_take_ready(&runtime.inner.local_ready);
    if batch.is_empty() {
        return false;
    }
    for id in batch {
        let taken = {
            let local = world.local_mut::<LocalTasks>().expect("local tasks");
            let Some(task) = local.tasks.get_mut(&id) else {
                continue;
            };
            if task.shared.is_cancelled() {
                local.tasks.remove(&id);
                continue;
            }
            let future = task.future.take();
            let schedule = Arc::clone(&task.schedule);
            let shared = Arc::clone(&task.shared);
            let weak = Weak::clone(&task.runtime);
            Some((future, schedule, shared, weak))
        };
        let Some((future, schedule, shared, weak)) = taken else {
            continue;
        };
        let Some(mut future) = future else {
            continue;
        };
        let Some(inner) = weak.upgrade() else {
            world
                .local_mut::<LocalTasks>()
                .expect("local tasks")
                .tasks
                .remove(&id);
            continue;
        };
        if shared.is_cancelled() {
            world
                .local_mut::<LocalTasks>()
                .expect("local tasks")
                .tasks
                .remove(&id);
            continue;
        }
        let waker = waker_for(schedule);
        let mut cx = Context::from_waker(&waker);
        let polled = {
            let _guard = enter(&inner);
            catch_unwind(AssertUnwindSafe(|| future.as_mut().poll(&mut cx)))
        };
        let keep = matches!(polled, Ok(Poll::Pending) if !shared.is_cancelled());
        let local = world.local_mut::<LocalTasks>().expect("local tasks");
        if keep {
            if let Some(task) = local.tasks.get_mut(&id) {
                task.future = Some(future);
            }
        } else {
            local.tasks.remove(&id);
        }
    }
    true
}

enum TimerKind {
    Once(Option<SendOnce>),
    Repeat(SendFn),
    Wake(Option<Waker>),
}

struct Timer {
    when: Duration,
    period: Option<Duration>,
    cancel: Arc<AtomicBool>,
    kind: TimerKind,
    burst: u8,
}

/// Cancels the timer when dropped.
#[must_use = "dropping a TimerHandle cancels the timer; call detach() to keep it"]
pub struct TimerHandle {
    cancel: Arc<AtomicBool>,
    detached: bool,
}

impl TimerHandle {
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::Acquire)
    }

    /// Keep firing after this handle is dropped.
    pub fn detach(mut self) {
        self.detached = true;
    }
}

impl Drop for TimerHandle {
    fn drop(&mut self) {
        if !self.detached {
            self.cancel.store(true, Ordering::Release);
        }
    }
}

fn arm_timer(
    world: &mut World,
    delay: Duration,
    period: Option<Duration>,
    cancel: Arc<AtomicBool>,
    kind: TimerKind,
) -> TimerHandle {
    let rt = runtime(world);
    let when = rt.elapsed().saturating_add(delay);
    lock(&rt.inner.timers).push(Timer {
        when,
        period,
        cancel: Arc::clone(&cancel),
        kind,
        burst: 0,
    });
    rt.wake_app();
    TimerHandle {
        cancel,
        detached: false,
    }
}

/// Emit `event` once `delay` has passed on the clock.
pub fn after<T: Event>(world: &mut World, delay: Duration, event: T) -> TimerHandle {
    let cancel = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&cancel);
    arm_timer(
        world,
        delay,
        None,
        cancel,
        TimerKind::Once(Some(Box::new(move |world| {
            if !flag.load(Ordering::Acquire) {
                event::send(world, event);
            }
        }))),
    )
}

/// Emit a clone of `event` every `period`. `period` must be non-zero.
/// A pump delivers at most 64 missed repeats, then jumps one period past `now`.
pub fn every<T: Event + Clone>(world: &mut World, period: Duration, event: T) -> TimerHandle {
    assert!(!period.is_zero(), "every() period must be non-zero");
    let cancel = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&cancel);
    arm_timer(
        world,
        period,
        Some(period),
        cancel,
        TimerKind::Repeat(Box::new(move |world| {
            if !flag.load(Ordering::Acquire) {
                event::send(world, event.clone());
            }
        })),
    )
}

/// Park the current task until `duration` has passed on the app clock.
pub fn sleep(duration: Duration) -> Sleep {
    Sleep {
        duration,
        deadline: None,
        cancel: Arc::new(AtomicBool::new(false)),
    }
}

/// Future returned by [`sleep`].
pub struct Sleep {
    duration: Duration,
    deadline: Option<Duration>,
    cancel: Arc<AtomicBool>,
}

impl Future for Sleep {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        let this = self.get_mut();
        if this.cancel.load(Ordering::Acquire) {
            return Poll::Ready(());
        }
        let inner = current_runtime();
        let now = lock(&inner.clock).elapsed();
        let deadline = *this.deadline.get_or_insert_with(|| {
            if this.duration.is_zero() {
                now
            } else {
                now.saturating_add(this.duration)
            }
        });
        if this.duration.is_zero() || now >= deadline {
            return Poll::Ready(());
        }
        Runtime { inner }.arm_wake(deadline, cx.waker().clone(), Arc::clone(&this.cancel));
        Poll::Pending
    }
}

impl Drop for Sleep {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Release);
    }
}

fn fire_timers(world: &mut World) -> bool {
    let now = runtime(world).elapsed();
    let due = {
        let rt = runtime(world);
        let mut timers = lock(&rt.inner.timers);
        timers.retain(|timer| !timer.cancel.load(Ordering::Acquire));
        let mut due = Vec::new();
        let mut index = 0;
        while index < timers.len() {
            if timers[index].when <= now {
                due.push(timers.swap_remove(index));
            } else {
                index += 1;
            }
        }
        due
    };
    if due.is_empty() {
        return false;
    }
    let mut repeats = Vec::new();
    for mut timer in due {
        if timer.cancel.load(Ordering::Acquire) {
            continue;
        }
        let period = timer.period;
        let mut kind = TimerKind::Wake(None);
        std::mem::swap(&mut kind, &mut timer.kind);
        match kind {
            TimerKind::Once(Some(callback)) => callback(world),
            TimerKind::Repeat(callback) => {
                callback(world);
                let alive = !timer.cancel.load(Ordering::Acquire);
                if alive {
                    if let Some(period) = period {
                        if !period.is_zero() {
                            timer.burst = timer.burst.saturating_add(1);
                            if timer.burst >= TIMER_BURST {
                                timer.burst = 0;
                                timer.when = now.saturating_add(period);
                            } else {
                                timer.when = timer.when.saturating_add(period);
                            }
                            timer.kind = TimerKind::Repeat(callback);
                            repeats.push(timer);
                        }
                    }
                }
            }
            TimerKind::Wake(Some(waker)) => waker.wake(),
            TimerKind::Once(None) | TimerKind::Wake(None) => {}
        }
    }
    if !repeats.is_empty() {
        lock(&runtime(world).inner.timers).extend(repeats);
    }
    true
}

struct DebounceTable<K> {
    slots: HashMap<K, DebounceSlot>,
}

struct DebounceSlot {
    id: u64,
    #[allow(dead_code)]
    timer: TimerHandle,
    task: Option<TaskHandle>,
    make: Option<LocalOnce>,
}

fn arm_debounce<K>(world: &mut World, key: K, delay: Duration, id: u64, make: LocalOnce)
where
    K: Eq + std::hash::Hash + 'static,
{
    if !world.has_local::<DebounceTable<K>>() {
        world.insert_local(DebounceTable::<K> {
            slots: HashMap::new(),
        });
    }
    let cancel = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&cancel);
    let timer = arm_timer(
        world,
        delay,
        None,
        cancel,
        TimerKind::Once(Some(Box::new(move |world| {
            if flag.load(Ordering::Acquire) {
                return;
            }
            fire_debounce::<K>(world, id);
        }))),
    );
    let table = world.local_mut::<DebounceTable<K>>().expect("debounce");
    table.slots.insert(
        key,
        DebounceSlot {
            id,
            timer,
            task: None,
            make: Some(make),
        },
    );
}

fn fire_debounce<K>(world: &mut World, id: u64)
where
    K: Eq + std::hash::Hash + 'static,
{
    let make = {
        let Some(table) = world.local_mut::<DebounceTable<K>>() else {
            return;
        };
        let Some(slot) = table.slots.values_mut().find(|slot| slot.id == id) else {
            return;
        };
        slot.make.take()
    };
    if let Some(make) = make {
        make(world);
    }
}

fn stash_task<K>(world: &mut World, id: u64, handle: TaskHandle)
where
    K: Eq + std::hash::Hash + 'static,
{
    let Some(table) = world.local_mut::<DebounceTable<K>>() else {
        return;
    };
    if let Some(slot) = table.slots.values_mut().find(|slot| slot.id == id) {
        slot.task = Some(handle);
    }
}

/// Reset a named timer. When `delay` passes with no further call for `key`,
/// `make` runs on the UI thread and its future is spawned in the background.
/// A later call for the same key cancels the pending timer and any in-flight task.
pub fn debounce<K, T, Fut>(
    world: &mut World,
    key: K,
    delay: Duration,
    make: impl FnOnce() -> Fut + 'static,
) where
    K: Eq + std::hash::Hash + 'static,
    T: Event,
    Fut: Future<Output = T> + Send + 'static,
{
    let id = runtime(world).alloc_id();
    arm_debounce(
        world,
        key,
        delay,
        id,
        Box::new(move |world| {
            let fut = make();
            let handle = spawn(world, fut);
            stash_task::<K>(world, id, handle);
        }),
    );
}

/// Like [`debounce`], but the future runs on the UI thread.
pub fn debounce_local<K, T, Fut>(
    world: &mut World,
    key: K,
    delay: Duration,
    make: impl FnOnce() -> Fut + 'static,
) where
    K: Eq + std::hash::Hash + 'static,
    T: Event,
    Fut: Future<Output = T> + 'static,
{
    let id = runtime(world).alloc_id();
    arm_debounce(
        world,
        key,
        delay,
        id,
        Box::new(move |world| {
            let fut = make();
            let handle = spawn_local(world, fut);
            stash_task::<K>(world, id, handle);
        }),
    );
}

/// Drop a pending debounce, its timer, and any in-flight task.
pub fn cancel_debounce<K>(world: &mut World, key: &K)
where
    K: Eq + std::hash::Hash + 'static,
{
    if let Some(table) = world.local_mut::<DebounceTable<K>>() {
        table.slots.remove(key);
    }
}

/// Fire due timers and poll until every task is parked, then commit results
/// that were not cancelled into the readable event buffers.
/// Does not advance [`Time`] and does not run systems.
pub fn run_until_parked(world: &mut World) {
    for _ in 0..MAX_STEPS {
        let fired = fire_timers(world);
        let background = exec(world).poll_inline();
        let local = poll_local(world);
        if !fired && !background && !local {
            break;
        }
    }
    drain_inbox(world);
    event::merge_events(world);
}

fn drain_inbox(world: &mut World) {
    let inbox = Arc::clone(&runtime(world).inner.inbox);
    let pending = std::mem::take(&mut *lock(&inbox));
    for delivery in pending {
        if delivery.shared.is_cancelled() {
            continue;
        }
        (delivery.emit)(world);
    }
}

pub(crate) fn tick_time(world: &mut World) {
    let now = runtime(world).elapsed();
    let time = world.resource_mut::<Time>().expect("time");
    let delta = now.saturating_sub(time.now);
    time.delta = delta;
    time.now = now;
    time.frame = time.frame.saturating_add(1);
}

pub(crate) fn next_deadline(world: &World) -> Option<std::time::Instant> {
    let rt = runtime(world);
    let when = rt.soonest()?;
    let clock = Arc::clone(&*lock(&rt.inner.clock));
    Some(clock.instant(when))
}

pub(crate) fn wants_frame(world: &World) -> bool {
    let rt = runtime(world);
    if !lock(&rt.inner.inbox).is_empty() {
        return true;
    }
    if !lock(&rt.inner.local_ready).is_empty() {
        return true;
    }
    if exec(world).has_ready() {
        return true;
    }
    if let Some(when) = rt.soonest() {
        if when <= rt.elapsed() {
            return true;
        }
    }
    false
}
