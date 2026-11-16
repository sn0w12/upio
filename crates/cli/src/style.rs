//! Shared look and feel for the CLI: tones, glyphs, widths, and terminal setup.

use std::sync::OnceLock;
use std::time::Duration;

use unicode_width::UnicodeWidthChar;

/// Semantic colour slots, named after the part of the screen they belong to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Plain,
    Strong,
    Muted,
    Branch,
    Good,
    Warn,
    Bad,
    Accent,
}

pub const ROOT: &str = "◆";
pub const TEE: &str = "├─";
pub const ELBOW: &str = "└─";
pub const PIPE: &str = "│";

const ANSI_RESET: &str = "\x1b[0m";

pub fn color_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        if std::env::var("CLICOLOR_FORCE").is_ok_and(|value| value != "0") {
            return true;
        }
        if std::env::var_os("NO_COLOR").is_some() {
            return false;
        }
        if std::env::var("CLICOLOR").is_ok_and(|value| value == "0") {
            return false;
        }
        if std::env::var("TERM").is_ok_and(|value| value == "dumb") {
            return false;
        }
        std::io::IsTerminal::is_terminal(&std::io::stdout())
    })
}

fn code(tone: Tone) -> Option<&'static str> {
    match tone {
        Tone::Plain => None,
        Tone::Strong => Some("\x1b[1;97m"),
        Tone::Muted | Tone::Branch => Some("\x1b[90m"),
        Tone::Good => Some("\x1b[32m"),
        Tone::Warn => Some("\x1b[33m"),
        Tone::Bad => Some("\x1b[1;91m"),
        Tone::Accent => Some("\x1b[36m"),
    }
}

pub fn paint(text: impl AsRef<str>, tone: Tone) -> String {
    let text = text.as_ref();
    if text.is_empty() {
        return String::new();
    }
    match code(tone) {
        Some(code) if color_enabled() => format!("{code}{text}{ANSI_RESET}"),
        _ => text.to_string(),
    }
}

/// Terminal dimensions, with a readable fallback when stdout is a pipe.
#[derive(Clone, Copy, Debug)]
pub struct Screen {
    pub width: usize,
    pub height: usize,
}

impl Default for Screen {
    fn default() -> Self {
        Self {
            width: 100,
            height: 40,
        }
    }
}

pub fn screen() -> Screen {
    match terminal_size::terminal_size() {
        Some((terminal_size::Width(width), terminal_size::Height(height))) => Screen {
            width: usize::from(width).saturating_sub(1).max(20),
            height: usize::from(height),
        },
        None => Screen::default(),
    }
}

/// Width of `text` on screen, ignoring ANSI escape sequences.
pub fn visible_width(text: &str) -> usize {
    let mut width = 0;
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\x1b' {
            if chars.peek() == Some(&'[') {
                chars.next();
                for next in chars.by_ref() {
                    if ('@'..='~').contains(&next) {
                        break;
                    }
                }
            }
            continue;
        }
        width += UnicodeWidthChar::width(ch).unwrap_or(0);
    }
    width
}

/// Shorten plain text to `max` columns, marking the cut with an ellipsis.
pub fn truncate(text: &str, max: usize) -> String {
    if visible_width(text) <= max {
        return text.to_string();
    }
    if max == 0 {
        return String::new();
    }
    let mut out = String::new();
    let mut width = 0;
    for ch in text.chars() {
        let ch_width = UnicodeWidthChar::width(ch).unwrap_or(0);
        if width + ch_width > max.saturating_sub(1) {
            break;
        }
        out.push(ch);
        width += ch_width;
    }
    out.push('…');
    out
}

/// Shorten plain text from the left, keeping the file name end of a path.
pub fn truncate_tail(text: &str, max: usize) -> String {
    if visible_width(text) <= max {
        return text.to_string();
    }
    if max == 0 {
        return String::new();
    }
    let mut tail: Vec<char> = Vec::new();
    let mut width = 0;
    for ch in text.chars().rev() {
        let ch_width = UnicodeWidthChar::width(ch).unwrap_or(0);
        if width + ch_width > max.saturating_sub(1) {
            break;
        }
        tail.push(ch);
        width += ch_width;
    }
    tail.reverse();
    format!("…{}", tail.into_iter().collect::<String>())
}

pub fn pad_right(text: &str, width: usize) -> String {
    let mut out = text.to_string();
    let visible = visible_width(text);
    if visible < width {
        out.push_str(&" ".repeat(width - visible));
    }
    out
}

pub fn pad_left(text: &str, width: usize) -> String {
    let visible = visible_width(text);
    if visible < width {
        format!("{}{}", " ".repeat(width - visible), text)
    } else {
        text.to_string()
    }
}

/// Compose a left label and a right-aligned value on one line.
pub fn spread(left: String, right: String, width: usize) -> String {
    let left_width = visible_width(&left);
    let right_width = visible_width(&right);
    if left_width + right_width + 1 > width {
        let room = width.saturating_sub(right_width + 1);
        return format!("{} {}", pad_right(&truncate_ansi(&left, room), room), right);
    }
    format!("{}{}", left, " ".repeat(width - left_width - right_width)) + &right
}

fn truncate_ansi(text: &str, max: usize) -> String {
    if visible_width(text) <= max {
        return text.to_string();
    }
    let plain = strip_ansi(text);
    truncate(&plain, max)
}

fn strip_ansi(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\x1b' {
            if chars.peek() == Some(&'[') {
                chars.next();
                for next in chars.by_ref() {
                    if ('@'..='~').contains(&next) {
                        break;
                    }
                }
            }
            continue;
        }
        out.push(ch);
    }
    out
}

pub fn rule(width: usize) -> String {
    paint("─".repeat(width), Tone::Branch)
}

pub fn spinner(elapsed: Duration) -> char {
    const FRAMES: [char; 4] = ['|', '/', '-', '\\'];
    FRAMES[(elapsed.as_millis() / 100) as usize % FRAMES.len()]
}

/// A filled bar of `width` cells. Values outside `0.0..=1.0` are clamped.
pub fn bar(fraction: f64, width: usize, tone: Tone) -> String {
    let fraction = fraction.clamp(0.0, 1.0);
    let filled = (fraction * width as f64).round() as usize;
    let filled = filled.min(width);
    let mut out = paint("█".repeat(filled), tone);
    out.push_str(&paint("░".repeat(width - filled), Tone::Branch));
    out
}

/// True when stdout is a terminal, so live redraws make sense.
pub fn interactive() -> bool {
    std::io::IsTerminal::is_terminal(&std::io::stdout())
}

/// Enable escape sequences on the Windows console. A no-op elsewhere.
pub fn init() {
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::Console::{
            GetConsoleMode, GetStdHandle, SetConsoleMode, ENABLE_VIRTUAL_TERMINAL_PROCESSING,
            STD_OUTPUT_HANDLE,
        };

        if !std::io::IsTerminal::is_terminal(&std::io::stdout()) {
            return;
        }
        unsafe {
            let handle = GetStdHandle(STD_OUTPUT_HANDLE);
            let mut mode = 0;
            if GetConsoleMode(handle, &mut mode) != 0 {
                SetConsoleMode(handle, mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ansi_sequences_do_not_count_toward_width() {
        let painted = paint("clip.mp4", Tone::Good);
        assert_eq!(visible_width(&painted), 8);
    }

    #[test]
    fn truncation_keeps_the_column_budget() {
        assert_eq!(truncate("abcdef", 6), "abcdef");
        assert_eq!(visible_width(&truncate("abcdef", 4)), 4);
        assert!(truncate("abcdef", 4).ends_with('…'));
    }

    #[test]
    fn tail_truncation_keeps_the_file_name() {
        let trimmed = truncate_tail("C:/videos/album/clip.mp4", 14);
        assert!(trimmed.starts_with('…'));
        assert!(trimmed.ends_with("clip.mp4"));
        assert_eq!(visible_width(&trimmed), 14);
    }

    #[test]
    fn wide_characters_take_two_columns() {
        assert_eq!(visible_width("映像物.mp4"), 10);
    }

    #[test]
    fn spread_reserves_the_right_hand_side() {
        let line = spread("left".to_string(), "right".to_string(), 20);
        assert_eq!(visible_width(&line), 20);
    }
}
