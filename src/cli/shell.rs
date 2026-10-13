use std::io::{self, Write};

use colored::Colorize;
use inquire::Text;
use rfd::FileDialog;

use crate::cli::prompts::{interactive_config, select_format};
use crate::config::manager::{ConfigManager, DwdrConfig};
use crate::engine::downloader::{download_media, fetch_video_info, find_format_by_quality};
use crate::engine::paths::{get_ffmpeg_path, get_ytdlp_path};
use crate::engine::updater::ensure_dependencies;
use crate::utils::validator::is_valid_url;

pub fn process_url(url: &str, config: &mut DwdrConfig) {
    if !is_valid_url(url) {
        println!(
            "{}",
            "\nERROR: Invalid URL provided. It must start with http:// or https://\n"
                .red()
                .bold()
        );
        return;
    }

    println!("{}", "Valid URL detected. Resolving engine paths...".cyan());

    let yt_dlp_path = get_ytdlp_path();
    let ffmpeg_path = get_ffmpeg_path();

    if let Err(e) = ensure_dependencies() {
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

    match fetch_video_info(url, &yt_dlp_path) {
        Ok(info) => {
            println!(
                "{}",
                format!("Successfully fetched data for: {}", info.title).green()
            );
            println!(
                "{}",
                format!("Extracted {} formats.", info.formats.len()).green()
            );

            let chosen_format = if config.default_quality == "always ask" {
                println!("{}", "INFO: Manual selection requested...".cyan());
                if let Some(format_id) = select_format(&info.formats) {
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
            } else if let Some(format_id) =
                find_format_by_quality(&info.formats, &config.default_quality)
            {
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
                        "WARNING: Your selected default quality ({}) is not available in this video. Falling back to manual selection...",
                        config.default_quality
                    )
                    .yellow()
                );
                if let Some(format_id) = select_format(&info.formats) {
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
                if config.download_path.is_empty() {
                    println!(
                        "{}",
                        "INFO: No download path set. Opening file browser...".cyan()
                    );
                    if let Some(folder) = FileDialog::new()
                        .set_title("Select Download Folder")
                        .pick_folder()
                    {
                        config.download_path = folder.to_string_lossy().to_string();
                        ConfigManager::save(config);
                    } else {
                        println!(
                            "{}",
                            "\nERROR: No download folder selected. Download cancelled.\n"
                                .red()
                                .bold()
                        );
                        return;
                    }
                }

                match download_media(
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

pub fn start_repl(config: &mut DwdrConfig) {
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
                if input.eq_ignore_ascii_case("config") || input.eq_ignore_ascii_case("settings") {
                    interactive_config(config);
                    continue;
                }
                if input.eq_ignore_ascii_case("cls") || input.eq_ignore_ascii_case("clear") {
                    print!("{}[2J{}[1;1H", 27 as char, 27 as char);
                    let _ = io::stdout().flush();
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
