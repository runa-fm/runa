//! Dialog size and position options for runa.

use serde::de::Error;
use serde::{Deserialize, Deserializer};

/// Specifies possible dialog positions within the TUI frame.
/// Also possible to customize the position via the runa.toml
///
/// Is used to determine where dialog/widgets such as dialogs and input boxes are rendered.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum DialogPosition {
    Center,
    Top,
    Bottom,
    Left,
    Right,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    Custom(u16, u16),
}

/// Deserialize so that the runa.toml custom position and size can be made simpler instead of just
/// standard serde [derive(Deserialize)]
/// position = "top_left"
/// position = "bottomright"
/// position = [25, 60]
/// position = { x = 42, y = 80 }
impl<'de> Deserialize<'de> for DialogPosition {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Helper {
            Str(String),
            Arr([u16; 2]),
            XY { x: u16, y: u16 },
        }

        match Helper::deserialize(deserializer)? {
            Helper::Str(ref s) if s.eq_ignore_ascii_case("center") => Ok(DialogPosition::Center),
            Helper::Str(ref s) if s.eq_ignore_ascii_case("top") => Ok(DialogPosition::Top),
            Helper::Str(ref s) if s.eq_ignore_ascii_case("bottom") => Ok(DialogPosition::Bottom),
            Helper::Str(ref s) if s.eq_ignore_ascii_case("left") => Ok(DialogPosition::Left),
            Helper::Str(ref s) if s.eq_ignore_ascii_case("right") => Ok(DialogPosition::Right),
            Helper::Str(ref s)
                if s.eq_ignore_ascii_case("top_left") || s.eq_ignore_ascii_case("topleft") =>
            {
                Ok(DialogPosition::TopLeft)
            }
            Helper::Str(ref s)
                if s.eq_ignore_ascii_case("top_right") || s.eq_ignore_ascii_case("topright") =>
            {
                Ok(DialogPosition::TopRight)
            }
            Helper::Str(ref s)
                if s.eq_ignore_ascii_case("bottom_left")
                    || s.eq_ignore_ascii_case("bottomleft") =>
            {
                Ok(DialogPosition::BottomLeft)
            }
            Helper::Str(ref s)
                if s.eq_ignore_ascii_case("bottom_right")
                    || s.eq_ignore_ascii_case("bottomright") =>
            {
                Ok(DialogPosition::BottomRight)
            }
            Helper::Str(s) => Err(D::Error::custom(format!("invalid DialogPosition: '{}'", s))),
            Helper::Arr([x, y]) => Ok(DialogPosition::Custom(x, y)),
            Helper::XY { x, y } => Ok(DialogPosition::Custom(x, y)),
        }
    }
}

/// Preset for all dialogs/widgets sizes as well as a customized size via the runa.toml
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum DialogSize {
    Small,
    Medium,
    Large,
    Custom(u16, u16),
}

/// Deserializer so that the runa.toml configuration can be made simpler to configure the size of
/// dialogs/widgets
///
/// size = "small"
/// size = [10, 10]
/// size = { w = 10, h = 20 }
impl<'de> Deserialize<'de> for DialogSize {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Helper {
            Str(String),
            Arr([u16; 2]),
            Obj { w: u16, h: u16 },
        }

        match Helper::deserialize(deserializer)? {
            Helper::Str(ref s) if s.eq_ignore_ascii_case("small") => Ok(DialogSize::Small),
            Helper::Str(ref s) if s.eq_ignore_ascii_case("medium") => Ok(DialogSize::Medium),
            Helper::Str(ref s) if s.eq_ignore_ascii_case("large") => Ok(DialogSize::Large),
            Helper::Str(s) => Err(D::Error::custom(format!("invalid DialogSize: '{}'", s))),
            Helper::Arr([w, h]) => Ok(DialogSize::Custom(w, h)),
            Helper::Obj { w, h } => Ok(DialogSize::Custom(w, h)),
        }
    }
}
