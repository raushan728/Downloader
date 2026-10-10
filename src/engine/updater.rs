use colored::Colorize;
use std::fs;
use std::io::Write;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

pub fn ensure_dependencies() -> Result<(), String> {
    let yt_dlp_path = crate::engine::paths::get_ytdlp_path();

    if yt_dlp_path.exists() {
        return Ok(());
    }

    println!(
        "{}",
        "Core engine missing. Downloading latest yt-dlp... This will only happen once. ⏳".yellow()
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

    if let Some(parent) = yt_dlp_path.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create deps directory: {}", e))?;
        }
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

    println!("{}", "✅ Engine downloaded successfully!".green());

    Ok(())
}
