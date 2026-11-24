//! Miscellaneous utility functions for runa.
//!
//! Holds generic helpers used across the runa codebase:
//! - [path]: expanding, normalizing, and and shortening paths for display.
//! - [text]: the [StrBuffer](text::StrBuffer) string type.
//! - [os]: OS-specific helpers.
//! - [timings]: named duration constants and the throttler.
//! - [ansi]: ansi escape parser for previews.

pub(crate) mod ansi;
pub(crate) mod os;
pub(crate) mod path;
pub(crate) mod text;
pub(crate) mod timings;

#[macro_use]
pub(crate) mod macros;

pub(crate) use timings::Timings;
