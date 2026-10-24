//! Terminal text sanitization, ANSI escape handling, and display-width measurement.
//!
//! Provides a security boundary against untrusted external strings (video titles,
//! format descriptions, filenames, and engine error streams) to protect terminal emulators
//! against escape-sequence injection, cursor hijacking, and line-wrapping desynchronization.

use unicode_width::UnicodeWidthStr;

/// Strips ANSI escape sequences and C1 control characters from terminal strings.
///
/// Handles CSI (`\x1b[`), OSC (`\x1b]`), DCS (`\x1bP`), APC (`\x1b_`), PM (`\x1b^`),
/// standard 2-character escapes, and raw C1 control codes (`\u{0080}`..=`\u{009F}`).
/// Sequences are consumed until standard terminators: BEL (`\x07`), 8-bit ST (`\u{009c}`),
/// or 7-bit ST (`\x1b\\`).
///
/// # Examples
///
/// ```
/// use dwdr::ui::text::strip_ansi;
///
/// let styled = "\x1b[31;1mError:\x1b[0m \x1b]0;Title\x07message\x1b[2K";
/// assert_eq!(strip_ansi(styled), "Error: message");
/// ```
pub fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '\x1b' {
            match chars.peek() {
                Some('[') => {
                    chars.next();
                    // CSI terminates at 0x40..=0x7E (@..=~) per ECMA-48.
                    for c in chars.by_ref() {
                        if ('@'..='~').contains(&c) {
                            break;
                        }
                    }
                }
                Some(']') | Some('P') | Some('_') | Some('^') => {
                    chars.next();
                    // OSC/DCS/APC/PM terminate at BEL (\x07), 8-bit ST (\u{009c}), or 7-bit ST (\x1b\\).
                    while let Some(c) = chars.next() {
                        if c == '\x07' || c == '\u{009c}' {
                            break;
                        }
                        if c == '\x1b' && chars.peek() == Some(&'\\') {
                            chars.next();
                            break;
                        }
                    }
                }
                Some('(') | Some(')') | Some('*') | Some('+') => {
                    chars.next();
                    chars.next();
                }
                Some(&c) if ('@'..='_').contains(&c) => {
                    chars.next();
                }
                _ => {}
            }
        } else if ('\u{0080}'..='\u{009F}').contains(&ch) {
            continue;
        } else {
            out.push(ch);
        }
    }

    out
}

/// Measures the display column width of a string after stripping ANSI escape sequences.
///
/// Uses `unicode-width` rules to account for double-width characters (e.g. CJK ideographs)
/// and zero-width non-spacing glyphs, returning the physical columns consumed on a terminal grid.
///
/// # Examples
///
/// ```
/// use dwdr::ui::text::visible_width;
///
/// assert_eq!(visible_width("\x1b[32mHello\x1b[0m"), 5);
/// assert_eq!(visible_width("こんにちは"), 10);
/// ```
pub fn visible_width(s: &str) -> usize {
    if !s.as_bytes().contains(&0x1b) {
        return UnicodeWidthStr::width(s);
    }
    let clean = strip_ansi(s);
    UnicodeWidthStr::width(clean.as_str())
}

/// Truncates a string to fit within `max_w` display columns, appending `...` if truncated.
///
/// Preserves intact ANSI styling sequences without charging them against visible display
/// columns, and emits a trailing `\x1b[0m` reset when truncating styled strings to prevent
/// color bleeding across subsequent terminal lines.
///
/// For edge widths:
/// - `max_w == 0` returns an empty string.
/// - `max_w <= 3` returns `.` repeated `max_w` times without underflowing.
///
/// # Examples
///
/// ```
/// use dwdr::ui::text::truncate_display;
///
/// assert_eq!(truncate_display("Hello Beautiful World", 10), "Hello B...");
/// assert_eq!(truncate_display("Tiny", 2), "..");
/// ```
pub fn truncate_display(s: &str, max_w: usize) -> String {
    if visible_width(s) <= max_w {
        return s.to_string();
    }
    if max_w == 0 {
        return String::new();
    }
    if max_w <= 3 {
        return ".".repeat(max_w);
    }
    let target_w = max_w.saturating_sub(3);
    let mut cur = 0;
    let mut truncated = String::new();
    let mut has_ansi = false;
    let mut chars = s.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '\x1b' {
            has_ansi = true;
            truncated.push(ch);
            match chars.peek() {
                Some('[') => {
                    truncated.push(chars.next().unwrap());
                    for c in chars.by_ref() {
                        truncated.push(c);
                        if ('@'..='~').contains(&c) {
                            break;
                        }
                    }
                }
                Some(']') | Some('P') | Some('_') | Some('^') => {
                    truncated.push(chars.next().unwrap());
                    while let Some(c) = chars.next() {
                        truncated.push(c);
                        if c == '\x07' || c == '\u{009c}' {
                            break;
                        }
                        if c == '\x1b' && chars.peek() == Some(&'\\') {
                            truncated.push(chars.next().unwrap());
                            break;
                        }
                    }
                }
                Some('(') | Some(')') | Some('*') | Some('+') => {
                    if let Some(c1) = chars.next() {
                        truncated.push(c1);
                    }
                    if let Some(c2) = chars.next() {
                        truncated.push(c2);
                    }
                }
                Some(&c) if ('@'..='_').contains(&c) => {
                    truncated.push(chars.next().unwrap());
                }
                _ => {}
            }
        } else {
            let ch_w = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
            if cur + ch_w > target_w {
                break;
            }
            cur += ch_w;
            truncated.push(ch);
        }
    }

    if has_ansi {
        format!("{}...\x1b[0m", truncated)
    } else {
        format!("{}...", truncated)
    }
}

/// Sanitizes display text by stripping escape sequences, control characters, and collapsing whitespace.
///
/// Used at ingest boundaries for untrusted strings (video titles from JSON, format notes,
/// and child process stderr lines) before putting them in terminal titles, summaries, or pickers.
///
/// # Examples
///
/// ```
/// use dwdr::ui::text::sanitize_display;
///
/// let injection = "My Video\x1b[2J\x1b]0;pwned\x07  (1080p)\r\n";
/// assert_eq!(sanitize_display(injection), "My Video (1080p)");
/// ```
pub fn sanitize_display(s: &str) -> String {
    let stripped = strip_ansi(s);
    let mut result = String::with_capacity(stripped.len());
    let mut in_whitespace = false;

    for ch in stripped.chars() {
        if ch.is_whitespace() {
            if !in_whitespace {
                result.push(' ');
                in_whitespace = true;
            }
        } else if ch.is_control() || ('\u{0080}'..='\u{009F}').contains(&ch) {
            continue;
        } else {
            result.push(ch);
            in_whitespace = false;
        }
    }

    result.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_ansi_csi_and_osc() {
        let text = "\x1b[31;1mError:\x1b[0m \x1b]0;Title\x07message\x1b[2K";
        assert_eq!(strip_ansi(text), "Error: message");
    }

    #[test]
    fn test_visible_width_calculation() {
        let plain = "Hello World";
        assert_eq!(visible_width(plain), 11);

        let colored = "\x1b[32mHello\x1b[0m \x1b[1mWorld\x1b[0m";
        assert_eq!(visible_width(colored), 11);

        // Japanese double-width characters
        let cjk = "\x1b[36mこんにちは\x1b[0m";
        assert_eq!(visible_width(cjk), 10);
    }

    #[test]
    fn test_sanitize_display_injection_payload() {
        let injection = "\u{001b}[2J\u{001b}]0;pwned\u{0007}";
        assert_eq!(sanitize_display(injection), "");

        let embedded = "My Video \u{001b}[2J\u{001b}]0;pwned\u{0007} (1080p)";
        assert_eq!(sanitize_display(embedded), "My Video (1080p)");
    }

    #[test]
    fn test_sanitize_display_control_characters_and_whitespace() {
        let messy = "  Line 1\r\nLine 2\t\twith   extra   spaces\u{0085}  ";
        assert_eq!(sanitize_display(messy), "Line 1 Line 2 with extra spaces");
    }

    #[test]
    fn test_strip_ansi_extended_sequences() {
        let dcs = "\x1bP1$tx\x1b\\DCS text";
        assert_eq!(strip_ansi(dcs), "DCS text");

        let apc = "\x1b_Gi=1;payload\x1b\\APC text";
        assert_eq!(strip_ansi(apc), "APC text");

        let pm = "\x1b^pm_cmd\x07PM text";
        assert_eq!(strip_ansi(pm), "PM text");

        let c1 = "C1\u{0080}\u{009F}Text";
        assert_eq!(strip_ansi(c1), "C1Text");
    }

    #[test]
    fn test_truncate_display_ansi_aware() {
        let plain = "Hello Beautiful World";
        assert_eq!(truncate_display(plain, 10), "Hello B...");

        let styled = "\x1b[31mHello Beautiful World\x1b[0m";
        let truncated = truncate_display(styled, 10);
        assert_eq!(visible_width(&truncated), 10);
        assert!(truncated.starts_with("\x1b[31mHello B..."));
        assert!(truncated.ends_with("\x1b[0m"));
    }
}
