use directories::BaseDirs;
use std::path::PathBuf;

pub fn get_ytdlp_path() -> PathBuf {
    if let Ok(override_path) = std::env::var("DWDR_YTDLP_PATH") {
        let trimmed = override_path.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }

    let mut path = BaseDirs::new()
        .map(|dirs| dirs.home_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    path.push(".dwdr");
    path.push("deps");

    if cfg!(target_os = "windows") {
        path.push("yt-dlp.exe");
    } else {
        path.push("yt-dlp");
    }

    path
}

pub fn get_ffmpeg_path() -> PathBuf {
    if let Ok(override_path) = std::env::var("DWDR_FFMPEG_PATH") {
        let trimmed = override_path.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }

    let mut path = BaseDirs::new()
        .map(|dirs| dirs.home_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    path.push(".dwdr");
    path.push("deps");

    if cfg!(target_os = "windows") {
        path.push("ffmpeg.exe");
    } else {
        path.push("ffmpeg");
    }

    path
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_ytdlp_path() {
        let path = get_ytdlp_path();
        let expected_name = if cfg!(target_os = "windows") {
            "yt-dlp.exe"
        } else {
            "yt-dlp"
        };
        assert!(path.ends_with(expected_name));
    }

    #[test]
    fn test_get_ffmpeg_path() {
        let path = get_ffmpeg_path();
        let expected_name = if cfg!(target_os = "windows") {
            "ffmpeg.exe"
        } else {
            "ffmpeg"
        };
        assert!(path.ends_with(expected_name));
    }

    #[test]
    #[serial_test::serial]
    fn test_path_overrides_via_env() {
        unsafe {
            std::env::set_var("DWDR_YTDLP_PATH", "/custom/bin/yt-dlp");
            std::env::set_var("DWDR_FFMPEG_PATH", "/custom/bin/ffmpeg");
        }

        assert_eq!(get_ytdlp_path(), PathBuf::from("/custom/bin/yt-dlp"));
        assert_eq!(get_ffmpeg_path(), PathBuf::from("/custom/bin/ffmpeg"));

        unsafe {
            std::env::remove_var("DWDR_YTDLP_PATH");
            std::env::remove_var("DWDR_FFMPEG_PATH");
        }
    }
}
