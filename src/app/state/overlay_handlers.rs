//! Overlay handlers for AppState.
//!
//! This module defines the overlay-related functions for the central
//! [app::state::handle_keypress](AppState::handle_keypress) function.

use std::sync::Arc;
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode::*, KeyEvent};
use ratatui::layout::Alignment;

use crate::app::overlay_stack::{Overlay, OverlayKind};
use crate::app::state::{AppState, KeypressResult};
use crate::core::metadata::FileMetadataCache;

/// AppState input and action handlers
impl AppState {
    pub(in crate::app) fn handle_esc_close_overlays(
        &mut self,
        key: &KeyEvent,
    ) -> Option<KeypressResult> {
        if key.code != Esc {
            return None;
        }

        self.actions.scroll().reset();

        if self.overlays().is_open(OverlayKind::KeybindHelp) {
            self.overlays_mut().remove_kind(OverlayKind::KeybindHelp);
            return Some(KeypressResult::Consumed);
        }
        None
    }

    /// Handles displaying a timed message overlay.
    pub(super) fn handle_timed_message(&mut self, duration: Duration) {
        self.notification_time = Some(Instant::now() + duration);
    }

    // Input processes

    /// Refreshes the file info overlay if it is currently open.
    pub(crate) fn refresh_show_info_if_open(&mut self) {
        if let Some(new_info) = self.selected_file_info_clone()
            && let Some(info_slot) = self.overlays_mut().find_show_info_mut()
            && !Arc::ptr_eq(&new_info, info_slot)
        {
            *info_slot = new_info;
        }
    }

    /// Shows the file info overlay for the currently selected entry.
    fn show_file_info(&mut self) {
        if let Some(info) = self.selected_file_info_clone() {
            self.overlays_mut().push(Overlay::ShowInfo { info });
        }
    }

    /// Toggles the file info overlay.
    pub(super) fn toggle_file_info(&mut self) {
        let is_open = self.overlays().is_open(OverlayKind::ShowInfo);

        if is_open {
            self.overlays_mut().remove_kind(OverlayKind::ShowInfo);
        } else {
            self.show_file_info();
        }
    }

    pub(super) fn show_prefix_help(&mut self) {
        if !self.overlays().is_top(OverlayKind::PrefixHelp) {
            self.overlays_mut().push(Overlay::PrefixHelp);
        }
    }

    pub(crate) fn hide_prefix_help(&mut self) {
        if self.overlays().is_top(OverlayKind::PrefixHelp) {
            self.overlays_mut().pop();
        }
    }

    pub(super) fn toggle_keybind_help(&mut self) {
        let is_open = self.overlays().is_open(OverlayKind::KeybindHelp);

        if is_open {
            self.overlays_mut().remove_kind(OverlayKind::KeybindHelp);
        } else {
            self.overlays_mut().push(Overlay::KeybindHelp);
        }
    }

    /// Pushes a message overlay that lasts for the specified duration.
    pub(crate) fn push_overlay_message(&mut self, text: impl Into<String>, duration: Duration) {
        self.push_overlay_message_aligned(text, duration, Alignment::Center);
    }

    /// Pushes a text aligned message overlay that lasts for the specified duration.
    pub(crate) fn push_overlay_message_aligned(
        &mut self,
        text: impl Into<String>,
        duration: Duration,
        alignment: Alignment,
    ) {
        self.notification_time = Some(Instant::now() + duration);

        if self.overlays().is_top(OverlayKind::Message) {
            self.overlays_mut().pop();
        }

        self.overlays_mut().push(Overlay::Message {
            text: text.into(),
            alignment,
        });
    }

    fn selected_file_info_clone(&self) -> Option<Arc<FileMetadataCache>> {
        self.file_info.selected_arc().map(Arc::clone)
    }
}
