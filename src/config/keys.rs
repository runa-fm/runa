//! Input configuration options for runa
//!
//! This module defines the input configuration options which are read from the runa.toml
//! configuration file.

use std::collections::HashMap;

use serde::Deserialize;

#[derive(Debug)]
pub(super) struct InputKeyLists(pub(super) Box<[String]>);

impl<'de> Deserialize<'de> for InputKeyLists {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum OneOrMany {
            One(String),
            Many(Box<[String]>),
        }

        Ok(InputKeyLists(match OneOrMany::deserialize(deserializer)? {
            OneOrMany::One(s) => Box::new([s]),
            OneOrMany::Many(v) => v,
        }))
    }
}

macro_rules! define_keys {
    ($($variant:ident => $method:ident = [$($default:expr),*]),+ $(,)?) => {
        #[derive(Deserialize, Debug, Hash, Eq, PartialEq)]
        #[serde(rename_all = "snake_case")]
        pub(crate) enum InputKeys {
            $($variant),+
        }

        impl Keys {
            $(
                #[inline]
                pub(crate) fn $method(&self) -> &[String] {
                    self.bindings
                        .get(&InputKeys::$variant)
                        .map(|v| v.0.as_ref())
                        .unwrap_or(&[])
                }
            )+
        }

        impl Default for Keys {
            fn default() -> Self {
                Keys {
                    bindings: HashMap::from([
                        $(
                            (
                                InputKeys::$variant,
                                InputKeyLists(Box::new([$($default.into()),*]))
                            )
                        ),+
                    ])
                }
            }
        }
    };
}

/// Input configuration options of all actions
#[derive(Debug)]
pub(crate) struct Keys {
    bindings: HashMap<InputKeys, InputKeyLists>,
}

impl<'de> Deserialize<'de> for Keys {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let user_bindings = HashMap::<InputKeys, InputKeyLists>::deserialize(deserializer)?;
        let mut keys = Keys::default();
        keys.bindings.extend(user_bindings);
        Ok(keys)
    }
}

define_keys!(
    OpenFile => open_file = ["enter"],
    GoUp => go_up = ["k", "up"],
    GoDown => go_down = ["j", "down"],
    GoParent => go_parent = ["h", "left", "back"],
    GoIntoDir => go_into_dir = ["l", "right"],
    Quit => quit = ["q", "esc"],
    Delete => delete = ["d"],
    Copy => copy = ["y"],
    Paste => paste = ["p"],
    Rename => rename = ["r"],
    Create => create = ["n"],
    CreateDirectory => create_directory = ["N"],
    MoveFile => move_file = ["m"],
    Filter => filter = ["f"],
    ToggleMarker => toggle_marker = ["<space>"],
    ShowInfo => show_info = ["i"],
    Find => find = ["s"],
    ClearMarkers => clear_markers = ["<c-c>"],
    ClearClipboard => clear_clipboard = ["<f2>"],
    ClearFilter => clear_filter = ["<c-f>"],
    ClearAll => clear_all = ["<c-l>"],
    AlternateDelete => alternate_delete = ["<m-d>"],
    SelectAll => select_all = ["<c-a>"],
    PrefixGoTo => prefix_go_to = ["g"],
    GoToTop => go_to_top = ["g"],
    GoToHome => go_to_home = ["h"],
    GoToPath => go_to_path = ["p"],
    GoToBottom => go_to_bottom = ["G"],
    TabNew => tab_new = ["<c-t>"],
    TabClose => tab_close = ["<c-w>"],
    TabNext => tab_next = ["<c-n>"],
    TabPrev => tab_prev = ["<c-p>"],
    ScrollUp => scroll_up = ["pgup"],
    ScrollDown => scroll_down = ["pgdn"],
    Sort => sort = ["o"],
    SortByName => sort_by_name = ["n"],
    SortByNatural => sort_by_natural = ["N"],
    SortByExtension => sort_by_extension = ["e"],
    SortBySize => sort_by_size = ["s"],
    SortByModified => sort_by_modified = ["m"],
    SortByAccessed => sort_by_accessed = ["a"],
    SortByCreated => sort_by_created = ["c"],
    KeybindHelp => keybind_help = ["?"],
    Reload => reload = ["<c-r>"],
);
