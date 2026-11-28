//! File Operation worker module
//!
//! Implementation of the file operation worker,
//! which handles file operations such as delete, rename, create, and copy.

use std::collections::HashSet;
use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;

use crossbeam_channel::{Receiver, Sender};

use crate::core::workers::{FileOperation, TrashEntry, TrashOperation, WorkerResponse};
use crate::utils::path;

struct ActiveOpGuard(Arc<AtomicUsize>);

impl ActiveOpGuard {
    fn new(counter: Arc<AtomicUsize>) -> Self {
        counter.fetch_add(1, Ordering::SeqCst);
        Self(counter)
    }
}

impl Drop for ActiveOpGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

pub(super) fn start(
    task_rx: Receiver<FileOperation>,
    res_tx: Sender<WorkerResponse>,
    active_fileop: Arc<AtomicUsize>,
) {
    thread::spawn(move || {
        while let Ok(op) = task_rx.recv() {
            let _guard = ActiveOpGuard::new(Arc::clone(&active_fileop));

            if let FileOperation::Trash { op, tab_id } = op {
                handle_trash_op(op, tab_id, &res_tx);
                continue;
            }

            let modified_dirs = collect_modified_dirs(&op);
            let mut focus_target: Option<OsString> = None;

            let result: Result<(), String> = match op {
                FileOperation::Delete(paths, move_to_trash) => {
                    let mut op_result = Ok(());
                    for p in paths {
                        let res = if move_to_trash {
                            trash::delete(&p).map_err(|e| e.to_string())
                        } else if p.is_dir() {
                            fs::remove_dir_all(&p).map_err(|e| e.to_string())
                        } else {
                            fs::remove_file(&p).map_err(|e| e.to_string())
                        };

                        if let Err(e) = res {
                            op_result = Err(format!("{}: {}", p.display(), e));
                            break;
                        }
                    }
                    op_result
                }
                FileOperation::Rename {
                    old,
                    new,
                    overwrite,
                } => {
                    let target = new;
                    let valid_target = match (old.parent(), target.parent(), target.file_name()) {
                        (Some(old_path), Some(new_path), Some(_)) => old_path == new_path,
                        _ => false,
                    };

                    if !valid_target {
                        Err(format!(
                            "Rename failed: invalid target '{}'",
                            target.display()
                        ))
                    } else {
                        let is_case_rename = old.to_string_lossy().to_lowercase()
                            == target.to_string_lossy().to_lowercase()
                            && path::is_same_entry(&old, &target);

                        if target.exists() && !is_case_rename && !overwrite {
                            Err(format!(
                                "Rename failed: '{}' already exists",
                                target.file_name().unwrap_or_default().to_string_lossy()
                            ))
                        } else {
                            if target.exists() && !is_case_rename && overwrite {
                                if old.is_dir() && target.is_dir() {
                                    match merge_dir(&old, &target, true) {
                                        Ok(()) => {
                                            focus_target =
                                                target.file_name().map(|n| n.to_os_string());
                                            Ok(())
                                        }
                                        Err(e) => Err(format!(
                                            "Could not merge directories '{}' -> '{}': {}",
                                            old.display(),
                                            target.display(),
                                            e
                                        )),
                                    }
                                } else {
                                    let remove_res = if target.is_dir() {
                                        if let (Ok(old_canon), Ok(target_canon)) =
                                            (fs::canonicalize(&old), fs::canonicalize(&target))
                                            && old_canon.starts_with(&target_canon)
                                        {
                                            Err(io::Error::new(
                                                io::ErrorKind::InvalidInput,
                                                "cannot overwrite an parent directory",
                                            ))
                                        } else {
                                            fs::remove_dir_all(&target)
                                        }
                                    } else {
                                        fs::remove_file(&target)
                                    };

                                    if let Err(e) = remove_res {
                                        Err(format!(
                                            "Could not remove existing target before rename: {}: {}",
                                            target.display(),
                                            e
                                        ))
                                    } else {
                                        focus_target = target.file_name().map(|n| n.to_os_string());
                                        rename_with_fallback(&old, &target, old.is_dir())
                                            .map_err(|e| e.to_string())
                                    }
                                }
                            } else {
                                focus_target = target.file_name().map(|n| n.to_os_string());
                                rename_with_fallback(&old, &target, old.is_dir())
                                    .map_err(|e| e.to_string())
                            }
                        }
                    }
                }
                FileOperation::Create {
                    path,
                    is_dir,
                    overwrite,
                } => {
                    let target = if overwrite {
                        path
                    } else {
                        get_unused_path(&path)
                    };

                    if target.exists() {
                        if target.is_dir() {
                            if is_dir {
                                focus_target = target.file_name().map(|n| n.to_os_string());
                                Ok(())
                            } else {
                                Err(format!(
                                    "Create failed: '{}' is an existing directory",
                                    target.display()
                                ))
                            }
                        } else {
                            let remove_res = fs::remove_file(&target);
                            if let Err(e) = remove_res {
                                Err(format!(
                                    "Could not remove existing file before create: {}: {}",
                                    target.display(),
                                    e
                                ))
                            } else {
                                focus_target = target.file_name().map(|n| n.to_os_string());
                                if is_dir {
                                    fs::create_dir_all(&target).map_err(|e| e.to_string())
                                } else {
                                    fs::OpenOptions::new()
                                        .write(true)
                                        .create_new(true)
                                        .open(&target)
                                        .map(|_| ())
                                        .map_err(|e| e.to_string())
                                }
                            }
                        }
                    } else {
                        focus_target = target.file_name().map(|n| n.to_os_string());
                        if is_dir {
                            fs::create_dir_all(&target).map_err(|e| e.to_string())
                        } else {
                            fs::OpenOptions::new()
                                .write(true)
                                .create_new(true)
                                .open(&target)
                                .map(|_| ())
                                .map_err(|e| e.to_string())
                        }
                    }
                }
                FileOperation::Copy {
                    src,
                    dest,
                    cut,
                    focus,
                } => {
                    focus_target = focus;
                    let mut op_result = Ok(());

                    for s in src {
                        if let Some(name) = s.file_name() {
                            let target = get_unused_path(&dest.join(name));

                            if let Some(ref ft) = focus_target
                                && ft == name
                            {
                                focus_target = target.file_name().map(|n| n.to_os_string());
                            }

                            if cut {
                                if fs::rename(&s, &target).is_err() {
                                    let copy_res = if s.is_dir() {
                                        copy_recursive(&s, &target)
                                    } else {
                                        fs::copy(&s, &target).map(|_| ())
                                    };

                                    match copy_res {
                                        Ok(_) => {
                                            let remove_res = if s.is_dir() {
                                                fs::remove_dir_all(&s)
                                            } else {
                                                fs::remove_file(&s)
                                            };

                                            if let Err(err) = remove_res {
                                                op_result = Err(format!(
                                                    "Copied to destination, but could not remove source: {}",
                                                    err
                                                ));
                                                break;
                                            }
                                        }
                                        Err(e) => {
                                            op_result = Err(format!("{}: {}", s.display(), e));
                                            break;
                                        }
                                    }
                                }
                            } else {
                                let res = if s.is_dir() {
                                    copy_recursive(&s, &target)
                                } else {
                                    fs::copy(&s, &target).map(|_| ())
                                };

                                if let Err(e) = res {
                                    op_result = Err(format!("{}: {}", s.display(), e));
                                    break;
                                }
                            }
                        }
                    }
                    op_result
                }
                FileOperation::Trash { .. } => unreachable!(),
            };

            match result {
                Ok(_) => {
                    let _ = res_tx.send(WorkerResponse::OperationComplete {
                        need_reload: true,
                        focus: focus_target,
                        modified_dirs,
                    });
                }
                Err(e) => {
                    let _ = res_tx.send(WorkerResponse::Error(format!("Op Error: {}", e)));
                }
            }
        }
    });
}

fn collect_modified_dirs(op: &FileOperation) -> Vec<PathBuf> {
    let add_parent = |dirs: &mut HashSet<PathBuf>, path: &Path| {
        if let Some(parent) = path.parent() {
            dirs.insert(parent.to_path_buf());
        }
    };

    let mut dirs = HashSet::new();

    match op {
        FileOperation::Delete(paths, _) => {
            for p in paths {
                add_parent(&mut dirs, p);
            }
        }
        FileOperation::Rename { old, new, .. } => {
            add_parent(&mut dirs, old);
            add_parent(&mut dirs, new);
        }
        FileOperation::Copy { src, dest, cut, .. } => {
            dirs.insert(dest.clone());
            if *cut {
                for s in src {
                    add_parent(&mut dirs, s);
                }
            }
        }
        FileOperation::Create { path, .. } => {
            add_parent(&mut dirs, path);
        }
        FileOperation::Trash { .. } => {}
    }

    dirs.into_iter().collect()
}

/// Recursively copies files and directories from `src` to `dest`, with safety checks.
///
/// Safety checks prevent copying a directory into its own subdirectory,
/// Returns an Error if such an operation is attempted.
pub(crate) fn copy_recursive(src: &Path, dest: &Path) -> io::Result<()> {
    let src_canon = src.canonicalize()?;
    let dest_parent = dest
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Destination has no parent"))?;
    let dest_parent_canon = dest_parent.canonicalize().map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "Destination parent does not exist",
        )
    })?;
    let file_name = dest.file_name().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "Destination has no file name")
    })?;
    let dest_canon = dest_parent_canon.join(file_name);

    if dest_canon.starts_with(&src_canon) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Cannot copy a directory into its own subdirectory",
        ));
    }

    copy_recursive_inner(src, dest)
}

/// Internal helper function to perform the actual recursive copy.
/// Handles directories, files, and symbolic links appropriately.
/// This function is called by [copy_recursive] after performing safety checks.
fn copy_recursive_inner(src: &Path, dest: &Path) -> io::Result<()> {
    let meta = fs::symlink_metadata(src)?;

    if meta.is_dir() {
        let entries = fs::read_dir(src)?;
        fs::create_dir_all(dest)?;

        for entry in entries {
            let entry = entry?;
            copy_recursive_inner(&entry.path(), &dest.join(entry.file_name()))?;
        }
    } else if meta.file_type().is_symlink() {
        let target = fs::read_link(src)?;
        #[cfg(windows)]
        {
            let is_dir_target = fs::metadata(src).map(|m| m.is_dir()).unwrap_or(false);

            if is_dir_target {
                std::os::windows::fs::symlink_dir(&target, dest)?;
            } else {
                std::os::windows::fs::symlink_file(&target, dest)?;
            }
        }
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(target, dest)?;
        }
    } else {
        fs::copy(src, dest)?;
    }

    Ok(())
}

/// Finds the next available filename by appending _1, _2, etc. if the target exists
///
/// Example: "notes.txt" -> "notes_1.txt"
pub(crate) fn get_unused_path(path: &Path) -> PathBuf {
    if !path.exists() {
        return path.to_path_buf();
    }

    let parent = path.parent().unwrap_or_else(|| Path::new(""));
    let name = path.file_name().unwrap_or_default();

    let stem = Path::new(name)
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy();

    let ext = Path::new(name)
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();

    let mut counter = 1;
    loop {
        let new_name = format!("{}_{}{}", stem, counter, ext);
        let target = parent.join(new_name);
        if !target.exists() {
            return target;
        }
        counter += 1;
    }
}

pub(crate) fn merge_dir(src: &Path, dst: &Path, overwrite: bool) -> io::Result<()> {
    if !dst.exists() {
        fs::create_dir_all(dst)?;
    }

    if dst.starts_with(src) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Cannot move a directory into itself",
        ));
    }

    for entry_res in fs::read_dir(src)? {
        let entry = entry_res?;
        let name = entry.file_name();
        let src_path = entry.path();
        let dst_path = dst.join(&name);

        let file_type = entry.file_type()?;

        if file_type.is_dir() {
            if dst_path.exists() {
                let dst_is_dir = fs::symlink_metadata(&dst_path)?.is_dir();

                if dst_is_dir {
                    merge_dir(&src_path, &dst_path, overwrite)?;

                    let _ = fs::remove_dir(&src_path);
                } else {
                    if overwrite {
                        fs::remove_file(&dst_path)?;
                        rename_with_fallback(&src_path, &dst_path, true)?;
                    } else {
                        let unique = get_unused_path(&dst_path);
                        rename_with_fallback(&src_path, &unique, true)?;
                    }
                }
            } else {
                rename_with_fallback(&src_path, &dst_path, true)?;
            }
        } else {
            if dst_path.exists() {
                let dst_is_dir = fs::symlink_metadata(&dst_path)?.is_dir();

                if overwrite {
                    if dst_is_dir {
                        let unique = get_unused_path(&dst_path);
                        rename_with_fallback(&src_path, &unique, false)?;
                    } else {
                        fs::remove_file(&dst_path)?;
                        rename_with_fallback(&src_path, &dst_path, false)?;
                    }
                } else {
                    let unique = get_unused_path(&dst_path);
                    rename_with_fallback(&src_path, &unique, false)?;
                }
            } else {
                rename_with_fallback(&src_path, &dst_path, false)?;
            }
        }
    }

    let _ = fs::remove_dir(src);

    Ok(())
}

/// Helper function to safely handle EXDEV (Cross-device link) errors
/// when moving files or directories between different drives/partitions.
pub(crate) fn rename_with_fallback(src: &Path, dst: &Path, is_dir: bool) -> io::Result<()> {
    match fs::rename(src, dst) {
        Ok(_) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::CrossesDevices => {
            if is_dir {
                copy_recursive(src, dst)?;
                fs::remove_dir_all(src)
            } else {
                fs::copy(src, dst)?;
                fs::remove_file(src)
            }
        }
        Err(e) => Err(e),
    }
}

fn handle_trash_op(op: TrashOperation, tab_id: Option<usize>, res_tx: &Sender<WorkerResponse>) {
    match op {
        TrashOperation::List => send_trash_list(res_tx, tab_id),
        TrashOperation::Restore(items) => {
            let count = items.len();
            let modified_dirs: Vec<PathBuf> = items
                .iter()
                .map(|i| i.original_parent.clone())
                .collect::<HashSet<_>>()
                .into_iter()
                .collect();

            match trash_os::restore_all(items) {
                Ok(()) => {
                    let label = if count == 1 { "item" } else { "items" };
                    let _ =
                        res_tx.send(WorkerResponse::Message(format!("Restored {count} {label}")));
                    let _ = res_tx.send(WorkerResponse::OperationComplete {
                        need_reload: true,
                        focus: None,
                        modified_dirs,
                    });
                }
                Err(e) => {
                    let _ = res_tx.send(WorkerResponse::Error(format!("Restore failed: {e}")));
                }
            }
            send_trash_list(res_tx, tab_id);
        }
        TrashOperation::Purge(items) => {
            let count = items.len();
            match trash_os::purge_all(items) {
                Ok(()) => {
                    let label = if count == 1 { "item" } else { "items" };
                    let _ = res_tx.send(WorkerResponse::Message(format!(
                        "Permanently deleted {count} {label}"
                    )));
                }
                Err(e) => {
                    let _ = res_tx.send(WorkerResponse::Error(format!("Delete failed: {e}")));
                }
            }
            send_trash_list(res_tx, tab_id);
        }
        TrashOperation::Empty => match trash_os::empty() {
            Ok(()) => {
                let _ = res_tx.send(WorkerResponse::Message("Trash emptied".to_string()));
                let _ = res_tx.send(WorkerResponse::TrashLoaded {
                    items: Arc::default(),
                    supported: trash_os::SUPPORTED,
                    tab_id,
                });
            }
            Err(e) => {
                let _ = res_tx.send(WorkerResponse::Error(format!("Empty trash failed: {e}")));
                send_trash_list(res_tx, tab_id);
            }
        },
    }
}

fn send_trash_list(res_tx: &Sender<WorkerResponse>, tab_id: Option<usize>) {
    match trash_os::list() {
        Ok(entries) => {
            let _ = res_tx.send(WorkerResponse::TrashLoaded {
                items: Arc::from(entries),
                supported: trash_os::SUPPORTED,
                tab_id,
            });
        }
        Err(e) => {
            let _ = res_tx.send(WorkerResponse::Error(format!("Trash error: {e}")));
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod trash_os {
    use super::TrashEntry;

    pub(super) const SUPPORTED: bool = true;

    pub(super) fn list() -> Result<Vec<TrashEntry>, String> {
        let mut entries: Vec<TrashEntry> = trash::os_limited::list()
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(|item| {
                let size = trash::os_limited::metadata(&item).ok().map(|m| m.size);
                let name_str = item.name.to_string_lossy().into_owned();
                TrashEntry {
                    item,
                    size,
                    name_str,
                }
            })
            .collect();

        entries.sort_by(|a, b| {
            b.item
                .time_deleted
                .cmp(&a.item.time_deleted)
                .then_with(|| a.name_str.cmp(&b.name_str))
        });

        Ok(entries)
    }

    pub(super) fn restore_all(items: Vec<trash::TrashItem>) -> Result<(), String> {
        trash::os_limited::restore_all(items).map_err(|e| match e {
            trash::Error::RestoreCollision { path, .. } => {
                format!("'{}' already exists", path.display())
            }
            trash::Error::RestoreTwins { path, .. } => {
                format!("multiple items share '{}'", path.display())
            }
            other => other.to_string(),
        })
    }

    pub(super) fn purge_all(items: Vec<trash::TrashItem>) -> Result<(), String> {
        trash::os_limited::purge_all(items).map_err(|e| e.to_string())
    }

    pub(super) fn empty() -> Result<(), String> {
        trash::os_limited::list()
            .and_then(trash::os_limited::purge_all)
            .map_err(|e| e.to_string())
    }
}

#[cfg(target_os = "macos")]
mod trash_os {
    use super::TrashEntry;

    pub(super) const SUPPORTED: bool = false;

    const UNSUPPORTED: &str = "Trash management is not supported on macOS";

    pub(super) fn list() -> Result<Vec<TrashEntry>, String> {
        Ok(Vec::new())
    }

    pub(super) fn restore_all(_items: Vec<trash::TrashItem>) -> Result<(), String> {
        Err(UNSUPPORTED.to_string())
    }

    pub(super) fn purge_all(_items: Vec<trash::TrashItem>) -> Result<(), String> {
        Err(UNSUPPORTED.to_string())
    }

    pub(super) fn empty() -> Result<(), String> {
        Err(UNSUPPORTED.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::core::workers::test_support::TEST_TIMEOUT;
    use crate::core::workers::{FileOperation, WorkerResponse, Workers};
    use std::error;
    use std::fs::File;
    use tempfile::tempdir;

    #[test]
    fn fileop_worker_create_and_delete_file() -> Result<(), Box<dyn std::error::Error>> {
        let temp = tempdir()?;
        let file_path = temp.path().join("touch.txt");
        let temp_dir = temp.path().to_path_buf();
        let workers = Workers::spawn();

        workers.fileop_tx().send(FileOperation::Create {
            path: file_path.clone(),
            is_dir: false,
            overwrite: false,
        })?;

        let r = workers.response_rx().recv_timeout(TEST_TIMEOUT)?;
        match r {
            WorkerResponse::OperationComplete { modified_dirs, .. } => {
                if !modified_dirs.iter().any(|path| path == &temp_dir) {
                    return Err(
                        "Expected operation completion to include file parent as affected".into(),
                    );
                }
                if !file_path.exists() {
                    return Err("Expected file to exist after creation".into());
                }
            }
            other => return Err(format!("Unexpected response: {:?}", other).into()),
        }

        workers
            .fileop_tx()
            .send(FileOperation::Delete(vec![file_path.clone()], false))?;

        let r = workers
            .response_rx()
            .recv_timeout(std::time::Duration::from_secs(2))?;
        match r {
            WorkerResponse::OperationComplete { modified_dirs, .. } => {
                if !modified_dirs.iter().any(|path| path == &temp_dir) {
                    return Err(
                        "Expected operation completion to include file parent as affected".into(),
                    );
                }
                if file_path.exists() {
                    return Err("Expected file to not exist after deletion".into());
                }
            }
            other => return Err(format!("Unexpected response: {:?}", other).into()),
        }
        Ok(())
    }

    #[test]
    fn path_collision_increments() -> Result<(), Box<dyn error::Error>> {
        let dir = tempdir()?;
        let path = dir.path().join("data.csv");

        assert_eq!(get_unused_path(&path.clone()), path);

        File::create(&path)?;
        assert_eq!(
            get_unused_path(&path.clone()),
            dir.path().join("data_1.csv")
        );

        File::create(dir.path().join("data_1.csv"))?;
        assert_eq!(get_unused_path(&path), dir.path().join("data_2.csv"));
        Ok(())
    }

    #[test]
    fn hidden_file_collision() -> Result<(), Box<dyn error::Error>> {
        let dir = tempdir()?;
        let path = dir.path().join(".config");

        File::create(&path)?;
        // Result: .config_1
        assert_eq!(get_unused_path(&path), dir.path().join(".config_1"));
        Ok(())
    }

    #[test]
    fn get_unused_path_no_extension() -> Result<(), Box<dyn error::Error>> {
        let dir = tempdir()?;
        let folder_path = dir.path().join("my_folder");

        File::create(&folder_path)?;
        let path = get_unused_path(&folder_path);

        // Should handle files/folders without extensions correctly
        let fname = path
            .file_name()
            .ok_or("No file name in path")?
            .to_str()
            .ok_or("File name not valid UTF-8")?;
        assert_eq!(fname, "my_folder_1");
        Ok(())
    }

    #[test]
    fn copy_recursive_basic_file() -> Result<(), Box<dyn error::Error>> {
        let src_dir = tempdir()?;
        let dest_dir = tempdir()?;

        let file_path = src_dir.path().join("test.txt");
        fs::write(&file_path, "hello runa")?;

        let dest_path = dest_dir.path().join("test_copied.txt");
        copy_recursive(&file_path, &dest_path)?;

        assert!(dest_path.exists());
        assert_eq!(fs::read_to_string(dest_path)?, "hello runa");
        Ok(())
    }

    #[test]
    fn copy_recursive_directory_structure() -> Result<(), Box<dyn error::Error>> {
        let src_dir = tempdir()?;
        let dest_base = tempdir()?;
        let dest_path = dest_base.path().join("backup");

        let subdir = src_dir.path().join("subdir");
        fs::create_dir(&subdir)?;
        fs::write(subdir.join("inner.txt"), "nested data")?;

        copy_recursive(src_dir.path(), &dest_path)?;

        assert!(dest_path.join("subdir").is_dir());
        assert_eq!(
            fs::read_to_string(dest_path.join("subdir").join("inner.txt"))?,
            "nested data"
        );
        Ok(())
    }

    #[test]
    fn copy_recursive_prevention_subdir() -> Result<(), Box<dyn error::Error>> {
        let src_dir = tempdir()?;
        let src_path = src_dir.path();

        let dest_path = src_path.join("backup");

        let result = copy_recursive(src_path, &dest_path);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
        assert!(err.to_string().contains("subdirectory"));

        Ok(())
    }

    #[test]
    fn rename_rejects_dot_parent_targets() -> Result<(), Box<dyn error::Error>> {
        let parent = tempdir()?;
        let child = parent.path().join("child");
        fs::create_dir(&child)?;

        let keep = parent.path().join("keep.txt");
        let file = child.join("file.txt");
        fs::write(&keep, "keep")?;
        fs::write(&file, "data")?;

        let workers = Workers::spawn();

        for bad_name in ["..", "."] {
            workers.fileop_tx().send(FileOperation::Rename {
                old: file.clone(),
                new: file.with_file_name(bad_name),
                overwrite: true,
            })?;

            let response = workers.response_rx().recv_timeout(TEST_TIMEOUT)?;
            assert!(
                matches!(response, WorkerResponse::Error(_)),
                "Expected error for target '{}', got {:?}",
                bad_name,
                response
            );
        }

        assert!(keep.exists());
        assert!(file.exists());
        Ok(())
    }

    #[test]
    fn rename_case_collision_no_overwrite() -> Result<(), Box<dyn error::Error>> {
        let dir = tempdir()?;
        let upper = dir.path().join("Foo.txt");
        let lower = dir.path().join("foo.txt");

        fs::write(&upper, "upper")?;
        if lower.exists() {
            return Ok(());
        }
        fs::write(&lower, "lower")?;

        let workers = Workers::spawn();
        workers.fileop_tx().send(FileOperation::Rename {
            old: upper.clone(),
            new: lower.clone(),
            overwrite: false,
        })?;

        let response = workers.response_rx().recv_timeout(TEST_TIMEOUT)?;
        assert!(
            matches!(response, WorkerResponse::Error(_)),
            "Got {:?}",
            response
        );
        assert_eq!(fs::read_to_string(&upper)?, "upper");
        assert_eq!(fs::read_to_string(&lower)?, "lower");
        Ok(())
    }
}
