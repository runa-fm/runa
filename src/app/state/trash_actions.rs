//! Trash view action handlers for AppState.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::{
    ListNav, TrashState,
    actions::InputMode,
    keymap::{Action, FileAction, NavAction},
    state::{AppState, KeypressResult},
};
use crate::core::workers::{FileOperation, TrashOperation, Workers};

impl AppState {
    pub(crate) fn request_trash_list(&mut self, workers: &Workers) {
        if let Some(trash) = self.trash.as_mut() {
            trash.set_loading(true);
        }
        let _ = workers.fileop_tx().send(FileOperation::Trash {
            op: TrashOperation::List,
            tab_id: self.tab_id(),
        });
    }

    pub(super) fn enter_trash(&mut self, workers: &Workers) {
        let can_exit = self.trash.as_ref().is_none_or(TrashState::can_exit);
        self.preview.clear();
        self.trash = Some(TrashState::new_loading(can_exit));
        self.request_trash_list(workers);
    }

    fn exit_trash(&mut self, workers: &Workers) {
        if !self.trash.as_ref().is_some_and(TrashState::can_exit) {
            return;
        }

        self.trash = None;
        self.preview.clear();
        self.request_dir_reload(workers);
        self.request_parent_content(workers);
        self.request_preview_force(workers);
        self.update_file_info_cache(workers);
    }

    pub(super) fn confirm_trash(&mut self, workers: &Workers, empty: bool) {
        let Some(trash) = self.trash.as_mut() else {
            return;
        };

        let op = if empty {
            TrashOperation::Empty
        } else {
            let items = trash.target_items();
            if items.is_empty() {
                return;
            }
            TrashOperation::Purge(items)
        };

        trash.clear_markers();
        trash.set_loading(true);
        let _ = workers.fileop_tx().send(FileOperation::Trash {
            op,
            tab_id: self.tab_id(),
        });
    }

    pub(super) fn handle_trash_keypress(
        &mut self,
        key: KeyEvent,
        workers: &Workers,
    ) -> KeypressResult {
        if let KeyCode::Char('E') = key.code
            && (key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT)
        {
            if let Some(trash) = self.trash.as_ref()
                && !trash.items().is_empty()
            {
                let count = trash.items().len();
                let label = if count == 1 { "item" } else { "items" };
                self.enter_input_mode(
                    InputMode::ConfirmTrash { empty: true },
                    format!("Empty trash ({count} {label})? (y/n)"),
                    None,
                );
            }
            return KeypressResult::Consumed;
        }

        if let KeyCode::Char('r') = key.code
            && key.modifiers.is_empty()
        {
            if let Some(trash) = self.trash.as_mut() {
                let items = trash.target_items();
                if !items.is_empty() {
                    trash.clear_markers();
                    trash.set_loading(true);
                    let _ = workers.fileop_tx().send(FileOperation::Trash {
                        op: TrashOperation::Restore(items),
                        tab_id: self.tab_id(),
                    });
                }
            }
            return KeypressResult::Consumed;
        }

        let Some(action) = self.keymap.lookup(key) else {
            return KeypressResult::Consumed;
        };

        match action {
            Action::System(sys_act) => {
                let res = self.handle_sys_action(sys_act);
                if matches!(res, KeypressResult::UiReload) {
                    self.request_trash_list(workers);
                }
                res
            }
            Action::Tab(tab_act) => KeypressResult::Tab(tab_act),
            Action::Nav(NavAction::GoUp) => {
                if let Some(trash) = self.trash.as_mut() {
                    trash.move_up();
                }
                KeypressResult::Consumed
            }
            Action::Nav(NavAction::GoDown) => {
                if let Some(trash) = self.trash.as_mut() {
                    trash.move_down();
                }
                KeypressResult::Consumed
            }
            Action::Nav(NavAction::GoToBottom) => {
                if let Some(trash) = self.trash.as_mut() {
                    trash.select_last();
                }
                KeypressResult::Consumed
            }
            Action::Nav(NavAction::GoParent) => {
                self.exit_trash(workers);
                KeypressResult::Consumed
            }
            Action::Nav(NavAction::ToggleMarker) => {
                let jump = self.config.display().toggle_marker_jump();
                if let Some(trash) = self.trash.as_mut() {
                    trash.toggle_marker_advance(jump);
                }
                KeypressResult::Consumed
            }
            Action::Nav(NavAction::SelectAll) => {
                if let Some(trash) = self.trash.as_mut() {
                    trash.select_all();
                }
                KeypressResult::Consumed
            }
            Action::Nav(NavAction::ClearMarker | NavAction::ClearAll) => {
                if let Some(trash) = self.trash.as_mut() {
                    trash.clear_markers();
                }
                KeypressResult::Consumed
            }
            Action::File(FileAction::Delete | FileAction::AlternateDelete) => {
                if let Some(trash) = self.trash.as_ref()
                    && !trash.target_items().is_empty()
                {
                    let count = trash.target_items().len();
                    let label = if count == 1 { "item" } else { "items" };
                    self.enter_input_mode(
                        InputMode::ConfirmTrash { empty: false },
                        format!("Permanently delete {count} {label}? (y/n)"),
                        None,
                    );
                }
                KeypressResult::Consumed
            }
            _ => KeypressResult::Consumed,
        }
    }
}
