//! The general configuration settings for runa.
//!
//! This module defines the [General] struct for deserializing
//! general settings from the runa.toml configuration file
//! and the [InternalGeneral] struct for internal use within runa.
//!
//! It includes settings such as display options, case sensitivity,
//! and file handling preferences.

use std::collections::HashSet;
use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::Arc;

use serde::Deserialize;

/// The minimum results which is set to if the maximum is overset in the runa.toml.
const MIN_FIND_RESULTS: usize = 15;
/// The maximum find result limit which is possible.
/// Can be set higher, but better to set it to a big limit instead of usize::MAX
const MAX_FIND_RESULTS_LIMIT: usize = 1000000;
/// Find result limit when nothing is configured.
const DEFAULT_FIND_RESULTS: usize = 20000;

#[derive(Deserialize, Debug)]
#[serde(default)]
pub(crate) struct General {
    dirs_first: bool,
    show_hidden: bool,
    show_symlink: bool,
    show_system: bool,
    case_insensitive: bool,
    always_show: Vec<String>,
    #[serde(default = "default_find_results")]
    max_find_results: usize,
    move_to_trash: bool,
    startup: StartupConfig,
}

impl Default for General {
    fn default() -> Self {
        General {
            dirs_first: true,
            show_hidden: true,
            show_symlink: true,
            show_system: false,
            case_insensitive: true,
            always_show: Vec::new(),
            max_find_results: DEFAULT_FIND_RESULTS,
            move_to_trash: true,
            startup: StartupConfig::default(),
        }
    }
}

#[derive(Debug)]
pub(crate) struct InternalGeneral {
    dirs_first: bool,
    show_hidden: bool,
    show_symlink: bool,
    show_system: bool,
    case_insensitive: bool,
    always_show: Arc<HashSet<OsString>>,
    max_find_results: usize,
    move_to_trash: bool,
    startup: InternalStartup,
}

impl From<General> for InternalGeneral {
    fn from(g: General) -> Self {
        let set = g
            .always_show
            .into_iter()
            .map(OsString::from)
            .collect::<HashSet<_>>();

        let internal_startup = InternalStartup {
            tabs: g.startup.tabs.into_iter().map(PathBuf::from).collect(),
        };

        Self {
            dirs_first: g.dirs_first,
            show_hidden: g.show_hidden,
            show_symlink: g.show_symlink,
            show_system: g.show_system,
            case_insensitive: g.case_insensitive,
            always_show: Arc::new(set),
            max_find_results: clamp_find_results(g.max_find_results),
            move_to_trash: g.move_to_trash,
            startup: internal_startup,
        }
    }
}

impl InternalGeneral {
    crate::getters! {
        dirs_first: bool,
        show_hidden: bool,
        show_symlink: bool,
        show_system: bool,
        case_insensitive: bool,
        always_show: &Arc<HashSet<OsString>>,
        max_find_results: usize,
        move_to_trash: bool,
    }

    #[inline]
    pub(crate) fn startup_tabs(&self) -> &[PathBuf] {
        &self.startup.tabs
    }
}

#[derive(Deserialize, Debug, Default)]
#[serde(default)]
pub(super) struct StartupConfig {
    pub(super) tabs: Vec<String>,
}

#[derive(Debug, Default)]
struct InternalStartup {
    pub(crate) tabs: Vec<PathBuf>,
}

/// Helper function for default max_find_results
const fn default_find_results() -> usize {
    DEFAULT_FIND_RESULTS
}

fn clamp_find_results(value: usize) -> usize {
    let clamped = value.clamp(MIN_FIND_RESULTS, MAX_FIND_RESULTS_LIMIT);
    if clamped != value {
        eprintln!(
            "[Warning] max_find_results={} out of range ({}..={}), clamped to {}",
            value, MIN_FIND_RESULTS, MAX_FIND_RESULTS_LIMIT, clamped
        );
    }
    clamped
}
