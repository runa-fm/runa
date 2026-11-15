//! Widget module which holds all the input widgets draw functions for the render to use.

use ratatui::{
    Frame,
    layout::Alignment,
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::app::{
    AppState,
    actions::{ActionMode, InputMode},
};
use crate::config::dialog_layout::DialogSize;
use crate::ui::widgets::{self, DialogLayout};
use crate::utils::path;

/// Either for ConfirmDelete or for anything else that requires input.
/// For other than ConfirmDelete, calculates the exact input field.
pub(crate) fn draw_input_dialog(frame: &mut Frame, app: &AppState, accent_style: Style) {
    if let ActionMode::Input { mode, prompt } = &app.actions().mode() {
        let widget = app.config().theme().widget();
        let position = widgets::dialog_position_unified(widget.position(), app);
        let size = widget.size();
        let move_size = widget.move_size();
        let border_type = app.config().display().border_type();

        match mode {
            InputMode::ConfirmDelete { is_trash } => {
                let confirm_size = widget.confirm_size();
                let mut targets: Vec<String> = app
                    .nav()
                    .get_action_targets()
                    .into_iter()
                    .map(|p| {
                        p.file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_default()
                    })
                    .collect();
                targets.sort();

                let heading = if *is_trash {
                    "Files to move to trash"
                } else {
                    "Files to delete"
                };

                let preview = if !targets.is_empty() {
                    format!(
                        "{heading} ({}):\n{}",
                        targets.len(),
                        targets
                            .iter()
                            .map(|n| format!("  - {}", n))
                            .collect::<Vec<_>>()
                            .join("\n")
                    )
                } else {
                    String::new()
                };

                let dialog_area = widgets::dialog_area(frame.area(), confirm_size, position);
                let visible_width = dialog_area.width.saturating_sub(2) as usize;

                let mut dialog_lines = vec![
                    Line::raw(prompt),
                    Line::from(vec![Span::styled(
                        "─".repeat(visible_width),
                        widget.border_style_or(Style::default().fg(Color::Red)),
                    )]),
                ];
                if !preview.is_empty() {
                    for line in preview.lines() {
                        dialog_lines.push(Line::raw(line));
                    }
                }
                let dialog_text = Text::from(dialog_lines);

                let dialog_layout = DialogLayout {
                    area: frame.area(),
                    position,
                    size: confirm_size,
                };

                widgets::draw_dialog(
                    frame,
                    dialog_layout,
                    border_type,
                    &widgets::get_dialog_style(
                        app,
                        Style::default().fg(Color::Red),
                        "Confirm Delete",
                        Some(Style::default().fg(Color::Red)),
                    ),
                    dialog_text,
                    Some(Alignment::Left),
                    Some(app.actions().scroll()),
                );
            }

            InputMode::MoveFile => {
                let targets_set = app.nav().get_action_targets();
                let mut action_targets: Vec<_> = targets_set.iter().collect();
                action_targets.sort();

                let preview = if !action_targets.is_empty() {
                    if action_targets.len() == 1 {
                        format!(
                            "File to move: {}",
                            path::clean_display_path(&action_targets[0].to_string_lossy())
                        )
                    } else {
                        format!(
                            "Files to move ({}):\n{}",
                            action_targets.len(),
                            action_targets
                                .iter()
                                .map(|p| format!(
                                    "  - {}",
                                    path::clean_display_path(&p.to_string_lossy())
                                ))
                                .collect::<Vec<_>>()
                                .join("\n")
                        )
                    }
                } else {
                    String::new()
                };

                let dialog_layout = DialogLayout {
                    area: frame.area(),
                    position,
                    size: move_size,
                };

                let input_text = app.actions().input_buffer();
                let cursor_pos = app.actions().input_cursor_pos();
                let dialog_area = widgets::dialog_area(frame.area(), move_size, position);
                let visible_width = dialog_area.width.saturating_sub(2) as usize;

                let (display_input, cursor_offset) =
                    input_field_view(input_text, cursor_pos, visible_width);

                let want_separator = move_size != DialogSize::Small && !preview.is_empty();
                let mut dialog_lines = vec![Line::raw(display_input)];
                if want_separator {
                    dialog_lines.push(Line::from(vec![Span::styled(
                        "─".repeat(visible_width),
                        widget.border_style_or(accent_style),
                    )]));
                }
                if !preview.is_empty() {
                    for lines in preview.lines() {
                        dialog_lines.push(Line::raw(lines));
                    }
                }
                let dialog_text = Text::from(dialog_lines);

                widgets::draw_dialog(
                    frame,
                    dialog_layout,
                    border_type,
                    &widgets::get_dialog_style(app, accent_style, prompt, None),
                    dialog_text,
                    Some(Alignment::Left),
                    Some(app.actions().scroll()),
                );

                frame.set_cursor_position((
                    dialog_area.x + 1 + cursor_offset as u16,
                    dialog_area.y + 1,
                ));
            }

            InputMode::ConfirmOverwrite { is_dir, old, new } => {
                let confirm_size = widget.confirm_size();

                let target_name = new
                    .as_ref()
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();

                let preview = if let Some(src) = old {
                    let src_name = src
                        .as_ref()
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    format!(
                        "Rename will overwrite target:\n  From: {}\n  To:   {}",
                        src_name, target_name
                    )
                } else {
                    let kind = if *is_dir { "directory" } else { "file" };
                    format!("This {kind} will be overwritten:\n  {}", target_name)
                };

                let dialog_area = widgets::dialog_area(frame.area(), confirm_size, position);
                let visible_width = dialog_area.width.saturating_sub(2) as usize;

                let mut dialog_lines = vec![
                    Line::raw(prompt),
                    Line::from(vec![Span::styled(
                        "─".repeat(visible_width),
                        widget.border_style_or(Style::default().fg(Color::Yellow)),
                    )]),
                ];

                if !preview.is_empty() {
                    for line in preview.lines() {
                        dialog_lines.push(Line::raw(line));
                    }
                }

                let dialog_text = Text::from(dialog_lines);

                let dialog_layout = DialogLayout {
                    area: frame.area(),
                    position,
                    size: confirm_size,
                };

                widgets::draw_dialog(
                    frame,
                    dialog_layout,
                    border_type,
                    &widgets::get_dialog_style(
                        app,
                        Style::default().fg(Color::Yellow),
                        "Confirm Overwrite",
                        Some(Style::default().fg(Color::Yellow)),
                    ),
                    dialog_text,
                    Some(Alignment::Left),
                    Some(app.actions().scroll()),
                );
            }

            _ => {
                let dialog_layout = DialogLayout {
                    area: frame.area(),
                    position,
                    size,
                };

                let input_text = app.actions().input_buffer();
                let cursor_pos = app.actions().input_cursor_pos();
                let dialog_area = widgets::dialog_area(frame.area(), size, position);
                let visible_width = dialog_area.width.saturating_sub(2) as usize;

                let (display_input, cursor_offset) =
                    input_field_view(input_text, cursor_pos, visible_width);

                widgets::draw_dialog(
                    frame,
                    dialog_layout,
                    border_type,
                    &widgets::get_dialog_style(app, accent_style, prompt, None),
                    display_input,
                    Some(Alignment::Left),
                    None,
                );

                frame.set_cursor_position((
                    dialog_area.x + 1 + cursor_offset as u16,
                    dialog_area.y + 1,
                ));
            }
        }
    }
}

/// Draws the fuzzy find dialog widget
///
/// Draws the input field and the result field as one widget.
/// Sets a find result indicator in the input line to the right.
/// Find result indicator being on the input line makes the actual input line smaller.
pub(crate) fn draw_find_dialog(frame: &mut Frame, app: &AppState, accent_style: Style) {
    let actions = app.actions();
    let widget = app.config().theme().widget();
    let base_dir = app.nav().current_dir();
    let area = frame.area();

    let position = widgets::dialog_position_unified(widget.position(), app);
    let columns = widget.find_width().min(area.width).max(20);

    let max_visible = widget.find_visible_results();
    let rows = max_visible as u16 + 4;

    let size = DialogSize::Custom(columns, rows);
    let border_type = app.config().display().border_type();

    let input_text = actions.input_buffer();
    let cursor_pos = actions.input_cursor_pos();
    let results = actions.find().results();
    let selected = actions.find().selected();
    let area = frame.area();
    let dialog_rect = widgets::dialog_area(area, size, position);

    let total = results.len();
    let selected = selected.min(total.saturating_sub(1));
    let mut scroll = 0;

    if selected < scroll {
        scroll = selected;
    } else if selected >= scroll + max_visible {
        scroll = selected + 1 - max_visible;
    }

    let mut display_lines = Vec::with_capacity(max_visible + 2);

    let indicator = format!(
        "[{} / {}]",
        if total == 0 { 0 } else { selected + 1 },
        total
    );
    let field_width = dialog_rect.width.saturating_sub(2) as usize;
    let indicator_width = indicator.width() + 2;
    let max_input_width = field_width.saturating_sub(indicator_width);

    let (display_input, cursor_x) = input_field_view(input_text, cursor_pos, max_input_width);

    let pad_width = max_input_width.saturating_sub(display_input.width());
    let mut line_input = vec![Span::styled(
        display_input,
        Style::default().add_modifier(Modifier::BOLD),
    )];
    if pad_width > 0 {
        line_input.push(Span::raw(" ".repeat(pad_width)));
    }
    line_input.push(Span::raw("  "));
    line_input.push(Span::styled(
        indicator,
        Style::default().fg(Color::DarkGray),
    ));
    display_lines.push(Line::from(line_input));

    let horizontal_line = Span::styled(
        "─".repeat(field_width),
        widget.border_style_or(accent_style),
    );
    display_lines.push(Line::from(horizontal_line));

    if results.is_empty() {
        display_lines.push(Line::from(Span::styled(
            " No matches",
            Style::default().fg(Color::DarkGray),
        )));
        for _ in 1..max_visible {
            display_lines.push(Line::from(""));
        }
    } else {
        for (idx, r) in results.iter().enumerate().skip(scroll).take(max_visible) {
            let marker = if idx == selected { "› " } else { "  " };
            let marker_style = if idx == selected {
                accent_style
            } else {
                Style::default()
            };
            display_lines.push(Line::from(vec![
                Span::styled(marker, marker_style),
                Span::raw(r.relative(base_dir)),
            ]));
        }
        let lines_drawn = results
            .iter()
            .enumerate()
            .skip(scroll)
            .take(max_visible)
            .count();
        for _ in lines_drawn..max_visible {
            display_lines.push(Line::from(""));
        }
    }

    widgets::draw_dialog(
        frame,
        DialogLayout {
            area,
            position,
            size,
        },
        border_type,
        &widgets::get_dialog_style(app, accent_style, "Find", None),
        display_lines,
        Some(Alignment::Left),
        None,
    );
    frame.set_cursor_position((dialog_rect.x + 1 + cursor_x as u16, dialog_rect.y + 1));
}

/// Helper function to calculate cursor offset for cursor moving
/// Handles horizontal truncation, variable width with unicode_width and clamps cursor to buffer.
/// Is used for draw widgets/dialogs with input fields.
fn input_field_view(input_text: &str, cursor_pos: usize, visible_width: usize) -> (&str, usize) {
    let cursor_pos = cursor_pos.min(input_text.len());
    let input_width = input_text.width();
    if input_width <= visible_width {
        let cursor_offset =
            unicode_width::UnicodeWidthStr::width(&input_text[..cursor_pos.min(input_text.len())]);
        (input_text, cursor_offset)
    } else {
        let mut current_w = 0;
        let mut start = input_text.len();
        for (idx, ch) in input_text.char_indices().rev() {
            current_w += ch.width().unwrap_or(0);
            if current_w > visible_width {
                start = idx + ch.len_utf8();
                break;
            }
        }

        let cursor_offset = if cursor_pos < start {
            0
        } else {
            unicode_width::UnicodeWidthStr::width(
                &input_text[start..cursor_pos.min(input_text.len())],
            )
        };

        (&input_text[start..], cursor_offset)
    }
}
