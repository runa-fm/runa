//! UI renderer implementation.
//!
//! Contains the top-level `render` entry point used by the runtime and the
//! layout helpers that split the screen into parent/main/preview chunks.
//!
//! This module should stay mostly “pure rendering”: it reads state + config and
//! produces widgets, without owning runa core logic.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
    symbols::line,
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
};

use crate::app::{
    AppState, Clipboard, LayoutMetrics,
    actions::{ActionMode, InputMode},
    overlays::Overlay,
};
use crate::core::workers::FileOpCounts;
use crate::ui::{
    panes::{self, PaneContext, PaneStyles},
    widgets,
};

pub(crate) struct FrameLayout {
    header: Rect,
    chunks: Vec<Rect>,
    metrics: LayoutMetrics,
}

impl FrameLayout {
    crate::getters! {
        metrics: LayoutMetrics,
    }
}

pub(crate) fn frame_layout(area: Rect, app: &AppState) -> FrameLayout {
    let (header, root_area) = header_and_root(area, app);
    let chunks = layout_chunks(root_area, app);
    let metrics = calculate_layout_metrics(area, chunks.last().copied(), app);
    FrameLayout {
        header,
        chunks,
        metrics,
    }
}

/// Render function which renders the entire terminal UI for runa on each frame.
/// Handles layout, pane rendering, borders, headers and coordinates all widgets.
pub(crate) fn render(
    frame: &mut Frame,
    app: &AppState,
    layout: &FrameLayout,
    clipboard: &Clipboard,
    fileops: FileOpCounts,
) {
    let outer_area = frame.area();
    render_header(frame, app, layout.header);
    let chunks = &layout.chunks;

    let cfg = app.config();
    let display_cfg = cfg.display();
    let theme_cfg = cfg.theme();

    let accent_style = theme_cfg.accent_style();

    let border_type = display_cfg.border_type();

    let markers = app.nav().markers();
    let marker_theme = theme_cfg.marker();
    let marker_icon = marker_theme.icon();
    let marker_style = marker_theme.style_or_theme();

    let clipboard_style = marker_theme.clipboard_style_or_theme();

    // Render Panes
    let mut pane_idx = 0;
    let show_separators = display_cfg.separators() && !display_cfg.is_split();

    // PARENT PANE
    if display_cfg.parent() && pane_idx < chunks.len() {
        let parent_dir = app
            .parent()
            .last_path()
            .or_else(|| app.nav().current_dir().parent());
        let parent_markers = panes::make_pane_markers(
            markers,
            clipboard.entries(),
            parent_dir,
            marker_icon,
            marker_style,
            clipboard_style,
        );

        fill_pane(frame, chunks[pane_idx], theme_cfg.parent().bg_style());

        let parent_pane_style = PaneStyles::new(theme_cfg)
            .with_entry(theme_cfg.parent().entry_style())
            .with_selection(theme_cfg.parent_selection_style());

        panes::draw_parent(
            frame,
            app,
            PaneContext::new(
                chunks[pane_idx],
                widgets::get_pane_block("Parent", app),
                border_type,
                theme_cfg,
                display_cfg,
                parent_pane_style,
                "",
            ),
            &parent_markers,
        );
        pane_idx += 1;
        if show_separators && pane_idx < chunks.len() {
            render_separator(
                frame,
                chunks[pane_idx].x,
                outer_area,
                theme_cfg.separator_style(),
                border_type,
            );
            pane_idx += 1;
        }
    }

    // MAIN PANE
    if pane_idx < chunks.len() {
        let symbol = if display_cfg.selection_marker() {
            theme_cfg.selection_icon()
        } else {
            ""
        };

        fill_pane(frame, chunks[pane_idx], theme_cfg.bg_style());

        panes::draw_main(
            frame,
            app,
            PaneContext::new(
                chunks[pane_idx],
                widgets::get_pane_block("Files", app),
                border_type,
                theme_cfg,
                display_cfg,
                PaneStyles::new(theme_cfg),
                symbol,
            ),
            clipboard,
        );
        pane_idx += 1;
        if show_separators && display_cfg.preview() && pane_idx < chunks.len() {
            render_separator(
                frame,
                chunks[pane_idx].x,
                outer_area,
                theme_cfg.separator_style(),
                border_type,
            );
            pane_idx += 1;
        }
    }

    // PREVIEW PANE
    if display_cfg.preview() && pane_idx < chunks.len() {
        fill_pane(frame, chunks[pane_idx], theme_cfg.preview().bg_style());

        let preview_dir = app.preview().current_path();
        let preview_markers = panes::make_pane_markers(
            markers,
            clipboard.entries(),
            preview_dir,
            marker_icon,
            marker_style,
            clipboard_style,
        );

        let preview_pane_style = PaneStyles::new(theme_cfg)
            .with_entry(theme_cfg.preview().entry_style())
            .with_selection(theme_cfg.preview_selection_style());

        panes::draw_preview(
            frame,
            app,
            PaneContext::new(
                chunks[pane_idx],
                widgets::get_pane_block("Preview", app),
                border_type,
                theme_cfg,
                display_cfg,
                preview_pane_style,
                "",
            ),
            &preview_markers,
        );
    }

    widgets::draw_status_bar(
        frame,
        app,
        widgets::StatusPosition::Header,
        fileops,
        clipboard,
    );
    widgets::draw_status_bar(
        frame,
        app,
        widgets::StatusPosition::Footer,
        fileops,
        clipboard,
    );
    render_overlays(frame, app, accent_style);
}

/// Returns the rectangular areas for all active panes, given the current configuration
///
/// The result is used for positioning file navigation, parent and preview panes in the layout.
/// Handles separators and dynamic ratios.
pub(crate) fn layout_chunks(size: Rect, app: &AppState) -> Vec<Rect> {
    let cfg = app.config().display();
    let mut constraints = Vec::new();
    let show_sep = cfg.separators() && !cfg.is_split();

    let parent = if cfg.parent() {
        cfg.parent_ratio() as u32
    } else {
        0
    };
    let main = cfg.main_ratio() as u32;
    let preview = if cfg.preview() {
        cfg.preview_ratio() as u32
    } else {
        0
    };

    let enabled = [
        (parent, cfg.parent()),
        (main, true),
        (preview, cfg.preview()),
    ];

    let total: u32 = enabled
        .iter()
        .filter(|e| e.1)
        .map(|e| e.0)
        .sum::<u32>()
        .max(1);

    let mut sum_pct: u16 = 0;
    let pane_count = enabled.iter().filter(|e| e.1).count();
    let mut pane_added = 0;

    for &(val, enabled) in &enabled {
        if enabled {
            pane_added += 1;
            let pct = if pane_added == pane_count {
                100 - sum_pct
            } else {
                let pct = ((val as f32 / total as f32) * 100.0).round() as u16;
                sum_pct += pct;
                pct
            };
            constraints.push(Constraint::Percentage(pct));
            if show_sep && pane_added < pane_count {
                constraints.push(Constraint::Length(1));
            }
        }
    }

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints(constraints)
        .split(size)
        .to_vec()
}

fn header_and_root(area: Rect, app: &AppState) -> (Rect, Rect) {
    let display_cfg = app.config().display();

    if display_cfg.is_unified() {
        (area, Block::default().borders(Borders::ALL).inner(area))
    } else {
        let header_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(0)])
            .split(area);

        let mut content = header_layout[1];
        if display_cfg.is_no_borders() {
            content.height = content.height.saturating_sub(1);
        }
        (header_layout[0], content)
    }
}

/// Renders the root block and header (if applicable) around the main content area.
fn render_header(frame: &mut Frame, app: &AppState, area: Rect) {
    let cfg = app.config();
    let display_cfg = cfg.display();
    let theme_cfg = cfg.theme();
    let path_str = app.nav().display_path();
    let border_type = display_cfg.border_type();

    if display_cfg.is_unified() {
        let mut outer_block = Block::default()
            .borders(Borders::ALL)
            .border_style(theme_cfg.accent_style())
            .border_type(border_type);

        if display_cfg.titles() {
            outer_block = outer_block.title(Line::from(vec![
                Span::raw(" "),
                Span::styled(path_str, theme_cfg.path_style()),
                Span::raw(" "),
            ]));
        }

        frame.render_widget(outer_block, area);
    } else {
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(path_str, theme_cfg.path_style()),
                Span::raw(" "),
            ])),
            area,
        );
    }
}

/// Renders a vertical separator line at the specified x-coordinate within the root area.
fn render_separator(
    frame: &mut Frame,
    x: u16,
    outer_area: Rect,
    style: Style,
    border_type: BorderType,
) {
    widgets::draw_separator(
        frame,
        Rect {
            x,
            y: outer_area.y + 1,
            width: 1,
            height: outer_area.height.saturating_sub(2),
        },
        style,
        border_type,
    );

    border_junctions(frame, outer_area, x, border_type, style);
}

/// Renders any active overlays such as input dialogs or message boxes.
///
/// Calls appropriate widget drawing functions based on the current overlays.
fn render_overlays(frame: &mut Frame, app: &AppState, accent_style: Style) {
    render_input_overlay(frame, app, accent_style);

    for overlay in app.overlays().iter() {
        render_single_overlay(frame, app, accent_style, overlay);
    }
}

fn render_input_overlay(frame: &mut Frame, app: &AppState, accent_style: Style) {
    if let ActionMode::Input { mode, .. } = app.actions().mode() {
        if *mode != InputMode::Find {
            widgets::draw_input_dialog(frame, app, accent_style);
        } else {
            widgets::draw_find_dialog(frame, app, accent_style);
        }
    }
}

fn render_single_overlay(
    frame: &mut Frame,
    app: &AppState,
    accent_style: Style,
    overlay: &Overlay,
) {
    match overlay {
        Overlay::ShowInfo { info } => {
            widgets::draw_show_info_dialog(frame, app, accent_style, info);
        }
        Overlay::Message { text, alignment } => {
            widgets::draw_message_overlay(frame, app, accent_style, text, *alignment);
        }
        Overlay::PrefixHelp => {
            widgets::draw_prefix_help_overlay(frame, app, accent_style);
        }
        Overlay::KeybindHelp => {
            widgets::draw_keybind_help(frame, app, accent_style);
        }
    }
}

/// Helper function to calculate and return layout metrics
///
/// Used to store pane widths and heights in the AppState for later use.
/// Preview height comes from the drawn pane, so it matches the rows.
fn calculate_layout_metrics(
    area: Rect,
    preview_area: Option<Rect>,
    app: &AppState,
) -> LayoutMetrics {
    let chunks = layout_chunks(area, app);
    let mut metrics = LayoutMetrics::default();
    let display_cfg = app.config().display();

    let mut idx = 0;
    let has_sep = display_cfg.separators() && !display_cfg.is_split();

    let get_width = |rect: Rect| {
        let width = if display_cfg.is_split() || display_cfg.is_unified() {
            rect.width.saturating_sub(2)
        } else {
            rect.width
        };

        width as usize
    };

    if display_cfg.parent() && idx < chunks.len() {
        metrics.parent_width = get_width(chunks[idx]);
        idx += if has_sep { 2 } else { 1 };
    }

    if idx < chunks.len() {
        metrics.main_width = get_width(chunks[idx]);
        idx += if has_sep && display_cfg.preview() {
            2
        } else {
            1
        };
    }

    if display_cfg.preview() && idx < chunks.len() {
        metrics.preview_width = get_width(chunks[idx]);
    }

    if display_cfg.preview()
        && let Some(preview_area) = preview_area
    {
        let block = Block::default().borders(widgets::pane_borders(app));
        metrics.preview_height = block.inner(preview_area).height as usize;
    }
    metrics
}

fn border_junctions(frame: &mut Frame, area: Rect, x: u16, border_type: BorderType, style: Style) {
    let (top_sym, bottom_sym) = match border_type {
        BorderType::Double => (line::DOUBLE_HORIZONTAL_DOWN, line::DOUBLE_HORIZONTAL_UP),
        BorderType::Thick => (line::THICK_HORIZONTAL_DOWN, line::THICK_HORIZONTAL_UP),
        _ => (line::HORIZONTAL_DOWN, line::HORIZONTAL_UP),
    };

    let horizontal_line = match border_type {
        BorderType::Double => line::DOUBLE_HORIZONTAL,
        BorderType::Thick => line::THICK_HORIZONTAL,
        _ => line::HORIZONTAL,
    };

    let buffer = frame.buffer_mut();

    if let Some(cell) = buffer.cell_mut((x, area.top()))
        && cell.symbol() == horizontal_line
    {
        cell.set_symbol(top_sym).set_style(style);
    }

    if let Some(cell) = buffer.cell_mut((x, area.bottom().saturating_sub(1)))
        && cell.symbol() == horizontal_line
    {
        cell.set_symbol(bottom_sym).set_style(style);
    }
}

fn fill_pane(frame: &mut Frame, area: Rect, style: Style) {
    if style != Style::default() {
        frame.render_widget(Block::default().style(style), area);
    }
}

/// render integration tests
#[cfg(test)]
mod tests {
    use super::*;

    use crate::Config;
    use crate::config::load::RawConfig;
    use ratatui::{Terminal, backend::TestBackend};
    use std::error;
    use std::sync::Arc;

    #[test]
    fn layout_chunks_with_config() -> Result<(), Box<dyn error::Error>> {
        let size = Rect::new(0, 0, 100, 10);

        let toml_content = r#"
            [display]
            parent = true
            preview = true
            separators = false

            [display.layout]
            parent = 50
            main = 50
            preview = 50
        "#;

        let raw: RawConfig = toml::from_str(toml_content)?;
        let config = Config::from(raw);

        let app = AppState::new(Arc::new(config)).expect("Failed to create AppState");

        let chunks = layout_chunks(size, &app);

        assert_eq!(chunks.len(), 3);
        let total_width: u16 = chunks.iter().map(|c| c.width).sum();

        assert!(total_width <= 100);
        assert!(chunks[0].width >= 33 && chunks[0].width <= 34);
        Ok(())
    }

    #[test]
    fn preview_height_matches_drawn_rows() -> Result<(), Box<dyn error::Error>> {
        let mut terminal = Terminal::new(TestBackend::new(120, 40))?;

        for (borders, rows) in [("none", 38), ("unified", 38), ("split", 37)] {
            let toml_content = format!("[display]\nborders = \"{borders}\"");
            let raw: RawConfig = toml::from_str(&toml_content)?;
            let app = AppState::new(Arc::new(Config::from(raw)))?;

            let mut metrics = LayoutMetrics::default();
            terminal.draw(|frame| {
                metrics = frame_layout(frame.area(), &app).metrics();
            })?;

            assert_eq!(metrics.preview_height, rows, "borders = {borders}");
        }
        Ok(())
    }
}
