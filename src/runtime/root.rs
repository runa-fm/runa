//! Runtime root for runa.
//!
//! Holds the app states, the shared clipboard and the workers and drives them for the event
//! loop.
//! This Module is the main orchestration of runa, that ties everything together.
//!
//! [RunaRoot] is the main struct, which holds the [AppContainer], the [Clipboard] and the [Workers].

use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crossterm::{
    cursor::Hide,
    event::KeyEvent,
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::Frame;

use crate::app::tab;
use crate::app::{AppContainer, Clipboard, KeypressResult};
use crate::config::Config;
use crate::core::workers::{WorkerResponse, Workers};
use crate::ui;
use crate::utils::timings::{Throttler, Timings};

/// The main struct of runa
/// Contains the AppContainer, the shared clipboard and the worker pool
pub(crate) struct RunaRoot {
    container: AppContainer,
    clipboard: Clipboard,
    workers: Workers,
    ui_reload_throttler: Throttler,
    config_reload_throttler: Throttler,
    watched_dirs: [Option<PathBuf>; 3],
}

impl RunaRoot {
    #[inline]
    pub(crate) fn new(container: AppContainer, workers: Workers) -> Self {
        Self {
            container,
            clipboard: Clipboard::default(),
            workers,
            config_reload_throttler: Throttler::default(),
            ui_reload_throttler: Throttler::default(),
            watched_dirs: Default::default(),
        }
    }

    pub(crate) fn tick(&mut self) -> bool {
        let mut changed = self.update();
        changed |= self.container.current_mut().process_timers(&self.workers);

        if changed {
            self.sync_watch();

            if let AppContainer::Tabs(tabs) = &mut self.container {
                tabs.sync_tab_line();
            }
        }

        changed
    }

    pub(crate) fn handle_keypress(&mut self, key: KeyEvent) -> KeypressResult {
        let result =
            self.container
                .current_mut()
                .handle_keypress(key, &self.workers, &mut self.clipboard);

        let result = match result {
            KeypressResult::Tab(tab_act) => {
                tab::handle_tab_action(&self.workers, &mut self.container, tab_act)
            }
            KeypressResult::Sort(config) => {
                tab::handle_sort_action(&mut self.container, config);
                KeypressResult::Consumed
            }
            other => other,
        };

        self.sync_watch();
        result
    }

    pub(crate) fn render(&mut self, frame: &mut Frame) {
        let app = self.container.current_mut();
        let layout = ui::frame_layout(frame.area(), app);
        app.update_layout_metrics(&self.workers, layout.metrics());
        ui::render(
            frame,
            app,
            &layout,
            &self.clipboard,
            self.workers.fileop_counts(),
        );
    }

    pub(crate) fn reload_ui(&mut self, writer: &mut impl io::Write) -> io::Result<bool> {
        if !self.ui_reload_throttler.can_trigger(Timings::UI_RELOAD_MS) {
            return Ok(false);
        }

        self.ui_reload_throttler.touch();
        execute!(writer, LeaveAlternateScreen, EnterAlternateScreen, Hide,)?;
        match &mut self.container {
            AppContainer::Single(app) => {
                app.push_overlay_message("UI reloaded!".into(), Timings::MESSAGE_SHORT, None);
            }
            AppContainer::Tabs(tabs) => {
                for tab in tabs.iter_mut() {
                    tab.push_overlay_message("UI reloaded!".into(), Timings::MESSAGE_SHORT, None);
                }
            }
        }
        Ok(true)
    }

    fn update(&mut self) -> bool {
        let mut changed = false;

        while let Ok(response) = self.workers.response_rx().try_recv() {
            changed = true;
            if matches!(response, WorkerResponse::ConfigChanged) {
                self.reload_config();
                continue;
            }

            self.container
                .route_worker_response(response, &self.workers);
        }
        changed
    }

    fn sync_watch(&mut self) {
        let shown = self.container.current().shown_dirs();
        if shown
            .iter()
            .zip(&self.watched_dirs)
            .all(|(now, last)| *now == last.as_deref())
        {
            return;
        }

        let cache = self.workers.cache();
        for last in self.watched_dirs.iter().flatten() {
            if !shown.contains(&Some(last.as_path())) {
                cache.invalidate_path(last);
            }
        }

        self.watched_dirs = shown.map(|dir| dir.map(Path::to_path_buf));
        self.workers
            .retarget_watch(self.watched_dirs.iter().flatten().cloned().collect());
    }

    fn reload_config(&mut self) {
        if !self
            .config_reload_throttler
            .can_trigger(Timings::CONFIG_RELOAD_MS)
        {
            return;
        }

        match Config::load() {
            Ok(config) => {
                self.config_reload_throttler.touch();
                let new_config = Arc::new(config);

                match &mut self.container {
                    AppContainer::Single(app) => {
                        app.apply_new_config(Arc::clone(&new_config));
                        app.push_overlay_message(
                            "Configuration reloaded!".into(),
                            Timings::MESSAGE_SHORT,
                            None,
                        );
                    }
                    AppContainer::Tabs(tabs) => {
                        for tab in tabs.iter_mut() {
                            tab.apply_new_config(Arc::clone(&new_config));
                        }
                        tabs.sync_tab_line();
                        tabs.current_tab_mut().push_overlay_message(
                            "Configuration reloaded!".into(),
                            Timings::MESSAGE_SHORT,
                            None,
                        );
                    }
                }
            }
            Err(e) => {
                self.config_reload_throttler.touch();
                self.container
                    .current_mut()
                    .push_overlay_message(e, Timings::MESSAGE_LONG, None);
            }
        }
    }
}
