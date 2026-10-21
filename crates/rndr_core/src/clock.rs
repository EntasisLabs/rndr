//! Monotonic [`Time`] driven by a [`Clock`].
//!
//! The desktop runner installs [`SystemClock`]. Tests install [`ManualClock`]
//! through [`crate::Sim`] and move it with [`ManualClock::advance`]. Nothing
//! in core sleeps.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Monotonic app time. `frame` is how many [`crate::App::update`] calls have
/// started. `delta` is the change in `now` since the previous one, and the
/// full elapsed time on the first update.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Time {
    pub now: Duration,
    pub delta: Duration,
    pub frame: u64,
}

/// Source of [`Time::now`]. Also maps a timestamp onto a wall [`std::time::Instant`]
/// so the window loop can wait until the next timer.
pub trait Clock: Send + Sync + 'static {
    fn elapsed(&self) -> Duration;

    /// Wall time corresponding to `at` on this clock.
    fn instant(&self, at: Duration) -> std::time::Instant;
}

/// `Instant::now()` sampled against construction.
#[derive(Clone, Debug)]
pub struct SystemClock {
    start: std::time::Instant,
}

impl SystemClock {
    pub fn new() -> Self {
        Self {
            start: std::time::Instant::now(),
        }
    }
}

impl Default for SystemClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for SystemClock {
    fn elapsed(&self) -> Duration {
        self.start.elapsed()
    }

    fn instant(&self, at: Duration) -> std::time::Instant {
        self.start + at
    }
}

/// Clock that moves only when [`ManualClock::advance`] is called.
#[derive(Debug, Default)]
pub struct ManualClock {
    nanos: AtomicU64,
}

impl ManualClock {
    pub fn new() -> Self {
        Self {
            nanos: AtomicU64::new(0),
        }
    }

    pub fn advance(&self, by: Duration) {
        let add = as_nanos_u64(by);
        let _ = self
            .nanos
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |cur| {
                Some(cur.saturating_add(add))
            });
    }

    /// Shared handle for a runtime and a test driver.
    pub fn handle() -> Arc<Self> {
        Arc::new(Self::new())
    }
}

impl Clock for ManualClock {
    fn elapsed(&self) -> Duration {
        Duration::from_nanos(self.nanos.load(Ordering::Relaxed))
    }

    fn instant(&self, at: Duration) -> std::time::Instant {
        let now = self.elapsed();
        let wall = std::time::Instant::now();
        if at > now {
            wall + (at - now)
        } else {
            wall
        }
    }
}

pub(crate) fn as_nanos_u64(duration: Duration) -> u64 {
    u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX)
}
