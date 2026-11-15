//! File info module for AppState usage.
//!
//! [FileInfoState] struct to wrap the [FileMetadata](crate::core::metadata::FileMetadata) and manage the state of worker requests,
//! pending paths, and selected file metadata.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::core::metadata::FileMetadataCache;
use crate::utils::timings::Throttler;

#[derive(Debug, Clone)]
struct PendingRequest {
    request_id: u64,
    path: PathBuf,
}

#[derive(Debug, Clone)]
pub(super) struct FileInfoState {
    request_id: u64,
    pending: Option<PendingRequest>,
    selected: Option<Arc<FileMetadataCache>>,
    last_request_time: Throttler,
}

impl FileInfoState {
    pub(super) fn new() -> Self {
        Self {
            request_id: 0,
            pending: None,
            selected: None,
            last_request_time: Throttler::new(),
        }
    }

    pub(super) fn prepare_new_request(&mut self) -> u64 {
        self.request_id = self.request_id.wrapping_add(1);
        self.request_id
    }

    pub(super) fn is_pending_path(&self, path: &Path) -> bool {
        self.pending
            .as_ref()
            .is_some_and(|pending| pending.path == path)
    }

    pub(super) fn try_request(&mut self, throttle_ms: u64) -> bool {
        self.last_request_time.try_trigger(throttle_ms)
    }

    pub(super) fn take_queued(&mut self, throttle_ms: u64) -> bool {
        self.last_request_time.take_queued(throttle_ms)
    }

    pub(super) fn touch(&mut self) {
        self.last_request_time.touch();
    }

    pub(super) fn set_pending(&mut self, id: u64, path: PathBuf) {
        self.pending = Some(PendingRequest {
            request_id: id,
            path,
        });
    }

    pub(super) fn matches_pending(&self, id: u64, path: &Path) -> bool {
        self.pending
            .as_ref()
            .is_some_and(|pending| pending.request_id == id && pending.path == path)
    }

    pub(super) fn selected(&self) -> Option<&FileMetadataCache> {
        self.selected.as_deref()
    }

    pub(super) fn selected_arc(&self) -> Option<&Arc<FileMetadataCache>> {
        self.selected.as_ref()
    }

    pub(super) fn set_selected(&mut self, meta: Option<Arc<FileMetadataCache>>) {
        self.selected = meta;
    }

    pub(super) fn clear_pending(&mut self) {
        self.pending = None;
    }

    pub(super) fn clear(&mut self) {
        self.pending = None;
        self.selected = None;
    }
}
