//! Values produced off the UI thread, delivered as events on the next frame.
//!
//! [`open`] returns a [`Feed`] and a [`Sender`]. The sender enqueues from a
//! standard thread, from a future on the installed [`Executor`](crate::Executor),
//! or from any other runtime the application already runs. The frame pump
//! drains the queue on the UI thread and commits each value with
//! [`send`](crate::send), before Input, so an Update system reads it with
//! [`events`](crate::events) the same way it reads a task result.
//!
//! The first enqueue after a drain wakes the window. Further enqueues, until
//! that drain, do not. A burst between frames is one redraw. A send that is
//! rejected, closed, or stamped with an older [`Epoch`] does not wake.
//!
//! The queue has a fixed capacity. [`OnFull::Wait`] blocks the sender until
//! the UI thread takes a value (backpressure). [`OnFull::DropOldest`] discards
//! the front of the queue. [`OnFull::Reject`] returns [`FeedError::Full`].
//! [`FeedConfig::latest`] keeps one slot and replaces it, which is the shape
//! of a search result. There is no unbounded mode.
//!
//! [`Feed::advance`] publishes a new epoch and drops anything still queued.
//! [`Sender::send_at`] refuses a value stamped with an older epoch, so a slow
//! reply cannot overwrite a newer query. Dropping the [`Feed`], or calling
//! [`Feed::cancel`], closes the sender, unblocks a waiting `send`, and wakes
//! [`Sender::cancelled`]. The producer is told to stop. It is not killed: a
//! thread has to return from [`Sender::wait_until_cancelled`] or from `send`,
//! and a future has to finish or be cancelled with the task handle.
//!
//! Two feeds of the same value type share one event queue. Give each stream
//! its own type when the system has to tell them apart.
//!
//! A reader that must not block, and that applies a fixed budget of bytes on
//! the UI thread, should keep that buffer beside the parser. Waiting on this
//! queue stalls the producer. Dropping values loses output.
//!
//! # Replace a list
//!
//! Capture the epoch when the query starts. Advance it when the text changes.
//! A reply from the previous query is dropped and does not wake the window.
//!
//! ```
//! use rndr_core::{events, feed, Event, FeedConfig, FeedError, Sim};
//!
//! #[derive(Debug, PartialEq, Eq)]
//! struct Hits(Vec<&'static str>);
//! impl Event for Hits {}
//!
//! let mut sim = Sim::new();
//! let (results, tx) = feed::open(sim.world(), FeedConfig::latest());
//! let stale = tx.epoch();
//! tx.send_at(stale, Hits(vec!["old"])).unwrap();
//! let epoch = results.advance();
//! assert_eq!(
//!     tx.send_at(stale, Hits(vec!["late"])),
//!     Err(FeedError::Stale)
//! );
//! tx.send_at(epoch, Hits(vec!["new"])).unwrap();
//! sim.update();
//! assert_eq!(events::<Hits>(sim.world()), &[Hits(vec!["new"])]);
//! drop(results);
//! ```
//!
//! # Append a stream
//!
//! [`FeedConfig::queue`] keeps order. The three chunks below arrive in one
//! frame and wake the window once.
//!
//! ```
//! use rndr_core::{events, feed, Event, FeedConfig, Sim};
//!
//! struct Chunk(&'static str);
//! impl Event for Chunk {}
//!
//! let mut sim = Sim::new();
//! let (chunks, tx) = feed::open(sim.world(), FeedConfig::queue(8));
//! tx.send(Chunk("Hello")).unwrap();
//! tx.send(Chunk(", ")).unwrap();
//! tx.send(Chunk("world")).unwrap();
//! sim.update();
//! let mut text = String::new();
//! for chunk in events::<Chunk>(sim.world()) {
//!     text.push_str(chunk.0);
//! }
//! assert_eq!(text, "Hello, world");
//! drop(chunks);
//! ```

use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::task::{Context, Poll, Waker};

use crate::event::{self, Event};
use crate::runtime::{self, TaskHandle, WakeSlot};
use crate::world::World;

const DEFAULT_CAPACITY: usize = 64;

/// What `send` does when the queue is at [`FeedConfig::capacity`].
///
/// [`FeedConfig::latest`] does not consult this policy. It replaces the one
/// waiting value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OnFull {
    /// Block until the UI thread takes a value, or the feed is closed.
    ///
    /// [`Sender::try_send`] does not block. It returns [`FeedError::Full`].
    ///
    /// Do not call [`Sender::send`] with this policy from the UI thread, or
    /// from a future polled on that thread, while the queue is full. The
    /// queue is drained on that same thread, so the wait never ends. A worker
    /// thread, or a future on the desktop thread pool, can block: the UI
    /// thread is the one that drains.
    Wait,
    /// Drop the oldest queued value until there is room, then enqueue.
    DropOldest,
    /// Leave the queue unchanged and return [`FeedError::Full`].
    Reject,
}

/// Bound and overflow policy for [`open`].
///
/// The default is a queue of 64 with [`OnFull::Wait`]: a burst of stream
/// chunks fits in one frame, and a stalled UI thread stops the producer at
/// 64 values instead of letting the queue grow.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FeedConfig {
    /// Maximum queued values. Numbers below 1 are treated as 1.
    /// Ignored when [`latest`](Self::latest) is set.
    pub capacity: usize,
    /// Behavior at [`capacity`](Self::capacity). Ignored when `latest` is set.
    pub on_full: OnFull,
    /// Keep one value. A newer send replaces the value waiting for this frame,
    /// and the extra sends do not wake the window.
    pub latest: bool,
}

impl FeedConfig {
    /// Ordered queue of `capacity` values.
    ///
    /// [`Sender::send`] blocks when the queue is full.
    /// [`Sender::try_send`] returns [`FeedError::Full`].
    /// `capacity` below 1 becomes 1.
    pub fn queue(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            on_full: OnFull::Wait,
            latest: false,
        }
    }

    /// Ordered queue with an explicit full policy.
    ///
    /// `capacity` below 1 becomes 1. [`OnFull::Wait`] blocks [`Sender::send`]
    /// and makes [`Sender::try_send`] return [`FeedError::Full`].
    pub fn bounded(capacity: usize, on_full: OnFull) -> Self {
        Self {
            capacity: capacity.max(1),
            on_full,
            latest: false,
        }
    }

    /// One slot. A newer value replaces whatever is still waiting.
    ///
    /// A burst between drains delivers the newest value and requests one
    /// redraw. This is the search-result shape. Pair it with
    /// [`Feed::advance`] so a slow reply from the previous query is dropped.
    pub fn latest() -> Self {
        Self {
            capacity: 1,
            on_full: OnFull::DropOldest,
            latest: true,
        }
    }
}

impl Default for FeedConfig {
    fn default() -> Self {
        Self::queue(DEFAULT_CAPACITY)
    }
}

/// Why a value was not queued.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FeedError {
    /// The [`Feed`] was dropped or [`Feed::cancel`]led.
    Closed,
    /// The queue is at capacity.
    ///
    /// [`OnFull::Reject`] returns this from [`Sender::send`] and
    /// [`Sender::try_send`]. [`OnFull::Wait`] returns it only from
    /// [`Sender::try_send`].
    Full,
    /// The value's [`Epoch`] is not the feed's current epoch.
    ///
    /// The value was not queued and the window was not woken.
    Stale,
    /// [`spawn_tokio`] found no running tokio runtime on this thread.
    #[cfg(feature = "tokio")]
    NoRuntime,
}

impl std::fmt::Display for FeedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Closed => "feed is closed",
            Self::Full => "feed is full",
            Self::Stale => "feed epoch is stale",
            #[cfg(feature = "tokio")]
            Self::NoRuntime => "no tokio runtime is running on this thread",
        })
    }
}

impl std::error::Error for FeedError {}

/// Token captured when a unit of work starts.
///
/// [`Feed::advance`] publishes the next token and drops queued values from
/// the previous one. [`Sender::send_at`] delivers a value only when its token
/// is still current. A new feed starts at 1.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Epoch(u64);

impl Epoch {
    /// Numeric value. A new feed starts at 1.
    pub const fn number(self) -> u64 {
        self.0
    }
}

struct Gate {
    closed: AtomicBool,
    pending: AtomicBool,
    dead: AtomicBool,
    space: Condvar,
    wake: WakeSlot,
    waker: Mutex<Option<Waker>>,
}

struct State<T> {
    items: VecDeque<Stamped<T>>,
    epoch: u64,
    closed: bool,
}

struct Stamped<T> {
    epoch: u64,
    value: T,
}

struct Shared<T> {
    state: Mutex<State<T>>,
    gate: Arc<Gate>,
    capacity: usize,
    on_full: OnFull,
    latest: bool,
}

/// UI-side handle. Dropping it tells the producer to stop.
///
/// The handle is [`Send`] and [`Sync`]. Store it in a resource. Drop, or
/// [`cancel`](Self::cancel), closes every [`Sender`], drops values that have
/// not been delivered, unblocks [`Sender::send`], and wakes
/// [`Sender::cancelled`]. A thread blocked in its own I/O has to notice
/// [`Sender::is_cancelled`] itself.
///
/// [`spawn_task`] also cancels that task. With the `tokio` feature,
/// `spawn_tokio` aborts that task. [`spawn_thread`] does not join the thread.
#[must_use = "dropping a Feed closes the sender and tells the producer to stop"]
pub struct Feed<T> {
    shared: Arc<Shared<T>>,
    task: Mutex<Option<TaskHandle>>,
    stop: Mutex<Option<Box<dyn FnOnce() + Send>>>,
}

/// Enqueues values from any thread.
///
/// Cloning copies the handle. The feed stays open until the [`Feed`] is
/// dropped or [`Feed::cancel`]led, even if every sender has been dropped.
pub struct Sender<T> {
    shared: Arc<Shared<T>>,
}

/// Resolves when the [`Feed`] is dropped or [`Feed::cancel`]led.
///
/// One waiter is stored. A later poll of [`Sender::cancelled`] replaces it.
/// [`Sender::wait_until_cancelled`] does not use this slot, so a thread and
/// one future can wait at the same time.
#[derive(Clone)]
pub struct Cancelled {
    gate: Arc<Gate>,
}

impl std::fmt::Debug for Cancelled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Cancelled")
            .field("closed", &self.gate.closed.load(Ordering::Acquire))
            .finish()
    }
}

impl Future for Cancelled {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        if self.gate.closed.load(Ordering::Acquire) {
            return Poll::Ready(());
        }
        *runtime::lock(&self.gate.waker) = Some(cx.waker().clone());
        if self.gate.closed.load(Ordering::Acquire) {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    }
}

impl<T> std::fmt::Debug for Feed<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Feed")
            .field("epoch", &self.epoch().number())
            .field("cancelled", &self.is_cancelled())
            .finish()
    }
}

impl<T> std::fmt::Debug for Sender<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Sender")
            .field("epoch", &self.epoch().number())
            .field("cancelled", &self.is_cancelled())
            .finish()
    }
}

impl<T> Clone for Sender<T> {
    fn clone(&self) -> Self {
        Self {
            shared: Arc::clone(&self.shared),
        }
    }
}

impl<T> Feed<T> {
    /// Publish a new epoch and drop values still waiting from the old one.
    ///
    /// Returns the epoch a new unit of work should stamp. A send that was
    /// already woken may still produce an empty frame; it does not deliver
    /// the discarded values. This does not wake by itself.
    ///
    /// A closed feed returns its current epoch and stays closed.
    pub fn advance(&self) -> Epoch {
        let mut state = runtime::lock(&self.shared.state);
        if !state.closed {
            state.epoch = state.epoch.saturating_add(1);
            state.items.clear();
            self.shared.gate.pending.store(false, Ordering::Release);
        }
        let epoch = Epoch(state.epoch);
        drop(state);
        self.shared.gate.space.notify_all();
        epoch
    }

    /// Epoch a new unit of work would capture.
    ///
    /// This is the same value as [`Sender::epoch`].
    pub fn epoch(&self) -> Epoch {
        Epoch(runtime::lock(&self.shared.state).epoch)
    }

    /// Close the sender and tell the producer to stop.
    ///
    /// Drop does the same thing. A second call is a no-op. Queued values are
    /// discarded and are not delivered.
    pub fn cancel(&self) {
        self.shutdown();
    }

    /// Whether [`cancel`](Self::cancel) has run, or the feed was dropped.
    pub fn is_cancelled(&self) -> bool {
        self.shared.gate.closed.load(Ordering::Acquire)
    }

    fn shutdown(&self) {
        {
            let mut state = runtime::lock(&self.shared.state);
            state.closed = true;
            state.items.clear();
            self.shared.gate.closed.store(true, Ordering::Release);
            self.shared.gate.dead.store(true, Ordering::Release);
            self.shared.gate.pending.store(false, Ordering::Release);
        }
        self.shared.gate.space.notify_all();
        if let Some(waker) = runtime::lock(&self.shared.gate.waker).take() {
            waker.wake();
        }
        if let Some(stop) = runtime::lock(&self.stop).take() {
            stop();
        }
        if let Some(task) = runtime::lock(&self.task).take() {
            task.cancel();
        }
    }
}

impl<T> Drop for Feed<T> {
    fn drop(&mut self) {
        self.shutdown();
    }
}

impl<T> Sender<T> {
    /// Epoch at this moment.
    ///
    /// Capture it when the work starts and pass it to [`send_at`](Self::send_at).
    /// [`send`](Self::send) stamps the epoch at the moment the value is queued,
    /// which can be later than the query that produced it if the send blocked.
    pub fn epoch(&self) -> Epoch {
        Epoch(runtime::lock(&self.shared.state).epoch)
    }

    /// Whether the [`Feed`] has been dropped or [`Feed::cancel`]led.
    pub fn is_cancelled(&self) -> bool {
        self.shared.gate.closed.load(Ordering::Acquire)
    }

    /// Completes when the feed is closed.
    ///
    /// The future is executor-agnostic. One waiter is remembered; a later
    /// `cancelled` poll replaces it. See [`Cancelled`].
    pub fn cancelled(&self) -> Cancelled {
        Cancelled {
            gate: Arc::clone(&self.shared.gate),
        }
    }

    /// Block until the feed is closed.
    ///
    /// A thread that has no other place to wait should call this after its
    /// last send, or instead of sleeping. Returns immediately when the feed
    /// is already closed. Safe to call from a worker. Calling it on the UI
    /// thread deadlocks: nothing there can drop the feed while this waits.
    pub fn wait_until_cancelled(&self) {
        let mut state = runtime::lock(&self.shared.state);
        while !self.shared.gate.closed.load(Ordering::Acquire) {
            state = self
                .shared
                .gate
                .space
                .wait(state)
                .unwrap_or_else(|err| err.into_inner());
        }
    }

    /// Enqueue `value` at the epoch current when it enters the queue.
    ///
    /// Blocks if the policy is [`OnFull::Wait`] and the queue is full.
    /// See [`OnFull::Wait`] for the deadlock that causes on the UI thread.
    ///
    /// A burst into a non-empty queue does not wake the window. The first
    /// value after a drain does.
    pub fn send(&self, value: T) -> Result<(), FeedError> {
        self.enqueue(None, value, true)
    }

    /// Enqueue `value` if `epoch` is still current.
    ///
    /// Returns [`FeedError::Stale`] without waking when [`Feed::advance`] has
    /// moved on. The epoch is the one [`epoch`](Self::epoch) returned when
    /// this unit of work started.
    ///
    /// Blocks under the same rules as [`send`](Self::send). If the epoch
    /// changes while a [`OnFull::Wait`] send is blocked, this returns
    /// [`FeedError::Stale`] and does not queue the value.
    pub fn send_at(&self, epoch: Epoch, value: T) -> Result<(), FeedError> {
        self.enqueue(Some(epoch.0), value, true)
    }

    /// [`send`](Self::send) without blocking.
    ///
    /// Returns [`FeedError::Full`] when the queue is at capacity and the
    /// policy is [`OnFull::Wait`] or [`OnFull::Reject`].
    /// [`OnFull::DropOldest`] and [`FeedConfig::latest`] make room instead.
    pub fn try_send(&self, value: T) -> Result<(), FeedError> {
        self.enqueue(None, value, false)
    }

    /// [`send_at`](Self::send_at) without blocking.
    pub fn try_send_at(&self, epoch: Epoch, value: T) -> Result<(), FeedError> {
        self.enqueue(Some(epoch.0), value, false)
    }

    fn enqueue(&self, stamp: Option<u64>, value: T, block: bool) -> Result<(), FeedError> {
        let mut value = Some(value);
        let mut state = runtime::lock(&self.shared.state);
        loop {
            if state.closed || self.shared.gate.closed.load(Ordering::Acquire) {
                return Err(FeedError::Closed);
            }
            if let Some(stamp) = stamp {
                if stamp != state.epoch {
                    return Err(FeedError::Stale);
                }
            }
            let epoch = stamp.unwrap_or(state.epoch);
            let cap = self.shared.capacity.max(1);
            if self.shared.latest || state.items.len() < cap {
                let wake_now = enqueue_in(
                    &mut state,
                    &self.shared.gate,
                    epoch,
                    value.take().expect("value"),
                    self.shared.latest,
                );
                drop(state);
                if wake_now {
                    wake(&self.shared.gate);
                }
                return Ok(());
            }
            match self.shared.on_full {
                OnFull::Reject => return Err(FeedError::Full),
                OnFull::Wait if !block => return Err(FeedError::Full),
                OnFull::DropOldest => {
                    let wake_now = enqueue_dropping(
                        &mut state,
                        &self.shared.gate,
                        epoch,
                        value.take().expect("value"),
                        cap,
                    );
                    drop(state);
                    if wake_now {
                        wake(&self.shared.gate);
                    }
                    return Ok(());
                }
                OnFull::Wait => {
                    state = self
                        .shared
                        .gate
                        .space
                        .wait(state)
                        .unwrap_or_else(|err| err.into_inner());
                }
            }
        }
    }
}

fn enqueue_in<T>(state: &mut State<T>, gate: &Gate, epoch: u64, value: T, replace: bool) -> bool {
    let was_empty = state.items.is_empty();
    if replace {
        state.items.clear();
    }
    state.items.push_back(Stamped { epoch, value });
    if was_empty {
        gate.pending.store(true, Ordering::Release);
    }
    was_empty
}

fn enqueue_dropping<T>(
    state: &mut State<T>,
    gate: &Gate,
    epoch: u64,
    value: T,
    cap: usize,
) -> bool {
    let was_empty = state.items.is_empty();
    while state.items.len() >= cap {
        state.items.pop_front();
    }
    state.items.push_back(Stamped { epoch, value });
    if was_empty {
        gate.pending.store(true, Ordering::Release);
    }
    was_empty
}

fn wake(gate: &Gate) {
    runtime::lock(&gate.wake).wake();
}

fn take_values<T>(shared: &Shared<T>) -> Vec<T> {
    let mut state = runtime::lock(&shared.state);
    if state.closed {
        state.items.clear();
        shared.gate.pending.store(false, Ordering::Release);
        drop(state);
        shared.gate.space.notify_all();
        return Vec::new();
    }
    let epoch = state.epoch;
    let items = std::mem::take(&mut state.items);
    shared.gate.pending.store(false, Ordering::Release);
    drop(state);
    shared.gate.space.notify_all();
    items
        .into_iter()
        .filter(|item| item.epoch == epoch)
        .map(|item| item.value)
        .collect()
}

trait FeedLane: Send + Sync {
    fn pending(&self) -> bool;
    fn dead(&self) -> bool;
    fn drain(&self, world: &mut World);
}

struct Lane<T: Event> {
    shared: Arc<Shared<T>>,
}

impl<T: Event> FeedLane for Lane<T> {
    fn pending(&self) -> bool {
        self.shared.gate.pending.load(Ordering::Acquire)
    }

    fn dead(&self) -> bool {
        self.shared.gate.dead.load(Ordering::Acquire)
    }

    fn drain(&self, world: &mut World) {
        for value in take_values(&self.shared) {
            event::send(world, value);
        }
    }
}

#[derive(Default)]
struct Hub {
    lanes: Vec<Arc<dyn FeedLane>>,
}

pub(crate) fn install(world: &mut World) {
    world.insert_resource(Hub::default());
}

pub(crate) fn pending(world: &World) -> bool {
    world
        .resource::<Hub>()
        .is_some_and(|hub| hub.lanes.iter().any(|lane| lane.pending()))
}

pub(crate) fn drain(world: &mut World) {
    let lanes = {
        let Some(hub) = world.resource_mut::<Hub>() else {
            return;
        };
        hub.lanes.retain(|lane| !lane.dead());
        hub.lanes.clone()
    };
    for lane in lanes {
        if !lane.dead() {
            lane.drain(world);
        }
    }
}

fn open_shared<T: Event>(world: &mut World, config: FeedConfig) -> (Feed<T>, Sender<T>) {
    let shared = Arc::new(Shared {
        state: Mutex::new(State {
            items: VecDeque::new(),
            epoch: 1,
            closed: false,
        }),
        gate: Arc::new(Gate {
            closed: AtomicBool::new(false),
            pending: AtomicBool::new(false),
            dead: AtomicBool::new(false),
            space: Condvar::new(),
            wake: runtime::wake_slot(world),
            waker: Mutex::new(None),
        }),
        capacity: config.capacity.max(1),
        on_full: config.on_full,
        latest: config.latest,
    });
    let lane: Arc<dyn FeedLane> = Arc::new(Lane {
        shared: Arc::clone(&shared),
    });
    {
        let hub = world
            .resource_mut::<Hub>()
            .expect("feed hub installed by App");
        hub.lanes.retain(|lane| !lane.dead());
        hub.lanes.push(lane);
    }
    let feed = Feed {
        shared: Arc::clone(&shared),
        task: Mutex::new(None),
        stop: Mutex::new(None),
    };
    let sender = Sender { shared };
    (feed, sender)
}

/// Register a feed. The sender can be handed to any thread or runtime.
///
/// Values queued with the sender are delivered as `T` events on the UI
/// thread during the next pump. See the [module docs](self) for the queue,
/// the epoch, and the wake rules.
///
/// # Panics
///
/// Panics if `world` has no runtime or feed hub. [`crate::App::new`]
/// installs both.
pub fn open<T: Event>(world: &mut World, config: FeedConfig) -> (Feed<T>, Sender<T>) {
    open_shared(world, config)
}

/// Spawn a standard thread that receives the sender.
///
/// The thread is detached. `work` is told to stop when the returned feed is
/// dropped: [`Sender::is_cancelled`] becomes true, a blocked
/// [`Sender::send`] returns [`FeedError::Closed`], and
/// [`Sender::wait_until_cancelled`] returns. `work` has to return. This
/// function does not join the thread.
///
/// The thread is named `rndr-feed`.
///
/// # Panics
///
/// Panics if `world` has no runtime, or if the thread cannot be spawned.
pub fn spawn_thread<T, F>(world: &mut World, config: FeedConfig, work: F) -> Feed<T>
where
    T: Event,
    F: FnOnce(Sender<T>) + Send + 'static,
{
    let (feed, tx) = open(world, config);
    std::thread::Builder::new()
        .name("rndr-feed".into())
        .spawn(move || work(tx))
        .expect("feed thread");
    feed
}

/// Spawn `work` on the installed [`Executor`](crate::Executor).
///
/// `work` runs on the UI thread when it builds the future. The future is
/// polled on the executor. Dropping the feed cancels that task the same way
/// [`crate::spawn`] does, and closes the sender.
///
/// The future should stop when [`Sender::is_cancelled`] is set or
/// [`Sender::cancelled`] resolves. A [`OnFull::Wait`] send from a future
/// polled on the UI thread can deadlock; see [`OnFull::Wait`].
///
/// # Panics
///
/// Panics if `world` has no runtime or executor. [`crate::App::new`]
/// installs both.
///
/// # Example
///
/// ```
/// use rndr_core::{events, feed, Event, FeedConfig, Sim};
///
/// #[derive(Debug, PartialEq, Eq)]
/// struct Chunk(&'static str);
/// impl Event for Chunk {}
///
/// let mut sim = Sim::new();
/// let handle = feed::spawn_task(sim.world(), FeedConfig::queue(4), |tx| async move {
///     tx.send(Chunk("a")).unwrap();
///     tx.send(Chunk("b")).unwrap();
/// });
/// sim.update();
/// let text: String = events::<Chunk>(sim.world()).iter().map(|chunk| chunk.0).collect();
/// assert_eq!(text, "ab");
/// drop(handle);
/// ```
pub fn spawn_task<T, F, Fut>(world: &mut World, config: FeedConfig, work: F) -> Feed<T>
where
    T: Event,
    F: FnOnce(Sender<T>) -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    let (feed, tx) = open(world, config);
    let task = runtime::spawn_detached(world, work(tx));
    *runtime::lock(&feed.task) = Some(task);
    feed
}

/// Spawn `work` on the tokio runtime running on this thread.
///
/// The application starts that runtime. This function does not create one.
/// Dropping the feed aborts the task and closes the sender. Enable the
/// `tokio` feature to call this.
///
/// Returns [`FeedError::NoRuntime`] when this thread is outside a runtime.
/// `work` is not called in that case.
///
/// # Panics
///
/// Panics if `world` has no runtime. [`crate::App::new`] installs one.
#[cfg(feature = "tokio")]
pub fn spawn_tokio<T, F, Fut>(
    world: &mut World,
    config: FeedConfig,
    work: F,
) -> Result<Feed<T>, FeedError>
where
    T: Event,
    F: FnOnce(Sender<T>) -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    let runtime = tokio::runtime::Handle::try_current().map_err(|_| FeedError::NoRuntime)?;
    let (feed, tx) = open(world, config);
    let join = runtime.spawn(work(tx));
    *runtime::lock(&feed.stop) = Some(Box::new(move || join.abort()));
    Ok(feed)
}
