//! Widget module which holds status relevant draw functions for renderer.

use std::time::Duration;

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
};

use crate::config::display::{StatusSegment, StatusTag};
use crate::core::workers::FileOpCounts;
use crate::ui::widgets::StatusPosition;
use crate::{
    app::{AppState, Clipboard},
    utils::Timings,
};

pub(crate) fn draw_separator(frame: &mut Frame, area: Rect, style: Style, border_type: BorderType) {
    frame.render_widget(
        Block::default()
            .borders(Borders::LEFT)
            .border_style(style)
            .border_type(border_type),
        area,
    );
}

pub(crate) fn draw_status_bar(
    frame: &mut Frame,
    app: &AppState,
    position: StatusPosition,
    fileops: FileOpCounts,
    clipboard: &Clipboard,
) {
    if position == StatusPosition::None {
        return;
    }

    let area = frame.area();
    let display_cfg = app.config().display();
    let status_cfg = display_cfg.status();
    let theme = app.config().theme();
    let base_style = theme.status_line_style();
    let status_bg = base_style.bg;
    let marker_theme = theme.marker();
    let use_icons = display_cfg.icons();

    let patch_style = |s: Style| match (s.bg, status_bg) {
        (None, Some(bg)) => s.bg(bg),
        _ => s,
    };

    let mut left_spans = Vec::with_capacity(12);

    if position == StatusPosition::Footer
        && display_cfg.info().status_bar()
        && let Some(file_meta) = app.selected_metadata()
    {
        let info_theme = theme.info();
        let segments = display_cfg.info().segments();

        for segment in segments {
            match segment {
                StatusSegment::Literal(lit) => {
                    left_spans.push(Span::styled(lit, patch_style(base_style)));
                }
                StatusSegment::Tag(tag) => match tag {
                    StatusTag::Perms => {
                        left_spans.push(Span::styled(
                            file_meta.perms(),
                            patch_style(info_theme.perms_style()),
                        ));
                    }
                    StatusTag::Size => {
                        let padded_size = format!("{:>8}", file_meta.size());
                        left_spans.push(Span::styled(
                            padded_size,
                            patch_style(info_theme.size_style()),
                        ));
                    }
                    StatusTag::Mtime => {
                        let modified = file_meta.modified();
                        left_spans.push(Span::styled(
                            modified,
                            patch_style(info_theme.modified_style()),
                        ));
                    }
                    StatusTag::Btime => {
                        let created = file_meta.created();
                        left_spans.push(Span::styled(
                            created,
                            patch_style(info_theme.created_style()),
                        ));
                    }
                    StatusTag::Atime => {
                        let accessed = file_meta.accessed();
                        left_spans.push(Span::styled(
                            accessed,
                            patch_style(info_theme.accessed_style()),
                        ));
                    }
                    StatusTag::Type => {
                        left_spans.push(Span::styled(
                            file_meta.file_type(),
                            patch_style(info_theme.file_type_style()),
                        ));
                    }
                    #[cfg(unix)]
                    StatusTag::Owner => {
                        let owner = file_meta.owner();
                        left_spans.push(Span::styled(owner, patch_style(info_theme.owner_style())));
                    }

                    #[cfg(unix)]
                    StatusTag::Group => {
                        let group = file_meta.group();
                        left_spans.push(Span::styled(group, patch_style(info_theme.group_style())));
                    }
                },
            }
        }
    }

    let mut spans = Vec::with_capacity(21);
    let separator = Span::styled(" | ", base_style);
    let add_sep = |s: &mut Vec<Span>| {
        if !s.is_empty() {
            s.push(separator.clone());
        }
    };

    if status_cfg.tasks() == position {
        let queued_ops = fileops.queued();
        let active_ops = fileops.active();
        let total_ops = queued_ops + active_ops;

        if total_ops > 0
            && let Some(start) = app.worker_time()
            && start.elapsed() >= Duration::from_millis(Timings::WORKER_INDICATOR_MS)
        {
            if active_ops > 0 {
                let symbols: &[&str] = if use_icons {
                    &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]
                } else {
                    &["-", "\\", "|", "/"]
                };

                let tick = start.elapsed().as_millis() / 100;
                let spinner = symbols[tick as usize % symbols.len()];

                spans.push(Span::styled("Tasks: ", base_style));
                spans.push(Span::styled(spinner, base_style));
                spans.push(Span::styled(" FileOp(", base_style));
                spans.push(Span::styled(total_ops.to_string(), base_style));
                spans.push(Span::styled(")", base_style));
            } else {
                spans.push(Span::styled("Tasks: FileOp(", base_style));
                spans.push(Span::styled(total_ops.to_string(), base_style));
                spans.push(Span::styled(")", base_style));
            }
        }
    }

    if status_cfg.clipboard() == position
        && let Some(clipboard_set) = clipboard.entries()
    {
        let count = clipboard_set.len();
        if count > 0 {
            add_sep(&mut spans);
            let icon = if use_icons { "󰆏 " } else { "" };
            let style = patch_style(marker_theme.clipboard_style_or_theme());
            spans.push(Span::styled(icon, style));
            spans.push(Span::styled(count.to_string(), style));
            spans.push(Span::styled(" copied", style));
        }
    }

    if status_cfg.markers() == position {
        let markers = app.nav().markers();
        let marker_count = markers.len();
        if marker_count > 0 {
            let is_redundant = if let Some(clipboard_set) = clipboard.entries() {
                if clipboard_set.len() != marker_count {
                    false
                } else {
                    markers.iter().all(|path| clipboard_set.contains(path))
                }
            } else {
                false
            };

            if !is_redundant {
                add_sep(&mut spans);
                let style = patch_style(marker_theme.style_or_theme());
                spans.push(Span::styled(marker_count.to_string(), style));
                spans.push(Span::styled(" marked", style));
            }
        }
    }

    if status_cfg.filter() == position {
        let filter = app.nav().active_filter();
        if !filter.is_empty() {
            add_sep(&mut spans);
            spans.push(Span::styled(format!("Filter: \"{}\"", filter), base_style));
        }
    }

    if status_cfg.tabs() == position && !app.tab_line().is_empty() {
        add_sep(&mut spans);
        spans.extend(app.tab_line().iter().cloned());
    }

    if status_cfg.entry_count() == position {
        let total = app.nav().shown_entries_len();
        add_sep(&mut spans);
        let count_str = if total == 0 {
            "0/0".to_string()
        } else {
            let idx_text = app
                .visible_selected()
                .map(|idx| (idx + 1).to_string())
                .unwrap_or_else(|| "0".to_string());
            format!("{}/{}", idx_text, total)
        };
        spans.push(Span::styled(count_str, base_style));
    }

    if left_spans.is_empty() && spans.is_empty() {
        return;
    }

    let y = match position {
        StatusPosition::Header => area.y,
        StatusPosition::Footer => area.y + area.height - 1,
        _ => return,
    };

    let rect = Rect::new(area.x, y, area.width, 1);
    let block =
        ratatui::widgets::Block::default().padding(ratatui::widgets::Padding::horizontal(1));

    if position == StatusPosition::Footer && !left_spans.is_empty() {
        let inner = block.inner(rect);

        let chunks = Layout::horizontal([
            Constraint::Length(left_spans.iter().map(|s| s.width() as u16).sum()),
            Constraint::Min(0),
            Constraint::Length(spans.iter().map(|s| s.width() as u16).sum()),
        ])
        .split(inner);

        frame.render_widget(Paragraph::new(Line::from(left_spans)), chunks[0]);

        frame.render_widget(
            Paragraph::new(Line::from(spans)).alignment(Alignment::Right),
            chunks[2],
        );

        frame.render_widget(block, rect);
    } else {
        frame.render_widget(
            Paragraph::new(Line::from(spans))
                .alignment(Alignment::Right)
                .block(block),
            rect,
        );
    }
}
