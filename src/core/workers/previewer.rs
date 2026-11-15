//! Preview worker thread module.
//!
//! This module holds the implementation of the preview worker thread.

use std::thread;

use crossbeam_channel::{Receiver, Sender};

use crate::core::preview;
use crate::core::workers::{PreviewTask, WorkerResponse};

pub(super) fn start(task_rx: Receiver<PreviewTask>, res_tx: Sender<WorkerResponse>) {
    thread::spawn(move || {
        while let Ok(PreviewTask {
            path,
            max_lines,
            pane_width,
            scroll,
            preview_mode,
            request_id,
            tab_id,
        }) = task_rx.recv()
        {
            let (lines, is_eof) =
                preview::load(&path, &preview_mode, max_lines, pane_width, scroll);
            let _ = res_tx.send(WorkerResponse::PreviewLoaded {
                path,
                lines,
                scroll,
                is_eof,
                request_id,
                tab_id,
            });
        }
    });
}

#[cfg(test)]
mod tests {
    use crate::core::preview::{PreviewLines, PreviewMode};
    use crate::core::workers::test_support::{TEST_TAB_ID, TEST_TIMEOUT, bat_available};
    use crate::core::workers::{PreviewTask, WorkerResponse, Workers};

    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn preview_worker_internal() -> Result<(), Box<dyn std::error::Error>> {
        let temp = tempfile::tempdir()?;
        let preview_file = temp.path().join("preview.txt");
        std::fs::write(&preview_file, "A\nB\nC\nD\n")?;
        let workers = Workers::spawn();
        workers.preview_file_tx().send(PreviewTask {
            path: preview_file.clone(),
            max_lines: 2,
            pane_width: 40,
            scroll: 0,
            preview_mode: PreviewMode::Internal,
            request_id: 3,
            tab_id: TEST_TAB_ID,
        })?;

        match workers.response_rx().recv_timeout(TEST_TIMEOUT)? {
            WorkerResponse::PreviewLoaded {
                lines: PreviewLines::Text(lines),
                ..
            } => {
                let previewed: Vec<_> = lines.iter().take(2).map(|s| s.trim_end()).collect();
                if previewed != vec!["A", "B"] {
                    return Err(format!("Preview did not match expected, got {:?}", lines).into());
                }
            }
            r => return Err(format!("Unexpected response: {:?}", r).into()),
        }
        Ok(())
    }

    #[test]
    fn preview_fallback_on_failure() -> Result<(), Box<dyn std::error::Error>> {
        if !bat_available() {
            return Ok(());
        }

        let temp = tempdir()?;
        let file_path = temp.path().join("fallback.txt");
        fs::write(&file_path, "Standard Text Content")?;

        let workers = Workers::spawn();

        workers.preview_file_tx().send(PreviewTask {
            path: file_path,
            max_lines: 5,
            pane_width: 40,
            scroll: 0,
            preview_mode: PreviewMode::Bat { args: vec![] },
            request_id: 99,
            tab_id: TEST_TAB_ID,
        })?;

        let resp = workers.response_rx().recv_timeout(TEST_TIMEOUT)?;
        if let WorkerResponse::PreviewLoaded {
            lines: PreviewLines::Text(lines),
            ..
        } = resp
        {
            assert!(
                !lines.is_empty(),
                "Should have fallen back to internal reader"
            );
            assert_eq!(lines[0].trim(), "Standard Text Content");
        } else {
            panic!("Worker failed to provide fallback preview");
        }
        Ok(())
    }
}
