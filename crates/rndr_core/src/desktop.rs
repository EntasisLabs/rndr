//! Several windows in one process, and messages between them.
//!
//! Each window keeps its own [`crate::App`]. [`Desktop`] is the handle a
//! system uses to address another window. The desktop runner installs one
//! into every window before the first frame and delivers queued messages at
//! the start of the receiver's next frame.
//!
//! [`Desktop::broadcast`] skips the sender. [`Desktop::send`] delivers even
//! when the target is the sender. [`Desktop::open`] asks the runner to create
//! another window on the event-loop thread. A headless [`crate::App`] has no
//! [`Desktop`] until a runner, or [`Desktop::pair`] in a test, inserts one.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crate::app::App;
use crate::event::{self, Event};
use crate::runtime::Wake;
use crate::window::WindowConfig;
use crate::world::World;

/// Identity of one window in a session. The first window the runner opens is 1.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct WindowId(pub u64);

/// A value sent from one window to another.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowMsg<T> {
    /// Window that called [`Desktop::send`] or [`Desktop::broadcast`].
    pub from: WindowId,
    /// Payload.
    pub body: T,
}

impl<T: Event> Event for WindowMsg<T> {}

struct Delivery {
    to: Option<WindowId>,
    from: WindowId,
    deliver: Box<dyn Fn(&mut World) + Send>,
}

struct OpenRequest {
    config: WindowConfig,
    build: Box<dyn FnOnce(&mut App) + Send>,
}

struct Hub {
    next: AtomicU64,
    mail: Mutex<Vec<Delivery>>,
    opens: Mutex<Vec<OpenRequest>>,
    wake: Mutex<Option<Arc<dyn Wake>>>,
}

impl Hub {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            next: AtomicU64::new(1),
            mail: Mutex::new(Vec::new()),
            opens: Mutex::new(Vec::new()),
            wake: Mutex::new(None),
        })
    }

    fn alloc(self: &Arc<Self>) -> WindowId {
        WindowId(self.next.fetch_add(1, Ordering::Relaxed))
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|err| err.into_inner())
}

/// Handle stored on each window. Clone shares the session bus.
#[derive(Clone)]
pub struct Desktop {
    hub: Arc<Hub>,
    id: WindowId,
}

impl std::fmt::Debug for Desktop {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Desktop")
            .field("id", &self.id)
            .finish()
    }
}

impl Desktop {
    /// Two handles on one bus, for a test that does not open a window.
    ///
    /// The ids are 1 and 2. Messages stay queued until [`deliver`] is called
    /// on the receiver's world.
    pub fn pair() -> (Self, Self) {
        let hub = Hub::new();
        let first = Self {
            id: hub.alloc(),
            hub: Arc::clone(&hub),
        };
        let second = Self {
            id: hub.alloc(),
            hub,
        };
        (first, second)
    }

    /// This window's id.
    pub fn id(&self) -> WindowId {
        self.id
    }

    /// Queue `body` for `to`. The receiver reads [`WindowMsg`] on its next frame.
    ///
    /// Wakes the desktop loop when a runner has installed one.
    pub fn send<T: Event + Clone>(&self, to: WindowId, body: T) {
        self.enqueue(Some(to), body);
    }

    /// Queue `body` for every window except this one.
    pub fn broadcast<T: Event + Clone>(&self, body: T) {
        self.enqueue(None, body);
    }

    /// Ask the runner to open a window.
    ///
    /// `build` runs on the event-loop thread after the runner has inserted this
    /// session's [`Desktop`] and `config`. It does not run in a headless test.
    /// The new window's first frame is later, after the OS window exists.
    pub fn open(&self, config: WindowConfig, build: impl FnOnce(&mut App) + Send + 'static) {
        lock(&self.hub.opens).push(OpenRequest {
            config,
            build: Box::new(build),
        });
        self.wake();
    }

    fn enqueue<T: Event + Clone>(&self, to: Option<WindowId>, body: T) {
        let from = self.id;
        lock(&self.hub.mail).push(Delivery {
            to,
            from,
            deliver: Box::new(move |world| {
                event::send(
                    world,
                    WindowMsg {
                        from,
                        body: body.clone(),
                    },
                );
            }),
        });
        self.wake();
    }

    fn wake(&self) {
        if let Some(wake) = lock(&self.hub.wake).clone() {
            wake.wake();
        }
    }
}

/// [`Desktop`] for this world, if a runner or [`Desktop::pair`] installed one.
pub fn desktop(world: &World) -> Option<Desktop> {
    world.resource::<Desktop>().cloned()
}

/// Move queued messages for `id` into `world`'s event writer.
///
/// Call this before [`crate::App::update`] so the frame flip makes
/// [`WindowMsg`] readable. A broadcast is delivered when `from` is not `id`.
/// A direct message is delivered when its target is `id`. Undelivered messages
/// stay queued.
pub fn deliver(world: &mut World, id: WindowId) {
    let Some(desktop) = world.resource::<Desktop>().cloned() else {
        return;
    };
    let mut waiting = Vec::new();
    let mut mine = Vec::new();
    for delivery in lock(&desktop.hub.mail).drain(..) {
        let hit = match delivery.to {
            Some(target) => target == id,
            None => delivery.from != id,
        };
        if hit {
            mine.push(delivery);
        } else {
            waiting.push(delivery);
        }
    }
    *lock(&desktop.hub.mail) = waiting;
    for delivery in mine {
        (delivery.deliver)(world);
    }
}

/// A window the runner has not created yet.
pub struct OpenWindow {
    /// Config inserted before `build` runs.
    pub config: WindowConfig,
    /// Builds the new window's world on the event-loop thread.
    pub build: Box<dyn FnOnce(&mut App) + Send>,
}

/// Bus shared by every window in a session. The runner owns one.
pub struct WindowHub {
    hub: Arc<Hub>,
}

impl WindowHub {
    /// An empty session. The first [`Self::attach`] receives id 1.
    pub fn new() -> Self {
        Self { hub: Hub::new() }
    }

    /// A [`Desktop`] with a fresh id, sharing this bus.
    pub fn attach(&self) -> Desktop {
        Desktop {
            id: self.hub.alloc(),
            hub: Arc::clone(&self.hub),
        }
    }

    /// Wake the loop when a window sends mail or asks to open another window.
    pub fn set_wake(&self, wake: Arc<dyn Wake>) {
        *lock(&self.hub.wake) = Some(wake);
    }

    /// Open requests queued since the last call, in order.
    pub fn take_opens(&self) -> Vec<OpenWindow> {
        lock(&self.hub.opens)
            .drain(..)
            .map(|request| OpenWindow {
                config: request.config,
                build: request.build,
            })
            .collect()
    }
}

impl Default for WindowHub {
    fn default() -> Self {
        Self::new()
    }
}

/// Wake used by a headless test. [`Desktop::send`] records that it fired.
#[derive(Clone, Default)]
pub struct CountingWake {
    hits: Arc<Mutex<u32>>,
}

impl CountingWake {
    /// A wake that has not fired.
    pub fn new() -> Self {
        Self::default()
    }

    /// Times [`Wake::wake`] has run.
    pub fn hits(&self) -> u32 {
        *lock(&self.hits)
    }
}

impl Wake for CountingWake {
    fn wake(&self) {
        *lock(&self.hits) += 1;
    }
}

/// Install `wake` on the [`Desktop`] in `world`, if there is one.
///
/// The desktop runner calls [`WindowHub::set_wake`] instead. A test that built
/// a pair with [`Desktop::pair`] can call this so [`Desktop::send`] is counted.
pub fn install_wake(world: &World, wake: Arc<dyn Wake>) {
    if let Some(desktop) = world.resource::<Desktop>() {
        *lock(&desktop.hub.wake) = Some(wake);
    }
}
