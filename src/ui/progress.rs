use std::time::Duration;

use colored::Colorize;

use crate::ui::text::{truncate_display, visible_width};
use crate::ui::theme::{
    BLOCK_CHARS_ASCII, BLOCK_CHARS_UNICODE, BREAKPOINT_COMPACT, BREAKPOINT_NARROW, BREAKPOINT_WIDE,
    ColorLevel, SPARKLINE_ASCII, SPARKLINE_UNICODE, ThemeMode, style_cyan_to_green, symbol_arrow,
    symbol_check,
};

/// Aggregate statistics captured across the duration of a download.
#[derive(Debug, Clone)]
pub struct DownloadStats {
    pub size_bytes: u64,
    pub avg_speed: f64,
    pub peak_speed: f64,
    pub samples: Vec<f64>,
    pub elapsed: Duration,
}

impl Default for DownloadStats {
    fn default() -> Self {
        Self {
            size_bytes: 0,
            avg_speed: 0.0,
            peak_speed: 0.0,
            samples: Vec::new(),
            elapsed: Duration::ZERO,
        }
    }
}

/// Phases of a multi-stream media download.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DownloadPhase {
    #[default]
    Video,
    Audio,
    Merge,
    Extract,
    Finalize,
    Complete,
}

impl DownloadPhase {
    pub fn label(&self) -> &'static str {
        match self {
            DownloadPhase::Video => "Video",
            DownloadPhase::Audio => "Audio",
            DownloadPhase::Merge => "Merge",
            DownloadPhase::Extract => "Extract",
            DownloadPhase::Finalize => "Finalize",
            DownloadPhase::Complete => "Complete",
        }
    }
}

/// Instantaneous snapshot of download progress state for pure rendering.
#[derive(Debug, Clone)]
pub struct ProgressState {
    pub percent: f64,
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
    pub speed_bytes_sec: f64,
    pub eta_seconds: Option<u64>,
    pub sparkline_samples: Vec<f64>,
    pub phase: DownloadPhase,
    pub is_finalizing: bool,
    pub finalizing_msg: Option<String>,
}

impl Default for ProgressState {
    fn default() -> Self {
        Self {
            percent: 0.0,
            downloaded_bytes: 0,
            total_bytes: 0,
            speed_bytes_sec: 0.0,
            eta_seconds: None,
            sparkline_samples: Vec::new(),
            phase: DownloadPhase::Video,
            is_finalizing: false,
            finalizing_msg: None,
        }
    }
}

/// Formats raw byte count into human-readable string (e.g. 45.2 MiB).
pub fn format_bytes(bytes: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = KIB * 1024;
    const GIB: u64 = MIB * 1024;

    if bytes >= GIB {
        format!("{:.2} GiB", bytes as f64 / GIB as f64)
    } else if bytes >= MIB {
        format!("{:.1} MiB", bytes as f64 / MIB as f64)
    } else if bytes >= KIB {
        format!("{:.1} KiB", bytes as f64 / KIB as f64)
    } else {
        format!("{} B", bytes)
    }
}

/// Formats speed into human-readable string (e.g. 2.50 MiB/s).
pub fn format_speed(bytes_per_sec: f64) -> String {
    if bytes_per_sec <= 0.0 {
        return "0.0 B/s".to_string();
    }
    format!("{}/s", format_bytes(bytes_per_sec as u64))
}

/// Formats seconds into ETA string (e.g. 01:23 or 01:23:45).
pub fn format_eta(seconds: Option<u64>) -> String {
    match seconds {
        Some(s) => {
            let h = s / 3600;
            let m = (s % 3600) / 60;
            let sec = s % 60;
            if h > 0 {
                format!("{:02}:{:02}:{:02}", h, m, sec)
            } else {
                format!("{:02}:{:02}", m, sec)
            }
        }
        None => "--:--".to_string(),
    }
}

/// Renders a sparkline graph string from recent speed samples scaled from 0 to max.
pub fn render_sparkline(samples: &[f64], theme: &ThemeMode) -> String {
    if samples.is_empty() {
        return String::new();
    }

    let max = samples.iter().copied().fold(0.0_f64, f64::max);

    let chars = match theme {
        ThemeMode::Unicode => SPARKLINE_UNICODE,
        ThemeMode::Ascii => SPARKLINE_ASCII,
    };
    let num_levels = chars.len();

    let mut result = String::with_capacity(samples.len() * 4);
    for &sample in samples {
        let level = if max <= 0.001 {
            0
        } else {
            let normalized = (sample / max).clamp(0.0, 1.0);
            ((normalized * (num_levels - 1) as f64).round() as usize).min(num_levels - 1)
        };
        result.push(chars[level]);
    }
    result
}

/// Renders phase stepper header (e.g. `Video › Audio › Merge`).
pub fn render_phase_stepper(phase: DownloadPhase, theme: &ThemeMode, color: ColorLevel) -> String {
    let arrow = symbol_arrow(theme);
    let check = symbol_check(theme);
    let effective_phase = if phase == DownloadPhase::Finalize {
        DownloadPhase::Merge
    } else {
        phase
    };

    let format_step = |step_phase: DownloadPhase, name: &str| -> String {
        let is_current = effective_phase == step_phase;
        let is_done = matches!(
            (step_phase, effective_phase),
            (
                DownloadPhase::Video,
                DownloadPhase::Audio | DownloadPhase::Merge | DownloadPhase::Complete,
            ) | (
                DownloadPhase::Audio,
                DownloadPhase::Merge | DownloadPhase::Complete
            ) | (DownloadPhase::Merge, DownloadPhase::Complete)
        );

        if is_done {
            match color {
                ColorLevel::None => format!("[{} {}]", check, name),
                _ => format!("[{} {}]", check, name).green().to_string(),
            }
        } else if is_current {
            match color {
                ColorLevel::None => format!(">{name}<"),
                _ => format!(">{name}<").cyan().bold().to_string(),
            }
        } else {
            match color {
                ColorLevel::None => format!(" {} ", name),
                _ => format!(" {} ", name).bright_black().to_string(),
            }
        }
    };

    let arrow_styled = match color {
        ColorLevel::None => arrow.to_string(),
        _ => arrow.bright_black().to_string(),
    };

    format!(
        "{} {} {} {} {}",
        format_step(DownloadPhase::Video, "Video"),
        arrow_styled,
        format_step(DownloadPhase::Audio, "Audio"),
        arrow_styled,
        format_step(DownloadPhase::Merge, "Merge")
    )
}

/// Pure renderer for the complete foreground download progress line.
/// Combines the optional phase stepper with the responsive progress bar while guaranteeing
/// that the final visible width never exceeds `term_width.saturating_sub(2)` to prevent
/// cursor auto-wrap and soft reflow corruption across terminal resizes.
pub fn render_foreground_frame(
    state: &ProgressState,
    term_width: usize,
    theme: &ThemeMode,
    color: ColorLevel,
    tick: u64,
    show_stepper: bool,
) -> String {
    let safe_width = term_width.saturating_sub(2);
    if show_stepper {
        let current_phase = if state.is_finalizing {
            DownloadPhase::Merge
        } else {
            state.phase
        };
        let stepper = render_phase_stepper(current_phase, theme, color);
        let stepper_width = visible_width(&stepper) + 1;
        if let Some(rem_width) = safe_width.checked_sub(stepper_width)
            && rem_width >= BREAKPOINT_NARROW
        {
            let bar = render_bar_pure(state, rem_width, theme, color, tick);
            let frame = format!("{} {}", stepper, bar);
            if visible_width(&frame) <= safe_width {
                return frame;
            }
        }
    }

    let bar = render_bar_pure(state, safe_width, theme, color, tick);
    if visible_width(&bar) <= safe_width {
        bar
    } else {
        truncate_display(&bar, safe_width)
    }
}

/// Pure progress bar renderer for both direct CLI and queue monitors.
pub fn render_bar_pure(
    state: &ProgressState,
    terminal_width: usize,
    theme: &ThemeMode,
    color: ColorLevel,
    tick: u64,
) -> String {
    let pct_clamped = state.percent.clamp(0.0, 100.0);

    // Indeterminate bouncing bar for media finalization
    if state.is_finalizing {
        let msg = state
            .finalizing_msg
            .as_deref()
            .unwrap_or("Finalizing media tracks...");

        let target_max_w = terminal_width.saturating_sub(1);
        if terminal_width < 45 {
            let prefix = " [Fin] ";
            let avail = target_max_w.saturating_sub(prefix.len());
            let trunc_msg = truncate_display(msg, avail);
            return format!("{}{}", prefix, trunc_msg);
        }

        let bar_width: usize = (terminal_width.saturating_sub(35)).clamp(6, 16);
        let block_size: usize = 3;
        let max_pos = bar_width.saturating_sub(block_size);
        let period = max_pos * 2;
        let step = if period > 0 {
            (tick as usize) % period
        } else {
            0
        };
        let pos = if step < max_pos { step } else { period - step };

        let block_char = match theme {
            ThemeMode::Unicode => '█',
            ThemeMode::Ascii => '#',
        };

        let mut bar_content = String::with_capacity(bar_width);
        for i in 0..bar_width {
            if i >= pos && i < pos + block_size {
                bar_content.push(block_char);
            } else {
                bar_content.push(' ');
            }
        }

        let colored_bar = match color {
            ColorLevel::None => bar_content,
            _ => bar_content.yellow().to_string(),
        };

        let overhead = bar_width + 12;
        let avail_msg = target_max_w.saturating_sub(overhead);
        let trunc_msg = truncate_display(msg, avail_msg);

        let clear_eol = if color != ColorLevel::None {
            "\x1b[K"
        } else {
            ""
        };
        return format!("[{}] 100.0% [{}]{}", colored_bar, trunc_msg, clear_eol);
    }

    let downloaded_str = format_bytes(state.downloaded_bytes);
    let total_str = if state.total_bytes > 0 {
        format_bytes(state.total_bytes)
    } else {
        "Unknown".to_string()
    };
    let speed_str = format_speed(state.speed_bytes_sec);
    let eta_str = format_eta(state.eta_seconds);
    let sparkline = render_sparkline(&state.sparkline_samples, theme);

    if terminal_width >= BREAKPOINT_WIDE {
        let available_bar = (terminal_width.saturating_sub(65)).clamp(8, 20);
        let bar_visual = render_sub_character_bar(pct_clamped, available_bar, theme, color);
        let base_line = format!(
            "[{}] {:>5.1}% [{:>8} / {:>8}] at {:>9} ETA {}",
            bar_visual, pct_clamped, downloaded_str, total_str, speed_str, eta_str
        );
        let base_w = visible_width(&base_line);
        let spark_w = visible_width(&sparkline);
        if !sparkline.is_empty() && terminal_width >= base_w + 2 + spark_w {
            format!("{}  {}", base_line, sparkline)
        } else {
            base_line
        }
    } else if terminal_width >= BREAKPOINT_COMPACT {
        let available_bar = (terminal_width.saturating_sub(48)).clamp(6, 12);
        let bar_visual = render_sub_character_bar(pct_clamped, available_bar, theme, color);
        format!(
            "[{}] {:>5.1}% [{:>8} / {:>8}] ETA {}",
            bar_visual, pct_clamped, downloaded_str, total_str, eta_str
        )
    } else if terminal_width >= BREAKPOINT_NARROW {
        format!(
            "{:>5.1}% [{:>8} / {:>8}] at {:>9}",
            pct_clamped, downloaded_str, total_str, speed_str
        )
    } else {
        format!("{:>5.1}% {}", pct_clamped, speed_str)
    }
}

/// Draws fractional sub-character bar blocks (▏▎▍▌▋▊▉█).
fn render_sub_character_bar(
    percent: f64,
    width: usize,
    theme: &ThemeMode,
    color: ColorLevel,
) -> String {
    let ratio = (percent / 100.0).clamp(0.0, 1.0);
    let total_filled = ratio * (width as f64);
    let full_blocks = total_filled.floor() as usize;
    let fraction = total_filled - (full_blocks as f64);

    let (full_char, sub_chars) = match theme {
        ThemeMode::Unicode => ('█', BLOCK_CHARS_UNICODE),
        ThemeMode::Ascii => ('#', BLOCK_CHARS_ASCII),
    };

    let mut out = String::with_capacity(width * 32);

    for i in 0..full_blocks.min(width) {
        let block_ratio = (i as f64) / (width as f64);
        out.push_str(&style_cyan_to_green(full_char, block_ratio, color));
    }

    if full_blocks < width {
        if theme == &ThemeMode::Unicode {
            let sub_idx = ((fraction * 8.0).round() as usize).min(8);
            if sub_idx > 0 {
                let sub_char = sub_chars[sub_idx];
                let block_ratio = (full_blocks as f64) / (width as f64);
                out.push_str(&style_cyan_to_green(sub_char, block_ratio, color));
            } else {
                out.push(' ');
            }
        } else if fraction >= 0.5 {
            out.push('#');
        } else {
            out.push(' ');
        }

        let remaining = width.saturating_sub(full_blocks + 1);
        for _ in 0..remaining {
            out.push(' ');
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(500), "500 B");
        assert_eq!(format_bytes(1024), "1.0 KiB");
        assert_eq!(format_bytes(15 * 1024 * 1024), "15.0 MiB");
        assert_eq!(format_bytes(2 * 1024 * 1024 * 1024), "2.00 GiB");
    }

    #[test]
    fn test_format_eta() {
        assert_eq!(format_eta(Some(0)), "00:00");
        assert_eq!(format_eta(Some(75)), "01:15");
        assert_eq!(format_eta(Some(3665)), "01:01:05");
        assert_eq!(format_eta(None), "--:--");
    }

    #[test]
    fn test_render_sparkline() {
        let samples = vec![1.0, 2.0, 5.0, 8.0, 4.0];
        let spark = render_sparkline(&samples, &ThemeMode::Unicode);
        assert_eq!(spark.chars().count(), 5);

        let spark_ascii = render_sparkline(&samples, &ThemeMode::Ascii);
        assert_eq!(spark_ascii.chars().count(), 5);
    }

    #[test]
    fn test_render_sparkline_zero_to_max_scaling() {
        let samples = vec![0.0, 50.0, 100.0];
        let spark_uni = render_sparkline(&samples, &ThemeMode::Unicode);
        let chars_uni: Vec<char> = spark_uni.chars().collect();
        assert_eq!(chars_uni[0], '\u{2581}');
        assert_eq!(chars_uni[2], '█');

        let spark_asc = render_sparkline(&samples, &ThemeMode::Ascii);
        let chars_asc: Vec<char> = spark_asc.chars().collect();
        assert_eq!(chars_asc[0], '_');
        assert_eq!(chars_asc[2], '^');
    }

    #[test]
    fn test_render_bar_pure_plain() {
        let state = ProgressState {
            percent: 50.0,
            downloaded_bytes: 25 * 1024 * 1024,
            total_bytes: 50 * 1024 * 1024,
            speed_bytes_sec: 2.5 * 1024.0 * 1024.0,
            eta_seconds: Some(10),
            ..Default::default()
        };

        let rendered = render_bar_pure(&state, 100, &ThemeMode::Ascii, ColorLevel::None, 0);
        assert!(rendered.contains("50.0%"));
        assert!(rendered.contains("25.0 MiB"));
        assert!(rendered.contains("50.0 MiB"));
        assert!(rendered.contains("2.5 MiB/s"));
    }

    #[test]
    fn test_render_bar_pure_responsive_widths() {
        let state = ProgressState {
            percent: 75.0,
            downloaded_bytes: 75 * 1024 * 1024,
            total_bytes: 100 * 1024 * 1024,
            speed_bytes_sec: 5.0 * 1024.0 * 1024.0,
            eta_seconds: Some(5),
            sparkline_samples: vec![1.0, 2.0, 3.0, 4.0, 5.0],
            ..Default::default()
        };

        // Full mode (>= 85 cols): line length must not exceed width
        let r110 = render_bar_pure(&state, 110, &ThemeMode::Ascii, ColorLevel::None, 0);
        assert!(r110.len() <= 110);
        assert!(r110.contains("75.0%"));
        assert!(r110.contains("75.0 MiB"));

        let r90 = render_bar_pure(&state, 90, &ThemeMode::Ascii, ColorLevel::None, 0);
        assert!(r90.len() <= 90);

        // Compact mode (65..84 cols)
        let r70 = render_bar_pure(&state, 70, &ThemeMode::Ascii, ColorLevel::None, 0);
        assert!(r70.len() <= 70);
        assert!(r70.contains("75.0%"));
        assert!(r70.contains("ETA 00:05"));

        // Narrow mode (45..64 cols)
        let r50 = render_bar_pure(&state, 50, &ThemeMode::Ascii, ColorLevel::None, 0);
        assert!(r50.len() <= 50);
        assert!(r50.contains("75.0%"));

        // Micro mode (< 45 cols)
        let r30 = render_bar_pure(&state, 30, &ThemeMode::Ascii, ColorLevel::None, 0);
        assert!(r30.len() <= 30);
        assert!(r30.contains("75.0%"));
    }

    #[test]
    fn test_render_bar_pure_finalizing() {
        let state = ProgressState {
            percent: 100.0,
            is_finalizing: true,
            finalizing_msg: Some("Packaging final media...".to_string()),
            ..Default::default()
        };

        let wide = render_bar_pure(&state, 80, &ThemeMode::Ascii, ColorLevel::None, 2);
        assert!(wide.contains("100.0%"));
        assert!(wide.contains("Packaging final media..."));

        let narrow = render_bar_pure(&state, 40, &ThemeMode::Ascii, ColorLevel::None, 2);
        assert!(narrow.contains("[Fin"));
        assert!(visible_width(&narrow) <= 39);
    }

    #[test]
    fn test_render_bar_pure_finalizing_loop_widths() {
        let state = ProgressState {
            percent: 100.0,
            is_finalizing: true,
            finalizing_msg: Some(
                "Very long finalization message that should be truncated appropriately".to_string(),
            ),
            ..Default::default()
        };

        for width in 20..=200 {
            for theme in [ThemeMode::Unicode, ThemeMode::Ascii] {
                for color in [ColorLevel::None, ColorLevel::TrueColor] {
                    let line = render_bar_pure(&state, width, &theme, color, 3);
                    let vis = visible_width(&line);
                    assert!(
                        vis < width,
                        "At width {}, theme {:?}, color {:?}: visible width {} exceeds max {}",
                        width,
                        theme,
                        color,
                        vis,
                        width - 1
                    );
                }
            }
        }
    }

    #[test]
    fn test_render_bar_pure_sparkline_fit_in_color_mode() {
        let state = ProgressState {
            percent: 75.0,
            downloaded_bytes: 75 * 1024 * 1024,
            total_bytes: 100 * 1024 * 1024,
            speed_bytes_sec: 5.0 * 1024.0 * 1024.0,
            eta_seconds: Some(5),
            sparkline_samples: vec![1.0, 2.0, 3.0, 4.0, 5.0],
            ..Default::default()
        };

        for width in [85, 100, 120] {
            let line =
                render_bar_pure(&state, width, &ThemeMode::Unicode, ColorLevel::TrueColor, 0);
            let vis = visible_width(&line);
            assert!(
                vis <= width,
                "At width {}: visible width {} exceeds terminal width",
                width,
                vis
            );
            let sparkline = render_sparkline(&state.sparkline_samples, &ThemeMode::Unicode);
            assert!(
                line.contains(&sparkline),
                "At width {}: sparkline {:?} was expected in rendered output {:?}",
                width,
                sparkline,
                line
            );
        }
    }

    #[test]
    fn test_render_phase_stepper() {
        let s_uni =
            render_phase_stepper(DownloadPhase::Audio, &ThemeMode::Unicode, ColorLevel::None);
        assert!(s_uni.contains("✓"));
        assert!(s_uni.contains(">Audio<"));

        let s_asc = render_phase_stepper(DownloadPhase::Audio, &ThemeMode::Ascii, ColorLevel::None);
        assert!(s_asc.contains(symbol_check(&ThemeMode::Ascii)));
        assert!(s_asc.contains(">Audio<"));

        let s_merge =
            render_phase_stepper(DownloadPhase::Merge, &ThemeMode::Unicode, ColorLevel::None);
        assert!(s_merge.contains(">Merge<"));
        assert!(s_merge.contains("[✓ Video]"));
        assert!(s_merge.contains("[✓ Audio]"));

        let s_fin = render_phase_stepper(
            DownloadPhase::Finalize,
            &ThemeMode::Unicode,
            ColorLevel::None,
        );
        assert!(s_fin.contains(">Merge<"));
        assert!(s_fin.contains("[✓ Video]"));
        assert!(s_fin.contains("[✓ Audio]"));
    }

    #[test]
    fn test_format_speed() {
        assert_eq!(format_speed(0.0), "0.0 B/s");
        assert_eq!(format_speed(500.0), "500 B/s");
        assert_eq!(format_speed(1500.0), "1.5 KiB/s");
        assert_eq!(format_speed(10.0 * 1024.0 * 1024.0), "10.0 MiB/s");
    }

    #[test]
    fn test_render_foreground_frame_responsive_widths() {
        let themes = [ThemeMode::Unicode, ThemeMode::Ascii];
        let colors = [ColorLevel::None, ColorLevel::TrueColor];
        let phases = [
            DownloadPhase::Video,
            DownloadPhase::Audio,
            DownloadPhase::Finalize,
        ];
        let steppers = [true, false];

        for term_w in 20..=200 {
            for theme in &themes {
                for color in colors {
                    for phase in phases {
                        for show_stepper in steppers {
                            let state = ProgressState {
                                percent: 50.0,
                                downloaded_bytes: 25 * 1024 * 1024,
                                total_bytes: 50 * 1024 * 1024,
                                speed_bytes_sec: 2.5 * 1024.0 * 1024.0,
                                eta_seconds: Some(10),
                                sparkline_samples: vec![1.0, 2.0, 3.0, 4.0, 5.0],
                                phase,
                                is_finalizing: phase == DownloadPhase::Finalize,
                                finalizing_msg: if phase == DownloadPhase::Finalize {
                                    Some("Finalizing media...".to_string())
                                } else {
                                    None
                                },
                            };

                            let frame = render_foreground_frame(
                                &state,
                                term_w,
                                theme,
                                color,
                                0,
                                show_stepper,
                            );
                            let vis = visible_width(&frame);
                            let safe_w = term_w.saturating_sub(2);
                            assert!(
                                vis <= safe_w,
                                "At width {}: visible width {} exceeds safe width {}",
                                term_w,
                                vis,
                                safe_w
                            );
                        }
                    }
                }
            }
        }

        let video_state = ProgressState {
            percent: 50.0,
            downloaded_bytes: 50 * 1024 * 1024,
            total_bytes: 100 * 1024 * 1024,
            speed_bytes_sec: 2.5 * 1024.0 * 1024.0,
            eta_seconds: Some(20),
            sparkline_samples: vec![1.0, 2.0],
            phase: DownloadPhase::Video,
            is_finalizing: false,
            finalizing_msg: None,
        };
        let frame_120 = render_foreground_frame(
            &video_state,
            120,
            &ThemeMode::Unicode,
            ColorLevel::None,
            0,
            true,
        );
        assert!(frame_120.contains(">Video<"));

        let frame_50 = render_foreground_frame(
            &video_state,
            50,
            &ThemeMode::Unicode,
            ColorLevel::None,
            0,
            true,
        );
        assert!(!frame_50.contains(">Video<"));
    }
}
