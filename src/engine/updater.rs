use colored::Colorize;
use std::fs;
use std::io::Seek;
use std::io::Write;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

pub fn ensure_dependencies() -> Result<(), String> {
    let yt_dlp_path = crate::engine::paths::get_ytdlp_path();

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

        let response =
            reqwest::blocking::get(url).map_err(|e| format!("Failed to download yt-dlp: {}", e))?;

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

        let mut file = fs::File::create(&yt_dlp_path)
            .map_err(|e| format!("Failed to create yt-dlp file: {}", e))?;

        file.write_all(&bytes)
            .map_err(|e| format!("Failed to write yt-dlp file: {}", e))?;

        #[cfg(unix)]
        {
            let mut perms = fs::metadata(&yt_dlp_path)
                .map_err(|e| format!("Failed to read metadata: {}", e))?
                .permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&yt_dlp_path, perms)
                .map_err(|e| format!("Failed to set permissions: {}", e))?;
        }

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
    let ffmpeg_path = crate::engine::paths::get_ffmpeg_path();
    if ffmpeg_path.exists() {
        return Ok(());
    }

    println!("{}", "INFO: Media Merger (FFmpeg) is missing. Downloading prebuilt binary... This will only happen once.".yellow());

    let url = get_ffmpeg_download_url();

    let client = reqwest::blocking::Client::builder()
        .user_agent("dwdr-cli/1.0")
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

            let mut out_file = std::fs::File::create(&ffmpeg_path)
                .map_err(|e| format!("Failed to create FFmpeg file: {}", e))?;
            std::io::copy(&mut file, &mut out_file)
                .map_err(|e| format!("Failed to write FFmpeg file: {}", e))?;
            found = true;
            break;
        }
    }

    if !found {
        return Err("FFmpeg executable not found in the downloaded archive".to_string());
    }

    #[cfg(unix)]
    {
        let mut perms = fs::metadata(&ffmpeg_path)
            .map_err(|e| format!("Failed to read metadata: {}", e))?
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&ffmpeg_path, perms)
            .map_err(|e| format!("Failed to set permissions: {}", e))?;
    }

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

    let response =
        reqwest::blocking::get(url).map_err(|e| format!("Failed to download update: {}", e))?;

    if !response.status().is_success() {
        return Err(format!(
            "Download failed with status: {}",
            response.status()
        ));
    }

    let bytes = response
        .bytes()
        .map_err(|e| format!("Failed to read update bytes: {}", e))?;

    let tmp_exe = current_exe.with_extension("tmp");

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
        let _ = std::fs::rename(&current_exe, &old_exe);
        std::fs::rename(&tmp_exe, &current_exe)
            .map_err(|e| format!("Failed to replace binary: {}", e))?;
    }

    println!(
        "{}",
        "SUCCESS: DWDR updated to the latest version! Restart the tool to apply changes.".green()
    );

    Ok(())
}
