use std::io::{self, BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use colored::Colorize;
use inquire::Text;
use rfd::FileDialog;

use crate::cli::prompts::{interactive_config, select_format};
use crate::config::manager::{ConfigManager, DwdrConfig};
use crate::engine::downloader::{
    download_media_ext, fetch_video_info, find_existing_media_file, find_format_by_quality,
    resolve_unique_output_template,
};
use crate::engine::paths::{get_ffmpeg_path, get_ytdlp_path};
use crate::engine::queue::{EnqueueResult, QueueManager, set_global_queue};
use crate::engine::updater::{check_core_engines_update, ensure_dependencies};
use crate::utils::validator::is_valid_url;

/// Resets terminal raw mode and restores cursor and colors to prevent leaked terminal styling.
pub fn reset_terminal_state() {
    let _ = crossterm::terminal::disable_raw_mode();
    let _ = crossterm::execute!(
        std::io::stdout(),
        crossterm::style::ResetColor,
        crossterm::cursor::Show
    );
    let _ = crossterm::execute!(std::io::stderr(), crossterm::style::ResetColor);
    let _ = std::io::stdout().flush();
    let _ = std::io::stderr().flush();
}

/// Resolved parameters for launching or queueing a media download.
pub struct ResolvedMedia {
    pub title: String,
    pub format_id: String,
    pub format_desc: String,
    pub yt_dlp_path: PathBuf,
    pub ffmpeg_path: PathBuf,
    pub custom_output_template: Option<String>,
    pub force_overwrite: bool,
}

/// Validates the URL, fetches available formats, and resolves the target format and destination directory.
pub fn resolve_media_to_download(
    url: &str,
    config: &mut DwdrConfig,
    force: bool,
) -> Option<ResolvedMedia> {
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
            if let Some(msg) = check_core_engines_update(&yt_dlp_path, &ffmpeg_path) {
                println!("{}\n", msg.yellow());
            }
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

    let (chosen_id, is_audio, format_desc) = if info.formats.len() == 1 {
        // Skip manual prompts when only a single format exists to avoid unnecessary confirmation.
        let single_fmt = &info.formats[0];
        let desc = single_fmt.summary();
        println!(
            "{}",
            format!(
                "INFO: Single available format detected: {}. Auto-starting download...",
                desc
            )
            .green()
        );
        (single_fmt.format_id.clone(), single_fmt.is_audio(), desc)
    } else if config.default_quality == "always ask" {
        println!("{}", "INFO: Manual selection requested...".cyan());
        if let Some(format_id) = select_format(&info.formats) {
            let (is_aud, desc) =
                if let Some(f) = info.formats.iter().find(|f| f.format_id == format_id) {
                    (f.is_audio(), f.summary())
                } else {
                    (false, format_id.clone())
                };
            println!(
                "{}",
                format!(
                    "SUCCESS: Selected format: {} (Format ID: {}). Download sequence will start here.",
                    desc, format_id
                )
                .green()
            );
            (format_id, is_aud, desc)
        } else {
            println!("{}", "WARNING: Download cancelled by user.".yellow());
            return None;
        }
    } else if let Some(format_id) = find_format_by_quality(&info.formats, &config.default_quality) {
        let (is_aud, desc) = if let Some(f) = info.formats.iter().find(|f| f.format_id == format_id)
        {
            (f.is_audio(), f.summary())
        } else {
            (false, config.default_quality.clone())
        };
        println!(
            "{}",
            format!(
                "INFO: Exact quality ({}) found: {}. Auto-starting download...",
                config.default_quality, desc
            )
            .green()
        );
        (format_id, is_aud, desc)
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
            let (is_aud, desc) =
                if let Some(f) = info.formats.iter().find(|f| f.format_id == format_id) {
                    (f.is_audio(), f.summary())
                } else {
                    (false, format_id.clone())
                };
            println!(
                "{}",
                format!(
                    "SUCCESS: Selected format: {} (Format ID: {}). Download sequence will start here.",
                    desc, format_id
                )
                .green()
            );
            (format_id, is_aud, desc)
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

    let download_dir = Path::new(&config.download_path);
    let mut custom_output_template: Option<String> = None;
    let mut force_overwrite = force;

    if !force
        && let Some(existing_file) = find_existing_media_file(download_dir, &info.title, is_audio)
    {
        let file_name = existing_file
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("media file");

        println!(
            "{}",
            format!(
                "\nINFO: Media file already exists in download directory: \"{}\"",
                file_name
            )
            .yellow()
        );

        if io::stdin().is_terminal() {
            println!(
                "{}",
                "Tip: Choose to skip, overwrite, or save as a numbered copy.".cyan()
            );
            let choices = vec![
                "Skip download (already available)",
                "Overwrite existing file",
                "Save as a new copy (e.g. filename (1).ext)",
            ];
            match inquire::Select::new("How would you like to proceed?", choices).prompt() {
                Ok("Skip download (already available)") => {
                    println!(
                        "{}",
                        format!(
                            "INFO: Skipping download. File is already available at: {}",
                            existing_file.display()
                        )
                        .cyan()
                    );
                    println!(
                        "{}",
                        "Tip: To download another copy, choose 'Save as a new copy' or use '--force' to overwrite."
                            .yellow()
                    );
                    reset_terminal_state();
                    return None;
                }
                Ok("Overwrite existing file") => {
                    println!(
                        "{}",
                        "INFO: Overwrite confirmed. Existing file will be replaced.".cyan()
                    );
                    force_overwrite = true;
                }
                Ok("Save as a new copy (e.g. filename (1).ext)") => {
                    let (tmpl, idx) =
                        resolve_unique_output_template(download_dir, &info.title, is_audio);
                    println!(
                        "{}",
                        format!(
                            "INFO: Saving as copy #{}: output will use template \"{}\"",
                            idx, tmpl
                        )
                        .cyan()
                    );
                    custom_output_template = Some(tmpl);
                }
                _ => {
                    println!("{}", "WARNING: Download cancelled by user.".yellow());
                    reset_terminal_state();
                    return None;
                }
            }
        } else {
            println!(
                "{}",
                format!(
                    "WARNING: File already exists at {}. Skipping to prevent clobbering in non-interactive mode.\nTip: Use 'dwdr --force <URL>' to overwrite existing files.",
                    existing_file.display()
                )
                .yellow()
            );
            reset_terminal_state();
            return None;
        }
    }

    Some(ResolvedMedia {
        title: info.title,
        format_id,
        format_desc,
        yt_dlp_path,
        ffmpeg_path,
        custom_output_template,
        force_overwrite,
    })
}

/// Single-URL direct execution entry point for non-interactive CLI usage.
pub fn process_url(url: &str, config: &mut DwdrConfig, force: bool) {
    if let Some(media) = resolve_media_to_download(url, config, force) {
        println!(
            "{}",
            format!(
                "INFO: Starting download for: \"{}\" [{}]",
                media.title, media.format_desc
            )
            .cyan()
        );
        match download_media_ext(
            url,
            &media.format_id,
            &media.yt_dlp_path,
            &media.ffmpeg_path,
            &config.download_path,
            Some(&media.title),
            media.custom_output_template.as_deref(),
            media.force_overwrite,
        ) {
            Ok(()) => {
                println!(
                    "{}",
                    format!(
                        "SUCCESS: Download completed successfully! File saved: \"{}\" in {}",
                        media.title, config.download_path
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
                if let Some(msg) = check_core_engines_update(&media.yt_dlp_path, &media.ffmpeg_path)
                {
                    println!("{}\n", msg.yellow());
                }
                reset_terminal_state();
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
            ) => {
                reset_terminal_state();
                None
            }
            Err(_) => {
                reset_terminal_state();
                read_line_fallback(prompt)
            }
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
                    QueueManager::print_status_live(&queue_manager);
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
                    QueueManager::print_queue_live(&queue_manager);
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
                    reset_terminal_state();
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
                    reset_terminal_state();
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

                if let Some(media) = resolve_media_to_download(input, config, false) {
                    let result = QueueManager::enqueue_ext(
                        &queue_manager,
                        input.to_string(),
                        media.title.clone(),
                        media.format_id,
                        config.download_path.clone(),
                        media.custom_output_template,
                        media.force_overwrite,
                        Some(media.format_desc.clone()),
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
                                    "INFO: Download started for: \"{}\" [{}] [Active: {}/{}, Queued: {}] (Type 'status' for real-time progress)",
                                    media.title, media.format_desc, active_count, max_p, queued_count
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
                                    "INFO: Added to download queue: \"{}\" [{}] [Active: {}/{}, Queued: {} (Position: #{})]",
                                    media.title, media.format_desc, active_count, max_p, queued_count, position
                                )
                                .yellow()
                            );
                        }
                    }
                }
            }
            None => {
                QueueManager::cancel_and_join(&queue_manager);
                reset_terminal_state();
                println!("{}", "INFO: Thanks for using DWDR!".green());
                break;
            }
        }
    }
}
