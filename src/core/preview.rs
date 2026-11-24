//! Preview conent logic.
//!
//! Builds what the preview pane will show for a file.
//! Text lines from `bat` or the internal [safe_read_preview] or tree rows form an archive lister.
//! Preview worker calls [load] once.

use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::io::{BufReader, ErrorKind, Read, Seek};
use std::path::{Path, PathBuf};

use unicode_width::UnicodeWidthChar;

use crate::core::proc;
use crate::utils::{os, timings};

const DENY: &[&str] = &["a", "lib", "ilk", "h5", "zip", "gz", "tar", "pdb"];

#[rustfmt::skip]
const TEMP_EXTS: &[&str] = &[
    "crdownload", "part", "partial", "download", "opdownload",
    "aria2", "tmp", "temp", "swp", "swo", "swx",
];

/// Minimum number of lines shown in any preview
const MIN_PREVIEW_LINES: usize = 3;

/// Maximum file size allowed for preview (5gb)
const MAX_PREVIEW_SIZE: u64 = 5_000 * 1024 * 1024;

/// Number of bytes to peek from file start for header checks (eg. PNG, ZIP, etc..)
const HEADER_PEEK_BYTES: usize = 8;

/// Bytes to peek for null bytes in binary detections
const BINARY_PEEK_BYTES: usize = 1024;

/// Size of the buffer used to read files for previews.
const READ_BUFFER_SIZE: usize = 64 * 1024;

/// Maximum bytes kept per preview lines.
/// Longer lines are cut to keep previews for minified files fast.
const MAX_LINE_BYTES: usize = 1024;

/// Message shown instead of a preview for anything that is not a regular file.
const NOT_REGULAR_FILE_MSG: &str = "[Not a regular file - preview skipped]";

/// Message shown instead of a preview for binary files.
const BINARY_FILE_PREVIEW_MSG: &str = "[Binary file - preview hidden]";

/// Supported modes for a file preview
///
/// Internal uses the builin [safe_read_preview] reader.
/// Bat runs `bat` with set arguments.
/// Archive runs the set lister programm with args in a dir and shows the paths as a tree.
pub(crate) enum PreviewMode {
    Internal,
    Bat {
        args: Vec<OsString>,
    },
    Archive {
        program: OsString,
        args: Vec<OsString>,
        dir: Option<PathBuf>,
    },
}

/// Preview content for the preview pane.
#[derive(Debug, Clone)]
pub(crate) enum PreviewLines {
    Text(Vec<String>),
    Tree(Vec<TreeRow>),
}

/// Loads the preview content of a path and chooses the source by [PreviewMode].
///
/// Temp files get a notice, archives their tree rows (if enabled) and any failure falls back to
/// [PreviewMode::Internal]
///
/// # Returns
/// The preview lines and whenever the preview is at the end of content.
pub(crate) fn load(
    path: &Path,
    mode: &PreviewMode,
    max_lines: usize,
    pane_width: usize,
    scroll: usize,
) -> (PreviewLines, bool) {
    let read_lines = max_lines + 1;

    let mut lines = if is_temp_file(path) {
        PreviewLines::Text(vec![sanitize_to_exact_width(
            "[Temporary file - preview skipped]",
            pane_width,
        )])
    } else if let PreviewMode::Archive { program, args, dir } = &mode
        && let Some(rows) = archive_rows(path, program, args, dir.as_deref(), scroll, read_lines)
    {
        PreviewLines::Tree(rows)
    } else {
        PreviewLines::Text(match mode {
            PreviewMode::Internal | PreviewMode::Archive { .. } => {
                safe_read_preview(path, read_lines, pane_width, scroll)
            }
            PreviewMode::Bat { args } => {
                if !os::is_regular_file(path) || is_preview_deny(path) {
                    safe_read_preview(path, read_lines, pane_width, scroll)
                } else {
                    match proc::preview_bat(path, read_lines, args.as_slice(), scroll) {
                        // Bat preview succeeded
                        // If bat fails, fallback to internal preview
                        // If bat is not installed or returns error, we fallback to internal preview
                        Ok(lines) => lines,
                        Err(_) => safe_read_preview(path, read_lines, pane_width, scroll),
                    }
                }
            }
        })
    };

    let is_eof = match &mut lines {
        PreviewLines::Text(lines) => split_eof(lines, max_lines),
        PreviewLines::Tree(rows) => split_eof(rows, max_lines),
    };

    (lines, is_eof)
}

/// Calculating the pane widht and clean the output to the widht of the pane
/// by removing control characters, expanding tabs to 4 spaces,
/// and truncating or padding the string to fit exactly.
///
/// # Returns
/// A sanitized string that fits exactly within the specified pane width.
fn sanitize_to_exact_width(line: &str, pane_width: usize) -> String {
    let mut out = String::with_capacity(pane_width);
    let mut current_w = 0;

    for ch in line.chars() {
        if ch == '\t' {
            let space_count = 4 - (current_w % 4);
            if current_w + space_count > pane_width {
                break;
            }
            for _ in 0..space_count {
                out.push(' ');
            }
            current_w += space_count;
            continue;
        }

        if ch.is_control() {
            continue;
        }

        let ch_width = UnicodeWidthChar::width(ch).unwrap_or(0);

        if current_w + ch_width > pane_width {
            break;
        }

        out.push(ch);
        current_w += ch_width;
    }

    while current_w < pane_width {
        out.push(' ');
        current_w += 1;
    }

    out
}

/// Loads a preview for any path (directory or file), returning an error or a padded lines for
/// display.
/// large binaries/unreadable and unsupported files are replaced with a notice.
///
/// # Returns
/// A vector of strings, each representing a line from the file or directory preview.
pub(crate) fn safe_read_preview(
    path: &Path,
    max_lines: usize,
    pane_width: usize,
    scroll: usize,
) -> Vec<String> {
    let max_lines = std::cmp::max(max_lines, MIN_PREVIEW_LINES);

    if !os::is_regular_file(path) {
        return vec![sanitize_to_exact_width(NOT_REGULAR_FILE_MSG, pane_width)];
    }

    // File Read and binary Check
    match File::open(path) {
        Ok(mut file) => {
            if let Ok(metadata) = file.metadata() {
                if !metadata.is_file() {
                    return vec![sanitize_to_exact_width(NOT_REGULAR_FILE_MSG, pane_width)];
                }

                if metadata.len() > MAX_PREVIEW_SIZE {
                    return vec![sanitize_to_exact_width(
                        "[File too large for preview]",
                        pane_width,
                    )];
                }
            }

            // Peek for null bytes to detect binary files
            let mut buffer = [0u8; BINARY_PEEK_BYTES];
            let n = file.read(&mut buffer).unwrap_or(0);

            let header_len = std::cmp::min(n, HEADER_PEEK_BYTES);
            let header = &buffer[..header_len];

            if header.len() >= 5 && &header[..5] == b"%PDF-" {
                return vec![sanitize_to_exact_width(BINARY_FILE_PREVIEW_MSG, pane_width)];
            }

            if buffer[..n].contains(&0) {
                return vec![sanitize_to_exact_width(BINARY_FILE_PREVIEW_MSG, pane_width)];
            }

            // Rewind to start for full read
            let _ = file.rewind();

            let mut reader = BufReader::with_capacity(READ_BUFFER_SIZE, file);
            let mut buf = [0u8; READ_BUFFER_SIZE];

            let mut preview_lines = Vec::with_capacity(max_lines);
            let mut current_line = Vec::with_capacity(256);

            let mut line_idx = 0usize;
            let mut collected = 0usize;

            loop {
                let n = match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => n,
                    Err(_) => break,
                };

                for &b in &buf[..n] {
                    if b == b'\n' {
                        if line_idx >= scroll {
                            if let Some(&last) = current_line.last()
                                && last == b'\r'
                            {
                                current_line.pop();
                            }

                            let line = String::from_utf8_lossy(&current_line);
                            preview_lines.push(sanitize_to_exact_width(&line, pane_width));
                            collected += 1;

                            if collected >= max_lines {
                                return preview_lines;
                            }
                        }

                        current_line.clear();
                        line_idx += 1;
                    } else {
                        if current_line.len() < MAX_LINE_BYTES {
                            current_line.push(b);
                        }
                    }
                }
            }

            if !current_line.is_empty() && line_idx >= scroll && collected < max_lines {
                if current_line.last() == Some(&b'\r') {
                    current_line.pop();
                }
                let line = String::from_utf8_lossy(&current_line);
                preview_lines.push(sanitize_to_exact_width(&line, pane_width));
            }

            if preview_lines.is_empty() {
                let msg = if scroll == 0 {
                    "[Empty file]"
                } else {
                    "[End of file]"
                };
                preview_lines.push(sanitize_to_exact_width(msg, pane_width));
            }

            preview_lines
        }
        Err(e) => {
            let msg = match e.kind() {
                ErrorKind::PermissionDenied => "[Error: Permission Denied]",
                ErrorKind::NotFound => "[Error: File Not Found]",
                _ => {
                    return vec![sanitize_to_exact_width(
                        &format!("[Error reading file: {}]", e),
                        pane_width,
                    )];
                }
            };
            vec![sanitize_to_exact_width(msg, pane_width)]
        }
    }
}

/// Check for file extension to deny file previews
fn is_preview_deny(path: &Path) -> bool {
    match path.extension() {
        Some(ext) => DENY.iter().any(|&s| ext == OsStr::new(s)),
        None => false,
    }
}

/// Checks if a file is likely a temporary file based on its extension or naming patterns.
/// Will be used to exclude temp files from file previews.
fn is_temp_file(path: &Path) -> bool {
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        let ext = ext.to_ascii_lowercase();
        if TEMP_EXTS.contains(&ext.as_str()) {
            return true;
        }
    }

    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|name| name.ends_with('~') || name.starts_with("~$") || name.starts_with(".~"))
}

/// Lists the content of an archive as tree rows.
///
/// # Returns
/// The archives tree or None if its not a regular file or the lister program fails. Will fallback
/// to text.
fn archive_rows(
    path: &Path,
    program: &OsStr,
    args: &[OsString],
    dir: Option<&Path>,
    scroll: usize,
    read_lines: usize,
) -> Option<Vec<TreeRow>> {
    if !os::is_regular_file(path) {
        return None;
    }

    let timeout = timings::delay::PREVIEW_COMMAND_TIMEOUT;
    let paths = proc::preview_command(program, args, dir, scroll + read_lines, timeout).ok()?;
    let mut rows = tree_rows(&paths);
    rows.drain(..scroll.min(rows.len()));
    Some(rows)
}

/// Cuts lines to max_lines
///
/// # Returns
/// True when the lines are already at EOF.
fn split_eof<T>(lines: &mut Vec<T>, max_lines: usize) -> bool {
    let is_eof = lines.len() <= max_lines;
    lines.truncate(max_lines);
    is_eof
}

/// One row of a tree preview.
#[derive(Debug, Clone)]
pub(crate) struct TreeRow {
    buf: Box<str>,
    name_len: usize,
    depth: usize,
    is_dir: bool,
}

impl TreeRow {
    fn new(name: &str, depth: usize, is_dir: bool) -> Self {
        let ext = name
            .rsplit_once('.')
            .filter(|(base, ext)| !is_dir && !base.is_empty() && !ext.is_empty())
            .map_or("", |(_, ext)| ext);

        let mut buf = String::with_capacity(name.len() + ext.len());
        buf.push_str(name);
        buf.extend(ext.chars().flat_map(char::to_lowercase));

        Self {
            buf: buf.into_boxed_str(),
            name_len: name.len(),
            depth,
            is_dir,
        }
    }

    crate::getters! {
        depth: usize,
        is_dir: bool,
    }

    pub(crate) fn name(&self) -> &str {
        &self.buf[..self.name_len]
    }

    pub(crate) fn ext(&self) -> Option<&str> {
        let ext = &self.buf[self.name_len..];
        (!ext.is_empty()).then_some(ext)
    }
}

/// Builds the tree rows from a list of paths.
///
/// Each directory gets one row before the first entry.
/// Paths under the same directories share the rows so paths should be grouped by directory.
/// Ignores the leading `./`
pub(crate) fn tree_rows(paths: &[String]) -> Vec<TreeRow> {
    let mut rows = Vec::with_capacity(paths.len());
    let mut open: Vec<&str> = Vec::new();

    for path in paths {
        let path = path.trim_start_matches("./").trim_start_matches('/');
        if path.is_empty() || path.ends_with('/') {
            continue;
        }

        let (dirs, file) = path.rsplit_once('/').unwrap_or(("", path));
        let parts = || {
            dirs.split('/')
                .filter(|part| !part.is_empty() && *part != ".")
        };

        let shared = open
            .iter()
            .zip(parts())
            .take_while(|(open, part)| *open == part)
            .count();
        open.truncate(shared);

        for dir in parts().skip(shared) {
            rows.push(TreeRow::new(dir, open.len(), true));
            open.push(dir);
        }
        rows.push(TreeRow::new(file, open.len(), false));
    }

    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ui_sanitization_and_exact_width() {
        let pane_width = 10;

        let cases = vec![
            ("short.txt", 10),
            ("very_long_filename.txt", 10),
            ("🦀_crab.rs", 10),
            ("\t_tab", 10),
        ];

        for (input, expected_width) in cases {
            let result = sanitize_to_exact_width(input, pane_width);

            let actual_width = unicode_width::UnicodeWidthStr::width(result.as_str());

            assert_eq!(
                actual_width, expected_width,
                "Failed to produce exact width for input: '{}'. Result was: '{}' (width: {})",
                input, result, actual_width
            );

            assert!(
                !result.chars().any(|c| c.is_control() && c != ' '),
                "Result contains control characters: {:?}",
                result
            );
        }
    }

    #[test]
    fn tree_builds_folders_from_file_paths() {
        let paths: Vec<String> = ["docs/", "pkg/src/a.rs", "pkg/B.MD", "docs/c.md"]
            .iter()
            .map(|p| p.to_string())
            .collect();
        let tree = tree_rows(&paths);
        let rows: Vec<_> = tree
            .iter()
            .map(|row| (row.name(), row.depth(), row.is_dir(), row.ext()))
            .collect();

        assert_eq!(
            rows,
            [
                ("pkg", 0, true, None),
                ("src", 1, true, None),
                ("a.rs", 2, false, Some("rs")),
                ("B.MD", 1, false, Some("md")),
                ("docs", 0, true, None),
                ("c.md", 1, false, Some("md")),
            ]
        );
    }
}
