//! User commands for runa.
//!
//! Reads the commands from the runa.toml and expands the file variables of the command arguments.

use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::path::Path;

use serde::Deserialize;

use crate::config::keys::InputKeyLists;

/// extension that {stem} drops when no extension matched
pub(crate) const LAST_EXTENSION: usize = 1;

/// File variables in editor commands: the full path, the file name, the parent directory, the file stem and an unused stem.
const FILE_VARIABLES: [&str; 5] = ["{path}", "{name}", "{dir}", "{stem}", UNUSED_STEM];
const RELATIVE_VARIABLES: [&str; 3] = ["{name}", "{stem}", UNUSED_STEM];
const UNUSED_STEM: &str = "{unused_stem}";

/// Prefix added to a file name that starts like an option so the command reads it as a path.
const OPTION_GUARD: &str = "./";

/// First bytes that make a command read an argument as an option.
/// `-` for flags, `+` for editor commands, `@` for respone files.
const OPTION_PREFIX: &[u8] = b"-+@";

/// Opens a file with the default app of the system.
#[cfg(windows)]
pub(crate) const SYSTEM_OPENER: &str = "explorer";
#[cfg(target_os = "macos")]
pub(crate) const SYSTEM_OPENER: &str = "open";
#[cfg(not(any(windows, target_os = "macos")))]
pub(crate) const SYSTEM_OPENER: &str = "xdg-open";

/// How a command runs: in the terminal, or in the background.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum RunMode {
    Terminal,
    Background { check_status: bool },
}

impl RunMode {
    /// Returns the run mode of a configured command.
    pub(crate) const fn from_terminal(terminal: bool) -> Self {
        if terminal {
            RunMode::Terminal
        } else {
            RunMode::Background { check_status: true }
        }
    }
}

/// User command configuration read from runa.toml.
#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub(crate) struct UserCommand {
    #[serde(skip)]
    name: String,
    keys: InputKeyLists,
    cmd: InputKeyLists,
    terminal: Option<bool>,
}

impl UserCommand {
    crate::getters! {
        name: &str,
    }

    pub(crate) fn keys(&self) -> &[String] {
        &self.keys.0
    }

    pub(crate) fn cmd(&self) -> &[String] {
        &self.cmd.0
    }

    pub(crate) fn run_mode(&self) -> RunMode {
        RunMode::from_terminal(self.terminal.unwrap_or(true))
    }
}

#[derive(Deserialize, Debug, Default)]
#[serde(from = "BTreeMap<String, UserCommand>")]
pub(crate) struct Commands {
    list: Vec<UserCommand>,
}

impl From<BTreeMap<String, UserCommand>> for Commands {
    fn from(map: BTreeMap<String, UserCommand>) -> Self {
        let list = map
            .into_iter()
            .map(|(name, mut command)| {
                command.name = name;
                command
            })
            .collect();
        Self { list }
    }
}

impl Commands {
    crate::getters! {
        list: &[UserCommand],
    }

    pub(crate) fn get(&self, index: usize) -> Option<&UserCommand> {
        self.list.get(index)
    }
}

pub(crate) fn extensions(name: &str) -> impl Iterator<Item = &str> {
    name.match_indices('.')
        .filter(|&(dot, _)| dot > 0)
        .map(move |(dot, _)| &name[dot + 1..])
}

pub(crate) fn expand_args(cmd: &[String], ext_parts: usize, path: &Path) -> Vec<OsString> {
    if has_file_variables(cmd) {
        return fill_file_variables(cmd, ext_parts, path);
    }

    let mut args = plain_args(cmd);
    args.push(path.as_os_str().to_owned());
    args
}

pub(crate) fn has_file_variables(cmd: &[String]) -> bool {
    cmd.get(1..)
        .unwrap_or_default()
        .iter()
        .any(|arg| FILE_VARIABLES.iter().any(|fv| arg.contains(fv)))
}

pub(crate) fn plain_args(cmd: &[String]) -> Vec<OsString> {
    cmd.get(1..)
        .unwrap_or_default()
        .iter()
        .map(OsString::from)
        .collect()
}

/// Returns the arguments after the program, with the file variables filled in.
pub(crate) fn fill_file_variables(cmd: &[String], ext_parts: usize, path: &Path) -> Vec<OsString> {
    let rest = cmd.get(1..).unwrap_or_default();

    let name = path.file_name().unwrap_or_default();
    let dir = path.parent().unwrap_or(Path::new(""));
    let stem = strip_extension(name, ext_parts);
    let unused_stem = if rest.iter().any(|arg| arg.contains(UNUSED_STEM)) {
        unused_name(dir, stem)
    } else {
        OsString::new()
    };
    let values = [path.as_os_str(), name, dir.as_os_str(), stem, &unused_stem];

    rest.iter()
        .map(|arg| {
            let expanded = expand_arg(arg, &values);
            if RELATIVE_VARIABLES.iter().any(|rv| arg.starts_with(rv)) {
                guard_option(expanded)
            } else {
                expanded
            }
        })
        .collect()
}

fn unused_name(dir: &Path, stem: &OsStr) -> OsString {
    let mut name = stem.to_owned();
    let mut counter = 1;
    while dir.join(&name).symlink_metadata().is_ok() {
        name = stem.to_owned();
        name.push(format!("_{counter}"));
        counter += 1;
    }
    name
}

/// Prexises a name that stats like an option with `./` so the command reads it as a path.
fn guard_option(name: OsString) -> OsString {
    let starts_like_option = name
        .as_encoded_bytes()
        .first()
        .is_some_and(|b| OPTION_PREFIX.contains(b));

    if !starts_like_option {
        return name;
    }

    let mut guarded = OsString::with_capacity(OPTION_GUARD.len() + name.len());
    guarded.push(OPTION_GUARD);
    guarded.push(name);
    guarded
}

/// Returns the name without its last count extension
fn strip_extension(name: &OsStr, count: usize) -> &OsStr {
    let mut stem = name;
    for _ in 0..count {
        match Path::new(stem).file_stem() {
            Some(shorter)
                if shorter.len() < stem.len()
                    && !shorter.as_encoded_bytes().iter().all(|&b| b == b'.') =>
            {
                stem = shorter
            }
            _ => break,
        }
    }

    stem
}

/// Replaces each file variable in the argument with its value. Unknown braces stay as braces.
fn expand_arg(arg: &str, values: &[&OsStr; FILE_VARIABLES.len()]) -> OsString {
    let mut expanded = OsString::with_capacity(arg.len());
    let mut rest = arg;

    while let Some(start) = rest.find('{') {
        expanded.push(&rest[..start]);
        let tail = &rest[start..];
        let found = FILE_VARIABLES
            .iter()
            .zip(values)
            .find(|(fv, _)| tail.starts_with(**fv));

        match found {
            Some((variable, value)) => {
                expanded.push(value);
                rest = &tail[variable.len()..];
            }
            None => {
                expanded.push("{");
                rest = &tail[1..];
            }
        }
    }

    expanded.push(rest);
    expanded
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cmd(args: &[&str]) -> Vec<String> {
        args.iter().map(|arg| arg.to_string()).collect()
    }

    fn os_args(args: &[&str]) -> Vec<OsString> {
        args.iter().map(OsString::from).collect()
    }

    #[test]
    fn unused_stem_skips_taken_names() -> std::io::Result<()> {
        let dir = tempfile::tempdir()?;
        std::fs::create_dir(dir.path().join("foo"))?;
        std::fs::create_dir(dir.path().join("foo_1"))?;
        let path = dir.path().join("foo.zip");

        assert_eq!(
            expand_args(&cmd(&["7z", "x", "-o{unused_stem}"]), 1, &path),
            os_args(&["x", "-ofoo_2"])
        );
        Ok(())
    }

    #[test]
    fn path_is_appended_without_file_variables() {
        let path = Path::new("/x/main.rs");
        assert_eq!(
            expand_args(&cmd(&["nvim", "-p"]), 1, path),
            os_args(&["-p", "/x/main.rs"])
        );
    }

    #[test]
    fn file_variables_replace_the_appended_path() {
        let path = Path::new("/x/release.tar.gz");
        assert_eq!(
            expand_args(&cmd(&["tar", "-xf", "{path}", "-C", "{dir}"]), 2, path),
            os_args(&["-xf", "/x/release.tar.gz", "-C", "/x"])
        );
    }

    #[test]
    fn stem_drops_the_matched_extension() {
        let path = Path::new("/x/Release.tar.gz");
        assert_eq!(
            expand_args(&cmd(&["7z", "x", "-o{stem}", "{name}"]), 2, path),
            os_args(&["x", "-oRelease", "Release.tar.gz"])
        );
    }

    #[test]
    fn stem_is_never_a_dot_directory() {
        let path = Path::new("/x/...zip");
        assert_eq!(
            expand_args(&cmd(&["7z", "x", "-o{stem}"]), 1, path),
            os_args(&["x", "-o...zip"])
        );

        let path = Path::new("/x/..tar.gz");
        assert_eq!(
            expand_args(&cmd(&["7z", "x", "-o{stem}"]), 2, path),
            os_args(&["x", "-o..tar"])
        );
    }

    #[test]
    fn names_cannot_become_options() {
        let path = Path::new("/x/-rf.rs");
        assert_eq!(
            expand_args(&cmd(&["rm", "{name}", "{stem}", "{stem}.bak"]), 1, path),
            os_args(&["./-rf.rs", "./-rf", "./-rf.bak"])
        );

        let path = Path::new("/x/+!id.md");
        assert_eq!(
            expand_args(&cmd(&["nvim", "{name}"]), 1, path),
            os_args(&["./+!id.md"])
        );

        let path = Path::new("/x/@list.txt");
        assert_eq!(
            expand_args(&cmd(&["7z", "a", "out.7z", "{name}"]), 1, path),
            os_args(&["a", "out.7z", "./@list.txt"])
        );
    }

    #[test]
    fn embedded_dash_names_stay_unchanged() {
        let path = Path::new("/x/-rf.zip");
        assert_eq!(
            expand_args(&cmd(&["7z", "x", "-o{stem}", "{path}"]), 1, path),
            os_args(&["x", "-o-rf", "/x/-rf.zip"])
        );
    }

    #[test]
    fn unknown_braces_stay_literal() {
        let path = Path::new("/x/a.txt");
        assert_eq!(
            expand_args(&cmd(&["run", "{}", "{foo}", "{path}"]), 1, path),
            os_args(&["{}", "{foo}", "/x/a.txt"])
        );
    }

    #[cfg(unix)]
    #[test]
    fn non_utf8_path_is_kept_exact() {
        use std::os::unix::ffi::OsStrExt;

        let path = Path::new(OsStr::from_bytes(b"/x/caf\xe9.zip"));
        let expected = vec![
            OsString::from("x"),
            path.as_os_str().to_owned(),
            OsString::from(OsStr::from_bytes(b"-ocaf\xe9")),
        ];

        assert_eq!(
            expand_args(&cmd(&["7z", "x", "{path}", "-o{stem}"]), 1, path),
            expected
        );
    }
}
