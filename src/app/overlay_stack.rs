//! Ovelay module to seamless stack widgets, dialogs with each other.
//! Currently handles ShowInfo as a overlay.
//!
//! Can be expanded to hanlde more widget types for more functions.
//!
//! Is used throughout the ui modules and in AppState handlers.

use std::{slice, time::Instant};

use ratatui::layout::Alignment;
use ratatui::text::Text;

pub(crate) struct CommandOutputOverlay {
    id: u64,
    title: String,
    text: Text<'static>,
    running: bool,
    success: bool,
    exit_code: Option<i32>,
    truncated: bool,
}

pub(crate) enum Overlay {
    ShowInfo,
    Message { text: String, alignment: Alignment },
    PrefixHelp,
    KeybindHelp,
    CommandOutput(Box<CommandOutputOverlay>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OverlayKind {
    ShowInfo,
    Message,
    PrefixHelp,
    KeybindHelp,
    CommandOutput,
}

pub(crate) struct OverlayStack {
    overlays: Vec<Overlay>,
}

pub(crate) struct SharedOverlays {
    overlays: Vec<Overlay>,
    notification_time: Option<Instant>,
}

impl CommandOutputOverlay {
    pub(crate) fn new_running(id: u64, title: String) -> Self {
        Self {
            id,
            title,
            text: Text::default(),
            running: true,
            success: true,
            exit_code: None,
            truncated: false,
        }
    }

    pub(crate) fn finish(
        &mut self,
        text: Text<'static>,
        success: bool,
        exit_code: Option<i32>,
        truncated: bool,
    ) {
        self.text = text;
        self.running = false;
        self.success = success;
        self.exit_code = exit_code;
        self.truncated = truncated;
    }

    crate::getters! {
        id: u64,
        title: &str,
        text: &Text<'static>,
        running: bool,
        success: bool,
        exit_code: Option<i32>,
        truncated: bool,
    }
}

impl Overlay {
    pub(crate) const fn kind(&self) -> OverlayKind {
        match self {
            Overlay::ShowInfo => OverlayKind::ShowInfo,
            Overlay::Message { .. } => OverlayKind::Message,
            Overlay::PrefixHelp => OverlayKind::PrefixHelp,
            Overlay::KeybindHelp => OverlayKind::KeybindHelp,
            Overlay::CommandOutput(_) => OverlayKind::CommandOutput,
        }
    }

    pub(crate) fn is_tab_shared(&self) -> bool {
        match self {
            Overlay::ShowInfo | Overlay::Message { .. } => true,
            Overlay::PrefixHelp | Overlay::KeybindHelp | Overlay::CommandOutput(_) => false,
        }
    }
}

impl OverlayStack {
    pub(crate) const fn new() -> Self {
        Self {
            overlays: Vec::new(),
        }
    }

    pub(crate) fn push(&mut self, overlay: Overlay) {
        self.overlays.push(overlay);
    }

    pub(crate) fn pop(&mut self) -> Option<Overlay> {
        self.overlays.pop()
    }

    pub(crate) fn top(&self) -> Option<&Overlay> {
        self.overlays.last()
    }

    pub(crate) fn iter(&self) -> slice::Iter<'_, Overlay> {
        self.overlays.iter()
    }

    pub(crate) fn retain<F>(&mut self, f: F)
    where
        F: FnMut(&Overlay) -> bool,
    {
        self.overlays.retain(f);
    }

    pub(crate) fn is_open(&self, kind: OverlayKind) -> bool {
        self.iter().any(|o| o.kind() == kind)
    }

    pub(crate) fn remove_kind(&mut self, kind: OverlayKind) {
        self.retain(|o| o.kind() != kind);
    }

    pub(crate) fn is_top(&self, kind: OverlayKind) -> bool {
        self.top().map(|o| o.kind() == kind).unwrap_or(false)
    }

    pub(crate) fn needs_scroll(&self) -> bool {
        matches!(
            self.top(),
            Some(Overlay::ShowInfo) | Some(Overlay::KeybindHelp) | Some(Overlay::CommandOutput(_))
        )
    }

    pub(crate) fn find_command_output_mut(&mut self, id: u64) -> Option<&mut CommandOutputOverlay> {
        self.overlays.iter_mut().find_map(|o| match o {
            Overlay::CommandOutput(out) if out.id() == id => Some(out.as_mut()),
            _ => None,
        })
    }

    pub(crate) fn running_command_output_id(&self) -> Option<u64> {
        self.overlays.iter().find_map(|o| match o {
            Overlay::CommandOutput(out) if out.running() => Some(out.id()),
            _ => None,
        })
    }

    pub(crate) fn take_shared(
        &mut self,
        notification_time: &mut Option<Instant>,
    ) -> SharedOverlays {
        let active_expiry = notification_time
            .take()
            .filter(|&expiry| expiry > Instant::now());

        let overlays = self
            .overlays
            .extract_if(.., |o| o.is_tab_shared())
            .filter(|o| !matches!(o, Overlay::Message { .. }) || active_expiry.is_some())
            .collect();

        SharedOverlays {
            overlays,
            notification_time: active_expiry,
        }
    }

    pub(crate) fn restore_shared(
        &mut self,
        shared: SharedOverlays,
        notification_time: &mut Option<Instant>,
    ) {
        for overlay in shared.overlays {
            self.remove_kind(overlay.kind());
            self.push(overlay);
        }

        if let Some(expiry) = shared.notification_time {
            *notification_time = Some(expiry);
        }
    }
}

impl Default for OverlayStack {
    fn default() -> Self {
        Self::new()
    }
}
