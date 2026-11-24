//! Timings for runa.
//!
//! Holds the throttle times, delays and message durations used across the codebase,
//! and the Throttler that enforces throttle times.

use std::time::{Duration, Instant};

/// Throttle intervals enforced by [Throttler].
pub(crate) mod throttle {
    use std::time::Duration;

    pub(crate) const PREVIEW_REQUEST: Duration = Duration::from_millis(30);
    pub(crate) const NAV: Duration = Duration::from_millis(15);
    pub(crate) const FILE_INFO: Duration = Duration::from_millis(60);
    pub(crate) const CONFIG_RELOAD: Duration = Duration::from_secs(1);
    pub(crate) const UI_RELOAD: Duration = Duration::from_millis(200);
}

/// Debounce delays, indicator thresholds, and command timeouts.
pub(crate) mod delay {
    use std::time::Duration;

    pub(crate) const PREVIEW_DEBOUNCE: Duration = Duration::from_millis(35);
    pub(crate) const FS_WATCH_DEBOUNCE: Duration = Duration::from_millis(150);
    pub(crate) const WORKER_INDICATOR: Duration = Duration::from_millis(200);
    pub(crate) const PREVIEW_COMMAND_TIMEOUT: Duration = Duration::from_secs(2);
}

/// Overlay message display durations.
pub(crate) mod message {
    use std::time::Duration;

    pub(crate) const SHORT: Duration = Duration::from_secs(2);
    pub(crate) const DEFAULT: Duration = Duration::from_secs(3);
    pub(crate) const LONG: Duration = Duration::from_secs(5);
    pub(crate) const WORKER_ERROR: Duration = Duration::from_secs(7);
}

/// Throttler to handle timings and debounce for relevant actions.
/// Tracks latest timings and whether a request is queued for later.
#[derive(Default, Debug, Clone)]
pub(crate) struct Throttler {
    last_timing: Option<Instant>,
    queued: bool,
}

impl Throttler {
    pub(crate) const fn new() -> Self {
        Self {
            last_timing: None,
            queued: false,
        }
    }

    /// Check if the throttler can trigger an action based on the last timing.
    pub(crate) fn can_trigger(&self, interval: Duration) -> bool {
        let now = Instant::now();
        match self.last_timing {
            Some(prev) => now.duration_since(prev) >= interval,
            None => true,
        }
    }

    /// Leading Edge: Try to trigger an action, returning whether it was allowed or not.
    /// true if a call is allowed, false if it is queued for later.
    /// A refused call is remembered and can be taken later with `take_queued`.
    pub(crate) fn try_trigger(&mut self, interval: Duration) -> bool {
        let allowed = self.can_trigger(interval);
        if !allowed {
            self.queued = true;
        }
        allowed
    }

    /// Trailing Edge: Check if a queued action can be taken based on the last timing.
    /// Take a queued action if it is allowed based on the last timing.
    pub(crate) fn take_queued(&mut self, interval: Duration) -> bool {
        if self.queued && self.can_trigger(interval) {
            self.queued = false;
            return true;
        }
        false
    }

    /// Update the last timings to now and reset queued state.
    pub(crate) fn touch(&mut self) {
        self.last_timing = Some(Instant::now());
        self.queued = false;
    }
}
