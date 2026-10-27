use std::collections::HashSet;
use std::io::{BufRead, BufReader, Write, stdout};
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use crossterm::terminal;
use directories::BaseDirs;
use serde::{Deserialize, Serialize};

use crate::config::manager::validate_download_path;
use crate::engine::queue::{clear_active_download, kill_process_by_pid, record_active_download};
use crate::ui::UiContext;
use crate::ui::progress::{
    DownloadPhase, DownloadStats, ProgressState, format_bytes as format_bytes_u64, format_eta,
    format_speed, render_foreground_frame,
};
use crate::ui::smooth::{EasedProgress, PlainProgressThrottle, SpeedSmoother};
use crate::ui::terminal_title::{clear_taskbar_progress, set_taskbar_progress, set_terminal_title};
use crate::ui::text::{sanitize_display, visible_width};
use crate::ui::theme::{SAMPLE_INTERVAL_MS, STALL_THRESHOLD, TICK_MS};

#[derive(Debug, Serialize, Deserialize)]
pub struct VideoInfo {
    pub title: String,
    pub duration: Option<f64>,
    pub formats: Vec<MediaFormat>,
}

/// Outcome of a foreground media download including file path and summary statistics.
#[derive(Debug, Clone)]
pub struct DownloadOutcome {
    pub path: Option<PathBuf>,
    pub stats: DownloadStats,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaFormat {
    pub format_id: String,
    pub ext: String,
    pub resolution: String,
    pub filesize: Option<f64>,
    #[serde(default)]
    pub is_estimated: bool,
    #[serde(default)]
    pub is_hdr: bool,
    pub vcodec: String,
    pub acodec: String,
    #[serde(default)]
    pub height: Option<u32>,
}

pub fn format_bytes(bytes: f64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    const TB: f64 = GB * 1024.0;

    if bytes < KB {
        format!("{:.0} B", bytes)
    } else if bytes < MB {
        format!("{:.1} KB", bytes / KB)
    } else if bytes < GB {
        format!("{:.1} MB", bytes / MB)
    } else if bytes < TB {
        format!("{:.1} GB", bytes / GB)
    } else {
        format!("{:.1} TB", bytes / TB)
    }
}

impl MediaFormat {
    pub fn format_filesize(&self) -> String {
        match self.filesize {
            Some(bytes) => {
                let formatted = format_bytes(bytes);
                if self.is_estimated {
                    format!("~{} (Est.)", formatted)
                } else {
                    formatted
                }
            }
            None => "Stream Size Pending".to_string(),
        }
    }

    pub fn size_in_mb(&self) -> String {
        self.format_filesize()
    }

    pub fn is_audio(&self) -> bool {
        self.resolution.eq_ignore_ascii_case("audio")
            || (self.vcodec == "none" && self.acodec != "none")
    }

    /// Formats resolution, container extension, and filesize into a clean user-facing description.
    pub fn summary(&self) -> String {
        let size_str = self.format_filesize();
        let s = if size_str == "Stream Size Pending" {
            if self.ext.is_empty() {
                self.resolution.clone()
            } else {
                format!("{} ({})", self.resolution, self.ext)
            }
        } else if self.ext.is_empty() {
            format!("{} [{}]", self.resolution, size_str)
        } else {
            format!("{} ({}, {})", self.resolution, self.ext, size_str)
        };
        sanitize_display(&s)
    }
}

pub fn calculate_media_filesize(
    exact: Option<f64>,
    approx: Option<f64>,
    bitrate_kbps: Option<f64>,
    duration: Option<f64>,
    height: Option<u64>,
    is_video: bool,
) -> (Option<f64>, bool) {
    if let Some(exact_bytes) = exact
        && exact_bytes > 0.0
    {
        return (Some(exact_bytes), false);
    }

    if let Some(approx_bytes) = approx
        && approx_bytes > 0.0
    {
        return (Some(approx_bytes), true);
    }

    if let Some(dur) = duration
        && dur > 0.0
    {
        if let Some(kbps) = bitrate_kbps {
            let bytes = (kbps * 1000.0 / 8.0) * dur;
            return (Some(bytes), true);
        }

        let typical_kbps = if is_video {
            match height.unwrap_or(0) {
                2160.. => 12000.0,
                1440..=2159 => 6000.0,
                1080..=1439 => 2500.0,
                720..=1079 => 1500.0,
                480..=719 => 800.0,
                360..=479 => 450.0,
                _ => 250.0,
            }
        } else {
            128.0
        };

        let bytes = (typical_kbps * 1000.0 / 8.0) * dur;
        return (Some(bytes), true);
    }

    (None, false)
}

pub fn map_height_to_resolution(height: u64) -> Option<&'static str> {
    match height {
        4320.. => Some("8K"),
        2000..=4319 => Some("4K"),
        1300..=1999 => Some("2K"),
        900..=1299 => Some("1080p"),
        600..=899 => Some("720p"),
        400..=599 => Some("480p"),
        300..=399 => Some("360p"),
        200..=299 => Some("240p"),
        120..=199 => Some("144p"),
        _ => None,
    }
}

/// Normalizes resolution labels across widescreen, ultrawide letterboxing, and vertical video formats.
pub fn resolve_resolution(
    height: Option<u64>,
    width: Option<u64>,
    format_note: Option<&str>,
) -> Option<(&'static str, u64)> {
    if let Some(note) = format_note {
        let lower = note.to_lowercase();
        if lower.contains("4320") || lower.contains("8k") {
            return Some(("8K", 4320));
        } else if lower.contains("2160") || lower.contains("4k") {
            return Some(("4K", 2160));
        } else if lower.contains("1440") || lower.contains("2k") {
            return Some(("2K", 1440));
        } else if lower.contains("1080") {
            return Some(("1080p", 1080));
        } else if lower.contains("720") {
            return Some(("720p", 720));
        } else if lower.contains("480") {
            return Some(("480p", 480));
        } else if lower.contains("360") {
            return Some(("360p", 360));
        } else if lower.contains("240") {
            return Some(("240p", 240));
        } else if lower.contains("144") {
            return Some(("144p", 144));
        }
    }

    // For vertical videos (Shorts/Reels), width corresponds to the standard resolution tier.
    if let (Some(w), Some(h)) = (width, height)
        && h > w
        && let Some(res) = map_height_to_resolution(w)
    {
        let rank = match res {
            "8K" => 4320,
            "4K" => 2160,
            "2K" => 1440,
            "1080p" => 1080,
            "720p" => 720,
            "480p" => 480,
            "360p" => 360,
            "240p" => 240,
            "144p" => 144,
            _ => w,
        };
        return Some((res, rank));
    }

    if let (Some(w), Some(h)) = (width, height)
        && w == h
        && let Some(res) = map_height_to_resolution(h)
    {
        let rank = match res {
            "8K" => 4320,
            "4K" => 2160,
            "2K" => 1440,
            "1080p" => 1080,
            "720p" => 720,
            "480p" => 480,
            "360p" => 360,
            "240p" => 240,
            "144p" => 144,
            _ => h,
        };
        return Some((res, rank));
    }

    let width_candidate = width.and_then(|w| {
        if w >= 7680 {
            Some(("8K", 4320))
        } else if w >= 3840 {
            Some(("4K", 2160))
        } else if w >= 2560 {
            Some(("2K", 1440))
        } else if w >= 1920 {
            Some(("1080p", 1080))
        } else if w >= 1280 {
            Some(("720p", 720))
        } else if w >= 854 {
            Some(("480p", 480))
        } else if w >= 640 {
            Some(("360p", 360))
        } else if w >= 426 {
            Some(("240p", 240))
        } else if w >= 256 {
            Some(("144p", 144))
        } else {
            None
        }
    });

    let height_candidate = height.and_then(|h| {
        map_height_to_resolution(h).map(|res| {
            let rank = match res {
                "8K" => 4320,
                "4K" => 2160,
                "2K" => 1440,
                "1080p" => 1080,
                "720p" => 720,
                "480p" => 480,
                "360p" => 360,
                "240p" => 240,
                "144p" => 144,
                _ => h,
            };
            (res, rank)
        })
    });

    [width_candidate, height_candidate]
        .into_iter()
        .flatten()
        .max_by_key(|(_, rank)| *rank)
}

pub fn extract_engine_error(stderr: &str) -> String {
    let trimmed = stderr.trim();
    if trimmed.is_empty() {
        return "yt-dlp process exited with an error".to_string();
    }

    let line = if let Some(err_line) = trimmed
        .lines()
        .rev()
        .find(|l| l.contains("ERROR:") || l.contains("error:"))
    {
        err_line.trim()
    } else {
        trimmed
            .lines()
            .next_back()
            .map(|l| l.trim())
            .unwrap_or(trimmed)
    };

    let raw_err = if let Some(stripped) = line.strip_prefix("ERROR:") {
        stripped.trim()
    } else if let Some(stripped) = line.strip_prefix("error:") {
        stripped.trim()
    } else {
        line
    };

    let sanitized = sanitize_display(raw_err);
    if sanitized.is_empty() {
        "yt-dlp process exited with an error".to_string()
    } else {
        sanitized
    }
}

fn score_vcodec(vcodec: &str, compat_mode: bool) -> u8 {
    let lower = vcodec.to_lowercase();
    if compat_mode {
        if lower.starts_with("avc1") || lower.starts_with("avc") || lower.starts_with("h264") {
            4
        } else if lower.starts_with("hev1")
            || lower.starts_with("hvc1")
            || lower.starts_with("hevc")
            || lower.starts_with("h265")
        {
            3
        } else if lower.starts_with("vp09") || lower.starts_with("vp9") {
            2
        } else if lower.starts_with("av01") || lower.starts_with("av1") {
            1
        } else {
            0
        }
    } else if lower.starts_with("av01") || lower.starts_with("av1") {
        4
    } else if lower.starts_with("vp09") || lower.starts_with("vp9") {
        3
    } else if lower.starts_with("hev1")
        || lower.starts_with("hvc1")
        || lower.starts_with("hevc")
        || lower.starts_with("h265")
    {
        2
    } else if lower.starts_with("avc1") || lower.starts_with("avc") || lower.starts_with("h264") {
        1
    } else {
        0
    }
}

fn score_acodec(acodec: &str, compat_mode: bool) -> u8 {
    let lower = acodec.to_lowercase();
    if compat_mode {
        if lower.contains("mp4a") || lower.contains("aac") {
            3
        } else if lower.contains("mp3") {
            2
        } else if lower.contains("opus") {
            1
        } else {
            0
        }
    } else if lower.contains("opus") {
        3
    } else if lower.contains("mp4a") || lower.contains("aac") {
        2
    } else if lower.contains("mp3") {
        1
    } else {
        0
    }
}

pub fn parse_video_info_json(stdout_str: &str) -> Result<VideoInfo, String> {
    parse_video_info_json_ext(stdout_str, false)
}

pub fn parse_video_info_json_ext(stdout_str: &str, compat_mode: bool) -> Result<VideoInfo, String> {
    let raw: serde_json::Value =
        serde_json::from_str(stdout_str).map_err(|e| format!("Failed to parse JSON: {}", e))?;

    let title = sanitize_display(raw["title"].as_str().unwrap_or("Unknown Title"));
    let duration = raw["duration"].as_f64();

    let empty_formats = Vec::new();
    let raw_formats = raw["formats"].as_array().unwrap_or(&empty_formats);

    let mut video_candidates = Vec::new();
    let mut audio_candidates = Vec::new();

    for f in raw_formats {
        let vcodec_opt = f["vcodec"].as_str();
        let acodec_opt = f["acodec"].as_str();

        let is_vcodec_none = vcodec_opt == Some("none");
        let is_acodec_none = acodec_opt == Some("none");
        let has_vcodec = vcodec_opt.is_some_and(|c| c != "none");
        let has_acodec = acodec_opt.is_some_and(|c| c != "none");

        let ext = sanitize_display(f["ext"].as_str().unwrap_or("unknown"));
        if ext == "mhtml" || ext == "weba" {
            continue;
        }

        let raw_note = f["format_note"].as_str().map(sanitize_display);
        let note = raw_note.as_deref();

        if note.unwrap_or("").to_lowercase().contains("storyboard") {
            continue;
        }

        let mut height = f["height"].as_u64();
        let mut width = f["width"].as_u64();

        if (height.is_none() || width.is_none())
            && let Some(res_str) = f["resolution"].as_str()
        {
            if let Some((w_str, h_str)) = res_str.split_once('x') {
                if width.is_none() {
                    width = w_str.trim().parse::<u64>().ok();
                }
                if height.is_none() {
                    height = h_str.trim().parse::<u64>().ok();
                }
            } else if height.is_none() {
                let cleaned = res_str.trim().trim_end_matches(['p', 'P']);
                height = cleaned.parse::<u64>().ok();
            }
        }

        if height.is_none() {
            let note_str = f["format_note"].as_str().unwrap_or("");
            let id_str = f["format_id"].as_str().unwrap_or("");
            for s in [note_str, id_str] {
                if let Some(h) = s
                    .strip_suffix('p')
                    .or_else(|| s.strip_suffix('P'))
                    .and_then(|v| v.parse::<u64>().ok())
                {
                    height = Some(h);
                    break;
                }
            }
        }

        let is_video = has_vcodec
            || (!is_vcodec_none
                && (height.is_some()
                    || width.is_some()
                    || f["resolution"]
                        .as_str()
                        .is_some_and(|r| !r.eq_ignore_ascii_case("audio only"))
                    || matches!(
                        ext.as_str(),
                        "mp4" | "webm" | "mkv" | "avi" | "flv" | "mov" | "3gp" | "m3u8" | "ts"
                    )));
        let is_audio = (is_vcodec_none && has_acodec)
            || (!is_video
                && !is_acodec_none
                && (has_acodec
                    || matches!(ext.as_str(), "mp3" | "m4a" | "aac" | "wav" | "ogg" | "opus")));

        if !is_video && !is_audio {
            continue;
        }

        let format_id = sanitize_display(f["format_id"].as_str().unwrap_or("unknown"));
        let exact = f["filesize"].as_f64();
        let approx = f["filesize_approx"].as_f64();
        let tbr = f["tbr"].as_f64();
        let vbr = f["vbr"].as_f64();
        let abr = f["abr"].as_f64();
        let protocol = f["protocol"].as_str().unwrap_or("https");
        let is_direct = !protocol.contains("m3u8");
        let is_exact = exact.is_some();

        if is_video {
            let (resolution, rank) = match resolve_resolution(height, width, note) {
                Some(r) => (sanitize_display(r.0), r.1),
                None => {
                    let label = match note {
                        Some(n) if !n.trim().is_empty() => sanitize_display(n.trim()),
                        _ if height.is_some() => format!("{}p", height.unwrap()),
                        _ if !format_id.is_empty() && format_id != "unknown" => format_id.clone(),
                        _ => "Default".to_string(),
                    };
                    (label, height.unwrap_or(500))
                }
            };

            let total_bitrate = tbr.or_else(|| {
                let v = vbr.unwrap_or(0.0);
                let a = abr.unwrap_or(0.0);
                if v + a > 0.0 { Some(v + a) } else { None }
            });
            let (filesize, is_estimated) =
                calculate_media_filesize(exact, approx, total_bitrate, duration, height, true);
            let size_ranking = filesize.unwrap_or(0.0);

            let vcodec_str = if has_vcodec {
                sanitize_display(vcodec_opt.unwrap())
            } else {
                "unknown".to_string()
            };
            let acodec_str = if has_acodec {
                sanitize_display(acodec_opt.unwrap())
            } else if is_acodec_none {
                "none".to_string()
            } else {
                "unknown".to_string()
            };

            let is_hdr = f["dynamic_range"]
                .as_str()
                .map(|d| {
                    let clean = sanitize_display(d);
                    !clean.trim().is_empty() && !clean.eq_ignore_ascii_case("sdr")
                })
                .unwrap_or(false)
                || note.unwrap_or("").to_lowercase().contains("hdr");
            let codec_score = score_vcodec(&vcodec_str, compat_mode);

            video_candidates.push((
                rank,
                is_direct,
                is_hdr,
                codec_score,
                is_exact,
                size_ranking,
                MediaFormat {
                    format_id: format_id.clone(),
                    ext: ext.clone(),
                    resolution,
                    filesize,
                    is_estimated,
                    is_hdr,
                    vcodec: vcodec_str,
                    acodec: acodec_str,
                    height: height.map(|h| h as u32),
                },
            ));
        } else if is_audio {
            let total_bitrate = abr.or(tbr);
            let (filesize, is_estimated) =
                calculate_media_filesize(exact, approx, total_bitrate, duration, height, false);
            let size_ranking = filesize.unwrap_or(0.0);

            let acodec_str = if has_acodec {
                sanitize_display(acodec_opt.unwrap())
            } else {
                "unknown".to_string()
            };
            let codec_score = score_acodec(&acodec_str, compat_mode);

            audio_candidates.push((
                is_direct,
                is_exact,
                codec_score,
                size_ranking,
                MediaFormat {
                    format_id,
                    ext,
                    resolution: "Audio".to_string(),
                    filesize,
                    is_estimated,
                    is_hdr: false,
                    vcodec: "none".to_string(),
                    acodec: acodec_str,
                    height: None,
                },
            ));
        }
    }

    video_candidates.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then_with(|| b.1.cmp(&a.1))
            .then_with(|| b.2.cmp(&a.2))
            .then_with(|| b.3.cmp(&a.3))
            .then_with(|| b.4.cmp(&a.4))
            .then_with(|| b.5.partial_cmp(&a.5).unwrap_or(std::cmp::Ordering::Equal))
    });

    let mut formats = Vec::new();
    let mut seen_resolutions = HashSet::new();

    for (_, _, _, _, _, _, format) in video_candidates {
        if seen_resolutions.insert(format.resolution.clone()) {
            formats.push(format);
        }
    }

    audio_candidates.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then_with(|| b.1.cmp(&a.1))
            .then_with(|| b.2.cmp(&a.2))
            .then_with(|| b.3.partial_cmp(&a.3).unwrap_or(std::cmp::Ordering::Equal))
    });

    if let Some((_, _, _, _, best_audio)) = audio_candidates.into_iter().next() {
        formats.push(best_audio);
    }

    // Synthesize fallback format when platforms return direct unpartitioned media without format tiers.
    if formats.is_empty() {
        let fallback_ext = raw["ext"].as_str().unwrap_or("mp4").to_string();
        let fallback_id = raw["format_id"].as_str().unwrap_or("best").to_string();
        let (filesize, is_estimated) = calculate_media_filesize(
            raw["filesize"].as_f64(),
            raw["filesize_approx"].as_f64(),
            raw["tbr"].as_f64(),
            duration,
            raw["height"].as_u64(),
            true,
        );
        formats.push(MediaFormat {
            format_id: fallback_id,
            ext: fallback_ext,
            resolution: "Default".to_string(),
            filesize,
            is_estimated,
            is_hdr: false,
            vcodec: raw["vcodec"].as_str().unwrap_or("unknown").to_string(),
            acodec: raw["acodec"].as_str().unwrap_or("unknown").to_string(),
            height: raw["height"].as_u64().map(|h| h as u32),
        });
    }

    Ok(VideoInfo {
        title,
        duration,
        formats,
    })
}

/// Returns the first detected installed browser for session cookie extraction.
pub fn detect_available_browser() -> Option<&'static str> {
    static DETECTED_BROWSER: OnceLock<Option<&'static str>> = OnceLock::new();
    *DETECTED_BROWSER.get_or_init(|| {
        let dirs = BaseDirs::new()?;
        #[cfg(target_os = "macos")]
        {
            let home = dirs.home_dir();
            let app_support = home.join("Library/Application Support");
            if app_support.exists() {
                const MACOS_PROFILES: &[(&str, &str)] = &[
                    ("BraveSoftware/Brave-Browser", "brave"),
                    ("Google/Chrome", "chrome"),
                    ("Microsoft Edge", "edge"),
                    ("Firefox", "firefox"),
                ];
                for &(rel, browser) in MACOS_PROFILES {
                    if app_support.join(rel).exists() {
                        return Some(browser);
                    }
                }
            }
        }
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            let home = dirs.home_dir();
            const LINUX_PROFILES: &[(&str, &str)] = &[
                (".config/BraveSoftware/Brave-Browser", "brave"),
                (".config/google-chrome", "chrome"),
                (".config/chromium", "chromium"),
                (".config/microsoft-edge", "edge"),
                (".mozilla/firefox", "firefox"),
                (".config/opera", "opera"),
                (".config/vivaldi", "vivaldi"),
            ];
            for &(rel, browser) in LINUX_PROFILES {
                if home.join(rel).exists() {
                    return Some(browser);
                }
            }
        }
        #[cfg(windows)]
        {
            let local_app_data = dirs.data_local_dir();
            const WIN_LOCAL_PROFILES: &[(&str, &str)] = &[
                ("BraveSoftware/Brave-Browser", "brave"),
                ("Google/Chrome", "chrome"),
                ("Microsoft/Edge", "edge"),
                ("Opera Software", "opera"),
                ("Vivaldi", "vivaldi"),
            ];
            for &(rel, browser) in WIN_LOCAL_PROFILES {
                if local_app_data.join(rel).exists() {
                    return Some(browser);
                }
            }
            if dirs.config_dir().join("Mozilla/Firefox").exists() {
                return Some("firefox");
            }
        }
        None
    })
}

const KNOWN_COOKIE_BROWSERS: &[&str] = &[
    "brave", "chrome", "edge", "firefox", "opera", "vivaldi", "chromium", "safari",
];

const BOT_ERROR_PATTERNS: &[&str] = &[
    "not a bot",
    "sign in to confirm",
    "bot verification",
    "captcha",
    "checkpoint_required",
    "challenge_required",
];

const AUTH_ERROR_PATTERNS: &[&str] = &[
    "cookies-from-browser",
    "--cookies",
    "login required",
    "log in or provide",
    "please log in",
    "requires authentication",
    "requires login",
    "must log in",
    "log in to",
    "post is private",
    "account is private",
    "video is private",
    "private video",
    "reel is private",
    "story is private",
    "follow this account",
    "must be following",
    "followers only",
    "account is protected",
    "protected tweets",
    "members-only",
    "members only",
    "join this channel",
    "channel members only",
    "granted access",
    "sign in to view",
    "confirm your age",
    "available to subscribers",
    "subscriber-only",
    "subscribers only",
];

/// Appends cookie arguments to a Command based on user configuration.
pub fn apply_cookie_args(cmd: &mut Command, cookies: Option<&str>) {
    let cookie_val = match cookies {
        Some(c) => c.trim(),
        None => return,
    };

    if cookie_val.is_empty()
        || cookie_val.starts_with('-')
        || matches!(
            cookie_val.to_ascii_lowercase().as_str(),
            "none" | "off" | "clear" | "disabled"
        )
    {
        return;
    }

    if cookie_val.eq_ignore_ascii_case("auto") {
        if let Some(browser) = detect_available_browser() {
            cmd.arg("--cookies-from-browser").arg(browser);
        }
        return;
    }

    if let Some((prefix, rest)) = cookie_val.split_once(':') {
        if prefix.eq_ignore_ascii_case("file") {
            let trimmed = rest.trim();
            if !trimmed.is_empty() && !trimmed.starts_with('-') {
                cmd.arg("--cookies").arg(trimmed);
            }
            return;
        } else if prefix.eq_ignore_ascii_case("browser") {
            let trimmed = rest.trim();
            if !trimmed.is_empty() && !trimmed.starts_with('-') {
                cmd.arg("--cookies-from-browser").arg(trimmed);
            }
            return;
        }
    }

    let browser_name = cookie_val
        .split_once(':')
        .map(|(b, _)| b)
        .unwrap_or(cookie_val);
    let lower = cookie_val.to_ascii_lowercase();
    let browser_lower = browser_name.to_ascii_lowercase();

    if KNOWN_COOKIE_BROWSERS.contains(&browser_lower.as_str()) {
        cmd.arg("--cookies-from-browser").arg(cookie_val);
    } else if lower.ends_with(".txt")
        || cookie_val.contains('/')
        || cookie_val.contains('\\')
        || Path::new(cookie_val).is_file()
    {
        cmd.arg("--cookies").arg(cookie_val);
    } else {
        cmd.arg("--cookies-from-browser").arg(cookie_val);
    }
}

/// Determines whether an error string indicates browser cookie database locking (e.g. Chromium exclusive file locks on Windows).
pub fn is_cookie_lock_error(error: &str) -> bool {
    let lower = error.to_lowercase();
    (lower.contains("could not copy")
        && (lower.contains("cookie database") || lower.contains("cookies")))
        || lower.contains("issues/7271")
        || lower.contains("lockprofilecookiedatabase")
}

/// Determines whether an error string indicates anti-bot verification or captcha requirements.
pub fn is_bot_error(error: &str) -> bool {
    let lower = error.to_lowercase().replace('’', "'");
    BOT_ERROR_PATTERNS.iter().any(|&pat| lower.contains(pat))
}

/// Determines whether an error string indicates anti-bot verification, login walls, private/follower-only media, or authentication cookie requirements.
pub fn is_auth_or_bot_error(error: &str) -> bool {
    if is_cookie_lock_error(error) {
        return true;
    }
    let lower = error.to_lowercase().replace('’', "'");
    BOT_ERROR_PATTERNS
        .iter()
        .chain(AUTH_ERROR_PATTERNS.iter())
        .any(|&pat| lower.contains(pat))
}

/// Determines whether an error string indicates HTTP 429 or platform rate limiting.
pub fn is_rate_limit_error(error: &str) -> bool {
    let lower = error.to_lowercase();
    lower.contains("too many requests")
        || lower.contains("rate limit")
        || lower.contains("rate-limit")
        || lower.contains("rate limited")
        || lower.contains("http error 429")
        || lower.contains("error 429")
        || lower.contains("status 429")
        || lower.contains("code 429")
        || lower.contains(" 429:")
        || lower.contains(" 429 ")
        || lower.contains("[429]")
        || lower.contains("(429)")
        || lower.contains("429 too many")
}

pub fn build_ytdlp_info_cmd(yt_dlp_path: &Path, url: &str) -> Command {
    build_ytdlp_info_cmd_ext(yt_dlp_path, url, None)
}

pub fn build_ytdlp_info_cmd_ext(yt_dlp_path: &Path, url: &str, cookies: Option<&str>) -> Command {
    let mut cmd = Command::new(yt_dlp_path);
    cmd.stdin(Stdio::null())
        .arg("-J")
        .arg("--no-warnings")
        .arg("--no-playlist")
        .arg("--socket-timeout")
        .arg("30");
    apply_cookie_args(&mut cmd, cookies);
    cmd.arg("--").arg(url);
    cmd
}

pub fn fetch_video_info(url: &str, yt_dlp_path: &Path) -> Result<VideoInfo, String> {
    fetch_video_info_full(url, yt_dlp_path, false, None)
}

pub fn fetch_video_info_ext(
    url: &str,
    yt_dlp_path: &Path,
    compat_mode: bool,
) -> Result<VideoInfo, String> {
    fetch_video_info_full(url, yt_dlp_path, compat_mode, None)
}

pub fn fetch_video_info_full(
    url: &str,
    yt_dlp_path: &Path,
    compat_mode: bool,
    cookies: Option<&str>,
) -> Result<VideoInfo, String> {
    let output = build_ytdlp_info_cmd_ext(yt_dlp_path, url, cookies)
        .output()
        .map_err(|e| format!("Failed to execute yt-dlp: {}", e))?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(extract_engine_error(&err));
    }

    let stdout_str = String::from_utf8_lossy(&output.stdout);
    parse_video_info_json_ext(&stdout_str, compat_mode)
}

/// Parsed numeric progress snapshot from yt-dlp stdout.
#[derive(Debug, Clone, Default)]
pub struct NumericProgress {
    pub percent: f64,
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
    pub speed_bytes_sec: f64,
    pub eta_seconds: Option<u64>,
    pub vcodec: Option<String>,
    pub format_id: Option<String>,
}

/// Parses a yt-dlp stdout progress line into raw numeric metrics.
///
/// Supports the numeric template emitted by `--progress-template`:
/// `download:PROGRESS|downloaded_bytes|total_bytes|total_bytes_estimate|speed|eta|vcodec|format_id`
/// as well as legacy string-formatted lines (`PROGRESS| 45.2%|...`) for backward compatibility.
pub fn parse_progress_numeric(line: &str) -> Option<NumericProgress> {
    let clean = line.strip_prefix("download:").unwrap_or(line);
    let rest = clean.strip_prefix("PROGRESS|")?;
    let parts: Vec<&str> = rest.split('|').collect();
    if parts.len() < 5 {
        return None;
    }

    if parts[0].contains('%') || parts[1].chars().any(|c| c.is_alphabetic()) {
        let percent = parts[0]
            .trim()
            .trim_end_matches('%')
            .trim()
            .parse::<f64>()
            .unwrap_or(0.0);
        let downloaded = parse_size_bytes(parts[1]).unwrap_or(0);
        let total = parse_size_bytes(parts[2]).unwrap_or(0);
        let speed = parse_speed_bytes(parts[3]).unwrap_or(0.0);
        let eta = parse_eta_secs(parts[4]);
        let vcodec = parts
            .get(5)
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty() && s != "NA");
        let format_id = parts
            .get(6)
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty() && s != "NA");

        return Some(NumericProgress {
            percent,
            downloaded_bytes: downloaded,
            total_bytes: total,
            speed_bytes_sec: speed,
            eta_seconds: eta,
            vcodec,
            format_id,
        });
    }

    let downloaded_bytes = parts[0].trim().parse::<u64>().unwrap_or(0);
    let total_bytes = parts[1]
        .trim()
        .parse::<u64>()
        .ok()
        .or_else(|| parts[2].trim().parse::<u64>().ok())
        .unwrap_or(0);
    let speed_bytes_sec = parts[3].trim().parse::<f64>().unwrap_or(0.0);
    let eta_seconds = parts[4].trim().parse::<u64>().ok();
    let vcodec = parts
        .get(5)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty() && s != "NA");
    let format_id = parts
        .get(6)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty() && s != "NA");

    let percent = if total_bytes > 0 {
        (downloaded_bytes as f64 / total_bytes as f64 * 100.0).clamp(0.0, 100.0)
    } else {
        0.0
    };

    Some(NumericProgress {
        percent,
        downloaded_bytes,
        total_bytes,
        speed_bytes_sec,
        eta_seconds,
        vcodec,
        format_id,
    })
}

/// Combines separate video and audio stream progress into unified percentage and byte totals.
///
/// Returns `(combined_percent, combined_total_bytes, combined_downloaded_bytes)`.
///
/// Multi-stream weighting strategy:
/// 1. Single progressive stream: Returns the stream's percent and bytes directly.
/// 2. Dual streams with known totals: Computes exact combined byte percentage:
///    `(video_downloaded + audio_downloaded) / (video_total + audio_total) * 100.0`.
/// 3. Indeterminate totals (chunked/live): Video represents [0%..85%] of overall progress,
///    and audio represents the remaining [85%..100%] (`85.0 + stream_percent * 0.15`),
///    preventing the progress bar from resetting back to 0% when the audio stream begins.
#[allow(clippy::too_many_arguments)]
pub fn combine_progress(
    expects_separate_audio: bool,
    streams_seen: usize,
    is_audio_stream: bool,
    stream_percent: f64,
    video_downloaded: u64,
    video_total: u64,
    audio_downloaded: u64,
    audio_total: u64,
) -> (f64, u64, u64) {
    if !expects_separate_audio && streams_seen < 2 {
        let (tot, dwn) = if is_audio_stream {
            (audio_total, audio_downloaded)
        } else {
            (video_total, video_downloaded)
        };
        (stream_percent.clamp(0.0, 100.0), tot, dwn)
    } else if video_total > 0 && audio_total > 0 {
        let tot = video_total + audio_total;
        let dwn = video_downloaded + audio_downloaded;
        let pct = if tot > 0 {
            (dwn as f64 / tot as f64 * 100.0).clamp(0.0, 100.0)
        } else {
            stream_percent.clamp(0.0, 100.0)
        };
        (pct, tot, dwn)
    } else if is_audio_stream {
        let tot = video_total.max(video_downloaded) + audio_total;
        let dwn = video_total.max(video_downloaded) + audio_downloaded;
        let pct = (85.0 + stream_percent * 0.15).clamp(0.0, 100.0);
        (pct, tot, dwn)
    } else {
        let pct = (stream_percent * 0.85).clamp(0.0, 100.0);
        (pct, video_total, video_downloaded)
    }
}

pub fn parse_progress_line(line: &str) -> Option<(f64, String, String, String, String)> {
    parse_progress_line_ext(line).map(|(p, d, t, s, e, _)| (p, d, t, s, e))
}

pub fn parse_progress_line_ext(
    line: &str,
) -> Option<(f64, String, String, String, String, Option<String>)> {
    let clean = line.strip_prefix("download:").unwrap_or(line);
    let rest = clean.strip_prefix("PROGRESS|")?;
    let parts: Vec<&str> = rest.split('|').collect();
    if parts.len() < 5 {
        return None;
    }

    if parts[0].contains('%') || parts[1].chars().any(|c| c.is_alphabetic()) {
        let percent_str = parts[0].trim().trim_end_matches('%').trim();
        let downloaded_str = parts[1].trim();
        let total_str = parts[2].trim();
        let speed_str = parts[3].trim();
        let eta_str = parts[4].trim();
        let vcodec = parts
            .get(5)
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty() && s != "NA");

        let percent = percent_str.parse::<f64>().unwrap_or(0.0);
        let downloaded_clean = if downloaded_str.is_empty() || downloaded_str == "NA" {
            "Unknown".to_string()
        } else {
            downloaded_str.to_string()
        };
        let total_clean = if total_str.is_empty() || total_str == "NA" {
            "Unknown".to_string()
        } else {
            total_str.to_string()
        };
        let speed_clean = if speed_str.is_empty() || speed_str == "NA" {
            "Unknown".to_string()
        } else {
            speed_str.to_string()
        };
        let eta_clean = if eta_str.is_empty() || eta_str == "NA" {
            "--:--".to_string()
        } else {
            eta_str.to_string()
        };

        return Some((
            percent,
            downloaded_clean,
            total_clean,
            speed_clean,
            eta_clean,
            vcodec,
        ));
    }

    let num = parse_progress_numeric(line)?;
    let downloaded_str = if num.downloaded_bytes > 0 {
        format_bytes_u64(num.downloaded_bytes)
    } else {
        "0 B".to_string()
    };
    let total_str = if num.total_bytes > 0 {
        format_bytes_u64(num.total_bytes)
    } else {
        "Unknown".to_string()
    };
    let speed_str = format_speed(num.speed_bytes_sec);
    let eta_str = format_eta(num.eta_seconds);

    Some((
        num.percent,
        downloaded_str,
        total_str,
        speed_str,
        eta_str,
        num.vcodec,
    ))
}

/// Parses a size string with optional binary units (e.g. "50.0MiB", "5M", "52428800") into raw bytes.
pub fn parse_size_bytes(s: &str) -> Option<u64> {
    let s = s.trim().trim_start_matches('~').trim();
    if s.is_empty() || s == "NA" || s == "Unknown" {
        return None;
    }
    if let Ok(bytes) = s.parse::<u64>() {
        return Some(bytes);
    }
    if let Ok(bytes_f) = s.parse::<f64>() {
        return Some(bytes_f as u64);
    }
    let alpha_pos = s.find(|c: char| c.is_alphabetic())?;
    let (num_part, unit) = s.split_at(alpha_pos);
    let num_clean: String = num_part.chars().filter(|c| !c.is_whitespace()).collect();
    let num: f64 = num_clean.parse().ok()?;
    let mult = match unit.trim().to_lowercase().as_str() {
        "b" => 1.0,
        "k" | "kb" | "kib" => 1024.0,
        "m" | "mb" | "mib" => 1024.0 * 1024.0,
        "g" | "gb" | "gib" => 1024.0 * 1024.0 * 1024.0,
        "t" | "tb" | "tib" => 1024.0 * 1024.0 * 1024.0 * 1024.0,
        _ => 1.0,
    };
    Some((num * mult) as u64)
}

/// Parses a speed string (e.g. "2.50MiB/s", "2.5M/s", "2621440") into raw bytes per second.
pub fn parse_speed_bytes(s: &str) -> Option<f64> {
    let s = s.trim().trim_end_matches("/s");
    parse_size_bytes(s).map(|b| b as f64)
}

/// Parses an ETA string (e.g. "01:23", "00:02:15", "42") into total seconds.
pub fn parse_eta_secs(s: &str) -> Option<u64> {
    let s = s.trim();
    if s.is_empty() || s == "NA" || s == "--:--" {
        return None;
    }
    if let Ok(secs) = s.parse::<u64>() {
        return Some(secs);
    }
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() == 2 {
        let m: u64 = parts[0].trim().parse().ok()?;
        let sec: u64 = parts[1].trim().parse().ok()?;
        Some(m * 60 + sec)
    } else if parts.len() == 3 {
        let h: u64 = parts[0].trim().parse().ok()?;
        let m: u64 = parts[1].trim().parse().ok()?;
        let sec: u64 = parts[2].trim().parse().ok()?;
        Some(h * 3600 + m * 60 + sec)
    } else {
        None
    }
}

pub fn get_terminal_width() -> usize {
    terminal::size()
        .map(|(w, _)| w as usize)
        .unwrap_or_else(|_| {
            std::env::var("COLUMNS")
                .ok()
                .and_then(|c| c.parse::<usize>().ok())
                .unwrap_or(100)
        })
}

pub fn render_progress_bar_with_width(
    percent: f64,
    downloaded: &str,
    total: &str,
    speed: &str,
    eta: &str,
    terminal_width: usize,
) -> String {
    if terminal_width >= 75 {
        let bar_width = (terminal_width.saturating_sub(69)).min(25);
        let filled = ((percent / 100.0) * bar_width as f64).clamp(0.0, bar_width as f64) as usize;
        let empty = bar_width.saturating_sub(filled + if filled < bar_width { 1 } else { 0 });
        let arrow = if filled < bar_width { ">" } else { "" };
        let bar = format!("[{}{}{}]", "=".repeat(filled), arrow, " ".repeat(empty));

        format!(
            "[download] {} {:>5.1}% [{:>9} / {:>9}] at {:>10} ETA {:>5}",
            bar, percent, downloaded, total, speed, eta
        )
    } else if terminal_width >= 55 {
        format!(
            "[download] {:>5.1}% [{} / {}] at {} ETA {}",
            percent, downloaded, total, speed, eta
        )
    } else {
        format!("[download] {:>5.1}% {} ETA {}", percent, speed, eta)
    }
}

pub fn render_progress_bar(
    percent: f64,
    downloaded: &str,
    total: &str,
    speed: &str,
    eta: &str,
) -> String {
    render_progress_bar_with_width(percent, downloaded, total, speed, eta, get_terminal_width())
}

pub const MEDIA_EXTENSIONS_AUDIO: &[&str] =
    &["mp3", "m4a", "aac", "opus", "wav", "flac", "ogg", "webm"];
pub const MEDIA_EXTENSIONS_VIDEO: &[&str] = &["mp4", "mkv", "webm", "avi", "mov"];

/// Sanitizes a media title to match filesystem-safe characters produced by yt-dlp.
///
/// Replaces colons with hyphens, path separators and pipes with underscores, and strips
/// Windows/POSIX reserved filesystem characters (`*`, `?`, `"`, `<`, `>`).
///
/// # Examples
///
/// ```
/// use dwdr::engine::downloader::sanitize_media_title;
///
/// assert_eq!(sanitize_media_title("What: If? (Part 1/2)"), "What - If (Part 1_2)");
/// ```
pub fn sanitize_media_title(title: &str) -> String {
    let replaced = title
        .replace(": ", " - ")
        .replace(':', "-")
        .replace(['/', '\\', '|'], "_");
    replaced
        .chars()
        .filter(|c| !['*', '?', '"', '<', '>'].contains(c))
        .collect::<String>()
        .trim()
        .to_string()
}

/// Derives the expected output file stem from title and optional custom template.
pub fn derive_output_stem(title: &str, custom_output_template: Option<&str>) -> String {
    let sanitized = sanitize_media_title(title);
    if let Some(tmpl) = custom_output_template {
        tmpl.replace("%(title)s", &sanitized)
            .replace(".%(ext)s", "")
    } else {
        sanitized
    }
}

/// Checks whether a completed media file matching the title and target extension already exists.
pub fn find_existing_media_file(
    download_dir: &Path,
    title: &str,
    is_audio: bool,
) -> Option<PathBuf> {
    if !download_dir.exists() || !download_dir.is_dir() {
        return None;
    }

    let sanitized = sanitize_media_title(title);
    if sanitized.is_empty() {
        return None;
    }

    let legacy_sanitized: String = title
        .chars()
        .filter(|c| !['/', '\\', ':', '*', '?', '"', '<', '>', '|'].contains(c))
        .collect::<String>()
        .trim()
        .to_string();

    let read_dir = std::fs::read_dir(download_dir).ok()?;

    for entry in read_dir.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n,
            None => continue,
        };
        let name_lower = name.to_lowercase();

        // Exclude in-flight chunk files from prior aborted downloads.
        if name_lower.ends_with(".part")
            || name_lower.ends_with(".ytdl")
            || name_lower.ends_with(".temp")
            || name_lower.ends_with(".tmp")
        {
            continue;
        }

        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        if stem.eq_ignore_ascii_case(&sanitized) || stem.eq_ignore_ascii_case(&legacy_sanitized) {
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();
            let is_media_ext = if is_audio {
                MEDIA_EXTENSIONS_AUDIO.contains(&ext.as_str())
            } else {
                MEDIA_EXTENSIONS_VIDEO.contains(&ext.as_str())
            };
            if is_media_ext && path.metadata().map(|m| m.len() > 0).unwrap_or(false) {
                return Some(path);
            }
        }
    }

    None
}

/// Finds the next available numbered copy template (e.g. "%(title)s (1).%(ext)s").
pub fn resolve_unique_output_template(
    download_dir: &Path,
    title: &str,
    is_audio: bool,
) -> (String, usize) {
    let sanitized = sanitize_media_title(title);
    let mut index = 1;
    let target_exts = if is_audio {
        MEDIA_EXTENSIONS_AUDIO
    } else {
        MEDIA_EXTENSIONS_VIDEO
    };

    loop {
        let candidate_stem = format!("{} ({})", sanitized, index);
        let mut exists = false;
        for ext in target_exts {
            let candidate_path = download_dir.join(format!("{}.{}", candidate_stem, ext));
            if candidate_path.exists() {
                exists = true;
                break;
            }
        }
        if !exists {
            return (format!("%(title)s ({}).%(ext)s", index), index);
        }
        index += 1;
    }
}

/// Purges completed media files matching the title if interrupted during download.
pub fn clean_interrupted_media_files(
    download_dir: &str,
    title_hint: &str,
    custom_stem: Option<&str>,
) -> usize {
    let dir = Path::new(download_dir);
    if !dir.exists() || !dir.is_dir() {
        return 0;
    }

    let sanitized = sanitize_media_title(title_hint);
    if sanitized.is_empty() {
        return 0;
    }

    let read_dir = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(_) => return 0,
    };

    let mut removed_count = 0;
    for entry in read_dir.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        let matches = if let Some(target) = custom_stem {
            stem.eq_ignore_ascii_case(target)
        } else {
            stem.eq_ignore_ascii_case(&sanitized)
        };

        if matches {
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();
            if (MEDIA_EXTENSIONS_AUDIO.contains(&ext.as_str())
                || MEDIA_EXTENSIONS_VIDEO.contains(&ext.as_str()))
                && std::fs::remove_file(&path).is_ok()
            {
                removed_count += 1;
            }
        }
    }

    removed_count
}

/// Purges incomplete download files (.part, .ytdl, unmerged chunks) and temp dirs while preserving completed media.
pub fn clean_partial_files(download_dir: &str, title_hint: Option<&str>) -> usize {
    let dir = Path::new(download_dir);
    if !dir.exists() || !dir.is_dir() {
        return 0;
    }

    let read_dir = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(_) => return 0,
    };

    let mut removed_count = 0;
    let sanitized_title = title_hint.map(sanitize_media_title);

    for entry in read_dir.flatten() {
        let path = entry.path();
        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n,
            None => continue,
        };
        let name_lower = name.to_lowercase();

        // Clean up orphaned temp directories from aborted downloads.
        if path.is_dir() {
            let is_temp_dir = name_lower.ends_with(".temp")
                || name_lower.ends_with(".tmp")
                || name_lower.ends_with(".part");
            let dir_matches = match &sanitized_title {
                Some(title) if !title.trim().is_empty() => name.contains(title.as_str()),
                Some(_) => false,
                None => is_temp_dir,
            };
            if is_temp_dir && dir_matches && std::fs::remove_dir_all(&path).is_ok() {
                removed_count += 1;
            }
            continue;
        }

        let is_part_or_temp = name_lower.ends_with(".part")
            || name_lower.ends_with(".ytdl")
            || name_lower.ends_with(".temp");

        let matches_title = match &sanitized_title {
            Some(title) if !title.trim().is_empty() => name.contains(title.as_str()),
            Some(_) => false,
            None => true,
        };

        let is_unmerged_chunk = match &sanitized_title {
            Some(title) if !title.trim().is_empty() => {
                name.contains(title.as_str())
                    && name.contains(".f")
                    && !name_lower.ends_with(".mp4")
            }
            Some(_) => false,
            None => false,
        };

        if ((is_part_or_temp && matches_title) || is_unmerged_chunk)
            && std::fs::remove_file(&path).is_ok()
        {
            removed_count += 1;
        }
    }

    removed_count
}

// Tracks foreground download process metadata so signal handlers can perform targeted cleanup.
struct DirectDownloadTracker {
    pid: Option<u32>,
    temp_dir: PathBuf,
}

static DIRECT_TRACKER: Mutex<Option<DirectDownloadTracker>> = Mutex::new(None);

pub fn is_direct_download_active() -> bool {
    if let Ok(g) = DIRECT_TRACKER.lock() {
        g.is_some()
    } else {
        false
    }
}

pub fn register_direct_download(pid: u32, download_dir: &str, title: &str) {
    register_direct_download_ext(pid, download_dir, title, None);
}

pub fn register_direct_download_ext(
    pid: u32,
    download_dir: &str,
    _title: &str,
    _custom_stem: Option<&str>,
) {
    let temp_dir = Path::new(download_dir).join(".dwdr_direct");
    register_direct_download_full(pid, temp_dir);
}

pub fn register_direct_download_full(pid: u32, temp_dir: PathBuf) {
    record_active_download();
    let mut guard = match DIRECT_TRACKER.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    *guard = Some(DirectDownloadTracker {
        pid: Some(pid),
        temp_dir,
    });
}

pub fn unregister_direct_download() {
    let mut guard = match DIRECT_TRACKER.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    *guard = None;
    drop(guard);
    clear_active_download();
}

pub fn cancel_direct_download() {
    let mut guard = match DIRECT_TRACKER.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    if let Some(tracker) = guard.take() {
        if let Some(pid) = tracker.pid {
            kill_process_by_pid(pid);
        }
        let _ = std::fs::remove_dir_all(&tracker.temp_dir);
    }
    drop(guard);
    clear_active_download();
}

pub fn build_ytdlp_download_cmd(
    yt_dlp_path: &Path,
    ffmpeg_path: &Path,
    url: &str,
    format_id: &str,
    download_dir: &str,
) -> Command {
    build_ytdlp_download_cmd_ext(
        yt_dlp_path,
        ffmpeg_path,
        url,
        format_id,
        download_dir,
        None,
        false,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn build_ytdlp_download_cmd_ext(
    yt_dlp_path: &Path,
    ffmpeg_path: &Path,
    url: &str,
    format_id: &str,
    download_dir: &str,
    custom_output_template: Option<&str>,
    force_overwrites: bool,
    task_id: Option<&str>,
) -> Command {
    build_ytdlp_download_cmd_full(
        yt_dlp_path,
        ffmpeg_path,
        url,
        format_id,
        download_dir,
        custom_output_template,
        force_overwrites,
        task_id,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn build_ytdlp_download_cmd_full(
    yt_dlp_path: &Path,
    ffmpeg_path: &Path,
    url: &str,
    format_id: &str,
    download_dir: &str,
    custom_output_template: Option<&str>,
    force_overwrites: bool,
    task_id: Option<&str>,
    cookies: Option<&str>,
) -> Command {
    let clean_dir = download_dir.trim_end_matches(['/', '\\']);
    let mut cmd = Command::new(yt_dlp_path);

    let (is_audio, has_audio, clean_format_id) = if let Some(stripped) =
        format_id.strip_prefix("audio:")
    {
        (true, true, stripped)
    } else if let Some(stripped) = format_id.strip_prefix("direct:") {
        (false, true, stripped)
    } else if let Some(stripped) = format_id.strip_prefix("prog:") {
        (false, true, stripped)
    } else if format_id.eq_ignore_ascii_case("audio") || format_id.eq_ignore_ascii_case("bestaudio")
    {
        (true, true, "bestaudio/best")
    } else {
        (false, false, format_id)
    };

    // yt-dlp requires separate -P directory and -o template; combined -o paths cause mixed slashes and [Errno 22] on Windows.
    // --windows-filenames and --trim-filenames sanitize special unicode (e.g. accents) and keep filenames within Windows MAX_PATH.
    let format_arg = if is_audio || has_audio {
        clean_format_id.to_string()
    } else if clean_format_id == "best" {
        "bestvideo+bestaudio/best".to_string()
    } else if clean_format_id.contains('+') {
        clean_format_id.to_string()
    } else {
        format!("{}+bestaudio/{}/best", clean_format_id, clean_format_id)
    };

    let output_template = custom_output_template.unwrap_or("%(title)s.%(ext)s");
    let id_str = task_id.unwrap_or("direct");
    let temp_arg = format!("temp:{}/.dwdr_{}", clean_dir, id_str);

    cmd.arg("-f")
        .arg(format_arg)
        .arg("-P")
        .arg(clean_dir)
        .arg("-P")
        .arg(temp_arg)
        .arg("-o")
        .arg(output_template)
        .arg("--windows-filenames")
        .arg("--trim-filenames")
        .arg("160");

    if force_overwrites {
        cmd.arg("--force-overwrites");
        cmd.arg("--postprocessor-args").arg("Merger:-y");
    }

    if is_audio {
        cmd.arg("-x").arg("--audio-format").arg("m4a");
    } else {
        cmd.arg("--merge-output-format").arg("mp4");
    }

    cmd.arg("-N")
        .arg("4")
        .arg("--retries")
        .arg("10")
        .arg("--fragment-retries")
        .arg("10")
        .arg("--no-mtime")
        .arg("--http-chunk-size")
        .arg("10M")
        .arg("--print")
        .arg("after_move:filepath")
        .arg("--progress")
        .arg("--newline")
        .arg("--encoding")
        .arg("utf-8")
        .arg("--progress-template")
        .arg("download:PROGRESS|%(progress.downloaded_bytes)s|%(progress.total_bytes)s|%(progress.total_bytes_estimate)s|%(progress.speed)s|%(progress.eta)s|%(info.vcodec)s|%(info.format_id)s")
        .arg("--progress-template")
        .arg("postprocess:POSTPROCESS|%(progress.status)s")
        .arg("--no-warnings")
        .arg("--no-playlist")
        .arg("--socket-timeout")
        .arg("30");

    if ffmpeg_path.exists() {
        cmd.arg("--ffmpeg-location").arg(ffmpeg_path);
    }

    apply_cookie_args(&mut cmd, cookies);

    cmd.env("PYTHONUNBUFFERED", "1");
    cmd.arg("--").arg(url);
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    cmd
}

/// Reads lines from a buffered reader byte-by-byte, converting non-UTF-8 sequences lossily without aborting.
pub fn read_lossy_lines<R: BufRead, F: FnMut(&str)>(mut reader: R, mut on_line: F) {
    let mut line_bytes = Vec::new();
    while let Ok(n) = reader.read_until(b'\n', &mut line_bytes) {
        if n == 0 {
            break;
        }
        let line_lossy = String::from_utf8_lossy(&line_bytes);
        on_line(line_lossy.trim());
        line_bytes.clear();
    }
}

/// Downloads media in the foreground, streaming live progress and managing process lifecycle.
pub fn download_media(
    url: &str,
    format_id: &str,
    yt_dlp_path: &Path,
    ffmpeg_path: &Path,
    download_dir: &str,
    title: Option<&str>,
) -> Result<Option<PathBuf>, String> {
    download_media_ext(
        url,
        format_id,
        yt_dlp_path,
        ffmpeg_path,
        download_dir,
        title,
        None,
        false,
    )
    .map(|outcome| outcome.path)
}

#[derive(Debug)]
struct SharedForegroundProgress {
    target_percent: f64,
    downloaded_bytes: u64,
    total_bytes: u64,
    raw_speed: f64,
    raw_eta: Option<u64>,
    phase: DownloadPhase,
    is_finalizing: bool,
    finalizing_msg: Option<String>,
    last_speed_update: Instant,
}

/// Downloads media with custom output filename templates and overwrite options.
#[allow(clippy::too_many_arguments)]
pub fn download_media_ext(
    url: &str,
    format_id: &str,
    yt_dlp_path: &Path,
    ffmpeg_path: &Path,
    download_dir: &str,
    title: Option<&str>,
    custom_output_template: Option<&str>,
    force_overwrites: bool,
) -> Result<DownloadOutcome, String> {
    download_media_full(
        url,
        format_id,
        yt_dlp_path,
        ffmpeg_path,
        download_dir,
        title,
        custom_output_template,
        force_overwrites,
        None,
    )
}

/// Downloads media with optional cookie-based authentication.
#[allow(clippy::too_many_arguments)]
pub fn download_media_full(
    url: &str,
    format_id: &str,
    yt_dlp_path: &Path,
    ffmpeg_path: &Path,
    download_dir: &str,
    title: Option<&str>,
    custom_output_template: Option<&str>,
    force_overwrites: bool,
    cookies: Option<&str>,
) -> Result<DownloadOutcome, String> {
    let download_start = Instant::now();
    let validated_dir = validate_download_path(download_dir, true)
        .map_err(|e| format!("Invalid download directory: {}", e))?;
    let clean_download_dir = validated_dir.to_string_lossy();
    let temp_dir = validated_dir.join(".dwdr_direct");

    let mut cmd = build_ytdlp_download_cmd_full(
        yt_dlp_path,
        ffmpeg_path,
        url,
        format_id,
        &clean_download_dir,
        custom_output_template,
        force_overwrites,
        Some("direct"),
        cookies,
    );

    #[cfg(unix)]
    {
        // Spawns yt-dlp in an isolated process group (PGID = child PID) so that terminating
        // DWDR broadcasts signals to all descendants (including ffmpeg mergers spawned by yt-dlp),
        // preventing orphaned processes from surviving and locking partial files.
        cmd.process_group(0);
    }

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Failed to execute yt-dlp: {}", e))?;

    let pid = child.id();
    register_direct_download_full(pid, temp_dir.clone());

    let (tx, rx) = mpsc::channel();
    let tx_err = tx.clone();

    let stderr_handle = child.stderr.take();
    let stderr_thread = thread::spawn(move || {
        let mut err_output = Vec::new();
        if let Some(err_pipe) = stderr_handle {
            let reader = BufReader::new(err_pipe);
            read_lossy_lines(reader, |line| {
                if parse_progress_numeric(line).is_some()
                    || line.starts_with("POSTPROCESS|")
                    || line.starts_with("[Merger]")
                    || line.starts_with("[ExtractAudio]")
                    || line.starts_with("[Fixup")
                    || line.contains("Merging formats")
                {
                    let _ = tx_err.send((false, line.to_string()));
                } else {
                    err_output.extend_from_slice(line.as_bytes());
                    err_output.push(b'\n');
                }
            });
        }
        String::from_utf8_lossy(&err_output).to_string()
    });

    let is_audio_only = format_id.starts_with("audio:")
        || format_id.eq_ignore_ascii_case("audio")
        || format_id.eq_ignore_ascii_case("bestaudio");

    let ctx = UiContext::get();
    let is_rich = ctx.animations && !ctx.is_plain();

    let shared = Arc::new(Mutex::new(SharedForegroundProgress {
        target_percent: 0.0,
        downloaded_bytes: 0,
        total_bytes: 0,
        raw_speed: 0.0,
        raw_eta: None,
        phase: if is_audio_only {
            DownloadPhase::Audio
        } else {
            DownloadPhase::Video
        },
        is_finalizing: false,
        finalizing_msg: None,
        last_speed_update: Instant::now(),
    }));
    let running = Arc::new(AtomicBool::new(true));

    let render_handle = if is_rich {
        let shared_render = Arc::clone(&shared);
        let running_render = Arc::clone(&running);
        let theme = ctx.theme;
        let color = ctx.color_level;
        let title_opt = title.map(|t| t.to_string());

        Some(thread::spawn(move || {
            let mut smoother = SpeedSmoother::new();
            let mut eased = EasedProgress::new();
            let mut tick: u64 = 0;
            let mut last_frame = String::new();
            let mut last_term_w: usize = 0;
            let mut last_sample_time = Instant::now();

            while running_render.load(Ordering::Relaxed) {
                let (state, target_pct, is_fin) = {
                    let guard = shared_render.lock().unwrap_or_else(|p| p.into_inner());
                    let now = Instant::now();

                    if now.duration_since(last_sample_time)
                        >= Duration::from_millis(SAMPLE_INTERVAL_MS)
                    {
                        let is_stalled =
                            now.duration_since(guard.last_speed_update) > STALL_THRESHOLD;
                        let spd = if is_stalled { 0.0 } else { guard.raw_speed };
                        smoother.feed(spd, now);
                        last_sample_time = now;
                    }

                    eased.set_target(guard.target_percent);
                    let current_pct = eased.tick();

                    let remaining = guard.total_bytes.saturating_sub(guard.downloaded_bytes);
                    let eta = smoother.eta_seconds(remaining).or(guard.raw_eta);

                    let state = ProgressState {
                        percent: current_pct,
                        downloaded_bytes: guard.downloaded_bytes,
                        total_bytes: guard.total_bytes,
                        speed_bytes_sec: smoother.ema,
                        eta_seconds: eta,
                        sparkline_samples: smoother.sparkline_samples(),
                        phase: guard.phase,
                        is_finalizing: guard.is_finalizing,
                        finalizing_msg: guard.finalizing_msg.clone(),
                    };
                    (state, guard.target_percent, guard.is_finalizing)
                };

                set_taskbar_progress(target_pct as u8);
                if let Some(ref t) = title_opt {
                    set_terminal_title(&format!("DWDR - {:.0}% - {}", target_pct, t));
                }

                let term_w = crossterm::terminal::size()
                    .map(|(w, _)| w as usize)
                    .unwrap_or(80);
                let show_stepper = !is_audio_only && !is_fin;
                let frame =
                    render_foreground_frame(&state, term_w, &theme, color, tick, show_stepper);

                // Clean wrapped rows when terminal width changes to prevent duplicate line artifacts on Windows Conhost.
                if term_w != last_term_w && last_term_w > 0 {
                    let prev_vis = visible_width(&last_frame);
                    let wrapped_rows = if term_w > 0 && prev_vis >= term_w {
                        (prev_vis / term_w).min(10) as u16
                    } else {
                        0
                    };
                    if wrapped_rows > 0 {
                        let _ = crossterm::execute!(
                            stdout(),
                            crossterm::cursor::MoveUp(wrapped_rows),
                            crossterm::cursor::MoveToColumn(0),
                            crossterm::terminal::Clear(
                                crossterm::terminal::ClearType::FromCursorDown
                            )
                        );
                    } else {
                        let _ = crossterm::execute!(
                            stdout(),
                            crossterm::cursor::MoveToColumn(0),
                            crossterm::terminal::Clear(crossterm::terminal::ClearType::CurrentLine)
                        );
                    }
                    last_frame.clear();
                }
                last_term_w = term_w;

                if frame != last_frame {
                    print!("\r\x1b[2K{}", frame);
                    let _ = stdout().flush();
                    last_frame = frame;
                }

                tick = tick.wrapping_add(1);
                // Throttle progress updates to ~500ms for stable stats without jitter, using 80ms for finalizer animation.
                let sleep_duration = if is_fin {
                    Duration::from_millis(TICK_MS)
                } else {
                    Duration::from_millis(500)
                };
                thread::sleep(sleep_duration);
            }

            (smoother.peak, smoother.sparkline_samples())
        }))
    } else {
        None
    };

    let mut captured_filepath: Option<PathBuf> = None;
    let expects_separate_audio = format_id.contains('+');
    let mut streams_seen: usize = 0;
    let mut current_stream_key: Option<String> = None;
    let mut video_downloaded: u64 = 0;
    let mut video_total: u64 = 0;
    let mut audio_downloaded: u64 = 0;
    let mut audio_total: u64 = 0;
    let mut plain_throttle = PlainProgressThrottle::default();
    let mut peak_speed_plain: f64 = 0.0;
    let mut last_plain_bytes: u64 = 0;

    let stdout_handle = child.stdout.take();
    let tx_out = tx;
    let stdout_thread = thread::spawn(move || {
        if let Some(stdout_pipe) = stdout_handle {
            let reader = BufReader::new(stdout_pipe);
            read_lossy_lines(reader, |line| {
                let _ = tx_out.send((true, line.to_string()));
            });
        }
    });

    while let Ok((is_stdout, line)) = rx.recv() {
        let trimmed = line.trim();
        if let Some(num) = parse_progress_numeric(&line) {
            let is_audio_stream = is_audio_only
                || num
                    .vcodec
                    .as_deref()
                    .map(|v| v.eq_ignore_ascii_case("none"))
                    .unwrap_or(false);

            let stream_key = num.format_id.clone().unwrap_or_else(|| {
                if is_audio_stream {
                    "audio".to_string()
                } else {
                    "video".to_string()
                }
            });

            if current_stream_key.as_ref() != Some(&stream_key) {
                current_stream_key = Some(stream_key);
                streams_seen += 1;
            }

            let phase = if is_audio_stream {
                audio_downloaded = num.downloaded_bytes;
                if num.total_bytes > 0 {
                    audio_total = num.total_bytes;
                }
                DownloadPhase::Audio
            } else {
                video_downloaded = num.downloaded_bytes;
                if num.total_bytes > 0 {
                    video_total = num.total_bytes;
                }
                DownloadPhase::Video
            };

            let (combined_pct, total_bytes, downloaded_bytes) = combine_progress(
                expects_separate_audio,
                streams_seen,
                is_audio_stream,
                num.percent,
                video_downloaded,
                video_total,
                audio_downloaded,
                audio_total,
            );

            last_plain_bytes = downloaded_bytes;
            if num.speed_bytes_sec > peak_speed_plain {
                peak_speed_plain = num.speed_bytes_sec;
            }

            if is_rich {
                let mut guard = shared.lock().unwrap_or_else(|p| p.into_inner());
                guard.target_percent = combined_pct;
                guard.downloaded_bytes = downloaded_bytes;
                guard.total_bytes = total_bytes;
                guard.raw_speed = num.speed_bytes_sec;
                guard.raw_eta = num.eta_seconds;
                guard.phase = phase;
                guard.last_speed_update = Instant::now();
            } else if plain_throttle.should_print(combined_pct, Instant::now()) {
                let d_str = format_bytes_u64(downloaded_bytes);
                let t_str = if total_bytes > 0 {
                    format_bytes_u64(total_bytes)
                } else {
                    "Unknown".to_string()
                };
                let s_str = format_speed(num.speed_bytes_sec);
                let e_str = format_eta(num.eta_seconds);
                println!(
                    "{}",
                    render_progress_bar(combined_pct, &d_str, &t_str, &s_str, &e_str)
                );
            }
        } else if line.starts_with("[Merger]") || line.contains("Merging formats") {
            let msg = "Merging video and audio tracks...";
            if is_rich {
                let mut guard = shared.lock().unwrap_or_else(|p| p.into_inner());
                guard.target_percent = 100.0;
                guard.phase = DownloadPhase::Finalize;
                guard.is_finalizing = true;
                guard.finalizing_msg = Some(msg.to_string());
            } else {
                println!(
                    "[download] [{}] 100.0% [Finalizing: {}]",
                    "=".repeat(25),
                    msg
                );
            }
        } else if line.starts_with("[ExtractAudio]") {
            let msg = "Extracting audio stream...";
            if is_rich {
                let mut guard = shared.lock().unwrap_or_else(|p| p.into_inner());
                guard.target_percent = 100.0;
                guard.phase = DownloadPhase::Extract;
                guard.is_finalizing = true;
                guard.finalizing_msg = Some(msg.to_string());
            } else {
                println!(
                    "[download] [{}] 100.0% [Finalizing: {}]",
                    "=".repeat(25),
                    msg
                );
            }
        } else if line.starts_with("[Fixup") {
            let msg = "Correcting media container...";
            if is_rich {
                let mut guard = shared.lock().unwrap_or_else(|p| p.into_inner());
                guard.target_percent = 100.0;
                guard.phase = DownloadPhase::Finalize;
                guard.is_finalizing = true;
                guard.finalizing_msg = Some(msg.to_string());
            } else {
                println!(
                    "[download] [{}] 100.0% [Finalizing: {}]",
                    "=".repeat(25),
                    msg
                );
            }
        } else if line.starts_with("POSTPROCESS|") {
            let msg = "Packaging final media...";
            if is_rich {
                let mut guard = shared.lock().unwrap_or_else(|p| p.into_inner());
                guard.target_percent = 100.0;
                guard.phase = DownloadPhase::Finalize;
                guard.is_finalizing = true;
                guard.finalizing_msg = Some(msg.to_string());
            } else {
                println!(
                    "[download] [{}] 100.0% [Finalizing: {}]",
                    "=".repeat(25),
                    msg
                );
            }
        } else if is_stdout
            && !trimmed.is_empty()
            && !trimmed.starts_with("PROGRESS|")
            && !trimmed.starts_with("POSTPROCESS|")
            && !trimmed.starts_with('[')
            && !trimmed.starts_with("download:")
            && !trimmed.starts_with("postprocess:")
        {
            let p = PathBuf::from(trimmed);
            if p.is_file() || p.is_absolute() {
                captured_filepath = Some(p);
            }
        }
    }

    let _ = stdout_thread.join();
    let wait_res = child.wait();
    let stderr_output = stderr_thread.join().unwrap_or_default();
    unregister_direct_download();

    let _ = std::fs::remove_dir_all(&temp_dir);

    running.store(false, Ordering::Relaxed);
    let (peak_speed, samples) = if let Some(handle) = render_handle {
        let res = handle.join().unwrap_or((0.0, Vec::new()));
        print!("\r\x1b[2K");
        let _ = stdout().flush();
        res
    } else {
        (peak_speed_plain, Vec::new())
    };

    let status = wait_res.map_err(|e| format!("Failed to wait for yt-dlp: {}", e))?;

    let elapsed = download_start.elapsed();
    let size_bytes = captured_filepath
        .as_ref()
        .and_then(|p| std::fs::metadata(p).ok())
        .map(|m| m.len())
        .unwrap_or(last_plain_bytes);
    let avg_speed = if elapsed.as_secs_f64() > 0.0 {
        size_bytes as f64 / elapsed.as_secs_f64()
    } else {
        0.0
    };

    let stats = DownloadStats {
        size_bytes,
        avg_speed,
        peak_speed,
        samples,
        elapsed,
    };

    if status.success() {
        clear_taskbar_progress();
        set_terminal_title("DWDR - Complete");
        Ok(DownloadOutcome {
            path: captured_filepath,
            stats,
        })
    } else {
        clear_taskbar_progress();
        set_terminal_title("DWDR - Error");
        Err(extract_engine_error(&stderr_output))
    }
}

/// Downloads media asynchronously in worker threads, reporting progress and lifecycle via callbacks.
pub fn download_media_background<F, G>(
    url: &str,
    format_id: &str,
    yt_dlp_path: &Path,
    ffmpeg_path: &Path,
    download_dir: &str,
    on_spawn: F,
    on_progress: G,
) -> Result<Option<PathBuf>, String>
where
    F: FnOnce(&mut Child),
    G: Fn(f64, &str, &str, &str, &str, bool),
{
    download_media_background_ext(
        url,
        format_id,
        yt_dlp_path,
        ffmpeg_path,
        download_dir,
        None,
        false,
        None,
        on_spawn,
        move |pct, dwn, tot, spd, eta, _phase, fin| {
            let d_str = format_bytes_u64(dwn);
            let t_str = if tot > 0 {
                format_bytes_u64(tot)
            } else {
                "Unknown".to_string()
            };
            let s_str = format_speed(spd);
            let e_str = format_eta(eta);
            on_progress(pct, &d_str, &t_str, &s_str, &e_str, fin);
        },
    )
}

/// Downloads media asynchronously with custom template and overwrite flags.
#[allow(clippy::too_many_arguments)]
pub fn download_media_background_ext<F, G>(
    url: &str,
    format_id: &str,
    yt_dlp_path: &Path,
    ffmpeg_path: &Path,
    download_dir: &str,
    custom_output_template: Option<&str>,
    force_overwrites: bool,
    task_id: Option<&str>,
    on_spawn: F,
    on_progress: G,
) -> Result<Option<PathBuf>, String>
where
    F: FnOnce(&mut Child),
    G: Fn(f64, u64, u64, f64, Option<u64>, DownloadPhase, bool),
{
    download_media_background_full(
        url,
        format_id,
        yt_dlp_path,
        ffmpeg_path,
        download_dir,
        custom_output_template,
        force_overwrites,
        task_id,
        None,
        on_spawn,
        on_progress,
    )
}

/// Downloads media asynchronously with optional cookie authentication.
#[allow(clippy::too_many_arguments)]
pub fn download_media_background_full<F, G>(
    url: &str,
    format_id: &str,
    yt_dlp_path: &Path,
    ffmpeg_path: &Path,
    download_dir: &str,
    custom_output_template: Option<&str>,
    force_overwrites: bool,
    task_id: Option<&str>,
    cookies: Option<&str>,
    on_spawn: F,
    on_progress: G,
) -> Result<Option<PathBuf>, String>
where
    F: FnOnce(&mut Child),
    G: Fn(f64, u64, u64, f64, Option<u64>, DownloadPhase, bool),
{
    let validated_dir = validate_download_path(download_dir, true)
        .map_err(|e| format!("Invalid download directory: {}", e))?;
    let clean_download_dir = validated_dir.to_string_lossy();

    let mut cmd = build_ytdlp_download_cmd_full(
        yt_dlp_path,
        ffmpeg_path,
        url,
        format_id,
        &clean_download_dir,
        custom_output_template,
        force_overwrites,
        task_id,
        cookies,
    );

    #[cfg(unix)]
    {
        // Spawns worker in an isolated process group to enable tree-wide signal termination.
        cmd.process_group(0);
    }

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Failed to execute yt-dlp: {}", e))?;

    on_spawn(&mut child);

    let (tx, rx) = mpsc::channel();
    let tx_err = tx.clone();

    let stderr_handle = child.stderr.take();
    let stderr_thread = thread::spawn(move || {
        let mut err_output = Vec::new();
        if let Some(pipe) = stderr_handle {
            let reader = BufReader::new(pipe);
            read_lossy_lines(reader, |line| {
                if parse_progress_numeric(line).is_some()
                    || line.starts_with("POSTPROCESS|")
                    || line.starts_with("[Merger]")
                    || line.starts_with("[ExtractAudio]")
                    || line.starts_with("[Fixup")
                    || line.contains("Merging formats")
                {
                    let _ = tx_err.send((false, line.to_string()));
                } else {
                    err_output.extend_from_slice(line.as_bytes());
                    err_output.push(b'\n');
                }
            });
        }
        String::from_utf8_lossy(&err_output).to_string()
    });

    let is_audio_only = format_id.starts_with("audio:")
        || format_id.eq_ignore_ascii_case("audio")
        || format_id.eq_ignore_ascii_case("bestaudio");

    let expects_separate_audio = format_id.contains('+');
    let mut streams_seen: usize = 0;
    let mut current_stream_key: Option<String> = None;
    let mut postprocess_started = false;
    let mut video_downloaded: u64 = 0;
    let mut video_total: u64 = 0;
    let mut audio_downloaded: u64 = 0;
    let mut audio_total: u64 = 0;
    let mut captured_filepath: Option<PathBuf> = None;

    let stdout_handle = child.stdout.take();
    let tx_out = tx;
    let stdout_thread = thread::spawn(move || {
        if let Some(stdout_pipe) = stdout_handle {
            let reader = BufReader::new(stdout_pipe);
            read_lossy_lines(reader, |line| {
                let _ = tx_out.send((true, line.to_string()));
            });
        }
    });

    while let Ok((is_stdout, line)) = rx.recv() {
        let trimmed = line.trim();
        if let Some(num) = parse_progress_numeric(&line) {
            let is_audio_stream = is_audio_only
                || num
                    .vcodec
                    .as_deref()
                    .map(|v| v.eq_ignore_ascii_case("none"))
                    .unwrap_or(false);

            let stream_key = num.format_id.clone().unwrap_or_else(|| {
                if is_audio_stream {
                    "audio".to_string()
                } else {
                    "video".to_string()
                }
            });

            if current_stream_key.as_ref() != Some(&stream_key) {
                current_stream_key = Some(stream_key);
                streams_seen += 1;
            }

            let phase = if is_audio_stream {
                audio_downloaded = num.downloaded_bytes;
                if num.total_bytes > 0 {
                    audio_total = num.total_bytes;
                }
                DownloadPhase::Audio
            } else {
                video_downloaded = num.downloaded_bytes;
                if num.total_bytes > 0 {
                    video_total = num.total_bytes;
                }
                DownloadPhase::Video
            };

            let (combined_pct, total_bytes, downloaded_bytes) = combine_progress(
                expects_separate_audio,
                streams_seen,
                is_audio_stream,
                num.percent,
                video_downloaded,
                video_total,
                audio_downloaded,
                audio_total,
            );

            on_progress(
                combined_pct,
                downloaded_bytes,
                total_bytes,
                num.speed_bytes_sec,
                num.eta_seconds,
                phase,
                false,
            );
        } else if (line.starts_with("POSTPROCESS|")
            || line.starts_with("[Merger]")
            || line.starts_with("[ExtractAudio]")
            || line.starts_with("[Fixup")
            || line.contains("Merging formats"))
            && !postprocess_started
        {
            postprocess_started = true;
            let phase = if line.starts_with("[ExtractAudio]") {
                DownloadPhase::Extract
            } else {
                DownloadPhase::Finalize
            };
            on_progress(100.0, 0, 0, 0.0, None, phase, true);
        } else if is_stdout
            && !trimmed.is_empty()
            && !trimmed.starts_with("PROGRESS|")
            && !trimmed.starts_with("POSTPROCESS|")
            && !trimmed.starts_with('[')
            && !trimmed.starts_with("download:")
            && !trimmed.starts_with("postprocess:")
        {
            let p = PathBuf::from(trimmed);
            if p.is_file() || p.is_absolute() {
                captured_filepath = Some(p);
            }
        }
    }

    let _ = stdout_thread.join();
    let wait_res = child.wait();
    let stderr_output = stderr_thread.join().unwrap_or_default();
    let status = wait_res.map_err(|e| format!("Failed to wait for yt-dlp: {}", e))?;

    if status.success() {
        Ok(captured_filepath)
    } else {
        Err(extract_engine_error(&stderr_output))
    }
}

pub fn find_format_by_quality(formats: &[MediaFormat], target_quality: &str) -> Option<String> {
    let target = target_quality.trim().to_lowercase();
    if target == "best" {
        return formats.first().map(|f| f.format_id.clone());
    }
    if target == "always ask" {
        return None;
    }

    for format in formats {
        let res_lower = format.resolution.to_lowercase();
        let matched = if target == "8k" {
            res_lower.contains("4320") || res_lower.contains("8k")
        } else if target == "4k" {
            res_lower.contains("2160") || res_lower.contains("4k")
        } else if target == "2k" {
            res_lower.contains("1440") || res_lower.contains("2k")
        } else {
            res_lower.contains(&target)
        };

        if matched {
            return Some(format.format_id.clone());
        }
    }
    None
}

pub fn resolution_to_rank(res: &str) -> u64 {
    let lower = res.to_lowercase();
    if lower.contains("4320") || lower.contains("8k") {
        4320
    } else if lower.contains("2160") || lower.contains("4k") {
        2160
    } else if lower.contains("1440") || lower.contains("2k") {
        1440
    } else if lower.contains("1080") {
        1080
    } else if lower.contains("720") {
        720
    } else if lower.contains("480") {
        480
    } else if lower.contains("360") {
        360
    } else if lower.contains("240") {
        240
    } else if lower.contains("144") {
        144
    } else {
        0
    }
}

/// Attempts to find exact match for target quality, falling back to nearest lower resolution tier.
pub fn find_format_with_fallback(
    formats: &[MediaFormat],
    target_quality: &str,
) -> Option<(String, bool)> {
    if let Some(id) = find_format_by_quality(formats, target_quality) {
        return Some((id, false));
    }

    let target = target_quality.trim().to_lowercase();
    if target == "always ask" {
        return None;
    }

    let target_rank = resolution_to_rank(&target);
    if target_rank == 0 {
        return formats.first().map(|f| (f.format_id.clone(), true));
    }

    // Prefer closest lower resolution to respect user bandwidth constraints.
    let mut lower_candidates: Vec<(&MediaFormat, u64)> = formats
        .iter()
        .filter(|f| !f.is_audio())
        .map(|f| (f, resolution_to_rank(&f.resolution)))
        .filter(|(_, r)| *r <= target_rank && *r > 0)
        .collect();

    lower_candidates.sort_by_key(|a| std::cmp::Reverse(a.1));

    if let Some((best_lower, _)) = lower_candidates.first() {
        return Some((best_lower.format_id.clone(), true));
    }

    // Scale up only when no lower resolution tier exists.
    let mut all_video: Vec<(&MediaFormat, u64)> = formats
        .iter()
        .filter(|f| !f.is_audio())
        .map(|f| (f, resolution_to_rank(&f.resolution)))
        .collect();
    all_video.sort_by_key(|a| std::cmp::Reverse(a.1));
    if let Some((best_vid, _)) = all_video.first() {
        return Some((best_vid.format_id.clone(), true));
    }

    formats.first().map(|f| (f.format_id.clone(), true))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    use crate::ui::format_table::build_format_table_options;
    use crate::ui::theme::{ColorLevel, ThemeMode};

    #[test]
    fn test_find_format_by_quality() {
        let formats = vec![
            MediaFormat {
                format_id: "571".to_string(),
                ext: "webm".to_string(),
                resolution: "4320p".to_string(),
                filesize: Some(200_000_000.0),
                is_estimated: false,
                is_hdr: false,
                vcodec: "av01".to_string(),
                acodec: "none".to_string(),
                height: Some(4320),
            },
            MediaFormat {
                format_id: "313".to_string(),
                ext: "webm".to_string(),
                resolution: "4K".to_string(),
                filesize: Some(100_000_000.0),
                is_estimated: false,
                is_hdr: false,
                vcodec: "vp9".to_string(),
                acodec: "none".to_string(),
                height: Some(2160),
            },
            MediaFormat {
                format_id: "271".to_string(),
                ext: "webm".to_string(),
                resolution: "2K".to_string(),
                filesize: Some(50_000_000.0),
                is_estimated: false,
                is_hdr: false,
                vcodec: "vp9".to_string(),
                acodec: "none".to_string(),
                height: Some(1440),
            },
            MediaFormat {
                format_id: "22".to_string(),
                ext: "mp4".to_string(),
                resolution: "720p".to_string(),
                filesize: Some(25_000_000.0),
                is_estimated: false,
                is_hdr: false,
                vcodec: "avc1".to_string(),
                acodec: "mp4a".to_string(),
                height: Some(720),
            },
            MediaFormat {
                format_id: "18".to_string(),
                ext: "mp4".to_string(),
                resolution: "360p".to_string(),
                filesize: Some(10_000_000.0),
                is_estimated: false,
                is_hdr: false,
                vcodec: "avc1".to_string(),
                acodec: "mp4a".to_string(),
                height: Some(360),
            },
        ];

        assert_eq!(
            find_format_by_quality(&formats, "best"),
            Some("571".to_string())
        );
        assert_eq!(find_format_by_quality(&formats, "always ask"), None);
        assert_eq!(
            find_format_by_quality(&formats, "8k"),
            Some("571".to_string())
        );
        assert_eq!(
            find_format_by_quality(&formats, "4k"),
            Some("313".to_string())
        );
        assert_eq!(
            find_format_by_quality(&formats, "2k"),
            Some("271".to_string())
        );
        assert_eq!(
            find_format_by_quality(&formats, "720p"),
            Some("22".to_string())
        );
        assert_eq!(find_format_by_quality(&formats, "1080p"), None);
    }

    #[test]
    fn test_media_format_filesize_formatting() {
        let format_with_size = MediaFormat {
            format_id: "1".to_string(),
            ext: "mp4".to_string(),
            resolution: "720p".to_string(),
            filesize: Some(10_485_760.0),
            is_estimated: false,
            is_hdr: false,
            vcodec: "avc1".to_string(),
            acodec: "mp4a".to_string(),
            height: Some(720),
        };
        assert_eq!(format_with_size.format_filesize(), "10.0 MB");
        assert_eq!(format_with_size.size_in_mb(), "10.0 MB");

        let format_bytes_test = MediaFormat {
            format_id: "2".to_string(),
            ext: "mp4".to_string(),
            resolution: "720p".to_string(),
            filesize: Some(500.0),
            is_estimated: false,
            is_hdr: false,
            vcodec: "avc1".to_string(),
            acodec: "mp4a".to_string(),
            height: Some(720),
        };
        assert_eq!(format_bytes_test.format_filesize(), "500 B");

        let format_mb_test = MediaFormat {
            format_id: "3".to_string(),
            ext: "mp4".to_string(),
            resolution: "720p".to_string(),
            filesize: Some(1_500_000.0),
            is_estimated: false,
            is_hdr: false,
            vcodec: "avc1".to_string(),
            acodec: "mp4a".to_string(),
            height: Some(720),
        };
        assert_eq!(format_mb_test.format_filesize(), "1.4 MB");

        let format_gb_test = MediaFormat {
            format_id: "4".to_string(),
            ext: "mp4".to_string(),
            resolution: "720p".to_string(),
            filesize: Some(2_500_000_000.0),
            is_estimated: false,
            is_hdr: false,
            vcodec: "avc1".to_string(),
            acodec: "mp4a".to_string(),
            height: Some(720),
        };
        assert_eq!(format_gb_test.format_filesize(), "2.3 GB");

        let format_without_size = MediaFormat {
            format_id: "5".to_string(),
            ext: "mp4".to_string(),
            resolution: "Audio".to_string(),
            filesize: None,
            is_estimated: false,
            is_hdr: false,
            vcodec: "none".to_string(),
            acodec: "opus".to_string(),
            height: None,
        };
        assert_eq!(format_without_size.format_filesize(), "Stream Size Pending");
        assert_eq!(format_without_size.size_in_mb(), "Stream Size Pending");

        let format_estimated = MediaFormat {
            format_id: "6".to_string(),
            ext: "mp4".to_string(),
            resolution: "1080p".to_string(),
            filesize: Some(44_040_192.0),
            is_estimated: true,
            is_hdr: false,
            vcodec: "avc1".to_string(),
            acodec: "mp4a".to_string(),
            height: Some(1080),
        };
        assert_eq!(format_estimated.format_filesize(), "~42.0 MB (Est.)");
    }

    #[test]
    fn test_resolve_resolution() {
        assert_eq!(
            resolve_resolution(Some(1012), Some(1920), Some("1080p50")),
            Some(("1080p", 1080))
        );
        assert_eq!(
            resolve_resolution(Some(2026), Some(3840), Some("2160p50")),
            Some(("4K", 2160))
        );
        assert_eq!(
            resolve_resolution(Some(1350), Some(2560), Some("1440p50")),
            Some(("2K", 1440))
        );
        assert_eq!(
            resolve_resolution(Some(676), Some(1280), Some("720p50")),
            Some(("720p", 720))
        );
        assert_eq!(
            resolve_resolution(Some(1080), Some(1920), None),
            Some(("1080p", 1080))
        );
        // 4:3 standard aspect ratio (e.g. 640x480) must map to 480p, not 360p
        assert_eq!(
            resolve_resolution(Some(480), Some(640), None),
            Some(("480p", 480))
        );
        // 4:3 standard aspect ratio (e.g. 1440x1080) must map to 1080p, not 720p
        assert_eq!(
            resolve_resolution(Some(1080), Some(1440), None),
            Some(("1080p", 1080))
        );
        // Letterboxed widescreen (e.g. 1920x800) must map to 1080p without note
        assert_eq!(
            resolve_resolution(Some(800), Some(1920), None),
            Some(("1080p", 1080))
        );
        assert_eq!(resolve_resolution(None, None, None), None);
    }

    #[test]
    fn test_parse_progress_line() {
        let line = "PROGRESS| 45.2%|35.40MiB|78.20MiB|2.10MiB/s|00:20";
        let parsed = parse_progress_line(line);
        assert!(parsed.is_some());
        let (percent, downloaded, total, speed, eta) = parsed.unwrap();
        assert!((percent - 45.2).abs() < f64::EPSILON);
        assert_eq!(downloaded, "35.40MiB");
        assert_eq!(total, "78.20MiB");
        assert_eq!(speed, "2.10MiB/s");
        assert_eq!(eta, "00:20");
    }

    #[test]
    fn test_parse_size_speed_eta_helpers() {
        assert_eq!(parse_size_bytes("50.0MiB"), Some(50 * 1024 * 1024));
        assert_eq!(parse_size_bytes("~10.00MiB"), Some(10 * 1024 * 1024));
        assert_eq!(parse_size_bytes("~ 10.00MiB"), Some(10 * 1024 * 1024));
        assert_eq!(parse_size_bytes("10.00 MiB"), Some(10 * 1024 * 1024));
        assert_eq!(parse_size_bytes("5M"), Some(5 * 1024 * 1024));
        assert_eq!(parse_size_bytes("1024"), Some(1024));
        assert_eq!(parse_size_bytes("NA"), None);
        assert_eq!(parse_size_bytes("Unknown"), None);
        assert_eq!(parse_size_bytes(""), None);

        assert_eq!(parse_speed_bytes("2.50MiB/s"), Some(2.5 * 1024.0 * 1024.0));
        assert_eq!(parse_speed_bytes("1000/s"), Some(1000.0));

        assert_eq!(parse_eta_secs("00:20"), Some(20));
        assert_eq!(parse_eta_secs("01:30"), Some(90));
        assert_eq!(parse_eta_secs("01:00:00"), Some(3600));
        assert_eq!(parse_eta_secs("42"), Some(42));
        assert_eq!(parse_eta_secs("--:--"), None);

        let fallback_line = "PROGRESS|100.0%|NA|NA|NA|NA";
        let (p, d, t, s, e) = parse_progress_line(fallback_line).unwrap();
        assert!((p - 100.0).abs() < f64::EPSILON);
        assert_eq!(d, "Unknown");
        assert_eq!(t, "Unknown");
        assert_eq!(s, "Unknown");
        assert_eq!(e, "--:--");

        assert!(parse_progress_line("INVALID|FORMAT").is_none());
        assert!(parse_progress_line("PROGRESS|10%|10MB").is_none());
    }

    #[test]
    fn test_parse_progress_line_ext_vcodec() {
        let video_line = "PROGRESS| 45.2%|35.40MiB|78.20MiB|2.10MiB/s|00:20|avc1.640028";
        let parsed_video = parse_progress_line_ext(video_line).unwrap();
        assert_eq!(parsed_video.5, Some("avc1.640028".to_string()));

        let audio_line = "PROGRESS| 10.0%|1.00MiB|10.00MiB|1.50MiB/s|00:06|none";
        let parsed_audio = parse_progress_line_ext(audio_line).unwrap();
        assert_eq!(parsed_audio.5, Some("none".to_string()));

        let legacy_line = "PROGRESS| 50.0%|25.00MiB|50.00MiB|2.00MiB/s|00:12";
        let parsed_legacy = parse_progress_line_ext(legacy_line).unwrap();
        assert_eq!(parsed_legacy.5, None);
    }

    #[test]
    fn test_render_progress_bar() {
        let bar = render_progress_bar(50.0, "35.40MiB", "78.20MiB", "2.10MiB/s", "00:20");
        assert!(bar.contains("50.0%"));
        assert!(bar.contains("35.40MiB /  78.20MiB"));
        assert!(bar.contains("2.10MiB/s"));
        assert!(bar.contains("00:20"));
    }
    #[test]
    fn test_format_bytes_boundaries() {
        assert_eq!(format_bytes(0.0), "0 B");
        assert_eq!(format_bytes(1.0), "1 B");
        assert_eq!(format_bytes(1023.0), "1023 B");
        assert_eq!(format_bytes(1024.0), "1.0 KB");
        assert_eq!(format_bytes(1048576.0), "1.0 MB");
        assert_eq!(format_bytes(1073741824.0), "1.0 GB");
        assert_eq!(format_bytes(1099511627776.0), "1.0 TB");
        assert_eq!(format_bytes(5497558138880.0), "5.0 TB");
    }

    #[test]
    fn test_resolve_resolution_vertical_and_edge() {
        // YouTube Shorts (1080x1920)
        assert_eq!(
            resolve_resolution(Some(1920), Some(1080), None),
            Some(("1080p", 1080))
        );
        // YouTube Shorts (720x1280)
        assert_eq!(
            resolve_resolution(Some(1280), Some(720), None),
            Some(("720p", 720))
        );
        // 8K Landscape
        assert_eq!(
            resolve_resolution(Some(4320), Some(7680), None),
            Some(("8K", 4320))
        );
        // Format notes
        assert_eq!(
            resolve_resolution(None, None, Some("4320p60")),
            Some(("8K", 4320))
        );
        assert_eq!(
            resolve_resolution(None, None, Some("1080p60 HDR")),
            Some(("1080p", 1080))
        );
        assert_eq!(
            resolve_resolution(None, None, Some("480p")),
            Some(("480p", 480))
        );
        assert_eq!(
            resolve_resolution(None, None, Some("360p")),
            Some(("360p", 360))
        );
        assert_eq!(
            resolve_resolution(None, None, Some("240p")),
            Some(("240p", 240))
        );
        assert_eq!(
            resolve_resolution(None, None, Some("144p")),
            Some(("144p", 144))
        );
    }

    #[test]
    fn test_parse_progress_line_whitespace_and_extra_pipes() {
        let line = "PROGRESS|  65.5%  |  20.00MiB  |  40.00MiB  |  2.50MiB/s  |  00:08  ";
        let (p, d, t, s, e) = parse_progress_line(line).unwrap();
        assert!((p - 65.5).abs() < f64::EPSILON);
        assert_eq!(d, "20.00MiB");
        assert_eq!(t, "40.00MiB");
        assert_eq!(s, "2.50MiB/s");
        assert_eq!(e, "00:08");

        let line_extra = "PROGRESS|70%|30MB|50MB|3MB/s|00:05|extra|ignored";
        let (p, d, t, s, e) = parse_progress_line(line_extra).unwrap();
        assert!((p - 70.0).abs() < f64::EPSILON);
        assert_eq!(d, "30MB");
        assert_eq!(t, "50MB");
        assert_eq!(s, "3MB/s");
        assert_eq!(e, "00:05");

        let non_num = "PROGRESS|invalid|10MB|20MB|1MB/s|00:10";
        let (p, _, _, _, _) = parse_progress_line(non_num).unwrap();
        assert_eq!(p, 0.0);
    }

    #[test]
    fn test_render_progress_bar_clamping() {
        // Zero percent
        let bar_zero = render_progress_bar(0.0, "0B", "10MB", "0MB/s", "--:--");
        assert!(bar_zero.contains("0.0%"));

        // Over 100 percent
        let bar_over = render_progress_bar(150.0, "15MB", "10MB", "1MB/s", "00:00");
        assert!(bar_over.contains("150.0%"));

        // Negative percent
        let bar_neg = render_progress_bar(-10.0, "0B", "10MB", "0MB/s", "--:--");
        assert!(bar_neg.contains("-10.0%"));
    }

    #[test]
    fn test_find_format_by_quality_case_and_whitespace() {
        let formats = vec![
            MediaFormat {
                format_id: "1080".to_string(),
                ext: "mp4".to_string(),
                resolution: "1080p".to_string(),
                filesize: Some(50_000_000.0),
                is_estimated: false,
                is_hdr: false,
                vcodec: "avc1".to_string(),
                acodec: "none".to_string(),
                height: Some(1080),
            },
            MediaFormat {
                format_id: "audio1".to_string(),
                ext: "m4a".to_string(),
                resolution: "Audio".to_string(),
                filesize: Some(5_000_000.0),
                is_estimated: false,
                is_hdr: false,
                vcodec: "none".to_string(),
                acodec: "mp4a".to_string(),
                height: None,
            },
        ];

        assert_eq!(
            find_format_by_quality(&formats, "1080P"),
            Some("1080".to_string())
        );
        assert_eq!(
            find_format_by_quality(&formats, "  1080p  "),
            Some("1080".to_string())
        );
        assert_eq!(
            find_format_by_quality(&formats, "AUDIO"),
            Some("audio1".to_string())
        );
        assert_eq!(
            find_format_by_quality(&formats, "audio"),
            Some("audio1".to_string())
        );
        assert_eq!(find_format_by_quality(&formats, "9999p"), None);
    }

    #[test]
    fn test_calculate_media_filesize_tiers() {
        // Tier 1: exact size
        let (size, est) = calculate_media_filesize(
            Some(10_000_000.0),
            Some(8_000_000.0),
            Some(2000.0),
            Some(60.0),
            Some(1080),
            true,
        );
        assert_eq!(size, Some(10_000_000.0));
        assert!(!est);

        // Tier 2: engine approx
        let (size, est) = calculate_media_filesize(
            None,
            Some(8_000_000.0),
            Some(2000.0),
            Some(60.0),
            Some(1080),
            true,
        );
        assert_eq!(size, Some(8_000_000.0));
        assert!(est);

        // Tier 3: from bitrate and duration (2000 kbps * 60 s = 15 MB)
        let (size, est) =
            calculate_media_filesize(None, None, Some(2000.0), Some(60.0), Some(1080), true);
        assert_eq!(size, Some(15_000_000.0));
        assert!(est);

        // Tier 4: from profile (1080p -> 2500 kbps * 60 s = 18.75 MB)
        let (size, est) = calculate_media_filesize(None, None, None, Some(60.0), Some(1080), true);
        assert_eq!(size, Some(18_750_000.0));
        assert!(est);

        // Tier 4 Audio: (128 kbps * 60 s = 960,000 bytes)
        let (size, est) = calculate_media_filesize(None, None, None, Some(60.0), None, false);
        assert_eq!(size, Some(960_000.0));
        assert!(est);

        // Tier 5: Live stream (duration is None)
        let (size, est) = calculate_media_filesize(None, None, None, None, Some(1080), true);
        assert_eq!(size, None);
        assert!(!est);
    }

    #[test]
    fn test_render_progress_bar_responsive_widths() {
        // Full width (120 cols) should have 25-char bar
        let full = render_progress_bar_with_width(50.0, "50MB", "100MB", "5MB/s", "00:10", 120);
        assert!(full.contains("[download] [="));
        assert!(full.contains("50.0%"));
        assert!(!full.contains('\r'));
        assert!(visible_width(&full) <= 120);

        // Standard terminal (80 cols) should fit within 80 characters without visual wrapping
        let std_term = render_progress_bar_with_width(50.0, "50MB", "100MB", "5MB/s", "00:10", 80);
        assert!(std_term.contains("50.0%"));
        assert!(!std_term.contains('\r'));
        assert!(visible_width(&std_term) <= 80);

        // Compact mode for 60 cols
        let compact = render_progress_bar_with_width(50.0, "50MB", "100MB", "5MB/s", "00:10", 60);
        assert!(!compact.contains('\r'));
        assert!(visible_width(&compact) <= 60);

        // Ultra-compact mode for 40 cols
        let ultra = render_progress_bar_with_width(50.0, "50MB", "100MB", "5MB/s", "00:10", 40);
        assert!(!ultra.contains('\r'));
        assert!(visible_width(&ultra) <= 40);
    }

    #[test]
    fn test_extract_engine_error_status_codes() {
        // HTTP 403 Forbidden
        let stderr_403 = "WARNING: [youtube] Some warning\nERROR: unable to download video data: HTTP Error 403: Forbidden\n";
        assert_eq!(
            extract_engine_error(stderr_403),
            "unable to download video data: HTTP Error 403: Forbidden"
        );

        // HTTP 429 Too Many Requests
        let stderr_429 = "ERROR: HTTP Error 429: Too Many Requests\n";
        assert_eq!(
            extract_engine_error(stderr_429),
            "HTTP Error 429: Too Many Requests"
        );

        // Video unavailable
        let stderr_unavail = "ERROR: [youtube] rNpaYXAO2K4: Video unavailable\n";
        assert_eq!(
            extract_engine_error(stderr_unavail),
            "[youtube] rNpaYXAO2K4: Video unavailable"
        );

        // Filesystem error (Errno 22)
        let stderr_errno22 = "ERROR: Unable to download video: [Errno 22] Invalid argument\n";
        assert_eq!(
            extract_engine_error(stderr_errno22),
            "Unable to download video: [Errno 22] Invalid argument"
        );

        // Empty stderr fallback
        assert_eq!(
            extract_engine_error("   "),
            "yt-dlp process exited with an error"
        );

        // Plain stderr without ERROR tag
        let stderr_plain = "Command exited with status code 127";
        assert_eq!(
            extract_engine_error(stderr_plain),
            "Command exited with status code 127"
        );

        // Lowercase error tag
        let stderr_lower = "error: connection reset by peer";
        assert_eq!(
            extract_engine_error(stderr_lower),
            "connection reset by peer"
        );
    }

    #[test]
    fn test_build_ytdlp_download_cmd_flags() {
        let yt = Path::new("yt-dlp");
        let ff = Path::new("ffmpeg");
        let cmd =
            build_ytdlp_download_cmd(yt, ff, "https://youtu.be/123", "302", "/downloads/path");
        let args: Vec<String> = cmd
            .get_args()
            .map(|a| a.to_string_lossy().to_string())
            .collect();

        assert!(args.contains(&"-P".to_string()));
        assert!(args.contains(&"/downloads/path".to_string()));
        assert!(args.contains(&"temp:/downloads/path/.dwdr_direct".to_string()));
        assert!(args.contains(&"-o".to_string()));
        assert!(args.contains(&"%(title)s.%(ext)s".to_string()));
        assert!(args.contains(&"--windows-filenames".to_string()));
        assert!(args.contains(&"--trim-filenames".to_string()));
        assert!(args.contains(&"160".to_string()));
        assert!(args.contains(&"--no-playlist".to_string()));
        assert!(args.contains(&"--no-warnings".to_string()));
        assert!(args.contains(&"--socket-timeout".to_string()));
        assert!(args.contains(&"30".to_string()));
        assert!(args.contains(&"--merge-output-format".to_string()));
        assert!(args.contains(&"mp4".to_string()));
        assert!(args.contains(&"--encoding".to_string()));
        assert!(args.contains(&"utf-8".to_string()));
        assert!(args.contains(&"302+bestaudio/302/best".to_string()));

        let cmd_best =
            build_ytdlp_download_cmd(yt, ff, "https://youtu.be/123", "best", "/downloads/path");
        let args_best: Vec<String> = cmd_best
            .get_args()
            .map(|a| a.to_string_lossy().to_string())
            .collect();
        assert!(args_best.contains(&"bestvideo+bestaudio/best".to_string()));

        let cmd_direct = build_ytdlp_download_cmd(
            yt,
            ff,
            "https://youtu.be/123",
            "direct:18",
            "/downloads/path",
        );
        let args_direct: Vec<String> = cmd_direct
            .get_args()
            .map(|a| a.to_string_lossy().to_string())
            .collect();
        assert!(args_direct.contains(&"18".to_string()));
        assert!(!args_direct.contains(&"18+bestaudio/18/best".to_string()));

        let cmd_audio = build_ytdlp_download_cmd(
            yt,
            ff,
            "https://youtu.be/123",
            "audio:140",
            "/downloads/path",
        );
        let args_audio: Vec<String> = cmd_audio
            .get_args()
            .map(|a| a.to_string_lossy().to_string())
            .collect();
        assert!(args_audio.contains(&"140".to_string()));
        assert!(!args_audio.contains(&"140+bestaudio/140/best".to_string()));
        assert!(!args_audio.contains(&"--merge-output-format".to_string()));
    }

    #[test]
    fn test_parse_video_info_json_valid_and_invalid() {
        // Invalid JSON
        let invalid = parse_video_info_json("not json {{{{");
        assert!(invalid.is_err());
        assert!(invalid.unwrap_err().contains("Failed to parse JSON"));

        // Missing formats array synthesizes fallback format
        let fallback = parse_video_info_json(r#"{"title": "Test Video"}"#).unwrap();
        assert_eq!(fallback.title, "Test Video");
        assert_eq!(fallback.formats.len(), 1);
        assert_eq!(fallback.formats[0].format_id, "best");
        assert_eq!(fallback.formats[0].resolution, "Default");

        // Formats with unprobed codecs (vcodec/acodec null) detected via extension/dimensions
        let unprobed_json = r#"{
            "title": "Unprobed Video",
            "formats": [
                {
                    "format_id": "http-720p",
                    "ext": "mp4",
                    "width": 1280,
                    "height": 720
                }
            ]
        }"#;
        let unprobed = parse_video_info_json(unprobed_json).unwrap();
        assert_eq!(unprobed.formats.len(), 1);
        assert_eq!(unprobed.formats[0].format_id, "http-720p");
        assert_eq!(unprobed.formats[0].resolution, "720p");
        assert_eq!(unprobed.formats[0].vcodec, "unknown");

        // Formats with non-standard format_note fallback
        let non_standard_json = r#"{
            "title": "Custom Note Video",
            "formats": [
                {
                    "format_id": "src",
                    "ext": "mp4",
                    "format_note": "Source"
                }
            ]
        }"#;
        let non_standard = parse_video_info_json(non_standard_json).unwrap();
        assert_eq!(non_standard.formats.len(), 1);
        assert_eq!(non_standard.formats[0].format_id, "src");
        assert_eq!(non_standard.formats[0].resolution, "Source");

        // WebM audio streams (e.g. YouTube format 251: vcodec "none", acodec "opus") must be classified as Audio
        let webm_audio_json = r#"{
            "title": "WebM Audio Video",
            "formats": [
                {
                    "format_id": "251",
                    "ext": "webm",
                    "vcodec": "none",
                    "acodec": "opus",
                    "abr": 160.0,
                    "filesize": 3500000.0
                }
            ]
        }"#;
        let webm_audio = parse_video_info_json(webm_audio_json).unwrap();
        assert_eq!(webm_audio.formats.len(), 1);
        assert_eq!(webm_audio.formats[0].format_id, "251");
        assert_eq!(webm_audio.formats[0].resolution, "Audio");
        assert_eq!(webm_audio.formats[0].vcodec, "none");

        // Fallback format calculation computes estimated filesize from approximate size and bitrate
        let fallback_with_size_json = r#"{
            "title": "Fallback Size Video",
            "duration": 60.0,
            "filesize_approx": 12000000.0
        }"#;
        let fallback_with_size = parse_video_info_json(fallback_with_size_json).unwrap();
        assert_eq!(fallback_with_size.formats.len(), 1);
        assert_eq!(fallback_with_size.formats[0].filesize, Some(12000000.0));
        assert!(fallback_with_size.formats[0].is_estimated);

        // Valid minimal payload
        let valid_json = r#"{
            "title": "Sample Video",
            "duration": 120.0,
            "formats": [
                {
                    "format_id": "137",
                    "ext": "mp4",
                    "width": 1920,
                    "height": 1080,
                    "vcodec": "avc1",
                    "acodec": "none",
                    "filesize": 50000000.0
                },
                {
                    "format_id": "140",
                    "ext": "m4a",
                    "vcodec": "none",
                    "acodec": "mp4a",
                    "filesize": 5000000.0
                }
            ]
        }"#;

        let parsed = parse_video_info_json(valid_json).unwrap();
        assert_eq!(parsed.title, "Sample Video");
        assert_eq!(parsed.duration, Some(120.0));
        assert_eq!(parsed.formats.len(), 2);
        assert_eq!(parsed.formats[0].resolution, "1080p");
        assert_eq!(parsed.formats[1].resolution, "Audio");
    }

    #[cfg(unix)]
    #[test]
    fn test_fetch_video_info_simulated_engine_errors() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mock_engine = temp_dir.path().join("mock_ytdlp.sh");

        // Create mock executable that simulates an HTTP 403 error
        std::fs::write(
            &mock_engine,
            "#!/bin/sh\necho \x27ERROR: unable to download video data: HTTP Error 403: Forbidden\x27 >&2\nexit 1\n",
        )
        .unwrap();

        std::fs::set_permissions(&mock_engine, std::fs::Permissions::from_mode(0o755)).unwrap();

        let result = fetch_video_info("https://youtube.com/watch?v=mock", &mock_engine);
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            "unable to download video data: HTTP Error 403: Forbidden"
        );

        // Use a separate mock script to avoid ETXTBSY (text file busy) on Unix when rewriting an executed file
        let mock_engine_ok = temp_dir.path().join("mock_ytdlp_ok.sh");
        let mock_json = r#"{"title": "Offline Mock Video", "duration": 60, "formats": [{"format_id": "18", "ext": "mp4", "width": 640, "height": 360, "vcodec": "avc1", "acodec": "mp4a", "filesize": 1000000}]}"#;
        std::fs::write(
            &mock_engine_ok,
            format!("#!/bin/sh\ncat << 'EOF'\n{}\nEOF\nexit 0\n", mock_json),
        )
        .unwrap();
        std::fs::set_permissions(&mock_engine_ok, std::fs::Permissions::from_mode(0o755)).unwrap();

        let result_ok = fetch_video_info("https://youtube.com/watch?v=mock", &mock_engine_ok);
        assert!(result_ok.is_ok());
        let info = result_ok.unwrap();
        assert_eq!(info.title, "Offline Mock Video");
        assert_eq!(info.formats.len(), 1);
        assert_eq!(info.formats[0].format_id, "18");
    }

    #[test]
    fn test_clean_partial_files() {
        let temp_dir = tempfile::tempdir().unwrap();
        let dir_path = temp_dir.path();

        let part1 = dir_path.join("Consistent Hashing - System Design.f248.webm.part");
        let part2 = dir_path.join("Consistent Hashing - System Design.f251.webm");
        let ytdl_file = dir_path.join("Consistent Hashing - System Design.ytdl");
        let completed_file = dir_path.join("Consistent Hashing - System Design.mp4");
        let unrelated_file = dir_path.join("Other Video.mp4");

        std::fs::write(&part1, "test part data").unwrap();
        std::fs::write(&part2, "test chunk data").unwrap();
        std::fs::write(&ytdl_file, "test ytdl data").unwrap();
        std::fs::write(&completed_file, "test completed data").unwrap();
        std::fs::write(&unrelated_file, "test other data").unwrap();

        let removed = clean_partial_files(
            dir_path.to_str().unwrap(),
            Some("Consistent Hashing - System Design"),
        );

        assert_eq!(removed, 3);
        assert!(!part1.exists());
        assert!(!part2.exists());
        assert!(!ytdl_file.exists());
        assert!(completed_file.exists());
        assert!(unrelated_file.exists());

        // Empty title must not delete unrelated partial files
        let other_part = dir_path.join("Important Video.part");
        std::fs::write(&other_part, "other data").unwrap();
        let removed_empty = clean_partial_files(dir_path.to_str().unwrap(), Some(""));
        assert_eq!(removed_empty, 0);
        assert!(other_part.exists());

        // Temp directory removal
        let temp_subdir = dir_path.join("Temp Video.temp");
        std::fs::create_dir(&temp_subdir).unwrap();
        let removed_dir = clean_partial_files(dir_path.to_str().unwrap(), Some("Temp Video"));
        assert_eq!(removed_dir, 1);
        assert!(!temp_subdir.exists());
    }

    #[test]
    fn test_parse_video_info_prefers_exact_progressive_over_hls_estimate() {
        let json_payload = r#"{
            "title": "Exact vs Estimate Test",
            "duration": 100.0,
            "formats": [
                {
                    "format_id": "312",
                    "ext": "mp4",
                    "width": 1920,
                    "height": 1080,
                    "vcodec": "avc1.64002",
                    "acodec": "none",
                    "tbr": 6000.0,
                    "protocol": "m3u8_native"
                },
                {
                    "format_id": "299",
                    "ext": "mp4",
                    "width": 1920,
                    "height": 1080,
                    "vcodec": "avc1.64002",
                    "acodec": "none",
                    "filesize": 75000000.0,
                    "protocol": "https"
                },
                {
                    "format_id": "999",
                    "ext": "m4a",
                    "vcodec": "none",
                    "acodec": "mp4a",
                    "tbr": 320.0,
                    "protocol": "m3u8_native"
                },
                {
                    "format_id": "140",
                    "ext": "m4a",
                    "vcodec": "none",
                    "acodec": "mp4a",
                    "filesize": 4000000.0,
                    "protocol": "https"
                }
            ]
        }"#;

        let parsed = parse_video_info_json(json_payload).unwrap();
        assert_eq!(parsed.formats.len(), 2);

        // 1080p must select format 299 with exact filesize and is_estimated: false
        assert_eq!(parsed.formats[0].format_id, "299");
        assert_eq!(parsed.formats[0].resolution, "1080p");
        assert_eq!(parsed.formats[0].filesize, Some(75000000.0));
        assert!(!parsed.formats[0].is_estimated);

        // Audio must select format 140 with exact filesize and is_estimated: false
        assert_eq!(parsed.formats[1].format_id, "140");
        assert_eq!(parsed.formats[1].resolution, "Audio");
        assert_eq!(parsed.formats[1].filesize, Some(4000000.0));
        assert!(!parsed.formats[1].is_estimated);
    }

    #[test]
    fn test_parse_video_info_falls_back_to_estimate_when_no_exact_size() {
        let json_payload = r#"{
            "title": "Fallback Estimate Test",
            "duration": 60.0,
            "formats": [
                {
                    "format_id": "hls_720",
                    "ext": "mp4",
                    "width": 1280,
                    "height": 720,
                    "vcodec": "avc1",
                    "acodec": "none",
                    "tbr": 2500.0,
                    "protocol": "m3u8_native"
                }
            ]
        }"#;

        let parsed = parse_video_info_json(json_payload).unwrap();
        assert_eq!(parsed.formats.len(), 1);
        assert_eq!(parsed.formats[0].format_id, "hls_720");
        assert_eq!(parsed.formats[0].resolution, "720p");
        assert!(parsed.formats[0].is_estimated);
        assert!(parsed.formats[0].filesize.is_some());
    }

    #[test]
    fn test_resolve_resolution_square_and_vr() {
        assert_eq!(
            resolve_resolution(Some(1080), Some(1080), None),
            Some(("1080p", 1080))
        );
        assert_eq!(
            resolve_resolution(Some(720), Some(720), None),
            Some(("720p", 720))
        );
        assert_eq!(
            resolve_resolution(Some(2880), Some(2880), None),
            Some(("4K", 2160))
        );
        assert_eq!(
            resolve_resolution(Some(4320), Some(4320), None),
            Some(("8K", 4320))
        );

        assert_eq!(
            resolve_resolution(Some(1920), Some(3840), None),
            Some(("4K", 2160))
        );
    }

    #[test]
    fn test_parse_video_info_retains_square_video() {
        let json_payload = r#"{
            "title": "Square Video Test",
            "duration": 30.0,
            "formats": [
                {
                    "format_id": "sq_1080",
                    "ext": "mp4",
                    "width": 1080,
                    "height": 1080,
                    "vcodec": "avc1",
                    "acodec": "mp4a",
                    "filesize": 15000000.0,
                    "protocol": "https"
                }
            ]
        }"#;

        let parsed = parse_video_info_json(json_payload).unwrap();
        assert_eq!(parsed.formats.len(), 1);
        assert_eq!(parsed.formats[0].format_id, "sq_1080");
        assert_eq!(parsed.formats[0].resolution, "1080p");
    }

    #[test]
    fn test_parse_video_info_parses_missing_width_height_from_resolution_string() {
        let json_payload = r#"{
            "title": "Resolution String Test",
            "duration": 45.0,
            "formats": [
                {
                    "format_id": "hls_no_dims",
                    "ext": "mp4",
                    "resolution": "1920x1080",
                    "vcodec": "avc1",
                    "acodec": "none",
                    "tbr": 4000.0,
                    "protocol": "https"
                }
            ]
        }"#;

        let parsed = parse_video_info_json(json_payload).unwrap();
        assert_eq!(parsed.formats.len(), 1);
        assert_eq!(parsed.formats[0].format_id, "hls_no_dims");
        assert_eq!(parsed.formats[0].resolution, "1080p");
        assert_eq!(parsed.formats[0].height, Some(1080));
    }

    #[test]
    fn test_parse_video_info_hdr_and_codec_preference() {
        let json_payload = r#"{
            "title": "HDR and Codec Preference Test",
            "duration": 120.0,
            "formats": [
                {
                    "format_id": "4k_avc_sdr",
                    "ext": "mp4",
                    "width": 3840,
                    "height": 2160,
                    "vcodec": "avc1.640033",
                    "acodec": "none",
                    "dynamic_range": "SDR",
                    "filesize": 150000000.0,
                    "protocol": "https"
                },
                {
                    "format_id": "4k_av1_hdr",
                    "ext": "mp4",
                    "width": 3840,
                    "height": 2160,
                    "vcodec": "av01.0.12M.10",
                    "acodec": "none",
                    "dynamic_range": "HDR",
                    "filesize": 90000000.0,
                    "protocol": "https"
                }
            ]
        }"#;

        let parsed = parse_video_info_json(json_payload).unwrap();
        assert_eq!(parsed.formats.len(), 1);
        // Must select the AV1 HDR format over the AVC SDR format despite smaller filesize
        assert_eq!(parsed.formats[0].format_id, "4k_av1_hdr");
        assert_eq!(parsed.formats[0].resolution, "4K");
        assert!(parsed.formats[0].is_hdr);
    }

    #[test]
    fn test_parse_video_info_hdr10_dynamic_range() {
        let json_payload = r#"{
            "title": "HDR10 Test Video",
            "duration": 60.0,
            "formats": [
                {
                    "format_id": "hdr10_fmt",
                    "ext": "mp4",
                    "width": 1920,
                    "height": 1080,
                    "vcodec": "av01.0.08M.10",
                    "acodec": "none",
                    "dynamic_range": "HDR10",
                    "filesize": 50000000.0,
                    "protocol": "https"
                }
            ]
        }"#;

        let parsed = parse_video_info_json(json_payload).unwrap();
        assert_eq!(parsed.formats.len(), 1);
        assert!(parsed.formats[0].is_hdr);

        let options = build_format_table_options(
            &parsed.formats,
            "1080p",
            90,
            &ThemeMode::Ascii,
            ColorLevel::None,
        );
        assert_eq!(options.len(), 1);
        assert!(options[0].contains("[HDR]"));
    }

    #[test]
    fn test_parse_video_info_sanitizes_title_injection() {
        let json_payload = r#"{
            "title": "Pwned\u001b[2J\u001b]0;pwned\u0007 Title",
            "duration": 60.0,
            "formats": []
        }"#;

        let parsed = parse_video_info_json(json_payload).unwrap();
        assert_eq!(parsed.title, "Pwned Title");
    }

    #[test]
    fn test_parse_video_info_sanitizes_format_metadata() {
        let json_payload = r#"{
            "title": "Escape Sequence Test",
            "duration": 60.0,
            "formats": [
                {
                    "format_id": "22\u001b[0m",
                    "ext": "mp4\u001b[31m",
                    "format_note": "1080p\u001b[2J\u001b]0;pwned\u0007",
                    "vcodec": "avc1\u001b[32m",
                    "acodec": "none",
                    "height": 1080,
                    "width": 1920,
                    "filesize": 50000000.0,
                    "protocol": "https"
                }
            ]
        }"#;

        let parsed = parse_video_info_json(json_payload).unwrap();
        assert_eq!(parsed.formats.len(), 1);
        let f = &parsed.formats[0];

        assert!(!f.format_id.contains('\u{1b}') && !f.format_id.contains('\u{7}'));
        assert!(!f.ext.contains('\u{1b}') && !f.ext.contains('\u{7}'));
        assert!(!f.resolution.contains('\u{1b}') && !f.resolution.contains('\u{7}'));
        assert!(!f.vcodec.contains('\u{1b}') && !f.vcodec.contains('\u{7}'));
        assert!(!f.acodec.contains('\u{1b}') && !f.acodec.contains('\u{7}'));

        let summary = f.summary();
        assert!(!summary.contains('\u{1b}') && !summary.contains('\u{7}'));

        let picker_rows = build_format_table_options(
            &parsed.formats,
            "1080p",
            90,
            &ThemeMode::Ascii,
            ColorLevel::None,
        );
        assert_eq!(picker_rows.len(), 1);
        assert!(!picker_rows[0].contains('\u{1b}') && !picker_rows[0].contains('\u{7}'));
    }

    #[test]
    fn test_combine_progress() {
        // 1. Single video (expects_separate_audio = false, streams_seen = 1)
        let (pct, tot, dwn) = combine_progress(false, 1, false, 50.0, 50_000, 100_000, 0, 0);
        assert!((pct - 50.0).abs() < f64::EPSILON);
        assert_eq!(tot, 100_000);
        assert_eq!(dwn, 50_000);

        let (pct, tot, dwn) = combine_progress(false, 1, false, 100.0, 100_000, 100_000, 0, 0);
        assert!((pct - 100.0).abs() < f64::EPSILON);
        assert_eq!(tot, 100_000);
        assert_eq!(dwn, 100_000);

        // 2. Single audio (expects_separate_audio = false, streams_seen = 1)
        let (pct, tot, dwn) = combine_progress(false, 1, true, 75.0, 0, 0, 7_500, 10_000);
        assert!((pct - 75.0).abs() < f64::EPSILON);
        assert_eq!(tot, 10_000);
        assert_eq!(dwn, 7_500);

        // 3. Separate video (expects_separate_audio = true, streams_seen = 1)
        let (pct, tot, dwn) = combine_progress(true, 1, false, 50.0, 50_000, 100_000, 0, 0);
        assert!((pct - 42.5).abs() < f64::EPSILON);
        assert_eq!(tot, 100_000);
        assert_eq!(dwn, 50_000);

        let (pct, tot, dwn) = combine_progress(true, 1, false, 100.0, 100_000, 100_000, 0, 0);
        assert!((pct - 85.0).abs() < f64::EPSILON);
        assert_eq!(tot, 100_000);
        assert_eq!(dwn, 100_000);

        // 4. Separate audio (expects_separate_audio = true, streams_seen = 2)
        let (pct, tot, dwn) =
            combine_progress(true, 2, true, 50.0, 100_000, 100_000, 10_000, 20_000);
        assert!((pct - (110_000.0 / 120_000.0 * 100.0)).abs() < 1e-4);
        assert_eq!(tot, 120_000);
        assert_eq!(dwn, 110_000);

        let (pct, tot, dwn) = combine_progress(true, 2, true, 50.0, 100_000, 100_000, 5_000, 0);
        assert!((pct - 92.5).abs() < f64::EPSILON);
        assert_eq!(tot, 100_000);
        assert_eq!(dwn, 105_000);

        // 5. Dynamic fallback: expects_separate_audio = false, but streams_seen = 2 triggers audio weights
        let (pct, tot, dwn) =
            combine_progress(false, 2, true, 50.0, 100_000, 100_000, 10_000, 20_000);
        assert!((pct - (110_000.0 / 120_000.0 * 100.0)).abs() < 1e-4);
        assert_eq!(tot, 120_000);
        assert_eq!(dwn, 110_000);
    }

    #[test]
    fn test_extract_engine_error_sanitizes_escapes() {
        let stderr =
            "ERROR: \u{001b}[31;1mFatal\u{001b}[0m \u{001b}]0;pwned\u{0007}engine crash\r\n";
        let clean = extract_engine_error(stderr);
        assert_eq!(clean, "Fatal engine crash");
        assert!(!clean.contains('\u{001b}'));
        assert!(!clean.contains('\u{0007}'));
    }

    #[test]
    fn test_sanitize_media_title() {
        assert_eq!(
            sanitize_media_title("Sample / Title: With * Forbidden? Characters!"),
            "Sample _ Title - With  Forbidden Characters!"
        );
        assert_eq!(sanitize_media_title("Clean Title"), "Clean Title");
    }

    #[test]
    fn test_build_ytdlp_info_cmd_flags() {
        let yt = Path::new("yt-dlp");
        let cmd = build_ytdlp_info_cmd(yt, "https://youtube.com/watch?v=abc");
        let args: Vec<String> = cmd
            .get_args()
            .map(|a| a.to_string_lossy().to_string())
            .collect();
        assert!(args.contains(&"-J".to_string()));
        assert!(args.contains(&"--no-warnings".to_string()));
        assert!(args.contains(&"--no-playlist".to_string()));
        assert!(args.contains(&"--socket-timeout".to_string()));
        assert!(args.contains(&"30".to_string()));
        assert!(args.contains(&"--".to_string()));
        assert!(args.contains(&"https://youtube.com/watch?v=abc".to_string()));
    }

    #[test]
    fn test_find_format_with_fallback() {
        let formats = vec![
            MediaFormat {
                format_id: "720".to_string(),
                ext: "mp4".to_string(),
                resolution: "720p".to_string(),
                filesize: Some(25_000_000.0),
                is_estimated: false,
                is_hdr: false,
                vcodec: "avc1".to_string(),
                acodec: "mp4a".to_string(),
                height: Some(720),
            },
            MediaFormat {
                format_id: "480".to_string(),
                ext: "mp4".to_string(),
                resolution: "480p".to_string(),
                filesize: Some(15_000_000.0),
                is_estimated: false,
                is_hdr: false,
                vcodec: "avc1".to_string(),
                acodec: "mp4a".to_string(),
                height: Some(480),
            },
            MediaFormat {
                format_id: "audio_1".to_string(),
                ext: "m4a".to_string(),
                resolution: "Audio".to_string(),
                filesize: Some(5_000_000.0),
                is_estimated: false,
                is_hdr: false,
                vcodec: "none".to_string(),
                acodec: "mp4a".to_string(),
                height: None,
            },
        ];

        // Exact match
        assert_eq!(
            find_format_with_fallback(&formats, "720p"),
            Some(("720".to_string(), false))
        );

        // Fallback: 1080p requested -> nearest lower is 720p
        assert_eq!(
            find_format_with_fallback(&formats, "1080p"),
            Some(("720".to_string(), true))
        );

        // Fallback: 4k requested -> nearest lower is 720p
        assert_eq!(
            find_format_with_fallback(&formats, "4k"),
            Some(("720".to_string(), true))
        );
    }

    #[test]
    fn test_compat_mode_codec_preference() {
        let json_payload = r#"{
            "title": "Compat Mode Preference Test",
            "duration": 120.0,
            "formats": [
                {
                    "format_id": "1080_av1",
                    "ext": "webm",
                    "width": 1920,
                    "height": 1080,
                    "vcodec": "av01.0.08M.08",
                    "acodec": "none",
                    "filesize": 80000000.0,
                    "protocol": "https"
                },
                {
                    "format_id": "1080_avc",
                    "ext": "mp4",
                    "width": 1920,
                    "height": 1080,
                    "vcodec": "avc1.640028",
                    "acodec": "none",
                    "filesize": 70000000.0,
                    "protocol": "https"
                }
            ]
        }"#;

        // Default mode: prefers AV1
        let default_parsed = parse_video_info_json_ext(json_payload, false).unwrap();
        assert_eq!(default_parsed.formats[0].format_id, "1080_av1");

        // Compat mode: prefers AVC
        let compat_parsed = parse_video_info_json_ext(json_payload, true).unwrap();
        assert_eq!(compat_parsed.formats[0].format_id, "1080_avc");
    }

    #[test]
    fn test_find_existing_media_file_and_unique_copy() {
        let temp = tempfile::tempdir().unwrap();
        let title = "My Test Video";

        // Initially no existing file
        assert!(find_existing_media_file(temp.path(), title, false).is_none());

        // Create completed file
        let video_path = temp.path().join("My Test Video.mp4");
        std::fs::write(&video_path, b"test video bytes").unwrap();

        // Now should be detected
        let found = find_existing_media_file(temp.path(), title, false);
        assert!(found.is_some());
        assert_eq!(found.unwrap(), video_path);

        // Next copy template should be copy 1
        let (tmpl1, idx1) = resolve_unique_output_template(temp.path(), title, false);
        assert_eq!(idx1, 1);
        assert_eq!(tmpl1, "%(title)s (1).%(ext)s");

        // Create copy 1
        let copy1_path = temp.path().join("My Test Video (1).mp4");
        std::fs::write(&copy1_path, b"copy 1 bytes").unwrap();

        // Next copy template should be copy 2
        let (tmpl2, idx2) = resolve_unique_output_template(temp.path(), title, false);
        assert_eq!(idx2, 2);
        assert_eq!(tmpl2, "%(title)s (2).%(ext)s");
    }

    #[test]
    fn test_clean_interrupted_media_files() {
        let temp = tempfile::tempdir().unwrap();
        let title = "Interrupted Download";

        let media_path = temp.path().join("Interrupted Download.mp4");
        std::fs::write(&media_path, b"video content").unwrap();
        assert!(media_path.exists());

        let removed = clean_interrupted_media_files(temp.path().to_str().unwrap(), title, None);
        assert_eq!(removed, 1);
        assert!(!media_path.exists());
    }

    #[test]
    fn test_build_ytdlp_download_cmd_ext_flags() {
        let yt = Path::new("yt-dlp");
        let ff = Path::new("ffmpeg");
        let cmd = build_ytdlp_download_cmd_ext(
            yt,
            ff,
            "https://youtu.be/123",
            "1080",
            "/downloads/path",
            Some("%(title)s (1).%(ext)s"),
            true,
            Some("1"),
        );
        let args: Vec<String> = cmd
            .get_args()
            .map(|a| a.to_string_lossy().to_string())
            .collect();

        assert!(args.contains(&"-o".to_string()));
        assert!(args.contains(&"%(title)s (1).%(ext)s".to_string()));
        assert!(args.contains(&"-P".to_string()));
        assert!(args.contains(&"/downloads/path".to_string()));
        assert!(args.contains(&"temp:/downloads/path/.dwdr_1".to_string()));
        assert!(args.contains(&"--force-overwrites".to_string()));
        assert!(args.contains(&"--encoding".to_string()));
        assert!(args.contains(&"utf-8".to_string()));
        assert!(args.contains(&"--postprocessor-args".to_string()));
        assert!(args.contains(&"Merger:-y".to_string()));
        assert!(args.contains(&"--progress".to_string()));
        assert!(args.contains(&"--".to_string()));
    }

    #[test]
    fn test_task_temp_dir_cleanup_preserves_sibling_and_original_files() {
        let temp_dir = tempfile::tempdir().unwrap();
        let dir = temp_dir.path();

        let original_file = dir.join("Video.mp4");
        std::fs::write(&original_file, b"original content").unwrap();

        let sibling_file = dir.join("Video 2.mp4");
        std::fs::write(&sibling_file, b"sibling content").unwrap();

        let flac_file = dir.join("Video.flac");
        std::fs::write(&flac_file, b"flac audio").unwrap();

        let task_temp_dir = dir.join(".dwdr_1");
        std::fs::create_dir(&task_temp_dir).unwrap();
        let part_file = task_temp_dir.join("Video.f137.mp4.part");
        std::fs::write(&part_file, b"partial bytes").unwrap();

        assert!(task_temp_dir.exists());
        let _ = std::fs::remove_dir_all(&task_temp_dir);

        assert!(!task_temp_dir.exists());
        assert!(!part_file.exists());
        assert!(original_file.exists());
        assert_eq!(std::fs::read(&original_file).unwrap(), b"original content");
        assert!(sibling_file.exists());
        assert_eq!(std::fs::read(&sibling_file).unwrap(), b"sibling content");
        assert!(flac_file.exists());
        assert_eq!(std::fs::read(&flac_file).unwrap(), b"flac audio");
    }

    #[test]
    fn test_clean_interrupted_media_files_preserves_siblings_and_original() {
        let temp_dir = tempfile::tempdir().unwrap();
        let dir = temp_dir.path();

        let original_file = dir.join("Lecture 1.mp4");
        std::fs::write(&original_file, b"original lecture 1").unwrap();

        let sibling_10 = dir.join("Lecture 10.mp4");
        std::fs::write(&sibling_10, b"sibling lecture 10").unwrap();

        let sibling_copy = dir.join("Lecture 1 (1).mp4");
        std::fs::write(&sibling_copy, b"copy of lecture 1").unwrap();

        let failed_download = dir.join("Lecture 1 (2).mp4");
        std::fs::write(&failed_download, b"corrupted partial").unwrap();

        let failed_part = dir.join("Lecture 1 (2).mp4.part");
        std::fs::write(&failed_part, b"part bytes").unwrap();

        clean_interrupted_media_files(dir.to_str().unwrap(), "Lecture 1", Some("Lecture 1 (2)"));
        clean_partial_files(dir.to_str().unwrap(), Some("Lecture 1 (2)"));

        assert!(!failed_download.exists());
        assert!(!failed_part.exists());

        assert!(original_file.exists());
        assert_eq!(
            std::fs::read(&original_file).unwrap(),
            b"original lecture 1"
        );
        assert!(sibling_10.exists());
        assert_eq!(std::fs::read(&sibling_10).unwrap(), b"sibling lecture 10");
        assert!(sibling_copy.exists());
        assert_eq!(std::fs::read(&sibling_copy).unwrap(), b"copy of lecture 1");
    }

    #[test]
    fn test_read_lossy_lines_with_non_utf8_and_eof() {
        let mut data = Vec::new();
        data.extend_from_slice(b"download:PROGRESS|10.0%|1M|10M|1M/s|00:09|avc1\n");
        // Non-UTF-8 byte sequence (0xFF, 0xFE) inside a progress line
        data.extend_from_slice(b"download:PROGRESS|20.0%|2M|10M|1M/s|00:08|\xFF\xFE\n");
        data.extend_from_slice(b"[Merger] Merging formats into file\n");
        // Line without trailing newline at EOF
        data.extend_from_slice(b"postprocess:POSTPROCESS|finished");

        let cursor = Cursor::new(data);
        let mut lines = Vec::new();
        read_lossy_lines(cursor, |line| {
            lines.push(line.to_string());
        });

        assert_eq!(lines.len(), 4);
        assert_eq!(lines[0], "download:PROGRESS|10.0%|1M|10M|1M/s|00:09|avc1");
        assert!(lines[1].starts_with("download:PROGRESS|20.0%|2M|10M|1M/s|00:08|"));
        assert!(lines[1].contains('\u{FFFD}'));
        assert_eq!(lines[2], "[Merger] Merging formats into file");
        assert_eq!(lines[3], "postprocess:POSTPROCESS|finished");
    }

    #[test]
    fn test_read_lossy_lines_non_utf8_1000_lines_no_hang() {
        let mut data = Vec::new();
        for i in 0..1000 {
            if i % 3 == 0 {
                data.extend_from_slice(format!("download:PROGRESS|{}%|invalid:", i).as_bytes());
                data.extend_from_slice(&[0xFF, 0xFE]);
                data.extend_from_slice(b"|end\n");
            } else if i % 3 == 1 {
                data.extend_from_slice(b"\x80\x81\x82\x83\x84\n");
            } else {
                data.extend_from_slice(
                    format!("download:PROGRESS|{}%|1M|10M|1M/s|00:01\n", i).as_bytes(),
                );
            }
        }

        let cursor = Cursor::new(data);
        let mut count = 0;
        read_lossy_lines(cursor, |_| {
            count += 1;
        });

        assert_eq!(count, 1000);
    }

    #[test]
    fn test_cleanup_current_heuristic_limitations() {
        let temp_dir = tempfile::tempdir().unwrap();
        let dir = temp_dir.path();

        let sibling_part = dir.join("Video 2.part");
        std::fs::write(&sibling_part, b"sibling part bytes").unwrap();

        let flac_file = dir.join("Video.flac");
        std::fs::write(&flac_file, b"flac audio bytes").unwrap();

        let existing_video = dir.join("Video.mp4");
        std::fs::write(&existing_video, b"original video bytes").unwrap();

        clean_partial_files(dir.to_str().unwrap(), Some("Video"));
        assert!(!sibling_part.exists());
        assert!(!flac_file.exists());

        clean_interrupted_media_files(dir.to_str().unwrap(), "Video", None);
        assert!(!existing_video.exists());
    }

    #[test]
    fn test_media_format_summary() {
        let fmt_known = MediaFormat {
            format_id: "137".to_string(),
            ext: "mp4".to_string(),
            resolution: "1080p".to_string(),
            filesize: Some(273.55 * 1024.0 * 1024.0),
            is_estimated: false,
            is_hdr: false,
            vcodec: "avc1".to_string(),
            acodec: "none".to_string(),
            height: Some(1080),
        };
        assert_eq!(fmt_known.summary(), "1080p (mp4, 273.6 MB)");

        let fmt_est = MediaFormat {
            format_id: "137".to_string(),
            ext: "mp4".to_string(),
            resolution: "1080p".to_string(),
            filesize: Some(273.55 * 1024.0 * 1024.0),
            is_estimated: true,
            is_hdr: false,
            vcodec: "avc1".to_string(),
            acodec: "none".to_string(),
            height: Some(1080),
        };
        assert_eq!(fmt_est.summary(), "1080p (mp4, ~273.6 MB (Est.))");

        let fmt_no_size = MediaFormat {
            format_id: "22".to_string(),
            ext: "mp4".to_string(),
            resolution: "720p".to_string(),
            filesize: None,
            is_estimated: false,
            is_hdr: false,
            vcodec: "avc1".to_string(),
            acodec: "mp4a".to_string(),
            height: Some(720),
        };
        assert_eq!(fmt_no_size.summary(), "720p (mp4)");

        let fmt_audio = MediaFormat {
            format_id: "140".to_string(),
            ext: "m4a".to_string(),
            resolution: "Audio".to_string(),
            filesize: Some(15.2 * 1024.0 * 1024.0),
            is_estimated: false,
            is_hdr: false,
            vcodec: "none".to_string(),
            acodec: "mp4a".to_string(),
            height: None,
        };
        assert_eq!(fmt_audio.summary(), "Audio (m4a, 15.2 MB)");
    }

    #[test]
    fn test_apply_cookie_args_and_detection() {
        let mut cmd_brave = Command::new("yt-dlp");
        apply_cookie_args(&mut cmd_brave, Some("brave"));
        let args: Vec<String> = cmd_brave
            .get_args()
            .map(|a| a.to_string_lossy().to_string())
            .collect();
        assert_eq!(args, vec!["--cookies-from-browser", "brave"]);

        let mut cmd_chrome = Command::new("yt-dlp");
        apply_cookie_args(&mut cmd_chrome, Some("chrome"));
        let args_chrome: Vec<String> = cmd_chrome
            .get_args()
            .map(|a| a.to_string_lossy().to_string())
            .collect();
        assert_eq!(args_chrome, vec!["--cookies-from-browser", "chrome"]);

        let mut cmd_edge = Command::new("yt-dlp");
        apply_cookie_args(&mut cmd_edge, Some("edge"));
        let args_edge: Vec<String> = cmd_edge
            .get_args()
            .map(|a| a.to_string_lossy().to_string())
            .collect();
        assert_eq!(args_edge, vec!["--cookies-from-browser", "edge"]);

        let mut cmd_file = Command::new("yt-dlp");
        apply_cookie_args(&mut cmd_file, Some("/path/to/cookies.txt"));
        let args_file: Vec<String> = cmd_file
            .get_args()
            .map(|a| a.to_string_lossy().to_string())
            .collect();
        assert_eq!(args_file, vec!["--cookies", "/path/to/cookies.txt"]);

        let mut cmd_prefix_file = Command::new("yt-dlp");
        apply_cookie_args(&mut cmd_prefix_file, Some("file:my_cookies.txt"));
        let args_pfile: Vec<String> = cmd_prefix_file
            .get_args()
            .map(|a| a.to_string_lossy().to_string())
            .collect();
        assert_eq!(args_pfile, vec!["--cookies", "my_cookies.txt"]);

        let mut cmd_none = Command::new("yt-dlp");
        apply_cookie_args(&mut cmd_none, Some("none"));
        assert!(cmd_none.get_args().next().is_none());

        let mut cmd_off = Command::new("yt-dlp");
        apply_cookie_args(&mut cmd_off, Some("off"));
        assert!(cmd_off.get_args().next().is_none());

        let mut cmd_clear = Command::new("yt-dlp");
        apply_cookie_args(&mut cmd_clear, Some("clear"));
        assert!(cmd_clear.get_args().next().is_none());

        let mut cmd_disabled = Command::new("yt-dlp");
        apply_cookie_args(&mut cmd_disabled, Some("disabled"));
        assert!(cmd_disabled.get_args().next().is_none());

        let mut cmd_injected = Command::new("yt-dlp");
        apply_cookie_args(&mut cmd_injected, Some("--exec id"));
        assert!(cmd_injected.get_args().next().is_none());

        let mut cmd_uppercase_prefix = Command::new("yt-dlp");
        apply_cookie_args(&mut cmd_uppercase_prefix, Some("FILE:/path/to/cookies.txt"));
        let args_up: Vec<String> = cmd_uppercase_prefix
            .get_args()
            .map(|a| a.to_string_lossy().to_string())
            .collect();
        assert_eq!(args_up, vec!["--cookies", "/path/to/cookies.txt"]);

        let mut cmd_uppercase_browser = Command::new("yt-dlp");
        apply_cookie_args(&mut cmd_uppercase_browser, Some("BROWSER:edge"));
        let args_ub: Vec<String> = cmd_uppercase_browser
            .get_args()
            .map(|a| a.to_string_lossy().to_string())
            .collect();
        assert_eq!(args_ub, vec!["--cookies-from-browser", "edge"]);

        let mut cmd_browser_path = Command::new("yt-dlp");
        apply_cookie_args(&mut cmd_browser_path, Some("firefox:/custom/profile/path"));
        let args_bp: Vec<String> = cmd_browser_path
            .get_args()
            .map(|a| a.to_string_lossy().to_string())
            .collect();
        assert_eq!(
            args_bp,
            vec!["--cookies-from-browser", "firefox:/custom/profile/path"]
        );

        // UTF-8 multibyte safety check: must never panic on multibyte chars
        let mut cmd_cjk = Command::new("yt-dlp");
        apply_cookie_args(&mut cmd_cjk, Some("FILE:/home/日本語/cookies.txt"));
        let args_cjk: Vec<String> = cmd_cjk
            .get_args()
            .map(|a| a.to_string_lossy().to_string())
            .collect();
        assert_eq!(args_cjk, vec!["--cookies", "/home/日本語/cookies.txt"]);

        let mut cmd_short_cjk = Command::new("yt-dlp");
        apply_cookie_args(&mut cmd_short_cjk, Some("你好"));
        let args_short: Vec<String> = cmd_short_cjk
            .get_args()
            .map(|a| a.to_string_lossy().to_string())
            .collect();
        assert_eq!(args_short, vec!["--cookies-from-browser", "你好"]);

        let mut cmd_empty = Command::new("yt-dlp");
        apply_cookie_args(&mut cmd_empty, None);
        assert!(cmd_empty.get_args().next().is_none());
    }

    #[test]
    fn test_build_ytdlp_cmd_cookie_integration() {
        let yt = Path::new("yt-dlp");
        let ff = Path::new("ffmpeg");

        let info_cmd = build_ytdlp_info_cmd_ext(yt, "https://example.com/watch", Some("brave"));
        let info_args: Vec<String> = info_cmd
            .get_args()
            .map(|a| a.to_string_lossy().to_string())
            .collect();
        assert!(info_args.contains(&"--cookies-from-browser".to_string()));
        assert!(info_args.contains(&"brave".to_string()));

        let download_cmd = build_ytdlp_download_cmd_full(
            yt,
            ff,
            "https://example.com/watch",
            "best",
            "/tmp",
            None,
            false,
            None,
            Some("cookies.txt"),
        );
        let dl_args: Vec<String> = download_cmd
            .get_args()
            .map(|a| a.to_string_lossy().to_string())
            .collect();
        assert!(dl_args.contains(&"--cookies".to_string()));
        assert!(dl_args.contains(&"cookies.txt".to_string()));
    }

    #[test]
    fn test_is_auth_or_bot_error_across_platforms() {
        assert!(is_auth_or_bot_error("Sign in to confirm you’re not a bot"));
        assert!(is_auth_or_bot_error(
            "Use --cookies-from-browser or --cookies for authentication"
        ));
        assert!(is_auth_or_bot_error(
            "[instagram] 12345: Please log in or provide authentication cookies"
        ));
        assert!(is_auth_or_bot_error(
            "[instagram] 12345: This post is private"
        ));
        assert!(is_auth_or_bot_error(
            "[instagram] 12345: This account is private"
        ));
        assert!(is_auth_or_bot_error(
            "[instagram] 12345: This reel is private"
        ));
        assert!(is_auth_or_bot_error(
            "[twitter] 12345: This account is protected"
        ));
        assert!(is_auth_or_bot_error(
            "[twitter] 12345: Protected tweets are only visible to followers"
        ));
        assert!(is_auth_or_bot_error(
            "[youtube] 12345: Join this channel to get access to members-only content"
        ));
        assert!(is_auth_or_bot_error(
            "[youtube] 12345: Private video. Sign in if you've been granted access"
        ));
        assert!(is_auth_or_bot_error(
            "[youtube] 12345: Sign in to confirm your age."
        ));
        assert!(is_auth_or_bot_error(
            "[facebook] 12345: You must log in to continue"
        ));
        assert!(is_auth_or_bot_error(
            "[facebook] 12345: This video requires login to view"
        ));
        assert!(is_auth_or_bot_error("[tiktok] 12345: captcha required"));
        assert!(is_auth_or_bot_error("[tiktok] bot verification required"));
        assert!(is_auth_or_bot_error(
            "[twitch] 12345: This video is available to subscribers only"
        ));
        assert!(is_auth_or_bot_error("[YOUTUBE] THIS VIDEO IS PRIVATE"));
        assert!(!is_auth_or_bot_error("Video does not exist (HTTP 404)"));
        assert!(!is_auth_or_bot_error("Connection timed out"));
        assert!(!is_auth_or_bot_error("HTTP Error 429: Too Many Requests"));

        assert!(is_bot_error("Sign in to confirm you’re not a bot"));
        assert!(is_bot_error("[tiktok] bot verification required"));
        assert!(is_bot_error("[tiktok] returned error: captcha"));
        assert!(!is_bot_error("[instagram] 12345: This post is private"));

        assert!(is_rate_limit_error("HTTP Error 429: Too Many Requests"));
        assert!(is_rate_limit_error(
            "Server returned too many requests error"
        ));
        assert!(is_rate_limit_error("Platform rate limit reached"));
        assert!(!is_rate_limit_error("[youtube] a429xyz: Video unavailable"));
    }

    #[test]
    fn test_is_cookie_lock_error() {
        assert!(is_cookie_lock_error(
            "Could not copy Chrome cookie database. See https://github.com/yt-dlp/yt-dlp/issues/7271 for more info"
        ));
        assert!(is_cookie_lock_error("Could not copy Brave cookie database"));
        assert!(is_cookie_lock_error("Could not copy cookies database"));
        assert!(is_cookie_lock_error("LockProfileCookieDatabase failed"));
        assert!(is_cookie_lock_error(
            "https://github.com/yt-dlp/yt-dlp/issues/7271"
        ));
        assert!(!is_cookie_lock_error("Sign in to confirm you’re not a bot"));
        assert!(!is_cookie_lock_error("HTTP Error 404: Not Found"));
        assert!(is_auth_or_bot_error(
            "Could not copy Chrome cookie database. See https://github.com/yt-dlp/yt-dlp/issues/7271 for more info"
        ));
    }
}
