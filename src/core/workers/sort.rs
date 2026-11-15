//! Sort worker thread module
//!
//! Module contains the implementation for the sort worker thread,
//! which is responsible for sorting the directory entries in a separate thread.

use std::sync::Arc;
use std::thread;
use std::time::Instant;

use crossbeam_channel::{Receiver, Sender};
use dashmap::DashMap;

use crate::core::{
    FileEntry, Listing,
    cache::DirCache,
    workers::{SortDirTask, WorkerResponse},
};
use crate::utils::text::StrBuffer;

pub(super) fn start(
    task_rx: Receiver<SortDirTask>,
    res_tx: Sender<WorkerResponse>,
    cache: Arc<DirCache>,
) {
    thread::spawn(move || {
        while let Ok(SortDirTask {
            path,
            entries,
            focus,
            list,
            sort_config,
            sort_date_format,
            always_show,
            request,
            tab_id,
        }) = task_rx.recv()
        {
            let mut entries_vec = entries.to_vec();
            let meta_cache = DashMap::with_capacity(entries.len());

            let listing = Listing::new(list.clone(), sort_config, always_show);
            listing.filter_entries(&mut entries_vec);
            let sort_column =
                listing.sort_entries(&path, &mut entries_vec, &sort_date_format, &meta_cache);

            let entries_arc: Arc<[FileEntry]> = Arc::from(entries_vec);
            let sort_column_arc: Option<Arc<StrBuffer>> =
                sort_column.map(|v| Arc::new(StrBuffer::from_iter(v)));

            cache.insert_if_newer(
                &path,
                sort_config,
                &list,
                Arc::clone(&entries_arc),
                sort_column_arc.clone(),
                Instant::now(),
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
    });
}
