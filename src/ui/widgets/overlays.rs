//! Overlay widgets draw functions for ui::render::render_overlays

use ratatui::{
    Frame,
    layout::Alignment,
    style::{Modifier, Style},
    text::{Line, Span, Text},
};

use crate::app::{AppState, overlay_stack::CommandOutputOverlay};
use crate::config::{
    dialog_layout::{DialogPosition, DialogSize},
    keys::InputKeys,
};
use crate::core::metadata::FileMetadataCache;
use crate::ui::widgets::{self, DialogLayout};
use crate::utils::ansi;

type HelpRow = (String, &'static str);

pub(crate) fn draw_show_info_dialog(
    frame: &mut Frame,
    app: &AppState,
    accent_style: Style,
    meta_cache: &FileMetadataCache,
) {
    let theme = app.config().theme();
    let info_cfg = &app.config().display().info();

    let label_style = theme.widget().label_style_or_theme();
    let value_style = theme.widget().value_style_or_theme();

    let position = widgets::dialog_position_unified(info_cfg.position(), app);
    let border_type = app.config().display().border_type();

    let mut lines: Vec<Line> = Vec::with_capacity(9);

    let mut add_line = |label: &str, value: &str| {
        if !value.is_empty() {
            lines.push(Line::from(vec![
                Span::styled(format!("{:<11}", label), label_style),
                Span::styled(value.to_string(), value_style),
            ]));
        }
    };

    if info_cfg.name() {
        add_line("Name:", meta_cache.name());
    }
    if info_cfg.file_type() {
        add_line("Type:", meta_cache.file_type());
    }
    if info_cfg.size() {
        add_line("Size:", meta_cache.size());
    }
    if info_cfg.modified() {
        add_line("Modified:", meta_cache.modified());
    }
    if info_cfg.created() {
        add_line("Created:", meta_cache.created());
    }
    if info_cfg.accessed() {
        add_line("Accessed:", meta_cache.accessed());
    }
    if info_cfg.perms() {
        add_line("Perms:", meta_cache.perms());
    }

    #[cfg(unix)]
    {
        if info_cfg.owner() {
            add_line("Owner:", meta_cache.owner());
        }

        if info_cfg.group() {
            add_line("Group:", meta_cache.group());
        }
    }

    if lines.is_empty() {
        return;
    }

    let max_width = lines.iter().map(|l| l.width()).max().unwrap_or(0);

    let min_width = 27;
    let border_pad = 2;
    let right_pad = 2;
    let area = frame.area();

    let raw_width = (max_width + right_pad).max(min_width) + border_pad;
    let width = raw_width.min(area.width as usize) as u16;
    let height = (lines.len() + border_pad).min(area.height as usize) as u16;

    let dialog_layout = DialogLayout {
        area,
        position,
        size: DialogSize::Custom(width, height),
    };

    widgets::draw_dialog(
        frame,
        dialog_layout,
        border_type,
        &widgets::get_dialog_style(app, accent_style, "File Info", None),
        Text::from(lines),
        Some(Alignment::Left),
        None,
    );
}

pub(crate) fn draw_prefix_help_overlay(frame: &mut Frame, app: &AppState, accent_style: Style) {
    let widget = app.config().theme().widget();
    let area = frame.area();
    let border_type = app.config().display().border_type();
    let keys = app.config().keys();
    let go_to_top_keys = keys.go_to_top();
    let go_to_home_keys = keys.go_to_home();
    let go_to_path_keys = keys.go_to_path();
    let go_to_trash_keys = keys.go_to_trash();

    let is_sort_prefix = app.actions().prefix_recognizer().is_sort_state();
    if is_sort_prefix {
        fn mk_line(items: &[(&[String], &str)], accent_style: Style) -> Line<'static> {
            let mut spans: Vec<Span<'static>> = Vec::with_capacity(items.len() * 4);
            for (i, (keys, desc)) in items.iter().enumerate() {
                if i > 0 {
                    spans.push(Span::raw("    "));
                }
                let primary_key = keys.first().map(|s| s.as_str()).unwrap_or("?");
                spans.push(Span::styled(format!("[{}]", primary_key), accent_style));
                spans.push(Span::raw(" "));
                spans.push(Span::raw((*desc).to_string()));
            }
            Line::from(spans)
        }

        let row1: [(&[String], &str); 4] = [
            (keys.sort_by_name(), "Name"),
            (keys.sort_by_modified(), "Modified"),
            (keys.sort_by_created(), "Created"),
            (keys.sort_by_accessed(), "Accessed"),
        ];
        let row2: [(&[String], &str); 3] = [
            (keys.sort_by_size(), "Size"),
            (keys.sort_by_extension(), "Ext"),
            (keys.sort_by_natural(), "Natural"),
        ];

        let lines: Vec<Line<'static>> =
            vec![mk_line(&row1, accent_style), mk_line(&row2, accent_style)];

        let area = frame.area();
        let border_pad = 2usize;
        let inner_pad = 2usize;

        let max_line_w = lines.iter().map(|l| l.width()).max().unwrap_or(0);

        let min_width = 24usize;
        let max_width = (area.width as usize).saturating_sub(2).max(min_width);

        let width = (max_line_w + inner_pad + border_pad)
            .max(min_width)
            .min(max_width) as u16;

        let height = (lines.len() + border_pad).min(area.height as usize) as u16;

        widgets::draw_dialog(
            frame,
            DialogLayout {
                area,
                position: widget.go_to_help_position(),
                size: DialogSize::Custom(width, height),
            },
            border_type,
            &widgets::get_dialog_style(app, accent_style, "Sort", None),
            Text::from(lines),
            Some(Alignment::Center),
            None,
        );
        return;
    }

    let mut go_to_prefixes: Vec<HelpRow> =
        Vec::with_capacity(go_to_top_keys.len() + go_to_path_keys.len());
    if let Some(k) = go_to_top_keys.first() {
        go_to_prefixes.push((k.clone(), "Go to top"));
    }
    if let Some(k) = go_to_home_keys.first() {
        go_to_prefixes.push((k.clone(), "Go to home"));
    }
    if let Some(k) = go_to_path_keys.first() {
        go_to_prefixes.push((k.clone(), "Go to path"));
    }
    if let Some(k) = go_to_trash_keys.first() {
        go_to_prefixes.push((k.clone(), "Go to trash"));
    }

    let mut spans = Vec::with_capacity(4 * go_to_prefixes.len() + 2);
    for (i, (ch, desc)) in go_to_prefixes.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw("    "));
        }
        spans.push(Span::styled(format!("[{}]", ch), accent_style));
        spans.push(Span::raw(" "));
        spans.push(Span::raw(*desc));
    }

    spans.push(Span::raw(" "));
    let line = Line::from(spans);

    let size = widget.go_to_help_size();
    let position = widget.go_to_help_position();

    widgets::draw_dialog(
        frame,
        DialogLayout {
            area,
            position,
            size,
        },
        border_type,
        &widgets::get_dialog_style(app, accent_style, "Go to", None),
        line,
        Some(Alignment::Center),
        None,
    );
}

/// Draws a simple message overlay dialog at the bottom right
/// Used for notifications such as "fd is not available" etc.
pub(crate) fn draw_message_overlay(
    frame: &mut Frame,
    app: &AppState,
    accent_style: Style,
    text: &str,
    alignment: Alignment,
) {
    let position = DialogPosition::BottomRight;
    let border_type = app.config().display().border_type();

    let parsed = ansi::parse_lines(text.lines().map(str::to_owned).collect());
    let max_line_width = parsed.lines.iter().map(|l| l.width()).max().unwrap_or(0);
    let line_count = parsed.lines.len();

    let min_width = 8;
    let border_pad = 2;
    let right_pad = 2;
    let area = frame.area();

    let width =
        ((max_line_width + right_pad).max(min_width) + border_pad).min(area.width as usize) as u16;
    let height = ((line_count + border_pad).min(area.height as usize)) as u16;

    let dialog_size = DialogSize::Custom(width, height);
    let mut dialog_rect = widgets::dialog_area(area, dialog_size, position);
    if dialog_rect.y + dialog_rect.height >= area.y + area.height && dialog_rect.y > area.y {
        dialog_rect.y -= 1;
    }

    let custom_layout = DialogLayout {
        area: dialog_rect,
        position: DialogPosition::BottomLeft,
        size: dialog_size,
    };

    widgets::draw_dialog(
        frame,
        custom_layout,
        border_type,
        &widgets::get_dialog_style(app, accent_style, "Message", None),
        parsed,
        Some(alignment),
        None,
    );
}

struct HelpEntry {
    key: InputKeys,
    desc: &'static str,
}

struct HelpSection {
    name: &'static str,
    entries: &'static [HelpEntry],
}

#[rustfmt::skip]
const HELP_DATA: &[HelpSection] = &[
    HelpSection {
        name: "Navigation",
        entries: &[
            HelpEntry { key: InputKeys::GoUp, desc: "Move selection up" },
            HelpEntry { key: InputKeys::GoDown, desc: "Move selection down" },
            HelpEntry { key: InputKeys::GoParent, desc: "Go to parent directory" },
            HelpEntry { key: InputKeys::GoIntoDir, desc: "Enter directory" },
            HelpEntry { key: InputKeys::ToggleMarker, desc: "Toggle marker" },
            HelpEntry { key: InputKeys::ClearMarkers, desc: "Clear markers" },
            HelpEntry { key: InputKeys::ClearFilter, desc: "Clear filter" },
            HelpEntry { key: InputKeys::ClearAll, desc: "Clear all markers and filters" },
            HelpEntry { key: InputKeys::SelectAll, desc: "Select all entries in directory" },
            HelpEntry { key: InputKeys::GoToBottom, desc: "Go to bottom" },
            HelpEntry { key: InputKeys::ScrollUp, desc: "Scroll widget up" },
            HelpEntry { key: InputKeys::ScrollDown, desc: "Scroll widget down" },
        ],
    },
    HelpSection {
        name: "Tabs",
        entries: &[
            HelpEntry { key: InputKeys::TabNew, desc: "Create a new tab" },
            HelpEntry { key: InputKeys::TabClose, desc: "Close the selected tab" },
            HelpEntry { key: InputKeys::TabNext, desc: "Switch to next tab" },
            HelpEntry { key: InputKeys::TabPrev, desc: "Switch to previous tab" },
        ],
    },
    HelpSection {
        name: "File",
        entries: &[
            HelpEntry { key: InputKeys::OpenFile, desc: "Open file in editor" },
            HelpEntry { key: InputKeys::Copy, desc: "Copy/Yank selection" },
            HelpEntry { key: InputKeys::Paste, desc: "Paste" },
            HelpEntry { key: InputKeys::Rename, desc: "Rename" },
            HelpEntry { key: InputKeys::Create, desc: "Create file" },
            HelpEntry { key: InputKeys::CreateDirectory, desc: "Create directory" },
            HelpEntry { key: InputKeys::Delete, desc: "Delete / move to trash" },
            HelpEntry { key: InputKeys::AlternateDelete, desc: "Alternate delete mode" },
            HelpEntry { key: InputKeys::Filter, desc: "Filter entries" },
            HelpEntry { key: InputKeys::Find, desc: "Find (fuzzy)" },
            HelpEntry { key: InputKeys::MoveFile, desc: "Move file(s)" },
            HelpEntry { key: InputKeys::ShowInfo, desc: "Toggle file info" },
            HelpEntry { key: InputKeys::ClearClipboard, desc: "Clear copied entries" },
        ],
    },
    HelpSection {
        name: "Go To",
        entries: &[
            HelpEntry { key: InputKeys::PrefixGoTo, desc: "Go to prefix" },
            HelpEntry { key: InputKeys::GoToTop, desc: "Go to top" },
            HelpEntry { key: InputKeys::GoToHome, desc: "Go to home" },
            HelpEntry { key: InputKeys::GoToPath, desc: "Go to path" },
            HelpEntry { key: InputKeys::GoToTrash, desc: "Go to trash" },
        ],
    },
    HelpSection {
        name: "Sort",
        entries: &[
            HelpEntry { key: InputKeys::Sort, desc: "Sort prefix" },
            HelpEntry { key: InputKeys::SortByName, desc: "Sort by name" },
            HelpEntry { key: InputKeys::SortByNatural, desc: "Sort by natural order" },
            HelpEntry { key: InputKeys::SortByExtension, desc: "Sort by extension" },
            HelpEntry { key: InputKeys::SortBySize, desc: "Sort by size" },
            HelpEntry { key: InputKeys::SortByModified, desc: "Sort by modified time" },
            HelpEntry { key: InputKeys::SortByCreated, desc: "Sort by created time" },
            HelpEntry { key: InputKeys::SortByAccessed, desc: "Sort by accessed time" },
        ],
    },
    HelpSection {
        name: "System",
        entries: &[
            HelpEntry { key: InputKeys::Quit, desc: "Quit" },
            HelpEntry { key: InputKeys::Reload, desc: "Reload the UI" },
            HelpEntry { key: InputKeys::KeybindHelp, desc: "Toggle keybind help" },
        ],
    },
];

pub(crate) fn draw_keybind_help(frame: &mut Frame, app: &AppState, accent_style: Style) {
    let keys = app.config().keys();
    let widget = app.config().theme().widget();
    let area = frame.area();

    let position = widgets::dialog_position_unified(widget.position(), app);

    let border_type = app.config().display().border_type();
    let dim_style = Style::default().add_modifier(Modifier::DIM);
    let header_style = app.config().theme().widget().label_style_or_theme();
    let key_style = app.config().theme().widget().value_style_or_theme();

    let max_keys_to_show: usize = 3;
    let key_pad: usize = 3;

    let fmt_key_token = |k: &str| -> String {
        if k == " " || k.trim().is_empty() || k.eq_ignore_ascii_case("space") {
            "Space".to_string()
        } else {
            k.to_string()
        }
    };

    let fmt_keys = |list: &[String]| -> String {
        if list.is_empty() {
            return "-".to_string();
        }
        let mut out: Vec<String> = list
            .iter()
            .take(max_keys_to_show)
            .map(|k| fmt_key_token(k))
            .collect();
        out.reverse();
        if list.len() > max_keys_to_show {
            out.push("...".to_string());
        }
        out.join(", ")
    };

    let go_to_prefix = keys.prefix_go_to();
    let sort_prefix = keys.sort();
    let go_leader = go_to_prefix.first().map(|s| s.as_str()).unwrap_or("g");
    let sort_leader = sort_prefix.first().map(|s| s.as_str()).unwrap_or("o");

    let fmt_prefix =
        |leader: &str, list: &[String]| -> String { format!("{leader} + {}", fmt_keys(list)) };

    let sections: Vec<(&str, Vec<HelpRow>)> = HELP_DATA
        .iter()
        .map(|section| {
            let mut rows: Vec<HelpRow> = section
                .entries
                .iter()
                .map(|entry| {
                    let key_text = match section.name {
                        "Go To" if entry.key != InputKeys::PrefixGoTo => {
                            fmt_prefix(go_leader, keys.get(&entry.key))
                        }
                        "Sort" if entry.key != InputKeys::Sort => {
                            fmt_prefix(sort_leader, keys.get(&entry.key))
                        }
                        _ => fmt_keys(keys.get(&entry.key)),
                    };
                    (key_text, entry.desc)
                })
                .collect();

            if section.name == "Tabs" {
                rows.push(("[0-9]".into(), "Switch to tab by index"));
            }
            (section.name, rows)
        })
        .collect();

    let key_w: usize = sections
        .iter()
        .flat_map(|(_, rows)| rows.iter().map(|(k, _)| k.len()))
        .max()
        .unwrap_or(10)
        .clamp(10, 28);

    let margin = 6;
    let margin_spacer = " ".repeat(margin);

    let mk_line = |key_text: &str, desc: &str| -> Line<'static> {
        let key_string = format!("{:>width$}", key_text, width = key_w);
        Line::from(vec![
            Span::raw(margin_spacer.to_string()),
            Span::styled(key_string, key_style),
            Span::styled(" ".repeat(key_pad), dim_style),
            Span::raw(desc.to_string()),
        ])
    };

    let mut lines: Vec<Line> = Vec::new();
    let section_title_indent = " ".repeat(key_w + key_pad);

    for (section_name, rows) in sections {
        lines.push(Line::from(vec![
            Span::raw(margin_spacer.to_string()),
            Span::raw(&section_title_indent),
            Span::styled(format!("{section_name}:"), header_style),
        ]));

        for (k, desc) in rows {
            lines.push(mk_line(&k, desc));
        }
        lines.push(Line::raw(""));
    }

    while matches!(lines.last(), Some(l) if l.width() == 0) {
        lines.pop();
    }
    lines.push(Line::raw(""));

    let content_height = lines.len() as u16;
    let dynamic_width = (area.width * 60 / 100).clamp(45, 80);
    let max_allowed_height = (area.height * 80 / 100).max(12);
    let dynamic_height = (content_height + 1).min(max_allowed_height);

    widgets::draw_dialog(
        frame,
        DialogLayout {
            area,
            position,
            size: DialogSize::Custom(dynamic_width, dynamic_height),
        },
        border_type,
        &widgets::get_dialog_style(app, accent_style, "Keybinds", None),
        Text::from(lines),
        Some(Alignment::Left),
        Some(app.actions().scroll()),
    );
}

pub(crate) fn draw_command_output(
    frame: &mut Frame,
    app: &AppState,
    accent_style: Style,
    output: &CommandOutputOverlay,
) {
    let area = frame.area();
    let position = widgets::dialog_position_unified(app.config().theme().widget().position(), app);
    let border_type = app.config().display().border_type();
    let dim_style = Style::default().add_modifier(Modifier::DIM);

    let title = if output.running() {
        format!("{} (running...)", output.title())
    } else if output.success() {
        output.title().to_string()
    } else if let Some(code) = output.exit_code() {
        format!("{} [exit {}]", output.title(), code)
    } else {
        format!("{} [failed]", output.title())
    };

    let mut text = if output.running() {
        Text::from(Line::styled("Running...", dim_style))
    } else if output.text().lines.is_empty() {
        Text::from(Line::styled("[No output]", dim_style))
    } else {
        output.text().clone()
    };

    if output.truncated() {
        text.lines.push(Line::raw(""));
        text.lines
            .push(Line::styled("[Output truncated]", dim_style));
    }

    let max_line_w = text.lines.iter().map(|l| l.width()).max().unwrap_or(0);
    let min_w = 40.min(area.width as usize);
    let max_w = ((area.width as usize) * 85 / 100).max(min_w);
    let width = (max_line_w + 4).clamp(min_w, max_w) as u16;

    let min_h = 5.min(area.height as usize);
    let max_h = ((area.height as usize) * 80 / 100).max(min_h);
    let height = (text.lines.len() + 2).clamp(min_h, max_h) as u16;

    widgets::draw_dialog(
        frame,
        DialogLayout {
            area,
            position,
            size: DialogSize::Custom(width, height),
        },
        border_type,
        &widgets::get_dialog_style(app, accent_style, &title, None),
        text,
        Some(Alignment::Left),
        Some(app.actions().scroll()),
    );
}
