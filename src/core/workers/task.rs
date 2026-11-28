//! Task definitions for the worker threads.
//!
//! Defines all the tasks that can be sent to the worker threads for processing.
//! Each task is represented by a struct that contains the necessary information for the worker to perform the task.

use std::collections::HashSet;
use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use crate::core::FileEntry;
use crate::core::listing::{DirListOptions, SortConfig};
use crate::core::metadata::MetadataNeeds;
use crate::core::preview::PreviewMode;

/// Represents a request to load a directory, either for navigation, parent directory, or preview.
/// Holds the request type and an associated request ID for tracking.
#[derive(Debug, Clone, Copy)]
pub(crate) enum DirRequest {
    Nav(u64),
    Parent(u64),
    Preview(u64),
}

pub(crate) struct LoadDirTask {
    pub(crate) path: PathBuf,
    pub(crate) focus: Option<OsString>,
    pub(crate) list: DirListOptions,
    pub(crate) sort_config: SortConfig,
    pub(crate) sort_date_format: Arc<str>,
    pub(crate) always_show: Arc<HashSet<OsString>>,
    pub(crate) request: DirRequest,
    pub(crate) tab_id: Option<usize>,
}

pub(crate) struct SortDirTask {
    pub(crate) path: PathBuf,
    pub(crate) entries: Arc<[FileEntry]>,
    pub(crate) focus: Option<OsString>,
    pub(crate) list: DirListOptions,
    pub(crate) sort_config: SortConfig,
    pub(crate) sort_date_format: Arc<str>,
    pub(crate) always_show: Arc<HashSet<OsString>>,
    pub(crate) request: DirRequest,
    pub(crate) tab_id: Option<usize>,
}

pub(crate) struct PreviewTask {
    pub(crate) path: PathBuf,
    pub(crate) max_lines: usize,
    pub(crate) pane_width: usize,
    pub(crate) tab_width: usize,
    pub(crate) scroll: usize,
    pub(crate) preview_mode: PreviewMode,
    pub(crate) request_id: u64,
    pub(crate) tab_id: Option<usize>,
}

pub(crate) struct FindTask {
    pub(crate) base_dir: PathBuf,
    pub(crate) query: String,
    pub(crate) max_results: usize,
    pub(crate) cancel: Arc<AtomicBool>,
    pub(crate) show_hidden: bool,
    pub(crate) request_id: u64,
    pub(crate) tab_id: Option<usize>,
}

pub(crate) struct MetadataTask {
    pub(crate) path: PathBuf,
    pub(crate) date_format: String,
    pub(crate) needs: MetadataNeeds,
    pub(crate) request_id: u64,
    pub(crate) tab_id: Option<usize>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum CommandMode {
    Background { check_status: bool },
    Message,
    Overlay { id: u64, tab_id: Option<usize> },
}

pub(crate) struct CommandTask {
    pub(crate) program: PathBuf,
    pub(crate) args: Vec<OsString>,
    pub(crate) dir: Option<PathBuf>,
    pub(crate) mode: CommandMode,
}

/// Supported file system operations the worker can perform.
pub(crate) enum FileOperation {
    Delete(Vec<PathBuf>, bool),
    Rename {
        old: PathBuf,
        new: PathBuf,
        overwrite: bool,
    },
    Copy {
        src: Vec<PathBuf>,
        dest: PathBuf,
        cut: bool,
        focus: Option<OsString>,
    },
    Create {
        path: PathBuf,
        is_dir: bool,
        overwrite: bool,
    },
}
