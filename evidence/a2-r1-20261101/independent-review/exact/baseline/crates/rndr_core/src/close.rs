//! Opt-in asynchronous close protection. See `docs/safe-close.md` in the repository.

use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(1);

/// Opaque identity for one protection attempt. Not interchangeable across gates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CloseRequest(u64);

fn next_request() -> CloseRequest {
    CloseRequest(
        NEXT.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .expect("close request identities exhausted"),
    )
}

/// App-visible close state. Failure never authorizes closing.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum CloseState {
    /// No active attempt, or the user cancelled it.
    #[default]
    Idle,
    /// Freeze mutations, drain pending writes, then resolve this exact request.
    Pending(CloseRequest),
    /// Keep the window and draft alive; display the error and offer retry/cancel/discard.
    Failed(CloseRequest, String),
    /// Protection succeeded, or the user explicitly chose informed discard.
    Ready(CloseRequest),
}

/// Insert before `run` to intercept native window close and `Input::quit`.
///
/// The runner requests protection, but never performs or claims durability of IO.
/// Systems observe [`Self::state`], start/drain work once per request, and call
/// [`Self::complete`] on the UI thread (for example from a task result event).
/// Do not replace/remove this resource during a close. A missing gate opts out.
/// Never resolve success until *all* text that closing would destroy is protected.
/// Freeze editing during Pending/Ready, or call [`Self::invalidate`] on *every*
/// mutation before the runner can consume Ready. Tokens alone cannot detect edits.
#[derive(Debug, Default)]
pub struct CloseGuard {
    state: CloseState,
}

impl CloseGuard {
    /// Current state, including an error suitable for app-owned failure UI.
    pub fn state(&self) -> &CloseState {
        &self.state
    }

    /// Request closing. Repeated requests coalesce, including after failure.
    pub fn request(&mut self) -> CloseRequest {
        match self.state {
            CloseState::Idle => {
                let request = next_request();
                self.state = CloseState::Pending(request);
                request
            }
            CloseState::Pending(r) | CloseState::Failed(r, _) | CloseState::Ready(r) => r,
        }
    }

    /// Resolve only the currently pending attempt. Stale/duplicate results return false.
    /// An error retains all app state and requires an explicit retry or decision.
    pub fn complete(&mut self, request: CloseRequest, result: Result<(), String>) -> bool {
        if self.state != CloseState::Pending(request) {
            return false;
        }
        self.state = match result {
            Ok(()) => CloseState::Ready(request),
            Err(error) => CloseState::Failed(request, error),
        };
        true
    }

    /// Retry a failed attempt with a fresh token. Old results cannot resolve it.
    pub fn retry(&mut self, failed: CloseRequest) -> Option<CloseRequest> {
        if !matches!(self.state, CloseState::Failed(r, _) if r == failed) {
            return None;
        }
        let request = next_request();
        self.state = CloseState::Pending(request);
        Some(request)
    }

    /// Cancel closing (not the IO itself). Every outstanding result becomes stale.
    pub fn cancel(&mut self) {
        self.state = CloseState::Idle;
    }

    /// Invalidate protection after a mutation; re-drain using a fresh token if closing.
    /// This must happen before the runner consumes a successful completion.
    pub fn invalidate(&mut self) {
        if !matches!(self.state, CloseState::Idle) {
            self.state = CloseState::Pending(next_request());
        }
    }

    /// Authorize loss only after app UI explains the failed protection and the user
    /// confirms discard. Only the exact displayed failure can be discarded.
    /// This is deliberately separate from successful protection.
    pub fn discard(&mut self, failed: CloseRequest) -> bool {
        if !matches!(self.state, CloseState::Failed(r, _) if r == failed) {
            return false;
        }
        self.state = CloseState::Ready(failed);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failure_duplicate_retry_and_stale_completions() {
        let mut g = CloseGuard::default();
        let r = g.request();
        assert_eq!(g.request(), r);
        assert!(g.complete(r, Err("disk full".into())));
        assert_eq!(g.request(), r); // native double-close must not retry or discard
        assert!(!g.complete(r, Ok(())));
        let retry = g.retry(r).unwrap();
        assert_ne!(r, retry);
        assert!(!g.complete(r, Ok(())));
        assert!(!g.discard(r));
        assert!(g.complete(retry, Ok(())));
        assert!(!g.complete(retry, Err("late".into())));
    }

    #[test]
    fn cancel_invalidate_and_foreign_tokens() {
        let mut g = CloseGuard::default();
        let mut other = CloseGuard::default();
        let r = g.request();
        assert!(!other.complete(r, Ok(())));
        g.cancel();
        assert!(!g.complete(r, Ok(())));
        let r = g.request();
        assert!(g.complete(r, Ok(())));
        g.invalidate();
        assert!(!g.complete(r, Ok(())));
        assert!(matches!(g.state(), CloseState::Pending(new) if *new != r));
        let foreign = other.request();
        assert!(!g.complete(foreign, Ok(())));
    }

    #[test]
    fn discard_requires_current_failure() {
        let mut g = CloseGuard::default();
        let r = g.request();
        assert!(!g.discard(r));
        g.complete(r, Err("permission denied".into()));
        assert!(g.discard(r));
        g.cancel();
        assert!(!g.discard(r));
    }
}
