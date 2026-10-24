use std::time::Duration;

use colored::Colorize;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::ui::UiContext;
use crate::ui::progress::{format_bytes, format_speed, render_sparkline};
use crate::ui::text::{sanitize_display, truncate_display};
use crate::ui::theme::{ColorLevel, ThemeMode, box_chars, symbol_check, symbol_cross};

pub fn wrap_text(s: &str, max_w: usize) -> Vec<String> {
    if max_w == 0 {
        return vec![String::new()];
    }
    let mut lines = Vec::new();
    for raw_line in s.lines() {
        let mut cur_line = String::new();
        let mut cur_w = 0;
        for word in raw_line.split_whitespace() {
            let word_w = UnicodeWidthStr::width(word);
            if word_w <= max_w {
                if cur_w == 0 {
                    cur_line.push_str(word);
                    cur_w = word_w;
                } else if cur_w + 1 + word_w <= max_w {
                    cur_line.push(' ');
                    cur_line.push_str(word);
                    cur_w += 1 + word_w;
                } else {
                    lines.push(cur_line);
                    cur_line = word.to_string();
                    cur_w = word_w;
                }
            } else {
                if cur_w > 0 {
                    lines.push(cur_line);
                    cur_line = String::new();
                    cur_w = 0;
                }
                for ch in word.chars() {
                    let ch_w = UnicodeWidthChar::width(ch).unwrap_or(1);
                    if cur_w + ch_w > max_w && !cur_line.is_empty() {
                        lines.push(cur_line);
                        cur_line = String::new();
                        cur_w = 0;
                    }
                    cur_line.push(ch);
                    cur_w += ch_w;
                }
            }
        }
        if !cur_line.is_empty() {
            lines.push(cur_line);
        }
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

/// Renders a boxed completion summary card upon download success with responsive width.
#[allow(clippy::too_many_arguments)]
pub fn render_completion_card_with_width(
    title: &str,
    size_bytes: u64,
    elapsed: Duration,
    avg_speed: f64,
    peak_speed: f64,
    path: &str,
    sparkline_samples: &[f64],
    theme: &ThemeMode,
    color: ColorLevel,
    term_width: usize,
) -> String {
    let b = box_chars(theme);
    let check = symbol_check(theme);

    let secs = elapsed.as_secs();
    let time_str = if secs >= 3600 {
        format!("{}h {}m {}s", secs / 3600, (secs % 3600) / 60, secs % 60)
    } else if secs >= 60 {
        format!("{}m {}s", secs / 60, secs % 60)
    } else {
        format!("{}s", secs)
    };

    let size_str = format_bytes(size_bytes);
    let avg_speed_str = format_speed(avg_speed);
    let peak_speed_str = format_speed(peak_speed);
    let sparkline = render_sparkline(sparkline_samples, theme);

    let max_inner_w: usize = term_width.saturating_sub(4).clamp(30, 72);
    let clean_title = sanitize_display(title);
    let title_display = truncate_display(&clean_title, max_inner_w.saturating_sub(10));
    let clean_path = sanitize_display(path);
    let path_display = truncate_display(&clean_path, max_inner_w.saturating_sub(10));
    let sparkline_display = truncate_display(&sparkline, max_inner_w.saturating_sub(10));
    let speed_val = if max_inner_w >= 50 {
        format!("{} (avg) | {} (peak)", avg_speed_str, peak_speed_str)
    } else {
        format!("{} (avg)", avg_speed_str)
    };

    let h_line = b.h.repeat(max_inner_w + 2);
    let top = format!("{}{}{}", b.tl, h_line, b.tr);
    let bot = format!("{}{}{}", b.bl, h_line, b.br);

    let format_line = |label: &str, value: &str| -> String {
        let prefix = format!("  {:<7} {}", label, value);
        let pad = (max_inner_w + 2).saturating_sub(UnicodeWidthStr::width(prefix.as_str()));
        format!("{}{}{}{}", b.v, prefix, " ".repeat(pad), b.v)
    };

    let header_text = format!("  {} Download Complete", check);
    let header_pad = (max_inner_w + 2).saturating_sub(UnicodeWidthStr::width(header_text.as_str()));
    let header_line = format!("{}{}{}{}", b.v, header_text, " ".repeat(header_pad), b.v);
    let empty_line = format!("{}{}{}", b.v, " ".repeat(max_inner_w + 2), b.v);

    let mut lines = vec![
        top,
        header_line,
        empty_line.clone(),
        format_line("Title:", &title_display),
        format_line("Size:", &size_str),
        format_line("Time:", &time_str),
        format_line("Speed:", &speed_val),
    ];
    if !sparkline_display.is_empty() {
        lines.push(format_line("Graph:", &sparkline_display));
    }
    lines.push(format_line("Path:", &path_display));
    lines.push(bot);

    let card_str = lines.join("\n");
    match color {
        ColorLevel::None => card_str,
        _ => card_str.green().to_string(),
    }
}

/// Renders a boxed completion summary card using live terminal width.
#[allow(clippy::too_many_arguments)]
pub fn render_completion_card(
    title: &str,
    size_bytes: u64,
    elapsed: Duration,
    avg_speed: f64,
    peak_speed: f64,
    path: &str,
    sparkline_samples: &[f64],
    theme: &ThemeMode,
    color: ColorLevel,
) -> String {
    let term_w = UiContext::current_width();
    render_completion_card_with_width(
        title,
        size_bytes,
        elapsed,
        avg_speed,
        peak_speed,
        path,
        sparkline_samples,
        theme,
        color,
        term_w,
    )
}

/// Returns an actionable troubleshooting hint for a given engine error string.
pub fn hint_for_error(error: &str) -> &'static str {
    let lower = error.to_lowercase();
    if lower.contains("429") || lower.contains("too many requests") {
        "Rate limit exceeded (HTTP 429).\nWait and retry, or lower parallel downloads with 'dwdr config set parallel <num>'."
    } else if lower.contains("confirm you're not a bot")
        || lower.contains("confirm you are not a bot")
        || lower.contains("sign in to confirm")
    {
        "Platform bot verification or authentication required.\nProvide cookies via browser session or yt-dlp arguments."
    } else if lower.contains("403") || lower.contains("forbidden") {
        "Stream token expired or platform signature updated.\nTry running 'dwdr update' to refresh core engines."
    } else if lower.contains("ffmpeg") || lower.contains("merger") {
        "Media merger engine missing or broken.\nRun 'dwdr update' to fetch prebuilt binaries."
    } else if lower.contains("private") || lower.contains("sign in") {
        "This media is private or requires account authentication."
    } else if lower.contains("geo")
        || lower.contains("country")
        || lower.contains("not available in your region")
    {
        "This media is geo-restricted in your region."
    } else if lower.contains("no space left")
        || lower.contains("disk full")
        || lower.contains("enospc")
    {
        "Disk is full. Free disk space or change path with 'dwdr config set path <dir>'."
    } else if lower.contains("connection") || lower.contains("timeout") || lower.contains("network")
    {
        "Network connection issue. Verify your internet connection and retry."
    } else {
        "Verify the URL syntax or check engine status with 'dwdr update'."
    }
}

/// Renders a boxed error card with actionable troubleshooting hints and responsive width.
pub fn render_error_card_with_width(
    title: &str,
    error: &str,
    theme: &ThemeMode,
    color: ColorLevel,
    term_width: usize,
) -> String {
    let clean_title = sanitize_display(title);
    let clean_error = sanitize_display(error);
    let b = box_chars(theme);
    let cross = symbol_cross(theme);

    let hint = hint_for_error(&clean_error);

    let max_inner_w: usize = term_width.saturating_sub(4).clamp(30, 72);
    let title_display = truncate_display(&clean_title, max_inner_w.saturating_sub(10));

    let h_line = b.h.repeat(max_inner_w + 2);
    let top = format!("{}{}{}", b.tl, h_line, b.tr);
    let bot = format!("{}{}{}", b.bl, h_line, b.br);

    let format_line = |label: &str, value: &str| -> String {
        let prefix = format!("  {:<7} {}", label, value);
        let pad = (max_inner_w + 2).saturating_sub(UnicodeWidthStr::width(prefix.as_str()));
        format!("{}{}{}{}", b.v, prefix, " ".repeat(pad), b.v)
    };

    let header_text = format!("  {} Download Failed", cross);
    let header_pad = (max_inner_w + 2).saturating_sub(UnicodeWidthStr::width(header_text.as_str()));
    let header_line = format!("{}{}{}{}", b.v, header_text, " ".repeat(header_pad), b.v);
    let empty_line = format!("{}{}{}", b.v, " ".repeat(max_inner_w + 2), b.v);

    let mut lines = vec![
        top,
        header_line,
        empty_line.clone(),
        format_line("Title:", &title_display),
    ];

    let error_lines = wrap_text(&clean_error, max_inner_w.saturating_sub(10));
    for (i, err_line) in error_lines.iter().enumerate() {
        let label = if i == 0 { "Error:" } else { "" };
        lines.push(format_line(label, err_line));
    }
    lines.push(empty_line);

    let hint_lines = wrap_text(hint, max_inner_w.saturating_sub(10));
    for (i, hint_line) in hint_lines.iter().enumerate() {
        let label = if i == 0 { "Hint:" } else { "" };
        lines.push(format_line(label, hint_line));
    }
    lines.push(bot);

    let card_str = lines.join("\n");
    match color {
        ColorLevel::None => card_str,
        _ => card_str.red().to_string(),
    }
}

/// Renders a boxed error card using live terminal width.
pub fn render_error_card(title: &str, error: &str, theme: &ThemeMode, color: ColorLevel) -> String {
    let term_w = UiContext::current_width();
    render_error_card_with_width(title, error, theme, color, term_w)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::text::visible_width;

    #[test]
    fn test_completion_card_render() {
        let card = render_completion_card(
            "Sample Video",
            15_000_000,
            Duration::from_secs(42),
            350_000.0,
            750_000.0,
            "/home/user/Downloads",
            &[1.0, 2.0, 3.0],
            &ThemeMode::Ascii,
            ColorLevel::None,
        );
        assert!(card.contains("Sample Video"));
        assert!(card.contains("14.3 MiB"));
        assert!(card.contains("42s"));
        assert!(card.contains("/home/user/Downloads"));
    }

    #[test]
    fn test_error_card_hint_403() {
        let card = render_error_card(
            "Forbidden Stream",
            "HTTP Error 403: Forbidden",
            &ThemeMode::Ascii,
            ColorLevel::None,
        );
        assert!(card.contains("Forbidden Stream"));
        assert!(card.contains("dwdr update"));
    }

    #[test]
    fn test_error_card_long_strings_no_distortion() {
        let long_title = "A".repeat(120);
        let long_err = "B".repeat(200);
        let card = render_error_card_with_width(
            &long_title,
            &long_err,
            &ThemeMode::Ascii,
            ColorLevel::None,
            64,
        );
        for line in card.lines() {
            assert_eq!(line.chars().count(), 64);
        }
    }

    #[test]
    fn test_completion_card_cjk_characters() {
        let card = render_completion_card_with_width(
            "日本語タイトル非常に長い動画ファイル名サンプルテストあいうえお",
            50_000_000,
            Duration::from_secs(120),
            400_000.0,
            800_000.0,
            "/home/user/ダウンロード",
            &[],
            &ThemeMode::Unicode,
            ColorLevel::None,
            64,
        );
        for line in card.lines() {
            assert_eq!(line.width(), 64);
        }
    }

    #[test]
    fn test_error_card_all_hint_mappings() {
        let ffmpeg =
            render_error_card("T", "ffmpeg not found", &ThemeMode::Ascii, ColorLevel::None);
        assert!(ffmpeg.contains("Media merger engine"));

        let private =
            render_error_card("T", "Video is private", &ThemeMode::Ascii, ColorLevel::None);
        assert!(private.contains("media is private"));

        let geo = render_error_card("T", "geo restricted", &ThemeMode::Ascii, ColorLevel::None);
        assert!(geo.contains("geo-restricted"));

        let disk = render_error_card("T", "disk full", &ThemeMode::Ascii, ColorLevel::None);
        assert!(disk.contains("Disk is full"));

        let conn = render_error_card(
            "T",
            "connection timeout",
            &ThemeMode::Ascii,
            ColorLevel::None,
        );
        assert!(conn.contains("Network connection issue"));
    }

    #[test]
    fn test_hint_for_error_table_driven() {
        let cases = vec![
            ("HTTP Error 429: Too Many Requests", "Rate limit exceeded"),
            (
                "Server returned too many requests error",
                "Rate limit exceeded",
            ),
            (
                "Please confirm you're not a bot before proceeding",
                "Platform bot verification",
            ),
            (
                "Sign in to confirm you’re not a bot",
                "Platform bot verification",
            ),
            ("ERROR 403: Forbidden stream", "Stream token expired"),
            ("ffmpeg: command not found", "Media merger engine"),
            ("This video is private", "media is private"),
            ("Video not available in your region (geo)", "geo-restricted"),
            ("No space left on device (errno 28)", "Disk is full"),
            ("disk full writing chunk", "Disk is full"),
            ("Error: ENOSPC during write", "Disk is full"),
            // Negative test: "workspace" must not trigger disk full hint
            (
                "Failed to prepare workspace directory",
                "Verify the URL syntax",
            ),
            ("Unknown generic error occurred", "Verify the URL syntax"),
        ];

        for (err, expected_sub) in cases {
            let hint = hint_for_error(err);
            assert!(
                hint.contains(expected_sub),
                "Error {:?} produced hint {:?}, expected to contain {:?}",
                err,
                hint,
                expected_sub
            );
        }
    }

    #[test]
    fn test_card_width_and_cjk_wrapping() {
        let title_cjk = "テスト動画タイトル 日本語テキスト CJK Sample Title";
        let error_cjk =
            "エラーが発生しました: ネットワーク接続がタイムアウトしました。確認してください。";

        for &width in &[40, 48, 52, 64, 100] {
            for &theme in &[ThemeMode::Unicode, ThemeMode::Ascii] {
                // Error card
                let err_card = render_error_card_with_width(
                    title_cjk,
                    error_cjk,
                    &theme,
                    ColorLevel::None,
                    width,
                );
                let lines: Vec<&str> = err_card.lines().collect();
                assert!(!lines.is_empty());
                let first_w = visible_width(lines[0]);
                assert!(
                    first_w <= width,
                    "Card line width {} exceeds terminal width {}",
                    first_w,
                    width
                );
                for (idx, line) in lines.iter().enumerate() {
                    let w = visible_width(line);
                    assert_eq!(
                        w, first_w,
                        "Line {} width {} does not match first line width {} at width {}",
                        idx, w, first_w, width
                    );
                }

                // Completion card
                let comp_card = render_completion_card_with_width(
                    title_cjk,
                    15_000_000,
                    Duration::from_secs(42),
                    350_000.0,
                    750_000.0,
                    "/home/user/ダウンロード/日本語パス",
                    &[1.0, 2.0, 3.0],
                    &theme,
                    ColorLevel::None,
                    width,
                );
                let comp_lines: Vec<&str> = comp_card.lines().collect();
                assert!(!comp_lines.is_empty());
                let comp_first_w = visible_width(comp_lines[0]);
                assert!(
                    comp_first_w <= width,
                    "Completion line width {} exceeds terminal width {}",
                    comp_first_w,
                    width
                );
                for (idx, line) in comp_lines.iter().enumerate() {
                    let w = visible_width(line);
                    assert_eq!(
                        w, comp_first_w,
                        "Completion line {} width {} does not match first line width {} at width {}",
                        idx, w, comp_first_w, width
                    );
                }
            }
        }
    }
}
