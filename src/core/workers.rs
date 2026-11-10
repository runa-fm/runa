//! Worker thread for the runa core operations.
//!
//! Handles directory reads, previews and file operatios on a background thread.
//! All results and errors are sent back via channels.
//!
//! Small changes here can have big effects since this module is tightly integrated with every part
//! of runa.
//!
//! Tasks come in from the app state and the runtime via channels, and results or errors.
//! [WorkerResponse] go back the same way. All filesystem I/O and previews happen on these threads
//!
//! # Caution:
//! This module is a central protocol boundary. Small changes (adding or editing variants, fields, or error handling)
//! may require corresponding changes throughout state, response-handling code and UI.

mod fileop;
mod find;
mod io;
mod metadata;
mod preview;
mod runner;
mod sort;
mod task;
mod watch;

pub(crate) use task::*;

use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use crossbeam_channel::{Receiver, Sender, TrySendError, bounded, unbounded};

use crate::core::{
    FileEntry, FindResult, cache::DirCache, metadata::FileMetadataCache, preview::ListingRow,
};
use crate::utils::text::StrBuffer;

/// Manages worker threads channels for different task types.
pub(crate) struct Workers {
    nav_io_tx: Sender<LoadDirTask>,
    nav_io_queue: Receiver<LoadDirTask>,

    parent_io_tx: Sender<LoadDirTask>,
    parent_io_queue: Receiver<LoadDirTask>,

    preview_dir_tx: Sender<LoadDirTask>,
    preview_file_tx: Sender<PreviewTask>,

    sort_tx: Sender<SortDirTask>,
    sort_queue: Receiver<SortDirTask>,

    metadata_tx: Sender<MetadataTask>,

    find_tx: Sender<FindTask>,
    find_queue: Receiver<FindTask>,

    fileop_tx: Sender<FileOperation>,

    runner_tx: Sender<CommandTask>,
    watch_cmd_tx: Sender<WatchCommand>,

    response_rx: Receiver<WorkerResponse>,

    active_fileops: Arc<AtomicUsize>,
    cache: Arc<DirCache>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct FileOpCounts {
    queued: usize,
    active: usize,
}

impl FileOpCounts {
    crate::getters! {
        queued: usize,
        active: usize,
    }
}

impl Workers {
    /// Create the worker set.
    ///
    /// Spawns one thread per worker:
    /// - I/O (nav, parent, folder preview),
    /// - sort,
    /// - file preview,
    /// - metadata,
    /// - find,
    /// - file operations,
    /// - command
    /// - watcher.
    pub(crate) fn spawn() -> Self {
        let cache = Arc::new(DirCache::new());
        let active_fileops = Arc::new(AtomicUsize::new(0));

        let (nav_io_tx, nav_io_rx) = bounded::<LoadDirTask>(1);
        let (parent_io_tx, parent_io_rx) = bounded::<LoadDirTask>(1);

        let (preview_dir_tx, preview_dir_rx) = bounded::<LoadDirTask>(1);
        let (preview_file_tx, preview_file_rx) = bounded::<PreviewTask>(1);

        let (sort_tx, sort_rx) = bounded::<SortDirTask>(1);
        let (metadata_tx, metadata_rx) = bounded::<MetadataTask>(1);
        let (find_tx, find_rx) = bounded::<FindTask>(1);

        let (fileop_tx, fileop_rx) = unbounded::<FileOperation>();
        let (runner_tx, runner_rx) = unbounded::<CommandTask>();
        let (watch_cmd_tx, watch_cmd_rx) = unbounded::<WatchCommand>();

        let (res_tx, response_rx) = unbounded::<WorkerResponse>();

        let nav_io_queue = nav_io_rx.clone();
        let parent_io_queue = parent_io_rx.clone();
        let sort_queue = sort_rx.clone();
        let find_queue = find_rx.clone();

        io::start(nav_io_rx, res_tx.clone(), Arc::clone(&cache));
        io::start(parent_io_rx, res_tx.clone(), Arc::clone(&cache));
        io::start(preview_dir_rx, res_tx.clone(), Arc::clone(&cache));

        sort::start(sort_rx, res_tx.clone(), Arc::clone(&cache));
        preview::start(preview_file_rx, res_tx.clone());
        metadata::start(metadata_rx, res_tx.clone());
        find::start(find_rx, res_tx.clone());
        fileop::start(fileop_rx, res_tx.clone(), Arc::clone(&active_fileops));
        runner::start(runner_rx, res_tx.clone());
        watch::start(watch_cmd_rx, res_tx.clone());

        Self {
            nav_io_tx,
            nav_io_queue,
            parent_io_tx,
            parent_io_queue,
            preview_dir_tx,
            sort_tx,
            sort_queue,
            preview_file_tx,
            metadata_tx,
            find_tx,
            find_queue,
            fileop_tx,
            runner_tx,
            watch_cmd_tx,
            response_rx,
            active_fileops,
            cache,
        }
    }

    crate::getters! {
        preview_dir_tx: &Sender<LoadDirTask>,
        preview_file_tx: &Sender<PreviewTask>,
        metadata_tx: &Sender<MetadataTask>,
        fileop_tx: &Sender<FileOperation>,
        runner_tx: &Sender<CommandTask>,
        response_rx: &Receiver<WorkerResponse>,
        active_fileops: &Arc<AtomicUsize>,
    }

    pub(crate) fn cache(&self) -> Arc<DirCache> {
        Arc::clone(&self.cache)
    }

    pub(crate) fn fileop_counts(&self) -> FileOpCounts {
        FileOpCounts {
            queued: self.fileop_tx.len(),
            active: self.active_fileops.load(Ordering::Relaxed),
        }
    }

    pub(crate) fn send_nav_io(&self, task: LoadDirTask) {
        send_newest(&self.nav_io_tx, &self.nav_io_queue, task);
    }

    pub(crate) fn send_parent_io(&self, task: LoadDirTask) {
        send_newest(&self.parent_io_tx, &self.parent_io_queue, task);
    }

    pub(crate) fn send_sort(&self, task: SortDirTask) {
        send_newest(&self.sort_tx, &self.sort_queue, task);
    }

    pub(crate) fn send_find(&self, task: FindTask) {
        send_newest(&self.find_tx, &self.find_queue, task);
    }

    pub(crate) fn retarget_watch(&self, dirs: Vec<PathBuf>) {
        let _ = self.watch_cmd_tx.send(WatchCommand::Retarget(dirs));
    }
}

#[derive(Debug, Clone)]
pub(crate) enum PreviewLines {
    Text(Vec<String>),
    Listing(Vec<ListingRow>),
}

/// Responses sent form the worker thread back to the main thread via the channel
///
/// Each variant delivers the result or error from a request taks.
#[derive(Debug, Clone)]
pub(crate) enum WorkerResponse {
    DirectoryLoaded {
        path: PathBuf,
        entries: Arc<[FileEntry]>,
        focus: Option<OsString>,
        sort_column: Option<Arc<StrBuffer>>,
        request: DirRequest,
        tab_id: Option<usize>,
    },
    PreviewLoaded {
        path: PathBuf,
        lines: PreviewLines,
        scroll: usize,
        is_eof: bool,
        request_id: u64,
        tab_id: Option<usize>,
    },
    OperationComplete {
        need_reload: bool,
        focus: Option<OsString>,
        modified_dirs: Vec<PathBuf>,
    },
    FindResults {
        base_dir: PathBuf,
        results: Vec<FindResult>,
        request_id: u64,
        tab_id: Option<usize>,
    },
    FileMetadataLoaded {
        metadata: Arc<FileMetadataCache>,
        path: PathBuf,
        request_id: u64,
        tab_id: Option<usize>,
    },
    ConfigChanged,
    DirsChanged {
        dirs: Vec<PathBuf>,
    },
    DirectoryFailed {
        path: PathBuf,
        message: String,
        request: DirRequest,
        tab_id: Option<usize>,
    },
    Error(String),
}

impl WorkerResponse {
    pub(crate) fn tab_id(&self) -> Option<usize> {
        match self {
            WorkerResponse::DirectoryLoaded { tab_id, .. } => *tab_id,
            WorkerResponse::DirectoryFailed { tab_id, .. } => *tab_id,
            WorkerResponse::PreviewLoaded { tab_id, .. } => *tab_id,
            WorkerResponse::FindResults { tab_id, .. } => *tab_id,
            WorkerResponse::FileMetadataLoaded { tab_id, .. } => *tab_id,
            _ => None,
        }
    }
}

pub(crate) enum WatchCommand {
    Retarget(Vec<PathBuf>),
}

fn send_newest<T>(tx: &Sender<T>, queue: &Receiver<T>, task: T) {
    if let Err(TrySendError::Full(task)) = tx.try_send(task) {
        let _ = queue.try_recv();
        let _ = tx.try_send(task);
    }
}

/// Shared helpers for the worker tests.
#[cfg(test)]
mod test_support {
    use std::time::Duration;

    use crate::core::cache::DirListOptions;

    pub(super) const TEST_TIMEOUT: Duration = Duration::from_secs(15);

    pub(super) const TEST_TAB_ID: Option<usize> = None;

    pub(super) fn fd_available() -> bool {
        which::which("fd").is_ok()
    }

    pub(super) fn bat_available() -> bool {
        which::which("bat").is_ok()
    }

    pub(super) fn test_list_opts() -> DirListOptions {
        DirListOptions {
            dirs_first: true,
            show_hidden: false,
            show_symlink: false,
            show_system: false,
            case_insensitive: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newest_task_replaces_the_waiting_one() {
        let (tx, rx) = bounded(1);
        send_newest(&tx, &rx, 1);
        send_newest(&tx, &rx, 2);
        assert_eq!(rx.try_iter().collect::<Vec<_>>(), [2]);
    }
}
