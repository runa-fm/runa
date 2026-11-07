//! Color paring for foreground and background colors of theme fields.

use ratatui::style::{Color, Style};
use serde::Deserialize;

use crate::config::theme::{ColorFallback, deserialize_color_field, parse_color};

/// ColorPair struct to hold foreground and background colors.
/// Used throughout the theme configuration.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct ColorPair {
    fg: Color,
    bg: Color,
}

impl<'de> Deserialize<'de> for ColorPair {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum ColorFormat {
            Short(String),
            Full {
                #[serde(default, deserialize_with = "deserialize_color_field")]
                fg: Color,
                #[serde(default, deserialize_with = "deserialize_color_field")]
                bg: Color,
            },
        }

        let inner = ColorFormat::deserialize(deserializer)?;
        match inner {
            ColorFormat::Short(s) => Ok(ColorPair {
                fg: parse_color(&s),
                bg: Color::Reset,
            }),
            ColorFormat::Full { fg, bg } => Ok(ColorPair { fg, bg }),
        }
    }
}

impl Default for ColorPair {
    fn default() -> Self {
        Self {
            fg: Color::Reset,
            bg: Color::Reset,
        }
    }
}

impl From<Style> for ColorPair {
    fn from(style: Style) -> Self {
        Self {
            fg: style.fg.unwrap_or(Color::Reset),
            bg: style.bg.unwrap_or(Color::Reset),
        }
    }
}

impl ColorPair {
    pub(super) fn new(fg: Color, bg: Color) -> Self {
        Self { fg, bg }
    }

    pub(super) fn resolve(&self, other: &ColorPair) -> Self {
        Self {
            fg: self.fg.or(other.fg),
            bg: self.bg.or(other.bg),
        }
    }

    pub(super) fn style_or(&self, fallback: &ColorPair) -> Style {
        let resolved = self.resolve(fallback);
        let mut style = Style::default();
        if resolved.fg != Color::Reset {
            style = style.fg(resolved.fg);
        }

        if resolved.bg != Color::Reset {
            style = style.bg(resolved.bg);
        }

        style
    }

    pub(super) fn fg_style(&self) -> Style {
        Style::default().fg(self.fg)
    }

    pub(super) fn bg_style(&self) -> Style {
        Style::default().bg(self.bg)
    }
}
