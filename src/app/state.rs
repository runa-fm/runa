//! Application State and main controller module for runa.
//!
//! This module defines the overall [AppState] struct, which holds all major application
//! information and passes it to relevant UI/Terminal functions

mod file_actions;
mod input_mode;
mod nav_actions;
mod overlay_handlers;

use std::env;
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crossterm::event::KeyEvent;
use ratatui::text::Span;

use crate::app::{
    Clipboard, NavState, ParentState, PreviewData, PreviewState,
    actions::{ActionContext, ActionMode, InputMode},
    keymap::{Action, Keymap, TabAction},
    metadata::MetadataState,
    overlays::{OverlayKind, OverlayStack},
};
use crate::config::{Config, display::PreviewMethod};
use crate::core::{
    cache::DirListOptions,
    command,
    metadata::{FileMetadataCache, MetadataNeeds},
    sort::SortConfig,
    workers::{
        DirRequest, FindTask, LoadDirTask, MetadataTask, PreviewMode, PreviewTask, SortDirTask,
        WorkerResponse, Workers,
    },
};
use crate::utils::timings::{Throttler, Timings};

/// Enumeration for each individual keypress result processed.
///
/// Is used to process action logic correctly.
pub(crate) enum KeypressResult {
    Continue,
    Consumed,
    Quit,
    OpenedEditor,
    Recovered,
    UiReload,
    Tab(TabAction),
    Sort(SortConfig),
}

/// Enumeration which holds the metrics of the layout of the TUI
#[derive(Debug, Clone, Copy)]
pub(crate) struct LayoutMetrics {
    pub(crate) parent_width: usize,
    pub(crate) main_width: usize,
    pub(crate) preview_width: usize,
    pub(crate) preview_height: usize,
}

impl Default for LayoutMetrics {
    fn default() -> Self {
        Self {
            parent_width: 20,
            main_width: 40,
            preview_width: 40,
            preview_height: 50,
        }
    }
}

/// Main struct which holds the central Application state of runa
///
/// AppState holds all the persisten state for the application while it is running
///
/// Functions are provided for the core event loop, input handling, file navigationm
/// worker requests and Notification management.
pub(crate) struct AppState {
    config: Arc<Config>,
    keymap: Keymap,

    metrics: LayoutMetrics,

    nav: NavState,
    actions: ActionContext,
    preview: PreviewState,
    parent: ParentState,

    metadata: MetadataState,

    is_loading: bool,

    notification_time: Option<Instant>,
    worker_time: Option<Instant>,

    nav_time: Throttler,
    preview_request_time: Throttler,

    overlays: OverlayStack,

    tab_id: Option<usize>,
    tab_line: Arc<Vec<Span<'static>>>,
}

impl AppState {
    pub(crate) fn new(config: Arc<Config>) -> io::Result<Self> {
        let current_dir = env::current_dir()?;
        Self::from_dir(config, &current_dir)
    }

    pub(crate) fn new_current_dir(&self) -> io::Result<Self> {
        let mut app = Self::from_dir(Arc::clone(&self.config), self.nav.current_dir())?;
        app.nav.set_sort_config(self.nav.sort_config());
        Ok(app)
    }

    pub(crate) fn from_dir(config: Arc<Config>, initial_path: &Path) -> io::Result<Self> {
        let current_dir = if initial_path.exists() && initial_path.is_dir() {
            initial_path.to_path_buf()
        } else {
            env::current_dir()?
        };

        let app = Self {
            keymap: Keymap::from_config(config.as_ref()),
            config,
            metrics: LayoutMetrics::default(),
            nav: NavState::new(current_dir),
            actions: ActionContext::default(),
            preview: PreviewState::default(),
            parent: ParentState::default(),
            metadata: MetadataState::new(),
            is_loading: false,
            notification_time: None,
            nav_time: Throttler::new(),
            preview_request_time: Throttler::new(),
            worker_time: None,
            overlays: OverlayStack::new(),
            tab_line: Arc::new(Vec::new()),
            tab_id: None,
        };

        Ok(app)
    }

    pub(crate) fn apply_new_config(&mut self, config: Arc<Config>) {
        self.config = config;
        self.keymap = Keymap::from_config(self.config.as_ref());
    }

    /// Initializes the AppState by requesting the initial directory load and parent content.
    pub(crate) fn initialize(&mut self, workers: &Workers, focus: Option<OsString>) {
        self.is_loading = true;
        self.request_dir_load(workers, focus);
        self.request_parent_content(workers);
    }

    crate::getters! {
        nav: &NavState,
        actions: &ActionContext,
        preview: &PreviewState,
        parent: &ParentState,
        is_loading: bool,
        worker_time: &Option<Instant>,
        overlays: &OverlayStack,
    }

    #[inline]
    pub(crate) fn config(&self) -> &Config {
        self.config.as_ref()
    }

    #[inline]
    pub(crate) fn overlays_mut(&mut self) -> &mut OverlayStack {
        &mut self.overlays
    }

    #[inline]
    pub(crate) fn tab_line(&self) -> &Arc<Vec<Span<'_>>> {
        &self.tab_line
    }

    #[inline]
    pub(super) fn tab_id(&self) -> Option<usize> {
        self.tab_id
    }

    #[inline]
    pub(crate) fn set_tab_id(&mut self, id: usize) {
        self.tab_id = Some(id);
    }

    pub(super) fn set_tab_line(&mut self, line: Vec<Span<'static>>) {
        self.tab_line = Arc::new(line);
    }

    #[inline]
    pub(crate) fn selected_metadata(&self) -> Option<&FileMetadataCache> {
        self.metadata.selected()
    }

    // Entry functions

    pub(crate) fn visible_selected(&self) -> Option<usize> {
        if self.nav.entries().is_empty() {
            None
        } else {
            Some(self.nav.selected_idx())
        }
    }

    pub(crate) fn has_visible_entries(&self) -> bool {
        !self.nav.entries().is_empty()
    }

    /// Returns the directories the panes show in order:
    /// - current/main pane
    /// - parent
    /// - preview, only when it shows a directory
    pub(crate) fn shown_dirs(&self) -> [Option<&Path>; 3] {
        let current = self.nav.current_dir();
        let preview = self
            .preview
            .loaded_path()
            .filter(|_| matches!(self.preview.data(), PreviewData::Directory { .. }));

        [Some(current), current.parent(), preview]
    }

    /// Metrics updater for LayoutMetrics to request_preview new preview after old metrics
    pub(crate) fn update_layout_metrics(&mut self, workers: &Workers, metrics: LayoutMetrics) {
        let old_width = self.metrics.preview_width;
        let old_height = self.metrics.preview_height;

        self.metrics = metrics;

        if old_width != self.metrics.preview_width || old_height != self.metrics.preview_height {
            if self.preview.data().is_empty() {
                self.request_preview(workers);
            } else {
                self.preview.mark_pending();
            }
        }
    }

    /// Update the file info cache by requesting a metadata load.
    pub(crate) fn update_file_info_cache(&mut self, workers: &Workers) {
        let status_info = self.config.display().info().status_bar();
        let info_overlay = self.overlays().is_open(OverlayKind::ShowInfo);

        if !status_info && !info_overlay {
            return;
        }
        if !self.metadata.try_request(Timings::FILE_INFO_THROTTLE_MS) {
            return;
        }

        let Some(entry) = self.nav.selected_entry() else {
            self.metadata.clear();
            return;
        };

        if let Some(selected_cache) = self.metadata.selected_arc()
            && entry.name() == selected_cache.name()
        {
            return;
        }

        let path = self.nav.current_dir().join(entry.name());
        if self.metadata.is_pending_path(&path) {
            return;
        }

        let request_id = self.metadata.prepare_new_request();
        let date_format = self.config.display().info().date_format();

        let needs = self.metadata_needs();

        if workers
            .metadata_tx()
            .try_send(MetadataTask {
                path: path.clone(),
                date_format: date_format.to_string(),
                needs,
                request_id,
                tab_id: self.tab_id(),
            })
            .is_ok()
        {
            self.metadata.touch();
            self.metadata.set_pending(request_id, path);
        }
    }

    /// Applies a new sort configuration to the navigation state.
    pub(crate) fn apply_sort_config(&mut self, config: SortConfig) {
        self.nav.set_sort_config(config);
    }

    /// Process time-based state: message expiration, fil-op indicator, prefix timeout,
    /// preview debounce, and request throttles.
    ///
    /// Returns true if anything change and the UI needs a redraw.
    pub(crate) fn process_timers(&mut self, workers: &Workers) -> bool {
        let mut changed = false;

        if let Some(expiry) = self.notification_time
            && Instant::now() >= expiry
        {
            self.notification_time = None;

            self.overlays_mut().remove_kind(OverlayKind::Message);

            changed = true;
        }

        // Handle file operation worker activity indicator
        if workers.active_fileops().load(Ordering::Relaxed) > 0 {
            let start = *self.worker_time.get_or_insert_with(Instant::now);

            if start.elapsed() >= Duration::from_millis(Timings::WORKER_INDICATOR_MS) {
                changed = true;
            }
        } else if self.worker_time.is_some() {
            self.worker_time = None;
            changed = true;
        }

        // Handle prefix recognition expiration and cancel if expired
        let prefix_recognizer = self.actions.prefix_recognizer_mut();
        if prefix_recognizer.expired() {
            prefix_recognizer.cancel();
            self.hide_prefix_help();
            changed = true;
        }

        // Handle preview debounc
        if self.preview.should_trigger() {
            self.request_preview(workers);
            changed = true;
        }

        // Handle preview request throttling for a tick
        if self
            .preview_request_time
            .take_queued(Timings::PREVIEW_REQUEST_MS)
        {
            self.request_preview(workers);
            changed = true;
        }

        // Metadata handler for file info and cache
        if self.metadata.take_queued(Timings::FILE_INFO_THROTTLE_MS) {
            self.update_file_info_cache(workers);
            changed = true;
        }

        // Find handling with debounce
        if let ActionMode::Input {
            mode: InputMode::Find,
            ..
        } = self.actions.mode()
            && let Some(query) = self.actions.take_query()
        {
            if query.is_empty() {
                self.actions.find_mut().clear_results();
            } else {
                self.request_find(workers, query);
            }
            changed = true;
        }

        changed
    }

    pub(crate) fn handle_worker_response(&mut self, response: WorkerResponse, workers: &Workers) {
        match response {
            WorkerResponse::DirectoryLoaded {
                path,
                entries,
                focus,
                sort_column,
                request,
                ..
            } => match request {
                DirRequest::Nav(request_id) => {
                    // only update nav if BOTH the ID and path match.
                    if request_id == self.nav.request_id() && path == self.nav.current_dir() {
                        let focus = if self.nav.take_reload(request_id) {
                            self.nav.selected_entry().map(|e| e.name().to_os_string())
                        } else {
                            focus
                        };

                        self.nav
                            .update_from_worker(path, entries, sort_column, focus);
                        self.is_loading = false;

                        self.request_parent_content(workers);
                        self.request_preview_force(workers);
                        self.update_file_info_cache(workers);
                        self.refresh_show_info_if_open();
                    }
                }
                DirRequest::Preview(request_id) => {
                    // PREVIEW CHECK: Must match the current preview request
                    if request_id == self.preview.request_id() && self.nav.is_selected_path(&path) {
                        let pos = self
                            .nav
                            .get_position()
                            .get(&path)
                            .and_then(|saved| entries.iter().position(|e| e.name() == saved))
                            .unwrap_or(0);

                        self.preview.update_from_entries(
                            entries,
                            sort_column,
                            pos,
                            self.metrics.preview_height,
                            request_id,
                        );
                    }
                }
                DirRequest::Parent(request_id) => {
                    // PARENT CHECK: Must match the current parent request
                    if request_id == self.parent.request_id() {
                        let expected_parent = self.nav.current_dir().parent();
                        if expected_parent == Some(path.as_path()) {
                            let current_name = self
                                .nav
                                .current_dir()
                                .file_name()
                                .map(|n| n.to_string_lossy().to_string())
                                .unwrap_or_default();
                            self.parent.update_from_entries(
                                entries,
                                &current_name,
                                request_id,
                                &path,
                                sort_column,
                            );
                        }
                    }
                }
            },

            WorkerResponse::PreviewLoaded {
                path,
                lines,
                scroll,
                is_eof,
                request_id,
                tab_id: _tab_id,
            } => {
                if request_id == self.preview.request_id() && self.nav.is_selected_path(&path) {
                    self.preview.update_content(
                        lines,
                        self.metrics.preview_height,
                        scroll,
                        is_eof,
                        request_id,
                    );
                }
            }

            WorkerResponse::OperationComplete {
                need_reload,
                focus,
                modified_dirs,
            } => {
                if need_reload {
                    let invalidation_paths = self.fileop_invalidation_paths(modified_dirs);

                    for path in &invalidation_paths {
                        workers.cache().invalidate_path(path);
                        self.parent.invalidate_if_path(path);
                    }

                    let should_refresh_preview = self.preview.current_path().is_some_and(|path| {
                        invalidation_paths
                            .iter()
                            .any(|dir| path == dir || path.parent() == Some(dir.as_path()))
                    });
                    if should_refresh_preview {
                        self.request_preview_force(workers);
                    }

                    self.request_dir_load(workers, focus);
                    self.request_parent_content(workers);
                }
            }

            WorkerResponse::DirsChanged { dirs } => {
                self.refresh_changed_dirs(workers, &dirs);
            }

            WorkerResponse::FileMetadataLoaded {
                metadata,
                path,
                request_id,
                tab_id: _tab_id,
            } => {
                if self.metadata.matches_pending(request_id, &path) {
                    if path.parent() == Some(self.nav.current_dir()) {
                        self.metadata.set_selected(Some(metadata));
                        self.refresh_show_info_if_open();
                    }
                    self.metadata.clear_pending();
                }
            }

            WorkerResponse::FindResults {
                base_dir,
                results,
                request_id,
                tab_id: _tab_id,
            } => {
                if base_dir == self.nav.current_dir()
                    && request_id == self.actions.find().request_id()
                {
                    self.actions.find_mut().set_results(results);
                }
            }

            WorkerResponse::DirectoryFailed {
                path,
                message,
                request,
                ..
            } => match request {
                DirRequest::Nav(request_id) => {
                    if request_id == self.nav.request_id() && path == self.nav.current_dir() {
                        self.is_loading = false;
                        self.push_overlay_message(message, Timings::MESSAGE_WORKER_ERROR, None);
                    }
                }
                DirRequest::Preview(request_id) => {
                    if request_id == self.preview.request_id() && self.nav.is_selected_path(&path) {
                        self.preview.set_error(message);
                    }
                }
                DirRequest::Parent(request_id) => {
                    if request_id == self.parent.request_id()
                        && self.nav.current_dir().parent() == Some(path.as_path())
                    {
                        self.push_overlay_message(message, Timings::MESSAGE_WORKER_ERROR, None);
                    }
                }
            },

            WorkerResponse::Error(e) => {
                self.push_overlay_message(e, Timings::MESSAGE_WORKER_ERROR, None);
            }

            WorkerResponse::ConfigChanged => {}
        }
    }

    /// Central key handlers
    ///
    /// Coordinates the action and handler module functions.
    pub(crate) fn handle_keypress(
        &mut self,
        key: KeyEvent,
        workers: &Workers,
        clipboard: &mut Clipboard,
    ) -> KeypressResult {
        if self.actions.is_input_mode() {
            return self.handle_input_mode(workers, key);
        }

        if let Some(res) = self.handle_esc_close_overlays(&key) {
            return res;
        }

        if let Some(res) = self.handle_prefix_dispatch(workers, &key) {
            return res;
        }

        if let Some(action) = self.keymap.lookup(key) {
            match action {
                Action::System(sys_act) => return self.handle_sys_action(sys_act),
                Action::Nav(nav_act) => return self.handle_nav_action(workers, nav_act, clipboard),
                Action::File(file_act) => {
                    return self.handle_file_action(workers, file_act, clipboard);
                }
                Action::Tab(tab_act) => return KeypressResult::Tab(tab_act),
                Action::Command(index) => return self.handle_user_command(workers, index),
            }
        }

        KeypressResult::Continue
    }

    // Worker requests functions for directory loading, preview and parent pane content

    /// Requests a directory load for the current navigation directory
    pub(crate) fn request_dir_load(&mut self, workers: &Workers, focus: Option<OsString>) {
        let request_id = self.nav.prepare_new_request();
        let sort_config = self.nav.sort_config();
        let sort_date_format: Arc<str> = Arc::from(self.config.display().sort_date_format());
        workers.send_nav_io(LoadDirTask {
            path: self.nav.current_dir().to_path_buf(),
            focus,
            list: self.dir_list_options(),
            sort_config,
            sort_date_format,
            always_show: Arc::clone(self.config.general().always_show()),
            request: DirRequest::Nav(request_id),
            tab_id: self.tab_id(),
        });
    }

    pub(crate) fn request_dir_reload(&mut self, workers: &Workers) {
        self.request_dir_load(workers, None);
        self.nav.mark_reload(self.nav.request_id());
    }

    #[inline]
    pub(crate) fn request_preview(&mut self, workers: &Workers) {
        self.do_request_preview(workers, false);
    }

    #[inline]
    pub(crate) fn request_preview_force(&mut self, workers: &Workers) {
        self.do_request_preview(workers, true);
    }

    /// Requests loading of the parent directory content for the parent pane
    pub(crate) fn request_parent_content(&mut self, workers: &Workers) {
        let Some(parent_path) = self.nav.current_dir().parent() else {
            self.parent.clear();
            return;
        };

        let sort_config = self.nav.sort_config();
        let list_opts = self.dir_list_options();

        if let Some(val) = workers.cache().get(parent_path, sort_config, &list_opts) {
            let (entries, sort_column) = (val.entries(), val.sort_column());
            let current_name = self
                .nav
                .current_dir()
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            let request_id = self.parent.prepare_new_request(parent_path);
            self.parent.update_from_entries(
                entries.clone(),
                &current_name,
                request_id,
                parent_path,
                sort_column.clone(),
            );
            return;
        }

        let parent_path_buf = parent_path.to_path_buf();
        let sort_date_format: Arc<str> = Arc::from(self.config.display().sort_date_format());
        let request_id = self.parent.prepare_new_request(&parent_path_buf);
        workers.send_parent_io(LoadDirTask {
            path: parent_path_buf,
            focus: None,
            list: self.dir_list_options(),
            sort_config,
            sort_date_format,
            always_show: Arc::clone(self.config.general().always_show()),
            request: DirRequest::Parent(request_id),
            tab_id: self.tab_id(),
        });
    }

    pub(crate) fn request_dir_sort(&mut self, workers: &Workers, focus: Option<OsString>) {
        let request_id = self.nav.request_id();
        let sort_config = self.nav.sort_config();
        let sort_date_format: Arc<str> = Arc::from(self.config.display().sort_date_format());
        let entries = self.nav.entries_arc();

        workers.send_sort(SortDirTask {
            path: self.nav.current_dir().to_path_buf(),
            entries,
            focus,
            list: self.dir_list_options(),
            sort_config,
            sort_date_format,
            always_show: Arc::clone(self.config.general().always_show()),
            request: DirRequest::Nav(request_id),
            tab_id: self.tab_id(),
        });
    }

    pub(crate) fn dir_list_options(&self) -> DirListOptions {
        DirListOptions {
            dirs_first: self.config.general().dirs_first(),
            show_hidden: self.config.general().show_hidden(),
            show_symlink: self.config.general().show_symlink(),
            show_system: self.config.general().show_system(),
            case_insensitive: self.config.general().case_insensitive(),
        }
    }

    /// Requests a recursive find operation for the current navigation directory
    fn request_find(&mut self, workers: &Workers, query: String) {
        self.actions.find_mut().cancel_current();

        let request_id = self.actions.find_mut().prepare_new_request();
        let cancel_token = Arc::new(AtomicBool::new(false));

        let show_hidden = self.config.general().show_hidden();

        self.actions
            .set_cancel_find_token(Arc::clone(&cancel_token));

        workers.send_find(FindTask {
            base_dir: self.nav.current_dir().to_path_buf(),
            query,
            max_results: self.config().general().max_find_results(),
            request_id,
            show_hidden,
            cancel: cancel_token,
            tab_id: self.tab_id(),
        });
    }

    /// Requests a preview load for the currently selected entry in the navigation pane
    fn do_request_preview(&mut self, workers: &Workers, force: bool) {
        if let Some(entry) = self.nav.selected_entry() {
            let path = self.nav.current_dir().join(entry.name());

            if !force
                && self.preview.loaded_path() == Some(path.as_path())
                && !self.preview.data().is_empty()
                && (self.preview.in_flight()
                    || !self.preview.needs_load(self.metrics.preview_height))
            {
                self.preview.clear_pending();
                return;
            }

            if !force
                && !self
                    .preview_request_time
                    .try_trigger(Timings::PREVIEW_REQUEST_MS)
            {
                return;
            }

            let request_id = self.preview.prepare_new_request(path.clone());
            let sort_config = self.nav.sort_config();
            let list_opts = self.dir_list_options();
            let sort_date_format: Arc<str> = Arc::from(self.config.display().sort_date_format());

            if entry.is_dir() || entry.is_symlink() {
                if let Some(val) = workers.cache().get(&path, sort_config, &list_opts) {
                    let (entries, sort_column) = (val.entries(), val.sort_column());
                    let pos = self
                        .nav
                        .get_position()
                        .get(&path)
                        .and_then(|saved| entries.iter().position(|e| e.name() == saved))
                        .unwrap_or(0);

                    self.preview.update_from_entries(
                        entries.clone(),
                        sort_column.clone(),
                        pos,
                        self.metrics.preview_height,
                        request_id,
                    );

                    self.preview_request_time.touch();
                    return;
                }

                if workers
                    .preview_dir_tx()
                    .try_send(LoadDirTask {
                        path,
                        focus: None,
                        list: list_opts.clone(),
                        sort_config,
                        sort_date_format,
                        always_show: Arc::clone(self.config.general().always_show()),
                        request: DirRequest::Preview(request_id),
                        tab_id: self.tab_id(),
                    })
                    .is_ok()
                {
                    self.preview_request_time.touch();
                } else {
                    self.preview.mark_pending();
                }
            } else {
                let preview_options = self.config.display().preview_options();
                let preview_method = preview_options.method();
                let preview_mode = if let Some(archive) = preview_options.archive_cmd(&path)
                    && let Some(program) = archive.args().first()
                {
                    let target = std::path::absolute(&path).unwrap_or_else(|_| path.clone());
                    PreviewMode::Archive {
                        program: program.into(),
                        args: command::expand_args(archive.args(), archive.ext_parts(), &target),
                        dir: target.parent().map(Path::to_path_buf),
                    }
                } else {
                    match preview_method {
                        PreviewMethod::Internal => PreviewMode::Internal,
                        PreviewMethod::Bat => {
                            let args = self
                                .config
                                .bat_args_for_preview(self.metrics.preview_width)
                                .into_iter()
                                .map(OsString::from)
                                .collect();
                            PreviewMode::Bat { args }
                        }
                    }
                };
                let (scroll, max_lines) = self.preview.load_window(self.metrics.preview_height);

                if workers
                    .preview_file_tx()
                    .try_send(PreviewTask {
                        path,
                        max_lines,
                        pane_width: self.metrics.preview_width,
                        scroll,
                        preview_mode,
                        request_id,
                        tab_id: self.tab_id(),
                    })
                    .is_ok()
                {
                    self.preview_request_time.touch();
                    self.preview.set_in_flight();
                } else {
                    self.preview.mark_pending();
                }
            }
        } else {
            self.preview.clear();
        }
    }

    fn fileop_invalidation_paths(&self, modified_dirs: Vec<PathBuf>) -> Vec<PathBuf> {
        let current_dir = self.nav.current_dir().to_path_buf();
        let mut invalidation_paths = Vec::with_capacity(modified_dirs.len().saturating_mul(2) + 1);

        for path in modified_dirs {
            invalidation_paths.push(path.clone());
            #[cfg(windows)]
            {
                use crate::utils::path;
                let cleaned = PathBuf::from(path::clean_display_path(&path.to_string_lossy()));
                if cleaned != path {
                    invalidation_paths.push(cleaned);
                }
            }
        }
        invalidation_paths.push(current_dir);
        invalidation_paths.dedup();
        invalidation_paths
    }

    fn refresh_changed_dirs(&mut self, workers: &Workers, dirs: &[PathBuf]) {
        for path in dirs {
            workers.cache().invalidate_path(path);
            self.parent.invalidate_if_path(path);
        }

        let current = self.nav.current_dir();
        let touches_current = dirs.iter().any(|d| d == current);
        let touches_parent = current
            .parent()
            .is_some_and(|parent| dirs.iter().any(|d| d == parent));
        let touches_preview = self.preview.current_path().is_some_and(|preview| {
            dirs.iter()
                .any(|d| preview == d || preview.parent() == Some(d.as_path()))
        });

        if touches_current {
            self.request_dir_reload(workers);
        }

        if touches_parent {
            self.request_parent_content(workers);
        }

        if touches_preview && !touches_current {
            self.request_preview_force(workers);
        }
    }

    fn metadata_needs(&self) -> MetadataNeeds {
        MetadataNeeds {
            name: self.config.display().info().name(),
            file_type: self.config.display().info().file_type(),
            size: self.config.display().info().size(),
            accessed: self.config.display().info().accessed(),
            modified: self.config.display().info().modified(),
            created: self.config.display().info().created(),
            perms: self.config.display().info().perms(),
            #[cfg(unix)]
            owner: self.config.display().info().owner(),
            #[cfg(unix)]
            group: self.config.display().info().group(),
        }
    }
}

// AppState tests
#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::overlays::Overlay;
    use crate::core::FileEntry;
    use ratatui::layout::Alignment;
    use std::ffi::OsString;
    use std::time::{Duration, Instant};
    use tempfile::tempdir;

    fn dummy_config() -> Config {
        Config::default()
    }

    fn dummy_workers() -> Workers {
        Workers::spawn()
    }

    #[test]
    fn tick_clears_expired_notification_message() -> Result<(), Box<dyn std::error::Error>> {
        let config = dummy_config();
        let workers = dummy_workers();
        let temp = tempdir()?;
        let mut app = AppState::from_dir(Arc::new(config), temp.path())?;
        app.notification_time = Some(Instant::now() - Duration::from_secs(2));
        app.overlays_mut().push(Overlay::Message {
            text: "Timed MSG".to_string(),
            alignment: Alignment::Center,
        });
        let changed = app.process_timers(&workers);
        assert!(changed);
        assert!(app.notification_time.is_none());
        assert!(!app.overlays().is_open(OverlayKind::Message));
        Ok(())
    }

    #[test]
    fn initialize_sets_is_loading() -> Result<(), Box<dyn std::error::Error>> {
        let config = dummy_config();
        let workers = dummy_workers();
        let temp = tempdir()?;
        let mut app = AppState::from_dir(Arc::new(config), temp.path())?;
        app.initialize(&workers, None);
        assert!(app.is_loading);
        Ok(())
    }

    #[test]
    fn request_dir_load_sets_is_loading() -> Result<(), Box<dyn std::error::Error>> {
        let config = dummy_config();
        let workers = dummy_workers();
        let temp = tempdir()?;
        let mut app = AppState::from_dir(Arc::new(config), temp.path())?;
        app.is_loading = false;
        app.request_dir_reload(&workers);
        assert!(!app.is_loading);
        Ok(())
    }

    #[test]
    fn reload_keeps_the_selection_made_while_loading() -> Result<(), Box<dyn std::error::Error>> {
        let workers = dummy_workers();
        let temp = tempdir()?;
        let mut app = AppState::from_dir(Arc::new(dummy_config()), temp.path())?;
        let dir = app.nav().current_dir().to_path_buf();
        let entries: Arc<[FileEntry]> = ["a", "b", "c"]
            .iter()
            .map(|name| FileEntry::new(OsString::from(name), 0, None))
            .collect();
        app.nav
            .update_from_worker(dir.clone(), Arc::clone(&entries), None, None);

        app.refresh_changed_dirs(&workers, std::slice::from_ref(&dir));
        let request_id = app.nav.request_id();
        app.nav.move_down();
        app.nav.move_down();

        let loaded = WorkerResponse::DirectoryLoaded {
            path: dir,
            entries,
            focus: Some(OsString::from("a")),
            sort_column: None,
            request: DirRequest::Nav(request_id),
            tab_id: None,
        };
        app.handle_worker_response(loaded, &workers);

        assert_eq!(app.nav().selected_entry().map(|e| e.name_str()), Some("c"));
        Ok(())
    }

    #[test]
    fn only_matching_errors_reach_the_preview() -> Result<(), Box<dyn std::error::Error>> {
        let workers = dummy_workers();
        let temp = tempdir()?;
        let mut app = AppState::from_dir(Arc::new(dummy_config()), temp.path())?;
        let dir = app.nav().current_dir().to_path_buf();
        let sub = dir.join("sub");
        let entries = Arc::from(vec![FileEntry::new(OsString::from("sub"), 0, None)]);
        app.nav.update_from_worker(dir.clone(), entries, None, None);

        let id = app.preview.prepare_new_request(sub.clone());
        let lines = crate::core::workers::PreviewLines::Text(vec!["hello".to_string()]);
        app.preview.update_content(lines, 10, 0, true, id);
        let shown = |app: &AppState| match app.preview().data() {
            crate::app::PreviewData::File(text) => text.lines[0].to_string(),
            _ => String::new(),
        };
        let failed = |path: PathBuf| WorkerResponse::DirectoryFailed {
            path,
            message: "I/O Error: access denied".to_string(),
            request: DirRequest::Preview(id),
            tab_id: None,
        };

        let metadata_error = WorkerResponse::Error("Metadata Error: access denied".to_string());
        app.handle_worker_response(metadata_error, &workers);
        assert_eq!(shown(&app), "hello");

        app.handle_worker_response(failed(dir.join("other")), &workers);
        assert_eq!(shown(&app), "hello");

        app.handle_worker_response(failed(dir.join("other").join("sub")), &workers);
        assert_eq!(shown(&app), "hello");

        app.handle_worker_response(failed(sub), &workers);
        assert_eq!(shown(&app), "I/O Error: access denied");
        Ok(())
    }

    #[test]
    fn request_parent_content_handles_root() -> Result<(), Box<dyn std::error::Error>> {
        let config = dummy_config();
        let workers = dummy_workers();
        let temp = tempdir()?;
        let mut app = AppState::from_dir(Arc::new(config), temp.path())?;
        #[cfg(unix)]
        {
            use std::path::PathBuf;
            app.nav.set_path(PathBuf::from("/"));
            app.request_parent_content(&workers);
            assert!(app.parent.entries().is_empty());
        }
        #[cfg(windows)]
        {
            use std::path::PathBuf;
            app.nav.set_path(PathBuf::from("C:\\"));
            app.request_parent_content(&workers);
            assert!(app.parent.entries().is_empty());
        }
        Ok(())
    }

    #[test]
    fn request_parent_content_does_not_use_parent_state_as_cache()
    -> Result<(), Box<dyn std::error::Error>> {
        let config = Config::default();
        let workers = dummy_workers();
        let temp = tempdir()?;
        let subdir = temp.path().join("subdir");
        std::fs::create_dir(&subdir)?;
        let mut app = AppState::from_dir(Arc::new(config), &subdir)?;

        let parent_path = app
            .nav()
            .current_dir()
            .parent()
            .expect("Should have parent")
            .to_path_buf();

        let prev_request_id = app.parent.request_id();
        let file_entry = FileEntry::new(OsString::from("test_file"), 0, None);
        let dir_entry = FileEntry::new(OsString::from("test_dir"), 1, None);

        app.parent.update_from_entries(
            Arc::from(vec![file_entry, dir_entry]),
            "irrelevant",
            prev_request_id,
            &parent_path,
            None,
        );

        app.request_parent_content(&workers);

        assert!(
            app.parent.request_id() > prev_request_id,
            "request_id did not change: request_parent_content should schedule a fresh parent request when worker cache misses"
        );

        Ok(())
    }

    #[test]
    fn shown_dirs_follow_panes() -> Result<(), Box<dyn std::error::Error>> {
        let temp = tempdir()?;
        let current = temp.path().join("a");
        let y = current.join("y");

        std::fs::create_dir_all(&y)?;
        let mut app = AppState::from_dir(Arc::new(dummy_config()), &current)?;

        assert_eq!(
            app.shown_dirs(),
            [Some(current.as_path()), Some(temp.path()), None]
        );

        let request_id = app.preview.prepare_new_request(y.clone());
        app.preview
            .update_from_entries(Arc::default(), None, 0, 10, request_id);
        let with_y = [
            Some(current.as_path()),
            Some(temp.path()),
            Some(y.as_path()),
        ];
        assert_eq!(app.shown_dirs(), with_y);

        let request_id = app.preview.prepare_new_request(current.join("f"));
        assert_eq!(
            app.shown_dirs(),
            with_y,
            "y stays shown until the file loads"
        );

        let lines = crate::core::workers::PreviewLines::Text(vec!["x".to_string()]);
        app.preview.update_content(lines, 10, 0, true, request_id);
        assert_eq!(
            app.shown_dirs(),
            [Some(current.as_path()), Some(temp.path()), None]
        );
        Ok(())
    }
}
