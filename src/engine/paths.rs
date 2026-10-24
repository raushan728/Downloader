use directories::BaseDirs;
use std::path::PathBuf;

/// Resolves the filesystem path to the isolated `yt-dlp` executable.
///
/// Adheres to Core Invariant 1 (Zero System Pollution): internal dependencies are stored
/// exclusively in `~/.dwdr/deps/` (or `%USERPROFILE%\.dwdr\deps\` on Windows) without modifying
/// system directories. Supports `DWDR_YTDLP_PATH` for offline integration tests.
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

/// Resolves the filesystem path to the isolated `ffmpeg` executable.
///
/// Adheres to Core Invariant 1 (Zero System Pollution): internal dependencies are stored
/// exclusively in `~/.dwdr/deps/` (or `%USERPROFILE%\.dwdr\deps\` on Windows) without modifying
/// system directories. Supports `DWDR_FFMPEG_PATH` for offline integration tests.
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
