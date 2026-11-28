//! Application module.

pub(crate) mod actions;
mod clipboard;
mod container;
mod file_info;
mod keymap;
mod nav;
pub(crate) mod overlay_stack;
mod parent_pane;
mod preview_pane;
mod state;
pub(crate) mod tab;
mod trash;

pub(crate) use clipboard::Clipboard;
pub(crate) use container::AppContainer;
pub(crate) use nav::{ListNav, NavState};
pub(crate) use parent_pane::ParentState;
pub(crate) use preview_pane::{PreviewData, PreviewState};
pub(crate) use state::{AppState, KeypressResult, LayoutMetrics};
pub(crate) use tab::MAX_TABS;
pub(crate) use trash::TrashState;
