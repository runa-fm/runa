//! Directory loader worker thread.
//!
//! Handles the nav, parent, preview directory loading.

use std::sync::Arc;
use std::thread;
use std::time::Instant;

use crossbeam_channel::{Receiver, Sender};
use dashmap::DashMap;

use crate::core::{
    cache::DirCache,
    fm::{self, FileEntry},
    listing::Listing,
    workers::{LoadDirTask, WorkerResponse},
};
use crate::utils::text::StrBuffer;

/// Starts the loader worker thread, wich listens to [LoadDirTask] and sends back to [WorkerResponse]
pub(super) fn start(
    task_rx: Receiver<LoadDirTask>,
    res_tx: Sender<WorkerResponse>,
    cache: Arc<DirCache>,
) {
    thread::spawn(move || {
        while let Ok(LoadDirTask {
            path,
            focus,
            list,
            sort_config,
            sort_date_format,
            always_show,
            request,
            tab_id,
        }) = task_rx.recv()
        {
            let read_at = Instant::now();
            match fm::browse_dir(&path) {
                Ok(mut entries) => {
                    let meta_cache = DashMap::with_capacity(entries.len());
                    let listing = Listing::new(list.clone(), sort_config, always_show);
                    listing.filter_entries(&mut entries);
                    let sort_column =
                        listing.sort_entries(&path, &mut entries, &sort_date_format, &meta_cache);

                    let entries_arc: Arc<[FileEntry]> = Arc::from(entries);
                    let sort_column_arc: Option<Arc<StrBuffer>> =
                        sort_column.map(|v| Arc::new(StrBuffer::from_iter(v)));

                    cache.insert_if_newer(
                        &path,
                        sort_config,
                        &list,
                        Arc::clone(&entries_arc),
                        sort_column_arc.clone(),
                        read_at,
                    );

                    let _ = res_tx.send(WorkerResponse::DirectoryLoaded {
                        path,
                        entries: entries_arc,
                        focus,
                        sort_column: sort_column_arc,
                        request,
                        tab_id,
                    });
                }
                Err(e) => {
                    let _ = res_tx.send(WorkerResponse::DirectoryFailed {
                        path,
                        message: format!("I/O Error: {}", e),
                        request,
                        tab_id,
                    });
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use crate::core::listing::SortConfig;
    use crate::core::workers::test_support::{TEST_TAB_ID, TEST_TIMEOUT, test_list_opts};
    use crate::core::workers::{DirRequest, LoadDirTask, WorkerResponse, Workers};

    use std::collections::HashSet;
    use std::fs;
    use std::path::Path;
    use std::sync::Arc;
    use tempfile::tempdir;

    fn load_task(path: &Path, request: DirRequest) -> LoadDirTask {
        LoadDirTask {
            path: path.to_path_buf(),
            focus: None,
            list: test_list_opts(),
            sort_config: SortConfig::default(),
            sort_date_format: Arc::from(""),
            always_show: Arc::new(HashSet::new()),
            request,
            tab_id: TEST_TAB_ID,
        }
    }

    #[test]
    fn cache_keeps_newest_read() -> Result<(), Box<dyn std::error::Error>> {
        let workers = Workers::spawn();
        let temp = tempdir()?;
        fs::File::create(temp.path().join("a.txt"))?;

        workers
            .preview_dir_tx()
            .send(load_task(temp.path(), DirRequest::Preview(500)))?;
        workers.response_rx().recv_timeout(TEST_TIMEOUT)?;

        fs::File::create(temp.path().join("b.txt"))?;
        workers.send_nav(load_task(temp.path(), DirRequest::Nav(40)));
        match workers.response_rx().recv_timeout(TEST_TIMEOUT)? {
            WorkerResponse::DirectoryLoaded { entries, .. } => assert_eq!(entries.len(), 2),
            other => return Err(format!("unexpected response: {other:?}").into()),
        }

        let cached = workers
            .cache()
            .get(temp.path(), SortConfig::default(), &test_list_opts())
            .ok_or("listing is not cached")?;
        assert_eq!(cached.entries().len(), 2, "cache kept the older listing");
        Ok(())
    }
}
