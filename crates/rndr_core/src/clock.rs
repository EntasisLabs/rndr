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
    /// [`Clock::elapsed`] copied at the start of this frame.
    pub now: Duration,
    /// Change in [`Time::now`] since the previous frame. The first update
    /// reports the whole elapsed time, because the resource starts at zero.
    pub delta: Duration,
    /// [`crate::App::update`] calls that have started, including this one.
    pub frame: u64,
}

/// Source of [`Time::now`]. Also maps a timestamp onto a wall [`std::time::Instant`]
/// so the window loop can wait until the next timer.
pub trait Clock: Send + Sync + 'static {
    /// Monotonic time since this clock's origin.
    ///
    /// [`crate::App::update`] copies it into [`Time::now`] at the start of the frame.
    fn elapsed(&self) -> Duration;

    /// Wall time corresponding to `at` on this clock.
    ///
    /// [`SystemClock`] adds `at` to its construction instant, so an earlier
    /// timestamp stays in the past. [`ManualClock`] returns the current wall
    /// time once `at` has been reached, and that wall time plus the remaining
    /// delay while `at` is still ahead.
    fn instant(&self, at: Duration) -> std::time::Instant;
}

/// `Instant::now()` sampled against construction.
///
/// [`Clock::elapsed`] is how long since [`SystemClock::new`]. The desktop
/// runner installs this clock.
#[derive(Clone, Debug)]
pub struct SystemClock {
    start: std::time::Instant,
}

impl SystemClock {
    /// Sample the wall clock. Elapsed time is zero at this call.
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
///
/// [`crate::Sim`] installs one so a test can step time without sleeping.
#[derive(Debug, Default)]
pub struct ManualClock {
    nanos: AtomicU64,
}

impl ManualClock {
    /// Clock frozen at zero until [`ManualClock::advance`].
    pub fn new() -> Self {
        Self {
            nanos: AtomicU64::new(0),
        }
    }

    /// Add `by` to the elapsed time. The total saturates at `u64::MAX` nanoseconds.
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
