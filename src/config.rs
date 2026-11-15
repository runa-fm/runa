//! Configuration options for runa
//!
//! This module holds the submodules and structs necessary to load and represent
//! configuration options for runa, including display settings, input keybindings,

pub(crate) mod assets;
pub(crate) mod commands;
pub(crate) mod dialog_layout;
pub(crate) mod display;
pub(crate) mod editor;
mod general;
pub(crate) mod keys;
pub(crate) mod load;
mod presets;
pub(crate) mod theme;

pub(crate) use commands::Commands;
pub(crate) use display::Display;
pub(crate) use editor::Editor;
pub(crate) use general::{General, InternalGeneral};
pub(crate) use keys::Keys;
pub(crate) use load::Config;
pub(crate) use theme::Theme;
