use serde::Deserialize;
use std::path::Path;
use std::process::Command;

#[derive(Debug, Deserialize)]
pub struct VideoInfo {
    pub title: String,
    pub duration: Option<f64>,
    pub formats: Vec<MediaFormat>,
}

#[derive(Debug, Deserialize)]
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
