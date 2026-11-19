//! Path string and formatting relevant utilities.

use std::borrow::Cow;
use std::fs;
use std::io;
use std::path::{self, Component, MAIN_SEPARATOR, Path, PathBuf};

use crate::utils::os;

/// Helper to resolve the initial loaded for path string.
/// Checks if the path arg is a file and then loads the parent directory of that file.
/// Used by cli path args.
/// # Example
/// `rn ~/test.txt`
/// -> runa started at `~`
pub(crate) fn resolve_initial_dir(path_arg: &Path) -> PathBuf {
    let expaned = expand_home_path_buf(path_arg);

    let mut normalized = PathBuf::new();
    for component in expaned.components() {
        normalized.push(component);
    }
    if normalized.is_file() {
        normalized
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or(normalized)
    } else {
        normalized
    }
}

/// Helper to clean path strings by removing applied prefixes applied by caonicalized paths.
/// Mostly useful for windows.
/// On unix, it justs passes the path string back.
pub(crate) fn clean_display_path(path: &str) -> &str {
    #[cfg(windows)]
    {
        if let Some(stripped) = path
            .strip_prefix(r"\\?\")
            .or_else(|| path.strip_prefix("//?/"))
            .or_else(|| path.strip_prefix(r"\??\"))
        {
            return stripped;
        }
    }

    path
}

/// Util function to shorten home directory to ~.
/// Is used by the path_str in the ui.rs render function.
pub(crate) fn shorten_home_path<P: AsRef<Path>>(path: P) -> String {
    let path = path.as_ref();

    let home_dir = os::get_home();

    if let Some(home) = home_dir
        && let Ok(stripped) = path.strip_prefix(home)
    {
        if stripped.as_os_str().is_empty() {
            return "~".to_string();
        } else {
            return format!("~{}{}", MAIN_SEPARATOR, stripped.display());
        }
    }
    path.display().to_string()
}

/// Normalize a relative path to use forward slashes for consistency across platforms.
pub(crate) fn normalize_search_path(path: &Path) -> Cow<'_, str> {
    let rel = path.to_string_lossy();

    #[cfg(windows)]
    {
        if rel.contains('\\') {
            Cow::Owned(rel.replace('\\', "/"))
        } else {
            rel
        }
    }

    #[cfg(not(windows))]
    {
        rel
    }
}

/// Normalize separators in a given string to use forward slashes.
pub(crate) fn normalize_separators<'a>(separator: &'a str) -> Cow<'a, str> {
    if separator.contains('\\') {
        Cow::Owned(separator.replace('\\', "/"))
    } else {
        Cow::Borrowed(separator)
    }
}

/// Flatten separators by removing all '/' and '\' characters from the string.
/// This is used to create a simplified version of the path for fuzzy matching.
///
/// # Examples
/// let flat = flatten_separators("src/core/proc.rs");
/// flat = "srccoreprocrs";
pub(crate) fn flatten_separators(separator: &str) -> String {
    let mut buf = String::with_capacity(separator.len());
    for char in separator.chars() {
        if char != '/' && char != '\\' {
            buf.push(char);
        }
    }
    buf
}

/// Expands the home path (~ to `home/<user>/`) and returns the string of the path.
pub(crate) fn expand_home_path(input: &str) -> String {
    expand_home_path_buf(input).to_string_lossy().to_string()
}

/// Expands the home path and returns the PathBuf of the home path.
pub(crate) fn expand_home_path_buf<P: AsRef<Path>>(input: P) -> PathBuf {
    let home = os::get_home();
    let input_ref = input.as_ref();
    let input_str = input_ref.to_string_lossy();

    if let Some(home) = home {
        if input_str == "~" {
            return home.clone();
        }

        if let Some(rest) = input_str.strip_prefix("~/") {
            return home.join(rest);
        }

        #[cfg(windows)]
        if let Some(rest) = input_str.strip_prefix(r"~\") {
            return home.join(rest);
        }
    }

    #[cfg(windows)]
    {
        if input_str.len() == 2
            && input_str.ends_with(':')
            && let Some(first_char) = input_str.chars().next()
            && first_char.is_ascii_alphabetic()
        {
            return PathBuf::from(format!(r"{}\", input_str));
        }
    }
    input_ref.to_path_buf()
}

/// Hardened directory check to make sure passed path is actually a directory and not
/// innaccessible.
pub(crate) fn validate_path(path: &Path) -> io::Result<()> {
    let meta = fs::metadata(path)
        .map_err(|_| io::Error::new(io::ErrorKind::NotFound, "path not found"))?;

    if !meta.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "is not a directory",
        ));
    }

    fs::read_dir(path)?;

    if path.components().count() > 255 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "path depth exceeds limit",
        ));
    }

    Ok(())
}

/// Checks if the string of a entry name is a valid
/// either as correct string or as a vaild path.
///
/// # Returns
/// True if the entry name is valid and false if its invalid.
pub(crate) fn is_valid_entry_name(name: &str) -> bool {
    if name.is_empty() || name.contains('\0') || name.contains(path::is_separator) {
        return false;
    }

    let mut components = Path::new(name).components();
    matches!(components.next(), Some(Component::Normal(_))) && components.next().is_none()
}

/// Compares two paths to check if they match to an entry
///
/// # Returns
/// True if the two paths share the entry, false if not.
pub(crate) fn is_same_entry(path_a: &Path, path_b: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        match (fs::symlink_metadata(path_a), fs::symlink_metadata(path_b)) {
            (Ok(ma), Ok(mb)) => ma.dev() == mb.dev() && ma.ino() == mb.ino(),
            _ => false,
        }
    }

    #[cfg(windows)]
    {
        match (fs::canonicalize(path_a), fs::canonicalize(path_b)) {
            (Ok(ca), Ok(cb)) => ca == cb,
            _ => false,
        }
    }
}

/// Format a path type to a string for display on the UI
///
/// # Returns
/// The path in a cleaned string
pub(crate) fn format_display_path(path: &Path) -> String {
    let path_str = path.to_string_lossy();
    let cleaned = clean_display_path(&path_str);
    let path_short = shorten_home_path(cleaned);
    #[cfg(windows)]
    {
        path_short.replace('/', "\\")
    }
    #[cfg(not(windows))]
    {
        path_short
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::error;
    use tempfile::tempdir;

    #[test]
    fn home_expansion() -> Result<(), Box<dyn error::Error>> {
        let path = expand_home_path_buf("~");
        assert!(path.is_absolute());

        let path = expand_home_path_buf("~/downloads");
        assert!(path.ends_with("downloads"));
        assert!(path.is_absolute());

        Ok(())
    }

    #[test]
    #[cfg(windows)]
    fn windows_drive_normalization() -> Result<(), Box<dyn error::Error>> {
        // Test C: -> C:\
        let path = expand_home_path_buf("C:");
        assert_eq!(path.to_str().ok_or("UTF8 error")?, r"C:\");

        // Test lowercase d: -> d:\
        let path = expand_home_path_buf("d:");
        assert_eq!(path.to_str().ok_or("UTF8 error")?, r"d:\");

        let path = expand_home_path_buf(r"C:\Users");
        assert_eq!(path.to_str().ok_or("UTF8 error")?, r"C:\Users");

        let path = expand_home_path_buf(r"~\Documents");
        assert!(path.is_absolute());
        assert!(path.ends_with("Documents"));

        Ok(())
    }

    #[test]
    fn standard_paths() -> Result<(), Box<dyn error::Error>> {
        let path = expand_home_path_buf("projects/runa");
        assert_eq!(path, Path::new("projects/runa"));

        let sandbox = tempdir()?;
        let absolute_input = sandbox.path().join("my_app").join("config.toml");

        let result = expand_home_path_buf(&absolute_input);

        assert_eq!(result, absolute_input);
        assert!(result.is_absolute());

        Ok(())
    }

    #[test]
    fn valid_entry_name() {
        assert!(is_valid_entry_name("file.txt"));
        assert!(is_valid_entry_name(".hidden"));
        assert!(is_valid_entry_name("..hidden"));

        assert!(!is_valid_entry_name(""));
        assert!(!is_valid_entry_name("."));
        assert!(!is_valid_entry_name(".."));
        assert!(!is_valid_entry_name("a/b"));
        assert!(!is_valid_entry_name("foo/"));
        assert!(!is_valid_entry_name("a\0n"));
    }
}
