use colored::Colorize;
use crossterm::terminal;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::io::{BufRead, BufReader, Read, Write, stdout};
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::thread;

use crate::engine::queue::kill_process_by_pid;

#[derive(Debug, Serialize, Deserialize)]
pub struct VideoInfo {
    pub title: String,
    pub duration: Option<f64>,
    pub formats: Vec<MediaFormat>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaFormat {
    pub format_id: String,
    pub ext: String,
    pub resolution: String,
    pub filesize: Option<f64>,
    #[serde(default)]
    pub is_estimated: bool,
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
        if size_str == "Stream Size Pending" {
            if self.ext.is_empty() {
                self.resolution.clone()
            } else {
                format!("{} ({})", self.resolution, self.ext)
            }
        } else if self.ext.is_empty() {
            format!("{} [{}]", self.resolution, size_str)
        } else {
            format!("{} ({}, {})", self.resolution, self.ext, size_str)
        }
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

    // Square videos (1:1 aspect ratio, e.g. Instagram/VR SBS): height equals width.
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

    // Evaluate width and height candidates and take the higher resolution tier.
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

    if let Some(stripped) = line.strip_prefix("ERROR:") {
        stripped.trim().to_string()
    } else if let Some(stripped) = line.strip_prefix("error:") {
        stripped.trim().to_string()
    } else {
        line.to_string()
    }
}

fn score_vcodec(vcodec: &str) -> u8 {
    let lower = vcodec.to_lowercase();
    if lower.starts_with("av01") || lower.starts_with("av1") {
        4 // AV1: state-of-the-art compression & fidelity
    } else if lower.starts_with("vp09") || lower.starts_with("vp9") {
        3 // VP9
    } else if lower.starts_with("hev1")
        || lower.starts_with("hvc1")
        || lower.starts_with("hevc")
        || lower.starts_with("h265")
    {
        2 // HEVC / H.265
    } else if lower.starts_with("avc1") || lower.starts_with("avc") || lower.starts_with("h264") {
        1 // AVC / H.264
    } else {
        0 // Legacy / unknown
    }
}

fn score_acodec(acodec: &str) -> u8 {
    let lower = acodec.to_lowercase();
    if lower.contains("opus") {
        3 // Opus
    } else if lower.contains("mp4a") || lower.contains("aac") {
        2 // AAC
    } else if lower.contains("mp3") {
        1 // MP3
    } else {
        0
    }
}

pub fn parse_video_info_json(stdout_str: &str) -> Result<VideoInfo, String> {
    let raw: serde_json::Value =
        serde_json::from_str(stdout_str).map_err(|e| format!("Failed to parse JSON: {}", e))?;

    let title = raw["title"].as_str().unwrap_or("Unknown Title").to_string();
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

        let ext = f["ext"].as_str().unwrap_or("unknown");
        if ext == "mhtml" || ext == "weba" {
            continue;
        }

        if f["format_note"]
            .as_str()
            .unwrap_or("")
            .to_lowercase()
            .contains("storyboard")
        {
            continue;
        }

        let mut height = f["height"].as_u64();
        let mut width = f["width"].as_u64();

        // Parse resolution string when integer width/height fields are null or unpopulated.
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

        // Categorize into video vs audio:
        // 1. Explicit video codec -> Video
        // 2. Unprobed codec (not explicitly 'none'), with video dimensions or container -> Video
        // 3. Explicit audio codec with vcodec == 'none' -> Audio
        // 4. Not video and audio indicator present -> Audio
        let is_video = has_vcodec
            || (!is_vcodec_none
                && (height.is_some()
                    || width.is_some()
                    || f["resolution"]
                        .as_str()
                        .is_some_and(|r| !r.eq_ignore_ascii_case("audio only"))
                    || matches!(
                        ext,
                        "mp4" | "webm" | "mkv" | "avi" | "flv" | "mov" | "3gp" | "m3u8" | "ts"
                    )));
        let is_audio = (is_vcodec_none && has_acodec)
            || (!is_video
                && !is_acodec_none
                && (has_acodec || matches!(ext, "mp3" | "m4a" | "aac" | "wav" | "ogg" | "opus")));

        if !is_video && !is_audio {
            continue;
        }

        let format_id = f["format_id"].as_str().unwrap_or("unknown").to_string();
        let exact = f["filesize"].as_f64();
        let approx = f["filesize_approx"].as_f64();
        let tbr = f["tbr"].as_f64();
        let vbr = f["vbr"].as_f64();
        let abr = f["abr"].as_f64();
        let note = f["format_note"].as_str();
        let protocol = f["protocol"].as_str().unwrap_or("https");
        let is_direct = !protocol.contains("m3u8");
        let is_exact = exact.is_some();

        if is_video {
            let (resolution, rank) = match resolve_resolution(height, width, note) {
                Some(r) => (r.0.to_string(), r.1),
                None => {
                    // Fall back to note, height label, format ID, or "Default" if resolution cannot be mapped to standard dimensions.
                    let label = match note {
                        Some(n) if !n.trim().is_empty() => n.trim().to_string(),
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
                vcodec_opt.unwrap().to_string()
            } else {
                "unknown".to_string()
            };
            let acodec_str = if has_acodec {
                acodec_opt.unwrap().to_string()
            } else if is_acodec_none {
                "none".to_string()
            } else {
                "unknown".to_string()
            };

            let is_hdr = f["dynamic_range"]
                .as_str()
                .is_some_and(|d| d.eq_ignore_ascii_case("hdr"))
                || note.unwrap_or("").to_lowercase().contains("hdr");
            let codec_score = score_vcodec(&vcodec_str);

            video_candidates.push((
                rank,
                is_direct,
                is_hdr,
                codec_score,
                is_exact,
                size_ranking,
                MediaFormat {
                    format_id,
                    ext: ext.to_string(),
                    resolution,
                    filesize,
                    is_estimated,
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
                acodec_opt.unwrap().to_string()
            } else {
                "unknown".to_string()
            };
            let codec_score = score_acodec(&acodec_str);

            audio_candidates.push((
                is_direct,
                is_exact,
                codec_score,
                size_ranking,
                MediaFormat {
                    format_id,
                    ext: ext.to_string(),
                    resolution: "Audio".to_string(),
                    filesize,
                    is_estimated,
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

pub fn fetch_video_info(url: &str, yt_dlp_path: &Path) -> Result<VideoInfo, String> {
    let output = Command::new(yt_dlp_path)
        .arg("-J")
        .arg("--no-warnings")
        .arg("--no-playlist")
        .arg("--socket-timeout")
        .arg("30")
        .arg(url)
        .output()
        .map_err(|e| format!("Failed to execute yt-dlp: {}", e))?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(extract_engine_error(&err));
    }

    let stdout_str = String::from_utf8_lossy(&output.stdout);
    parse_video_info_json(&stdout_str)
}

pub fn parse_progress_line(line: &str) -> Option<(f64, String, String, String, String)> {
    parse_progress_line_ext(line).map(|(p, d, t, s, e, _)| (p, d, t, s, e))
}

pub fn parse_progress_line_ext(
    line: &str,
) -> Option<(f64, String, String, String, String, Option<String>)> {
    let rest = line.strip_prefix("PROGRESS|")?;
    let parts: Vec<&str> = rest.split('|').collect();
    if parts.len() < 5 {
        return None;
    }

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

    Some((
        percent,
        downloaded_clean,
        total_clean,
        speed_clean,
        eta_clean,
        vcodec,
    ))
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
            "\r[download] {} {:>5.1}% [{:>9} / {:>9}] at {:>10} ETA {:>5}",
            bar, percent, downloaded, total, speed, eta
        )
    } else if terminal_width >= 55 {
        format!(
            "\r[download] {:>5.1}% [{} / {}] at {} ETA {}",
            percent, downloaded, total, speed, eta
        )
    } else {
        format!("\r[download] {:>5.1}% {} ETA {}", percent, speed, eta)
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

pub const MEDIA_EXTENSIONS_AUDIO: &[&str] = &["mp3", "m4a", "aac", "opus", "wav", "flac", "ogg"];
pub const MEDIA_EXTENSIONS_VIDEO: &[&str] = &["mp4", "mkv", "webm", "avi", "mov"];

/// Sanitizes a media title to match filesystem-safe characters produced by yt-dlp.
pub fn sanitize_media_title(title: &str) -> String {
    title
        .chars()
        .filter(|c| !['/', '\\', ':', '*', '?', '"', '<', '>', '|'].contains(c))
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

        // Skip partial and incomplete temporary files
        if name_lower.ends_with(".part")
            || name_lower.ends_with(".ytdl")
            || name_lower.ends_with(".temp")
            || name_lower.ends_with(".tmp")
        {
            continue;
        }

        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        if stem.eq_ignore_ascii_case(&sanitized) {
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

        // Support removing leftover temporary directories matching title or temp indicators.
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
    crate::engine::queue::record_active_download();
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
    crate::engine::queue::clear_active_download();
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
    crate::engine::queue::clear_active_download();
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

    if !is_audio {
        cmd.arg("--merge-output-format").arg("mp4");
    }

    cmd.arg("--newline")
        .arg("--encoding")
        .arg("utf-8")
        .arg("--progress-template")
        .arg("download:PROGRESS|%(progress._percent_str)s|%(progress._downloaded_bytes_str,progress._total_bytes_str)s|%(progress._total_bytes_str,progress._total_bytes_estimate_str)s|%(progress._speed_str)s|%(progress._eta_str)s|%(info.vcodec)s")
        .arg("--progress-template")
        .arg("postprocess:POSTPROCESS|%(progress.status)s")
        .arg("--no-warnings")
        .arg("--no-playlist")
        .arg("--socket-timeout")
        .arg("30");

    if ffmpeg_path.exists() {
        cmd.arg("--ffmpeg-location").arg(ffmpeg_path);
    }

    cmd.arg(url);
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
) -> Result<(), String> {
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
}

/// Downloads media with custom output filename templates and overwrite options.
#[allow(clippy::too_many_arguments)]
pub fn download_media_ext(
    url: &str,
    format_id: &str,
    yt_dlp_path: &Path,
    ffmpeg_path: &Path,
    download_dir: &str,
    _title: Option<&str>,
    custom_output_template: Option<&str>,
    force_overwrites: bool,
) -> Result<(), String> {
    let validated_dir = crate::config::manager::validate_download_path(download_dir, true)
        .map_err(|e| format!("Invalid download directory: {}", e))?;
    let clean_download_dir = validated_dir.to_string_lossy();
    let temp_dir = validated_dir.join(".dwdr_direct");

    let mut cmd = build_ytdlp_download_cmd_ext(
        yt_dlp_path,
        ffmpeg_path,
        url,
        format_id,
        &clean_download_dir,
        custom_output_template,
        force_overwrites,
        Some("direct"),
    );

    #[cfg(unix)]
    {
        cmd.process_group(0);
    }

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Failed to execute yt-dlp: {}", e))?;

    let pid = child.id();
    register_direct_download_full(pid, temp_dir.clone());

    let mut stderr_handle = child.stderr.take();
    // Drain stderr asynchronously so unread process error buffers do not deadlock stdout pipe reading.
    let stderr_thread = thread::spawn(move || {
        let mut err_output = Vec::new();
        if let Some(err_pipe) = stderr_handle.as_mut() {
            let _ = err_pipe.read_to_end(&mut err_output);
        }
        String::from_utf8_lossy(&err_output).to_string()
    });

    let is_audio_only = format_id.starts_with("audio:")
        || format_id.eq_ignore_ascii_case("audio")
        || format_id.eq_ignore_ascii_case("bestaudio");

    let mut postprocess_started = false;
    let mut primary_download_finished = false;
    let mut max_observed_percent: f64 = 0.0;
    let bar_display = "=".repeat(25);

    if let Some(stdout_pipe) = child.stdout.take() {
        let reader = BufReader::new(stdout_pipe);

        read_lossy_lines(reader, |line| {
            if let Some((percent, downloaded, total, speed, eta, vcodec)) =
                parse_progress_line_ext(line)
            {
                let is_secondary_audio = !is_audio_only
                    && (vcodec
                        .as_deref()
                        .map(|v| v.eq_ignore_ascii_case("none"))
                        .unwrap_or(false)
                        || (max_observed_percent >= 90.0 && percent < 20.0));

                if is_secondary_audio || primary_download_finished {
                    if !postprocess_started {
                        postprocess_started = true;
                        print!(
                            "\r[download] [{}] 100.0% [Finalizing: Merging video and audio tracks...]       ",
                            bar_display
                        );
                        let _ = stdout().flush();
                    }
                    primary_download_finished = true;
                } else {
                    max_observed_percent = max_observed_percent.max(percent);
                    if percent >= 100.0 {
                        primary_download_finished = true;
                    }
                    print!(
                        "{}",
                        render_progress_bar(percent, &downloaded, &total, &speed, &eta)
                    );
                    let _ = stdout().flush();
                }
            } else if line.starts_with("[Merger]") || line.contains("Merging formats") {
                postprocess_started = true;
                print!(
                    "\r[download] [{}] 100.0% [Finalizing: Merging video and audio tracks...]       ",
                    bar_display
                );
                let _ = stdout().flush();
            } else if line.starts_with("[ExtractAudio]") {
                postprocess_started = true;
                print!(
                    "\r[download] [{}] 100.0% [Finalizing: Extracting audio stream...]            ",
                    bar_display
                );
                let _ = stdout().flush();
            } else if line.starts_with("[Fixup") {
                postprocess_started = true;
                print!(
                    "\r[download] [{}] 100.0% [Finalizing: Correcting media container...]         ",
                    bar_display
                );
                let _ = stdout().flush();
            } else if line.starts_with("POSTPROCESS|") && !postprocess_started {
                postprocess_started = true;
                print!(
                    "\r[download] [{}] 100.0% [Finalizing: Packaging final media...]              ",
                    bar_display
                );
                let _ = stdout().flush();
            }
        });
    }

    let status = child
        .wait()
        .map_err(|e| format!("Failed to wait for yt-dlp: {}", e))?;

    let stderr_output = stderr_thread.join().unwrap_or_default();
    unregister_direct_download();

    let _ = std::fs::remove_dir_all(&temp_dir);

    if status.success() {
        println!(
            "\r[download] [{}] 100.0% Complete! Finalized successfully!                       ",
            bar_display
        );
        let _ = stdout().flush();
        println!("{}", "SUCCESS: Media finalization complete!".green());
        Ok(())
    } else {
        println!();
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
) -> Result<(), String>
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
        on_progress,
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
) -> Result<(), String>
where
    F: FnOnce(&mut Child),
    G: Fn(f64, &str, &str, &str, &str, bool),
{
    let validated_dir = crate::config::manager::validate_download_path(download_dir, true)
        .map_err(|e| format!("Invalid download directory: {}", e))?;
    let clean_download_dir = validated_dir.to_string_lossy();

    let mut cmd = build_ytdlp_download_cmd_ext(
        yt_dlp_path,
        ffmpeg_path,
        url,
        format_id,
        &clean_download_dir,
        custom_output_template,
        force_overwrites,
        task_id,
    );

    #[cfg(unix)]
    {
        cmd.process_group(0);
    }

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Failed to execute yt-dlp: {}", e))?;

    on_spawn(&mut child);

    let mut stderr_handle = child.stderr.take();
    // Drain stderr asynchronously so unread process error buffers do not deadlock stdout pipe reading.
    let stderr_thread = thread::spawn(move || {
        let mut err_output = Vec::new();
        if let Some(err_pipe) = stderr_handle.as_mut() {
            let _ = err_pipe.read_to_end(&mut err_output);
        }
        String::from_utf8_lossy(&err_output).to_string()
    });

    let is_audio_only = format_id.starts_with("audio:")
        || format_id.eq_ignore_ascii_case("audio")
        || format_id.eq_ignore_ascii_case("bestaudio");

    let mut postprocess_started = false;
    let mut primary_download_finished = false;
    let mut max_observed_percent: f64 = 0.0;

    if let Some(stdout_pipe) = child.stdout.take() {
        let reader = BufReader::new(stdout_pipe);

        read_lossy_lines(reader, |line| {
            if let Some((percent, downloaded, total, speed, eta, vcodec)) =
                parse_progress_line_ext(line)
            {
                let is_secondary_audio = !is_audio_only
                    && (vcodec
                        .as_deref()
                        .map(|v| v.eq_ignore_ascii_case("none"))
                        .unwrap_or(false)
                        || (max_observed_percent >= 90.0 && percent < 20.0));

                if is_secondary_audio || primary_download_finished {
                    if !postprocess_started {
                        postprocess_started = true;
                        on_progress(100.0, "", "", "", "", true);
                    }
                    primary_download_finished = true;
                } else {
                    max_observed_percent = max_observed_percent.max(percent);
                    if percent >= 100.0 {
                        primary_download_finished = true;
                    }
                    on_progress(percent, &downloaded, &total, &speed, &eta, false);
                }
            } else if (line.starts_with("POSTPROCESS|")
                || line.starts_with("[Merger]")
                || line.starts_with("[ExtractAudio]")
                || line.contains("Merging formats"))
                && !postprocess_started
            {
                postprocess_started = true;
                on_progress(100.0, "", "", "", "", true);
            }
        });
    }

    let status = child
        .wait()
        .map_err(|e| format!("Failed to wait for yt-dlp: {}", e))?;

    let stderr_output = stderr_thread.join().unwrap_or_default();

    if status.success() {
        Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn test_find_format_by_quality() {
        let formats = vec![
            MediaFormat {
                format_id: "571".to_string(),
                ext: "webm".to_string(),
                resolution: "4320p".to_string(),
                filesize: Some(200_000_000.0),
                is_estimated: false,
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

        // Standard terminal (80 cols) should fit within 80 characters without visual wrapping
        let std_term = render_progress_bar_with_width(50.0, "50MB", "100MB", "5MB/s", "00:10", 80);
        assert!(std_term.contains("50.0%"));
        let display_width = std_term.trim_start_matches('\r').chars().count();
        assert!(display_width <= 80);

        // Compact mode for 60 cols
        let compact = render_progress_bar_with_width(50.0, "50MB", "100MB", "5MB/s", "00:10", 60);
        let display_width_compact = compact.trim_start_matches('\r').chars().count();
        assert!(display_width_compact <= 60);

        // Ultra-compact mode for 40 cols
        let ultra = render_progress_bar_with_width(50.0, "50MB", "100MB", "5MB/s", "00:10", 40);
        let display_width_ultra = ultra.trim_start_matches('\r').chars().count();
        assert!(display_width_ultra <= 40);
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

        // Update mock executable to simulate successful JSON extraction
        let mock_json = r#"{"title": "Offline Mock Video", "duration": 60, "formats": [{"format_id": "18", "ext": "mp4", "width": 640, "height": 360, "vcodec": "avc1", "acodec": "mp4a", "filesize": 1000000}]}"#;
        std::fs::write(
            &mock_engine,
            format!("#!/bin/sh\ncat << 'EOF'\n{}\nEOF\nexit 0\n", mock_json),
        )
        .unwrap();

        let result_ok = fetch_video_info("https://youtube.com/watch?v=mock", &mock_engine);
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
    }

    #[test]
    fn test_sanitize_media_title() {
        assert_eq!(
            sanitize_media_title("Sample / Title: With * Forbidden? Characters!"),
            "Sample  Title With  Forbidden Characters!"
        );
        assert_eq!(sanitize_media_title("Clean Title"), "Clean Title");
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
            vcodec: "none".to_string(),
            acodec: "mp4a".to_string(),
            height: None,
        };
        assert_eq!(fmt_audio.summary(), "Audio (m4a, 15.2 MB)");
    }
}
