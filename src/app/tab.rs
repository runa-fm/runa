//! Tab management for runa.
//!
//! This module deines the `TabManager` struct, which manages multiple tabs in the application,
//! allowing to switch between them, add new tabs, and close existing ones.
//!
//! It also includes the [handle_tab_action] function, which processes tab actions related to tab management.

use std::ffi::OsString;
use std::path::Path;

use ratatui::text::Span;

use crate::app::{AppContainer, AppState, KeypressResult, keymap::TabAction};
use crate::core::{listing::SortConfig, workers::Workers};
use crate::utils::{path, timings};

/// Maximum number of tabs runa opens.
pub(crate) const MAX_TABS: usize = 9;

pub(crate) struct TabManager {
    tabs: Vec<AppState>,
    current: usize,
    next_tab_id: usize,
}

impl TabManager {
    pub(crate) fn new(
        mut existing: AppState,
        mut new_tab: AppState,
        workers: &Workers,
        focus: Option<OsString>,
    ) -> Self {
        existing.set_tab_id(0);
        new_tab.set_tab_id(1);

        new_tab.initialize(workers, focus);
        existing.transfer_shared_overlays_to(&mut new_tab);

        let mut manager = Self {
            tabs: vec![existing, new_tab],
            current: 1,
            next_tab_id: 2,
        };
        manager.sync_tab_line();
        manager
    }

    pub(crate) fn from_vec(mut tabs: Vec<AppState>) -> Self {
        for (i, tab) in tabs.iter_mut().enumerate() {
            if tab.tab_id().is_none() {
                tab.set_tab_id(i);
            }
        }

        let next_id = tabs.len();
        let mut manager = Self {
            tabs,
            current: 0,
            next_tab_id: next_id,
        };
        manager.sync_tab_line();
        manager
    }

    #[inline]
    pub(crate) fn len(&self) -> usize {
        self.tabs.len()
    }

    #[inline]
    pub(crate) fn is_empty(&self) -> bool {
        self.tabs.is_empty()
    }

    pub(crate) fn iter_mut(&mut self) -> impl Iterator<Item = &mut AppState> {
        self.tabs.iter_mut()
    }

    pub(crate) fn current_tab(&self) -> &AppState {
        &self.tabs[self.current]
    }

    pub(crate) fn current_tab_mut(&mut self) -> &mut AppState {
        &mut self.tabs[self.current]
    }

    pub(crate) fn sync_tab_line(&mut self) {
        let tab_spans = if self.tabs.len() <= 1 {
            Vec::new()
        } else {
            let (tab_theme, separator) = {
                let theme = self.tabs[self.current].config().theme();
                (theme.tab().clone(), theme.tab().separator().to_string())
            };

            let mut spans = Vec::with_capacity(self.tabs.len() * 2 - 1);
            for (i, tab) in self.tabs.iter().enumerate() {
                let is_current = i == self.current;
                let style = if is_current {
                    tab_theme.active_style_or_theme()
                } else {
                    tab_theme.inactive_style_or_theme()
                };

                let name = if tab_theme.uses_name() {
                    if tab.is_trash() {
                        "Trash".to_string()
                    } else {
                        let cwd = tab.nav().current_dir();
                        let shortened = path::shorten_home_path(cwd);
                        Path::new(&shortened)
                            .file_name()
                            .map(|os_str| os_str.to_string_lossy().into_owned())
                            .unwrap_or_else(|| cwd.to_string_lossy().into_owned())
                    }
                } else {
                    String::new()
                };

                let formatted = tab_theme.format_tab(i, is_current, Some(&name));

                spans.push(Span::styled(formatted, style));
                if i != self.tabs.len() - 1 {
                    spans.push(Span::raw(separator.clone()));
                }
            }
            spans
        };

        if let Some(active_tab) = self.tabs.get_mut(self.current) {
            active_tab.set_tab_line(tab_spans);
        }
    }

    fn add_tab(&mut self, mut tab: AppState, workers: &Workers, focus: Option<OsString>) -> usize {
        if self.tabs.len() >= MAX_TABS {
            return self.current;
        }

        tab.set_tab_id(self.next_tab_id);
        self.next_tab_id = self.next_tab_id.saturating_add(1);

        tab.initialize(workers, focus);
        self.current_tab_mut().transfer_shared_overlays_to(&mut tab);

        self.tabs.push(tab);
        self.current = self.tabs.len() - 1;
        self.sync_tab_line();
        self.current
    }

    fn select(&mut self, next: usize) -> bool {
        let Ok([from, to]) = self.tabs.get_disjoint_mut([self.current, next]) else {
            return false;
        };

        from.transfer_shared_overlays_to(to);
        self.current = next;
        self.sync_tab_line();
        true
    }

    fn switch(&mut self, dir: isize) -> bool {
        let n = self.tabs.len() as isize;
        if n == 0 {
            return false;
        }
        let next = ((self.current as isize + dir + n) % n) as usize;
        self.select(next)
    }

    fn set_active(&mut self, idx: usize) -> bool {
        idx < self.tabs.len() && self.select(idx)
    }

    fn close_tab(&mut self, idx: usize) -> bool {
        if idx >= self.tabs.len() {
            return !self.tabs.is_empty();
        }

        let mut removed = self.tabs.remove(idx);
        if self.tabs.is_empty() {
            self.current = 0;
            false
        } else {
            if self.current >= self.tabs.len() {
                self.current = self.tabs.len() - 1;
            }

            removed.transfer_shared_overlays_to(self.current_tab_mut());
            self.sync_tab_line();
            true
        }
    }
}

pub(crate) fn handle_tab_action(
    workers: &Workers,
    container: &mut AppContainer,
    action: TabAction,
) -> KeypressResult {
    match action {
        TabAction::New => {
            match container {
                AppContainer::Single(app_state) => {
                    let focus = app_state
                        .nav()
                        .selected_entry()
                        .map(|entry| entry.name().to_os_string());

                    let (placeholder, new_tab) =
                        match (app_state.new_current_dir(), app_state.new_current_dir()) {
                            (Ok(placeholder), Ok(new_tab)) => (placeholder, new_tab),
                            (Err(e), _) | (_, Err(e)) => {
                                app_state.push_overlay_message(
                                    format!("Could not open a new tab: {e}"),
                                    timings::message::DEFAULT,
                                );
                                return KeypressResult::Consumed;
                            }
                        };

                    let original = std::mem::replace(app_state, Box::new(placeholder));
                    *container = AppContainer::Tabs(Box::new(TabManager::new(
                        *original, new_tab, workers, focus,
                    )));

                    if let AppContainer::Tabs(tabs) = container {
                        tabs.current_tab_mut().process_timers(workers);
                    }
                }
                AppContainer::Tabs(tabs) => {
                    let focus = tabs
                        .current_tab()
                        .nav()
                        .selected_entry()
                        .map(|entry| entry.name().to_os_string());

                    let new_tab = match tabs.current_tab().new_current_dir() {
                        Ok(tab) => tab,
                        Err(e) => {
                            tabs.current_tab_mut().push_overlay_message(
                                format!("Could not open a new tab: {e}"),
                                timings::message::DEFAULT,
                            );
                            return KeypressResult::Consumed;
                        }
                    };

                    tabs.add_tab(new_tab, workers, focus);
                }
            }
            KeypressResult::Consumed
        }
        TabAction::Close => {
            match container {
                AppContainer::Single(_) => {}
                AppContainer::Tabs(tab) => {
                    tab.close_tab(tab.current);
                    if tab.len() == 1 {
                        let last = tab.tabs.remove(0);
                        *container = AppContainer::Single(Box::new(last));
                    } else if tab.is_empty() {
                        return KeypressResult::Quit;
                    }

                    activate_tab(container.current_mut(), workers);
                }
            }
            KeypressResult::Consumed
        }
        TabAction::Next | TabAction::Prev | TabAction::Switch(_) => {
            if let AppContainer::Tabs(tabs) = container {
                let switched = match action {
                    TabAction::Next => tabs.switch(1),
                    TabAction::Prev => tabs.switch(-1),
                    TabAction::Switch(n) => tabs.set_active(n as usize - 1),
                    _ => unreachable!(),
                };

                if switched {
                    activate_tab(tabs.current_tab_mut(), workers);
                }
            }

            KeypressResult::Consumed
        }
    }
}

pub(crate) fn handle_sort_action(container: &mut AppContainer, config: SortConfig) {
    if let AppContainer::Tabs(mgr) = container {
        for tab in &mut mgr.tabs {
            tab.apply_sort_config(config);
        }
    }
}

fn activate_tab(tab: &mut AppState, workers: &Workers) {
    if tab.is_trash() {
        tab.request_trash_list(workers);
    } else if tab.nav().request_id() == 0 {
        tab.initialize(workers, None);
    } else {
        tab.request_dir_reload(workers);
        tab.request_parent_content(workers);
        tab.update_file_info_cache(workers);
    }
}
