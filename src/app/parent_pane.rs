//! State and helpers for displaying and managing the parent pane in runa.
//!
//! Tracks entries, selection, worker requests for the parent pane view above the current working
//! directory

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::core::FileEntry;
use crate::utils::text::StrBuffer;

/// Holds the state of the parent directory pane
///
/// Stores the list of entries in the parent directory, the selected entry (index)
/// and tracks the request IDs to coordinate updates.
#[derive(Default)]
pub(crate) struct ParentState {
    entries: Arc<[FileEntry]>,
    sort_column: Option<Arc<StrBuffer>>,
    selected_idx: Option<usize>,
    last_path: Option<PathBuf>,
    request_id: u64,
}

impl ParentState {
    crate::getters! {
        request_id: u64,
        entries: &Arc<[FileEntry]>,
        sort_column: &Option<Arc<StrBuffer>>,
        selected_idx: Option<usize>,
    }

    pub(crate) fn last_path(&self) -> Option<&Path> {
        self.last_path.as_deref()
    }

    pub(super) fn prepare_new_request(&mut self, path: &Path) -> u64 {
        self.request_id = self.request_id.wrapping_add(1);
        self.last_path = Some(path.to_path_buf());
        self.request_id
    }

    /// Updates the state with new entries
    ///
    /// Only applies the update if request ID is the latest
    pub(super) fn update_from_entries(
        &mut self,
        entries: Arc<[FileEntry]>,
        current_name: &str,
        request_id: u64,
        parent_path: &Path,
        sort_column: Option<Arc<StrBuffer>>,
    ) {
        if request_id < self.request_id {
            return;
        }
        // Find the index of the folder we are currently inside to highlight it
        self.selected_idx = entries.iter().position(|e| e.name_str() == current_name);
        self.entries = entries;
        self.last_path = Some(parent_path.to_path_buf());
        self.sort_column = sort_column;
        self.request_id = request_id;
    }

    pub(super) fn invalidate_if_path(&mut self, path: &Path) {
        if self.last_path() == Some(path) {
            self.request_id = self.request_id.wrapping_add(1);
        }
    }

    /// Clears all entries, resets the selected entry index,
    /// resets the last path and increases the request_id
    pub(super) fn clear(&mut self) {
        self.entries = Arc::default();
        self.selected_idx = None;
        self.last_path = None;
        self.request_id = self.request_id.wrapping_add(1);
    }
}
