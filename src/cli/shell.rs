use colored::Colorize;
use inquire::Text;

pub fn process_url(url: &str, config: &crate::config::manager::DwdrConfig) {
    if !crate::utils::validator::is_valid_url(url) {
        println!(
            "{}",
            "\nERROR: Invalid URL provided. It must start with http:// or https://\n"
                .red()
                .bold()
        );
        return;
    }

    println!("{}", "Valid URL detected. Resolving engine paths...".cyan());

    let yt_dlp_path = crate::engine::paths::get_ytdlp_path();
    let ffmpeg_path = crate::engine::paths::get_ffmpeg_path();

    if let Err(e) = crate::engine::updater::ensure_dependencies() {
        println!(
            "{}",
            format!("\nERROR: Failed to setup engine: {}\n", e)
                .red()
                .bold()
        );
        return;
    }

    println!(
        "{}",
        "INFO: Fetching media details from the server...".cyan()
    );

    match crate::engine::downloader::fetch_video_info(url, &yt_dlp_path) {
        Ok(info) => {
            println!(
                "{}",
                format!("Successfully fetched data for: {}", info.title).green()
            );
            println!(
                "{}",
                format!("Extracted {} formats.", info.formats.len()).green()
            );

            let chosen_format = if let Some(format_id) =
                crate::engine::downloader::find_format_by_quality(
                    &info.formats,
                    &config.default_quality,
                ) {
                println!(
                    "{}",
                    format!(
                        "INFO: Exact quality ({}) found! Auto-starting download...",
                        config.default_quality
                    )
                    .green()
                );
                Some(format_id)
            } else {
                println!(
                    "{}",
                    format!(
                        "WARNING: Quality ({}) not found. Falling back to manual selection...",
                        config.default_quality
                    )
                    .yellow()
                );
                if let Some(format_id) = crate::cli::prompts::select_format(&info.formats) {
                    println!(
                        "{}",
                        format!(
                            "SUCCESS: Selected Format ID: {}. Download sequence will start here.",
                            format_id
                        )
                        .green()
                    );
                    Some(format_id)
                } else {
                    println!("{}", "WARNING: Download cancelled by user.".red());
                    None
                }
            };

            if let Some(format_id) = chosen_format {
                match crate::engine::downloader::download_media(
                    url,
                    &format_id,
                    &yt_dlp_path,
                    &ffmpeg_path,
                    &config.download_path,
                ) {
                    Ok(()) => {
                        println!(
                            "{}",
                            format!(
                                "SUCCESS: Download completed successfully! File saved to: {}",
                                config.download_path
                            )
                            .green()
                            .bold()
                        );
                    }
                    Err(e) => {
                        println!(
                            "{}",
                            format!("\nERROR: Download failed: {}\n", e).red().bold()
                        );
                    }
                }
            }
        }
        Err(e) => {
            println!(
                "{}",
                format!("\nERROR: Failed to extract video info: {}\n", e).red()
            );
        }
    }
}

pub fn start_repl(config: &crate::config::manager::DwdrConfig) {
    println!(
        "{}",
        "INFO: Welcome to DWDR Interactive Shell. Type a URL to download, or 'exit' to quit."
            .cyan()
    );
    loop {
        match Text::new("dwdr >").prompt() {
            Ok(input) => {
                let input = input.trim();
                if input.is_empty() {
                    continue;
                }
                if input.eq_ignore_ascii_case("exit") || input.eq_ignore_ascii_case("quit") {
                    println!("{}", "INFO: Thanks for using DWDR!".green());
                    break;
                }
                process_url(input, config);
            }
            Err(_) => {
                println!("{}", "INFO: Thanks for using DWDR!".green());
                break;
            }
        }
    }
}
