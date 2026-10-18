use colored::Colorize;
use std::fs;
use std::io::{Read, Seek, Write};
use std::path::Path;
use std::time::Duration;

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

/// Resolves the yt-dlp release URL based on target operating system and architecture.
pub fn resolve_ytdlp_url(target_os: &str, target_arch: &str) -> &'static str {
    match (target_os, target_arch) {
        ("windows", "aarch64") => {
            "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp_arm64.exe"
        }
        ("windows", _) => "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe",
        ("linux", "aarch64") => {
            "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp_linux_aarch64"
        }
        ("linux", _) => "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp",
        ("macos", _) => "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp_macos",
        _ => "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp",
    }
}

pub fn get_ytdlp_download_url() -> &'static str {
    resolve_ytdlp_url(std::env::consts::OS, std::env::consts::ARCH)
}

/// Resolves the FFmpeg release URL based on target operating system and architecture.
pub fn resolve_ffmpeg_url(target_os: &str, target_arch: &str) -> &'static str {
    match (target_os, target_arch) {
        ("windows", _) => {
            "https://github.com/ffbinaries/ffbinaries-prebuilt/releases/download/v4.4.1/ffmpeg-4.4.1-win-64.zip"
        }
        ("linux", "aarch64") => {
            "https://github.com/ffbinaries/ffbinaries-prebuilt/releases/download/v4.4.1/ffmpeg-4.4.1-linux-arm-64.zip"
        }
        ("linux", _) => {
            "https://github.com/ffbinaries/ffbinaries-prebuilt/releases/download/v4.4.1/ffmpeg-4.4.1-linux-64.zip"
        }
        ("macos", "aarch64") => {
            "https://github.com/eugeneware/ffmpeg-static/releases/download/b6.1.1/ffmpeg-darwin-arm64"
        }
        ("macos", _) => {
            "https://github.com/ffbinaries/ffbinaries-prebuilt/releases/download/v4.4.1/ffmpeg-4.4.1-osx-64.zip"
        }
        _ => {
            "https://github.com/ffbinaries/ffbinaries-prebuilt/releases/download/v4.4.1/ffmpeg-4.4.1-linux-64.zip"
        }
    }
}

pub fn get_ffmpeg_download_url() -> &'static str {
    resolve_ffmpeg_url(std::env::consts::OS, std::env::consts::ARCH)
}

/// Resolves the DWDR binary release URL based on target operating system and architecture.
pub fn resolve_dwdr_url(target_os: &str, target_arch: &str) -> &'static str {
    match (target_os, target_arch) {
        ("windows", _) => {
            "https://github.com/raushan728/Downloader/releases/download/latest/dwdr-windows.exe"
        }
        ("macos", "aarch64") => {
            "https://github.com/raushan728/Downloader/releases/download/latest/dwdr-mac-arm64"
        }
        ("macos", _) => {
            "https://github.com/raushan728/Downloader/releases/download/latest/dwdr-mac"
        }
        ("linux", "aarch64") => {
            "https://github.com/raushan728/Downloader/releases/download/latest/dwdr-linux-arm64"
        }
        ("linux", _) => {
            "https://github.com/raushan728/Downloader/releases/download/latest/dwdr-linux"
        }
        _ => "https://github.com/raushan728/Downloader/releases/download/latest/dwdr-linux",
    }
}

pub fn get_dwdr_download_url() -> &'static str {
    resolve_dwdr_url(std::env::consts::OS, std::env::consts::ARCH)
}

/// Inspects environment variables for proxy configuration in order of precedence.
pub fn resolve_proxy_url() -> Option<String> {
    for var in &[
        "DWDR_PROXY",
        "HTTPS_PROXY",
        "https_proxy",
        "ALL_PROXY",
        "all_proxy",
        "HTTP_PROXY",
        "http_proxy",
    ] {
        if let Ok(val) = std::env::var(var) {
            let trimmed = val.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

/// Builds an HTTP client with configured proxy, user agent, and timeouts.
pub fn create_http_client(user_agent: &str) -> Result<reqwest::blocking::Client, String> {
    let mut builder = reqwest::blocking::Client::builder()
        .user_agent(user_agent)
        .connect_timeout(Duration::from_secs(30));

    if let Some(proxy_url) = resolve_proxy_url() {
        let proxy = reqwest::Proxy::all(&proxy_url)
            .map_err(|e| format!("Invalid proxy URL '{}': {}", proxy_url, e))?;
        builder = builder.proxy(proxy);
    }

    builder
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))
}

/// Determines whether an HTTP status code indicates a temporary, retryable condition.
pub fn is_retryable_status(status: reqwest::StatusCode) -> bool {
    matches!(status.as_u16(), 408 | 429 | 500 | 502 | 503 | 504)
}

/// Represents an in-memory or streamed HTTP response with integrity verification.
#[derive(Debug)]
pub struct DownloadedResponse {
    status: reqwest::StatusCode,
    bytes: Vec<u8>,
}

impl DownloadedResponse {
    pub fn new(status: reqwest::StatusCode, bytes: Vec<u8>) -> Self {
        Self { status, bytes }
    }

    pub fn status(&self) -> reqwest::StatusCode {
        self.status
    }

    pub fn bytes(self) -> Result<Vec<u8>, String> {
        Ok(self.bytes)
    }

    pub fn copy_to<W: Write>(&self, w: &mut W) -> Result<u64, std::io::Error> {
        w.write_all(&self.bytes)?;
        Ok(self.bytes.len() as u64)
    }
}

/// Executes an HTTP GET request with retries and exponential backoff for transient failures,
/// covering both initial connection and complete body streaming.
pub fn download_with_retry(
    client: &reqwest::blocking::Client,
    url: &str,
    max_retries: u32,
    initial_delay: Duration,
) -> Result<DownloadedResponse, String> {
    let mut last_error = String::new();
    let mut delay = initial_delay;

    for attempt in 1..=max_retries {
        match client.get(url).send() {
            Ok(response) => {
                let status = response.status();
                if status.is_success() {
                    match response.bytes() {
                        Ok(body) => return Ok(DownloadedResponse::new(status, body.to_vec())),
                        Err(e) => {
                            last_error = format!("Body stream interrupted: {}", e);
                            if attempt < max_retries {
                                std::thread::sleep(delay);
                                delay = delay.saturating_mul(2);
                                continue;
                            }
                        }
                    }
                } else if is_retryable_status(status) && attempt < max_retries {
                    last_error = format!("HTTP {}", status);
                    std::thread::sleep(delay);
                    delay = delay.saturating_mul(2);
                    continue;
                } else {
                    return Err(format!("Download failed with status: {}", status));
                }
            }
            Err(e) => {
                last_error = format!("{}", e);
                if attempt < max_retries {
                    std::thread::sleep(delay);
                    delay = delay.saturating_mul(2);
                    continue;
                }
            }
        }
    }

    Err(format!(
        "Download failed after {} attempts: {}",
        max_retries, last_error
    ))
}

pub fn ensure_dependencies() -> Result<(), String> {
    let yt_dlp_path = get_ytdlp_path();

    if std::env::var("DWDR_YTDLP_PATH").is_ok() && yt_dlp_path.exists() {
        return ensure_ffmpeg();
    }

    if yt_dlp_path.exists() {
        if is_valid_executable(&yt_dlp_path, false) {
            return ensure_ffmpeg();
        }
        println!(
            "{}",
            "WARNING: Detected corrupted or invalid yt-dlp binary. Re-downloading...".yellow()
        );
        fs::remove_file(&yt_dlp_path).map_err(|e| {
            format!(
                "Failed to remove corrupted yt-dlp binary at '{}': {}",
                yt_dlp_path.display(),
                e
            )
        })?;
    }

    if !yt_dlp_path.exists() {
        println!(
            "{}",
            "INFO: Core engine missing. Downloading latest yt-dlp... This will only happen once."
                .yellow()
        );

        let url = get_ytdlp_download_url();
        let client = create_http_client("dwdr-cli/1.0")?;
        let response = download_with_retry(&client, url, 3, Duration::from_millis(1000))?;

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

pub fn ensure_ffmpeg() -> Result<(), String> {
    let ffmpeg_path = get_ffmpeg_path();

    if std::env::var("DWDR_FFMPEG_PATH").is_ok() && ffmpeg_path.exists() {
        return Ok(());
    }

    if ffmpeg_path.exists() {
        if is_valid_executable(&ffmpeg_path, true) {
            return Ok(());
        }
        println!(
            "{}",
            "WARNING: Detected corrupted or invalid FFmpeg binary. Re-downloading...".yellow()
        );
        fs::remove_file(&ffmpeg_path).map_err(|e| {
            format!(
                "Failed to remove corrupted FFmpeg binary at '{}': {}",
                ffmpeg_path.display(),
                e
            )
        })?;
    }

    println!("{}", "INFO: Media Merger (FFmpeg) is missing. Downloading prebuilt binary... This will only happen once.".yellow());

    let url = get_ffmpeg_download_url();
    let client = create_http_client("dwdr-cli/1.0")?;
    let response = download_with_retry(&client, url, 3, Duration::from_millis(1000))?;

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

    if let Some(parent) = ffmpeg_path.parent()
        && !parent.exists()
    {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create deps directory: {}", e))?;
    }

    if url.ends_with(".zip") {
        let mut tmp_file =
            tempfile::tempfile().map_err(|e| format!("Failed to create temp file: {}", e))?;

        response
            .copy_to(&mut tmp_file)
            .map_err(|e| format!("Failed to write to temp file: {}", e))?;

        tmp_file
            .seek(std::io::SeekFrom::Start(0))
            .map_err(|e| format!("Failed to seek temp file: {}", e))?;

        let mut archive = zip::ZipArchive::new(tmp_file)
            .map_err(|e| format!("Failed to read zip archive: {}", e))?;

        let mut found = false;
        for i in 0..archive.len() {
            let mut file = archive
                .by_index(i)
                .map_err(|e| format!("Failed to read file from archive: {}", e))?;
            if file.name() == expected_filename {
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
    } else {
        let bytes = response
            .bytes()
            .map_err(|e| format!("Failed to read response bytes: {}", e))?;
        let mut out_file = std::fs::File::create(&tmp_ffmpeg)
            .map_err(|e| format!("Failed to create temporary FFmpeg file: {}", e))?;
        out_file
            .write_all(&bytes)
            .map_err(|e| format!("Failed to write temporary FFmpeg file: {}", e))?;
    }

    #[cfg(unix)]
    {
        let mut perms = fs::metadata(&tmp_ffmpeg)
            .map_err(|e| format!("Failed to read metadata: {}", e))?
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&tmp_ffmpeg, perms)
            .map_err(|e| format!("Failed to set permissions on FFmpeg: {}", e))?;
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

    if let Some(parent) = current_exe.parent() {
        let probe_file = parent.join(format!(".dwdr_update_perm_test_{}.tmp", std::process::id()));
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&probe_file)
        {
            Ok(_) => {
                let _ = std::fs::remove_file(&probe_file);
            }
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                return Err(format!(
                    "Permission denied writing to install directory '{}'. Please re-run with elevated permissions (e.g. 'sudo dwdr self-update') or install to a user-writable directory.",
                    parent.display()
                ));
            }
            Err(e) => {
                return Err(format!(
                    "Install directory '{}' is not writable: {}",
                    parent.display(),
                    e
                ));
            }
        }
    }

    let url = get_dwdr_download_url();

    println!("{}", "INFO: Downloading latest DWDR release...".cyan());

    let client = create_http_client("dwdr-updater")?;
    let response = download_with_retry(&client, url, 3, Duration::from_millis(1000))?;

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

        if !is_valid_executable(&tmp_exe, false) {
            let _ = fs::remove_file(&tmp_exe);
            return Err("Downloaded update binary failed executable validation check".to_string());
        }

        std::fs::rename(&tmp_exe, &current_exe)
            .map_err(|e| format!("Failed to replace binary: {}", e))?;
    }

    #[cfg(target_os = "windows")]
    {
        if !is_valid_executable(&tmp_exe, false) {
            let _ = fs::remove_file(&tmp_exe);
            return Err("Downloaded update binary failed executable validation check".to_string());
        }

        let old_exe = current_exe.with_extension("old");
        if old_exe.exists()
            && let Err(e) = std::fs::remove_file(&old_exe)
        {
            return Err(format!(
                "Cannot perform update: previous backup '{}' is locked or in use ({}). Please close running DWDR instances and retry.",
                old_exe.display(),
                e
            ));
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
    use serial_test::serial;
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;

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

    #[test]
    fn test_resolve_ytdlp_url_platforms() {
        assert!(resolve_ytdlp_url("windows", "aarch64").ends_with("yt-dlp_arm64.exe"));
        assert!(resolve_ytdlp_url("windows", "x86_64").ends_with("yt-dlp.exe"));
        assert!(resolve_ytdlp_url("linux", "aarch64").ends_with("yt-dlp_linux_aarch64"));
        assert!(resolve_ytdlp_url("linux", "x86_64").ends_with("yt-dlp"));
        assert!(resolve_ytdlp_url("macos", "aarch64").ends_with("yt-dlp_macos"));
        assert!(resolve_ytdlp_url("macos", "x86_64").ends_with("yt-dlp_macos"));
        assert!(resolve_ytdlp_url("unknown", "unknown").ends_with("yt-dlp"));
    }

    #[test]
    fn test_resolve_ffmpeg_url_platforms() {
        assert!(resolve_ffmpeg_url("windows", "x86_64").ends_with("ffmpeg-4.4.1-win-64.zip"));
        assert!(resolve_ffmpeg_url("linux", "aarch64").ends_with("ffmpeg-4.4.1-linux-arm-64.zip"));
        assert!(resolve_ffmpeg_url("linux", "x86_64").ends_with("ffmpeg-4.4.1-linux-64.zip"));
        assert!(resolve_ffmpeg_url("macos", "aarch64").ends_with("ffmpeg-darwin-arm64"));
        assert!(resolve_ffmpeg_url("macos", "x86_64").ends_with("ffmpeg-4.4.1-osx-64.zip"));
        assert!(resolve_ffmpeg_url("unknown", "unknown").ends_with("ffmpeg-4.4.1-linux-64.zip"));
    }

    #[test]
    fn test_resolve_dwdr_url_platforms() {
        assert!(resolve_dwdr_url("windows", "x86_64").ends_with("dwdr-windows.exe"));
        assert!(resolve_dwdr_url("windows", "aarch64").ends_with("dwdr-windows.exe"));
        assert!(resolve_dwdr_url("linux", "aarch64").ends_with("dwdr-linux-arm64"));
        assert!(resolve_dwdr_url("linux", "x86_64").ends_with("dwdr-linux"));
        assert!(resolve_dwdr_url("macos", "aarch64").ends_with("dwdr-mac-arm64"));
        assert!(resolve_dwdr_url("macos", "x86_64").ends_with("dwdr-mac"));
        assert!(resolve_dwdr_url("unknown", "unknown").ends_with("dwdr-linux"));
    }

    #[test]
    #[serial]
    fn test_resolve_proxy_url_precedence() {
        let vars = [
            "DWDR_PROXY",
            "HTTPS_PROXY",
            "https_proxy",
            "ALL_PROXY",
            "all_proxy",
            "HTTP_PROXY",
            "http_proxy",
        ];
        for v in &vars {
            unsafe { std::env::remove_var(v) };
        }
        assert_eq!(resolve_proxy_url(), None);

        unsafe { std::env::set_var("HTTP_PROXY", "http://127.0.0.1:8080") };
        assert_eq!(
            resolve_proxy_url(),
            Some("http://127.0.0.1:8080".to_string())
        );

        unsafe { std::env::set_var("HTTPS_PROXY", "http://127.0.0.1:8443") };
        assert_eq!(
            resolve_proxy_url(),
            Some("http://127.0.0.1:8443".to_string())
        );

        unsafe { std::env::set_var("DWDR_PROXY", "http://127.0.0.1:9000") };
        assert_eq!(
            resolve_proxy_url(),
            Some("http://127.0.0.1:9000".to_string())
        );

        for v in &vars {
            unsafe { std::env::remove_var(v) };
        }
    }

    #[test]
    fn test_is_retryable_status() {
        assert!(is_retryable_status(reqwest::StatusCode::REQUEST_TIMEOUT));
        assert!(is_retryable_status(reqwest::StatusCode::TOO_MANY_REQUESTS));
        assert!(is_retryable_status(
            reqwest::StatusCode::INTERNAL_SERVER_ERROR
        ));
        assert!(is_retryable_status(reqwest::StatusCode::BAD_GATEWAY));
        assert!(is_retryable_status(
            reqwest::StatusCode::SERVICE_UNAVAILABLE
        ));
        assert!(is_retryable_status(reqwest::StatusCode::GATEWAY_TIMEOUT));

        assert!(!is_retryable_status(reqwest::StatusCode::NOT_FOUND));
        assert!(!is_retryable_status(reqwest::StatusCode::UNAUTHORIZED));
        assert!(!is_retryable_status(reqwest::StatusCode::FORBIDDEN));
        assert!(!is_retryable_status(reqwest::StatusCode::OK));
    }

    #[test]
    fn test_download_with_retry_recovers_from_503() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        let handle = std::thread::spawn(move || {
            // First attempt: return 503
            if let Ok((mut stream, _)) = listener.accept() {
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                let _ = reader.read_line(&mut line);
                let resp = b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
                let _ = stream.write_all(resp);
                let _ = stream.flush();
                let _ = stream.shutdown(std::net::Shutdown::Write);
            }
            // Second attempt: return 200
            if let Ok((mut stream, _)) = listener.accept() {
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                let _ = reader.read_line(&mut line);
                let resp =
                    b"HTTP/1.1 200 OK\r\nContent-Length: 7\r\nConnection: close\r\n\r\nsuccess";
                let _ = stream.write_all(resp);
                let _ = stream.flush();
                let _ = stream.shutdown(std::net::Shutdown::Write);
            }
        });

        let client = reqwest::blocking::Client::builder()
            .no_proxy()
            .build()
            .unwrap();
        let url = format!("http://{}/retry-test", addr);
        let resp = download_with_retry(&client, &url, 3, Duration::from_millis(10)).unwrap();
        assert_eq!(resp.status().as_u16(), 200);

        let _ = handle.join();
    }

    #[test]
    fn test_download_with_retry_fails_on_404_without_retrying() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        let handle = std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                let _ = reader.read_line(&mut line);
                let resp =
                    b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
                let _ = stream.write_all(resp);
                let _ = stream.flush();
                let _ = stream.shutdown(std::net::Shutdown::Write);
            }
        });

        let client = reqwest::blocking::Client::builder()
            .no_proxy()
            .build()
            .unwrap();
        let url = format!("http://{}/not-found", addr);
        let err = download_with_retry(&client, &url, 3, Duration::from_millis(10)).unwrap_err();
        assert!(err.contains("Download failed with status: 404 Not Found"));

        let _ = handle.join();
    }

    #[test]
    fn test_download_with_retry_recovers_from_mid_stream_disconnection() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        let handle = std::thread::spawn(move || {
            // First attempt: HTTP 200 with truncated body (declares 50 bytes, sends 5, then abruptly closes)
            if let Ok((mut stream, _)) = listener.accept() {
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                let _ = reader.read_line(&mut line);
                let resp =
                    b"HTTP/1.1 200 OK\r\nContent-Length: 50\r\nConnection: close\r\n\r\nshort";
                let _ = stream.write_all(resp);
                let _ = stream.flush();
                let _ = stream.shutdown(std::net::Shutdown::Both);
            }
            // Second attempt: HTTP 200 with full body
            if let Ok((mut stream, _)) = listener.accept() {
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                let _ = reader.read_line(&mut line);
                let resp =
                    b"HTTP/1.1 200 OK\r\nContent-Length: 7\r\nConnection: close\r\n\r\nsuccess";
                let _ = stream.write_all(resp);
                let _ = stream.flush();
                let _ = stream.shutdown(std::net::Shutdown::Write);
            }
        });

        let client = reqwest::blocking::Client::builder()
            .no_proxy()
            .build()
            .unwrap();
        let url = format!("http://{}/truncated-test", addr);
        let resp = download_with_retry(&client, &url, 3, Duration::from_millis(10)).unwrap();
        assert_eq!(resp.status().as_u16(), 200);
        assert_eq!(resp.bytes().unwrap(), b"success");

        let _ = handle.join();
    }
}
