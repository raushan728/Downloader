use indicatif::{ProgressBar, ProgressStyle};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

#[derive(Debug, Serialize, Deserialize)]
pub struct VideoInfo {
    pub title: String,
    pub duration: Option<f64>,
    pub formats: Vec<MediaFormat>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MediaFormat {
    pub format_id: String,
    pub ext: String,
    pub resolution: String,
    pub filesize: Option<f64>,
    pub vcodec: String,
    pub acodec: String,
}

impl MediaFormat {
    pub fn size_in_mb(&self) -> String {
        match self.filesize {
            Some(bytes) => format!("{:.1} MB", bytes / 1_048_576.0),
            None => "Unknown Size".to_string(),
        }
    }
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

    let mut formats = Vec::new();

    for f in raw_formats {
        let vcodec = f["vcodec"].as_str().unwrap_or("none").to_string();
        let acodec = f["acodec"].as_str().unwrap_or("none").to_string();

        if vcodec == "none" && acodec == "none" {
            continue; // Skip broken formats
        }

        let format_id = f["format_id"].as_str().unwrap_or("unknown").to_string();
        let ext = f["ext"].as_str().unwrap_or("unknown").to_string();
        let filesize = f["filesize"]
            .as_f64()
            .or_else(|| f["filesize_approx"].as_f64());

        let resolution = if let Some(note) = f["format_note"].as_str() {
            note.to_string()
        } else if let Some(res) = f["resolution"].as_str() {
            res.to_string()
        } else if vcodec == "none" {
            "Audio".to_string()
        } else {
            "Unknown".to_string()
        };

        formats.push(MediaFormat {
            format_id,
            ext,
            resolution,
            filesize,
            vcodec,
            acodec,
        });
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
    let mut cmd = Command::new(yt_dlp_path);
    cmd.arg("-f")
        .arg(format!("{}+bestaudio/best", format_id))
        .arg("-N")
        .arg("8")
        .arg("-P")
        .arg(download_dir)
        .arg("--newline")
        .arg("--no-warnings")
        .arg("--no-playlist");

    if ffmpeg_path.exists() {
        cmd.arg("--ffmpeg-location").arg(ffmpeg_path);
    }

    cmd.arg(url);
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} {msg:.cyan}")
            .unwrap_or_else(|_| ProgressStyle::default_spinner()),
    );
    pb.enable_steady_tick(Duration::from_millis(100));
    pb.set_message("Starting download...");

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Failed to execute yt-dlp: {}", e))?;

    if let Some(stdout) = child.stdout.take() {
        let reader = BufReader::new(stdout);
        for line in reader.lines().map_while(Result::ok) {
            if line.contains("[download]") && line.contains('%') {
                let clean = line.replace("[download]", "").trim().to_string();
                pb.set_message(clean);
            }
        }
    }

    let status = child
        .wait()
        .map_err(|e| format!("Failed to wait for yt-dlp: {}", e))?;

    if status.success() {
        pb.finish_with_message("SUCCESS: Download complete!");
        Ok(())
    } else {
        pb.finish_with_message("ERROR: Download failed.");
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
                resolution: "2160p".to_string(),
                filesize: Some(100_000_000.0),
                vcodec: "vp9".to_string(),
                acodec: "none".to_string(),
            },
            MediaFormat {
                format_id: "271".to_string(),
                ext: "webm".to_string(),
                resolution: "1440p".to_string(),
                filesize: Some(50_000_000.0),
                vcodec: "vp9".to_string(),
                acodec: "none".to_string(),
            },
            MediaFormat {
                format_id: "22".to_string(),
                ext: "mp4".to_string(),
                resolution: "720p".to_string(),
                filesize: Some(25_000_000.0),
                vcodec: "avc1".to_string(),
                acodec: "mp4a".to_string(),
            },
            MediaFormat {
                format_id: "18".to_string(),
                ext: "mp4".to_string(),
                resolution: "360p".to_string(),
                filesize: Some(10_000_000.0),
                vcodec: "avc1".to_string(),
                acodec: "mp4a".to_string(),
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
    fn test_media_format_size_in_mb() {
        let format_with_size = MediaFormat {
            format_id: "1".to_string(),
            ext: "mp4".to_string(),
            resolution: "720p".to_string(),
            filesize: Some(10_485_760.0),
            vcodec: "avc1".to_string(),
            acodec: "mp4a".to_string(),
        };
        assert_eq!(format_with_size.size_in_mb(), "10.0 MB");

        let format_without_size = MediaFormat {
            format_id: "2".to_string(),
            ext: "mp4".to_string(),
            resolution: "Audio".to_string(),
            filesize: None,
            vcodec: "none".to_string(),
            acodec: "opus".to_string(),
        };
        assert_eq!(format_without_size.size_in_mb(), "Unknown Size");
    }
}
