//! Key mapping and action dispatch system for runa
//!
//! Defines key to an action, parsing from the config, and enum variants
//! for all navigation, file and actions used by runa.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::Config;
use crate::core::listing::SortMode;

/// Represents any action in the app: navigation, file, or system.
#[derive(Copy, Clone, Debug, PartialEq)]
pub(crate) enum Action {
    Nav(NavAction),
    File(FileAction),
    System(SystemAction),
    Tab(TabAction),
    Command(usize),
}

/// Navigation actions (move, into_parent, markers, etc.)
#[derive(Copy, Clone, Debug, PartialEq)]
pub(crate) enum NavAction {
    GoParent,
    GoIntoDir,
    GoUp,
    GoDown,
    GoToTop,
    GoToBottom,
    GoToPath,
    GoToHome,
    ToggleMarker,
    ClearMarker,
    ClearFilter,
    ClearAll,
    SelectAll,
    ScrollUp,
    ScrollDown,
}

/// File actions (delete, copy, open, paste, etc.)
#[derive(Copy, Clone, Debug, PartialEq)]
pub(crate) enum FileAction {
    Delete,
    Copy,
    Open,
    Paste,
    Rename,
    Create,
    CreateDirectory,
    Filter,
    ShowInfo,
    Find,
    MoveFile,
    AlternateDelete,
    ClearClipboard,
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub(crate) enum TabAction {
    New,
    Close,
    Next,
    Prev,
    Switch(u8),
}

/// System actions (quit)
#[derive(Copy, Clone, Debug, PartialEq)]
pub(crate) enum SystemAction {
    Quit,
    KeyBindHelp,
    Reload,
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub(crate) enum PrefixCommand {
    Nav(NavAction),
    Sort(SortMode),
}

/// Key + modifiers as used in keybind/keymap
#[derive(Hash, Eq, PartialEq, Copy, Clone, Debug)]
pub(crate) struct Key {
    pub(crate) code: KeyCode,
    modifiers: KeyModifiers,
}

/// Stores the mapping from Key to action, which is built in the config
pub(crate) struct Keymap {
    map: HashMap<Key, Action>,
    go_to_map: HashMap<KeyCode, PrefixCommand>,
    sort_map: HashMap<KeyCode, PrefixCommand>,
    go_to_prefix: Vec<Key>,
    sort_prefix: Vec<Key>,
}

impl Keymap {
    /// Builds the keymap from the config
    #[rustfmt::skip]
    pub(crate) fn from_config(config: &Config) -> Self {
        let mut map = HashMap::new();
        let mut go_to_map = HashMap::new();
        let mut sort_map = HashMap::new();
        let keys = config.keys();
        let sort_prefix: Vec<Key> = keys
            .sort().iter()
            .filter_map(|k| parse_key(k))
            .collect();

        let go_to_prefix: Vec<Key> = keys
            .prefix_go_to()
            .iter()
            .filter_map(|k| parse_key(k))
            .collect();

        macro_rules! bind {
            ($keys:expr, $action:expr) => {
                bind($keys, $action, &mut map);
            };
        }

        macro_rules! bind_prefix {
            ($keys:expr, $action:expr, $prefix:expr) => {
                bind_prefix($keys, $prefix, &mut go_to_map);
            };
        }

        macro_rules! bind_sort {
            ($keys:expr, $mode:expr) => {
                bind_prefix($keys, PrefixCommand::Sort($mode), &mut sort_map);
            };
        }

        use NavAction as N;
        use FileAction as F;
        use SystemAction as S;
        use TabAction as T;

        // NavActions
        bind!(keys.go_parent(),         Action::Nav(N::GoParent));
        bind!(keys.go_into_dir(),       Action::Nav(N::GoIntoDir));
        bind!(keys.go_up(),             Action::Nav(N::GoUp));
        bind!(keys.go_down(),           Action::Nav(N::GoDown));
        bind!(keys.toggle_marker(),     Action::Nav(N::ToggleMarker));
        bind!(keys.clear_filter(),      Action::Nav(N::ClearFilter));
        bind!(keys.clear_markers(),     Action::Nav(N::ClearMarker));
        bind!(keys.clear_all(),         Action::Nav(N::ClearAll));
        bind!(keys.go_to_bottom(),      Action::Nav(N::GoToBottom));
        bind!(keys.scroll_up(),         Action::Nav(N::ScrollUp));
        bind!(keys.scroll_down(),       Action::Nav(N::ScrollDown));
        bind!(keys.select_all(),        Action::Nav(N::SelectAll));

        // FileActions
        bind!(keys.open_file(),         Action::File(F::Open));
        bind!(keys.delete(),            Action::File(F::Delete));
        bind!(keys.copy(),              Action::File(F::Copy));
        bind!(keys.paste(),             Action::File(F::Paste));
        bind!(keys.move_file(),         Action::File(F::MoveFile));
        bind!(keys.rename(),            Action::File(F::Rename));
        bind!(keys.create(),            Action::File(F::Create));
        bind!(keys.create_directory(),  Action::File(F::CreateDirectory));
        bind!(keys.filter(),            Action::File(F::Filter));
        bind!(keys.show_info(),         Action::File(F::ShowInfo));
        bind!(keys.find(),              Action::File(F::Find));
        bind!(keys.clear_clipboard(),   Action::File(F::ClearClipboard));
        bind!(keys.alternate_delete(),  Action::File(F::AlternateDelete));

        // TabActions
        bind!(keys.tab_new(),           Action::Tab(T::New));
        bind!(keys.tab_close(),         Action::Tab(T::Close));
        bind!(keys.tab_next(),          Action::Tab(T::Next));
        bind!(keys.tab_prev(),          Action::Tab(T::Prev));

        // SystemActions
        bind!(keys.keybind_help(),      Action::System(S::KeyBindHelp));
        bind!(keys.quit(),              Action::System(S::Quit));
        bind!(keys.reload(),            Action::System(S::Reload));

        // Prefix actions
        bind_prefix!(keys.go_to_top(),  Action::Nav(N::GoToTop),  PrefixCommand::Nav(N::GoToTop));
        bind_prefix!(keys.go_to_home(), Action::Nav(N::GoToHome), PrefixCommand::Nav(N::GoToHome));
        bind_prefix!(keys.go_to_path(), Action::Nav(N::GoToPath), PrefixCommand::Nav(N::GoToPath));

        bind_sort!(keys.sort_by_name(),         SortMode::Name);
        bind_sort!(keys.sort_by_natural(),      SortMode::Natural);
        bind_sort!(keys.sort_by_modified(),     SortMode::Modified);
        bind_sort!(keys.sort_by_created(),      SortMode::Created);
        bind_sort!(keys.sort_by_accessed(),     SortMode::Accessed);
        bind_sort!(keys.sort_by_size(),         SortMode::Size);
        bind_sort!(keys.sort_by_extension(),    SortMode::Extension);

        for (index, command) in config.commands().list().iter().enumerate() {
            bind(command.keys(), Action::Command(index), &mut map);
        }

        Keymap { map, go_to_map, sort_map, go_to_prefix, sort_prefix }
    }

    /// Looks up the action for a given key event
    pub(crate) fn lookup(&self, key: KeyEvent) -> Option<Action> {
        let k = Key {
            code: key.code,
            modifiers: key.modifiers,
        };

        if let Some(action) = self.map.get(&k).copied() {
            return Some(action);
        }

        if matches!(key.code, KeyCode::Char(_)) && key.modifiers.contains(KeyModifiers::SHIFT) {
            let k2 = Key {
                code: key.code,
                modifiers: key.modifiers - KeyModifiers::SHIFT,
            };
            return self.map.get(&k2).copied();
        }

        if key.modifiers.is_empty()
            && let KeyCode::Char(c) = key.code
            && let Some(digit) = c.to_digit(10)
        {
            return Some(Action::Tab(TabAction::Switch(digit as u8)));
        }

        None
    }

    pub(crate) fn go_to_map(&self) -> &HashMap<KeyCode, PrefixCommand> {
        &self.go_to_map
    }

    pub(crate) fn sort_map(&self) -> &HashMap<KeyCode, PrefixCommand> {
        &self.sort_map
    }

    crate::getters! {
        sort_prefix: &[Key],
        go_to_prefix: &[Key],
    }
}

pub(crate) struct KeyPrefix {
    state: PrefixState,
    last_time: Option<Instant>,
    timeout: Duration,
    started: bool,
    exited: bool,
}

#[derive(Copy, Clone, Debug, PartialEq)]
enum PrefixState {
    None,
    GoTo,
    Sort,
}

impl KeyPrefix {
    pub(crate) fn new(timeout: Duration) -> Self {
        Self {
            state: PrefixState::None,
            last_time: None,
            timeout,
            started: false,
            exited: false,
        }
    }

    pub(crate) fn feed(
        &mut self,
        key: &KeyEvent,
        go_to_map: &HashMap<KeyCode, PrefixCommand>,
        sort_map: &HashMap<KeyCode, PrefixCommand>,
        sort_prefix: &[Key],
        go_to_prefix: &[Key],
    ) -> Option<PrefixCommand> {
        self.started = false;
        self.exited = false;

        match self.state {
            PrefixState::None => {
                let k = Key {
                    code: key.code,
                    modifiers: key.modifiers,
                };

                if go_to_prefix.contains(&k) {
                    self.start(PrefixState::GoTo);
                } else if sort_prefix.contains(&k) {
                    self.start(PrefixState::Sort);
                }

                None
            }
            PrefixState::GoTo => self.finish(key, go_to_map),
            PrefixState::Sort => self.finish(key, sort_map),
        }
    }

    #[inline]
    pub(crate) fn started_prefix(&self) -> bool {
        self.started
    }

    #[inline]
    pub(crate) fn exited_prefix(&self) -> bool {
        self.exited
    }

    pub(crate) fn is_active(&self) -> bool {
        self.state != PrefixState::None
    }

    pub(crate) fn is_sort_state(&self) -> bool {
        self.state == PrefixState::Sort
    }

    pub(crate) fn expired(&self) -> bool {
        self.is_active()
            && self
                .last_time
                .is_some_and(|time| time.elapsed() >= self.timeout)
    }

    pub(crate) fn cancel(&mut self) {
        self.state = PrefixState::None;
        self.last_time = None;
        self.exited = true;
    }

    fn start(&mut self, state: PrefixState) {
        self.state = state;
        self.last_time = Some(Instant::now());
        self.started = true;
    }

    fn finish(
        &mut self,
        key: &KeyEvent,
        map: &HashMap<KeyCode, PrefixCommand>,
    ) -> Option<PrefixCommand> {
        let in_time = !self.expired();

        self.cancel();
        if !in_time {
            return None;
        }

        map.get(&key.code).copied()
    }
}

fn parse_key(s: &str) -> Option<Key> {
    let mut modifiers = KeyModifiers::NONE;
    let mut code: Option<KeyCode> = None;

    let is_bracketed = s.starts_with('<') && s.ends_with('>');
    let mut input = s.trim_start_matches('<').trim_end_matches('>').to_string();

    if is_bracketed && input.contains('-') {
        let parts: Vec<&str> = input.split('-').collect();

        for &prefix in parts.iter().take(parts.len().saturating_sub(1)) {
            match prefix.to_lowercase().as_str() {
                "c" | "ctrl" => modifiers |= KeyModifiers::CONTROL,
                "a" | "m" | "alt" => modifiers |= KeyModifiers::ALT,
                "s" | "shift" => modifiers |= KeyModifiers::SHIFT,
                _ => return None,
            }
        }
        input = parts.last()?.to_string();
    }

    let normalized = input.replace('-', "+");
    for part in normalized.split('+') {
        let p_low = part.to_lowercase();
        match p_low.as_str() {
            "ctrl" | "control" => modifiers |= KeyModifiers::CONTROL,
            "alt" | "meta" => modifiers |= KeyModifiers::ALT,
            "shift" => modifiers |= KeyModifiers::SHIFT,

            "up" => code = Some(KeyCode::Up),
            "down" => code = Some(KeyCode::Down),
            "left" => code = Some(KeyCode::Left),
            "right" => code = Some(KeyCode::Right),
            "enter" => code = Some(KeyCode::Enter),
            "esc" => code = Some(KeyCode::Esc),
            "backspace" | "back" => code = Some(KeyCode::Backspace),
            "tab" => code = Some(KeyCode::Tab),
            "space" | "spc" => code = Some(KeyCode::Char(' ')),
            "page_up" | "pageup" | "pgup" => code = Some(KeyCode::PageUp),
            "page_down" | "pagedown" | "pgdn" => code = Some(KeyCode::PageDown),
            "home" => code = Some(KeyCode::Home),
            "end" => code = Some(KeyCode::End),
            _ => {
                if part.len() == 1 {
                    let mut c = part.chars().next()?;
                    if modifiers.contains(KeyModifiers::SHIFT) {
                        c = c.to_ascii_uppercase();
                    }
                    code = Some(KeyCode::Char(c));
                } else if p_low.starts_with('f')
                    && p_low.len() > 1
                    && p_low[1..].chars().all(|c| c.is_ascii_digit())
                {
                    let n = p_low[1..].parse().ok()?;
                    code = Some(KeyCode::F(n));
                } else if part.is_empty() {
                    continue;
                } else {
                    return None;
                }
            }
        }
    }

    Some(Key {
        code: code?,
        modifiers,
    })
}

fn bind(key_list: &[String], action: Action, map: &mut HashMap<Key, Action>) {
    for k in key_list {
        if let Some(key) = parse_key(k) {
            map.insert(key, action);
        }
    }
}

fn bind_prefix(
    key_list: &[String],
    prefix: PrefixCommand,
    map: &mut HashMap<KeyCode, PrefixCommand>,
) {
    for k in key_list {
        if let Some(key) = parse_key(k)
            && key.modifiers.is_empty()
        {
            map.insert(key.code, prefix);
        }
    }
}
