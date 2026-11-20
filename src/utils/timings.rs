//! Timings for runa.
//!
//! Holds the throttle times, delays and message durations used across the codebase,
//! and the Throttler that enforces throttle times.

use std::time::{Duration, Instant};

/// Timings constants for various actions.
pub(crate) struct Timings;

impl Timings {
    // Throttlers
    /// Minimum time between two preview requests.
    /// Refused requests are sent once the time has passed.
    pub(crate) const PREVIEW_REQUEST_MS: u64 = 30;
    /// Minimum time between two navigation moves that refresh the preview
    /// and file info right away.
    pub(crate) const NAV_THROTTLE_MS: u64 = 15;
    /// Minimum time between two file info requests.
    pub(crate) const FILE_INFO_THROTTLE_MS: u64 = 60;
    /// Minimum time between two config reloads.
    pub(crate) const CONFIG_RELOAD_MS: u64 = 1000;
    /// Minimum time between two manual UI reloads.
    pub(crate) const UI_RELOAD_MS: u64 = 200;

    // Delays
    /// Time a pending preview refresh waits before it is requested.
    pub(crate) const PREVIEW_DEBOUNCE_MS: u64 = 35;
    /// Time the watcher collects file system events
    /// before it reports the changed directories.
    pub(crate) const FS_WATCH_DEBOUNCE_MS: u64 = 150;
    /// Time a file operation runs before the status line shows it.
    pub(crate) const WORKER_INDICATOR_MS: u64 = 200;
    /// Time a preview command it runs before it is stopped.
    pub(crate) const PREVIEW_COMMAND_TIMEOUT_MS: u64 = 2000;

    // Message durations
    /// How long short confirmations stay visible.
    pub(crate) const MESSAGE_SHORT: Duration = Duration::from_secs(2);
    /// Default message and errors duration.
    pub(crate) const MESSAGE: Duration = Duration::from_secs(3);
    /// How long important messages stay visible.
    pub(crate) const MESSAGE_LONG: Duration = Duration::from_secs(5);
    /// How long worker errors stay visible.
    pub(crate) const MESSAGE_WORKER_ERROR: Duration = Duration::from_secs(7);
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

    /// Check if the throttler can trigger an action based on the last timing
    pub(crate) fn can_trigger(&self, ms: u64) -> bool {
        let now = Instant::now();
        match self.last_timing {
            Some(prev) => now.duration_since(prev) >= Duration::from_millis(ms),
            None => true,
        }
    }

    /// Leading Edge: Try to trigger an action, returning whether it was allowed or not.
    /// true if a call is allowed, false if it is queued for later.
    /// A refused call is remembered and can be taken later with `take_queued`.
    pub(crate) fn try_trigger(&mut self, ms: u64) -> bool {
        let allowed = self.can_trigger(ms);
        if !allowed {
            self.queued = true;
        }
        allowed
    }

    /// Trailing Edge: Check if a queued action can be taken based on the last timing.
    /// Take a queued action if it is allowed based on the last timing.
    pub(crate) fn take_queued(&mut self, ms: u64) -> bool {
        if self.queued && self.can_trigger(ms) {
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
