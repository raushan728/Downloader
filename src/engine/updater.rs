use std::fs;
use std::io::{Read, Seek, Write};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use colored::Colorize;
use sha2::{Digest, Sha256};

use crate::engine::paths::{get_ffmpeg_path, get_ytdlp_path};
use crate::ui::UiContext;
use crate::ui::spinner::Spinner;

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
        ("linux", _) => "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp_linux",
        ("macos", _) => "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp_macos",
        _ => "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp_linux",
    }
}

pub fn get_ytdlp_download_url() -> String {
    std::env::var("DWDR_YTDLP_DOWNLOAD_URL").unwrap_or_else(|_| {
        resolve_ytdlp_url(std::env::consts::OS, std::env::consts::ARCH).to_string()
    })
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

pub fn get_ffmpeg_download_url() -> String {
    std::env::var("DWDR_FFMPEG_DOWNLOAD_URL").unwrap_or_else(|_| {
        resolve_ffmpeg_url(std::env::consts::OS, std::env::consts::ARCH).to_string()
    })
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

pub fn get_dwdr_download_url() -> String {
    std::env::var("DWDR_SELF_DOWNLOAD_URL").unwrap_or_else(|_| {
        resolve_dwdr_url(std::env::consts::OS, std::env::consts::ARCH).to_string()
    })
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

pub fn update_ytdlp_engine() -> Result<(), String> {
    if let Ok(custom) = std::env::var("DWDR_YTDLP_PATH") {
        println!(
            "{}",
            format!(
                "INFO: DWDR_YTDLP_PATH is set ('{}'). Skipping managed yt-dlp update.",
                custom
            )
            .yellow()
        );
        return Ok(());
    }

    let yt_dlp_path = get_ytdlp_path();
    let ctx = UiContext::get();
    let spinner = Spinner::new(
        "Downloading latest yt-dlp release...",
        ctx.theme,
        ctx.color_level,
        ctx.animations,
    );

    let url = get_ytdlp_download_url();
    let client = create_http_client("dwdr-cli/1.0")?;
    let response = download_with_retry(&client, &url, 3, Duration::from_millis(1000))?;

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
        yt_dlp_path.with_extension(format!("tmp.{}.exe", std::process::id()))
    } else {
        yt_dlp_path.with_extension(format!("tmp.{}", std::process::id()))
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

    #[cfg(target_os = "windows")]
    {
        let old_path = yt_dlp_path.with_extension("old");
        if old_path.exists() {
            let _ = fs::remove_file(&old_path);
        }
        if yt_dlp_path.exists() {
            let _ = fs::rename(&yt_dlp_path, &old_path);
        }
    }

    fs::rename(&tmp_path, &yt_dlp_path)
        .map_err(|e| format!("Failed to finalize yt-dlp binary: {}", e))?;

    spinner.finish_success("Engine yt-dlp updated successfully!");
    Ok(())
}

pub fn update_ffmpeg_engine() -> Result<(), String> {
    if let Ok(custom) = std::env::var("DWDR_FFMPEG_PATH") {
        println!(
            "{}",
            format!(
                "INFO: DWDR_FFMPEG_PATH is set ('{}'). Skipping managed FFmpeg update.",
                custom
            )
            .yellow()
        );
        return Ok(());
    }

    let ffmpeg_path = get_ffmpeg_path();
    let ctx = UiContext::get();
    let spinner = Spinner::new(
        "Downloading latest FFmpeg release...",
        ctx.theme,
        ctx.color_level,
        ctx.animations,
    );

    let url = get_ffmpeg_download_url();
    let client = create_http_client("dwdr-cli/1.0")?;
    let response = download_with_retry(&client, &url, 3, Duration::from_millis(1000))?;

    let expected_filename = if cfg!(target_os = "windows") {
        "ffmpeg.exe"
    } else {
        "ffmpeg"
    };

    let tmp_ffmpeg = if cfg!(target_os = "windows") {
        ffmpeg_path.with_extension(format!("tmp.{}.exe", std::process::id()))
    } else {
        ffmpeg_path.with_extension(format!("tmp.{}", std::process::id()))
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

    #[cfg(target_os = "windows")]
    {
        let old_path = ffmpeg_path.with_extension("old");
        if old_path.exists() {
            let _ = fs::remove_file(&old_path);
        }
        if ffmpeg_path.exists() {
            let _ = fs::rename(&ffmpeg_path, &old_path);
        }
    }

    fs::rename(&tmp_ffmpeg, &ffmpeg_path)
        .map_err(|e| format!("Failed to finalize FFmpeg binary: {}", e))?;

    spinner.finish_success("Media Merger (FFmpeg) updated successfully!");
    Ok(())
}

static DEPENDENCIES_VALIDATED: AtomicBool = AtomicBool::new(false);

pub fn reset_dependencies_validated() {
    DEPENDENCIES_VALIDATED.store(false, Ordering::SeqCst);
}

pub fn update_core_engines() -> Result<(), String> {
    reset_dependencies_validated();
    update_ytdlp_engine()?;
    update_ffmpeg_engine()?;
    Ok(())
}

pub fn ensure_dependencies() -> Result<(), String> {
    if DEPENDENCIES_VALIDATED.load(Ordering::SeqCst) {
        return Ok(());
    }

    let yt_dlp_path = get_ytdlp_path();

    if std::env::var("DWDR_YTDLP_PATH").is_ok() && yt_dlp_path.exists() {
        let res = ensure_ffmpeg();
        if res.is_ok() {
            DEPENDENCIES_VALIDATED.store(true, Ordering::SeqCst);
        }
        return res;
    }

    if !yt_dlp_path.exists() || !is_valid_executable(&yt_dlp_path, false) {
        if yt_dlp_path.exists() {
            println!(
                "{}",
                "WARNING: Detected corrupted or invalid yt-dlp binary. Re-downloading...".yellow()
            );
        } else {
            println!(
                "{}",
                "INFO: Core engine missing. Downloading latest yt-dlp... This will only happen once."
                    .yellow()
            );
        }
        update_ytdlp_engine()?;
    }

    ensure_ffmpeg()?;
    DEPENDENCIES_VALIDATED.store(true, Ordering::SeqCst);
    Ok(())
}

pub fn ensure_ffmpeg() -> Result<(), String> {
    let ffmpeg_path = get_ffmpeg_path();

    if std::env::var("DWDR_FFMPEG_PATH").is_ok() && ffmpeg_path.exists() {
        return Ok(());
    }

    if !ffmpeg_path.exists() || !is_valid_executable(&ffmpeg_path, true) {
        if ffmpeg_path.exists() {
            println!(
                "{}",
                "WARNING: Detected corrupted or invalid FFmpeg binary. Re-downloading...".yellow()
            );
        } else {
            println!(
                "{}",
                "INFO: Media Merger (FFmpeg) is missing. Downloading prebuilt binary... This will only happen once."
                    .yellow()
            );
        }
        update_ffmpeg_engine()?;
    }

    Ok(())
}

pub fn fetch_latest_dwdr_version(client: &reqwest::blocking::Client) -> Option<String> {
    if std::env::var("DWDR_DISABLE_VERSION_CHECK").is_ok() {
        return None;
    }
    let api_url = std::env::var("DWDR_RELEASE_API_URL").unwrap_or_else(|_| {
        "https://api.github.com/repos/raushan728/Downloader/releases/latest".to_string()
    });
    let resp = client.get(&api_url).send().ok()?;
    if resp.status().is_success() {
        let body = resp.text().ok()?;
        let json: serde_json::Value = serde_json::from_str(&body).ok()?;
        if let Some(tag) = json["tag_name"].as_str() {
            return Some(tag.trim_start_matches('v').to_string());
        }
    }
    None
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

    let client = create_http_client("dwdr-updater")?;

    // Version check against latest tag if available
    let current_ver = env!("CARGO_PKG_VERSION");
    if let Some(latest_ver) = fetch_latest_dwdr_version(&client)
        && !is_newer_version(current_ver, &latest_ver)
    {
        println!(
            "{}",
            format!(
                "INFO: DWDR is already up to date (version v{}).",
                current_ver
            )
            .green()
        );
        return Ok(());
    }

    let url = get_dwdr_download_url();

    let ctx = UiContext::get();
    let spinner = Spinner::new(
        "Downloading latest DWDR release...",
        ctx.theme,
        ctx.color_level,
        ctx.animations,
    );

    let response = download_with_retry(&client, &url, 3, Duration::from_millis(1000))?;

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

    // Verify SHA256 checksum if release checksum manifest is available
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let actual_hash = format!("{:x}", hasher.finalize());

    let checksum_url = std::env::var("DWDR_CHECKSUM_URL").unwrap_or_else(|_| {
        "https://github.com/raushan728/Downloader/releases/download/latest/SHA256SUMS".to_string()
    });
    if let Ok(resp) = client.get(&checksum_url).send()
        && resp.status().is_success()
        && let Ok(manifest) = resp.text()
    {
        let target_filename = url.rsplit('/').next().unwrap_or("");
        let mut expected_hash = None;
        for line in manifest.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                let fn_entry = parts[1].trim_start_matches('*');
                if fn_entry == target_filename || parts[1].contains(target_filename) {
                    expected_hash = Some(parts[0].to_lowercase());
                    break;
                }
            }
        }

        if let Some(expected) = expected_hash {
            if actual_hash != expected {
                return Err(format!(
                    "SHA256 checksum mismatch for DWDR binary. Expected: {}, Actual: {}",
                    expected, actual_hash
                ));
            }
            println!("{}", "INFO: SHA256 checksum verified.".green());
        }
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

    {
        let mut file = fs::File::create(&tmp_exe)
            .map_err(|e| format!("Failed to create temporary executable: {}", e))?;

        file.write_all(&bytes)
            .map_err(|e| format!("Failed to write temporary executable: {}", e))?;

        file.flush()
            .map_err(|e| format!("Failed to flush temporary executable: {}", e))?;
    }

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

    spinner
        .finish_success("DWDR updated to the latest version! Restart the tool to apply changes.");

    Ok(())
}

/// Compares two CalVer or SemVer version strings and returns true if `latest` is strictly newer than `installed`.
pub fn is_newer_version(installed: &str, latest: &str) -> bool {
    let parse_parts = |v: &str| -> Vec<u32> {
        v.trim_start_matches('v')
            .split('.')
            .filter_map(|s| s.parse::<u32>().ok())
            .collect()
    };
    let inst_parts = parse_parts(installed);
    let lat_parts = parse_parts(latest);

    if inst_parts.is_empty() || lat_parts.is_empty() {
        return false;
    }

    lat_parts > inst_parts
}

/// Queries the installed yt-dlp binary version via `--version`.
pub fn get_installed_ytdlp_version(yt_dlp_path: &Path) -> Option<String> {
    if !yt_dlp_path.exists() {
        return None;
    }
    let output = std::process::Command::new(yt_dlp_path)
        .arg("--version")
        .output()
        .ok()?;
    if output.status.success() {
        let ver = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !ver.is_empty() {
            return Some(ver);
        }
    }
    None
}

/// Queries the latest yt-dlp release version tag via GitHub releases redirect header.
pub fn fetch_latest_ytdlp_version(endpoint_url: Option<&str>) -> Result<String, String> {
    let url = endpoint_url.unwrap_or("https://github.com/yt-dlp/yt-dlp/releases/latest");
    let mut builder = reqwest::blocking::Client::builder()
        .user_agent("dwdr-cli/1.0")
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(3));

    if let Some(proxy_url) = resolve_proxy_url()
        && let Ok(proxy) = reqwest::Proxy::all(&proxy_url)
    {
        builder = builder.proxy(proxy);
    }

    let client = builder
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    let res = client
        .head(url)
        .send()
        .or_else(|_| client.get(url).send())
        .map_err(|e| format!("Request failed: {}", e))?;

    if let Some(loc) = res.headers().get(reqwest::header::LOCATION) {
        if let Ok(loc_str) = loc.to_str()
            && let Some(tag) = loc_str.rsplit('/').next()
        {
            let clean_tag = tag.trim_start_matches('v').trim();
            if !clean_tag.is_empty() {
                return Ok(clean_tag.to_string());
            }
        }
    } else if res.status().is_success()
        && let Ok(body) = res.text()
    {
        let trimmed = body.trim().trim_start_matches('v').trim();
        if !trimmed.is_empty() && trimmed.chars().all(|c| c.is_ascii_digit() || c == '.') {
            return Ok(trimmed.to_string());
        }
    }

    Err("Could not determine latest version from release response".to_string())
}

/// Checks if a newer yt-dlp engine release is available compared to the locally installed binary.
/// Returns Some((installed_version, latest_version)) if an update is available.
pub fn check_engine_update_available(yt_dlp_path: &Path) -> Option<(String, String)> {
    // Hermetic test guard: do not make external network requests in test environment
    if std::env::var("DWDR_YTDLP_PATH").is_ok() {
        return None;
    }

    let installed = get_installed_ytdlp_version(yt_dlp_path)?;
    let latest = fetch_latest_ytdlp_version(None).ok()?;

    if is_newer_version(&installed, &latest) {
        Some((installed, latest))
    } else {
        None
    }
}

/// Formats an actionable warning message when media extraction or download fails
/// and a newer core engine release is available.
pub fn format_engine_update_reason(installed: &str, latest: &str) -> String {
    format!(
        "WARNING: A newer engine version ({}) is available (installed: {}).\n\
         Media platforms (e.g. YouTube) frequently update extraction algorithms and video players.\n\
         Run 'dwdr update' to update both core engines (yt-dlp and ffmpeg) to the latest release.",
        latest, installed
    )
}

/// Queries the installed FFmpeg binary version via `-version`.
pub fn get_installed_ffmpeg_version(ffmpeg_path: &Path) -> Option<String> {
    if !ffmpeg_path.exists() {
        return None;
    }
    let output = std::process::Command::new(ffmpeg_path)
        .arg("-version")
        .output()
        .ok()?;
    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        if let Some(first_line) = stdout.lines().next() {
            let parts: Vec<&str> = first_line.split_whitespace().collect();
            if parts.len() >= 3 && parts[0] == "ffmpeg" && parts[1] == "version" {
                return Some(parts[2].to_string());
            }
        }
    }
    None
}

/// Inspects core engine dependencies (yt-dlp and ffmpeg) upon download failure and returns
/// actionable advice if an engine is outdated, missing, or corrupted.
pub fn check_core_engines_update(yt_dlp_path: &Path, ffmpeg_path: &Path) -> Option<String> {
    // Hermetic test guard: do not make external network requests in test environment
    if std::env::var("DWDR_YTDLP_PATH").is_ok() || std::env::var("DWDR_FFMPEG_PATH").is_ok() {
        return None;
    }

    // Check FFmpeg binary validity
    if !is_valid_executable(ffmpeg_path, true) {
        return Some(
            "WARNING: FFmpeg media merger binary is missing or corrupted.\n\
             Run 'dwdr update' to re-download and restore core engines (yt-dlp and ffmpeg)."
                .to_string(),
        );
    }

    // Check if a newer yt-dlp release is available
    if let Some((installed, latest)) = check_engine_update_available(yt_dlp_path) {
        return Some(format_engine_update_reason(&installed, &latest));
    }

    None
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
        assert!(resolve_ytdlp_url("linux", "x86_64").ends_with("yt-dlp_linux"));
        assert!(resolve_ytdlp_url("macos", "aarch64").ends_with("yt-dlp_macos"));
        assert!(resolve_ytdlp_url("macos", "x86_64").ends_with("yt-dlp_macos"));
        assert!(resolve_ytdlp_url("unknown", "unknown").ends_with("yt-dlp_linux"));
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

    #[test]
    fn test_is_newer_version() {
        assert!(is_newer_version("2026.08.19", "2026.10.21"));
        assert!(is_newer_version("v2026.08.19", "v2026.10.21"));
        assert!(is_newer_version("2026.10.21", "2026.10.21.1"));
        assert!(!is_newer_version("2026.10.21", "2026.10.21"));
        assert!(!is_newer_version("2026.10.22", "2026.10.21"));
        assert!(!is_newer_version("invalid", "2026.10.21"));
        assert!(!is_newer_version("2026.10.21", "invalid"));
        assert!(!is_newer_version("", ""));
    }

    #[test]
    fn test_format_engine_update_reason() {
        let msg = format_engine_update_reason("2026.08.19", "2026.10.21");
        assert!(msg.contains(
            "WARNING: A newer engine version (2026.10.21) is available (installed: 2026.08.19)"
        ));
        assert!(msg.contains("YouTube"));
        assert!(msg.contains("dwdr update"));
    }

    #[test]
    #[serial]
    fn test_fetch_latest_ytdlp_version_redirect() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        let handle = std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                let _ = reader.read_line(&mut line);
                let resp = b"HTTP/1.1 302 Found\r\nLocation: https://github.com/yt-dlp/yt-dlp/releases/tag/2026.10.21\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
                let _ = stream.write_all(resp);
                let _ = stream.flush();
                let _ = stream.shutdown(std::net::Shutdown::Write);
            }
        });

        let url = format!("http://{}/releases/latest", addr);
        let version = fetch_latest_ytdlp_version(Some(&url)).unwrap();
        assert_eq!(version, "2026.10.21");

        let _ = handle.join();
    }

    #[test]
    #[serial]
    fn test_check_engine_update_available_offline_guard() {
        let temp_dir = tempfile::tempdir().unwrap();
        let fake_bin = temp_dir.path().join("fake_yt");
        std::fs::write(&fake_bin, b"#!/bin/sh\necho 2026.08.19\n").unwrap();

        // DWDR_YTDLP_PATH set -> hermetic guard returns None without network calls
        unsafe {
            std::env::set_var("DWDR_YTDLP_PATH", fake_bin.to_str().unwrap());
        }
        let result = check_engine_update_available(&fake_bin);
        assert!(result.is_none());
        unsafe {
            std::env::remove_var("DWDR_YTDLP_PATH");
        }
    }

    #[test]
    #[serial]
    fn test_check_core_engines_update_guards() {
        let temp_dir = tempfile::tempdir().unwrap();
        let yt = temp_dir.path().join("yt");
        let ff = temp_dir.path().join("ff");

        // When DWDR_FFMPEG_PATH or DWDR_YTDLP_PATH is set, returns None (offline test guard)
        unsafe {
            std::env::set_var("DWDR_FFMPEG_PATH", ff.to_str().unwrap());
        }
        assert!(check_core_engines_update(&yt, &ff).is_none());
        unsafe {
            std::env::remove_var("DWDR_FFMPEG_PATH");
        }

        // When FFmpeg binary is missing / invalid executable, detects corruption
        let advice = check_core_engines_update(&yt, &ff);
        assert!(advice.is_some());
        let msg = advice.unwrap();
        assert!(msg.contains("FFmpeg media merger binary is missing or corrupted"));
        assert!(msg.contains("dwdr update"));
    }

    #[test]
    #[serial]
    fn test_update_engines_respects_custom_path_overrides() {
        let temp_dir = tempfile::tempdir().unwrap();
        let custom_yt = temp_dir.path().join("my_custom_yt");
        let custom_ff = temp_dir.path().join("my_custom_ff");
        std::fs::write(&custom_yt, b"custom-yt-content").unwrap();
        std::fs::write(&custom_ff, b"custom-ff-content").unwrap();

        unsafe {
            std::env::set_var("DWDR_YTDLP_PATH", custom_yt.to_str().unwrap());
            std::env::set_var("DWDR_FFMPEG_PATH", custom_ff.to_str().unwrap());
        }

        // Must succeed without deleting or touching custom paths
        let res_yt = update_ytdlp_engine();
        let res_ff = update_ffmpeg_engine();
        let res_all = update_core_engines();

        assert!(res_yt.is_ok());
        assert!(res_ff.is_ok());
        assert!(res_all.is_ok());

        assert!(custom_yt.exists());
        assert!(custom_ff.exists());
        assert_eq!(std::fs::read(&custom_yt).unwrap(), b"custom-yt-content");
        assert_eq!(std::fs::read(&custom_ff).unwrap(), b"custom-ff-content");

        unsafe {
            std::env::remove_var("DWDR_YTDLP_PATH");
            std::env::remove_var("DWDR_FFMPEG_PATH");
        }
    }
}
