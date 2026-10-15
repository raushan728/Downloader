use colored::Colorize;
use crossterm::terminal;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::io::{BufRead, BufReader, Read, Write, stdout};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::thread;

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

    // Letterboxed widescreen streams (e.g. 1920x800) have reduced height, so check width first.
    if let Some(w) = width {
        if w >= 7680 {
            return Some(("8K", 4320));
        } else if w >= 3840 {
            return Some(("4K", 2160));
        } else if w >= 2560 {
            return Some(("2K", 1440));
        } else if w >= 1920 {
            return Some(("1080p", 1080));
        } else if w >= 1280 {
            return Some(("720p", 720));
        } else if w >= 854 {
            return Some(("480p", 480));
        } else if w >= 640 {
            return Some(("360p", 360));
        } else if w >= 426 {
            return Some(("240p", 240));
        } else if w >= 256 {
            return Some(("144p", 144));
        }
    }

    if let Some(h) = height
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

    None
}

pub fn extract_engine_error(stderr: &str) -> String {
    let trimmed = stderr.trim();
    if trimmed.is_empty() {
        return "yt-dlp process exited with an error".to_string();
    }

    if let Some(err_line) = trimmed
        .lines()
        .rev()
        .find(|l| l.contains("ERROR:") || l.contains("error:"))
    {
        return err_line.trim().to_string();
    }

    trimmed
        .lines()
        .next_back()
        .map(|l| l.trim().to_string())
        .unwrap_or_else(|| trimmed.to_string())
}

pub fn parse_video_info_json(stdout_str: &str) -> Result<VideoInfo, String> {
    let raw: serde_json::Value =
        serde_json::from_str(stdout_str).map_err(|e| format!("Failed to parse JSON: {}", e))?;

    let title = raw["title"].as_str().unwrap_or("Unknown Title").to_string();
    let duration = raw["duration"].as_f64();

    let raw_formats = raw["formats"]
        .as_array()
        .ok_or("No formats found in JSON")?;

    let mut video_candidates = Vec::new();
    let mut audio_candidates = Vec::new();

    for f in raw_formats {
        let vcodec = f["vcodec"].as_str().unwrap_or("none");
        let acodec = f["acodec"].as_str().unwrap_or("none");

        if vcodec == "none" && acodec == "none" {
            continue;
        }

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

        let format_id = f["format_id"].as_str().unwrap_or("unknown").to_string();
        let exact = f["filesize"].as_f64();
        let approx = f["filesize_approx"].as_f64();
        let tbr = f["tbr"].as_f64();
        let vbr = f["vbr"].as_f64();
        let abr = f["abr"].as_f64();
        let height = f["height"].as_u64();
        let width = f["width"].as_u64();
        let note = f["format_note"].as_str();
        let protocol = f["protocol"].as_str().unwrap_or("https");
        let is_direct = !protocol.contains("m3u8");
        let is_exact = exact.is_some();

        if vcodec != "none" {
            if let (Some(w), Some(h)) = (width, height)
                && w == h
                && w > 0
            {
                continue;
            }

            let (resolution, rank) = match resolve_resolution(height, width, note) {
                Some(r) => r,
                None => continue,
            };

            let total_bitrate = tbr.or_else(|| {
                let v = vbr.unwrap_or(0.0);
                let a = abr.unwrap_or(0.0);
                if v + a > 0.0 { Some(v + a) } else { None }
            });
            let (filesize, is_estimated) =
                calculate_media_filesize(exact, approx, total_bitrate, duration, height, true);
            let size_ranking = filesize.unwrap_or(0.0);

            video_candidates.push((
                rank,
                is_direct,
                is_exact,
                size_ranking,
                MediaFormat {
                    format_id,
                    ext: ext.to_string(),
                    resolution: resolution.to_string(),
                    filesize,
                    is_estimated,
                    vcodec: vcodec.to_string(),
                    acodec: acodec.to_string(),
                    height: height.map(|h| h as u32),
                },
            ));
        } else if acodec != "none" {
            let total_bitrate = abr.or(tbr);
            let (filesize, is_estimated) =
                calculate_media_filesize(exact, approx, total_bitrate, duration, height, false);
            let size_ranking = filesize.unwrap_or(0.0);

            audio_candidates.push((
                is_direct,
                is_exact,
                size_ranking,
                MediaFormat {
                    format_id,
                    ext: ext.to_string(),
                    resolution: "Audio".to_string(),
                    filesize,
                    is_estimated,
                    vcodec: "none".to_string(),
                    acodec: acodec.to_string(),
                    height: None,
                },
            ));
        }
    }

    video_candidates.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then_with(|| b.1.cmp(&a.1))
            .then_with(|| b.2.cmp(&a.2))
            .then_with(|| b.3.partial_cmp(&a.3).unwrap_or(std::cmp::Ordering::Equal))
    });

    let mut formats = Vec::new();
    let mut seen_resolutions = HashSet::new();

    for (_, _, _, _, format) in video_candidates {
        if seen_resolutions.insert(format.resolution.clone()) {
            formats.push(format);
        }
    }

    audio_candidates.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then_with(|| b.1.cmp(&a.1))
            .then_with(|| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal))
    });

    if let Some((_, _, _, best_audio)) = audio_candidates.into_iter().next() {
        formats.push(best_audio);
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

/// Purges incomplete download files (.part, .ytdl, unmerged chunks) while preserving completed .mp4 media.
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
    let sanitized_title = title_hint.map(|t| {
        t.chars()
            .filter(|c| !['/', '\\', ':', '*', '?', '"', '<', '>', '|'].contains(c))
            .collect::<String>()
    });

    for entry in read_dir.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let file_name = match path.file_name().and_then(|n| n.to_str()) {
            Some(name) => name,
            None => continue,
        };

        let file_name_lower = file_name.to_lowercase();
        let is_part_or_temp = file_name_lower.ends_with(".part")
            || file_name_lower.ends_with(".ytdl")
            || file_name_lower.ends_with(".temp");

        let matches_title = if let Some(ref title) = sanitized_title {
            if title.is_empty() {
                true
            } else {
                let first_word = title.split_whitespace().next().unwrap_or(title.as_str());
                file_name.contains(title.as_str()) || file_name.contains(first_word)
            }
        } else {
            true
        };

        let is_unmerged_chunk = if let Some(ref title) = sanitized_title {
            if title.is_empty() {
                file_name.contains(".f") && !file_name_lower.ends_with(".mp4")
            } else {
                let first_word = title.split_whitespace().next().unwrap_or(title.as_str());
                (file_name.contains(title.as_str()) || file_name.contains(first_word))
                    && file_name.contains(".f")
                    && !file_name_lower.ends_with(".mp4")
            }
        } else {
            false
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
    download_dir: String,
    title: String,
}

static DIRECT_TRACKER: Mutex<Option<DirectDownloadTracker>> = Mutex::new(None);

pub fn register_direct_download(pid: u32, download_dir: &str, title: &str) {
    if let Ok(mut g) = DIRECT_TRACKER.lock() {
        *g = Some(DirectDownloadTracker {
            pid: Some(pid),
            download_dir: download_dir.to_string(),
            title: title.to_string(),
        });
    }
}

pub fn unregister_direct_download() {
    if let Ok(mut g) = DIRECT_TRACKER.lock() {
        *g = None;
    }
}

pub fn cancel_direct_download() {
    if let Ok(mut g) = DIRECT_TRACKER.lock()
        && let Some(tracker) = g.take()
    {
        if let Some(pid) = tracker.pid {
            crate::engine::queue::kill_process_by_pid(pid);
        }
        clean_partial_files(&tracker.download_dir, Some(&tracker.title));
    }
}

/// Downloads media in the foreground, streaming live progress and managing process lifecycle.
pub fn download_media(
    url: &str,
    format_id: &str,
    yt_dlp_path: &Path,
    ffmpeg_path: &Path,
    download_dir: &str,
) -> Result<(), String> {
    let clean_dir = download_dir.trim_end_matches(['/', '\\']);
    let output_template = format!("{}/%(title)s.%(ext)s", clean_dir);

    let mut cmd = Command::new(yt_dlp_path);
    cmd.arg("-f")
        .arg(format!("{}+bestaudio/best", format_id))
        .arg("-o")
        .arg(&output_template)
        .arg("--merge-output-format")
        .arg("mp4")
        .arg("--newline")
        .arg("--progress-template")
        .arg("download:PROGRESS|%(progress._percent_str)s|%(progress._downloaded_bytes_str,progress._total_bytes_str)s|%(progress._total_bytes_str,progress._total_bytes_estimate_str)s|%(progress._speed_str)s|%(progress._eta_str)s")
        .arg("--progress-template")
        .arg("postprocess:POSTPROCESS|%(progress.status)s")
        .arg("--no-warnings")
        .arg("--no-playlist");

    if ffmpeg_path.exists() {
        cmd.arg("--ffmpeg-location").arg(ffmpeg_path);
    }

    cmd.arg(url);
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Failed to execute yt-dlp: {}", e))?;

    let pid = child.id();
    register_direct_download(pid, download_dir, "");

    let mut stderr_handle = child.stderr.take();
    // Drain stderr asynchronously so unread process error buffers do not deadlock stdout pipe reading.
    let stderr_thread = thread::spawn(move || {
        let mut err_output = Vec::new();
        if let Some(err_pipe) = stderr_handle.as_mut() {
            let _ = err_pipe.read_to_end(&mut err_output);
        }
        String::from_utf8_lossy(&err_output).to_string()
    });

    let mut postprocess_started = false;

    if let Some(stdout_pipe) = child.stdout.take() {
        let reader = BufReader::new(stdout_pipe);

        for line in reader.lines().map_while(Result::ok) {
            let line = line.trim();
            if let Some((percent, downloaded, total, speed, eta)) = parse_progress_line(line) {
                print!(
                    "{}",
                    render_progress_bar(percent, &downloaded, &total, &speed, &eta)
                );
                let _ = stdout().flush();
            } else if (line.starts_with("POSTPROCESS|")
                || line.starts_with("[Merger]")
                || line.starts_with("[ExtractAudio]")
                || line.contains("Merging formats"))
                && !postprocess_started
            {
                postprocess_started = true;
                println!(
                    "\r[download] [{}] 100.0% Complete!                                                  ",
                    "=".repeat(25)
                );
                println!("{}", "INFO: Extracting and finalizing media...".cyan());
                let _ = stdout().flush();
            }
        }
    }

    let status = child
        .wait()
        .map_err(|e| format!("Failed to wait for yt-dlp: {}", e))?;

    let stderr_output = stderr_thread.join().unwrap_or_default();
    unregister_direct_download();

    if status.success() {
        if !postprocess_started {
            println!(
                "\r[download] [{}] 100.0% Complete!                                                  ",
                "=".repeat(25)
            );
        }
        println!("{}", "SUCCESS: Finalizing complete!".green());
        Ok(())
    } else {
        clean_partial_files(download_dir, None);
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
    let clean_dir = download_dir.trim_end_matches(['/', '\\']);
    let output_template = format!("{}/%(title)s.%(ext)s", clean_dir);

    let mut cmd = Command::new(yt_dlp_path);
    cmd.arg("-f")
        .arg(format!("{}+bestaudio/best", format_id))
        .arg("-o")
        .arg(&output_template)
        .arg("--merge-output-format")
        .arg("mp4")
        .arg("--newline")
        .arg("--progress-template")
        .arg("download:PROGRESS|%(progress._percent_str)s|%(progress._downloaded_bytes_str,progress._total_bytes_str)s|%(progress._total_bytes_str,progress._total_bytes_estimate_str)s|%(progress._speed_str)s|%(progress._eta_str)s")
        .arg("--progress-template")
        .arg("postprocess:POSTPROCESS|%(progress.status)s")
        .arg("--no-warnings")
        .arg("--no-playlist");

    if ffmpeg_path.exists() {
        cmd.arg("--ffmpeg-location").arg(ffmpeg_path);
    }

    cmd.arg(url);
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

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

    let mut postprocess_started = false;

    if let Some(stdout_pipe) = child.stdout.take() {
        let reader = BufReader::new(stdout_pipe);

        for line in reader.lines().map_while(Result::ok) {
            let line = line.trim();
            if let Some((percent, downloaded, total, speed, eta)) = parse_progress_line(line) {
                on_progress(percent, &downloaded, &total, &speed, &eta, false);
            } else if (line.starts_with("POSTPROCESS|")
                || line.starts_with("[Merger]")
                || line.starts_with("[ExtractAudio]")
                || line.contains("Merging formats"))
                && !postprocess_started
            {
                postprocess_started = true;
                on_progress(100.0, "", "", "", "", true);
            }
        }
    }

    let status = child
        .wait()
        .map_err(|e| format!("Failed to wait for yt-dlp: {}", e))?;

    let stderr_output = stderr_thread.join().unwrap_or_default();

    if status.success() {
        Ok(())
    } else {
        clean_partial_files(download_dir, None);
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
        let matched = if target == "4k" {
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

    #[test]
    fn test_find_format_by_quality() {
        let formats = vec![
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
            Some("313".to_string())
        );
        assert_eq!(find_format_by_quality(&formats, "always ask"), None);
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
            "ERROR: unable to download video data: HTTP Error 403: Forbidden"
        );

        // HTTP 429 Too Many Requests
        let stderr_429 = "ERROR: HTTP Error 429: Too Many Requests\n";
        assert_eq!(
            extract_engine_error(stderr_429),
            "ERROR: HTTP Error 429: Too Many Requests"
        );

        // Video unavailable
        let stderr_unavail = "ERROR: [youtube] rNpaYXAO2K4: Video unavailable\n";
        assert_eq!(
            extract_engine_error(stderr_unavail),
            "ERROR: [youtube] rNpaYXAO2K4: Video unavailable"
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
    }

    #[test]
    fn test_parse_video_info_json_valid_and_invalid() {
        // Invalid JSON
        let invalid = parse_video_info_json("not json {{{{");
        assert!(invalid.is_err());
        assert!(invalid.unwrap_err().contains("Failed to parse JSON"));

        // Missing formats
        let missing_formats = parse_video_info_json(r#"{"title": "Test Video"}"#);
        assert!(missing_formats.is_err());
        assert_eq!(missing_formats.unwrap_err(), "No formats found in JSON");

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
        use std::os::unix::fs::PermissionsExt;

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
            "ERROR: unable to download video data: HTTP Error 403: Forbidden"
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
}
