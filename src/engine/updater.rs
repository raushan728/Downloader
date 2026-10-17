use colored::Colorize;
use std::fs;
use std::io::{Read, Seek, Write};
use std::path::Path;

use crate::engine::paths::{get_ffmpeg_path, get_ytdlp_path};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

/// Validates binary file integrity via minimum filesize, executable magic headers, and test execution.
pub fn is_valid_executable(path: &Path, is_ffmpeg: bool) -> bool {
    if !path.exists() || !path.is_file() {
        return false;
    }
    let metadata = match fs::metadata(path) {
        Ok(m) => m,
        Err(_) => return false,
    };
    if metadata.len() < 100_000 {
        return false;
    }

    let mut file = match fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return false,
    };
    let mut header = [0u8; 4];
    if file.read_exact(&mut header).is_err() {
        return false;
    }

    #[cfg(target_os = "windows")]
    {
        if header[0] != b'M' || header[1] != b'Z' {
            return false;
        }
    }

    #[cfg(target_os = "linux")]
    {
        if is_ffmpeg {
            if &header != b"\x7fELF" {
                return false;
            }
        } else {
            let is_valid = (&header == b"\x7fELF")
                || (&header[0..2] == b"#!")
                || (header == [0x50, 0x4b, 0x03, 0x04]);
            if !is_valid {
                return false;
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        let is_macho = (header == [0xfe, 0xed, 0xfa, 0xce])
            || (header == [0xfe, 0xed, 0xfa, 0xcf])
            || (header == [0xce, 0xfa, 0xed, 0xfe])
            || (header == [0xcf, 0xfa, 0xed, 0xfe])
            || (header == [0xca, 0xfe, 0xba, 0xbe]);
        if is_ffmpeg {
            if !is_macho {
                return false;
            }
        } else {
            let is_valid =
                is_macho || (&header[0..2] == b"#!") || (header == [0x50, 0x4b, 0x03, 0x04]);
            if !is_valid {
                return false;
            }
        }
    }

    let flag = if is_ffmpeg { "-version" } else { "--version" };
    match std::process::Command::new(path).arg(flag).output() {
        Ok(output) => output.status.success(),
        Err(_) => false,
    }
}

pub fn ensure_dependencies() -> Result<(), String> {
    let yt_dlp_path = get_ytdlp_path();

    if yt_dlp_path.exists() && !is_valid_executable(&yt_dlp_path, false) {
        println!(
            "{}",
            "WARNING: Detected corrupted or invalid yt-dlp binary. Re-downloading...".yellow()
        );
        let _ = fs::remove_file(&yt_dlp_path);
    }

    if !yt_dlp_path.exists() {
        println!(
            "{}",
            "INFO: Core engine missing. Downloading latest yt-dlp... This will only happen once."
                .yellow()
        );

        let url = if cfg!(target_os = "windows") {
            "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe"
        } else {
            "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp"
        };

        let client = reqwest::blocking::Client::builder()
            .user_agent("dwdr-cli/1.0")
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

        let response = client
            .get(url)
            .send()
            .map_err(|e| format!("Failed to download yt-dlp: {}", e))?;

        if !response.status().is_success() {
            return Err(format!(
                "Download failed with status: {}",
                response.status()
            ));
        }

        let bytes = response
            .bytes()
            .map_err(|e| format!("Failed to read response bytes: {}", e))?;

        if let Some(parent) = yt_dlp_path.parent()
            && !parent.exists()
        {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create deps directory: {}", e))?;
        }

        let tmp_path = if cfg!(target_os = "windows") {
            yt_dlp_path.with_extension("tmp.exe")
        } else {
            yt_dlp_path.with_extension("tmp")
        };
        {
            let mut file = fs::File::create(&tmp_path)
                .map_err(|e| format!("Failed to create temporary yt-dlp file: {}", e))?;
            file.write_all(&bytes)
                .map_err(|e| format!("Failed to write temporary yt-dlp file: {}", e))?;
        }

        #[cfg(unix)]
        {
            let mut perms = fs::metadata(&tmp_path)
                .map_err(|e| format!("Failed to read metadata: {}", e))?
                .permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&tmp_path, perms)
                .map_err(|e| format!("Failed to set permissions: {}", e))?;
        }

        if !is_valid_executable(&tmp_path, false) {
            let _ = fs::remove_file(&tmp_path);
            return Err("Downloaded yt-dlp binary failed integrity validation".to_string());
        }

        fs::rename(&tmp_path, &yt_dlp_path)
            .map_err(|e| format!("Failed to finalize yt-dlp binary: {}", e))?;

        println!("{}", "SUCCESS: Engine downloaded successfully!".green());
    }

    ensure_ffmpeg()?;

    Ok(())
}

fn get_ffmpeg_download_url() -> &'static str {
    if cfg!(target_os = "windows") {
        "https://github.com/ffbinaries/ffbinaries-prebuilt/releases/download/v4.4.1/ffmpeg-4.4.1-win-64.zip"
    } else if cfg!(target_os = "macos") {
        "https://github.com/ffbinaries/ffbinaries-prebuilt/releases/download/v4.4.1/ffmpeg-4.4.1-osx-64.zip"
    } else {
        "https://github.com/ffbinaries/ffbinaries-prebuilt/releases/download/v4.4.1/ffmpeg-4.4.1-linux-64.zip"
    }
}

pub fn ensure_ffmpeg() -> Result<(), String> {
    let ffmpeg_path = get_ffmpeg_path();
    if ffmpeg_path.exists() {
        if is_valid_executable(&ffmpeg_path, true) {
            return Ok(());
        }
        println!(
            "{}",
            "WARNING: Detected corrupted or invalid FFmpeg binary. Re-downloading...".yellow()
        );
        let _ = fs::remove_file(&ffmpeg_path);
    }

    println!("{}", "INFO: Media Merger (FFmpeg) is missing. Downloading prebuilt binary... This will only happen once.".yellow());

    let url = get_ffmpeg_download_url();

    let client = reqwest::blocking::Client::builder()
        .user_agent("dwdr-cli/1.0")
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {}", e))?;

    let mut response = client
        .get(url)
        .send()
        .map_err(|e| format!("Failed to download FFmpeg: {}", e))?;

    if !response.status().is_success() {
        return Err(format!(
            "FFmpeg download failed with HTTP status: {}",
            response.status()
        ));
    }

    let mut tmp_file =
        tempfile::tempfile().map_err(|e| format!("Failed to create temp file: {}", e))?;

    response
        .copy_to(&mut tmp_file)
        .map_err(|e| format!("Failed to write to temp file: {}", e))?;

    tmp_file
        .seek(std::io::SeekFrom::Start(0))
        .map_err(|e| format!("Failed to seek temp file: {}", e))?;

    let mut archive =
        zip::ZipArchive::new(tmp_file).map_err(|e| format!("Failed to read zip archive: {}", e))?;

    let expected_filename = if cfg!(target_os = "windows") {
        "ffmpeg.exe"
    } else {
        "ffmpeg"
    };

    let tmp_ffmpeg = if cfg!(target_os = "windows") {
        ffmpeg_path.with_extension("tmp.exe")
    } else {
        ffmpeg_path.with_extension("tmp")
    };
    let mut found = false;
    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .map_err(|e| format!("Failed to read file from archive: {}", e))?;
        if file.name() == expected_filename {
            if let Some(parent) = ffmpeg_path.parent()
                && !parent.exists()
            {
                fs::create_dir_all(parent)
                    .map_err(|e| format!("Failed to create deps directory: {}", e))?;
            }

            let mut out_file = std::fs::File::create(&tmp_ffmpeg)
                .map_err(|e| format!("Failed to create temporary FFmpeg file: {}", e))?;
            std::io::copy(&mut file, &mut out_file)
                .map_err(|e| format!("Failed to write temporary FFmpeg file: {}", e))?;
            found = true;
            break;
        }
    }

    if !found {
        let _ = fs::remove_file(&tmp_ffmpeg);
        return Err("FFmpeg executable not found in the downloaded archive".to_string());
    }

    #[cfg(unix)]
    {
        let mut perms = fs::metadata(&tmp_ffmpeg)
            .map_err(|e| format!("Failed to read metadata: {}", e))?
            .permissions();
        perms.set_mode(0o755);
        let _ = fs::set_permissions(&tmp_ffmpeg, perms);
    }

    if !is_valid_executable(&tmp_ffmpeg, true) {
        let _ = fs::remove_file(&tmp_ffmpeg);
        return Err("Extracted FFmpeg binary failed integrity validation".to_string());
    }

    fs::rename(&tmp_ffmpeg, &ffmpeg_path)
        .map_err(|e| format!("Failed to finalize FFmpeg binary: {}", e))?;

    println!(
        "{}",
        "SUCCESS: Media Merger downloaded and extracted successfully!".green()
    );
    Ok(())
}

pub fn update_self() -> Result<(), String> {
    let current_exe = std::env::current_exe()
        .map_err(|e| format!("Failed to get current executable path: {}", e))?;

    let url = if cfg!(target_os = "windows") {
        "https://github.com/raushan728/Downloader/releases/download/latest/dwdr-windows.exe"
    } else if cfg!(target_os = "macos") {
        "https://github.com/raushan728/Downloader/releases/download/latest/dwdr-mac"
    } else {
        "https://github.com/raushan728/Downloader/releases/download/latest/dwdr-linux"
    };

    println!("{}", "INFO: Downloading latest DWDR release...".cyan());

    let client = reqwest::blocking::Client::builder()
        .user_agent("dwdr-updater")
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    let response = client
        .get(url)
        .send()
        .map_err(|e| format!("Failed to download update: {}", e))?;

    if !response.status().is_success() {
        return Err(format!(
            "Download failed with status: {}",
            response.status()
        ));
    }

    let bytes = response
        .bytes()
        .map_err(|e| format!("Failed to read update bytes: {}", e))?;

    if bytes.len() < 100_000 {
        return Err("Downloaded update binary is too small or invalid".to_string());
    }

    #[cfg(target_os = "windows")]
    {
        if bytes.len() < 2 || bytes[0] != b'M' || bytes[1] != b'Z' {
            return Err(
                "Downloaded binary does not have a valid Windows executable header".to_string(),
            );
        }
    }

    #[cfg(target_os = "linux")]
    {
        if bytes.len() < 4 || &bytes[0..4] != b"\x7fELF" {
            return Err(
                "Downloaded binary does not have a valid ELF executable header".to_string(),
            );
        }
    }

    #[cfg(target_os = "macos")]
    {
        if bytes.len() < 4 {
            return Err("Downloaded binary is too small".to_string());
        }
        let header = &bytes[0..4];
        let is_macho = (header == [0xfe, 0xed, 0xfa, 0xce])
            || (header == [0xfe, 0xed, 0xfa, 0xcf])
            || (header == [0xce, 0xfa, 0xed, 0xfe])
            || (header == [0xcf, 0xfa, 0xed, 0xfe])
            || (header == [0xca, 0xfe, 0xba, 0xbe]);
        if !is_macho {
            return Err(
                "Downloaded binary does not have a valid Mach-O executable header".to_string(),
            );
        }
    }

    let tmp_exe = if cfg!(target_os = "windows") {
        current_exe.with_extension("tmp.exe")
    } else {
        current_exe.with_extension("tmp")
    };

    let mut file = fs::File::create(&tmp_exe)
        .map_err(|e| format!("Failed to create temporary executable: {}", e))?;

    file.write_all(&bytes)
        .map_err(|e| format!("Failed to write temporary executable: {}", e))?;

    #[cfg(not(target_os = "windows"))]
    {
        #[cfg(unix)]
        {
            let mut perms = fs::metadata(&tmp_exe)
                .map_err(|e| format!("Failed to read metadata: {}", e))?
                .permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&tmp_exe, perms)
                .map_err(|e| format!("Failed to set permissions: {}", e))?;
        }

        std::fs::rename(&tmp_exe, &current_exe)
            .map_err(|e| format!("Failed to replace binary: {}", e))?;
    }

    #[cfg(target_os = "windows")]
    {
        let old_exe = current_exe.with_extension("old");
        if old_exe.exists() {
            let _ = std::fs::remove_file(&old_exe);
        }
        std::fs::rename(&current_exe, &old_exe)
            .map_err(|e| format!("Failed to backup current executable: {}", e))?;

        if let Err(e) = std::fs::rename(&tmp_exe, &current_exe) {
            let _ = std::fs::rename(&old_exe, &current_exe);
            let _ = std::fs::remove_file(&tmp_exe);
            return Err(format!("Failed to replace binary (rolled back): {}", e));
        }
    }

    println!(
        "{}",
        "SUCCESS: DWDR updated to the latest version! Restart the tool to apply changes.".green()
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_valid_executable_rejects_missing_and_corrupt() {
        let temp_dir = tempfile::tempdir().unwrap();
        let missing = temp_dir.path().join("missing.exe");
        assert!(!is_valid_executable(&missing, false));

        let empty = temp_dir.path().join("empty.exe");
        std::fs::write(&empty, b"").unwrap();
        assert!(!is_valid_executable(&empty, false));

        let small = temp_dir.path().join("small.exe");
        std::fs::write(&small, b"not enough bytes").unwrap();
        assert!(!is_valid_executable(&small, false));
    }
}
