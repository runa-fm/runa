//! File and directory browsing logic for runa.
//!
//! Provides the FileEntry struct which is used throughout runa.

use std::ffi::{OsStr, OsString};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const DEFAULT_DIR_CAPACITY: usize = 256;

/// Represents a single entry in a directory listing
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FileEntry {
    buf: Box<str>,
    lower_len: u16,
    name_start: u16,
    raw_name: Option<Box<OsStr>>,
    symlink: Option<Box<Path>>,
    ext_offset: u16,
    flags: u8,
}

/// Keep FileEntry small, it exists once per listed entry.
#[cfg(target_pointer_width = "64")]
const _: () = assert!(std::mem::size_of::<FileEntry>() == 56);

impl FileEntry {
    // Bitflags definitions
    // These are used to set and check attributes in the flags field
    pub(crate) const IS_DIR: u8 = 1 << 0;
    pub(crate) const IS_HIDDEN: u8 = 1 << 1;
    pub(crate) const IS_SYSTEM: u8 = 1 << 2;
    pub(crate) const IS_SYMLINK: u8 = 1 << 3;
    const IS_BROKEN_SYM: u8 = 1 << 4;
    const IS_EXECUTABLE: u8 = 1 << 5;

    /// Used to set the IS_EXECUTABLE flag for files which can be executed.
    /// Used for coloring executable files in UI
    #[cfg(unix)]
    pub(super) const EXEC_FLAG: u32 = 0o111;

    pub(crate) fn new(name: OsString, flags: u8, symlink: Option<Box<Path>>) -> Self {
        let (mut buf, raw_name) = match name.into_string() {
            Ok(s) => (s, None),
            Err(raw) => (
                raw.to_string_lossy().into_owned(),
                Some(raw.into_boxed_os_str()),
            ),
        };

        let mut name_start = 0;
        if buf.is_ascii() {
            if buf.bytes().any(|b| b.is_ascii_uppercase()) {
                let len = buf.len();
                buf.reserve_exact(len);
                buf.extend_from_within(..len);
                buf[..len].make_ascii_lowercase();
                name_start = len;
            }
        } else {
            let mut lowered = buf.to_lowercase();
            if lowered != buf {
                name_start = lowered.len();
                lowered.push_str(&buf);
                buf = lowered;
            }
        }

        let lower_len = if name_start == 0 {
            buf.len()
        } else {
            name_start
        };
        let lowered = &buf[..lower_len];

        let ext_offset = lowered.rsplit_once('.').and_then(|(base, ext)| {
            if !base.is_empty() && !ext.is_empty() {
                Some((lowered.len() - ext.len()) as u16)
            } else {
                None
            }
        });

        FileEntry {
            buf: buf.into_boxed_str(),
            lower_len: lower_len as u16,
            name_start: name_start as u16,
            raw_name,
            ext_offset: ext_offset.unwrap_or(0),
            flags,
            symlink,
        }
    }

    crate::getters! {
        flags: u8,
    }

    /// Returns the name of a entry in FileEntry from the raw version of the name.
    /// If no raw version of the name exists, it creates a new OsStr for the name string.
    #[inline]
    pub(crate) fn name(&self) -> &OsStr {
        match &self.raw_name {
            Some(raw) => raw,
            None => OsStr::new(self.name_str()),
        }
    }

    /// Returns the name of a entry in FileEntry as a string.
    #[inline]
    pub(crate) fn name_str(&self) -> &str {
        &self.buf[self.name_start as usize..]
    }

    /// Returns the lowercase name of a entry in FileEntry as a string
    #[inline]
    pub(crate) fn lowered(&self) -> &str {
        &self.buf[..self.lower_len as usize]
    }

    /// Returns the lowercase name of a entry in FileEntry as bytes.
    #[inline]
    pub(crate) fn lowered_bytes(&self) -> &[u8] {
        &self.buf.as_bytes()[..self.lower_len as usize]
    }

    #[inline]
    pub(crate) fn ext(&self) -> Option<&str> {
        if self.ext_offset == 0 {
            return None;
        }

        Some(&self.lowered()[self.ext_offset as usize..])
    }

    #[inline]
    pub(crate) fn symlink(&self) -> Option<&Path> {
        self.symlink.as_deref()
    }

    #[inline]
    pub(crate) const fn is_dir(&self) -> bool {
        self.flags & Self::IS_DIR != 0
    }

    #[inline]
    pub(crate) const fn is_symlink(&self) -> bool {
        self.flags & Self::IS_SYMLINK != 0
    }

    #[inline]
    pub(crate) const fn is_broken_sym(&self) -> bool {
        self.flags & Self::IS_BROKEN_SYM != 0
    }

    #[inline]
    pub(crate) const fn is_executable(&self) -> bool {
        self.flags & Self::IS_EXECUTABLE != 0
    }

    #[cfg(windows)]
    pub(super) fn match_executable_extension(ext: &str, flags: &mut u8) {
        match ext {
            "exe" | "com" | "bat" | "cmd" | "ps1" => *flags |= Self::IS_EXECUTABLE,
            _ => {}
        }
    }
}

/// Reads the cotents of the proviced directory and returns them in a vector of FileEntry
/// # Returns
/// A Result containing a vector of FileEntry structs or an std::io::Error
pub(crate) fn browse_dir(path: &Path) -> io::Result<Vec<FileEntry>> {
    let mut entries = Vec::with_capacity(DEFAULT_DIR_CAPACITY);

    for entry in fs::read_dir(path)? {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };

        let name = entry.file_name();
        let ft = match entry.file_type() {
            Ok(ft) => ft,
            Err(_) => continue,
        };

        let mut flags = 0u8;
        if ft.is_dir() {
            flags |= FileEntry::IS_DIR;
        }
        if ft.is_symlink() {
            flags |= FileEntry::IS_SYMLINK;
        }

        let mut path_cache: Option<PathBuf> = None;

        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;
            use std::os::unix::fs::PermissionsExt;

            let md_res = if (flags & FileEntry::IS_SYMLINK) != 0 {
                fs::metadata(path_cache.get_or_insert_with(|| entry.path()))
            } else {
                entry.metadata()
            };

            if let Ok(md) = md_res {
                if md.is_dir() {
                    flags |= FileEntry::IS_DIR;
                }

                if md.permissions().mode() & FileEntry::EXEC_FLAG != 0 {
                    flags |= FileEntry::IS_EXECUTABLE;
                }
            } else if path_cache.is_some() {
                flags |= FileEntry::IS_BROKEN_SYM;
            }

            if name.as_bytes().first() == Some(&b'.') {
                flags |= FileEntry::IS_HIDDEN;
            }
        }

        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if let Ok(md) = entry.metadata() {
                let attrs = md.file_attributes();

                if attrs & 0x2 != 0 {
                    flags |= FileEntry::IS_HIDDEN;
                }
                if attrs & 0x4 != 0 {
                    flags |= FileEntry::IS_SYSTEM;
                }

                if (flags & FileEntry::IS_SYMLINK) != 0 {
                    let p = path_cache.get_or_insert_with(|| entry.path());
                    match fs::metadata(p) {
                        Ok(target_md) => {
                            if target_md.is_dir() {
                                flags |= FileEntry::IS_DIR;
                            }
                        }
                        Err(_) => {
                            flags |= FileEntry::IS_BROKEN_SYM;
                        }
                    }
                } else if attrs & 0x10 != 0 {
                    flags |= FileEntry::IS_DIR;
                }
            }

            if ft.is_file() {
                let path = Path::new(&name);

                if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    FileEntry::match_executable_extension(ext, &mut flags);
                }
            }
        }
        let symlink = if (flags & FileEntry::IS_SYMLINK) != 0 {
            let p = path_cache.get_or_insert_with(|| entry.path());
            fs::read_link(p).ok().map(|pb| pb.into_boxed_path())
        } else {
            None
        };

        entries.push(FileEntry::new(name, flags, symlink));
    }
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_stay_identical() {
        let names = [
            "file.txt",
            "README.md",
            ".Hidden",
            "noext",
            "archive.tar.GZ",
            "Äpfel.TXT",
            "µ.md",
            "ΟΣ.md",
            "\u{212A}elvin.TXT",
            "a.",
            ".b",
        ];

        for name in names {
            let entry = FileEntry::new(OsString::from(name), 0, None);
            let lowered = name.to_lowercase();
            assert_eq!(entry.name(), OsStr::new(name));
            assert_eq!(entry.name_str(), name);
            assert_eq!(entry.lowered(), lowered);
            assert_eq!(entry.lowered_bytes(), lowered.as_bytes());
        }

        let ext = |n: &str| {
            FileEntry::new(OsString::from(n), 0, None)
                .ext()
                .map(str::to_string)
        };
        assert_eq!(ext("archive.tar.GZ").as_deref(), Some("gz"));
        assert_eq!(ext("\u{212A}elvin.TXT").as_deref(), Some("txt"));
        assert_eq!(ext("a."), None);
        assert_eq!(ext(".b"), None);
    }

    #[test]
    fn non_utf8_name_keeps_raw_name() {
        #[cfg(windows)]
        let raw = {
            use std::os::windows::ffi::OsStringExt;
            OsString::from_wide(&[0xD800, 0x41, 0x2E, 0x54])
        };

        #[cfg(unix)]
        let raw = {
            use std::os::unix::ffi::OsStringExt;
            OsString::from_vec(b"\xffA.T".to_vec())
        };

        let entry = FileEntry::new(raw.clone(), 0, None);
        assert_eq!(entry.name(), raw.as_os_str());
        assert_eq!(entry.name_str(), "\u{FFFD}A.T");
        assert_eq!(entry.lowered(), "\u{FFFD}a.t");
        assert_eq!(entry.ext(), Some("t"));
    }
}
