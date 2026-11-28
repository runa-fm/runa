//! Ovelay module to seamless stack widgets, dialogs with each other.
//! Currently handles ShowInfo as a overlay.
//!
//! Can be expanded to hanlde more widget types for more functions.
//!
//! Is used throughout the ui modules and in AppState handlers.

use std::{slice, time::Instant};

use ratatui::layout::Alignment;

pub(crate) enum Overlay {
    ShowInfo,
    Message { text: String, alignment: Alignment },
    PrefixHelp,
    KeybindHelp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OverlayKind {
    ShowInfo,
    Message,
    PrefixHelp,
    KeybindHelp,
}

pub(crate) struct OverlayStack {
    overlays: Vec<Overlay>,
}

pub(crate) struct SharedOverlays {
    overlays: Vec<Overlay>,
    notification_time: Option<Instant>,
}

impl Overlay {
    pub(crate) const fn kind(&self) -> OverlayKind {
        match self {
            Overlay::ShowInfo => OverlayKind::ShowInfo,
            Overlay::Message { .. } => OverlayKind::Message,
            Overlay::PrefixHelp => OverlayKind::PrefixHelp,
            Overlay::KeybindHelp => OverlayKind::KeybindHelp,
        }
    }

    pub(crate) fn is_tab_shared(&self) -> bool {
        match self {
            Overlay::ShowInfo | Overlay::Message { .. } => true,
            Overlay::PrefixHelp | Overlay::KeybindHelp => false,
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
            Some(Overlay::ShowInfo) | Some(Overlay::KeybindHelp)
        )
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
