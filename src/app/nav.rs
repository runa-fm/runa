//! Navigation state and file list logic for runa.
//!
//! Manages the current directory, file entries, selection, markers and filters.
//! Provides helpers for pane navigation, selection, filtering, and bulk actions.

use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::app::Clipboard;
use crate::core::{FileEntry, listing::SortConfig};
use crate::utils::{path, text::StrBuffer};

const MAX_SAVED_POSITIONS: usize = 100;
const MAX_SAVED_FILTERS: usize = 50;

/// Holds the navigation, selection and file list state of a pane.
pub(crate) struct NavState {
    current_dir: PathBuf,
    entries: Arc<[FileEntry]>,
    selected: usize,
    shown_indices: Vec<usize>,
    positions: HashMap<PathBuf, OsString>,
    markers: HashSet<PathBuf>,
    active_filter: String,
    filters: HashMap<PathBuf, String>,
    sort_config: SortConfig,
    request_id: u64,
    reload_id: Option<u64>,

    // UI releted fields
    sort_column: Option<Arc<StrBuffer>>,
    display_path: String,
}

impl NavState {
    pub(crate) fn new(path: PathBuf) -> Self {
        let display_path = path::format_display_path(&path);
        Self {
            current_dir: path,
            entries: Arc::default(),
            selected: 0,
            shown_indices: Vec::new(),
            positions: HashMap::new(),
            markers: HashSet::new(),
            active_filter: String::new(),
            filters: HashMap::new(),
            sort_config: SortConfig::default(),
            sort_column: None,
            display_path,
            request_id: 0,
            reload_id: None,
        }
    }

    pub(crate) fn mark_reload(&mut self, request_id: u64) {
        self.reload_id = Some(request_id);
    }

    pub(crate) fn take_reload(&mut self, request_id: u64) -> bool {
        self.reload_id.take_if(|id| *id == request_id).is_some()
    }

    crate::getters! {
        current_dir: &Path,
        entries: &[FileEntry],
        selected_idx => selected: usize,
        shown_indices: &[usize],
        sort_config: SortConfig,
        sort_column: &Option<Arc<StrBuffer>>,
        markers: &HashSet<PathBuf>,
        active_filter: &str,
        display_path: &str,
        request_id: u64,
    }

    pub(crate) fn entries_arc(&self) -> Arc<[FileEntry]> {
        Arc::clone(&self.entries)
    }

    pub(crate) fn first_selected(&mut self) -> usize {
        self.selected = 0;
        self.selected
    }

    pub(crate) fn last_selected(&mut self) {
        if !self.shown_indices.is_empty() {
            self.selected = self.shown_indices.len().saturating_sub(1);
        }
    }

    pub(crate) fn select_all(&mut self) {
        if self.entries.is_empty() {
            return;
        }

        let paths_to_mark: Vec<PathBuf> = self
            .shown_entries()
            .map(|e| self.current_dir.join(e.name()))
            .collect();
        self.markers.extend(paths_to_mark);
    }

    // Navigation functions

    /// Prepares a new request by incrementing the request ID.
    pub(crate) fn prepare_new_request(&mut self) -> u64 {
        self.request_id = self.request_id.wrapping_add(1);
        self.request_id
    }

    /// Moves the selection up by one entry, wrapping around if necessary.
    /// Returns `true` if the selection was moved, `false` if there are no entries.
    pub(crate) fn move_up(&mut self) -> bool {
        let len = self.shown_entries_len();
        if len == 0 {
            return false;
        }

        if self.selected == 0 {
            self.selected = len - 1;
        } else {
            self.selected -= 1;
        }
        true
    }

    /// Moves the selection down by one entry, wrapping around if necessary.
    /// Returns `true` if the selection was moved, `false` if there are no entries.
    pub(crate) fn move_down(&mut self) -> bool {
        let len = self.shown_entries_len();
        if len == 0 {
            return false;
        }

        self.selected = (self.selected + 1) % len;
        true
    }

    /// Saves the current selection position for the current directory.
    pub(crate) fn save_position(&mut self) {
        if let Some(entry) = self.selected_entry() {
            self.positions
                .insert(self.current_dir.clone(), entry.name().to_os_string());

            if self.positions.len() > MAX_SAVED_POSITIONS
                && let Some(key) = self.positions.keys().next().cloned()
            {
                self.positions.remove(&key);
            }
        }
    }

    /// Returns a reference to the saved positions map.
    pub(crate) fn get_position(&self) -> &HashMap<PathBuf, OsString> {
        &self.positions
    }

    /// Sets a new current directory path, clearing entries and selection.
    /// Increments the request ID to cancel pending requests.
    pub(crate) fn set_path(&mut self, path: PathBuf) {
        self.save_position();

        self.current_dir = path;
        // instantly ends all pending messages from the previous directory.
        self.request_id = self.request_id.wrapping_add(1);
    }

    /// Updates the navigation state from a worker thread's result.
    /// Sets the current directory, entries, and selection based on the provided focus.
    pub(crate) fn update_from_worker(
        &mut self,
        path: PathBuf,
        entries: Arc<[FileEntry]>,
        sort_column: Option<Arc<StrBuffer>>,
        focus: Option<OsString>,
    ) {
        self.current_dir = path;
        self.display_path = path::format_display_path(&self.current_dir);
        self.entries = entries;
        self.sort_column = sort_column;

        self.restore_filter_for_current_dir();
        self.rebuild_shown_cache();

        if self.entries.is_empty() || self.shown_indices.is_empty() {
            self.selected = 0;
            return;
        }

        let target_name = focus.or_else(|| self.positions.get(&self.current_dir).cloned());

        if let Some(name) = target_name {
            self.selected = self
                .shown_indices
                .iter()
                .position(|&abs_idx| self.entries[abs_idx].name() == name)
                .unwrap_or(0);
        } else {
            self.selected = 0;
        }
    }

    /// Toggles the marker state of the currently selected entry.
    /// If the entry is in the clipboard, it is unmarked and removed from the clipboard.
    pub(crate) fn toggle_marker(&mut self, clipboard: &mut Clipboard) {
        if let Some(entry) = self.selected_entry() {
            let path = self.current_dir().join(entry.name());

            if clipboard.remove(&path) {
                self.markers.insert(path);
                return;
            }

            if !self.markers.remove(&path) {
                self.markers.insert(path);
            }
        }
    }

    /// Toggles the marker state of the currently selected entry and advances the selection.
    pub(crate) fn toggle_marker_advance(&mut self, clipboard: &mut Clipboard, jump: bool) {
        self.toggle_marker(clipboard);
        let count = self.shown_entries_len();

        if count == 0 {
            return;
        }

        if self.selected == count - 1 {
            if jump {
                self.selected = 0;
            }
        } else {
            self.selected += 1;
        }
    }

    /// Clears all markers.
    pub(crate) fn clear_markers(&mut self) {
        self.markers.clear();
    }

    /// Returns the set of action targets, either marked entries or the selected entry.
    pub(crate) fn get_action_targets(&self) -> HashSet<PathBuf> {
        if self.markers.is_empty() {
            self.selected_entry()
                .map(|e| self.current_dir.join(e.name()))
                .into_iter()
                .collect()
        } else {
            self.markers.iter().cloned().collect()
        }
    }

    /// Returns the number of entries that match the current filter.
    pub(crate) fn shown_entries_len(&self) -> usize {
        self.shown_indices.len()
    }

    /// Returns a reference to the currently selected entry that matches the filter.
    pub(crate) fn selected_entry(&self) -> Option<&FileEntry> {
        let actual_idx = self.shown_indices.get(self.selected)?;
        self.entries.get(*actual_idx)
    }

    /// Returns true if the path points to the selected entry of the current directory.
    pub(crate) fn is_selected_path(&self, path: &Path) -> bool {
        path.parent() == Some(self.current_dir.as_path())
            && self
                .selected_entry()
                .is_some_and(|entry| path.file_name() == Some(entry.name()))
    }

    /// Sets a new filter string, preserving the selected entry if possible.
    pub(crate) fn set_filter(&mut self, filter: String) {
        if self.active_filter == filter {
            return;
        }

        let target_name = self.selected_entry().map(|e| e.name().to_os_string());
        self.active_filter = filter;
        self.save_filter_for_current_dir();

        self.rebuild_shown_cache();

        if let Some(name) = target_name {
            self.selected = self
                .shown_indices
                .iter()
                .position(|&i| self.entries[i].name() == name)
                .unwrap_or(0);
        } else {
            self.selected = 0;
        }
    }

    /// Clears the current filter.
    pub(crate) fn clear_filters(&mut self) {
        let prev_abs = self.shown_indices.get(self.selected).cloned();

        self.active_filter.clear();
        self.save_filter_for_current_dir();

        self.rebuild_shown_cache();

        if let Some(abs_idx) = prev_abs {
            self.selected = self
                .shown_indices
                .iter()
                .position(|&i| i == abs_idx)
                .unwrap_or(0);
        } else {
            self.selected = 0;
        }
    }

    /// Saves the current filter for the current directory.
    fn save_filter_for_current_dir(&mut self) {
        if self.active_filter.is_empty() {
            self.filters.remove(&self.current_dir);
        } else {
            self.filters
                .insert(self.current_dir.clone(), self.active_filter.clone());

            if self.filters.len() > MAX_SAVED_FILTERS
                && let Some(key) = self.filters.keys().next().cloned()
            {
                self.filters.remove(&key);
            }
        }
    }

    /// Restores the saved filter for the current directory, if any.
    fn restore_filter_for_current_dir(&mut self) {
        self.active_filter = self
            .filters
            .get(&self.current_dir)
            .cloned()
            .unwrap_or_default();
    }

    fn rebuild_shown_cache(&mut self) {
        if self.active_filter.is_empty() {
            self.shown_indices = (0..self.entries.len()).collect();
        } else {
            let filter_lower = self.active_filter.to_lowercase();
            self.shown_indices = self
                .entries
                .iter()
                .enumerate()
                .filter(|(_, e)| e.lowered().contains(&filter_lower))
                .map(|(i, _)| i)
                .collect();
        }

        let len = self.shown_indices.len();
        if self.selected >= len {
            self.selected = len.saturating_sub(1);
        }
    }

    pub(crate) fn set_sort_config(&mut self, sort_config: SortConfig) {
        self.sort_config = sort_config;
    }

    /// Returns an iterator over the entries that match the current filter.
    /// If the filter is empty, returns all entries.
    fn shown_entries(&self) -> impl Iterator<Item = &FileEntry> {
        self.shown_indices
            .iter()
            .filter_map(|&i| self.entries.get(i))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::core::fm;

    use rand::rng;
    use rand::seq::SliceRandom;
    use std::error;
    use std::fs;
    use std::fs::File;
    use tempfile::tempdir;

    #[test]
    fn navstate_rapid_navigation() -> Result<(), Box<dyn error::Error>> {
        let dir = tempdir()?;
        let file_count = 10;

        for i in 0..file_count {
            let file_path = dir.path().join(format!("testfile_{i}.txt"));
            File::create(&file_path)?;
        }

        let entries = fm::browse_dir(dir.path())?;
        assert!(!entries.is_empty(), "sandbox should not be empty");

        let mut nav = NavState::new(dir.path().to_path_buf());
        nav.update_from_worker(
            dir.path().to_path_buf(),
            Arc::from(entries.clone()),
            None,
            None,
        );

        assert_eq!(
            nav.entries().len(),
            file_count,
            "initial entry count mismatch"
        );

        let down_presses = 1000;
        for _ in 0..down_presses {
            assert!(nav.move_down(), "nav.move_down() failed during stress");
        }

        let expected_idx = down_presses % file_count;
        assert_eq!(
            nav.selected_idx(),
            expected_idx,
            "wrong index after DOWN stress"
        );

        let selected = nav.selected_entry().ok_or("no entry selected after DOWN")?;
        assert_eq!(selected.name_str(), entries[expected_idx].name_str());

        let up_presses = 1000;
        for _ in 0..up_presses {
            assert!(nav.move_up(), "nav.move_up() failed during stress");
        }

        // Mathematical wrap-around check
        let expected_idx_up = (expected_idx + file_count - (up_presses % file_count)) % file_count;
        assert_eq!(
            nav.selected_idx(),
            expected_idx_up,
            "wrong index after UP stress"
        );

        let selected_up = nav.selected_entry().ok_or("no entry selected after UP")?;
        assert_eq!(selected_up.name_str(), entries[expected_idx_up].name_str());

        // Ensure the internal state hasn't corrupted the entry list
        for (i, entry) in nav.entries().iter().enumerate() {
            assert_eq!(
                entry.name_str(),
                entries[i].name_str(),
                "data corruption at index {i}"
            );
        }
        Ok(())
    }

    #[test]
    fn navstate_selection_persistence() -> Result<(), Box<dyn error::Error>> {
        let base = tempdir()?;
        let base_path = base.path().to_path_buf();
        let subdir_path = base_path.join("subdir");

        fs::create_dir_all(&subdir_path)?;
        for i in 0..20 {
            File::create(subdir_path.join(format!("file_{}.txt", i)))?;
        }

        let base_entries = fm::browse_dir(&base_path)?;
        let sub_entries = fm::browse_dir(&subdir_path)?;

        let mut nav = NavState::new(base_path.clone());
        let repetitions = 200;

        nav.set_path(subdir_path.clone());
        nav.update_from_worker(
            subdir_path.clone(),
            Arc::from(sub_entries.clone()),
            None,
            None,
        );

        for _ in 0..5 {
            nav.move_down();
        }
        assert_eq!(nav.selected_idx(), 5, "Initial move failed");

        for i in 0..repetitions {
            nav.set_path(base_path.clone());
            nav.update_from_worker(
                base_path.clone(),
                Arc::from(base_entries.clone()),
                None,
                None,
            );

            nav.move_down();

            // Return to Subdir
            nav.set_path(subdir_path.clone());
            nav.update_from_worker(
                subdir_path.clone(),
                Arc::from(sub_entries.clone()),
                None,
                None,
            );

            assert_eq!(
                nav.selected_idx(),
                5,
                "Selection lost at iteration {}. Should have stayed at 5.",
                i
            );
        }
        Ok(())
    }

    #[test]
    fn navstate_filter_persistence() -> Result<(), Box<dyn error::Error>> {
        let dir = tempdir()?;
        let base_path = dir.path().to_path_buf();

        let names = vec![
            "main.rs",
            "lib.rs",
            "cargo.toml",
            "readme.md",
            "app.rs",
            "ui.rs",
            "file_manager.rs",
            "config.json",
            "styles.css",
        ];
        for name in &names {
            fs::write(base_path.join(name), "")?;
        }

        let mut entries = fm::browse_dir(&base_path)?;
        entries.shuffle(&mut rng());

        let mut nav = NavState::new(base_path.clone());
        nav.update_from_worker(base_path.clone(), Arc::from(entries), None, None);

        let target_name = "file_manager.rs";

        let actual_start_pos = nav
            .shown_entries()
            .position(|e| e.name_str() == target_name)
            .ok_or("Target not found in nav state")?;

        for _ in 0..actual_start_pos {
            nav.move_down();
        }

        let selected = nav.selected_entry().ok_or("No entry selected")?;
        assert_eq!(selected.name_str(), target_name);

        let input_buffer = "file".to_string();
        nav.set_filter(input_buffer);

        let final_entry = nav.selected_entry().ok_or("Selection lost after filter")?;

        assert_eq!(
            final_entry.name_str(),
            target_name,
            "Selection persistence failed! Found {} instead. Filter was 'file'.",
            final_entry.name_str()
        );
        Ok(())
    }

    #[test]
    fn navstate_marker_persistence() -> Result<(), Box<dyn error::Error>> {
        let dir = tempdir()?;
        let base_path = dir.path().to_path_buf();

        let names = vec!["apple.txt", "banana.txt", "crab.txt"];
        for name in &names {
            fs::write(base_path.join(name), "")?;
        }

        let mut entries = fm::browse_dir(&base_path)?;
        // Shuffle to ensure we arent relying on alphabetical order
        entries.shuffle(&mut rng());

        let mut nav = NavState::new(base_path.clone());
        nav.update_from_worker(base_path.clone(), Arc::from(entries), None, None);

        let mut clipboard = Clipboard::default();

        let to_mark = vec!["apple.txt", "banana.txt"];
        for target in to_mark {
            // Find it in the current view
            let pos = nav
                .shown_entries()
                .position(|e| e.name_str() == target)
                .ok_or("target not found")?;

            // Reset to top and move down to simulate real user navigation
            while nav.selected_idx() > 0 {
                nav.move_up();
            }
            for _ in 0..pos {
                nav.move_down();
            }

            let selected = nav.selected_entry().ok_or("No entry selected to mark")?;
            assert_eq!(selected.name_str(), target);
            nav.toggle_marker(&mut clipboard);
        }

        assert_eq!(nav.markers().len(), 2);

        nav.set_filter("crab".to_string());

        assert_eq!(nav.shown_entries_len(), 1);
        let crab_selected = nav.selected_entry().ok_or("No crab selected")?;
        assert_eq!(crab_selected.name_str(), "crab.txt");

        let targets = nav.get_action_targets();
        assert_eq!(
            targets.len(),
            2,
            "Should target 2 marked files even if hidden"
        );
        assert!(targets.contains(&base_path.join("apple.txt")));
        assert!(targets.contains(&base_path.join("banana.txt")));
        assert!(
            !targets.contains(&base_path.join("cherry.txt")),
            "Should ignore selection when markers exist"
        );

        nav.clear_filters();
        // Navigate to apple.txt
        let apple_pos = nav
            .shown_entries()
            .position(|e| e.name_str() == "apple.txt")
            .ok_or("Apple not found")?;
        while nav.selected_idx() < apple_pos {
            nav.move_down();
        }
        while nav.selected_idx() > apple_pos {
            nav.move_up();
        }

        nav.toggle_marker(&mut clipboard);
        assert_eq!(nav.markers().len(), 1);
        assert!(nav.markers().contains(&base_path.join("banana.txt")));
        Ok(())
    }
}
