//! Navigation actions handler for AppState
//!
//! This module contains the implementation of navigation-related actions for the AppState.
//!
//! It includes handling of actions such as moving up/down the file list,
//! navigating into directories, going to the parent directory,
//! toggling markers, clearing filters, and more.
//!
//! The functions in this module are responsible for updating
//! the navigation state, requesting previews, and managing the file information cache
//! as needed based on user interactions.

use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::PathBuf;

use crate::app::{
    Clipboard, NavState,
    keymap::NavAction,
    state::{AppState, KeypressResult},
};
use crate::core::workers::Workers;
use crate::utils::{os, path, timings};

impl AppState {
    /// Handles navigation actions (up, down, into dir, etc).
    /// Returns a [KeypressResult] indicating how the action was handled.
    pub(in crate::app) fn handle_nav_action(
        &mut self,
        workers: &Workers,
        action: NavAction,
        clipboard: &mut Clipboard,
    ) -> KeypressResult {
        match action {
            NavAction::GoUp => {
                self.move_nav_if_possible(workers, |nav| nav.move_up());
            }
            NavAction::GoDown => {
                self.move_nav_if_possible(workers, |nav| nav.move_down());
            }
            NavAction::GoParent => {
                let res = self.handle_go_parent(workers);
                return res;
            }
            NavAction::GoIntoDir => {
                let res = self.handle_go_into_dir(workers);
                return res;
            }
            NavAction::ToggleMarker => {
                let marker_jump = self.config.display().toggle_marker_jump();
                self.nav.toggle_marker_advance(clipboard, marker_jump);
                self.preview.mark_pending();
                self.update_file_info_cache(workers);
            }
            NavAction::ClearMarker => {
                self.nav.clear_markers();
                self.preview.mark_pending();
            }
            NavAction::ClearFilter => {
                self.nav.clear_filters();
                self.update_file_info_cache(workers);
                self.preview.mark_pending();
            }
            NavAction::ClearAll => {
                self.nav.clear_markers();
                self.nav.clear_filters();
                clipboard.clear();
                self.update_file_info_cache(workers);
                self.preview.mark_pending();
            }
            NavAction::GoToBottom => {
                self.nav.last_selected();
                self.update_file_info_cache(workers);
                self.preview.mark_pending();
            }
            NavAction::ScrollUp => {
                if self.actions.is_input_mode() || self.overlays.needs_scroll() {
                    self.actions.scroll().scroll_up();
                } else {
                    self.preview.scroll_up(self.metrics.preview_height);
                }
            }
            NavAction::ScrollDown => {
                if self.actions.is_input_mode() || self.overlays.needs_scroll() {
                    self.actions.scroll().scroll_down();
                } else {
                    self.preview.scroll_down(self.metrics.preview_height);
                }
            }
            NavAction::SelectAll => {
                clipboard.clear();
                self.nav.select_all();
                self.preview.mark_pending();
                self.update_file_info_cache(workers);
            }
            _ => {}
        }
        KeypressResult::Continue
    }

    /// Calls the provided function to move navigation if possible.
    ///
    /// If the movement was successful (f returns true), marks the preview as pending refresh.
    /// Used to encapsulate common logic for nav actions that change selection or directory.
    fn move_nav_if_possible<F>(&mut self, workers: &Workers, f: F)
    where
        F: FnOnce(&mut NavState) -> bool,
    {
        if !f(&mut self.nav) {
            return;
        }

        let allow_immediate = self.nav_time.can_trigger(timings::throttle::NAV);

        let selected_changed_preview = if let Some(entry) = self.nav.selected_entry() {
            let sel_path = self.nav.current_dir().join(entry.name());
            self.preview.current_path() != Some(sel_path.as_path())
        } else {
            true
        };

        if allow_immediate {
            self.update_file_info_cache(workers);

            if selected_changed_preview {
                if self.config.display().instant_preview() {
                    self.request_preview(workers);
                } else {
                    self.preview.mark_pending();
                }
            }
            self.nav_time.touch();
        } else {
            self.preview.mark_pending();
            self.update_file_info_cache(workers);
        }
    }

    /// Handles the go to parent directory action.
    ///
    /// If the current directory has a parent, navigates to it, saves the current position,
    /// and requests loading of the new directory and its parent content.
    fn handle_go_parent(&mut self, workers: &Workers) -> KeypressResult {
        let current = self.nav.current_dir();

        let Some(parent) = current.parent() else {
            return KeypressResult::Continue;
        };

        let parent_path = parent.to_path_buf();

        if fs::metadata(&parent_path).is_err() {
            self.push_overlay_message("Parent directory is unreachable", timings::message::DEFAULT);
            return KeypressResult::Consumed;
        }

        let exited_name = current.file_name().map(|n| n.to_os_string());
        self.navigate_to(parent_path, exited_name, workers);
        KeypressResult::Continue
    }

    /// Handles the go into directory action.
    ///
    /// If the selected entry is a directory, navigates into it, saves the current position,
    /// and requests loading of the new directory and its parent content.
    fn handle_go_into_dir(&mut self, workers: &Workers) -> KeypressResult {
        let Some(entry) = self.nav.selected_entry() else {
            return KeypressResult::Continue;
        };

        let entry_path = self.nav.current_dir().join(entry.name());

        let Ok(meta) = fs::metadata(&entry_path) else {
            return KeypressResult::Continue;
        };

        if !meta.is_dir() {
            return KeypressResult::Continue;
        }

        match fs::read_dir(&entry_path) {
            Ok(_) => {
                self.navigate_to(entry_path, None, workers);
                KeypressResult::Continue
            }
            Err(e) if e.kind() == io::ErrorKind::PermissionDenied => {
                let msg = format!("'{}': Permission denied", entry.name_str());
                self.push_overlay_message(msg, timings::message::DEFAULT);
                KeypressResult::Consumed
            }
            Err(_) => KeypressResult::Continue,
        }
    }

    /// Handles the find action.
    ///
    /// If a result is selected in the find results, navigates to its path.
    /// If the path is a directory, navigates into it.
    /// If the path is a file, navigates to its parent directory and focuses on the file.
    pub(super) fn handle_find(&mut self, workers: &Workers) {
        let Some(r) = self
            .actions
            .find()
            .results()
            .get(self.actions.find().selected())
        else {
            return;
        };

        let path = r.path();

        let (target_dir, focus) = if path.is_dir() {
            (path.to_path_buf(), None)
        } else {
            let Some(parent) = path.parent() else {
                return;
            };
            let file_name = path.file_name().map(|n| n.to_os_string());
            (parent.to_path_buf(), file_name)
        };

        if let Err(e) = fs::read_dir(&target_dir) {
            let msg = format!("Access Denied: {}", e);
            self.push_overlay_message(msg, timings::message::DEFAULT);
            return;
        }
        self.navigate_to(target_dir, focus, workers);
        self.exit_input_mode();
    }

    pub(super) fn handle_go_to_home(&mut self, workers: &Workers) {
        if let Some(home_path) = os::get_home() {
            self.navigate_to(home_path.clone(), None, workers);
        }
    }

    pub(super) fn handle_go_to_path(&mut self, workers: &Workers) {
        let path = self.actions.input_buffer();
        if path.trim().is_empty() {
            self.push_overlay_message("Error: No path entered", timings::message::DEFAULT);
            return;
        }

        let expaned = path::expand_home_path_buf(path);
        let abs_path = if expaned.is_absolute() {
            expaned
        } else {
            self.nav.current_dir().join(expaned)
        };

        if let Ok(meta) = fs::metadata(&abs_path) {
            if meta.is_dir() {
                self.navigate_to(abs_path.clone(), None, workers);
            } else {
                self.push_overlay_message("Error: Not a directory", timings::message::DEFAULT);
            }
        } else {
            self.push_overlay_message("Error: Invalid path", timings::message::DEFAULT);
        }
    }

    pub(super) fn handle_go_to_top(&mut self, workers: &Workers) {
        self.nav.first_selected();
        self.request_preview(workers);
    }

    fn navigate_to(&mut self, path: PathBuf, focus: Option<OsString>, workers: &Workers) {
        self.nav.save_position();
        self.nav.set_path(path.clone());

        let sort_config = self.nav.sort_config();
        let list_opts = self.dir_list_options();

        if let Some(val) = workers.cache().get(&path, sort_config, &list_opts) {
            let (entries, sort_column) = (val.entries(), val.sort_column());
            self.nav
                .update_from_worker(path, entries.clone(), sort_column.clone(), focus.clone());
            self.is_loading = false;
        } else if let Some(parent) = self.nav.current_dir().parent()
            && path == parent
            && let Some(val) =
                workers
                    .cache()
                    .get(parent, self.nav.sort_config(), &self.dir_list_options())
        {
            let (entries, sort_column) = (val.entries(), val.sort_column());
            self.nav
                .update_from_worker(path, entries.clone(), sort_column.clone(), focus.clone());
            self.is_loading = false;
        } else {
            self.is_loading = true;
        }

        self.request_dir_load(workers, focus);
        self.request_parent_content(workers);
    }
}
