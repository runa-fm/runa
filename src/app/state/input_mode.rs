//! Input handlers for AppState, covering all key events that occur
//! while in an input mode (rename, filter, etc).
//!
//! This module is responsible for processing key events related to text input,
//! mode confirmation/cancellation and any special keys relevant
//! to the specific input modes (e.g., Tab for autocomplete in path input).
//!
//! Defines the central [handle_input_mode](AppState::handle_input_mode) function that routes key events
//! to the appropriate handlers based on the current input mode as well as
//! helper functions for entering/exiting input modes
//! and processing specific actions within those modes.

use std::mem;
use std::path::{MAIN_SEPARATOR, Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use crossterm::event::{KeyCode::*, KeyEvent};

use crate::app::overlay_stack::OverlayKind;
use crate::app::{
    ListNav,
    actions::{ActionMode, InputMode},
    keymap::{Action, NavAction, PrefixCommand, SystemAction},
    state::{AppState, KeypressResult},
};
use crate::core::listing::{SortConfig, SortOrder};
use crate::core::proc;
use crate::core::workers::Workers;
use crate::utils::{os, path, timings};

/// AppState input handlers
impl AppState {
    // AppState core handlers

    /// Handles key events when in an input mode (rename, filter, etc).
    /// Returns a [KeypressResult] indicating how the key event was handled.
    ///
    /// If not in an input mode, returns [KeypressResult::Continue].
    /// Consumes keys related to input editing and mode confirmation/cancellation.
    pub(in crate::app) fn handle_input_mode(
        &mut self,
        workers: &Workers,
        key: KeyEvent,
    ) -> KeypressResult {
        let prev_action_mode = self.actions().mode().clone();
        let ActionMode::Input { mode, .. } = &prev_action_mode else {
            return KeypressResult::Continue;
        };

        if let Some(action) = self.keymap.lookup(key) {
            match action {
                Action::Nav(NavAction::ScrollUp) => {
                    self.actions.scroll().scroll_up();
                    return KeypressResult::Consumed;
                }
                Action::Nav(NavAction::ScrollDown) => {
                    self.actions.scroll().scroll_down();
                    return KeypressResult::Consumed;
                }
                _ => {}
            }
        }

        match key.code {
            Enter => {
                match mode {
                    InputMode::NewFile => self.create_file(workers),
                    InputMode::NewFolder => self.create_folder(workers),
                    InputMode::Rename => self.rename_entry(workers),
                    InputMode::Filter => self.apply_filter(workers),
                    InputMode::ConfirmDelete { .. } => self.confirm_delete(workers),
                    InputMode::ConfirmTrash { .. } => self.confirm_trash(workers),
                    InputMode::ConfirmOverwrite { .. } => self.confirm_overwrite(workers),
                    InputMode::Find => self.handle_find(workers),
                    InputMode::MoveFile => self.move_file(workers),
                    InputMode::GoToPath => self.handle_go_to_path(workers),
                }
                if matches!(
                    self.actions().mode(),
                    ActionMode::Input { mode: cur, .. }
                        if mem::discriminant(cur) == mem::discriminant(mode)
                ) {
                    self.exit_input_mode();
                }
                KeypressResult::Consumed
            }

            Esc => {
                self.exit_input_mode();
                KeypressResult::Consumed
            }

            Left => {
                self.actions.action_move_cursor_left();
                KeypressResult::Consumed
            }

            Up => {
                if matches!(mode, InputMode::Find) {
                    self.actions.find_mut().select_prev();
                    KeypressResult::Consumed
                } else {
                    KeypressResult::Continue
                }
            }

            Down => {
                if matches!(mode, InputMode::Find) {
                    self.actions.find_mut().select_next();
                    KeypressResult::Consumed
                } else {
                    KeypressResult::Continue
                }
            }

            Right => {
                self.actions.action_move_cursor_right();
                KeypressResult::Consumed
            }

            Home => {
                self.actions.action_cursor_home();
                KeypressResult::Consumed
            }

            End => {
                self.actions.action_cursor_end();
                KeypressResult::Consumed
            }

            Backspace => {
                self.actions.action_backspace_at_cursor();
                if matches!(mode, InputMode::Filter) {
                    self.apply_filter(workers);
                }
                if matches!(mode, InputMode::Find) {
                    self.actions.find_debounce(Duration::from_millis(90));
                }
                KeypressResult::Consumed
            }

            Tab => {
                if matches!(mode, InputMode::MoveFile | InputMode::GoToPath) {
                    if os::fd_binary().is_ok() {
                        self.tab_autocomplete();
                        KeypressResult::Consumed
                    } else {
                        KeypressResult::Continue
                    }
                } else {
                    KeypressResult::Continue
                }
            }

            Char(c) => match mode {
                InputMode::ConfirmDelete { .. } => {
                    self.process_confirm_char(workers, c, Self::confirm_delete);
                    KeypressResult::Consumed
                }
                InputMode::ConfirmTrash { .. } => {
                    self.process_confirm_char(workers, c, Self::confirm_trash);
                    KeypressResult::Consumed
                }
                InputMode::ConfirmOverwrite { .. } => {
                    self.process_confirm_char(workers, c, Self::confirm_overwrite);
                    KeypressResult::Consumed
                }
                InputMode::Filter => {
                    self.actions.action_insert_at_cursor(c);
                    self.apply_filter(workers);
                    self.update_file_info_cache(workers);
                    KeypressResult::Consumed
                }
                InputMode::Rename
                | InputMode::NewFile
                | InputMode::NewFolder
                | InputMode::MoveFile
                | InputMode::GoToPath => {
                    self.actions.action_insert_at_cursor(c);
                    KeypressResult::Consumed
                }
                InputMode::Find => {
                    self.actions.action_insert_at_cursor(c);
                    self.actions.find_debounce(Duration::from_millis(120));
                    KeypressResult::Consumed
                }
            },

            _ => KeypressResult::Consumed,
        }
    }

    pub(in crate::app) fn handle_sys_action(&mut self, action: SystemAction) -> KeypressResult {
        match action {
            SystemAction::Quit => KeypressResult::Quit,
            SystemAction::KeyBindHelp => {
                self.toggle_keybind_help();
                KeypressResult::Consumed
            }
            SystemAction::Reload => KeypressResult::UiReload,
        }
    }

    pub(in crate::app) fn handle_prefix_dispatch(
        &mut self,
        workers: &Workers,
        key: &KeyEvent,
    ) -> Option<KeypressResult> {
        let allow_sort = !self.is_trash();
        let (started, exited, result, consumed) = {
            let prefix = self.actions.prefix_recognizer_mut();
            let was_active = prefix.is_active();
            let result = prefix.feed(key, &self.keymap, allow_sort);
            let consumed = was_active && key.code == Esc;

            (
                prefix.started_prefix(),
                prefix.exited_prefix(),
                result,
                consumed,
            )
        };

        if started {
            self.show_prefix_help();
        }
        if exited {
            self.hide_prefix_help();
        }

        if consumed {
            return Some(KeypressResult::Consumed);
        }

        if let Some(cmd) = result {
            return self.handle_prefix_action(workers, cmd);
        }

        None
    }

    fn handle_prefix_action(
        &mut self,
        workers: &Workers,
        prefix: PrefixCommand,
    ) -> Option<KeypressResult> {
        match prefix {
            PrefixCommand::Nav(NavAction::GoToTop) => {
                if let Some(trash) = self.trash.as_mut() {
                    trash.select_first();
                } else {
                    self.handle_go_to_top(workers);
                    self.update_file_info_cache(workers);
                }
                Some(KeypressResult::Consumed)
            }
            PrefixCommand::Nav(NavAction::GoToHome) => {
                if self.trash.as_ref().is_none_or(|t| t.can_exit()) {
                    self.handle_go_to_home(workers);
                }
                Some(KeypressResult::Consumed)
            }
            PrefixCommand::Nav(NavAction::GoToPath) => {
                if self.trash.as_ref().is_none_or(|t| t.can_exit()) {
                    self.prompt_go_to_path();
                }
                Some(KeypressResult::Consumed)
            }
            PrefixCommand::Nav(NavAction::GoToTrash) => {
                self.enter_trash(workers);
                Some(KeypressResult::Consumed)
            }
            PrefixCommand::Sort(sort_mode) => {
                let mut sort_config: SortConfig = self.nav.sort_config();

                if sort_config.mode() == sort_mode {
                    sort_config.set_order(sort_config.order().toggle());
                } else {
                    sort_config.set_mode(sort_mode);
                    sort_config.set_order(SortOrder::Ascending);
                }

                self.nav.set_sort_config(sort_config);

                let focus = self
                    .nav
                    .selected_entry()
                    .map(|file_entry| file_entry.name().to_os_string());

                self.request_dir_sort(workers, focus);
                self.request_parent_content(workers);
                self.preview.mark_pending();
                Some(KeypressResult::Sort(sort_config))
            }
            _ => None,
        }
    }

    /// Enters an input mode with the given parameters.
    pub(super) fn enter_input_mode(
        &mut self,
        mode: InputMode,
        prompt: String,
        initial: Option<String>,
    ) {
        self.overlays_mut().remove_kind(OverlayKind::KeybindHelp);

        let buffer = initial.unwrap_or_default();
        self.actions
            .enter_mode(ActionMode::Input { mode, prompt }, buffer);
        self.actions.scroll().reset();
    }

    /// Exits the current input mode.
    /// Simple wrapper around actions::exit_mode.
    pub(super) fn exit_input_mode(&mut self) {
        self.actions.exit_mode();
    }

    fn process_confirm_char<F>(&mut self, workers: &Workers, c: char, on_confirm: F)
    where
        F: FnOnce(&mut Self, &Workers),
    {
        if matches!(c, 'y' | 'Y') {
            on_confirm(self, workers);
        }
        self.exit_input_mode();
    }

    // Prompt functions

    /// Prompts the user to confirm deletion of selected items.
    pub(super) fn prompt_delete(&mut self, is_trash: bool) {
        let targets: Vec<PathBuf> = self.nav.get_action_targets().into_iter().collect();
        let count = targets.len();
        if count == 0 {
            return;
        }

        let mut names: Vec<String> = targets
            .iter()
            .map(|p| {
                p.file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default()
            })
            .collect();
        names.sort();

        let action_word = if is_trash { "Trash" } else { "Delete" };
        let item_label = if count > 1 {
            format!("{} items", count)
        } else {
            "item".to_string()
        };

        let prompt_text = format!("{} {}? [Y/n]", action_word, item_label);
        self.enter_input_mode(
            InputMode::ConfirmDelete {
                is_trash,
                targets: Arc::from(targets),
                names: Arc::from(names),
            },
            prompt_text,
            None,
        );
    }

    /// Prompts the user to rename the selected entry.
    pub(super) fn prompt_rename(&mut self) {
        if let Some(entry) = self.nav.selected_entry() {
            let name = entry.name().to_string_lossy().to_string();
            self.enter_input_mode(InputMode::Rename, "Rename: ".to_string(), Some(name));
        }
    }

    /// Prompts the user to create a new file.
    pub(super) fn prompt_create_file(&mut self) {
        self.enter_input_mode(InputMode::NewFile, "New File: ".to_string(), None);
    }

    /// Prompts the user to create a new folder.
    pub(super) fn prompt_create_folder(&mut self) {
        self.enter_input_mode(InputMode::NewFolder, "New Folder: ".to_string(), None);
    }

    /// Prompts the user to enter a filter string.
    pub(super) fn prompt_filter(&mut self) {
        let current_filter = self.nav.active_filter().to_string();
        self.enter_input_mode(
            InputMode::Filter,
            "Filter: ".to_string(),
            Some(current_filter),
        );
    }

    /// Prompts the user to enter a fuzzy find query.
    /// Requires the `fd` tool to be installed.
    /// If `fd` is not found, displays a temporary overlay message.
    pub(super) fn prompt_find(&mut self) {
        if os::fd_binary().is_err() {
            self.push_overlay_message(
                "Fuzzy Find requires the `fd` tool.",
                timings::message::DEFAULT,
            );
            return;
        }
        self.enter_input_mode(InputMode::Find, "".to_string(), None);
    }

    pub(super) fn prompt_move(&mut self) {
        let prompt = "Move to directory: ".to_string();
        self.enter_input_mode(InputMode::MoveFile, prompt, None);
    }

    fn prompt_go_to_path(&mut self) {
        self.enter_input_mode(InputMode::GoToPath, "Go To Path:".to_string(), None);
    }

    /// Handles the autocomplete for move to directory action
    fn tab_autocomplete(&mut self) {
        if os::fd_binary().is_err() {
            return;
        }

        let input = self.actions.input_buffer().to_string();
        let expanded = path::expand_home_path(input.trim());

        let (base_dir, prefix) = if let Some(idx) = expanded.rfind(MAIN_SEPARATOR) {
            let (base, frag) = expanded.split_at(idx + 1);
            (Path::new(base), frag)
        } else {
            (self.nav.current_dir(), expanded.as_str())
        };

        let show_hidden = self.config.general().show_hidden();

        let suggestion_opt = {
            let ac = self.actions.autocomplete_mut();

            let needs_update = ac.last_input() != input || ac.suggestions().is_empty();
            if needs_update {
                let suggestions =
                    proc::complete_dirs_with_fd(base_dir, prefix, show_hidden).unwrap_or_default();
                ac.update(suggestions, &input);
            }

            let suggestion = ac.current().cloned();

            if suggestion.is_some() {
                ac.advance();
            }

            suggestion
        };

        if let Some(suggestion) = suggestion_opt {
            let mut completed_path = base_dir.to_path_buf();
            completed_path.push(&suggestion);
            let mut out = completed_path.to_string_lossy().to_string();
            if !out.ends_with(MAIN_SEPARATOR) {
                out.push(MAIN_SEPARATOR);
            }
            self.actions.set_input_buffer(out);
        }
    }
}
