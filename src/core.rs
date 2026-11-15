//! Core runtime logic for runa.
//!
//! This module contains the non-UI pieces used by runa:
//! - [fm]: directory traversal (see [browse_dir](fm::browse_dir), [FileEntry]).
//! - [listing]: sorting and filtering logic.
//! - [workers]: background work and message passing back into the RunaRoot struct.
//! - [preview]: file previews and archive tree rows.
//! - [proc]: process management for running external commands like `bat`, `fd`.
//! - [metadata]: file metadata extraction and caching, including file properties.
//! - [cache]: caching of FileEntry data for pane rendering.
//! - [command]: editor and user command expansion: file variables and system opener.

pub(crate) mod cache;
pub(crate) mod command;
pub(crate) mod fm;
pub(crate) mod listing;
pub(crate) mod metadata;
pub(crate) mod preview;
pub(crate) mod proc;
pub(crate) mod workers;

pub(crate) use fm::FileEntry;
pub(crate) use listing::Listing;
pub(crate) use proc::FindResult;
