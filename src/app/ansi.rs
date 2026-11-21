//! ANSI escape parser for preview pane lines.

use std::ops::RangeInclusive;

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};

// ASCII control and boundary bytes
const ESC: u8 = 0x1b;
const DEL: u8 = 0x7f;
const BEL: u8 = 0x07;
const FIRST_PRINTABLE: u8 = 0x20;

// Sequence byte ranges and markers
const CSI_PARAM_BYTES: RangeInclusive<u8> = 0x30..=0x3f;
const CSI_INTERMEDIATE_BYTES: RangeInclusive<u8> = 0x20..=0x2f;
const ESC_FINAL_BYTES: RangeInclusive<u8> = 0x40..=0x7e;
const PRINTABLE_ASCII: RangeInclusive<u8> = 0x20..=0x7e;
const CHARSET_INTRO: RangeInclusive<u8> = b'('..=b'+';

const CSI_INTRO: u8 = b'[';
const OSC_INTRO: u8 = b']';
const SGR_FINAL: u8 = b'm';
const ST_SECOND: u8 = b'\\';
const TAB: u8 = b'\t';
const TAB_SPACES: &str = "    ";

// SGR codes shared across semicolon and colon parsers
const SGR_UNDERLINE: u16 = 4;
const SGR_FG_EXTENDED: u16 = 38;
const SGR_BG_EXTENDED: u16 = 48;
const SGR_UNDERLINE_EXTENDED: u16 = 58;
const EXTENDED_COLOR_INDEXED: u16 = 5;
const EXTENDED_COLOR_RGB: u16 = 2;

#[inline]
const fn is_control_byte(b: u8) -> bool {
    b < FIRST_PRINTABLE || b == DEL
}

#[derive(Default, Clone, Copy, PartialEq, Eq)]
struct AnsiStyle {
    fg: Option<Color>,
    bg: Option<Color>,
    modifiers: Modifier,
}

impl AnsiStyle {
    fn is_default(&self) -> bool {
        self.fg.is_none() && self.bg.is_none() && self.modifiers.is_empty()
    }

    fn reset(&mut self) {
        *self = Self::default();
    }

    fn to_style(self) -> Style {
        Style {
            fg: self.fg,
            bg: self.bg,
            add_modifier: self.modifiers,
            ..Style::default()
        }
    }
}

/// Parses ANSI styled lines into ratatui [Text].
/// Preverses line count and style.
///
/// # Returns
/// ratatui [Text]
pub(crate) fn parse_lines(lines: Vec<String>) -> Text<'static> {
    let mut out = Vec::with_capacity(lines.len());
    let mut style = AnsiStyle::default();

    for line in lines {
        if line.bytes().all(|b| !is_control_byte(b)) {
            let parsed = if line.is_empty() {
                Line::default()
            } else if style.is_default() {
                Line::from(Span::raw(line))
            } else {
                Line::from(Span::styled(line, style.to_style()))
            };

            out.push(parsed);
        } else {
            out.push(parse_line(&line, &mut style));
        }
    }

    Text::from(out)
}

/// Parses a single ANSI line with a set styling to a ratatui [Line]
///
/// # Returns
/// ratatui [Line]
fn parse_line(line: &str, style: &mut AnsiStyle) -> Line<'static> {
    let bytes = line.as_bytes();

    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut last_style: Option<AnsiStyle> = None;
    let mut text_start = 0;
    let mut i = 0;

    while i < bytes.len() {
        match bytes[i] {
            ESC => {
                push_text(&mut spans, &mut last_style, *style, &line[text_start..i]);
                i = consume_escape(bytes, i, style);
                text_start = i;
            }
            TAB => {
                push_text(&mut spans, &mut last_style, *style, &line[text_start..i]);
                push_text(&mut spans, &mut last_style, *style, TAB_SPACES);
                i += 1;
                text_start = i;
            }
            b if is_control_byte(b) => {
                push_text(&mut spans, &mut last_style, *style, &line[text_start..i]);
                i += 1;
                text_start = i;
            }
            _ => i += 1,
        }
    }

    push_text(&mut spans, &mut last_style, *style, &line[text_start..]);
    Line::from(spans)
}

/// Push a single styled or raw span to the span vector.
fn push_text(
    spans: &mut Vec<Span<'static>>,
    last_style: &mut Option<AnsiStyle>,
    style: AnsiStyle,
    chunk: &str,
) {
    if chunk.is_empty() {
        return;
    }

    if *last_style == Some(style)
        && let Some(prev) = spans.last_mut()
    {
        prev.content.to_mut().push_str(chunk);
        return;
    }

    let span = if style.is_default() {
        Span::raw(chunk.to_owned())
    } else {
        Span::styled(chunk.to_owned(), style.to_style())
    };

    spans.push(span);
    *last_style = Some(style);
}

/// Consume the escapce sequence starting at esc and applies any SGR updates to style.
///
/// # Returns
/// The byte index after consumed sequence
fn consume_escape(bytes: &[u8], esc: usize, style: &mut AnsiStyle) -> usize {
    let mut i = esc + 1;
    if i >= bytes.len() {
        return i;
    }

    match bytes[i] {
        CSI_INTRO => {
            i += 1;
            let param_start = i;
            while i < bytes.len() && CSI_PARAM_BYTES.contains(&bytes[i]) {
                i += 1;
            }

            let param_end = i;
            let mut has_intermediates = false;
            while i < bytes.len() && CSI_INTERMEDIATE_BYTES.contains(&bytes[i]) {
                has_intermediates = true;
                i += 1;
            }

            if i < bytes.len() && ESC_FINAL_BYTES.contains(&bytes[i]) {
                let final_byte = bytes[i];
                i += 1;

                if final_byte == SGR_FINAL && !has_intermediates {
                    apply_sgr(&bytes[param_start..param_end], style);
                }
            }

            i
        }
        OSC_INTRO => {
            i += 1;
            while i < bytes.len() {
                if bytes[i] == BEL {
                    return i + 1;
                }
                if bytes[i] == ESC && bytes.get(i + 1) == Some(&ST_SECOND) {
                    return i + 2;
                }
                i += 1;
            }
            bytes.len()
        }
        b if CHARSET_INTRO.contains(&b) => {
            i += 1;
            if i < bytes.len() && PRINTABLE_ASCII.contains(&bytes[i]) {
                i += 1;
            }

            i
        }
        b if ESC_FINAL_BYTES.contains(&b) => i + 1,
        _ => i,
    }
}

/// Parses semicolon separated SGR params and applies them to style.
fn apply_sgr(params: &[u8], style: &mut AnsiStyle) {
    if params.is_empty() {
        style.reset();
        return;
    }

    let mut tokens = params.split(|&b| b == b';');
    while let Some(token) = tokens.next() {
        if token.contains(&b':') {
            apply_colon_sgr(token, style);
            continue;
        }

        let Some(code) = parse_param(token) else {
            continue;
        };

        match code {
            0 => style.reset(),
            1 => style.modifiers.insert(Modifier::BOLD),
            2 => style.modifiers.insert(Modifier::DIM),
            3 => style.modifiers.insert(Modifier::ITALIC),
            SGR_UNDERLINE => style.modifiers.insert(Modifier::UNDERLINED),
            5 => style.modifiers.insert(Modifier::SLOW_BLINK),
            6 => style.modifiers.insert(Modifier::RAPID_BLINK),
            7 => style.modifiers.insert(Modifier::REVERSED),
            8 => style.modifiers.insert(Modifier::HIDDEN),
            9 => style.modifiers.insert(Modifier::CROSSED_OUT),
            21 | 22 => style.modifiers.remove(Modifier::BOLD | Modifier::DIM),
            23 => style.modifiers.remove(Modifier::ITALIC),
            24 => style.modifiers.remove(Modifier::UNDERLINED),
            25 => style
                .modifiers
                .remove(Modifier::SLOW_BLINK | Modifier::RAPID_BLINK),
            27 => style.modifiers.remove(Modifier::REVERSED),
            28 => style.modifiers.remove(Modifier::HIDDEN),
            29 => style.modifiers.remove(Modifier::CROSSED_OUT),
            30..=37 => style.fg = Some(standard_color(code - 30)),
            SGR_FG_EXTENDED => {
                if let Some(color) = parse_extended_color(&mut tokens) {
                    style.fg = Some(color);
                }
            }
            39 => style.fg = None,
            40..=47 => style.bg = Some(standard_color(code - 40)),
            SGR_BG_EXTENDED => {
                if let Some(color) = parse_extended_color(&mut tokens) {
                    style.bg = Some(color);
                }
            }
            49 => style.bg = None,
            SGR_UNDERLINE_EXTENDED => {
                let _ = parse_extended_color(&mut tokens);
            }
            59 => {}
            90..=97 => style.fg = Some(bright_color(code - 90)),
            100..=107 => style.bg = Some(bright_color(code - 100)),
            _ => {}
        }
    }
}

/// Parses colon separated SGR params and applies them to style.
fn apply_colon_sgr(token: &[u8], style: &mut AnsiStyle) {
    let mut parts = token.split(|&b| b == b':');
    let Some(code) = parts.next().and_then(parse_param) else {
        return;
    };

    match code {
        SGR_UNDERLINE => match parts.next().and_then(parse_param) {
            Some(0) => style.modifiers.remove(Modifier::UNDERLINED),
            Some(1..=5) => style.modifiers.insert(Modifier::UNDERLINED),
            _ => {}
        },
        SGR_FG_EXTENDED | SGR_BG_EXTENDED => {
            let color = match parts.next().and_then(parse_param) {
                Some(EXTENDED_COLOR_INDEXED) => parts.next().and_then(parse_u8).map(Color::Indexed),
                Some(EXTENDED_COLOR_RGB) => {
                    let a = parts.next();
                    let b = parts.next();
                    let c = parts.next();
                    let d = parts.next();
                    // ISO 8613-6 has a colorspace slot (38:2:<cs>:r:g:b); fall back to 38:2:r:g:b.
                    let (r, g, b) = if d.is_some() { (b, c, d) } else { (a, b, c) };

                    match (
                        r.and_then(parse_u8),
                        g.and_then(parse_u8),
                        b.and_then(parse_u8),
                    ) {
                        (Some(r), Some(g), Some(b)) => Some(Color::Rgb(r, g, b)),
                        _ => None,
                    }
                }
                _ => None,
            };

            if let Some(color) = color {
                if code == SGR_FG_EXTENDED {
                    style.fg = Some(color);
                } else {
                    style.bg = Some(color);
                }
            }
        }
        _ => {}
    }
}

/// Parses an extended SGR color from tokens
fn parse_extended_color<'a>(tokens: &mut impl Iterator<Item = &'a [u8]>) -> Option<Color> {
    match tokens.next().and_then(parse_param)? {
        EXTENDED_COLOR_INDEXED => Some(Color::Indexed(tokens.next().and_then(parse_u8)?)),
        EXTENDED_COLOR_RGB => {
            let r = tokens.next();
            let g = tokens.next();
            let b = tokens.next();
            Some(Color::Rgb(
                r.and_then(parse_u8)?,
                g.and_then(parse_u8)?,
                b.and_then(parse_u8)?,
            ))
        }
        _ => None,
    }
}

/// Parses a ASCII decimal SGR parameter.
/// Treats an empty slie as `0`.
///
/// # Returns
/// The parsed value or None on non-digit bytes.
fn parse_param(bytes: &[u8]) -> Option<u16> {
    if bytes.is_empty() {
        return Some(0);
    }

    let mut val: u16 = 0;
    for &b in bytes {
        if !b.is_ascii_digit() {
            return None;
        }
        val = val.checked_mul(10)?.checked_add(u16::from(b - b'0'))?;
    }
    Some(val)
}

/// Parses a ASCII decimal SGR color (0..=255)
///
/// # Returns
/// The parsed u8 value or None on non-digit bytes.
fn parse_u8(bytes: &[u8]) -> Option<u8> {
    parse_param(bytes).and_then(|v| u8::try_from(v).ok())
}

/// Matches the ratatui standard [Color]s to an index
///
/// # Returns
/// The ratatui [Color] for that matched index
const fn standard_color(idx: u16) -> Color {
    match idx {
        0 => Color::Black,
        1 => Color::Red,
        2 => Color::Green,
        3 => Color::Yellow,
        4 => Color::Blue,
        5 => Color::Magenta,
        6 => Color::Cyan,
        _ => Color::Gray,
    }
}

/// Matches the ratatui bright [Color]s to an index
///
/// # Returns
/// The ratatui [Color] for that matched index
const fn bright_color(idx: u16) -> Color {
    match idx {
        0 => Color::DarkGray,
        1 => Color::LightRed,
        2 => Color::LightGreen,
        3 => Color::LightYellow,
        4 => Color::LightBlue,
        5 => Color::LightMagenta,
        6 => Color::LightCyan,
        _ => Color::White,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(input: &[&str]) -> Vec<String> {
        input.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn plain_and_empty_lines_keep_line_count() {
        let text = parse_lines(lines(&["hello", "", "world", ""]));
        assert_eq!(text.lines.len(), 4);
        assert_eq!(text.lines[0].spans[0].content, "hello");
        assert!(text.lines[1].spans.is_empty());
        assert_eq!(text.lines[2].spans[0].content, "world");
        assert!(text.lines[3].spans.is_empty());
    }

    #[test]
    fn resets_produce_none_never_color_reset() {
        let text = parse_lines(lines(&[
            "\x1b[31;42mred\x1b[0mplain\x1b[33m\x1b[39myellow_off\x1b[44m\x1b[49mbg_off\x1b[31m\x1b[mempty",
        ]));
        let spans = &text.lines[0].spans;
        assert_eq!(spans[0].style.fg, Some(Color::Red));
        assert_eq!(spans[0].style.bg, Some(Color::Green));
        assert_eq!(spans[1].content, "plainyellow_offbg_offempty");
        assert_eq!(spans[1].style.fg, None);
        assert_eq!(spans[1].style.bg, None);
    }

    #[test]
    fn extended_colors_and_underline_color_skip() {
        let text = parse_lines(lines(&["\x1b[38;5;208;48;2;1;2;3ma\x1b[0m\
             \x1b[38:5:42mb\x1b[0m\
             \x1b[38:2::10:20:30mc\x1b[0m\
             \x1b[38:2:40:50:60md\x1b[0m\
             \x1b[58;2;255;0;0me"]));
        let spans = &text.lines[0].spans;
        assert_eq!(spans[0].style.fg, Some(Color::Indexed(208)));
        assert_eq!(spans[0].style.bg, Some(Color::Rgb(1, 2, 3)));
        assert_eq!(spans[1].style.fg, Some(Color::Indexed(42)));
        assert_eq!(spans[2].style.fg, Some(Color::Rgb(10, 20, 30)));
        assert_eq!(spans[3].style.fg, Some(Color::Rgb(40, 50, 60)));
        assert_eq!(spans[4].content, "e");
        assert!(spans[4].style.add_modifier.is_empty());
    }

    #[test]
    fn modifiers_and_span_coalescing() {
        let text = parse_lines(lines(&[
            "\x1b[1;3mhi\x1b[K there\x1b[22m italic\x1b[;31m red",
        ]));
        let spans = &text.lines[0].spans;
        assert_eq!(spans.len(), 3);
        assert_eq!(spans[0].content, "hi there");
        assert_eq!(
            spans[0].style.add_modifier,
            Modifier::BOLD | Modifier::ITALIC
        );
        assert_eq!(spans[1].content, " italic");
        assert_eq!(spans[1].style.add_modifier, Modifier::ITALIC);
        assert!(spans[1].style.sub_modifier.is_empty());
        assert_eq!(spans[2].content, " red");
        assert_eq!(spans[2].style.fg, Some(Color::Red));
        assert!(spans[2].style.add_modifier.is_empty());
    }

    #[test]
    fn style_persists_across_lines() {
        let text = parse_lines(lines(&["\x1b[32mline1", "line2", "\x1b[0mline3"]));
        assert_eq!(text.lines[0].spans[0].style.fg, Some(Color::Green));
        assert_eq!(text.lines[1].spans[0].style.fg, Some(Color::Green));
        assert_eq!(text.lines[2].spans[0].style.fg, None);
    }

    #[test]
    fn strips_osc_and_handles_malformed_utf8() {
        let text = parse_lines(lines(&[
            "\x1b]8;;https://example.com\x07link\x1b]8;;\x1b\\",
            "a\x1b[3🦀b\x1b[\x1b",
            "x\r\x07\ty",
        ]));
        assert_eq!(text.lines[0].spans[0].content, "link");
        assert_eq!(text.lines[1].spans[0].content, "a🦀b");
        assert_eq!(text.lines[2].spans[0].content, "x    y");
    }
}
