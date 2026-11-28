//! Virtual Trash view state.

use std::collections::HashSet;
use std::ffi::{OsStr, OsString};
use std::sync::Arc;

use crate::app::ListNav;
use crate::core::workers::TrashEntry;

pub(crate) struct TrashState {
    items: Arc<[TrashEntry]>,
    selected: usize,
    markers: HashSet<OsString>,
    supported: bool,
    loading: bool,
    can_exit: bool,
}

impl TrashState {
    pub(crate) fn new_loading(can_exit: bool) -> Self {
        Self {
            items: Arc::default(),
            selected: 0,
            markers: HashSet::new(),
            supported: true,
            loading: true,
            can_exit,
        }
    }

    crate::getters! {
        items: &[TrashEntry],
        markers: &HashSet<OsString>,
        selected: usize,
        supported: bool,
        loading: bool,
        can_exit: bool,
    }

    pub(crate) fn update(&mut self, items: Arc<[TrashEntry]>, supported: bool) {
        self.supported = supported;
        self.loading = false;

        let valid_ids: HashSet<&OsStr> = items.iter().map(|e| e.item.id.as_os_str()).collect();
        self.markers.retain(|id| valid_ids.contains(id.as_os_str()));

        self.items = items;
        if self.items.is_empty() {
            self.selected = 0;
        } else if self.selected >= self.items.len() {
            self.selected = self.items.len() - 1;
        }
    }

    pub(crate) fn set_loading(&mut self, loading: bool) {
        self.loading = loading;
    }

    pub(crate) fn toggle_marker(&mut self) {
        let Some(entry) = self.items.get(self.selected) else {
            return;
        };
        let id = entry.item.id.clone();
        if !self.markers.remove(&id) {
            self.markers.insert(id);
        }
    }

    pub(crate) fn toggle_marker_advance(&mut self, jump: bool) {
        self.toggle_marker();
        self.advance_selection(jump);
    }

    pub(crate) fn select_all(&mut self) {
        for entry in self.items.iter() {
            self.markers.insert(entry.item.id.clone());
        }
    }

    pub(crate) fn clear_markers(&mut self) {
        self.markers.clear();
    }

    pub(crate) fn is_marked(&self, id: &OsStr) -> bool {
        self.markers.contains(id)
    }

    pub(crate) fn selected_entry(&self) -> Option<&TrashEntry> {
        self.items.get(self.selected)
    }

    /// Returns marked items if any are marked otherwise the selected item.
    pub(crate) fn target_items(&self) -> Vec<trash::TrashItem> {
        if !self.markers.is_empty() {
            self.items
                .iter()
                .filter(|e| self.markers.contains(&e.item.id))
                .map(|e| e.item.clone())
                .collect()
        } else {
            self.selected_entry()
                .map(|e| vec![e.item.clone()])
                .unwrap_or_default()
        }
    }

    pub(crate) fn target_names(&self) -> Vec<String> {
        if !self.markers.is_empty() {
            let mut names: Vec<String> = self
                .items
                .iter()
                .filter(|e| self.markers.contains(&e.item.id))
                .map(|e| e.name_str.clone())
                .collect();
            names.sort();
            names
        } else {
            self.selected_entry()
                .map(|e| vec![e.name_str.clone()])
                .unwrap_or_default()
        }
    }
}

impl ListNav for TrashState {
    fn list_len(&self) -> usize {
        self.items.len()
    }

    fn selected_mut(&mut self) -> &mut usize {
        &mut self.selected
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::path::PathBuf;

    fn entry(id: &str, name: &str) -> TrashEntry {
        TrashEntry {
            item: trash::TrashItem {
                id: OsString::from(id),
                name: OsString::from(name),
                original_parent: PathBuf::from("/tmp"),
                time_deleted: 1_700_000_000,
            },
            size: Some(trash::TrashItemSize::Bytes(42)),
            name_str: name.to_string(),
        }
    }

    #[test]
    fn update_clamps_selection_and_drops_stale_markers() {
        let mut trash = TrashState::new_loading(true);
        trash.update(Arc::from([entry("1", "a.txt"), entry("2", "b.txt")]), true);
        trash.move_down();
        trash.toggle_marker();
        assert_eq!(trash.selected(), 1);
        assert_eq!(trash.markers().len(), 1);

        trash.update(Arc::from([entry("1", "a.txt")]), true);
        assert_eq!(trash.selected(), 0);
        assert!(trash.markers().is_empty());
    }

    #[test]
    fn target_items_prefers_markers_over_selection() {
        let mut trash = TrashState::new_loading(true);
        trash.update(Arc::from([entry("1", "a.txt"), entry("2", "b.txt")]), true);
        assert_eq!(trash.target_names(), vec!["a.txt"]);

        trash.move_down();
        trash.toggle_marker();
        trash.move_up();
        assert_eq!(trash.target_names(), vec!["b.txt"]);
        assert_eq!(trash.target_items().len(), 1);
        assert_eq!(trash.target_items()[0].id, OsString::from("2"));
    }

    #[test]
    fn navigation_wraps_and_marker_advances() {
        let mut trash = TrashState::new_loading(true);
        trash.update(
            Arc::from([
                entry("1", "a.txt"),
                entry("2", "b.txt"),
                entry("3", "c.txt"),
            ]),
            true,
        );

        trash.move_up();
        assert_eq!(trash.selected(), 2);

        trash.move_down();
        assert_eq!(trash.selected(), 0);

        trash.toggle_marker_advance(false);
        assert_eq!(trash.selected(), 1);
        assert!(trash.is_marked(OsStr::new("1")));

        trash.select_last();
        trash.toggle_marker_advance(false);
        assert_eq!(trash.selected(), 2);

        trash.toggle_marker_advance(true);
        assert_eq!(trash.selected(), 0);
    }
}
