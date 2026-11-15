//! runa TUI widget module
//!
//! Provides reusable UI components for widgets, panes, separator lines, and status lines,
//! as well as helpers to correctly render and position the input fields of these widgets.
//!
//! Module contains:
//! - Rendering of input dialogs/widgets and confirm dialogs.
//! - General pane blocks, separators and the status line.
//! - Configurable dialog/widget style, position and style

mod dialog;
mod input;
mod overlays;
mod status;

pub(crate) use crate::config::display::StatusPosition;
pub(crate) use dialog::{
    DialogLayout, DialogStyle, dialog_area, draw_dialog, get_pane_block, pane_borders,
};
pub(super) use input::*;
pub(super) use overlays::*;
pub(super) use status::*;

use ratatui::{style::Style, text::Span, widgets::Borders};

use crate::app::AppState;
use crate::config::dialog_layout::DialogPosition;

pub(super) fn get_dialog_style(
    app: &AppState,
    accent: Style,
    title: &str,
    title_style_override: Option<Style>,
) -> DialogStyle {
    let widget_cfg = app.config().theme().widget();
    let title_style = title_style_override.unwrap_or_else(|| widget_cfg.title_style_or_theme());
    DialogStyle {
        border: Borders::ALL,
        border_style: widget_cfg.border_style_or(accent),
        bg: widget_cfg.bg_or_theme(),
        fg: widget_cfg.fg_or_theme(),
        title: Some(Span::styled(format!(" {title} "), title_style)),
        ..Default::default()
    }
}

pub(super) fn adjusted_dialog_position(pos: DialogPosition, is_unified: bool) -> DialogPosition {
    match (is_unified, pos) {
        (true, DialogPosition::TopRight) => DialogPosition::Custom(100, 3),
        (true, DialogPosition::TopLeft) => DialogPosition::Custom(0, 3),
        (true, DialogPosition::Custom(x, 0)) => DialogPosition::Custom(x, 3),
        _ => pos,
    }
}

pub(super) fn dialog_position_unified(position: DialogPosition, app: &AppState) -> DialogPosition {
    adjusted_dialog_position(position, app.config().display().is_unified())
}
