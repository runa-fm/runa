//! File action handlers for AppState.
//!
//! Defines [handle_file_action](AppState::handle_file_action) method which dispatches file actions
//! to the central [app::state::handle_keypress](AppState::handle_keypress).
//!
//! Also defines handlers for each file action (open, delete, copy, etc)
//! which perform the necessary state updates
//! and call the appropriate methods in the actions module.
//!
//! This module is responsible for connecting file actions
//! to the underlying logic that performs those actions,

use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

use crossterm::{
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};

use crate::app::{
    Clipboard,
    actions::{ActionMode, InputMode},
    keymap::FileAction,
    state::{AppState, KeypressResult},
};
use crate::config::commands::{self, RunMode};
use crate::config::editor::EditorCommand;
use crate::core::workers::{CommandTask, Workers};
use crate::utils::{path, timings::message};

/// AppState file action handlers
impl AppState {
    /// Handles file actions (open, delete, copy, etc).
    /// Returns a [KeypressResult] indicating how the action was handled.
    pub(in crate::app) fn handle_file_action(
        &mut self,
        workers: &Workers,
        action: FileAction,
        clipboard: &mut Clipboard,
    ) -> KeypressResult {
        match action {
            FileAction::Open => return self.handle_open_file(workers),
            FileAction::Delete => {
                let is_trash = self.config.general().move_to_trash();
                self.prompt_delete(is_trash);
            }
            FileAction::AlternateDelete => {
                let is_trash = !self.config.general().move_to_trash();
                self.prompt_delete(is_trash);
            }
            FileAction::Copy => {
                let nav = &mut self.nav;
                self.actions.action_copy(nav, clipboard, false);
                self.handle_timed_message(Duration::from_secs(15));
            }
            FileAction::Paste => {
                let fileop_tx = workers.fileop_tx();
                self.actions
                    .action_paste(&mut self.nav, clipboard, fileop_tx);
            }
            FileAction::Rename => self.prompt_rename(),
            FileAction::Create => self.prompt_create_file(),
            FileAction::CreateDirectory => self.prompt_create_folder(),
            FileAction::Filter => self.prompt_filter(),
            FileAction::ShowInfo => self.toggle_file_info(),
            FileAction::Find => self.prompt_find(),
            FileAction::MoveFile => self.prompt_move(),
            FileAction::ClearClipboard => {
                clipboard.clear();
                self.request_preview(workers);
            }
        }
        KeypressResult::Continue
    }

    /// Handles the open file action.
    ///
    /// If a file is selected, attempts to open it in the configured editor.
    /// If an error occurs, prints it to stderr.
    fn handle_open_file(&mut self, workers: &Workers) -> KeypressResult {
        if let Some(entry) = self.nav.selected_entry() {
            let current_dir = self.nav.current_dir();
            let dir =
                std::path::absolute(current_dir).unwrap_or_else(|_| current_dir.to_path_buf());
            let path = dir.join(entry.name());
            let focus = Some(entry.name().to_os_string());

            let (binary, args, run_mode) = match self.config.editor().cmd(&path) {
                EditorCommand::System => {
                    let args = vec![path.into_os_string()];
                    let run_mode = RunMode::Background {
                        check_status: false,
                    };
                    (commands::SYSTEM_OPENER, args, run_mode)
                }
                EditorCommand::Program {
                    args,
                    run_mode,
                    ext_parts,
                } => {
                    let Some(binary) = args.first() else {
                        let msg = "No editor command configured".to_string();
                        self.push_overlay_message(msg, message::DEFAULT, None);
                        return KeypressResult::Continue;
                    };
                    let args = commands::expand_args(args, ext_parts, &path);
                    (binary.as_str(), args, run_mode)
                }
            };

            let Ok(program) = which::which(binary) else {
                let msg = format!("Editor '{}' not found", binary);
                self.push_overlay_message(msg, message::DEFAULT, None);
                return KeypressResult::Continue;
            };

            self.launch(workers, program, args, dir, run_mode, focus)
        } else {
            KeypressResult::Continue
        }
    }

    /// Handles the move action
    ///
    /// Checks if the directory for files to be moved to exists
    /// Also normalizes relative paths for easier moving of files.
    pub(super) fn move_file(&mut self, workers: &Workers) {
        let dest_dir = self.actions.input_buffer();
        if dest_dir.trim().is_empty() {
            self.push_overlay_message(
                "Move failed: target directory cannot be empty".to_string(),
                message::DEFAULT,
                None,
            );
            return;
        }

        let input_path = path::expand_home_path_buf(dest_dir.trim());
        let resolved_path = if input_path.is_absolute() {
            input_path
        } else {
            self.nav.current_dir().join(input_path)
        };

        let absolute_dest = match resolved_path.canonicalize() {
            Ok(p) => p,
            Err(e) => {
                let norm_msg = path::format_display_path(&resolved_path);
                self.push_overlay_message(
                    format!("Move failed: {}: {}", e, norm_msg),
                    message::DEFAULT,
                    None,
                );
                return;
            }
        };

        if !absolute_dest.is_dir() {
            let norm_msg = path::format_display_path(&absolute_dest);
            self.push_overlay_message(
                format!("Move failed: not a directory: {}", norm_msg),
                message::DEFAULT,
                None,
            );
            return;
        }

        if let Err(e) = std::fs::read_dir(&absolute_dest) {
            let norm_msg = path::format_display_path(&absolute_dest);
            self.push_overlay_message(
                format!("Move failed: Permission denied in {}: {}", norm_msg, e),
                message::DEFAULT,
                None,
            );
            return;
        }

        let targets = self.nav.get_action_targets();
        for src in &targets {
            if let Ok(absolute_src) = src.canonicalize()
                && absolute_dest.starts_with(&absolute_src)
            {
                let msg = if absolute_dest == absolute_src {
                    "Move failed: source and destination are the same".to_string()
                } else {
                    let normalized = path::format_display_path(&absolute_src);
                    let display_path = path::clean_display_path(&normalized);
                    format!(
                        "Move failed: cannot move directory into its own sub directory: {}",
                        display_path
                    )
                };
                self.push_overlay_message(msg, message::DEFAULT, None);
                return;
            }
        }

        let fileop_tx = workers.fileop_tx();
        let move_msg = format!(
            "Files moved to: {}",
            path::clean_display_path(&absolute_dest.to_string_lossy())
        );

        self.actions
            .actions_move(&mut self.nav, absolute_dest, fileop_tx);

        self.exit_input_mode();
        self.push_overlay_message(move_msg, message::DEFAULT, None);
    }

    /// Creates a new file with the name in the input buffer.
    /// Calls actions::action_create with `is_folder` set to false.
    pub(super) fn create_file(&mut self, workers: &Workers) {
        if self.actions.input_buffer().is_empty() {
            return;
        }

        let target = self.nav.current_dir().join(self.actions.input_buffer());
        if target.exists() {
            let name = target
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let prompt_text = format!("Overwrite file '{}' ? [Y/n]", name);
            self.enter_input_mode(
                InputMode::ConfirmOverwrite {
                    is_dir: false,
                    old: None,
                    new: Arc::new(target),
                },
                prompt_text,
                None,
            );
            return;
        }

        let fileop_tx = workers.fileop_tx();
        self.actions.action_create(&mut self.nav, false, fileop_tx);
    }

    /// Creates a new folder with the name in the input buffer.
    /// Calls actions::action_create with `is_folder` set to true.
    pub(super) fn create_folder(&mut self, workers: &Workers) {
        if self.actions.input_buffer().is_empty() {
            return;
        }

        let target = self.nav.current_dir().join(self.actions.input_buffer());
        if target.exists() {
            let name = target
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let prompt_text = format!("Overwrite directory '{}' ? [Y/n]", name);
            self.enter_input_mode(
                InputMode::ConfirmOverwrite {
                    is_dir: true,
                    old: None,
                    new: Arc::new(target),
                },
                prompt_text,
                None,
            );
            return;
        }

        let fileop_tx = workers.fileop_tx();
        self.actions.action_create(&mut self.nav, true, fileop_tx);
    }

    /// Renames the selected entry to the name in the input buffer.
    /// Calls actions::action_rename.
    pub(super) fn rename_entry(&mut self, workers: &Workers) {
        if self.actions.input_buffer().is_empty() {
            return;
        }

        if !path::is_valid_entry_name(self.actions.input_buffer()) {
            self.push_overlay_message(
                "Rename failed: invalid name.".to_string(),
                message::DEFAULT,
                None,
            );
            return;
        }

        if let Some(entry) = self.nav.selected_entry() {
            let old_path = self.nav.current_dir().join(entry.name());
            let new_path = old_path.with_file_name(self.actions.input_buffer());

            if old_path == new_path {
                return;
            }

            let is_case_rename = old_path.to_string_lossy().to_lowercase()
                == new_path.to_string_lossy().to_lowercase()
                && path::is_same_entry(&old_path, &new_path);

            if new_path.exists() && !is_case_rename {
                let name = new_path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                let prompt_text = format!("Overwrite '{}' ? [Y/n]", name);
                self.enter_input_mode(
                    InputMode::ConfirmOverwrite {
                        is_dir: new_path.is_dir(),
                        old: Some(Arc::new(old_path)),
                        new: Arc::new(new_path),
                    },
                    prompt_text,
                    None,
                );
                return;
            }

            let fileop_tx = workers.fileop_tx();
            self.actions.action_rename(&mut self.nav, fileop_tx);
        }
    }

    /// Applies the filter in the input buffer to the navigation state.
    /// Calls actions::action_filter and requests a preview refresh.
    pub(super) fn apply_filter(&mut self, workers: &Workers) {
        self.actions.action_filter(&mut self.nav);
        self.request_preview(workers);
    }

    /// Confirms deletion of the selected items.
    /// Calls actions::action_delete.
    pub(super) fn confirm_delete(&mut self, workers: &Workers) {
        let move_to_trash = if let ActionMode::Input {
            mode: InputMode::ConfirmDelete { is_trash },
            ..
        } = self.actions.mode()
        {
            *is_trash
        } else {
            self.config.general().move_to_trash()
        };

        let fileop_tx = workers.fileop_tx();

        self.actions
            .action_delete(&mut self.nav, fileop_tx, move_to_trash);
    }

    pub(super) fn confirm_overwrite(&mut self, workers: &Workers) {
        if let ActionMode::Input {
            mode: InputMode::ConfirmOverwrite { is_dir, old, new },
            ..
        } = self.actions.mode()
        {
            let fileop_tx = workers.fileop_tx();
            if let Some(old_arc) = old {
                self.actions
                    .action_rename_overwrite(old_arc.clone(), new.clone(), fileop_tx);
            } else {
                self.actions
                    .action_create_overwrite(new.clone(), *is_dir, fileop_tx);
            }
        }
    }

    pub(crate) fn handle_user_command(
        &mut self,
        workers: &Workers,
        index: usize,
    ) -> KeypressResult {
        let Some(user_command) = self.config.commands().get(index) else {
            return KeypressResult::Continue;
        };

        let cmd = user_command.cmd();
        let run_mode = user_command.run_mode();

        let Some(binary) = cmd.first() else {
            let msg = format!("Command '{}' has no program", user_command.name());
            self.push_overlay_message(msg, message::DEFAULT, None);
            return KeypressResult::Continue;
        };

        let current_dir = self.nav.current_dir();
        let dir = std::path::absolute(current_dir).unwrap_or_else(|_| current_dir.to_path_buf());
        let selected = self.nav.selected_entry().map(|e| e.name().to_os_string());

        let args = if commands::has_file_variables(cmd) {
            let Some(name) = &selected else {
                let msg = format!("Command '{}' needs a selected file", user_command.name());
                self.push_overlay_message(msg, message::DEFAULT, None);
                return KeypressResult::Continue;
            };

            commands::fill_file_variables(cmd, commands::LAST_EXTENSION, &dir.join(name))
        } else {
            commands::plain_args(cmd)
        };

        let Ok(program) = which::which(binary) else {
            let msg = format!("Command '{}' not found", binary);
            self.push_overlay_message(msg, message::DEFAULT, None);
            return KeypressResult::Continue;
        };

        self.launch(workers, program, args, dir, run_mode, selected)
    }

    /// Starts a resolved command in its working directory.
    /// After a terminal command, reloads the directory and keeps `focus` selected.
    fn launch(
        &mut self,
        workers: &Workers,
        program: PathBuf,
        args: Vec<OsString>,
        cwd: PathBuf,
        run_mode: RunMode,
        focus: Option<OsString>,
    ) -> KeypressResult {
        match run_mode {
            RunMode::Background { check_status } => {
                self.run_in_background(workers, program, args, cwd, check_status);
                KeypressResult::Continue
            }
            RunMode::Terminal => match Self::run_in_terminal(&program, args, &cwd) {
                Ok(_) => {
                    self.request_dir_load(workers, focus);
                    self.request_preview_force(workers);
                    KeypressResult::OpenedEditor
                }
                Err(e) => {
                    self.push_overlay_message(e.to_string(), message::DEFAULT, None);
                    KeypressResult::Recovered
                }
            },
        }
    }

    /// Runs an editor command in the background, so the TUI keeps running.
    fn run_in_background(
        &mut self,
        workers: &Workers,
        program: PathBuf,
        args: Vec<OsString>,
        cwd: PathBuf,
        check_status: bool,
    ) {
        let task = CommandTask {
            program,
            args,
            dir: Some(cwd),
            check_status,
        };
        if workers.runner_tx().send(task).is_err() {
            let msg = "Command runner is not running".to_string();
            self.push_overlay_message(msg, message::DEFAULT, None);
        }
    }

    /// Opens a specified path/file in the configured editor ("nvim" or "vim" etc.).
    ///
    /// Temporary disables raw mode and exits alternate sceen while the editor runs.
    /// On return, restores raw mode and alternate sceen.
    fn run_in_terminal(program: &Path, args: Vec<OsString>, cwd: &Path) -> io::Result<()> {
        let mut stdout = io::stdout();
        disable_raw_mode()?;
        execute!(stdout, LeaveAlternateScreen)?;

        let mut editor_cmd = Command::new(program);
        editor_cmd.args(args).current_dir(cwd);

        let status = editor_cmd.status();

        execute!(io::stdout(), EnterAlternateScreen)?;
        enable_raw_mode()?;
        match status {
            Ok(s) if s.success() => Ok(()),
            Ok(s) => Err(io::Error::other(format!(
                "Editor exited with status: {}",
                s
            ))),
            Err(e) => Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("Command '{}' not found: {}", program.display(), e),
            )),
        }
    }
}
