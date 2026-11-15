//! Directory listing rules.
//!
//! Sorts and filters directory entries by [DirListOptions] and [SortConfig].
//! The worker builds a [Listing] per request and applies it to the entries they read.

use std::cmp::Ordering;
use std::collections::HashSet;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::Local;
use dashmap::DashMap;

use crate::core::{
    FileEntry,
    metadata::{self, CachedMetaKey, TimeFormatCtx, format_file_size, format_file_time},
};

/// CachedMetaKey limit to prevent memory growth during sorting by metadata.
const HARD_SORT_CACHE_LIMIT: usize = 40_000;

/// Sort enum for metadata sorting.
#[derive(Clone, Copy)]
enum MetadataSortField {
    Size,
    Modified,
    Created,
    Accessed,
}

type MetaCache = DashMap<PathBuf, CachedMetaKey>;

/// Directory listing options for filtering and sorting.
#[derive(Clone, PartialEq, Eq, Hash)]
pub(crate) struct DirListOptions {
    pub(crate) dirs_first: bool,
    pub(crate) show_hidden: bool,
    pub(crate) show_symlink: bool,
    pub(crate) show_system: bool,
    pub(crate) case_insensitive: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum SortMode {
    Name,
    Modified,
    Created,
    Accessed,
    Size,
    Extension,
    Natural,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum SortOrder {
    Ascending,
    Descending,
}

impl SortOrder {
    pub(crate) fn toggle(self) -> Self {
        match self {
            SortOrder::Ascending => SortOrder::Descending,
            SortOrder::Descending => SortOrder::Ascending,
        }
    }
}

/// Configuration for sorting file entries.
/// Hold the sorting mode and order.
///
/// Main sorting data used by [Listing] and all other relevant sorting mechanisms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct SortConfig {
    mode: SortMode,
    order: SortOrder,
}

impl Default for SortConfig {
    fn default() -> Self {
        Self {
            mode: SortMode::Natural,
            order: SortOrder::Ascending,
        }
    }
}

impl From<(SortMode, SortOrder)> for SortConfig {
    fn from((mode, order): (SortMode, SortOrder)) -> Self {
        Self { mode, order }
    }
}

impl SortConfig {
    crate::getters!(
        mode: SortMode,
        order: SortOrder,
    );

    pub(crate) fn set_mode(&mut self, mode: SortMode) {
        self.mode = mode;
    }

    pub(crate) fn set_order(&mut self, order: SortOrder) {
        self.order = order
    }
}

/// Listing struct to handle sorting, filtering, and formatting of file entries
/// based on user preferences.
pub(crate) struct Listing {
    list: DirListOptions,
    sort_config: SortConfig,
    always_show: Option<Arc<HashSet<OsString>>>,
    always_show_lowercase: Option<Arc<HashSet<String>>>,
}

impl Listing {
    const PRIO_DIR: u8 = 0;
    const PRIO_FILE: u8 = 1;

    pub(crate) fn new(
        list: DirListOptions,
        sort_config: SortConfig,
        always_show: Arc<HashSet<OsString>>,
    ) -> Self {
        let (always_show, always_show_lowercase) = if always_show.is_empty() {
            (None, None)
        } else if list.case_insensitive {
            let lower = Arc::new(
                always_show
                    .iter()
                    .map(|s| s.to_string_lossy().to_lowercase())
                    .collect::<HashSet<_>>(),
            );
            (Some(always_show), Some(lower))
        } else {
            (Some(always_show), None)
        };

        Self {
            list,
            sort_config,
            always_show,
            always_show_lowercase,
        }
    }

    /// Determines the file entry priority for the sorting process based on listing options.
    #[inline]
    fn prio_for_entry(&self, entry: &FileEntry) -> u8 {
        if self.list.dirs_first && (entry.flags() & FileEntry::IS_DIR) != 0 {
            Self::PRIO_DIR
        } else {
            Self::PRIO_FILE
        }
    }

    /// Sorts the given file entries in place according to the listing's settings.
    /// Main sorting orchestrator for all sorting modes.
    ///
    /// # Returns
    /// The sort column if metadata sorting was used or None if no metadata sorting.
    pub(crate) fn sort_entries(
        &self,
        directory_path: &Path,
        entries: &mut Vec<FileEntry>,
        sort_date_format: &str,
        cache: &MetaCache,
    ) -> Option<Vec<Arc<str>>> {
        match self.sort_config.mode() {
            SortMode::Name => {
                self.sort_by_name(entries);
                None
            }
            SortMode::Natural => {
                self.sort_by_natural(entries);
                None
            }
            SortMode::Extension => {
                self.sort_by_extension(entries);
                None
            }
            SortMode::Size => Some(self.sort_by_metadata(
                directory_path,
                entries,
                MetadataSortField::Size,
                sort_date_format,
                cache,
            )),
            SortMode::Modified => Some(self.sort_by_metadata(
                directory_path,
                entries,
                MetadataSortField::Modified,
                sort_date_format,
                cache,
            )),
            SortMode::Created => Some(self.sort_by_metadata(
                directory_path,
                entries,
                MetadataSortField::Created,
                sort_date_format,
                cache,
            )),
            SortMode::Accessed => Some(self.sort_by_metadata(
                directory_path,
                entries,
                MetadataSortField::Accessed,
                sort_date_format,
                cache,
            )),
        }
    }

    /// Sorts entries by name.
    fn sort_by_name(&self, entries: &mut [FileEntry]) {
        let sort_order = self.sort_config.order();

        if !self.list.case_insensitive {
            entries.sort_unstable_by(|left_entry, right_entry| {
                let left_priority = self.prio_for_entry(left_entry);
                let right_priority = self.prio_for_entry(right_entry);

                if left_priority != right_priority {
                    return left_priority.cmp(&right_priority);
                }

                let result = left_entry.name().cmp(right_entry.name());
                if sort_order == SortOrder::Ascending {
                    result
                } else {
                    result.reverse()
                }
            });
            return;
        }

        entries.sort_unstable_by(|a, b| {
            let ord = self.prio_for_entry(a).cmp(&self.prio_for_entry(b));
            if ord != Ordering::Equal {
                return ord;
            }

            let result = a.lowered_bytes().cmp(b.lowered_bytes());

            if sort_order == SortOrder::Ascending {
                result
            } else {
                result.reverse()
            }
        });
    }

    /// Sorts entries by natural order.
    /// Uses [natural_cmp_bytes] for comparison.
    fn sort_by_natural(&self, entries: &mut [FileEntry]) {
        let sort_order = self.sort_config.order();
        let case_insensitive = self.list.case_insensitive;

        entries.sort_unstable_by(|a, b| {
            let ord = self.prio_for_entry(a).cmp(&self.prio_for_entry(b));
            if ord != Ordering::Equal {
                return ord;
            }

            let result = if case_insensitive {
                natural_cmp_bytes(a.lowered_bytes(), b.lowered_bytes(), true)
            } else {
                natural_cmp_bytes(a.lowered_bytes(), b.lowered_bytes(), false)
            };

            if sort_order == SortOrder::Ascending {
                result
            } else {
                result.reverse()
            }
        });
    }

    /// Sorts entries by their file extension.
    /// If extensions are equal, it then falls back to sorting by name.
    fn sort_by_extension(&self, entries: &mut [FileEntry]) {
        let sort_order = self.sort_config.order();
        let case_insensitive = self.list.case_insensitive;

        entries.sort_unstable_by(|a, b| {
            let ord = self.prio_for_entry(a).cmp(&self.prio_for_entry(b));
            if ord != Ordering::Equal {
                return ord;
            }

            let a_ext = a.ext().unwrap_or("");
            let b_ext = b.ext().unwrap_or("");

            let mut result = a_ext.cmp(b_ext);

            if result == Ordering::Equal {
                result = if case_insensitive {
                    a.lowered_bytes().cmp(b.lowered_bytes())
                } else {
                    a.name().cmp(b.name())
                };
            }

            if sort_order == SortOrder::Ascending {
                result
            } else {
                result.reverse()
            }
        });
    }

    /// Sort entries by file metadata (size, modified, created, accessed).
    ///
    /// # Returns
    /// The sort column that displays metadata values to the UI.
    fn sort_by_metadata(
        &self,
        directory_path: &Path,
        entries: &mut Vec<FileEntry>,
        metadata_sort_field: MetadataSortField,
        sort_date_format: &str,
        cache: &MetaCache,
    ) -> Vec<Arc<str>> {
        let now = Local::now();
        let ctx = TimeFormatCtx::new(sort_date_format, now);

        let mut keys: Vec<(u8, u128, usize)> = Vec::with_capacity(entries.len());
        let mut column: Vec<Arc<str>> = Vec::with_capacity(entries.len());

        let mut path_buffer = directory_path.to_path_buf();

        if cache.len() > HARD_SORT_CACHE_LIMIT {
            cache.clear();
        }

        for (index, file_entry) in entries.iter().enumerate() {
            let priority = self.prio_for_entry(file_entry);

            path_buffer.push(file_entry.name());

            if let Some(cached) = metadata::get_or_update_cached_meta(&path_buffer, cache) {
                let (key, display) = match metadata_sort_field {
                    MetadataSortField::Size => {
                        let val = cached.size.unwrap_or(0);
                        (
                            val as u128,
                            format_file_size(cached.size, file_entry.is_dir()),
                        )
                    }
                    MetadataSortField::Modified => (
                        system_time_to_key(cached.modified),
                        format_file_time(cached.modified, &ctx),
                    ),
                    MetadataSortField::Created => (
                        system_time_to_key(cached.created),
                        format_file_time(cached.created, &ctx),
                    ),
                    MetadataSortField::Accessed => (
                        system_time_to_key(cached.accessed),
                        format_file_time(cached.accessed, &ctx),
                    ),
                };

                keys.push((priority, key, index));
                column.push(Arc::from(display));
            } else {
                keys.push((priority, 0, index));
                column.push(Arc::from("-"));
            }

            path_buffer.pop();
        }

        let sort_order = self.sort_config.order();

        keys.sort_unstable_by(|left, right| {
            let p_ord = left.0.cmp(&right.0);
            if p_ord != Ordering::Equal {
                return p_ord;
            }

            let mut m_ord = if sort_order == SortOrder::Ascending {
                left.1.cmp(&right.1)
            } else {
                right.1.cmp(&left.1)
            };

            if m_ord == Ordering::Equal {
                let a = &entries[left.2];
                let b = &entries[right.2];
                m_ord = if self.list.case_insensitive {
                    a.lowered_bytes().cmp(b.lowered_bytes())
                } else {
                    a.name().cmp(b.name())
                };
            }
            m_ord
        });

        let mut old_entries: Vec<Option<FileEntry>> = entries.drain(..).map(Some).collect();
        let mut old_column: Vec<Option<Arc<str>>> = column.into_iter().map(Some).collect();

        let mut new_column = Vec::with_capacity(old_entries.len());

        for (_, _, idx) in &keys {
            if let Some(entry) = old_entries[*idx].take() {
                entries.push(entry);
            }
            if let Some(col_value) = old_column[*idx].take() {
                new_column.push(col_value);
            }
        }

        new_column
    }

    /// Filters the entries based on the listing options and if the entry is in the always_show set.
    pub(crate) fn filter_entries(&self, entries: &mut Vec<FileEntry>) {
        let mut hide = 0u8;
        if !self.list.show_hidden {
            hide |= FileEntry::IS_HIDDEN;
        }
        if !self.list.show_system {
            hide |= FileEntry::IS_SYSTEM;
        }
        if !self.list.show_symlink {
            hide |= FileEntry::IS_SYMLINK;
        }
        entries.retain(|e| {
            let flags = e.flags();

            if (flags & hide) != 0 {
                if self.list.case_insensitive {
                    if let Some(set) = &self.always_show_lowercase {
                        return set.contains(e.lowered());
                    }
                } else if let Some(set) = &self.always_show {
                    return set.contains(e.name());
                }
                return false;
            }
            true
        });
    }
}

/// Converts an SystemTime option to a u128 key representing the number of nanoseconds
/// since UNIX_EPOCH.
///
/// # Returns
/// 0 if the SystemTime is None or the duration since UNIX_EPOCH.
#[inline]
fn system_time_to_key(system_time: Option<SystemTime>) -> u128 {
    system_time
        .and_then(|st| st.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_nanos())
        .unwrap_or(0)
}

/// Natural comparison of two byte slices.
///
/// # Arguments
/// * a - First byte slice
/// * b - Second byte sice
/// * fold_case - Compares case-insensitiely if true, otherwise case-sensitive.
///
/// # Returns
/// The ordering of the two byte slices based on natural sorting.
fn natural_cmp_bytes(a: &[u8], b: &[u8], fold_case: bool) -> Ordering {
    let mut i = 0usize;
    let mut j = 0usize;

    while i < a.len() && j < b.len() {
        let a_is_digit = a[i].is_ascii_digit();
        let b_is_digit = b[j].is_ascii_digit();

        if a_is_digit && b_is_digit {
            let start_i = i;
            while i < a.len() && a[i].is_ascii_digit() {
                i += 1;
            }
            let start_j = j;
            while j < b.len() && b[j].is_ascii_digit() {
                j += 1;
            }

            let mut nonzero_i = start_i;
            while nonzero_i < i && a[nonzero_i] == b'0' {
                nonzero_i += 1;
            }
            let mut nonzero_j = start_j;
            while nonzero_j < j && b[nonzero_j] == b'0' {
                nonzero_j += 1;
            }

            let len_i = i - nonzero_i;
            let len_j = j - nonzero_j;

            if len_i != len_j {
                return len_i.cmp(&len_j);
            }

            for digit_offset in 0..len_i {
                let digit_i = a[nonzero_i + digit_offset];
                let digit_j = b[nonzero_j + digit_offset];
                if digit_i != digit_j {
                    return digit_i.cmp(&digit_j);
                }
            }

            let leading_zeros_i = nonzero_i - start_i;
            let leading_zeros_j = nonzero_j - start_j;
            if leading_zeros_i != leading_zeros_j {
                return leading_zeros_i.cmp(&leading_zeros_j);
            }

            continue;
        }

        let mut byte_i = a[i];
        let mut byte_j = b[j];

        if fold_case && (byte_i.is_ascii_uppercase() || byte_j.is_ascii_uppercase()) {
            byte_i = byte_i.to_ascii_lowercase();
            byte_j = byte_j.to_ascii_lowercase();
        }

        if byte_i != byte_j {
            return byte_i.cmp(&byte_j);
        }

        i += 1;
        j += 1;
    }

    a.len().cmp(&b.len())
}

/// Listing integration tests
#[cfg(test)]
mod tests {

    use super::*;
    use crate::core::fm;
    use std::fs::File;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn listing_filters_entries_by_flags() {
        let normal = FileEntry::new(OsString::from("normal.txt"), 0, None);
        let hidden = FileEntry::new(OsString::from(".hidden"), FileEntry::IS_HIDDEN, None);
        let system = FileEntry::new(OsString::from("system.sys"), FileEntry::IS_SYSTEM, None);
        let symlink = FileEntry::new(
            OsString::from("symlink"),
            FileEntry::IS_SYMLINK,
            Some(Path::new("target").to_path_buf().into_boxed_path()),
        );

        let mut entries = vec![
            normal.clone(),
            hidden.clone(),
            system.clone(),
            symlink.clone(),
        ];

        let list_hide = DirListOptions {
            dirs_first: true,
            show_hidden: false,
            show_system: false,
            show_symlink: false,
            case_insensitive: true,
        };
        let short_config = SortConfig::default();

        let fmt = Listing::new(list_hide, short_config, Arc::new(HashSet::new()));
        fmt.filter_entries(&mut entries);

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name_str(), "normal.txt");

        let mut entries = vec![
            normal.clone(),
            hidden.clone(),
            system.clone(),
            symlink.clone(),
        ];

        let list_show = DirListOptions {
            dirs_first: true,
            show_hidden: true,
            show_system: true,
            show_symlink: true,
            case_insensitive: true,
        };

        let fmt = Listing::new(list_show, short_config, Arc::new(HashSet::new()));
        fmt.filter_entries(&mut entries);
        assert_eq!(entries.len(), 4);
        assert!(entries.iter().any(|e| e.name_str() == ".hidden"));
        assert!(entries.iter().any(|e| e.name_str() == "system.sys"));
        assert!(entries.iter().any(|e| e.name_str() == "symlink"));
        assert!(entries.iter().any(|e| e.name_str() == "normal.txt"));
    }

    #[test]
    fn listing_sort_by_name() -> Result<(), Box<dyn std::error::Error>> {
        let entry1 = FileEntry::new(OsString::from("fileB.txt"), 0, None);
        let entry2 = FileEntry::new(OsString::from("fileA.txt"), 0, None);
        let entry3 = FileEntry::new(OsString::from("FileC.txt"), 0, None);
        let mut entries = vec![entry1.clone(), entry2.clone(), entry3.clone()];

        let meta_cache = DashMap::new();

        let fmt = Listing::new(
            DirListOptions {
                dirs_first: false,
                show_hidden: true,
                show_symlink: true,
                show_system: true,
                case_insensitive: false,
            },
            (SortMode::Name, SortOrder::Ascending).into(),
            Arc::new(HashSet::new()),
        );
        fmt.sort_entries(Path::new(""), &mut entries, "%b %e %H:%M", &meta_cache);
        let names: Vec<_> = entries.iter().map(|e| e.name_str()).collect();
        assert_eq!(names, vec!["FileC.txt", "fileA.txt", "fileB.txt"]);

        let mut entries = vec![entry1, entry2, entry3];
        let fmt_ci = Listing::new(
            DirListOptions {
                dirs_first: false,
                show_hidden: true,
                show_symlink: true,
                show_system: true,
                case_insensitive: true,
            },
            (SortMode::Name, SortOrder::Ascending).into(),
            Arc::new(HashSet::new()),
        );
        fmt_ci.sort_entries(Path::new(""), &mut entries, "%b %e %H:%M", &meta_cache);
        let names_ci: Vec<_> = entries.iter().map(|e| e.name_str()).collect();
        assert_eq!(names_ci, vec!["fileA.txt", "fileB.txt", "FileC.txt"]);
        Ok(())
    }

    #[test]
    fn listing_sort_by_extension() -> Result<(), Box<dyn std::error::Error>> {
        let entry1 = FileEntry::new(OsString::from("fileB.txt"), 0, None);
        let entry2 = FileEntry::new(OsString::from("fileA.md"), 0, None);
        let entry3 = FileEntry::new(OsString::from("FileC.txt"), 0, None);
        let entry4 = FileEntry::new(OsString::from("FileD.toml"), 0, None);
        let mut entries = vec![
            entry1.clone(),
            entry2.clone(),
            entry3.clone(),
            entry4.clone(),
        ];

        let meta_cache = DashMap::new();

        let fmt = Listing::new(
            DirListOptions {
                dirs_first: false,
                show_hidden: true,
                show_symlink: true,
                show_system: true,
                case_insensitive: false,
            },
            (SortMode::Extension, SortOrder::Ascending).into(),
            Arc::new(HashSet::new()),
        );
        fmt.sort_entries(Path::new(""), &mut entries, "%b %e %H:%M", &meta_cache);
        let names: Vec<_> = entries.iter().map(|e| e.name_str()).collect();
        assert_eq!(
            names,
            vec!["fileA.md", "FileD.toml", "FileC.txt", "fileB.txt"]
        );

        let mut entries = vec![entry1, entry2, entry3, entry4];
        let fmt_ci = Listing::new(
            DirListOptions {
                dirs_first: false,
                show_hidden: true,
                show_symlink: true,
                show_system: true,
                case_insensitive: true,
            },
            (SortMode::Extension, SortOrder::Ascending).into(),
            Arc::new(HashSet::new()),
        );
        fmt_ci.sort_entries(Path::new(""), &mut entries, "%b %e %H:%M", &meta_cache);
        let names_ci: Vec<_> = entries.iter().map(|e| e.name_str()).collect();
        assert_eq!(
            names_ci,
            vec!["fileA.md", "FileD.toml", "fileB.txt", "FileC.txt"]
        );
        Ok(())
    }

    #[test]
    fn listing_sort_by_natural() -> Result<(), Box<dyn std::error::Error>> {
        let entry1 = FileEntry::new(OsString::from("file10.txt"), 0, None);
        let entry2 = FileEntry::new(OsString::from("file2.txt"), 0, None);
        let entry3 = FileEntry::new(OsString::from("File1.txt"), 0, None);
        let mut entries = vec![entry1.clone(), entry2.clone(), entry3.clone()];

        let meta_cache = DashMap::new();

        let fmt = Listing::new(
            DirListOptions {
                dirs_first: false,
                show_hidden: true,
                show_symlink: true,
                show_system: true,
                case_insensitive: false,
            },
            (SortMode::Natural, SortOrder::Ascending).into(),
            Arc::new(HashSet::new()),
        );
        fmt.sort_entries(Path::new(""), &mut entries, "%b %e %H:%M", &meta_cache);
        let names: Vec<_> = entries.iter().map(|e| e.name_str()).collect();
        assert_eq!(names, vec!["File1.txt", "file2.txt", "file10.txt"]);

        let mut entries = vec![entry1, entry2, entry3];
        let fmt_ci = Listing::new(
            DirListOptions {
                dirs_first: false,
                show_hidden: true,
                show_symlink: true,
                show_system: true,
                case_insensitive: true,
            },
            (SortMode::Natural, SortOrder::Ascending).into(),
            Arc::new(HashSet::new()),
        );
        fmt_ci.sort_entries(Path::new(""), &mut entries, "%b %e %H:%M", &meta_cache);
        let names_ci: Vec<_> = entries.iter().map(|e| e.name_str()).collect();
        assert_eq!(names_ci, vec!["File1.txt", "file2.txt", "file10.txt"]);
        Ok(())
    }

    #[test]
    fn listing_sort_by_size() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = tempdir()?;
        let path1 = temp_dir.path().join("small.txt");
        let path2 = temp_dir.path().join("medium.txt");
        let path3 = temp_dir.path().join("large.txt");

        let meta_cache = DashMap::new();

        File::create(&path1)?.write_all(b"small")?;
        File::create(&path2)?.write_all(b"medium content")?;
        File::create(&path3)?.write_all(b"large content that is bigger")?;

        let mut entries = fm::browse_dir(temp_dir.path())?;
        let fmt = Listing::new(
            DirListOptions {
                dirs_first: true,
                show_hidden: true,
                show_symlink: true,
                show_system: true,
                case_insensitive: true,
            },
            (SortMode::Size, SortOrder::Ascending).into(),
            Arc::new(HashSet::new()),
        );
        fmt.sort_entries(temp_dir.path(), &mut entries, "%b %e %H:%M", &meta_cache);
        let names: Vec<_> = entries.iter().map(|e| e.name_str()).collect();
        assert_eq!(names, vec!["small.txt", "medium.txt", "large.txt"]);
        Ok(())
    }

    #[test]
    fn listing_sort_by_modified() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = tempdir()?;
        let path1 = temp_dir.path().join("old.txt");
        let path2 = temp_dir.path().join("newer.txt");
        let path3 = temp_dir.path().join("newest.txt");

        let meta_cache = DashMap::new();

        File::create(&path1)?.write_all(b"old")?;
        filetime::set_file_mtime(&path1, filetime::FileTime::from_unix_time(1000, 0))?;
        File::create(&path2)?.write_all(b"newer")?;
        filetime::set_file_mtime(&path2, filetime::FileTime::from_unix_time(2000, 0))?;
        File::create(&path3)?.write_all(b"newest")?;
        filetime::set_file_mtime(&path3, filetime::FileTime::from_unix_time(3000, 0))?;

        let mut entries = fm::browse_dir(temp_dir.path())?;
        let fmt = Listing::new(
            DirListOptions {
                dirs_first: true,
                show_hidden: true,
                show_symlink: true,
                show_system: true,
                case_insensitive: true,
            },
            (SortMode::Modified, SortOrder::Ascending).into(),
            Arc::new(HashSet::new()),
        );
        fmt.sort_entries(temp_dir.path(), &mut entries, "%b %e %H:%M", &meta_cache);
        let names: Vec<_> = entries.iter().map(|e| e.name_str()).collect();
        assert_eq!(names, vec!["old.txt", "newer.txt", "newest.txt"]);
        Ok(())
    }

    #[test]
    fn listing_sort_modified_with_dirs_first() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = tempdir()?;
        let dir_path = temp_dir.path().join("aaa_old_dir");
        let file_path = temp_dir.path().join("bbb_new_file.txt");

        let meta_cache = DashMap::new();

        std::fs::create_dir(&dir_path)?;
        filetime::set_file_mtime(&dir_path, filetime::FileTime::from_unix_time(1000, 0))?;

        File::create(&file_path)?.write_all(b"new")?;
        filetime::set_file_mtime(&file_path, filetime::FileTime::from_unix_time(5000, 0))?;

        let mut entries = fm::browse_dir(temp_dir.path())?;

        let fmt = Listing::new(
            DirListOptions {
                dirs_first: true,
                show_hidden: true,
                show_symlink: true,
                show_system: true,
                case_insensitive: true,
            },
            (SortMode::Modified, SortOrder::Descending).into(),
            Arc::new(HashSet::new()),
        );

        fmt.sort_entries(temp_dir.path(), &mut entries, "%b %e %H:%M", &meta_cache);

        let names: Vec<_> = entries.iter().map(|e| e.name_str()).collect();
        assert_eq!(names, vec!["aaa_old_dir", "bbb_new_file.txt"]);

        assert!(
            !meta_cache.is_empty(),
            "Metadata cache should not be empty after sorting"
        );

        Ok(())
    }
}
