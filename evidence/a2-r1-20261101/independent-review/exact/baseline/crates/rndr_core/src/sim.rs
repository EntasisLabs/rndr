//! Single-threaded driver for headless tests.
//!
//! [`Sim`] installs a [`ManualClock`] and an [`InlineExecutor`]. [`Sim::advance`]
//! moves the clock. [`Sim::run_until_parked`] polls until every task is waiting
//! on a timer (or has finished) and commits results that were not cancelled.
//! [`Sim::update`] does that and then runs the app schedules. Neither call sleeps.
//!
//! ```
//! use std::time::Duration;
//!
//! use rndr_core::{after, events, spawn, Event, Sim};
//!
//! #[derive(Debug, PartialEq, Eq)]
//! struct QueryResult(&'static str);
//! impl Event for QueryResult {}
//!
//! let mut sim = Sim::new();
//! let handle = spawn(sim.world(), async { QueryResult("palette") });
//! sim.update();
//! assert_eq!(events::<QueryResult>(sim.world())[0].0, "palette");
//! drop(handle);
//!
//! #[derive(Clone, Copy, Debug, PartialEq, Eq)]
//! struct Tick;
//! impl Event for Tick {}
//!
//! let timer = after(sim.world(), Duration::from_millis(40), Tick);
//! sim.advance(Duration::from_millis(39));
//! sim.update();
//! assert!(events::<Tick>(sim.world()).is_empty());
//! sim.advance(Duration::from_millis(1));
//! sim.update();
//! assert_eq!(events::<Tick>(sim.world()), &[Tick]);
//! drop(timer);
//! ```

use std::sync::Arc;
use std::time::Duration;

use crate::app::App;
use crate::clock::ManualClock;
use crate::runtime::{self, InlineExecutor, NopWake};
use crate::world::World;

/// Headless app plus a fake clock and inline executor.
pub struct Sim {
    /// Headless [`App`](crate::App) driven by the methods below.
    pub app: App,
    clock: Arc<ManualClock>,
}

impl Sim {
    /// Manual clock at zero, [`NopWake`], and an [`InlineExecutor`] that
    /// keeps spawn order.
    pub fn new() -> Self {
        Self::with_executor(None)
    }

    /// Same as [`new`](Self::new), but ready background tasks are shuffled with `seed`.
    pub fn with_seed(seed: u64) -> Self {
        Self::with_executor(Some(seed))
    }

    fn with_executor(seed: Option<u64>) -> Self {
        let mut app = App::new();
        let clock = Arc::new(ManualClock::new());
        app.set_clock(Arc::clone(&clock) as Arc<dyn crate::clock::Clock>);
        app.set_wake(Arc::new(NopWake));
        app.set_executor(Arc::new(InlineExecutor::new(seed)));
        Self { app, clock }
    }

    /// The app [`World`](crate::World).
    pub fn world(&mut self) -> &mut World {
        self.app.world_mut()
    }

    /// Move the fake clock forward. Timers notice on the next pump.
    pub fn advance(&mut self, by: Duration) {
        self.clock.advance(by);
    }

    /// Poll tasks and commit events. Does not run systems or tick [`crate::Time`]
    /// beyond what a later [`update`](Self::update) does.
    pub fn run_until_parked(&mut self) {
        runtime::run_until_parked(self.app.world_mut());
    }

    /// Tick [`crate::Time`], pump tasks and timers, then run schedules.
    ///
    /// This calls [`App::update`](crate::App::update).
    pub fn update(&mut self) {
        self.app.update();
    }
}

impl Default for Sim {
    fn default() -> Self {
        Self::new()
    }
}
