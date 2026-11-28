//! Action context and input mode logic for runa.
//!
//! Contains the [ActionContext] struct, tracking user input state,
//! clipboard, and action modes.
//! Defines available modes/actions for file operations like
//! copy, paste, rename, create, delete, filter.

use std::cell::Cell;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crossbeam_channel::Sender;

use crate::app::{Clipboard, keymap::KeyPrefix, nav::NavState};
use crate::core::{proc::FindResult, workers::FileOperation};

/// Describes the current mode for action handling/input.
///
/// Used to determine which UI overlays, prompts, or context actions should be active.
///
/// Used by [ActionContext] to track current user action state.
#[derive(Clone, PartialEq)]
pub(crate) enum ActionMode {
    Normal,
    Input { mode: InputMode, prompt: String },
}

/// Enumerates all the available input field modes
///
/// Used to select the prompts, behavior and the style of the input dialog.
#[derive(Clone, PartialEq)]
pub(crate) enum InputMode {
    Rename,
    NewFile,
    NewFolder,
    Filter,
    ConfirmDelete {
        is_trash: bool,
    },
    ConfirmTrash {
        empty: bool,
    },
    ConfirmOverwrite {
        is_dir: bool,
        old: Option<Arc<PathBuf>>,
        new: Arc<PathBuf>,
    },
    Find,
    MoveFile,
    GoToPath,
}

/// Tracks current user action and input buffer state for file operations and commands.
///
/// Stores the current mode/prompt, input buffer, cursor, and clipboard (for copy/yank) status.
/// Handles mutation for input, clipboard & command responses.
///
/// Used by the main application loop to manage user interactions.
/// Includes methods for performing actions like copy, paste, delete, rename, create, and filter
///
/// Also manages fuzzy find state via the embedded [FindState] struct.
///
/// Methods to manipulate input, clipboard, and perform file actions.
/// Also find management methods.
pub(crate) struct ActionContext {
    mode: ActionMode,
    input_buffer: String,
    input_cursor_pos: usize,
    autocomplete: AutoCompleteState,
    prefix_recognizer: KeyPrefix,
    find: FindState,
    scroll: ScrollState,
}

impl ActionContext {
    crate::getters! {
        mode: &ActionMode,
        input_buffer: &str,
        input_cursor_pos: usize,
        scroll: &ScrollState,
        prefix_recognizer: &KeyPrefix,
        find: &FindState,
    }

    pub(crate) fn prefix_recognizer_mut(&mut self) -> &mut KeyPrefix {
        &mut self.prefix_recognizer
    }

    pub(crate) fn autocomplete_mut(&mut self) -> &mut AutoCompleteState {
        &mut self.autocomplete
    }

    // Find functions

    pub(crate) fn find_mut(&mut self) -> &mut FindState {
        &mut self.find
    }

    pub(crate) fn take_query(&mut self) -> Option<String> {
        self.find.take_query(&self.input_buffer)
    }

    pub(crate) fn find_debounce(&mut self, delay: Duration) {
        self.find.set_debounce(delay);
    }

    pub(crate) fn set_cancel_find_token(&mut self, token: Arc<AtomicBool>) {
        self.find.set_cancel(token);
    }

    pub(crate) fn set_input_buffer(&mut self, new_buf: String) {
        self.input_cursor_pos = new_buf.len();
        self.input_buffer = new_buf;
    }

    // Mode functions

    #[inline]
    pub(crate) fn is_input_mode(&self) -> bool {
        matches!(self.mode, ActionMode::Input { .. })
    }

    pub(crate) fn enter_mode(&mut self, mode: ActionMode, initial_value: String) {
        self.mode = mode;
        self.input_buffer = initial_value;
        self.input_cursor_pos = self.input_buffer.len();
    }

    pub(crate) fn exit_mode(&mut self) {
        self.mode = ActionMode::Normal;
        self.input_buffer.clear();
        self.find.reset();
        self.autocomplete.reset();
    }

    // Actions functions

    /// Deletes the currently marked files or the selected file if no markers exist.
    ///
    /// Sends a delete task to the worker thread via the provided channel.
    pub(crate) fn action_delete(
        &mut self,
        nav: &mut NavState,
        worker_tx: &Sender<FileOperation>,
        move_to_trash: bool,
    ) {
        let targets = nav.get_action_targets();
        if targets.is_empty() {
            return;
        }

        let _ = worker_tx.send(FileOperation::Delete(
            targets.into_iter().collect(),
            move_to_trash,
        ));

        nav.clear_markers();
    }

    /// Copies or cuts the currently marked files or the selected file if no markers exist.
    /// Stores the paths in the clipboard along with the cut flag.
    pub(crate) fn action_copy(
        &mut self,
        nav: &mut NavState,
        clipboard: &mut Clipboard,
        is_cut: bool,
    ) {
        let mut set = HashSet::new();
        if !nav.markers().is_empty() {
            for path in nav.markers() {
                set.insert(path.clone());
            }
            nav.clear_markers();
        } else if let Some(entry) = nav.selected_entry() {
            set.insert(nav.current_dir().join(entry.name()));
        }
        if !set.is_empty() {
            clipboard.set(set, is_cut);
        }
    }

    /// Pastes the files from the clipboard into the current directory.
    ///
    /// Sends a copy task to the worker thread via the provided channel.
    pub(crate) fn action_paste(
        &mut self,
        nav: &mut NavState,
        clipboard: &mut Clipboard,
        worker_tx: &Sender<FileOperation>,
    ) {
        if let Some(source) = clipboard.entries() {
            let first_file_name = source
                .iter()
                .min()
                .and_then(|p| p.file_name())
                .map(|n| n.to_os_string());

            let _ = worker_tx.send(FileOperation::Copy {
                src: source.iter().cloned().collect(),
                dest: nav.current_dir().to_path_buf(),
                cut: clipboard.is_cut(),
                focus: first_file_name,
            });

            if clipboard.is_cut() {
                clipboard.clear();
            }
            nav.clear_markers();
        }
    }

    /// Applies the current input buffer as a filter to the navigation state.
    ///
    /// Sets the filter string in the navigation state.
    pub(crate) fn action_filter(&mut self, nav: &mut NavState) {
        nav.set_filter(self.input_buffer.clone());
    }

    /// Renames the currently selected file or folder to the name in the input buffer.
    ///
    /// Sends a rename task to the worker thread via the provided channel.
    ///
    /// Exits input mode after performing the action.
    pub(crate) fn action_rename(&mut self, nav: &mut NavState, worker_tx: &Sender<FileOperation>) {
        if self.input_buffer.is_empty() {
            return;
        }
        if let Some(entry) = nav.selected_entry() {
            let old_path = nav.current_dir().join(entry.name());
            let new_path = old_path.with_file_name(&self.input_buffer);

            let _ = worker_tx.send(FileOperation::Rename {
                old: old_path,
                new: new_path,
                overwrite: false,
            });
        }
        self.exit_mode();
    }

    /// Creates a new file or directory with the name in the input buffer.
    ///
    /// Sends a create task to the worker thread via the provided channel.
    ///
    /// Exits input mode after performing the action.
    pub(crate) fn action_create(
        &mut self,
        nav: &mut NavState,
        is_dir: bool,
        worker_tx: &Sender<FileOperation>,
    ) {
        if self.input_buffer.is_empty() {
            return;
        }

        let path = nav.current_dir().join(&self.input_buffer);
        let _ = worker_tx.send(FileOperation::Create {
            path,
            is_dir,
            overwrite: false,
        });
        self.exit_mode();
    }

    pub(crate) fn actions_move(
        &mut self,
        nav: &mut NavState,
        destination: PathBuf,
        worker_tx: &Sender<FileOperation>,
    ) {
        let targets = nav.get_action_targets();
        if targets.is_empty() {
            return;
        }
        let _ = worker_tx.send(FileOperation::Copy {
            src: targets.into_iter().collect(),
            dest: destination,
            cut: true,
            focus: None,
        });
        nav.clear_markers();
    }

    // Cursor actions

    /// Moves the input cursor by one character to the left, if possible.
    pub(crate) fn action_move_cursor_left(&mut self) {
        if let Some((previous, _)) = self.input_buffer[..self.input_cursor_pos]
            .char_indices()
            .next_back()
        {
            self.input_cursor_pos = previous;
        }
    }

    /// Moves the input cursor by one character to the right, if possible.
    pub(crate) fn action_move_cursor_right(&mut self) {
        if let Some(ch) = self.input_buffer[self.input_cursor_pos..].chars().next() {
            self.input_cursor_pos += ch.len_utf8();
        }
    }

    /// Inserts a character at the current cursor position in the input buffer.
    pub(crate) fn action_insert_at_cursor(&mut self, ch: char) {
        self.input_buffer.insert(self.input_cursor_pos, ch);
        self.input_cursor_pos += ch.len_utf8();
    }

    /// Deletes the character before the current cursor position in the input buffer.
    ///
    /// Moves the cursor back accordingly
    pub(crate) fn action_backspace_at_cursor(&mut self) {
        if self.input_cursor_pos > 0
            && let Some((previous, _)) = self.input_buffer[..self.input_cursor_pos]
                .char_indices()
                .next_back()
        {
            self.input_buffer.remove(previous);
            self.input_cursor_pos = previous;
        }
    }

    /// Moves the input cursor to the start of the input buffer.
    pub(crate) fn action_cursor_home(&mut self) {
        self.input_cursor_pos = 0;
    }

    /// Moves the input cursor to the end of the input buffer.
    pub(crate) fn action_cursor_end(&mut self) {
        self.input_cursor_pos = self.input_buffer.len();
    }

    pub(crate) fn action_rename_overwrite(
        &mut self,
        old: Arc<PathBuf>,
        new: Arc<PathBuf>,
        worker_tx: &Sender<FileOperation>,
    ) {
        let _ = worker_tx.send(FileOperation::Rename {
            old: (*old).clone(),
            new: (*new).clone(),
            overwrite: true,
        });
        self.exit_mode();
    }

    pub(crate) fn action_create_overwrite(
        &mut self,
        path: Arc<PathBuf>,
        is_dir: bool,
        worker_tx: &Sender<FileOperation>,
    ) {
        let _ = worker_tx.send(FileOperation::Create {
            path: (*path).clone(),
            is_dir,
            overwrite: true,
        });
        self.exit_mode();
    }
}

impl Default for ActionContext {
    fn default() -> Self {
        Self {
            mode: ActionMode::Normal,
            input_buffer: String::new(),
            input_cursor_pos: 0,
            autocomplete: AutoCompleteState::default(),
            prefix_recognizer: KeyPrefix::new(Duration::from_secs(4)),
            find: FindState::default(),
            scroll: ScrollState::default(),
        }
    }
}

/// Tracks the state of an ongoing fuzzy find operation.
///
/// It includes the cached results, request ID, debounce timer, last query,
/// selected result index, and cancellation token.
///
/// Used by [ActionContext] to manage fuzzy find operations.
/// Methods to manage results, requests, selection, and cancellation.
#[derive(Default)]
pub(crate) struct FindState {
    cache: Vec<FindResult>,
    request_id: u64,
    debounce: Option<Instant>,
    last_query: String,
    selected: usize,
    cancel: Option<Arc<AtomicBool>>,
}

impl FindState {
    crate::getters! {
        request_id: u64,
        selected: usize,
        results => cache: &[FindResult],
    }

    /// Cancels the current ongoing find operation, if any.
    ///
    /// Sets the cancellation token to true.
    pub(crate) fn cancel_current(&mut self) {
        if let Some(token) = self.cancel.take() {
            token.store(true, Ordering::Relaxed);
        }
    }

    /// Sets the cached find results and resets the selected index.
    pub(crate) fn set_results(&mut self, results: Vec<FindResult>) {
        self.cache = results;
        self.selected = 0;
    }

    /// Clears the cached find results.
    pub(crate) fn clear_results(&mut self) {
        self.cache.clear();
    }

    /// Prepares a new unique request ID for a find operation.
    /// Increments the internal request ID counter.
    pub(crate) fn prepare_new_request(&mut self) -> u64 {
        self.request_id = self.request_id.wrapping_add(1);
        self.request_id
    }

    /// Takes the current query if the debounce period has elapsed and its different from the last query.
    pub(crate) fn take_query(&mut self, current_query: &str) -> Option<String> {
        let until = self.debounce?;
        if Instant::now() < until {
            return None;
        }

        self.debounce = None;
        if current_query == self.last_query {
            self.last_query.clear();
            return None;
        }

        self.last_query.clear();
        self.last_query.push_str(current_query);
        Some(current_query.to_string())
    }

    /// Resets the find state, clearing cache, debounce, and last query.
    fn reset(&mut self) {
        self.cancel_current();
        self.cache.clear();
        self.debounce = None;
        self.last_query.clear();
    }

    /// Moves the selection to the next result in the cached results.
    pub(crate) fn select_next(&mut self) {
        if self.selected + 1 < self.cache.len() {
            self.selected += 1;
        }
    }

    /// Moves the selection to the previous result in the cached results.
    pub(crate) fn select_prev(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    /// Sets the cancellation token for the current find operation.
    ///
    /// Sets the internal cancel token.
    fn set_cancel(&mut self, token: Arc<AtomicBool>) {
        self.cancel = Some(token);
    }

    /// Sets the debounce timer for the find operation.
    fn set_debounce(&mut self, delay: Duration) {
        self.debounce = Some(Instant::now() + delay);
    }
}

#[derive(Default)]
pub(crate) struct AutoCompleteState {
    suggestions: Vec<String>,
    index: usize,
    last_input: String,
}

impl AutoCompleteState {
    crate::getters! {
        suggestions: &Vec<String>,
        last_input: &str,
    }

    pub(crate) fn reset(&mut self) {
        self.suggestions.clear();
        self.index = 0;
        self.last_input.clear();
    }

    pub(crate) fn update(&mut self, suggestions: Vec<String>, input: &str) {
        self.suggestions = suggestions;
        self.index = 0;
        self.last_input = input.to_string();
    }

    pub(crate) fn advance(&mut self) {
        if !self.suggestions.is_empty() {
            self.index = (self.index + 1) % self.suggestions.len();
        }
    }

    pub(crate) fn current(&self) -> Option<&String> {
        self.suggestions.get(self.index)
    }
}

#[derive(Default)]
pub(crate) struct ScrollState {
    offset: Cell<usize>,
    max_offset: Cell<usize>,
    h_offset: Cell<usize>,
    max_h_offset: Cell<usize>,
}

impl ScrollState {
    pub(crate) fn offset(&self) -> usize {
        self.offset.get()
    }

    pub(crate) fn h_offset(&self) -> usize {
        self.h_offset.get()
    }

    pub(crate) fn set_offset(&self, offset: usize) {
        self.offset.set(offset);
    }

    pub(crate) fn set_max_offset(&self, max: usize) {
        self.max_offset.set(max);
        if self.offset.get() > max {
            self.offset.set(max);
        }
    }

    pub(crate) fn set_max_h_offset(&self, max: usize) {
        self.max_h_offset.set(max);
        if self.h_offset.get() > max {
            self.h_offset.set(max);
        }
    }

    pub(crate) fn scroll_up(&self) {
        self.offset.set(self.offset.get().saturating_sub(1));
    }

    pub(crate) fn scroll_down(&self) {
        let current = self.offset.get();
        let max = self.max_offset.get();
        if current < max {
            self.offset.set(current.saturating_add(1));
        }
    }

    pub(crate) fn scroll_left(&self, step: usize) {
        self.h_offset.set(self.h_offset.get().saturating_sub(step));
    }

    pub(crate) fn scroll_right(&self, step: usize) {
        let next = self
            .h_offset
            .get()
            .saturating_add(step)
            .min(self.max_h_offset.get());
        self.h_offset.set(next);
    }

    pub(crate) fn scroll_to_bottom(&self) {
        self.offset.set(self.max_offset.get());
    }

    pub(crate) fn reset_h(&self) {
        self.h_offset.set(0);
    }

    pub(crate) fn reset(&self) {
        self.offset.set(0);
        self.max_offset.set(0);
        self.h_offset.set(0);
        self.max_h_offset.set(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_moves_by_char() {
        let mut ctx = ActionContext::default();
        ctx.action_insert_at_cursor('è');
        ctx.action_move_cursor_left();
        ctx.action_insert_at_cursor('a');
        ctx.action_move_cursor_right();
        ctx.action_backspace_at_cursor();

        assert_eq!(ctx.input_buffer, "a");
        assert_eq!(ctx.input_cursor_pos, 1);
    }
}
