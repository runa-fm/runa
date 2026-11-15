//! runa TUI dialog widget module.
//!
//! This module mostly holds dialog widget logic to help draw functions with positioning, size,
//! area and style.

use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Color, Style},
    text::{Span, Text},
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
};

use crate::app::AppState;
use crate::app::actions::ScrollState;
use crate::config::dialog_layout::{DialogPosition, DialogSize};

/// Struct to hold the dialog style.
///
/// Includes the dialog border, border_style, the background/foreground and the title.
pub(crate) struct DialogStyle {
    pub(crate) border: Borders,
    pub(crate) border_style: Style,
    pub(crate) bg: Style,
    pub(crate) fg: Style,
    pub(crate) title: Option<Span<'static>>,
    pub(crate) title_alignment: Option<Alignment>,
}

impl Default for DialogStyle {
    fn default() -> Self {
        Self {
            border: Borders::ALL,
            border_style: Style::default().fg(Color::White),
            bg: Style::default().bg(Color::Black),
            fg: Style::default().fg(Color::Reset),
            title: None,
            title_alignment: Some(Alignment::Left),
        }
    }
}

/// Struct to hold the overall layout of a dialog widget
pub(crate) struct DialogLayout {
    pub(crate) area: Rect,
    pub(crate) position: DialogPosition,
    pub(crate) size: DialogSize,
}

/// Function to correctly calculate the area of the dialog
///
/// Returns the Rect of the calculated are of the dialog
pub(crate) fn dialog_area(area: Rect, size: DialogSize, pos: DialogPosition) -> Rect {
    let min_w = 7;
    let min_h = 3;

    let (w, h) = match size {
        DialogSize::Small => (
            (area.width * 24 / 100).max(min_w).min(area.width),
            (area.height * 7 / 100).max(min_h).min(area.height),
        ),
        DialogSize::Medium => (
            (area.width * 26 / 100).max(min_w).min(area.width),
            (area.height * 14 / 100).max(min_h).min(area.height),
        ),
        DialogSize::Large => (
            (area.width * 32 / 100).max(min_w).min(area.width),
            (area.height * 40 / 100).max(min_h).min(area.height),
        ),
        DialogSize::Custom(w_cells, h_cells) => (
            w_cells.max(min_w).min(area.width),
            h_cells.max(min_h).min(area.height),
        ),
    };

    match pos {
        DialogPosition::Center => Rect {
            x: area.x + (area.width - w) / 2,
            y: area.y + (area.height - h) / 2,
            width: w,
            height: h,
        },
        DialogPosition::Top => Rect {
            x: area.x + (area.width - w) / 2,
            y: area.y,
            width: w,
            height: h,
        },
        DialogPosition::Bottom => Rect {
            x: area.x + (area.width - w) / 2,
            y: area.y + area.height - h,
            width: w,
            height: h,
        },
        DialogPosition::Left => Rect {
            x: area.x,
            y: area.y + (area.height - h) / 2,
            width: w,
            height: h,
        },
        DialogPosition::Right => Rect {
            x: area.x + area.width - w,
            y: area.y + (area.height - h) / 2,
            width: w,
            height: h,
        },
        DialogPosition::TopLeft => Rect {
            x: area.x,
            y: area.y,
            width: w,
            height: h,
        },
        DialogPosition::TopRight => Rect {
            x: area.x + area.width - w,
            y: area.y,
            width: w,
            height: h,
        },
        DialogPosition::BottomLeft => Rect {
            x: area.x,
            y: area.y + area.height - h,
            width: w,
            height: h,
        },
        DialogPosition::BottomRight => Rect {
            x: area.x + area.width - w,
            y: area.y + area.height - h,
            width: w,
            height: h,
        },
        DialogPosition::Custom(xp, yp) => {
            let x = area.x + ((area.width - w) * xp / 100).min(area.width - w);
            let y = area.y + ((area.height - h) * yp / 100).min(area.height - h);
            Rect {
                x,
                y,
                width: w,
                height: h,
            }
        }
    }
}

/// Draws the dialog widgets
/// Takes the frame area as a rect, sets the position of the dialog and the overall style.
pub(crate) fn draw_dialog<'a, T>(
    frame: &mut Frame,
    layout: DialogLayout,
    border: BorderType,
    style: &DialogStyle,
    content: T,
    alignment: Option<Alignment>,
    scroll_state: Option<&ScrollState>,
) where
    T: Into<Text<'a>>,
{
    let dialog = dialog_area(layout.area, layout.size, layout.position);
    let text = content.into();
    let inner_height = dialog.height.saturating_sub(2);

    let current_offset = if let Some(state) = scroll_state {
        let max = text.lines.len().saturating_sub(usize::from(inner_height));
        state.set_max_offset(max);
        u16::try_from(state.offset()).unwrap_or(u16::MAX)
    } else {
        0
    };

    frame.render_widget(Clear, dialog);

    let mut block = Block::default()
        .borders(style.border)
        .border_style(style.border_style)
        .border_type(border)
        .style(style.bg);

    if let Some(title) = &style.title {
        block = block.title(title.clone());
        if let Some(align) = style.title_alignment {
            block = block.title_alignment(align);
        }
    }

    let para = Paragraph::new(text)
        .block(block)
        .alignment(alignment.unwrap_or(Alignment::Left))
        .style(style.fg)
        .scroll((current_offset, 0));

    frame.render_widget(para, dialog);
}

/// Returns the borders of a pane. Only the split border layout draws them.
pub(crate) fn pane_borders(app: &AppState) -> Borders {
    if app.config().display().is_split() {
        Borders::ALL
    } else {
        Borders::NONE
    }
}

/// Getter for the overall pane block,
pub(crate) fn get_pane_block(title: &str, app: &AppState) -> Block<'static> {
    let mut block = Block::default().borders(pane_borders(app));
    if app.config().display().is_split() {
        block = block.border_style(app.config().theme().accent_style());
        if app.config().display().titles() {
            block = block.title(title.to_string());
        }
    }
    block
}
