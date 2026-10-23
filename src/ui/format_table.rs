use colored::Colorize;

use crate::engine::downloader::MediaFormat;
use crate::ui::text::{truncate_display, visible_width};
use crate::ui::theme::{BREAKPOINT_TABLE, ColorLevel, ThemeMode};

/// Returns the column header row when terminal width is wide (>= BREAKPOINT_TABLE).
pub fn format_table_header(term_width: usize, color: ColorLevel) -> Option<String> {
    if term_width < BREAKPOINT_TABLE {
        return None;
    }
    let header = if term_width >= 100 {
        format!(
            "{:<7} | {:<6} | {:<5} | {:<4} | {:<8} | {:<8}",
            "Type", "Res", "Codec", "Ext", "Size", "Ratio"
        )
    } else {
        format!(
            "{:<7} | {:<6} | {:<5} | {:<4} | {:<8}",
            "Type", "Res", "Codec", "Ext", "Size"
        )
    };
    Some(match color {
        ColorLevel::None => header,
        _ => header.bold().to_string(),
    })
}

/// Computes a relative size bar using block characters proportional to the largest candidate format.
pub fn render_relative_size_bar(
    size_bytes: Option<f64>,
    max_bytes: f64,
    bar_width: usize,
    theme: &ThemeMode,
) -> String {
    let size = match size_bytes {
        Some(s) if s > 0.0 => s,
        _ => return " ".repeat(bar_width),
    };

    if max_bytes <= 0.0 {
        return " ".repeat(bar_width);
    }

    let ratio = (size / max_bytes).clamp(0.0, 1.0);
    let filled_f = ratio * (bar_width as f64);
    let full = filled_f.floor() as usize;
    let fraction = filled_f - (full as f64);

    let block_char = match theme {
        ThemeMode::Unicode => '█',
        ThemeMode::Ascii => '#',
    };

    let mut out = String::with_capacity(bar_width);
    for _ in 0..full.min(bar_width) {
        out.push(block_char);
    }

    if full < bar_width {
        if theme == &ThemeMode::Unicode {
            let sub = if fraction >= 0.75 {
                '▊'
            } else if fraction >= 0.5 {
                '▌'
            } else if fraction >= 0.25 {
                '▎'
            } else {
                ' '
            };
            out.push(sub);
        } else if fraction >= 0.5 {
            out.push('#');
        } else {
            out.push(' ');
        }
        let remaining = bar_width.saturating_sub(full + 1);
        for _ in 0..remaining {
            out.push(' ');
        }
    }

    out
}

/// Identifies the default/recommended index for initial cursor focus.
pub fn find_recommended_index(formats: &[MediaFormat], default_quality: &str) -> usize {
    let target = default_quality.trim().to_lowercase();
    if target == "always ask" || target == "best" {
        return 0;
    }
    formats
        .iter()
        .position(|f| f.resolution.eq_ignore_ascii_case(&target))
        .unwrap_or(0)
}

/// Formats media format entries into table row strings for interactive selection.
pub fn build_format_table_options(
    formats: &[MediaFormat],
    default_quality: &str,
    term_width: usize,
    theme: &ThemeMode,
    color: ColorLevel,
) -> Vec<String> {
    let max_size = formats
        .iter()
        .filter_map(|f| f.filesize)
        .fold(0.0_f64, f64::max);

    let rec_idx = find_recommended_index(formats, default_quality);

    formats
        .iter()
        .enumerate()
        .map(|(idx, f)| {
            let is_audio = f.is_audio();
            let tag = if is_audio { "[AUDIO]" } else { "[VIDEO]" };
            let codec = if is_audio {
                if !f.acodec.is_empty() && f.acodec != "none" {
                    &f.acodec
                } else {
                    "audio"
                }
            } else if !f.vcodec.is_empty() && f.vcodec != "none" {
                &f.vcodec
            } else {
                "video"
            };
            let short_codec = if codec.starts_with("avc") || codec.starts_with("h264") {
                "AVC"
            } else if codec.starts_with("vp9") || codec.starts_with("vp09") {
                "VP9"
            } else if codec.starts_with("av01") || codec.starts_with("av1") {
                "AV1"
            } else if codec.starts_with("mp4a") || codec.starts_with("aac") {
                "AAC"
            } else if codec.starts_with("opus") {
                "Opus"
            } else {
                codec
            };

            let rec_badge = if idx == rec_idx {
                if color != ColorLevel::None {
                    " [R]".green().to_string()
                } else {
                    " [R]".to_string()
                }
            } else {
                String::new()
            };
            let hdr_badge = if f.is_hdr {
                if color != ColorLevel::None {
                    " [HDR]".yellow().to_string()
                } else {
                    " [HDR]".to_string()
                }
            } else {
                String::new()
            };

            let size_str = f.format_filesize();

            let pad_w = |s: &str, target: usize| -> String {
                let w = visible_width(s);
                if w < target {
                    format!("{}{}", s, " ".repeat(target - w))
                } else {
                    s.to_string()
                }
            };

            let res_display = pad_w(&truncate_display(&f.resolution, 6), 6);
            let ext_display = pad_w(&truncate_display(&f.ext, 4), 4);
            let codec_display = pad_w(short_codec, 5);

            if term_width >= 100 {
                let bar = render_relative_size_bar(f.filesize, max_size, 8, theme);
                format!(
                    "{:<7} | {} | {} | {} | {:<8} | {}{}{}",
                    tag,
                    res_display,
                    codec_display,
                    ext_display,
                    size_str,
                    bar,
                    rec_badge,
                    hdr_badge
                )
            } else if term_width >= BREAKPOINT_TABLE {
                format!(
                    "{:<7} | {} | {} | {} | {:<8}{}{}",
                    tag, res_display, codec_display, ext_display, size_str, rec_badge, hdr_badge
                )
            } else if term_width >= 60 {
                let short_tag = if is_audio { "[AUD]" } else { "[VID]" };
                format!(
                    "{:<5} | {} | {} | {:<8}{}{}",
                    short_tag, res_display, ext_display, size_str, rec_badge, hdr_badge
                )
            } else {
                let compact_badge = if idx == rec_idx {
                    if color != ColorLevel::None {
                        " [R]".green().to_string()
                    } else {
                        " [R]".to_string()
                    }
                } else {
                    String::new()
                };
                let res_trunc = pad_w(&truncate_display(&f.resolution, 6), 6);
                let ext_trunc = pad_w(&truncate_display(&f.ext, 3), 3);
                let raw_size = f.format_filesize();
                let short_size = if term_width < 50 {
                    truncate_display(&raw_size, 8)
                } else {
                    raw_size
                };
                format!(
                    "{} | {} | {:<8}{}",
                    res_trunc, ext_trunc, short_size, compact_badge
                )
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::text::visible_width;

    #[test]
    fn test_relative_size_bar() {
        let bar = render_relative_size_bar(Some(50.0), 100.0, 8, &ThemeMode::Ascii);
        assert_eq!(bar.chars().count(), 8);
        assert!(bar.starts_with("####"));
    }

    #[test]
    fn test_find_recommended_index() {
        let fmts = vec![
            MediaFormat {
                format_id: "1".into(),
                ext: "mp4".into(),
                resolution: "1080p".into(),
                filesize: Some(100.0),
                is_estimated: false,
                is_hdr: false,
                vcodec: "avc1".into(),
                acodec: "none".into(),
                height: Some(1080),
            },
            MediaFormat {
                format_id: "2".into(),
                ext: "mp4".into(),
                resolution: "720p".into(),
                filesize: Some(50.0),
                is_estimated: false,
                is_hdr: false,
                vcodec: "avc1".into(),
                acodec: "none".into(),
                height: Some(720),
            },
        ];

        assert_eq!(find_recommended_index(&fmts, "720p"), 1);
        assert_eq!(find_recommended_index(&fmts, "1080p"), 0);
        assert_eq!(find_recommended_index(&fmts, "4k"), 0);
    }

    #[test]
    fn test_build_format_table_options_wide_and_compact() {
        let fmts = vec![
            MediaFormat {
                format_id: "137".into(),
                ext: "mp4".into(),
                resolution: "1080p".into(),
                filesize: Some(150_000_000.0),
                is_estimated: false,
                is_hdr: true,
                vcodec: "av01.0.08M.08".into(),
                acodec: "none".into(),
                height: Some(1080),
            },
            MediaFormat {
                format_id: "140".into(),
                ext: "m4a".into(),
                resolution: "Audio".into(),
                filesize: Some(10_000_000.0),
                is_estimated: false,
                is_hdr: false,
                vcodec: "none".into(),
                acodec: "mp4a.40.2".into(),
                height: None,
            },
        ];

        // Wide terminal
        let wide_opts =
            build_format_table_options(&fmts, "1080p", 90, &ThemeMode::Ascii, ColorLevel::None);
        assert_eq!(wide_opts.len(), 2);
        assert!(wide_opts[0].contains("[VIDEO]"));
        assert!(wide_opts[0].contains("AV1"));
        assert!(wide_opts[0].contains("[R]"));
        assert!(wide_opts[0].contains("[HDR]"));
        assert!(wide_opts[1].contains("[AUDIO]"));
        assert!(wide_opts[1].contains("AAC"));

        // Compact terminal (< 75 columns)
        let compact_opts =
            build_format_table_options(&fmts, "1080p", 60, &ThemeMode::Ascii, ColorLevel::None);
        assert_eq!(compact_opts.len(), 2);
        assert!(compact_opts[0].contains("[VID]"));
        assert!(compact_opts[0].contains("[R]"));
        // Compact mode drops codec column
        assert!(!compact_opts[0].contains(" | AV1 | "));
    }

    #[test]
    fn test_format_picker_widths() {
        let fmts = vec![
            MediaFormat {
                format_id: "137".into(),
                ext: "mp4".into(),
                resolution: "1080p".into(),
                filesize: Some(150_000_000.0),
                is_estimated: false,
                is_hdr: true,
                vcodec: "av01.0.08M.08".into(),
                acodec: "none".into(),
                height: Some(1080),
            },
            MediaFormat {
                format_id: "140".into(),
                ext: "m4a".into(),
                resolution: "Audio".into(),
                filesize: Some(10_000_000.0),
                is_estimated: false,
                is_hdr: false,
                vcodec: "none".into(),
                acodec: "mp4a.40.2".into(),
                height: None,
            },
        ];

        for &w in &[40, 60, 80, 100, 140] {
            let opts =
                build_format_table_options(&fmts, "1080p", w, &ThemeMode::Ascii, ColorLevel::None);
            for opt in &opts {
                let vis = visible_width(opt);
                assert!(
                    vis < w - 6,
                    "Option {:?} with width {} exceeds w - 6 ({}) at terminal width {}",
                    opt,
                    vis,
                    w - 6,
                    w
                );
            }
            if w >= BREAKPOINT_TABLE {
                assert!(format_table_header(w, ColorLevel::None).is_some());
            } else {
                assert!(format_table_header(w, ColorLevel::None).is_none());
            }
        }
    }

    #[test]
    fn test_build_format_table_options_non_ascii_and_pending_size() {
        let fmts = vec![
            MediaFormat {
                format_id: "jp1".into(),
                ext: "ｗｅｂｍ".into(),
                resolution: "高画質プレミアム".into(),
                filesize: None,
                is_estimated: false,
                is_hdr: true,
                vcodec: "vp9".into(),
                acodec: "none".into(),
                height: Some(1080),
            },
            MediaFormat {
                format_id: "jp2".into(),
                ext: "mp4".into(),
                resolution: "日本語タイトル".into(),
                filesize: Some(50_000_000.0),
                is_estimated: false,
                is_hdr: false,
                vcodec: "avc1".into(),
                acodec: "none".into(),
                height: Some(720),
            },
        ];

        for &w in &[40, 55, 60, 80, 100] {
            let opts = build_format_table_options(
                &fmts,
                "高画質プレミアム",
                w,
                &ThemeMode::Unicode,
                ColorLevel::TrueColor,
            );
            assert_eq!(opts.len(), 2);
            for opt in &opts {
                let vis = visible_width(opt);
                if w >= 45 {
                    assert!(
                        vis < w - 6,
                        "At width {}: row {:?} with visible width {} is not < {}",
                        w,
                        opt,
                        vis,
                        w - 6
                    );
                }
            }
        }
    }
}
