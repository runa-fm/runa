//! State and helpers for displaying and managing the preview pane in runa.
//!
//! Tracks the state of the file/directory preview for the UI, including loaded preview
//! data, debounce for background rendering, selection within the preview and request tracking

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use ansi_to_tui::IntoText;
use ratatui::style::Color;
use ratatui::text::Text;

use crate::app::actions::ScrollState;
use crate::core::{FileEntry, preview::ListingRow, workers::PreviewLines};
use crate::utils::{text::StrBuffer, timings::Timings};

/// Pages loaded above and below the visible part of a file preview.
const PRELOAD_PAGES: usize = 1;

/// Preview content for the preview pane
///
/// Holds loaded lines for file preview, directory entries for folder preview or empty if nothing.
/// Used to display or render file/folder content in the preview pane
pub(crate) enum PreviewData {
    Directory {
        entries: Arc<[FileEntry]>,
        sort_column: Option<Arc<StrBuffer>>,
    },
    File(Text<'static>),
    Listing(Vec<ListingRow>),
    Empty,
}

/// State and helpers for managing the preview pane.
pub(crate) struct PreviewState {
    data: PreviewData,
    selected_idx: usize,
    current_path: Option<PathBuf>,
    loaded_path: Option<PathBuf>,
    request_id: u64,
    pending: bool,
    last_input_time: Instant,
    scroll: ScrollState,
    loaded_scroll: usize,
    loaded_eof: bool,
    in_flight: bool,
}

impl PreviewState {
    crate::getters! {
        data: &PreviewData,
        selected_idx: usize,
        scroll: &ScrollState,
        request_id: u64,
        in_flight: bool,
    }

    pub(crate) fn current_path(&self) -> Option<&Path> {
        self.current_path.as_deref()
    }

    pub(crate) fn loaded_path(&self) -> Option<&Path> {
        self.loaded_path.as_deref()
    }

    // Setters / mutators

    /// Marks a file load as sent, so no other request for the same file overtakes it
    pub(crate) fn set_in_flight(&mut self) {
        self.in_flight = true;
    }

    /// Marks the preview as pending and updates the last input time
    pub(crate) fn mark_pending(&mut self) {
        if !self.pending {
            self.pending = true;
            self.last_input_time = Instant::now();
        }
    }

    /// Clears the pending state to indicate
    /// that a new preview request can be made
    pub(crate) fn clear_pending(&mut self) {
        self.pending = false;
    }

    // Debounce timing for preview render
    pub(crate) fn should_trigger(&self) -> bool {
        self.pending
            && self.last_input_time.elapsed() > Duration::from_millis(Timings::PREVIEW_DEBOUNCE_MS)
    }

    /// Prepares a new preview request for the given path
    /// Increments the request ID, sets the current path and marks as not pending
    pub(crate) fn prepare_new_request(&mut self, path: PathBuf) -> u64 {
        self.request_id = self.request_id.wrapping_add(1);

        if self.current_path.as_ref() != Some(&path) {
            self.keep_visible_lines();
            self.reset_scroll();
            self.current_path = Some(path);
        }

        self.pending = false;
        self.in_flight = false;
        self.request_id
    }

    /// Updates the preview content with new file lines
    /// Only applies the update if the request ID matches the latest
    pub(crate) fn update_content(
        &mut self,
        lines: PreviewLines,
        preview_height: usize,
        loaded_scroll: usize,
        is_eof: bool,
        request_id: u64,
    ) {
        if request_id == self.request_id {
            self.in_flight = false;
            let (data, len) = match lines {
                PreviewLines::Text(lines) => {
                    let len = lines.len();
                    let raw = lines.join("\n");
                    let mut text: Text<'static> =
                        raw.into_text().unwrap_or_else(|_| Text::from(raw));

                    Self::clear_reset_colors(&mut text);
                    (PreviewData::File(text), len)
                }
                PreviewLines::Listing(rows) => {
                    let len = rows.len();
                    (PreviewData::Listing(rows), len)
                }
            };
            let total_lines = loaded_scroll + len;
            self.data = data;
            self.loaded_path = self.current_path.clone();

            let max_offset = total_lines.saturating_sub(preview_height);
            self.scroll.set_max_offset(max_offset);
            if self.scroll().offset() > max_offset {
                self.scroll().set_offset(max_offset);
            }

            self.loaded_scroll = loaded_scroll;
            self.loaded_eof = is_eof;
            if self.needs_load(preview_height) {
                self.mark_pending();
            }
        }
    }

    /// Updates the preview content with new directory entries
    /// Only applies the update if the request ID matches the latest
    pub(crate) fn update_from_entries(
        &mut self,
        entries: Arc<[FileEntry]>,
        sort_column: Option<Arc<StrBuffer>>,
        selected_idx: usize,
        preview_height: usize,
        request_id: u64,
    ) {
        if request_id == self.request_id {
            let is_reload = self.loaded_path == self.current_path;
            let len = entries.len();

            self.data = PreviewData::Directory {
                entries,
                sort_column,
            };
            self.loaded_path = self.current_path.clone();
            self.selected_idx = selected_idx.min(len.saturating_sub(1));

            let max_offset = len.saturating_sub(preview_height);
            self.scroll.set_max_offset(max_offset);
            let offset = if is_reload {
                self.scroll.offset()
            } else {
                (self.selected_idx + 1).saturating_sub(preview_height)
            };

            self.scroll.set_offset(offset.min(max_offset));
        }
    }

    /// Sets an error message as the preview content
    pub(crate) fn set_error(&mut self, err: String) {
        self.data = PreviewData::File(Text::from(err));
        self.loaded_path = self.current_path.clone();
        self.in_flight = false;
    }

    /// Clears the preview state
    pub(crate) fn clear(&mut self) {
        self.data = PreviewData::Empty;
        self.selected_idx = 0;
        self.current_path = None;
        self.loaded_path = None;
        self.pending = false;
    }

    pub(crate) fn scroll_up(&mut self, preview_height: usize) {
        self.scroll.scroll_up();
        if self.needs_load(preview_height) {
            self.mark_pending();
        }
    }

    pub(crate) fn scroll_down(&mut self, preview_height: usize) {
        self.scroll.scroll_down();
        if self.needs_load(preview_height) {
            self.mark_pending();
        }
    }

    /// Returns the first line and line count to load: the view plus the preloaded pages around it
    pub(crate) fn load_window(&self, preview_height: usize) -> (usize, usize) {
        let offset = self.scroll.offset();
        let preload = preview_height * PRELOAD_PAGES;
        (
            offset.saturating_sub(preload),
            preload + preview_height + preload,
        )
    }

    /// Returns true when the view has used up half of the preloaded lines on either side
    pub(crate) fn needs_load(&self, preview_height: usize) -> bool {
        let len = match &self.data {
            PreviewData::File(text) => text.lines.len(),
            PreviewData::Listing(rows) => rows.len(),
            PreviewData::Directory { .. } | PreviewData::Empty => return false,
        };

        let offset = self.scroll.offset();
        let start = self.loaded_scroll;
        let end = start + len;
        let preload = preview_height * PRELOAD_PAGES;
        let margin = preload / 2;

        (start > 0 && offset < start + margin)
            || (!self.loaded_eof && offset + preview_height + margin > end)
    }

    /// Returns how many loaded lines lie or entries lie above the view.
    pub(crate) fn window_offset(&self) -> usize {
        let skip = self.scroll.offset().saturating_sub(self.loaded_scroll);
        match &self.data {
            PreviewData::File(text) => skip.min(text.lines.len().saturating_sub(1)),
            PreviewData::Listing(rows) => skip.min(rows.len().saturating_sub(1)),
            PreviewData::Directory { .. } => self.scroll.offset(),
            PreviewData::Empty => 0,
        }
    }

    fn reset_scroll(&mut self) {
        self.scroll.reset();
        self.loaded_scroll = 0;
        self.loaded_eof = false;
    }

    fn keep_visible_lines(&mut self) {
        let skip = self.window_offset();
        match &mut self.data {
            PreviewData::File(text) => {
                text.lines.drain(..skip);
            }
            PreviewData::Listing(rows) => {
                rows.drain(..skip);
            }
            PreviewData::Directory { .. } | PreviewData::Empty => {}
        }
    }

    fn clear_reset_colors(text: &mut Text<'_>) {
        for span in text.lines.iter_mut().flat_map(|line| line.spans.iter_mut()) {
            if span.style.fg == Some(Color::Reset) {
                span.style.fg = None;
            }

            if span.style.bg == Some(Color::Reset) {
                span.style.bg = None;
            }
        }
    }
}

impl Default for PreviewState {
    fn default() -> Self {
        Self {
            data: PreviewData::Empty,
            selected_idx: 0,
            current_path: None,
            loaded_path: None,
            request_id: 0,
            pending: false,
            last_input_time: Instant::now(),
            scroll: ScrollState::default(),
            loaded_scroll: 0,
            loaded_eof: false,
            in_flight: false,
        }
    }
}

impl PreviewData {
    /// Determines if the preview data is empty
    #[inline]
    pub(crate) fn is_empty(&self) -> bool {
        match self {
            PreviewData::Directory { entries, .. } => entries.is_empty(),
            PreviewData::File(text) => text.lines.is_empty(),
            PreviewData::Listing(rows) => rows.is_empty(),
            PreviewData::Empty => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::ffi::OsString;

    fn dir_entries(count: usize) -> Arc<[FileEntry]> {
        (0..count)
            .map(|i| FileEntry::new(OsString::from(i.to_string()), 0, None))
            .collect()
    }

    #[test]
    fn late_preview_keeps_its_window() {
        let mut preview = PreviewState::default();
        let id = preview.prepare_new_request(PathBuf::from("file.txt"));
        preview.scroll.set_max_offset(usize::MAX);

        preview.scroll_down(5);
        preview.scroll_down(5);

        let lines = (0..10).map(|i| i.to_string()).collect();
        preview.update_content(PreviewLines::Text(lines), 5, 0, false, id);

        assert_eq!(preview.loaded_scroll, 0);
        assert_eq!(preview.scroll().offset(), 2);
    }

    #[test]
    fn scrolling_inside_window_stays_local() {
        let mut preview = PreviewState::default();
        let id = preview.prepare_new_request(PathBuf::from("file.txt"));
        let lines = (0..30).map(|i| i.to_string()).collect();
        preview.update_content(PreviewLines::Text(lines), 10, 0, false, id);

        for _ in 0..15 {
            preview.scroll_down(10);
        }
        assert!(!preview.pending);
        assert_eq!(preview.window_offset(), 15);

        preview.scroll_down(10);
        assert!(preview.pending);
    }

    #[test]
    fn switching_file_keeps_old_view() {
        let mut preview = PreviewState::default();
        let id = preview.prepare_new_request(PathBuf::from("a.txt"));
        let lines = (0..30).map(|i| i.to_string()).collect();
        preview.update_content(PreviewLines::Text(lines), 10, 0, false, id);
        for _ in 0..15 {
            preview.scroll_down(10);
        }

        // Switch files; the new content has not arrived yet.
        preview.prepare_new_request(PathBuf::from("b.txt"));

        let PreviewData::File(text) = preview.data() else {
            panic!("expected the old file preview to stay");
        };
        let first_visible = &text.lines[preview.window_offset()];
        assert_eq!(first_visible.spans[0].content, "15");
    }

    #[test]
    fn dir_scroll_stops_at_last_page() {
        let mut preview = PreviewState::default();
        let id = preview.prepare_new_request(PathBuf::from("dir"));
        preview.update_from_entries(dir_entries(30), None, 0, 10, id);

        for _ in 0..25 {
            preview.scroll_down(10);
        }
        assert_eq!(preview.window_offset(), 20);
        assert!(!preview.pending);
    }

    #[test]
    fn dir_preview_opens_at_saved_entry() {
        let mut preview = PreviewState::default();
        let id = preview.prepare_new_request(PathBuf::from("dir"));
        preview.update_from_entries(dir_entries(30), None, 25, 10, id);

        assert_eq!(preview.window_offset(), 16);
        assert_eq!(preview.selected_idx(), 25);
    }

    #[test]
    fn dir_reload_keeps_scroll() {
        let mut preview = PreviewState::default();
        let id = preview.prepare_new_request(PathBuf::from("dir"));
        preview.update_from_entries(dir_entries(30), None, 0, 10, id);
        for _ in 0..5 {
            preview.scroll_down(10);
        }

        let id = preview.prepare_new_request(PathBuf::from("dir"));
        preview.update_from_entries(dir_entries(30), None, 0, 10, id);
        assert_eq!(preview.window_offset(), 5);
    }

    #[test]
    fn new_dir_starts_at_top() {
        let mut preview = PreviewState::default();
        let id = preview.prepare_new_request(PathBuf::from("a"));
        preview.update_from_entries(dir_entries(30), None, 0, 10, id);
        for _ in 0..5 {
            preview.scroll_down(10);
        }

        let id = preview.prepare_new_request(PathBuf::from("b"));
        preview.update_from_entries(dir_entries(30), None, 0, 10, id);
        assert_eq!(preview.window_offset(), 0);
    }
}
