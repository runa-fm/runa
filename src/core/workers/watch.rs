//! Config watcher thread.
//!
//! This thread watches for changes to the runa config file and any directories that are being listed in the UI.
//! When a change is detected, it sends a message to the main thread to handle the change.

use std::collections::HashSet;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender, unbounded};
use notify::{
    Config as NotifyConfig, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher,
    event::ModifyKind,
};

use crate::core::workers::{WatchCommand, WorkerResponse};
use crate::utils::{os, timings::Timings};

/// Starts the filesystem watcher thread.
pub(super) fn start(cmd_rx: Receiver<WatchCommand>, res_tx: Sender<WorkerResponse>) {
    thread::spawn(move || {
        let (ev_tx, ev_rx) = unbounded::<notify::Result<Event>>();

        let mut state = match WatchState::new(res_tx.clone(), ev_tx, os::default_config_path()) {
            Ok(state) => state,
            Err(e) => {
                let _ = res_tx.send(WorkerResponse::Error(format!(
                    "Watch error: failed to create watcher: {}",
                    e
                )));
                return;
            }
        };

        loop {
            let timer = state.timer();

            crossbeam_channel::select! {
                recv(cmd_rx) -> msg => match msg {
                    Ok(WatchCommand::Retarget(dirs)) => state.retarget(dirs),
                    // All senders dropped: runa is shutting down.
                    Err(_) => break,
                },
                recv(ev_rx) -> msg => {
                    if let Ok(result) = msg {
                        state.handle_event(&result);
                    }
                },
                recv(timer) -> _ => state.flush(),
            }
        }
    });
}

/// Watch state of the watcher thread
///
/// Collects changed listing directories until debounce deadline passes.
/// Config changes are reported right away.
pub(crate) struct WatchState {
    watcher: RecommendedWatcher,
    res_tx: Sender<WorkerResponse>,
    config_path: PathBuf,
    config_name: OsString,
    config_dir: Option<PathBuf>,
    config_watched: Vec<PathBuf>,
    watched: HashSet<PathBuf>,
    pending: HashSet<PathBuf>,
    deadline: Option<Instant>,
}

impl WatchState {
    /// Creates the watcher and starts watching the config directory
    ///
    /// # Arguments
    /// * res_tx - channel for the change reportings
    /// * ev_tx - channel the watcher sends the raw events to
    /// * config_path - path to the runa toml config file
    ///
    /// # Error
    /// The notify error when the watcher can not be created
    pub(crate) fn new(
        res_tx: Sender<WorkerResponse>,
        ev_tx: Sender<notify::Result<Event>>,
        config_path: PathBuf,
    ) -> notify::Result<Self> {
        let watcher = RecommendedWatcher::new(
            move |result| {
                let _ = ev_tx.send(result);
            },
            NotifyConfig::default(),
        )?;

        let config_name = config_path
            .file_name()
            .map(|n| n.to_os_string())
            .unwrap_or_default();

        let config_dir = config_path.parent().map(Path::to_path_buf);

        let mut state = Self {
            watcher,
            res_tx,
            config_path,
            config_name,
            config_dir,
            config_watched: Vec::new(),
            watched: HashSet::new(),
            pending: HashSet::new(),
            deadline: None,
        };

        state.watch_config_dir();
        Ok(state)
    }

    pub(crate) fn retarget(&mut self, dirs: Vec<PathBuf>) {
        let new: HashSet<PathBuf> = dirs.into_iter().collect();

        for old in &self.watched {
            if !new.contains(old) && !self.config_watched.contains(old) {
                let _ = self.watcher.unwatch(old);
            }
        }

        for dir in &new {
            if !self.watched.contains(dir) && !self.config_watched.contains(dir) {
                let _ = self.watcher.watch(dir, RecursiveMode::NonRecursive);
            }
        }

        self.watched = new;
    }

    /// Watch event handler that sorts raw watcher event into config changes
    /// and changed listing directories.
    pub(crate) fn handle_event(&mut self, result: &notify::Result<Event>) {
        let event = match result {
            Ok(event) if !event.need_rescan() => event,
            _ => {
                self.pending.extend(self.watched.iter().cloned());
                self.schedule_flush();
                return;
            }
        };

        if let Some(dir) = &self.config_dir
            && !self.config_watched.contains(dir)
            && appeared(event, dir)
            && self.watcher.watch(dir, RecursiveMode::NonRecursive).is_ok()
        {
            self.config_watched.push(dir.clone());
            if self.config_path.is_file() {
                let _ = self.res_tx.send(WorkerResponse::ConfigChanged);
            }
        }

        self.classify_watch_event(event);
        self.schedule_flush();
    }

    /// Reports collected listings and clears the deadline.
    pub(crate) fn flush(&mut self) {
        if !self.pending.is_empty() {
            let dirs: Vec<PathBuf> = self.pending.drain().collect();
            let _ = self.res_tx.send(WorkerResponse::DirsChanged { dirs });
        }

        self.deadline = None;
    }

    fn schedule_flush(&mut self) {
        if !self.pending.is_empty() && self.deadline.is_none() {
            self.deadline =
                Some(Instant::now() + Duration::from_millis(Timings::FS_WATCH_DEBOUNCE_MS));
        }
    }

    fn timer(&self) -> Receiver<Instant> {
        match self.deadline {
            Some(dl) => crossbeam_channel::after(dl.saturating_duration_since(Instant::now())),
            None => crossbeam_channel::never(),
        }
    }

    fn watch_config_dir(&mut self) {
        let Some(dir) = self
            .config_dir
            .as_deref()
            .and_then(resolve_config_watch_dir)
        else {
            return;
        };

        match self.watcher.watch(&dir, RecursiveMode::NonRecursive) {
            Ok(()) => self.config_watched.push(dir),
            Err(e) => {
                let _ = self
                    .res_tx
                    .send(WorkerResponse::Error(format!("Config watch error: {}", e)));
            }
        }
    }

    fn classify_watch_event(&mut self, event: &Event) {
        if !matches!(
            event.kind,
            EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
        ) {
            return;
        }

        if is_config_changed(event, &self.config_path, &self.config_name) {
            let _ = self.res_tx.send(WorkerResponse::ConfigChanged);
        }

        for path in &event.paths {
            if self.watched.contains(path) {
                self.pending.insert(path.clone());
            } else if let Some(parent) = path.parent()
                && self.watched.contains(parent)
            {
                self.pending.insert(parent.to_path_buf());
            }
        }
    }
}

fn is_config_changed(event: &Event, config_path: &Path, config_name: &OsStr) -> bool {
    matches!(
        event.kind,
        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
    ) && event.paths.iter().any(|p| {
        p == config_path
            || (p.file_name().is_some_and(|name| name == config_name)
                && is_config_dir(p.parent(), config_path))
    })
}

fn is_config_dir(dir: Option<&Path>, config_path: &Path) -> bool {
    let (Some(dir), Some(config_dir)) = (dir, config_path.parent()) else {
        return false;
    };
    matches!((fs::canonicalize(dir), fs::canonicalize(config_dir)), (Ok(a), Ok(b)) if a == b)
}

fn appeared(event: &Event, path: &Path) -> bool {
    matches!(
        event.kind,
        EventKind::Create(_) | EventKind::Modify(ModifyKind::Name(_))
    ) && event.paths.iter().any(|p| p == path)
}

fn resolve_config_watch_dir(config_dir: &Path) -> Option<PathBuf> {
    if config_dir.is_dir() {
        return Some(config_dir.to_path_buf());
    }

    config_dir
        .parent()
        .filter(|parent| parent.is_dir())
        .map(Path::to_path_buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    use notify::event::CreateKind;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn resolve_config_watch_dir_config() -> Result<(), Box<dyn std::error::Error>> {
        let temp = tempdir()?;
        let config_dir = temp.path().join("runa");
        fs::create_dir_all(&config_dir)?;

        let resolved = resolve_config_watch_dir(&config_dir).expect("Expected watch dir");
        assert_eq!(resolved, config_dir);
        Ok(())
    }

    #[test]
    fn resolve_config_watch_dir_fallback() -> Result<(), Box<dyn std::error::Error>> {
        let temp = tempdir()?;
        let config_dir = temp.path().join("runa");

        let resolved = resolve_config_watch_dir(&config_dir).expect("Expected watch dir");
        assert_eq!(resolved, temp.path());
        Ok(())
    }

    #[test]
    fn only_the_config_file_is_a_config_change() -> Result<(), Box<dyn std::error::Error>> {
        let temp = tempdir()?;
        let config_dir = temp.path().join("runa");
        let other_dir = temp.path().join("dotfiles");
        fs::create_dir_all(&config_dir)?;
        fs::create_dir_all(&other_dir)?;
        let config_path = config_dir.join("runa.toml");
        let name = OsStr::new("runa.toml");
        let created =
            |path: PathBuf| Event::new(EventKind::Create(CreateKind::File)).add_path(path);

        assert!(is_config_changed(
            &created(config_path.clone()),
            &config_path,
            name
        ));
        assert!(!is_config_changed(
            &created(other_dir.join("runa.toml")),
            &config_path,
            name
        ));
        Ok(())
    }
}
