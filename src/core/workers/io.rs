//! IO worker thread module.
//!
//! Handles the nav, parent, preview I/O operations.

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

/// Starts the io worker thread, wich listens to [LoadDirTask] and sends back to [WorkerResponse]
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
    use crate::core::listing::{DirListOptions, SortConfig};
    use crate::core::workers::test_support::{TEST_TAB_ID, TEST_TIMEOUT, test_list_opts};
    use crate::core::workers::{DirRequest, LoadDirTask, WorkerResponse, Workers};

    use rand::{RngExt, rng};
    use std::collections::HashSet;
    use std::fs;
    use std::path::Path;
    use std::sync::Arc;
    use std::thread;
    use std::time::{Duration, Instant};
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
        workers.send_nav_io(load_task(temp.path(), DirRequest::Nav(40)));
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

    #[test]
    fn worker_load_current_dir() -> Result<(), Box<dyn std::error::Error>> {
        let workers = Workers::spawn();
        let temp = tempdir()?;
        let task_tx = &workers.nav_io_tx;
        let res_rx = workers.response_rx();

        let temp_path = temp.path().join("test_dir");
        fs::create_dir(&temp_path)?;
        fs::File::create(temp_path.join("crab.txt"))?;

        task_tx.send(LoadDirTask {
            path: temp_path,
            focus: None,
            list: test_list_opts(),
            sort_date_format: Arc::<str>::from(""),
            sort_config: SortConfig::default(),
            always_show: Arc::new(HashSet::new()),
            request: DirRequest::Nav(1),
            tab_id: TEST_TAB_ID,
        })?;

        match res_rx.recv()? {
            WorkerResponse::DirectoryLoaded { entries, .. } => {
                assert!(!entries.is_empty(), "Current dir should not be empty");

                // Check display name width
                for entry in entries.iter().enumerate() {
                    assert!(!entry.1.name_str().is_empty());
                }
            }
            WorkerResponse::DirectoryFailed { message, .. } => panic!("Worker error: {}", message),
            _ => panic!("Unexpected worker response"),
        }
        Ok(())
    }

    #[test]
    fn worker_dir_load_requests_multithreaded() -> Result<(), Box<dyn std::error::Error>> {
        let temp_root = tempdir()?;

        let dir_a = temp_root.path().join("dir_a");
        let dir_b = temp_root.path().join("dir_b");
        fs::create_dir(&dir_a)?;
        fs::create_dir(&dir_b)?;
        fs::write(dir_a.join("file.txt"), "content")?;

        let dirs = vec![temp_root.path().to_path_buf(), dir_a, dir_b];

        let thread_count = 2;
        let requests_per_thread = 25;

        let workers = Workers::spawn();
        let task_tx = &workers.nav_io_tx;
        let res_rx = workers.response_rx();

        // Spawn threads to send requests in parallel
        let mut handles = Vec::new();
        for t in 0..thread_count {
            let task_tx = task_tx.clone();
            let dirs = dirs.clone();
            handles.push(thread::spawn(move || {
                let mut rng = rng();
                for i in 0..requests_per_thread {
                    let dir = &dirs[rng.random_range(0..dirs.len())];
                    let list = DirListOptions {
                        dirs_first: rng.random_bool(0.5),
                        show_hidden: rng.random_bool(0.5),
                        show_symlink: rng.random_bool(0.5),
                        show_system: rng.random_bool(0.5),
                        case_insensitive: rng.random_bool(0.5),
                    };
                    task_tx
                        .send(LoadDirTask {
                            path: dir.clone(),
                            focus: None,
                            list,
                            sort_config: SortConfig::default(),
                            sort_date_format: Arc::<str>::from(""),
                            always_show: Arc::new(HashSet::new()),
                            request: DirRequest::Nav((t * requests_per_thread + i) as u64),
                            tab_id: TEST_TAB_ID,
                        })
                        .expect("Couldn't send task to worker");
                    if i % 50 == 0 {
                        thread::sleep(Duration::from_millis(rng.random_range(0..10)));
                    }
                }
            }));
        }

        // Wait for all senders to finish
        for h in handles {
            if let Err(err) = h.join() {
                panic!("Thread panicked during stress test: {:?}", err);
            }
        }

        // Read responses
        let total_requests = thread_count * requests_per_thread;
        let mut valid_responses = 0;
        let timeout = Instant::now() + TEST_TIMEOUT;

        for _ in 0..total_requests {
            let remaining = timeout.saturating_duration_since(Instant::now());
            match res_rx.recv_timeout(remaining.min(Duration::from_millis(500))) {
                Ok(WorkerResponse::DirectoryLoaded { entries, .. }) => {
                    valid_responses += 1;
                    for entry in entries.iter().enumerate() {
                        let name = entry.1.name_str();

                        assert!(!name.is_empty(), "Entry name_str must not be empty.");
                        assert!(
                            !name.contains('\0'),
                            "Entry name_str must not contain null."
                        );
                    }
                }
                Ok(WorkerResponse::DirectoryFailed { message, .. }) => {
                    panic!("Worker error: {}", message)
                }
                Ok(_) => {}
                Err(_) => break,
            }
        }

        assert_eq!(
            valid_responses, total_requests,
            "Not all worker requests returned results!"
        );
        Ok(())
    }
}
