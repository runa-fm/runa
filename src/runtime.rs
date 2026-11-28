//! Runtime module for runa.
//!
//! Provides the main event loop and orchestrates the application state and workers.
//! Sits above `app` and `ui` and ties them together.

mod root;
mod startup;
mod terminal;

pub(crate) use root::RunaRoot;
pub(crate) use startup::start;
pub(crate) use terminal::{POLL_INTERVAL, run_terminal};
