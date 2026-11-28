//! Startup orchestrator.
//!
//! Resolves CLI paths or startup tabs and builds the initial [AppContainer].
//! Attaches any skipped tab warnings before entering terminal loop.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::app::{AppContainer, AppState, MAX_TABS};
use crate::config::Config;
use crate::core::workers::Workers;
use crate::runtime::RunaRoot;
use crate::utils::{
    path::{resolve_initial_dir, validate_path},
    timings,
};

/// Initializes the RunaRoot.
/// Resolves startup paths and spawns the workers.
///
/// # Returns
/// The constructed [RunaRoot] with the built [AppContainer] and the spawned [Workers]
pub(crate) fn start(config: Config, cli_paths: Option<Vec<PathBuf>>) -> io::Result<RunaRoot> {
    let config = Arc::new(config);
    let is_cli = cli_paths.is_some();
    let paths = cli_paths
        .as_deref()
        .unwrap_or_else(|| config.general().startup_tabs());

    let (tabs, warnings) = resolve_startup_tabs(&config, paths, is_cli)?;

    let workers = Workers::spawn();

    let mut container = AppContainer::from_states(tabs, &workers)
        .ok_or_else(|| io::Error::other("The provided paths could not be opened"))?;

    if !warnings.is_empty() {
        container
            .current_mut()
            .push_overlay_message(warnings.join("\n"), timings::message::LONG);
    }

    Ok(RunaRoot::new(container, workers))
}

fn resolve_startup_tabs(
    config: &Arc<Config>,
    paths: &[PathBuf],
    is_cli: bool,
) -> io::Result<(Vec<AppState>, Vec<String>)> {
    let mut tabs = Vec::with_capacity(paths.len().min(MAX_TABS));
    let mut warnings = Vec::new();

    for path in paths {
        if tabs.len() >= MAX_TABS {
            break;
        }

        match open_startup_tab(config, path) {
            Ok(tab) => tabs.push(tab),
            Err(e) if is_cli => return Err(e),
            Err(e) => warnings.push(format!("Skipped tab ({e})")),
        }
    }

    if tabs.is_empty() {
        if is_cli {
            return Err(io::Error::other("The provided paths could not be opened"));
        }

        tabs.push(AppState::new(Arc::clone(config))?);
    }

    Ok((tabs, warnings))
}

fn open_startup_tab(config: &Arc<Config>, path: &Path) -> io::Result<AppState> {
    if path == Path::new(".") || path == Path::new("cwd") {
        return AppState::new(Arc::clone(config));
    }

    let target = resolve_initial_dir(path);
    validate_path(&target)
        .map_err(|e| io::Error::new(e.kind(), format!("{e}: '{}'", path.display())))?;

    AppState::from_dir(Arc::clone(config), &target)
}
