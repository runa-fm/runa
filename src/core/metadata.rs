//! FileMetadata struct and related functions for retrieving and formatting
//! file information for the ShowInfo overlay and the status line.
//!
//! This module defines the [FileMetadata] struct which holds relevant
//! information about a file, such as its name, size, modified time,
//! attributes, and type.
//!
//! The main entry point is [FileMetadata::new], which takes a
//! file path and returns a populated [FileMetadata] instance.

use std::fs::{self, Metadata};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::OnceLock;
use std::time::SystemTime;

use dashmap::DashMap;
use humansize::{self, DECIMAL};
use jiff::{SignedDuration, Timestamp, fmt, tz::TimeZone};

#[cfg(windows)]
const PERMS_WIDTH: usize = 5;
#[cfg(unix)]
const PERMS_WIDTH: usize = 10;

/// Cached metadata for sorting.
/// Holds system calls required info only.
#[derive(Clone, Copy)]
pub(crate) struct CachedMetaKey {
    pub(crate) size: Option<u64>,
    pub(crate) modified: Option<SystemTime>,
    pub(crate) created: Option<SystemTime>,
    pub(crate) accessed: Option<SystemTime>,
}

pub(crate) fn get_or_update_cached_meta(
    path: &Path,
    cache: &DashMap<PathBuf, CachedMetaKey>,
) -> Option<CachedMetaKey> {
    if let Some(c) = cache.get(path) {
        return Some(*c);
    }

    let md = fs::symlink_metadata(path).ok();
    let c = CachedMetaKey {
        size: md.as_ref().filter(|m| m.is_file()).map(|m| m.len()),
        modified: md.as_ref().and_then(|m| m.modified().ok()),
        created: md.as_ref().and_then(|m| m.created().ok()),
        accessed: md.as_ref().and_then(|m| m.accessed().ok()),
    };
    cache.insert(path.to_path_buf(), c);
    Some(c)
}

/// Enumerator for the filye types which are then shown inside [FileMetadata]
///
/// Hold File, Directory, Symlink and Other types.
#[derive(Debug, Clone, PartialEq)]
enum FileType {
    File,
    Directory,
    Symlink,
    Other,
}

/// Main FileMetadata struct that holds each info field for the ShowInfo overlay widget.
/// Holds name, size, modified time, attributes string, and file type.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FileMetadata {
    path: PathBuf,
    size: Option<u64>,
    modified: Option<SystemTime>,
    created: Option<SystemTime>,
    accessed: Option<SystemTime>,
    attributes: String,
    file_type: FileType,
    #[cfg(unix)]
    unix_meta: Option<unix_meta::UnixMetadata>,
}

impl FileMetadata {
    // Accessors

    /// Main file info getter used by the ShowInfo overlay functions
    /// # Returns
    /// A FileMetadata struct populated with the file's information.
    pub(crate) fn new(path: &Path) -> io::Result<FileMetadata> {
        let metadata = fs::symlink_metadata(path)?;

        let file_type = if metadata.is_file() {
            FileType::File
        } else if metadata.is_dir() {
            FileType::Directory
        } else if metadata.file_type().is_symlink() {
            FileType::Symlink
        } else {
            FileType::Other
        };

        #[cfg(unix)]
        let unix_meta = {
            use std::os::unix::fs::MetadataExt;
            Some(unix_meta::UnixMetadata {
                uid: metadata.uid(),
                gid: metadata.gid(),
            })
        };

        Ok(FileMetadata {
            path: path.to_path_buf(),
            size: if metadata.is_file() {
                Some(metadata.len())
            } else {
                None
            },
            modified: metadata.modified().ok(),
            created: metadata.created().ok(),
            accessed: metadata.accessed().ok(),
            attributes: format_attributes(&metadata),
            file_type,
            #[cfg(unix)]
            unix_meta,
        })
    }

    #[inline]
    pub(crate) fn name(&self) -> &str {
        self.path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
    }

    pub(crate) fn perms(&self) -> String {
        format!("{:width$}", self.attributes, width = PERMS_WIDTH)
    }

    pub(crate) fn size(&self) -> String {
        format_file_size(self.size, self.file_type == FileType::Directory)
    }

    pub(crate) fn modified(&self, ctx: &TimeFormatCtx) -> String {
        format_file_time(self.modified, ctx)
    }

    pub(crate) fn created(&self, ctx: &TimeFormatCtx) -> String {
        format_file_time(self.created, ctx)
    }

    pub(crate) fn accessed(&self, ctx: &TimeFormatCtx) -> String {
        format_file_time(self.accessed, ctx)
    }

    pub(crate) const fn file_type(&self) -> &'static str {
        format_file_type(&self.file_type)
    }

    #[cfg(unix)]
    fn uid(&self) -> u32 {
        self.unix_meta.as_ref().map(|m| m.uid).unwrap_or(0)
    }

    #[cfg(unix)]
    fn gid(&self) -> u32 {
        self.unix_meta.as_ref().map(|m| m.gid).unwrap_or(0)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct FileMetadataCache {
    name: Option<Arc<str>>,
    perms: Option<Arc<str>>,
    size: Option<Arc<str>>,
    modified: Option<Arc<str>>,
    created: Option<Arc<str>>,
    accessed: Option<Arc<str>>,
    file_type: Option<Arc<str>>,
    #[cfg(unix)]
    owner: Option<Arc<str>>,
    #[cfg(unix)]
    group: Option<Arc<str>>,
}

impl FileMetadataCache {
    #[rustfmt::skip]
    pub(crate) fn from(
        meta: &FileMetadata,
        date_format: &str,
        needs: &MetadataNeeds,
        now: Timestamp,
        #[cfg(unix)] ug_cache: &mut unix_meta::UserGroupCache,
    ) -> Self {
        let ctx = TimeFormatCtx::new(date_format, now);

        Self {
            name:       if needs.name { Some(Arc::from(meta.name())) } else { None },
            perms:      if needs.perms { Some(Arc::from(meta.perms())) } else { None },
            size:       if needs.size { Some(Arc::from(meta.size())) } else { None },
            modified:   if needs.modified { Some(Arc::from(meta.modified(&ctx))) } else { None },
            created:    if needs.created { Some(Arc::from(meta.created(&ctx))) } else { None },
            accessed:   if needs.accessed { Some(Arc::from(meta.accessed(&ctx))) } else { None },
            file_type:  if needs.file_type { Some(Arc::from(meta.file_type())) } else { None },
            #[cfg(unix)]
            owner:      if needs.owner { Some(ug_cache.resolve_user(meta.uid())) } else { None },
            #[cfg(unix)]
            group:      if needs.group { Some(ug_cache.resolve_group(meta.gid())) } else { None },
        }
    }

    crate::option_arc_str_getters! {
        name,
        perms,
        size,
        modified,
        created,
        accessed,
        file_type,
    }

    #[cfg(unix)]
    pub(crate) fn owner(&self) -> &str {
        self.owner.as_deref().unwrap_or_default()
    }

    #[cfg(unix)]
    pub(crate) fn group(&self) -> &str {
        self.group.as_deref().unwrap_or_default()
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct MetadataNeeds {
    pub(crate) name: bool,
    pub(crate) file_type: bool,
    pub(crate) size: bool,
    pub(crate) modified: bool,
    pub(crate) created: bool,
    pub(crate) accessed: bool,
    pub(crate) perms: bool,
    #[cfg(unix)]
    pub(crate) owner: bool,
    #[cfg(unix)]
    pub(crate) group: bool,
}

#[cfg(unix)]
pub(crate) mod unix_meta {
    use std::collections::HashMap;
    use std::sync::Arc;

    #[derive(Debug, Clone, Copy, PartialEq)]
    pub(crate) struct UnixMetadata {
        pub(crate) uid: u32,
        pub(crate) gid: u32,
    }

    #[derive(Debug, Clone)]
    pub(crate) struct UserGroupCache {
        users: HashMap<u32, Arc<str>>,
        groups: HashMap<u32, Arc<str>>,
    }

    impl UserGroupCache {
        pub(crate) fn new() -> Self {
            Self {
                users: HashMap::with_capacity(8),
                groups: HashMap::with_capacity(8),
            }
        }

        pub(crate) fn resolve_user(&mut self, uid: u32) -> Arc<str> {
            self.users
                .entry(uid)
                .or_insert_with(|| Self::fetch_user(uid))
                .clone()
        }

        pub(crate) fn resolve_group(&mut self, gid: u32) -> Arc<str> {
            self.groups
                .entry(gid)
                .or_insert_with(|| Self::fetch_group(gid))
                .clone()
        }

        fn fetch_user(uid: u32) -> Arc<str> {
            uzers::get_user_by_uid(uid)
                .map(|u| u.name().to_string_lossy().into())
                .unwrap_or_else(|| uid.to_string().into())
        }

        fn fetch_group(gid: u32) -> Arc<str> {
            uzers::get_group_by_gid(gid)
                .map(|g| g.name().to_string_lossy().into())
                .unwrap_or_else(|| gid.to_string().into())
        }
    }
}

/// Context for time formatting, holding the format string, current time,
/// and precomputed values for determining how to format timestamps based on their age.
pub(crate) struct TimeFormatCtx<'a> {
    fmt: &'a str,
    now: Timestamp,
    tz: &'static TimeZone,
    six_months: SignedDuration,
    has_year: bool,
}

impl<'a> TimeFormatCtx<'a> {
    #[inline]
    pub(crate) fn new(fmt: &'a str, now: Timestamp) -> Self {
        Self {
            fmt,
            now,
            tz: system_tz(),
            six_months: SignedDuration::from_hours(182 * 24),
            has_year: fmt
                .as_bytes()
                .windows(2)
                .any(|w| matches!(w, b"%Y" | b"%y")),
        }
    }
}

/// Formats the file size into a human-readable string.
/// # Returns
/// A string representing the formatted file size or "-" for directories/unknown sizes.
pub(crate) fn format_file_size(size: Option<u64>, is_dir: bool) -> String {
    if is_dir {
        "-".into()
    } else if let Some(sz) = size {
        humansize::format_size(sz, DECIMAL)
    } else {
        "-".to_string()
    }
}

/// Formats the file time (modified, created, or accessed) into a human-readable string based on the
/// provided format and context.
/// The context allows for dynamic formatting based on how old the timestamp is compared to the
/// current time.
pub(crate) fn format_file_time(time: Option<SystemTime>, ctx: &TimeFormatCtx) -> String {
    let Some(t) = time else {
        return "-".to_string();
    };

    let Ok(ts) = Timestamp::try_from(t) else {
        return "-".to_string();
    };

    let final_format = if !ctx.has_year && ts.duration_until(ctx.now).abs() > ctx.six_months {
        "%b %e  %Y"
    } else {
        ctx.fmt
    };
    let zoned = ts.to_zoned_datetime(ctx.tz.clone());
    match fmt::strtime::format(final_format, &zoned) {
        Ok(formatted) if !formatted.is_empty() => formatted,
        _ => fmt::strtime::format("%Y-%m-%d %H:%M", &zoned).unwrap_or_else(|_| "-".to_string()),
    }
}

/// Formatts the file attributes like Directory, Symlink, and permissions in a unix-like format
///
/// On Unix: Returns a string like 'drwxr-xr-x' etc. for directories and files.
/// On Windows: Returns a short string showing file type and attributes like:
/// (`d`, `l`, `h` for hidden, `s` for system, `a` for archive, `r` for read-only). Not all flags map 1:1 to Unix.
///
/// # Returns
/// A string representing the formatted file attributes used by FileMetadata
fn format_attributes(meta: &Metadata) -> String {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let file_type = meta.file_type();
        let first = if file_type.is_dir() {
            'd'
        } else if file_type.is_symlink() {
            'l'
        } else {
            '-'
        };
        let mode = meta.permissions().mode();
        let mut chars = [first, '-', '-', '-', '-', '-', '-', '-', '-', '-'];
        let shifts = [6, 3, 0];
        for (i, &shift) in shifts.iter().enumerate() {
            let base = 1 + i * 3;
            if (mode >> (shift + 2)) & 1u32 != 0 {
                chars[base] = 'r';
            }
            if (mode >> (shift + 1)) & 1u32 != 0 {
                chars[base + 1] = 'w';
            }
            if (mode >> shift) & 1u32 != 0 {
                chars[base + 2] = 'x';
            }
        }
        chars.iter().collect()
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        let attr = meta.file_attributes();
        let mut out = String::with_capacity(7);
        out.push(if attr & 0x10 != 0 {
            'd'
        } else if attr & 0x400 != 0 {
            'l'
        } else {
            '-'
        });

        out.push(if attr & 0x20 != 0 { 'a' } else { '-' });
        out.push(if attr & 0x01 != 0 { 'r' } else { '-' });
        out.push(if attr & 0x02 != 0 { 'h' } else { '-' });
        out.push(if attr & 0x04 != 0 { 's' } else { '-' });
        out
    }
}

/// Formats the FileType enum into a human-readable string.
/// # Returns
/// A static string representing the file type.
const fn format_file_type(file_type: &FileType) -> &'static str {
    match file_type {
        FileType::File => "File",
        FileType::Directory => "Directory",
        FileType::Symlink => "Symlink",
        FileType::Other => "Other",
    }
}

fn system_tz() -> &'static TimeZone {
    static TZ: OnceLock<TimeZone> = OnceLock::new();
    TZ.get_or_init(TimeZone::system)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};
    use std::io::Write;
    use std::path::PathBuf;
    use tempfile::TempDir;

    #[test]
    fn file_metada_basic_file() -> Result<(), Box<dyn std::error::Error>> {
        let tmp = TempDir::new()?;
        let file_path = tmp.path().join("hello.txt");
        let mut file = File::create(&file_path)?;
        writeln!(file, "abc123")?;

        let info = FileMetadata::new(&file_path)?;
        assert_eq!(&info.file_type, &FileType::File);
        assert_eq!(info.name(), "hello.txt");
        assert!(info.size.is_some());
        Ok(())
    }

    #[test]
    fn file_metada_directory() -> Result<(), Box<dyn std::error::Error>> {
        let tmp = TempDir::new()?;
        let dir_path = tmp.path().join("emptydir");
        fs::create_dir(&dir_path)?;

        let info = FileMetadata::new(&dir_path)?;
        assert_eq!(&info.file_type, &FileType::Directory);
        assert_eq!(&info.size, &None);
        Ok(())
    }

    #[test]
    fn file_metada_empty_name() {
        let info = FileMetadata {
            path: PathBuf::from(""),
            size: None,
            modified: None,
            created: None,
            accessed: None,
            attributes: "".into(),
            file_type: FileType::Other,
            #[cfg(unix)]
            unix_meta: None,
        };

        assert_eq!(info.name(), "");
        assert_eq!(info.file_type(), format_file_type(&FileType::Other));
        assert_eq!(info.size(), format_file_size(None, false));
    }

    #[test]
    fn file_metada_time_formatting_none() {
        let now = Timestamp::now();
        let fmt = "%Y-%m-%d %H:%M";
        let ctx = TimeFormatCtx::new(fmt, now);
        let info = FileMetadata {
            path: PathBuf::from("dummy"),
            size: None,
            modified: None,
            created: None,
            accessed: None,
            attributes: "".into(),
            file_type: FileType::File,
            #[cfg(unix)]
            unix_meta: None,
        };

        assert_eq!(info.modified(&ctx), format_file_time(None, &ctx));
        assert_eq!(info.created(&ctx), format_file_time(None, &ctx),);
        assert_eq!(info.accessed(&ctx), format_file_time(None, &ctx));
    }

    #[test]
    fn file_metada_perms_width() {
        let info = FileMetadata {
            path: PathBuf::from("file.txt"),
            size: None,
            modified: None,
            created: None,
            accessed: None,
            attributes: "rwx".into(),
            file_type: FileType::File,
            #[cfg(unix)]
            unix_meta: None,
        };

        let perms = info.perms();
        assert!(perms.len() >= 3);
        assert!(perms.contains("rwx"));
    }

    #[test]
    #[cfg(unix)]
    fn file_metada_unix_meta_defaults() {
        let info = FileMetadata {
            path: PathBuf::from("dummy"),
            size: None,
            modified: None,
            created: None,
            accessed: None,
            attributes: "".into(),
            file_type: FileType::File,
            unix_meta: None,
        };

        // Should fall back to 0
        assert_eq!(info.uid(), 0);
        assert_eq!(info.gid(), 0);
    }

    #[test]
    #[cfg(unix)]
    fn test_file_metada_symlink() -> Result<(), Box<dyn std::error::Error>> {
        let tmp = TempDir::new()?;
        let target_path = tmp.path().join("target.txt");
        File::create(&target_path)?;

        let link_path = tmp.path().join("link.txt");
        std::os::unix::fs::symlink(&target_path, &link_path)?;

        let info = FileMetadata::new(&link_path)?;
        assert_eq!(info.file_type, FileType::Symlink);
        assert_eq!(info.name(), "link.txt");
        Ok(())
    }

    #[test]
    #[cfg(unix)]
    fn test_unix_metadata_retrieval() -> Result<(), Box<dyn std::error::Error>> {
        let tmp = TempDir::new()?;
        let file_path = tmp.path().join("unix_test.txt");
        File::create(&file_path)?;
        let info = FileMetadata::new(&file_path)?;

        assert!(info.unix_meta.is_some());
        assert!(info.uid() > 0 || info.uid() == 0);
        Ok(())
    }
}
