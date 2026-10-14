use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;
use std::process::{Command, Stdio};

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
            Some(bytes) => format_bytes(bytes),
            None => "Unknown Size".to_string(),
        }
    }

    pub fn size_in_mb(&self) -> String {
        self.format_filesize()
    }
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
        return Err(err.to_string());
    }

    let stdout_str = String::from_utf8_lossy(&output.stdout);

    let raw: serde_json::Value =
        serde_json::from_str(&stdout_str).map_err(|e| format!("Failed to parse JSON: {}", e))?;

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
        let filesize = f["filesize"]
            .as_f64()
            .or_else(|| f["filesize_approx"].as_f64());

        if vcodec != "none" {
            let size = match filesize {
                Some(s) => s,
                None => continue,
            };

            if let (Some(w), Some(h)) = (f["width"].as_u64(), f["height"].as_u64())
                && w == h
                && w > 0
            {
                continue;
            }

            let height = f["height"].as_u64();
            let width = f["width"].as_u64();
            let note = f["format_note"].as_str();

            let (resolution, rank) = match resolve_resolution(height, width, note) {
                Some(r) => r,
                None => continue,
            };

            video_candidates.push((
                rank,
                size,
                MediaFormat {
                    format_id,
                    ext: ext.to_string(),
                    resolution: resolution.to_string(),
                    filesize: Some(size),
                    vcodec: vcodec.to_string(),
                    acodec: acodec.to_string(),
                    height: height.map(|h| h as u32),
                },
            ));
        } else if acodec != "none" {
            let size = match filesize {
                Some(s) => s,
                None => continue,
            };

            audio_candidates.push((
                size,
                MediaFormat {
                    format_id,
                    ext: ext.to_string(),
                    resolution: "Audio".to_string(),
                    filesize: Some(size),
                    vcodec: "none".to_string(),
                    acodec: acodec.to_string(),
                    height: None,
                },
            ));
        }
    }

    video_candidates.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then_with(|| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal))
    });

    let mut formats = Vec::new();
    let mut seen_resolutions = HashSet::new();

    for (_, _, format) in video_candidates {
        if seen_resolutions.insert(format.resolution.clone()) {
            formats.push(format);
        }
    }

    audio_candidates.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

    if let Some((_, best_audio)) = audio_candidates.into_iter().next() {
        formats.push(best_audio);
    }

    Ok(VideoInfo {
        title,
        duration,
        formats,
    })
}

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
        .arg("-N")
        .arg("8")
        .arg("-o")
        .arg(&output_template)
        .arg("--merge-output-format")
        .arg("mp4")
        .arg("--no-warnings")
        .arg("--no-playlist");

    if ffmpeg_path.exists() {
        cmd.arg("--ffmpeg-location").arg(ffmpeg_path);
    }

    cmd.arg(url);
    cmd.stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    let status = cmd
        .status()
        .map_err(|e| format!("Failed to execute yt-dlp: {}", e))?;

    if status.success() {
        Ok(())
    } else {
        Err("yt-dlp process exited with an error".to_string())
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
                vcodec: "vp9".to_string(),
                acodec: "none".to_string(),
                height: Some(2160),
            },
            MediaFormat {
                format_id: "271".to_string(),
                ext: "webm".to_string(),
                resolution: "2K".to_string(),
                filesize: Some(50_000_000.0),
                vcodec: "vp9".to_string(),
                acodec: "none".to_string(),
                height: Some(1440),
            },
            MediaFormat {
                format_id: "22".to_string(),
                ext: "mp4".to_string(),
                resolution: "720p".to_string(),
                filesize: Some(25_000_000.0),
                vcodec: "avc1".to_string(),
                acodec: "mp4a".to_string(),
                height: Some(720),
            },
            MediaFormat {
                format_id: "18".to_string(),
                ext: "mp4".to_string(),
                resolution: "360p".to_string(),
                filesize: Some(10_000_000.0),
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
            vcodec: "none".to_string(),
            acodec: "opus".to_string(),
            height: None,
        };
        assert_eq!(format_without_size.format_filesize(), "Unknown Size");
        assert_eq!(format_without_size.size_in_mb(), "Unknown Size");
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
}
