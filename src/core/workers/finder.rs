//! Find feature worker thread.
//!
//! Module holds the implementation of the find feature worker thread.
//! It listens for find tasks, executes them, and sends back the results.

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::thread;

use crossbeam_channel::{Receiver, Sender};

use crate::core::{
    proc,
    workers::{FindTask, WorkerResponse},
};

pub(super) fn start(task_rx: Receiver<FindTask>, res_tx: Sender<WorkerResponse>) {
    thread::spawn(move || {
        while let Ok(FindTask {
            base_dir,
            query,
            max_results,
            cancel,
            show_hidden,
            request_id,
            tab_id,
        }) = task_rx.recv()
        {
            let mut results = Vec::new();
            let _ = proc::find(
                &base_dir,
                &query,
                &mut results,
                Arc::clone(&cancel),
                max_results,
                show_hidden,
            );
            if results.len() > max_results {
                results.truncate(max_results);
            }

            if cancel.load(Ordering::Acquire) {
                continue;
            }

            let _ = res_tx.send(WorkerResponse::FindResults {
                base_dir,
                results,
                request_id,
                tab_id,
            });
        }
    });
}

#[cfg(test)]
mod tests {
    use crate::core::workers::test_support::{TEST_TAB_ID, TEST_TIMEOUT, fd_available};
    use crate::core::workers::{FindTask, WorkerResponse, Workers};

    use std::collections::HashSet;
    use std::fs::{self, File};
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;
    use std::time::Instant;
    use tempfile::tempdir;

    #[test]
    fn worker_find_pool() -> Result<(), Box<dyn std::error::Error>> {
        if !fd_available() {
            return Ok(());
        }

        let dir = tempdir()?;
        for i in 0..5 {
            File::create(dir.path().join(format!("crab_{i}.txt")))?;
        }
        File::create(dir.path().join("other.txt"))?;

        let workers = Workers::spawn();
        let find_tx = &workers.find_tx;
        let res_rx = workers.response_rx();

        let req_id = 42;
        find_tx.send(FindTask {
            base_dir: dir.path().to_path_buf(),
            query: "crab".to_string(),
            max_results: 10,
            cancel: Arc::new(AtomicBool::new(false)),
            show_hidden: false,
            request_id: req_id,
            tab_id: TEST_TAB_ID,
        })?;

        let mut got = false;
        let deadline = Instant::now() + TEST_TIMEOUT;
        let expected_files: HashSet<_> = (0..5).map(|i| format!("crab_{i}.txt")).collect();

        while Instant::now() < deadline {
            match res_rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
                Ok(WorkerResponse::FindResults {
                    results,
                    request_id,
                    ..
                }) => {
                    assert_eq!(request_id, req_id);

                    let found_files: HashSet<_> = results
                        .iter()
                        .filter_map(|r| r.path().file_name())
                        .filter_map(|os| os.to_str())
                        .filter(|name| name.contains("crab"))
                        .map(|s| s.to_string())
                        .collect();

                    for fname in &expected_files {
                        assert!(
                            found_files.contains(fname),
                            "Expected {fname:?} in results: {:?}",
                            found_files
                        );
                    }

                    for r in &results {
                        let name = r.path().file_name().unwrap().to_str().unwrap();
                        if name.contains("crab") {
                            assert!(
                                expected_files.contains(name),
                                "Unexpected crab result: {}",
                                name
                            );
                        }
                    }

                    got = true;
                    break;
                }
                Ok(_unexpected) => {
                    continue;
                }
                Err(_) => break,
            }
        }

        assert!(got, "Did not receive FindResults response in time");
        Ok(())
    }

    #[test]
    fn find_worker_sequential_execution() -> Result<(), Box<dyn std::error::Error>> {
        if !fd_available() {
            return Ok(());
        }

        let temp = tempdir()?;
        let temp_path = temp.path().to_path_buf();
        fs::File::create(temp_path.join("search_1.txt"))?;
        fs::File::create(temp_path.join("search_2.txt"))?;

        let workers = Workers::spawn();

        for i in 1..=2 {
            workers.find_tx.send(FindTask {
                base_dir: temp_path.clone(),
                query: format!("search_{i}"),
                max_results: 1,
                cancel: Arc::new(AtomicBool::new(false)),
                show_hidden: false,
                request_id: i as u64,
                tab_id: TEST_TAB_ID,
            })?;
        }

        let resp1 = workers.response_rx().recv_timeout(TEST_TIMEOUT)?;
        if let WorkerResponse::FindResults { request_id, .. } = resp1 {
            assert_eq!(request_id, 1);
        }

        let resp2 = workers.response_rx().recv_timeout(TEST_TIMEOUT)?;
        if let WorkerResponse::FindResults { request_id, .. } = resp2 {
            assert_eq!(request_id, 2);
        }

        Ok(())
    }
}
