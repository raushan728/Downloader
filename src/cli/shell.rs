use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::Arc;

use colored::Colorize;
use inquire::Text;
use rfd::FileDialog;

use crate::cli::prompts::{interactive_config, select_format};
use crate::config::manager::{ConfigManager, DwdrConfig};
use crate::engine::downloader::{download_media, fetch_video_info, find_format_by_quality};
use crate::engine::paths::{get_ffmpeg_path, get_ytdlp_path};
use crate::engine::queue::{EnqueueResult, QueueManager, set_global_queue};
use crate::engine::updater::ensure_dependencies;
use crate::utils::validator::is_valid_url;

pub fn resolve_media_to_download(
    url: &str,
    config: &mut DwdrConfig,
) -> Option<(String, String, PathBuf, PathBuf)> {
    if !is_valid_url(url) {
        println!(
            "{}",
            "\nERROR: Invalid URL provided. It must start with http:// or https://\n"
                .red()
                .bold()
        );
        return None;
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
        return None;
    }

    println!(
        "{}",
        "INFO: Fetching media details from the server...".cyan()
    );

    let info = match fetch_video_info(url, &yt_dlp_path) {
        Ok(info) => {
            println!(
                "{}",
                format!("Successfully fetched data for: {}", info.title).green()
            );
            println!(
                "{}",
                format!("Extracted {} formats.", info.formats.len()).green()
            );
            info
        }
        Err(e) => {
            println!(
                "{}",
                format!("\nERROR: Failed to extract video info: {}\n", e).red()
            );
            return None;
        }
    };

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
    } else if let Some(format_id) = find_format_by_quality(&info.formats, &config.default_quality) {
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

    let format_id = chosen_format?;

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
            return None;
        }
    }

    Some((info.title, format_id, yt_dlp_path, ffmpeg_path))
}

pub fn process_url(url: &str, config: &mut DwdrConfig) {
    if let Some((title, format_id, yt_dlp_path, ffmpeg_path)) =
        resolve_media_to_download(url, config)
    {
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
                        "SUCCESS: Download completed successfully! File saved: \"{}\" in {}",
                        title, config.download_path
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

pub fn start_repl(config: &mut DwdrConfig) {
    println!(
        "{}",
        "INFO: Welcome to DWDR Interactive Shell. Type a URL to download, or 'exit' to quit."
            .cyan()
    );

    let yt_dlp_path = get_ytdlp_path();
    let ffmpeg_path = get_ffmpeg_path();
    let queue_manager = QueueManager::new(
        config.max_parallel_downloads as usize,
        yt_dlp_path,
        ffmpeg_path,
    );
    set_global_queue(Arc::clone(&queue_manager));

    loop {
        match Text::new("dwdr >").prompt() {
            Ok(input) => {
                let input = input.trim();
                if input.is_empty() {
                    continue;
                }
                if input.eq_ignore_ascii_case("help") {
                    println!("\n{}", "Available Commands:".bold());
                    println!("  <URL>             - Download media stream (managed via queue)");
                    println!(
                        "  queue / status    - View live progress of active and queued downloads"
                    );
                    println!("  cancel            - Cancel all active downloads and clear queue");
                    println!(
                        "  config / settings - Configure download path, quality, and parallel limits"
                    );
                    println!("  cls / clear       - Clear terminal screen");
                    println!("  exit / quit       - Cancel tasks and exit shell\n");
                    continue;
                }
                if input.eq_ignore_ascii_case("queue")
                    || input.eq_ignore_ascii_case("status")
                    || input.eq_ignore_ascii_case("list")
                {
                    queue_manager.lock().unwrap().print_status();
                    continue;
                }
                if input.eq_ignore_ascii_case("cancel") {
                    queue_manager.lock().unwrap().cancel_all();
                    println!(
                        "{}",
                        "INFO: Cancelled all active downloads and cleared queue.".yellow()
                    );
                    continue;
                }
                if input.eq_ignore_ascii_case("config") || input.eq_ignore_ascii_case("settings") {
                    interactive_config(config);
                    queue_manager.lock().unwrap().max_parallel =
                        config.max_parallel_downloads as usize;
                    continue;
                }
                if input.eq_ignore_ascii_case("cls") || input.eq_ignore_ascii_case("clear") {
                    print!("{}[2J{}[1;1H", 27 as char, 27 as char);
                    let _ = io::stdout().flush();
                    continue;
                }
                if input.eq_ignore_ascii_case("exit") || input.eq_ignore_ascii_case("quit") {
                    queue_manager.lock().unwrap().cancel_all();
                    println!("{}", "INFO: Thanks for using DWDR!".green());
                    break;
                }

                if let Some((title, format_id, _, _)) = resolve_media_to_download(input, config) {
                    let result = QueueManager::enqueue(
                        &queue_manager,
                        input.to_string(),
                        title.clone(),
                        format_id,
                        config.download_path.clone(),
                    );
                    let max_p = queue_manager.lock().unwrap().max_parallel;
                    match result {
                        EnqueueResult::Started {
                            active_count,
                            queued_count,
                            ..
                        } => {
                            println!(
                                "{}",
                                format!(
                                    "INFO: Download started for: \"{}\" [Active: {}/{}, Queued: {}]",
                                    title, active_count, max_p, queued_count
                                )
                                .green()
                            );
                        }
                        EnqueueResult::Queued {
                            position,
                            active_count,
                            queued_count,
                            ..
                        } => {
                            println!(
                                "{}",
                                format!(
                                    "INFO: Added to download queue: \"{}\" [Active: {}/{}, Queued: {} (Position: #{})]",
                                    title, active_count, max_p, queued_count, position
                                )
                                .yellow()
                            );
                        }
                    }
                }
            }
            Err(_) => {
                queue_manager.lock().unwrap().cancel_all();
                println!("{}", "INFO: Thanks for using DWDR!".green());
                break;
            }
        }
    }
}
