//! Editor configuration options for runa
//!
//! This module defines the editor configuration options which are read from the runa.toml
//! configuration file.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use serde::Deserialize;

use crate::config::commands::{self, LAST_EXTENSION, OutputMode, RunMode};
use crate::config::keys::InputKeyLists;

/// An editor command and whether it takes over the terminal.
#[derive(Debug)]
struct EditorEntry {
    args: Box<[String]>,
    run_mode: RunMode,
}

/// The table form of an editor command: `{ cmd = [...], terminal = false }`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EditorTable {
    cmd: InputKeyLists,
    terminal: Option<bool>,
    output: Option<OutputMode>,
}

impl<'de> Deserialize<'de> for EditorEntry {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Entry {
            Command(InputKeyLists),
            Table(EditorTable),
        }

        Ok(match Entry::deserialize(deserializer)? {
            Entry::Command(cmd) => EditorEntry {
                args: cmd.0,
                run_mode: RunMode::Terminal,
            },
            Entry::Table(table) => EditorEntry {
                args: table.cmd.0,
                run_mode: RunMode::resolve(table.terminal, table.output)
                    .map_err(serde::de::Error::custom)?,
            },
        })
    }
}

/// Editor configuration options
#[derive(Deserialize, Debug)]
#[serde(default)]
pub(crate) struct Editor {
    default: EditorEntry,
    ext: HashMap<String, EditorEntry>,
    filename: HashMap<String, EditorEntry>,
    system: HashSet<String>,
}

/// Public methods for accessing editor configuration options
impl Editor {
    /// Returns the command that opens the path
    /// Checks the file name first then the extension from longest to shorted, then default.
    pub(crate) fn cmd(&self, path: &Path) -> EditorCommand<'_> {
        let Some(name) = path.file_name().map(|name| name.to_string_lossy()) else {
            return EditorCommand::program(&self.default, LAST_EXTENSION);
        };

        if let Some(entry) = self.filename.get(name.as_ref()) {
            return EditorCommand::program(entry, LAST_EXTENSION);
        }

        let lower = name.to_lowercase();
        if let Some(entry) = self.filename.get(&lower) {
            return EditorCommand::program(entry, LAST_EXTENSION);
        }

        for ext in commands::extensions(&lower) {
            if let Some(entry) = self.ext.get(ext) {
                let ext_parts = ext.matches('.').count() + 1;
                return EditorCommand::program(entry, ext_parts);
            }
            if self.system.contains(ext) {
                return EditorCommand::System;
            }
        }

        EditorCommand::program(&self.default, LAST_EXTENSION)
    }
}

/// Default editor configuration options
impl Default for Editor {
    fn default() -> Self {
        Editor {
            default: EditorEntry {
                args: Box::new(["nvim".into()]),
                run_mode: RunMode::Terminal,
            },
            ext: HashMap::new(),
            filename: HashMap::new(),
            system: HashSet::new(),
        }
    }
}

#[derive(Debug, PartialEq)]
pub(crate) enum EditorCommand<'a> {
    Program {
        args: &'a [String],
        run_mode: RunMode,
        ext_parts: usize,
    },
    System,
}

impl<'a> EditorCommand<'a> {
    fn program(entry: &'a EditorEntry, ext_parts: usize) -> Self {
        EditorCommand::Program {
            args: &entry.args,
            run_mode: entry.run_mode,
            ext_parts,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn editor(ext: &[(&str, &[&str])], filename: &[(&str, &[&str])]) -> Editor {
        let map = |pairs: &[(&str, &[&str])]| {
            pairs
                .iter()
                .map(|&(key, cmd)| {
                    let entry = EditorEntry {
                        args: cmd.iter().map(|arg| arg.to_string()).collect(),
                        run_mode: RunMode::Terminal,
                    };
                    (key.to_string(), entry)
                })
                .collect()
        };
        Editor {
            ext: map(ext),
            filename: map(filename),
            ..Editor::default()
        }
    }

    fn strings(args: &[&str]) -> Vec<String> {
        args.iter().map(|arg| arg.to_string()).collect()
    }

    fn program(args: &[String], terminal: bool, ext_parts: usize) -> EditorCommand<'_> {
        EditorCommand::Program {
            args,
            run_mode: if terminal {
                RunMode::Terminal
            } else {
                RunMode::Background { check_status: true }
            },
            ext_parts,
        }
    }

    #[test]
    fn longest_extension_wins() {
        let editor = editor(&[("gz", &["gunzip"]), ("tar.gz", &["tar"])], &[]);
        let (tar, gunzip) = (strings(&["tar"]), strings(&["gunzip"]));
        assert_eq!(
            editor.cmd(Path::new("/x/release.tar.gz")),
            program(&tar, true, 2)
        );
        assert_eq!(
            editor.cmd(Path::new("/x/access.log.gz")),
            program(&gunzip, true, LAST_EXTENSION)
        );
        assert_eq!(
            editor.cmd(Path::new("/x/RELEASE.TAR.GZ")),
            program(&tar, true, 2)
        );
    }

    #[test]
    fn hidden_file_has_no_extension() {
        let editor = editor(&[("bashrc", &["code"])], &[]);
        let nvim = strings(&["nvim"]);
        assert_eq!(
            editor.cmd(Path::new("/x/.bashrc")),
            program(&nvim, true, LAST_EXTENSION)
        );
    }

    #[test]
    fn filename_beats_extension() {
        let editor = editor(&[("toml", &["code"])], &[("Cargo.toml", &["vim"])]);
        let (vim, code) = (strings(&["vim"]), strings(&["code"]));
        assert_eq!(
            editor.cmd(Path::new("/x/Cargo.toml")),
            program(&vim, true, LAST_EXTENSION)
        );
        assert_eq!(
            editor.cmd(Path::new("/x/rustfmt.toml")),
            program(&code, true, LAST_EXTENSION)
        );
    }

    #[test]
    fn system_list_opens_with_the_default_app() -> Result<(), toml::de::Error> {
        let editor: Editor = toml::from_str(
            r#"
            system = ["pdf", "tar.gz"]
            ext = { gz = "gunzip" }
            filename = { "manual.pdf" = "zathura" }
            "#,
        )?;

        let (gunzip, zathura) = (strings(&["gunzip"]), strings(&["zathura"]));
        assert_eq!(
            editor.cmd(Path::new("/x/report.pdf")),
            EditorCommand::System
        );
        assert_eq!(
            editor.cmd(Path::new("/x/release.tar.gz")),
            EditorCommand::System
        );
        assert_eq!(
            editor.cmd(Path::new("/x/access.log.gz")),
            program(&gunzip, true, LAST_EXTENSION)
        );
        assert_eq!(
            editor.cmd(Path::new("/x/manual.pdf")),
            program(&zathura, true, LAST_EXTENSION)
        );
        Ok(())
    }
}
