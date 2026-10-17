use std::io::{self, BufRead, IsTerminal, Write};
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

/// Validates the URL, fetches available formats, and resolves the target format and destination directory.
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

    if info.formats.is_empty() {
        println!(
            "{}",
            "\nERROR: No downloadable formats found for this media stream.\n"
                .red()
                .bold()
        );
        return None;
    }

    let (chosen_id, is_audio) = if info.formats.len() == 1 {
        // Skip manual prompts when only a single format exists to avoid unnecessary confirmation.
        let single_fmt = &info.formats[0];
        println!(
            "{}",
            format!(
                "INFO: Single available format ({}) detected. Auto-starting download...",
                single_fmt.resolution
            )
            .green()
        );
        (single_fmt.format_id.clone(), single_fmt.is_audio())
    } else if config.default_quality == "always ask" {
        println!("{}", "INFO: Manual selection requested...".cyan());
        if let Some(format_id) = select_format(&info.formats) {
            let is_aud = info
                .formats
                .iter()
                .find(|f| f.format_id == format_id)
                .map(|f| f.is_audio())
                .unwrap_or(false);
            println!(
                "{}",
                format!(
                    "SUCCESS: Selected Format ID: {}. Download sequence will start here.",
                    format_id
                )
                .green()
            );
            (format_id, is_aud)
        } else {
            println!("{}", "WARNING: Download cancelled by user.".yellow());
            return None;
        }
    } else if let Some(format_id) = find_format_by_quality(&info.formats, &config.default_quality) {
        let is_aud = info
            .formats
            .iter()
            .find(|f| f.format_id == format_id)
            .map(|f| f.is_audio())
            .unwrap_or(false);
        println!(
            "{}",
            format!(
                "INFO: Exact quality ({}) found! Auto-starting download...",
                config.default_quality
            )
            .green()
        );
        (format_id, is_aud)
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
            let is_aud = info
                .formats
                .iter()
                .find(|f| f.format_id == format_id)
                .map(|f| f.is_audio())
                .unwrap_or(false);
            println!(
                "{}",
                format!(
                    "SUCCESS: Selected Format ID: {}. Download sequence will start here.",
                    format_id
                )
                .green()
            );
            (format_id, is_aud)
        } else {
            println!("{}", "WARNING: Download cancelled by user.".yellow());
            return None;
        }
    };

    let format_id = if is_audio {
        format!("audio:{}", chosen_id)
    } else {
        chosen_id
    };

    loop {
        match crate::config::manager::validate_download_path(&config.download_path, true) {
            Ok(valid_dir) => {
                config.download_path = valid_dir.to_string_lossy().to_string();
                break;
            }
            Err(e) => {
                println!(
                    "{}",
                    format!(
                        "\nWARNING: Invalid download path: {}\nOpening file browser...",
                        e
                    )
                    .yellow()
                );
                if let Some(folder) = FileDialog::new()
                    .set_title("Select Download Folder")
                    .pick_folder()
                {
                    match crate::config::manager::validate_download_path(
                        &folder.to_string_lossy(),
                        true,
                    ) {
                        Ok(valid_dir) => {
                            config.download_path = valid_dir.to_string_lossy().to_string();
                            if let Err(save_err) = ConfigManager::save(config) {
                                println!(
                                    "{}",
                                    format!(
                                        "\nERROR: Failed to save download path: {}\n",
                                        save_err
                                    )
                                    .red()
                                );
                            }
                            break;
                        }
                        Err(val_err) => {
                            println!(
                                "{}",
                                format!("\nWARNING: Selected folder is invalid: {}\n", val_err)
                                    .yellow()
                            );
                        }
                    }
                } else {
                    println!(
                        "{}",
                        "\nERROR: No valid download folder selected. Download cancelled.\n"
                            .red()
                            .bold()
                    );
                    return None;
                }
            }
        }
    }

    Some((info.title, format_id, yt_dlp_path, ffmpeg_path))
}

/// Single-URL direct execution entry point for non-interactive CLI usage.
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
            Some(&title),
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

fn read_line_fallback(prompt: &str) -> Option<String> {
    print!("{} ", prompt);
    let _ = io::stdout().flush();
    let mut line = String::new();
    let stdin = io::stdin();
    let mut handle = stdin.lock();
    if handle.read_line(&mut line).unwrap_or(0) == 0 {
        None
    } else {
        Some(line.trim_end_matches(['\r', '\n']).to_string())
    }
}

fn read_shell_input(prompt: &str) -> Option<String> {
    // Fall back to line-based reading when non-interactive or when inquire encounters a terminal error.
    if io::stdin().is_terminal() {
        match Text::new(prompt).prompt() {
            Ok(input) => Some(input),
            Err(
                inquire::InquireError::OperationCanceled
                | inquire::InquireError::OperationInterrupted,
            ) => None,
            Err(_) => read_line_fallback(prompt),
        }
    } else {
        read_line_fallback(prompt)
    }
}

/// Interactive shell loop managing concurrent background downloads via the FIFO queue.
pub fn start_repl(config: &mut DwdrConfig) {
    println!(
        "{}",
        "INFO: Welcome to DWDR Interactive Shell. Type a URL to download, 'status' for live progress, 'help' for commands, or 'exit' to quit."
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
        match read_shell_input("dwdr >") {
            Some(input) => {
                let input = input.trim();
                if input.is_empty() {
                    continue;
                }
                if input.eq_ignore_ascii_case("help") {
                    println!("\n{}", "Available Commands:".bold());
                    println!("  <URL>             - Download media stream (managed via queue)");
                    println!(
                        "  status            - Live real-time progress monitor of active downloads"
                    );
                    println!(
                        "  list              - Session report of all downloads (active, completed, queued)"
                    );
                    println!(
                        "  queue [live]      - Inspect pending queue (use 'queue live' for real-time view)"
                    );
                    println!("  active            - View currently active downloads snapshot");
                    println!("  cancel            - Cancel all active downloads and clear queue");
                    println!(
                        "  config / settings - Configure download path, quality, and parallel limits"
                    );
                    println!("  cls / clear       - Clear terminal screen");
                    println!("  exit / quit       - Cancel tasks and exit shell\n");
                    continue;
                }
                if input.eq_ignore_ascii_case("status") {
                    queue_manager.lock().unwrap().print_status_live();
                    continue;
                }
                if input.eq_ignore_ascii_case("list") {
                    queue_manager.lock().unwrap().print_list();
                    continue;
                }
                if input.eq_ignore_ascii_case("queue") {
                    queue_manager.lock().unwrap().print_queue();
                    continue;
                }
                if input.eq_ignore_ascii_case("queue live")
                    || input.eq_ignore_ascii_case("queue -l")
                {
                    queue_manager.lock().unwrap().print_queue_live();
                    continue;
                }
                if input.eq_ignore_ascii_case("active") {
                    queue_manager.lock().unwrap().print_status();
                    continue;
                }
                if input.eq_ignore_ascii_case("cancel") {
                    QueueManager::cancel_and_join(&queue_manager);
                    println!(
                        "{}",
                        "INFO: Cancelled all active downloads and cleared queue.".yellow()
                    );
                    println!(
                        "{}",
                        "Tip: Enter a new media URL to begin downloading or 'status' to verify idle state.".cyan()
                    );
                    continue;
                }
                if input.eq_ignore_ascii_case("config") || input.eq_ignore_ascii_case("settings") {
                    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
                        println!(
                            "{}",
                            "\nERROR: Interactive configuration requires an interactive terminal (TTY).\nTip: In scripts or headless environments, use 'dwdr config set <key> <value>' instead.\n     Type 'help' to see all available commands.\n"
                                .red()
                        );
                        continue;
                    }
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
                    QueueManager::cancel_and_join(&queue_manager);
                    println!("{}", "INFO: Thanks for using DWDR!".green());
                    break;
                }

                // Distinguish unknown shell commands from media URLs to guide users to 'help' rather than reporting "Invalid URL".
                if !is_valid_url(input) {
                    println!(
                        "{}",
                        format!(
                            "\nERROR: Unrecognized command: \"{}\". Type a valid media URL, or 'help' for available commands.\nTip: URLs must begin with http:// or https://\n",
                            input
                        )
                        .red()
                        .bold()
                    );
                    continue;
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
                                    "INFO: Download started for: \"{}\" [Active: {}/{}, Queued: {}] (Type 'status' for real-time progress)",
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
            None => {
                QueueManager::cancel_and_join(&queue_manager);
                println!("{}", "INFO: Thanks for using DWDR!".green());
                break;
            }
        }
    }
}
