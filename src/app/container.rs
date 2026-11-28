//! Application container to hold a single [AppState] or a [TabManager],
//! which holds multiple [AppState]s.
//!
//! It is used to route worker respones based on the correct container target,
//! either the single [AppState] or the current tab in the [TabManager].

use crate::app::AppState;
use crate::app::tab::TabManager;
use crate::core::workers::{WorkerResponse, Workers};

/// The main container enum to hold either the TabManager or a single boxed AppState to then match
/// either a single state or a tabs which then hold multiple AppStates.
pub(crate) enum AppContainer {
    Single(Box<AppState>),
    Tabs(Box<TabManager>),
}

impl AppContainer {
    pub(crate) fn from_states(mut states: Vec<AppState>, workers: &Workers) -> Option<Self> {
        if states.is_empty() {
            return None;
        }

        for (i, state) in states.iter_mut().enumerate() {
            state.set_tab_id(i);
        }

        states[0].initialize(workers, None);

        Some(if states.len() == 1 {
            Self::Single(Box::new(states.remove(0)))
        } else {
            Self::Tabs(Box::new(TabManager::from_vec(states)))
        })
    }

    /// Returns the active app state, the single one or the current tab.
    #[inline]
    pub(crate) fn current(&self) -> &AppState {
        match self {
            AppContainer::Single(app) => app,
            AppContainer::Tabs(tabs) => tabs.current_tab(),
        }
    }

    /// Returns the mutable app state, the single or the current tab.
    #[inline]
    pub(crate) fn current_mut(&mut self) -> &mut AppState {
        match self {
            AppContainer::Single(app) => app,
            AppContainer::Tabs(tabs) => tabs.current_tab_mut(),
        }
    }

    /// Routes the worker response to the correct target, either the single app state or the current tab.
    pub(crate) fn route_worker_response(&mut self, response: WorkerResponse, workers: &Workers) {
        let target = match self {
            AppContainer::Single(app) => Some(app.as_mut()),
            AppContainer::Tabs(tabs) => match response.tab_id() {
                Some(id) => tabs.iter_mut().find(|tab| tab.tab_id() == Some(id)),
                None => Some(tabs.current_tab_mut()),
            },
        };

        if let Some(app) = target {
            app.handle_worker_response(response, workers);
        }
    }
}
