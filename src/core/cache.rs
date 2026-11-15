//! Cache for sharing of entries betweeen navigation changes.
//!
//! Caches an Arc slice of FileEntry and the sort_column needed
//! to share entry states between panes.

use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use dashmap::{DashMap, mapref::entry::Entry};

use crate::core::{
    FileEntry,
    listing::{DirListOptions, SortConfig},
};
use crate::utils::text::StrBuffer;

const DIR_CACHE_CAP: usize = 30;

#[derive(Clone, PartialEq, Eq, Hash)]
struct DirCacheKey {
    path: Arc<Path>,
    sort: SortConfig,
    list: DirListOptions,
}

impl DirCacheKey {
    fn new(path: &Path, sort: SortConfig, list_options: &DirListOptions) -> Self {
        Self {
            path: Arc::from(path),
            sort,
            list: list_options.clone(),
        }
    }
}

pub(crate) struct CachedDir {
    entries: Arc<[FileEntry]>,
    sort_column: Option<Arc<StrBuffer>>,
    read_at: Instant,
}

impl CachedDir {
    crate::getters! {
        entries: &Arc<[FileEntry]>,
        sort_column: &Option<Arc<StrBuffer>>,
    }
}

pub(crate) struct DirCache {
    inner: DashMap<DirCacheKey, Arc<CachedDir>>,
}

impl DirCache {
    pub(crate) fn new() -> Self {
        Self {
            inner: DashMap::new(),
        }
    }

    pub(crate) fn get(
        &self,
        path: &Path,
        sort: SortConfig,
        list_options: &DirListOptions,
    ) -> Option<Arc<CachedDir>> {
        let key = DirCacheKey::new(path, sort, list_options);
        self.inner.get(&key).map(|v| Arc::clone(v.value()))
    }

    pub(crate) fn insert_if_newer(
        &self,
        path: &Path,
        sort: SortConfig,
        list_options: &DirListOptions,
        entries: Arc<[FileEntry]>,
        sort_column: Option<Arc<StrBuffer>>,
        read_at: Instant,
    ) {
        if self.inner.len() >= DIR_CACHE_CAP {
            let to_remove = self
                .inner
                .iter()
                .min_by_key(|entry| entry.value().read_at)
                .map(|entry| entry.key().clone());
            if let Some(key) = to_remove {
                self.inner.remove(&key);
            }
        }

        let key = DirCacheKey::new(path, sort, list_options);
        let new_value = Arc::new(CachedDir {
            entries,
            sort_column,
            read_at,
        });
        match self.inner.entry(key) {
            Entry::Occupied(mut occ) => {
                if read_at >= occ.get().read_at {
                    occ.insert(new_value);
                }
            }
            Entry::Vacant(vac) => {
                vac.insert(new_value);
            }
        }
    }

    pub(crate) fn invalidate_path(&self, path: &Path) {
        self.inner.retain(|key, _| &*key.path != path);
    }
}
