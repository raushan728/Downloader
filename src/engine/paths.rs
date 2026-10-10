use directories::BaseDirs;
use std::path::PathBuf;

pub fn get_ytdlp_path() -> PathBuf {
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
}
