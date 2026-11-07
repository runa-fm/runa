//! Theme configuration options for runa
//!
//! This module defines the theme configuration options which are read from the runa.toml
//! configuration file.
//!
//! Also holds the internal themes and the logic to apply user overrides on top of them.

mod colorpair;
mod components;

use colorpair::ColorPair;
use components::*;

use std::collections::HashMap;
use std::sync::LazyLock;

use ratatui::style::{Color, Modifier, Style};
use serde::{Deserialize, Deserializer};

use crate::config::presets;

trait ColorFallback {
    fn or(self, fallback: Color) -> Color;
}

impl ColorFallback for Color {
    fn or(self, fallback: Color) -> Color {
        if let Color::Reset = self {
            fallback
        } else {
            self
        }
    }
}

/// Theme configuration options
/// Holds all color and style options for the application.
/// Also holds the internal themes and the logic to apply user overrides on top of them.
#[derive(Deserialize, Debug)]
#[serde(default)]
pub(crate) struct Theme {
    name: Option<String>,
    selection: ColorPair,
    underline: ColorPair,
    accent: ColorPair,
    entry: ColorPair,
    #[serde(deserialize_with = "deserialize_color_field")]
    bg: Color,
    directory: ColorPair,
    separator: ColorPair,
    selection_icon: String,
    parent: PaneTheme,
    preview: PaneTheme,
    path: ColorPair,
    status_line: ColorPair,
    #[serde(deserialize_with = "deserialize_color_field")]
    exe_color: Color,
    #[serde(deserialize_with = "deserialize_color_map")]
    icon_color: HashMap<String, Color>,
    filename: HashMap<String, ColorPair>,
    ext: HashMap<String, ColorPair>,
    symlink: SymlinkTheme,
    marker: MarkerTheme,
    widget: WidgetTheme,
    tab: TabTheme,
    info: InfoStatusTheme,
    archive: ArchiveTheme,

    #[serde(skip)]
    filename_cache: HashMap<String, Style>,
    #[serde(skip)]
    extension_cache: HashMap<String, Style>,
    #[serde(skip)]
    icon_color_cache: HashMap<String, Color>,
}

impl Default for Theme {
    fn default() -> Self {
        Theme {
            name: None,
            accent: ColorPair::new(Color::Indexed(238), Color::Reset),
            selection: ColorPair::new(Color::Reset, Color::Indexed(236)),
            underline: ColorPair::default(),
            entry: ColorPair::default(),
            bg: Color::Reset,
            directory: ColorPair::new(Color::Blue, Color::Reset),
            separator: ColorPair::new(Color::Indexed(238), Color::Reset),
            selection_icon: "".into(),
            parent: PaneTheme::default(),
            preview: PaneTheme::default(),
            path: ColorPair::new(Color::Magenta, Color::Reset),
            status_line: ColorPair::default(),
            filename: HashMap::new(),
            ext: HashMap::new(),
            icon_color: HashMap::new(),
            exe_color: Color::LightGreen,
            symlink: SymlinkTheme::default(),
            marker: MarkerTheme::default(),
            widget: WidgetTheme::default(),
            tab: TabTheme::default(),
            info: InfoStatusTheme::default(),
            archive: ArchiveTheme::default(),
            filename_cache: HashMap::new(),
            extension_cache: HashMap::new(),
            icon_color_cache: HashMap::new(),
        }
    }
}

/// Macro to override a field in the target theme if it differs from the default theme.
/// This is used to apply user-defined overrides on top of a preset theme.
macro_rules! override_themes {
    ($target:ident, $user:ident, $default:ident, [$($field:ident),* $(,)?]) => {
        $(
            if $user.$field != $default.$field {
                $target.$field = $user.$field.clone();
            }
        )*
    };
}

macro_rules! builtin_styles {
    (in $section:ident: $($(#[$meta:meta])* $fn_name:ident => $field:ident),* $(,)?) => {
        $(
            $(#[$meta])*
            pub(crate) fn $fn_name(&self) -> ::ratatui::style::Style {
                self.$field.style_or(&$crate::config::Theme::builtin().$section.$field)
            }
        )*
    };
    ($($fn_name:ident => $field:ident),* $(,)?) => {
        $(
            pub(crate) fn $fn_name(&self) -> ::ratatui::style::Style {
                self.$field.style_or(&$crate::config::Theme::builtin().$field)
            }
        )*
    };
}
pub(super) use builtin_styles;

/// Theme implementation
/// Provides methods to access theme properties and apply user overrides.
impl Theme {
    /// Get internal default theme reference
    /// Used for fallback when a color is set to Reset
    /// This avoids recreating the default theme multiple times
    /// by using a static Lazy instance.
    pub(crate) fn builtin() -> &'static Self {
        static DEFAULT: LazyLock<Theme> = LazyLock::new(Theme::default);
        &DEFAULT
    }

    crate::getters! {
        exe_color: Color,
        icon_color_cache: &HashMap<String, Color>,
        selection_icon: &str,
        preview: &PaneTheme,
        parent: &PaneTheme,
        marker: &MarkerTheme,
        widget: &WidgetTheme,
        info: &InfoStatusTheme,
        tab: &TabTheme,
    }

    // Getters for various theme properties with fallbacks to internal defaults
    // _style methods for getting Style instances with fallbacks to internal defaults
    builtin_styles! {
        accent_style => accent,
        selection_style => selection,
        underline_style => underline,
        entry_style => entry,
        directory_style => directory,
        path_style => path,
        status_line_style => status_line,
    }

    pub(crate) fn preview_selection_style(&self) -> Style {
        self.preview.selection_style(&self.selection)
    }

    pub(crate) fn bg_style(&self) -> Style {
        ColorPair::new(Color::Reset, self.bg).style_or(&ColorPair::default())
    }

    pub(crate) fn parent_selection_style(&self) -> Style {
        self.parent.selection_style(&self.selection)
    }

    pub(crate) fn separator_style(&self) -> Style {
        let default = Theme::builtin();
        if self.separator != default.separator {
            self.separator.style_or(&default.separator)
        } else {
            self.accent_style()
        }
    }

    pub(crate) fn archive_guide_style(&self) -> Style {
        self.archive.guide_style(self.accent_style())
    }

    pub(crate) fn archive_modifier(&self) -> Modifier {
        self.archive.modifier()
    }

    pub(crate) fn symlink_theme(&self) -> SymlinkTheme {
        let defaults = Theme::builtin().symlink;
        self.symlink.with_fallback(defaults)
    }

    pub(crate) fn entry_color_override(
        &self,
        name: &str,
        is_dir: bool,
        ext: Option<&str>,
    ) -> Option<Style> {
        if let Some(s) = self.filename_cache.get(name) {
            return Some(*s);
        }

        if !is_dir
            && let Some(ext) = ext
            && let Some(s) = self.extension_cache.get(ext)
        {
            return Some(*s);
        }

        Self::get_default_style(name, ext, is_dir)
    }

    fn get_default_style(name: &str, ext: Option<&str>, is_dir: bool) -> Option<Style> {
        match name {
            "Dockerfile" => return Some(Style::default().fg(Color::Cyan)),
            "Cargo.toml" | "LICENSE" | "README.md" => {
                return Some(Style::default().fg(Color::Yellow));
            }
            _ => {}
        }

        if !is_dir && let Some(e) = ext {
            let color = match e {
                "zip" | "tar" | "gz" | "7z" | "rar" => Color::Red,
                "jpg" | "jpeg" | "png" | "gif" | "svg" | "webm" => Color::Magenta,
                "tmp" => Color::Rgb(108, 121, 135),
                _ => return None,
            };
            return Some(Style::default().fg(color));
        }

        None
    }

    /// Apply user overrides on top of a preset theme if a known preset name is provided.
    /// If no preset name is provided or the name is unknown, returns the theme as is.
    #[inline(never)]
    pub(super) fn with_overrides(mut self) -> Self {
        let preset_name = self.name.clone();
        let defaults = Theme::builtin();

        if self.accent != defaults.accent && self.separator == defaults.separator {
            self.separator = self.accent;
        }

        if let Some(name) = preset_name.as_deref()
            && let Some(mut base) = Self::get_preset_by_name(name)
        {
            base.apply_user_overrides(self);
            base.build_style_maps();
            base.inherit_pane_colors();
            return base;
        }

        self.build_style_maps();
        self.inherit_pane_colors();
        self
    }

    #[inline(never)]
    fn get_preset_by_name(name: &str) -> Option<Theme> {
        let (palette, icon) = match name {
            "gruvbox-dark-hard" => (presets::GRUV_DARK_HARD, "*"),
            "gruvbox-dark" => (presets::GRUV_DARK, "*"),
            "gruvbox-light" => (presets::GRUV_LIGHT, "*"),

            "catppuccin-mocha" | "catppuccin-macchiato" => (presets::MOCHA, "┃"),
            "catppuccin-frappe" => (presets::FRAPPE, "┃"),
            "catppuccin-latte" => (presets::LATTE, "┃"),

            "nord" => (presets::NORD, "*"),
            "two-dark" => (presets::TWO_DARK, "*"),
            "one-dark" => (presets::ONE_DARK, "*"),

            "solarized-dark" => (presets::SOLARIZED_DARK, "*"),
            "solarized-light" => (presets::SOLARIZED_LIGHT, "*"),

            "dracula" => (presets::DRACULA, "┃"),
            "monokai" => (presets::MONOKAI, "┃"),
            "nightfox" => (presets::NIGHTFOX, "┃"),
            "carbonfox" => (presets::CARBON, "┃"),

            "tokyonight-storm" => (presets::TOKYO_STORM, "┃"),
            "tokyonight" | "tokyonight-night" => (presets::TOKYO_NIGHT, "┃"),
            "tokyonight-day" => (presets::TOKYO_DAY, "┃"),

            "everforest" => (presets::FOREST, "*"),
            "rose-pine" | "rose_pine" => (presets::ROSE_PINE, "*"),

            _ => return None,
        };

        // One single call-site for make_theme prevents monomorphization bloat
        Some(make_theme(name, palette, icon))
    }

    /// Map internal theme name to bat theme name for syntax highlighting.
    /// If no name is set, defaults to "TwoDark".
    pub(super) fn bat_theme_name(&self) -> &'static str {
        self.name
            .as_deref()
            .map(Theme::map_to_bat_theme)
            .unwrap_or("TwoDark")
    }

    /// Helper function to map internal theme names to bat theme names.
    /// Used by bat for syntax highlighting.
    fn map_to_bat_theme(internal_theme: &str) -> &'static str {
        match internal_theme {
            "default" => "TwoDark",
            "two-dark" => "TwoDark",
            "one-dark" => "OneHalfDark",
            "gruvbox-dark" | "gruvbox-dark-hard" | "gruvbox" => "gruvbox-dark",
            "gruvbox-light" => "gruvbox-light",
            "tokyonight-night" | "tokyonight" | "tokyonight-storm" => "TwoDark",
            "catppuccin-latte" => "Catppuccin Latte",
            "catppuccin-frappe" => "Catppuccin Frappe",
            "catppuccin-macchiato" => "Catppuccin Macchiato",
            "catppuccin-mocha" | "catppuccin" => "Catppuccin Mocha",
            "nightfox" | "carbonfox" | "rose-pine" | "everforest" => "TwoDark",
            "monokai" => "Monokai Extended (default)",
            "nord" => "Nord",
            "solarized-dark" => "Solarized (dark)",
            "solarized-light" => "Solarized (light)",
            "dracula" => "Dracula",
            _ => "TwoDark",
        }
    }

    /// Apply user overrides on top of the current theme.
    /// Compares each field with the default theme and overrides if changed
    /// This allows to only specify the fields they want to change
    fn apply_user_overrides(&mut self, user: Theme) {
        let defaults = Theme::builtin();

        #[rustfmt::skip]
        override_themes!(self, user, defaults, [
            accent,
            selection,
            underline,
            entry,
            bg,
            directory,
            separator,
            parent,
            preview,
            path,
            ext,
            filename,
            icon_color,
            status_line,
            symlink,
            selection_icon,
            marker,
            widget,
            tab,
            info,
            archive,
        ]);

        if user.name.is_some() {
            self.name = user.name.clone();
        }
    }

    #[inline(never)]
    fn build_style_maps(&mut self) {
        let fallback = ColorPair::default();

        self.filename_cache = self
            .filename
            .iter()
            .map(|(k, v)| (k.clone(), v.style_or(&fallback)))
            .collect();

        self.extension_cache = self
            .ext
            .iter()
            .map(|(k, v)| (k.to_ascii_lowercase(), v.style_or(&fallback)))
            .collect();

        self.icon_color_cache = self
            .icon_color
            .iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect();
    }

    fn inherit_pane_colors(&mut self) {
        self.parent.inherit(&self.entry, self.bg);
        self.preview.inherit(&self.entry, self.bg);
    }
}

/// Parses a string (color name or hex) into a ratatui::style::color
///
/// Supports standard names (red, green, etc.) as well as hex values (#RRGGBB or #RGB)
pub(crate) fn parse_color(s: &str) -> Color {
    match s.to_lowercase().as_str() {
        "default" | "reset" => Color::Reset,
        "yellow" => Color::Yellow,
        "red" => Color::Red,
        "blue" => Color::Blue,
        "green" => Color::Green,
        "magenta" => Color::Magenta,
        "cyan" => Color::Cyan,
        "white" => Color::White,
        "black" => Color::Black,
        "gray" => Color::Gray,
        "darkgray" => Color::DarkGray,
        "lightred" => Color::LightRed,
        "lightgreen" => Color::LightGreen,
        "lightyellow" => Color::LightYellow,
        "lightblue" => Color::LightBlue,
        "lightmagenta" => Color::LightMagenta,
        "lightcyan" => Color::LightCyan,
        _ => {
            if let Some(color) = s.strip_prefix('#') {
                match color.len() {
                    6 => {
                        if let Ok(rgb) = u32::from_str_radix(color, 16) {
                            return Color::Rgb(
                                ((rgb >> 16) & 0xFF) as u8,
                                ((rgb >> 8) & 0xFF) as u8,
                                (rgb & 0xFF) as u8,
                            );
                        }
                    }
                    3 => {
                        let expanded = color
                            .chars()
                            .map(|c| format!("{}{}", c, c))
                            .collect::<String>();
                        if let Ok(rgb) = u32::from_str_radix(&expanded, 16) {
                            return Color::Rgb(
                                ((rgb >> 16) & 0xFF) as u8,
                                ((rgb >> 8) & 0xFF) as u8,
                                (rgb & 0xFF) as u8,
                            );
                        }
                    }
                    _ => {}
                }
            }
            // fallback
            Color::Reset
        }
    }
}

fn deserialize_color_field<'de, D>(deserializer: D) -> Result<Color, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    Ok(parse_color(&s))
}

fn deserialize_color_map<'de, D>(deserializer: D) -> Result<HashMap<String, Color>, D::Error>
where
    D: Deserializer<'de>,
{
    let raw_map: HashMap<String, String> = HashMap::deserialize(deserializer)?;

    let processed_map = raw_map
        .into_iter()
        .map(|(key, val)| (key, parse_color(&val)))
        .collect();

    Ok(processed_map)
}

fn rgb(c: (u8, u8, u8)) -> Color {
    Color::Rgb(c.0, c.1, c.2)
}

pub(super) struct Palette {
    pub(super) base: (u8, u8, u8),
    pub(super) surface: (u8, u8, u8),
    pub(super) overlay: (u8, u8, u8),
    pub(super) primary: (u8, u8, u8),
    pub(super) secondary: (u8, u8, u8),
    pub(super) directory: (u8, u8, u8),
}

/// Centralized function to create a Theme from a Palette.
/// Used by all internal themes to avoid code duplication.
fn make_theme(name: &str, palette: Palette, icon: &str) -> Theme {
    let primary = rgb(palette.primary);
    let secondary = rgb(palette.secondary);
    let muted = rgb(palette.overlay);
    let surface = rgb(palette.surface);
    let base_bg = rgb(palette.base);
    let dir_color = rgb(palette.directory);

    Theme {
        name: Some(name.to_string()),
        accent: ColorPair::new(surface, Color::Reset),
        selection: ColorPair::new(Color::Reset, surface),
        directory: ColorPair::new(dir_color, Color::Reset),
        separator: ColorPair::new(surface, Color::Reset),
        path: ColorPair::new(muted, Color::Reset),
        status_line: ColorPair::new(Color::Reset, base_bg),
        symlink: SymlinkTheme::from_palette(secondary, secondary),
        marker: MarkerTheme::new(icon.to_string(), ColorPair::new(primary, Color::Reset))
            .with_clipboard(ColorPair::new(secondary, Color::Reset)),
        widget: WidgetTheme::from_palette(primary, secondary, surface),
        tab: TabTheme::from_palette(primary, muted),
        ..Theme::default()
    }
}
