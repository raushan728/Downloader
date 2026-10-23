use std::fmt::Display;

use colored::Colorize;

/// Display theme mode for character sets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeMode {
    Unicode,
    Ascii,
}

impl ThemeMode {
    /// Detects the optimal theme mode based on terminal capabilities.
    pub fn detect(preference: &str) -> Self {
        match preference.to_lowercase().as_str() {
            "ascii" => ThemeMode::Ascii,
            "unicode" => ThemeMode::Unicode,
            _ => {
                if supports_unicode::on(supports_unicode::Stream::Stdout) {
                    ThemeMode::Unicode
                } else {
                    ThemeMode::Ascii
                }
            }
        }
    }
}

/// Color capability level for terminal styling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorLevel {
    TrueColor,
    Ansi256,
    Ansi16,
    None,
}

impl ColorLevel {
    /// Detects color capability level adhering to NO_COLOR and terminal traits.
    pub fn detect() -> Self {
        if std::env::var("NO_COLOR").is_ok_and(|v| !v.is_empty()) {
            return ColorLevel::None;
        }
        match supports_color::on(supports_color::Stream::Stdout) {
            Some(support) if support.has_16m => ColorLevel::TrueColor,
            Some(support) if support.has_256 => ColorLevel::Ansi256,
            Some(support) if support.has_basic => ColorLevel::Ansi16,
            _ => ColorLevel::None,
        }
    }
}

pub const SPINNER_UNICODE: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
pub const SPINNER_ASCII: &[&str] = &["|", "/", "-", "\\"];

pub const BLOCK_CHARS_UNICODE: &[char] = &[' ', '▏', '▎', '▍', '▌', '▋', '▊', '▉', '█'];
pub const BLOCK_CHARS_ASCII: &[char] = &[' ', '#'];

pub const SPARKLINE_UNICODE: &[char] = &['\u{2581}', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
pub const SPARKLINE_ASCII: &[char] = &['_', '.', '-', '=', '^'];

pub const CHECK_UNICODE: &str = "✓";
pub const CHECK_ASCII: &str = "+";

pub const CROSS_UNICODE: &str = "✗";
pub const CROSS_ASCII: &str = "x";

pub const ARROW_UNICODE: &str = "›";
pub const ARROW_ASCII: &str = ">";

pub const BOX_TL_UNICODE: &str = "┌";
pub const BOX_TR_UNICODE: &str = "┐";
pub const BOX_BL_UNICODE: &str = "└";
pub const BOX_BR_UNICODE: &str = "┘";
pub const BOX_H_UNICODE: &str = "─";
pub const BOX_V_UNICODE: &str = "│";

pub const BOX_TL_ASCII: &str = "+";
pub const BOX_TR_ASCII: &str = "+";
pub const BOX_BL_ASCII: &str = "+";
pub const BOX_BR_ASCII: &str = "+";
pub const BOX_H_ASCII: &str = "-";
pub const BOX_V_ASCII: &str = "|";

pub const TICK_MS: u64 = 80;
pub const STALL_THRESHOLD: std::time::Duration = std::time::Duration::from_secs(2);
pub const SAMPLE_INTERVAL_MS: u64 = 250;
pub const BAR_WIDTH_MAX: usize = 30;
pub const BREAKPOINT_WIDE: usize = 85;
pub const BREAKPOINT_TABLE: usize = 75;
pub const BREAKPOINT_COMPACT: usize = 65;
pub const BREAKPOINT_NARROW: usize = 45;
pub const SPARKLINE_SAMPLES: usize = 20;

/// Returns symbol for checkmark.
pub fn symbol_check(theme: &ThemeMode) -> &'static str {
    match theme {
        ThemeMode::Unicode => CHECK_UNICODE,
        ThemeMode::Ascii => CHECK_ASCII,
    }
}

/// Returns symbol for cross/error.
pub fn symbol_cross(theme: &ThemeMode) -> &'static str {
    match theme {
        ThemeMode::Unicode => CROSS_UNICODE,
        ThemeMode::Ascii => CROSS_ASCII,
    }
}

/// Returns symbol for arrow.
pub fn symbol_arrow(theme: &ThemeMode) -> &'static str {
    match theme {
        ThemeMode::Unicode => ARROW_UNICODE,
        ThemeMode::Ascii => ARROW_ASCII,
    }
}

/// Box borders matching selected theme.
pub struct BoxChars {
    pub tl: &'static str,
    pub tr: &'static str,
    pub bl: &'static str,
    pub br: &'static str,
    pub h: &'static str,
    pub v: &'static str,
}

pub fn box_chars(theme: &ThemeMode) -> BoxChars {
    match theme {
        ThemeMode::Unicode => BoxChars {
            tl: BOX_TL_UNICODE,
            tr: BOX_TR_UNICODE,
            bl: BOX_BL_UNICODE,
            br: BOX_BR_UNICODE,
            h: BOX_H_UNICODE,
            v: BOX_V_UNICODE,
        },
        ThemeMode::Ascii => BoxChars {
            tl: BOX_TL_ASCII,
            tr: BOX_TR_ASCII,
            bl: BOX_BL_ASCII,
            br: BOX_BR_ASCII,
            h: BOX_H_ASCII,
            v: BOX_V_ASCII,
        },
    }
}

/// Formats text with a cyan-to-green gradient according to completion ratio (0.0 to 1.0).
pub fn style_cyan_to_green<T: Display>(text: T, ratio: f64, color: ColorLevel) -> String {
    let r_clamped = ratio.clamp(0.0, 1.0);
    match color {
        ColorLevel::TrueColor => {
            // Interpolate from Cyan (0, 255, 255) to Green (0, 255, 0)
            let blue = ((1.0 - r_clamped) * 255.0) as u8;
            format!("\x1b[38;2;0;255;{}m{}\x1b[0m", blue, text)
        }
        ColorLevel::Ansi256 => {
            // 51 is cyan, 46 is green in 256-color palette
            let code = if r_clamped >= 0.85 {
                46
            } else if r_clamped >= 0.5 {
                48
            } else {
                51
            };
            format!("\x1b[38;5;{}m{}\x1b[0m", code, text)
        }
        ColorLevel::Ansi16 => {
            let s = text.to_string();
            if r_clamped >= 0.85 {
                s.green().to_string()
            } else {
                s.cyan().to_string()
            }
        }
        ColorLevel::None => text.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_theme_mode_detection() {
        assert_eq!(ThemeMode::detect("ascii"), ThemeMode::Ascii);
        assert_eq!(ThemeMode::detect("unicode"), ThemeMode::Unicode);
    }

    #[test]
    fn test_symbols() {
        assert_eq!(symbol_check(&ThemeMode::Unicode), "✓");
        assert_eq!(symbol_check(&ThemeMode::Ascii), "+");
        assert_eq!(symbol_cross(&ThemeMode::Unicode), "✗");
        assert_eq!(symbol_cross(&ThemeMode::Ascii), "x");
    }

    #[test]
    fn test_gradient_none() {
        let s = style_cyan_to_green("hello", 0.5, ColorLevel::None);
        assert_eq!(s, "hello");
    }

    #[test]
    fn test_gradient_truecolor() {
        let s = style_cyan_to_green("test", 1.0, ColorLevel::TrueColor);
        assert!(s.contains("\x1b[38;2;0;255;0m"));
    }
}
