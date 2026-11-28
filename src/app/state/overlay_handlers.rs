//! Overlay handlers for AppState.
//!
//! This module defines the overlay-related functions for the central
//! [app::state::handle_keypress](AppState::handle_keypress) function.

use std::time::{Duration, Instant};

use crossterm::event::{KeyCode::*, KeyEvent};
use ratatui::layout::Alignment;

use crate::app::overlay_stack::{CommandOutputOverlay, Overlay, OverlayKind};
use crate::app::state::{AppState, KeypressResult};
use crate::core::workers::Workers;

/// AppState input and action handlers
impl AppState {
    pub(in crate::app) fn handle_modal_overlay_keypress(
        &mut self,
        key: &KeyEvent,
        workers: &Workers,
    ) -> Option<KeypressResult> {
        if !self.overlays().is_open(OverlayKind::CommandOutput) {
            return None;
        }

        match key.code {
            Esc | Char('q') => self.close_command_output(workers),
            Up | Char('k') => self.actions.scroll().scroll_up(),
            Down | Char('j') => self.actions.scroll().scroll_down(),
            Left | Char('h') => self.actions.scroll().scroll_left(4),
            Right | Char('l') => self.actions.scroll().scroll_right(4),
            Char('0') => self.actions.scroll().reset_h(),
            PageUp => {
                for _ in 0..10 {
                    self.actions.scroll().scroll_up();
                }
            }
            PageDown => {
                for _ in 0..10 {
                    self.actions.scroll().scroll_down();
                }
            }
            Home | Char('g') => self.actions.scroll().set_offset(0),
            End | Char('G') => self.actions.scroll().scroll_to_bottom(),
            _ => {}
        }

        Some(KeypressResult::Consumed)
    }

    pub(crate) fn close_command_output(&mut self, workers: &Workers) {
        if let Some(id) = self.overlays.running_command_output_id() {
            workers.cancel_command(id);
        }
        self.actions.scroll().reset();
        self.overlays_mut().remove_kind(OverlayKind::CommandOutput);
    }

    pub(super) fn open_command_output(&mut self, id: u64, title: String, workers: &Workers) {
        self.close_command_output(workers);
        self.overlays_mut().push(Overlay::CommandOutput(Box::new(
            CommandOutputOverlay::new_running(id, title),
        )));
    }
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

    pub(crate) fn transfer_shared_overlays_to(&mut self, target: &mut AppState) {
        let shared = self.overlays.take_shared(&mut self.notification_time);
        target
            .overlays
            .restore_shared(shared, &mut target.notification_time);
    }

    /// Handles displaying a timed message overlay.
    pub(super) fn handle_timed_message(&mut self, duration: Duration) {
        self.notification_time = Some(Instant::now() + duration);
    }

    /// Toggles the file info overlay.
    pub(super) fn toggle_file_info(&mut self, workers: &Workers) {
        if self.overlays().is_open(OverlayKind::ShowInfo) {
            self.overlays_mut().remove_kind(OverlayKind::ShowInfo);
        } else {
            self.overlays_mut().push(Overlay::ShowInfo);
            self.update_file_info_cache(workers);
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

        self.overlays_mut().remove_kind(OverlayKind::Message);

        self.overlays_mut().push(Overlay::Message {
            text: text.into(),
            alignment,
        });
    }
}
