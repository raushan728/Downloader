use std::io::{self, BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use colored::Colorize;
use inquire::{Confirm, Text};

use crate::cli::prompts::{interactive_config, prompt_folder_path, select_format_ext};
use crate::config::manager::{ConfigManager, DwdrConfig, validate_download_path};
use crate::engine::downloader::{
    detect_available_browser, download_media_full, fetch_video_info_full, find_existing_media_file,
    find_format_with_fallback, is_auth_or_bot_error, is_rate_limit_error,
    resolve_unique_output_template,
};
use crate::engine::paths::{get_ffmpeg_path, get_ytdlp_path};
use crate::engine::queue::{
    EnqueueResult, QueueManager, flush_notifications, set_global_queue, set_in_interactive_prompt,
};
use crate::engine::updater::{check_core_engines_update, ensure_dependencies};
use crate::ui::UiContext;
use crate::ui::banner::render_banner;
use crate::ui::card::{render_completion_card, render_error_card};
use crate::ui::spinner::Spinner;
use crate::ui::terminal_title::{clear_taskbar_progress, set_terminal_title, terminal_bell};
use crate::ui::text::sanitize_display;
use crate::utils::validator::is_valid_url;

/// Resets terminal raw mode and restores cursor and colors to prevent leaked terminal styling.
pub fn reset_terminal_state() {
    let _ = crossterm::terminal::disable_raw_mode();
    if std::io::stdout().is_terminal() {
        let _ = crossterm::execute!(
            std::io::stdout(),
            crossterm::style::ResetColor,
            crossterm::cursor::Show
        );
        clear_taskbar_progress();
        set_terminal_title("");
        let _ = std::io::stdout().flush();
    }
    if std::io::stderr().is_terminal() {
        let _ = crossterm::execute!(std::io::stderr(), crossterm::style::ResetColor);
        let _ = std::io::stderr().flush();
    }
}

pub const ENABLE_QUICK_EDIT_MODE: u32 = 0x0040;
pub const ENABLE_EXTENDED_FLAGS: u32 = 0x0080;

/// Computes Windows console input mode flags ensuring QuickEdit mode and extended flags remain active.
pub fn compute_windows_console_mode(current_mode: u32) -> u32 {
    current_mode | ENABLE_QUICK_EDIT_MODE | ENABLE_EXTENDED_FLAGS
}

/// Sets Windows console input and output code pages to UTF-8 (65001) and ensures QuickEdit mode is active.
#[cfg(target_os = "windows")]
pub fn init_windows_console() {
    windows_console::init();
}

/// Restores previous Windows console code pages and input mode.
#[cfg(target_os = "windows")]
pub fn restore_windows_console() {
    windows_console::restore();
}

#[cfg(target_os = "windows")]
mod windows_console {
    use super::{ENABLE_EXTENDED_FLAGS, compute_windows_console_mode};

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn SetConsoleOutputCP(wCodePageID: u32) -> i32;
        fn SetConsoleCP(wCodePageID: u32) -> i32;
        fn GetConsoleOutputCP() -> u32;
        fn GetConsoleCP() -> u32;
        fn GetStdHandle(nStdHandle: u32) -> *mut std::ffi::c_void;
        fn GetConsoleMode(hConsoleHandle: *mut std::ffi::c_void, lpMode: *mut u32) -> i32;
        fn SetConsoleMode(hConsoleHandle: *mut std::ffi::c_void, dwMode: u32) -> i32;
    }

    const STD_INPUT_HANDLE: u32 = 0xFFFFFFF6;
    const CP_UTF8: u32 = 65001;

    static PREV_IN_MODE: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(u32::MAX);
    static PREV_OUT_CP: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    static PREV_IN_CP: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

    pub fn init() {
        // SAFETY: Invokes standard Win32 kernel32 console APIs with valid stack pointers
        // and checks handles against NULL and INVALID_HANDLE_VALUE (-1) before modification.
        unsafe {
            if PREV_OUT_CP.load(std::sync::atomic::Ordering::Acquire) == 0 {
                let out_cp = GetConsoleOutputCP();
                if out_cp != CP_UTF8 && SetConsoleOutputCP(CP_UTF8) != 0 {
                    PREV_OUT_CP.store(out_cp, std::sync::atomic::Ordering::Release);
                }
            }
            if PREV_IN_CP.load(std::sync::atomic::Ordering::Acquire) == 0 {
                let in_cp = GetConsoleCP();
                if in_cp != CP_UTF8 && SetConsoleCP(CP_UTF8) != 0 {
                    PREV_IN_CP.store(in_cp, std::sync::atomic::Ordering::Release);
                }
            }

            if PREV_IN_MODE.load(std::sync::atomic::Ordering::Acquire) == u32::MAX {
                let handle = GetStdHandle(STD_INPUT_HANDLE);
                if !handle.is_null() && handle as isize != -1 {
                    let mut mode: u32 = 0;
                    if GetConsoleMode(handle, &mut mode) != 0 {
                        PREV_IN_MODE.store(mode, std::sync::atomic::Ordering::Release);
                        // Ensure QuickEdit mode and extended flags remain enabled so right-click and clipboard paste work in interactive prompts.
                        let new_mode = compute_windows_console_mode(mode);
                        SetConsoleMode(handle, new_mode);
                    }
                }
            }
        }
    }

    pub fn restore() {
        // SAFETY: Invokes Win32 console APIs to restore captured code pages and mode flags.
        unsafe {
            let prev_mode = PREV_IN_MODE.swap(u32::MAX, std::sync::atomic::Ordering::AcqRel);
            if prev_mode != u32::MAX {
                let handle = GetStdHandle(STD_INPUT_HANDLE);
                if !handle.is_null() && handle as isize != -1 {
                    SetConsoleMode(handle, prev_mode | ENABLE_EXTENDED_FLAGS);
                }
            }
            let prev_out_cp = PREV_OUT_CP.swap(0, std::sync::atomic::Ordering::AcqRel);
            if prev_out_cp != 0 {
                SetConsoleOutputCP(prev_out_cp);
            }
            let prev_in_cp = PREV_IN_CP.swap(0, std::sync::atomic::Ordering::AcqRel);
            if prev_in_cp != 0 {
                SetConsoleCP(prev_in_cp);
            }
        }
    }
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
    pub cookies: Option<String>,
}

const AUTH_BOT_COOKIE_TIP: &str = "\
Tip: Platform authentication, bot verification, or private/follower-only access required (Instagram, YouTube, X/Twitter, Facebook, etc.).
     If you have access or follow this account in your browser, connect cookies in DWDR:
       dwdr config set cookies brave    (or chrome, edge, firefox, opera, vivaldi, auto)
     Or in interactive shell:    type 'config' -> '9. Cookies / Browser Auth'
     Or use a cookies file:      dwdr config set cookies /path/to/cookies.txt
     Note: For private media, your logged-in browser account must follow the creator and not be blocked.
     Ensure you are logged in, then retry.";

fn print_engine_failure_hints(err: &str, yt_dlp_path: &Path, ffmpeg_path: &Path) {
    if is_auth_or_bot_error(err) {
        eprintln!("{}", AUTH_BOT_COOKIE_TIP.yellow());
    }
    if let Some(msg) = check_core_engines_update(yt_dlp_path, ffmpeg_path) {
        eprintln!("{}\n", msg.yellow());
    }
}

/// Resolves format, destination directory, and media metadata for a URL.
pub fn resolve_media_to_download(
    url: &str,
    config: &mut DwdrConfig,
    force: bool,
) -> Option<ResolvedMedia> {
    if !is_valid_url(url) {
        eprintln!(
            "{}",
            "\nERROR: Invalid URL provided. It must start with http:// or https://\n"
                .red()
                .bold()
        );
        return None;
    }

    let ctx = UiContext::get();
    let yt_dlp_path = get_ytdlp_path();
    let ffmpeg_path = get_ffmpeg_path();

    if let Err(e) = ensure_dependencies() {
        eprintln!(
            "{}",
            format!(
                "\nERROR: Failed to setup engine: {}\nTip: Check your connection or run 'dwdr update' to retry.\n",
                e
            )
                .red()
                .bold()
        );
        return None;
    }

    let fetch_spinner = if !ctx.quiet {
        Some(Spinner::new(
            "Fetching media details from the server...",
            ctx.theme,
            ctx.color_level,
            ctx.animations,
        ))
    } else {
        None
    };

    let (info, used_cookies) = match fetch_video_info_full(
        url,
        &yt_dlp_path,
        config.compat_mode,
        None,
    ) {
        Ok(info) => {
            if let Some(ref sp) = fetch_spinner {
                sp.finish_info(&format!(
                    "Successfully fetched data for: {}",
                    sanitize_display(&info.title)
                ));
            }
            if !ctx.quiet {
                println!(
                    "{}",
                    format!("Extracted {} formats.", info.formats.len()).green()
                );
            }
            (info, None)
        }
        Err(e) => {
            let is_rate_limit = is_rate_limit_error(&e);
            let has_effective_cookies = match config.cookies.as_deref() {
                Some(c) if c.eq_ignore_ascii_case("auto") => detect_available_browser().is_some(),
                Some(_) => true,
                None => false,
            };
            let should_cookie_retry =
                (is_auth_or_bot_error(&e) || is_rate_limit) && has_effective_cookies;

            if should_cookie_retry {
                if let Some(ref sp) = fetch_spinner {
                    sp.finish_info(if is_rate_limit {
                        "Rate limit encountered on anonymous stream"
                    } else {
                        "Authentication required for this media stream"
                    });
                }
                drop(fetch_spinner);

                let cookie_source = config.cookies.as_deref().unwrap_or("configured browser");
                if is_rate_limit {
                    eprintln!(
                        "{}",
                        format!(
                            "WARNING: Platform rate limit reached ({}).\n         Backing off briefly and retrying with session cookies ('{}')...",
                            sanitize_display(&e),
                            cookie_source
                        )
                        .yellow()
                    );
                    std::thread::sleep(Duration::from_millis(1500));
                } else {
                    println!(
                        "{}",
                        format!(
                            "INFO: Protected media or bot challenge detected ({}).\n      Retrying with session cookies ('{}')...",
                            sanitize_display(&e),
                            cookie_source
                        )
                        .cyan()
                    );
                }

                let retry_spinner = if !ctx.quiet {
                    Some(Spinner::new(
                        "Fetching media details using session cookies...",
                        ctx.theme,
                        ctx.color_level,
                        ctx.animations,
                    ))
                } else {
                    None
                };

                match fetch_video_info_full(
                    url,
                    &yt_dlp_path,
                    config.compat_mode,
                    config.cookies.as_deref(),
                ) {
                    Ok(retry_info) => {
                        if let Some(ref sp) = retry_spinner {
                            sp.finish_info(&format!(
                                "Successfully authenticated and fetched data for: {}",
                                sanitize_display(&retry_info.title)
                            ));
                        }
                        if !ctx.quiet {
                            println!(
                                "{}",
                                format!(
                                    "Extracted {} formats with session cookies.",
                                    retry_info.formats.len()
                                )
                                .green()
                            );
                        }
                        (retry_info, config.cookies.clone())
                    }
                    Err(retry_e) => {
                        if let Some(ref sp) = retry_spinner {
                            sp.finish_error(&format!("Authentication retry failed: {}", retry_e));
                        } else {
                            eprintln!(
                                "{}",
                                format!(
                                    "\nERROR: Failed to extract video info with cookies: {}\n",
                                    retry_e
                                )
                                .red()
                            );
                        }
                        print_engine_failure_hints(&retry_e, &yt_dlp_path, &ffmpeg_path);
                        return None;
                    }
                }
            } else {
                if let Some(ref sp) = fetch_spinner {
                    let err_msg = format!("Failed to extract video info: {}", e);
                    sp.finish_error(if is_rate_limit {
                        "Rate limit encountered"
                    } else {
                        &err_msg
                    });
                } else if is_rate_limit {
                    eprintln!(
                        "{}",
                        "\nWARNING: Platform rate limit reached (HTTP 429 / Too Many Requests).\nTip: Wait before retrying, or configure a browser session to access authenticated limits.\n"
                            .yellow()
                            .bold()
                    );
                } else {
                    eprintln!(
                        "{}",
                        format!("\nERROR: Failed to extract video info: {}\n", e).red()
                    );
                }
                drop(fetch_spinner);
                if config.cookies.as_deref() == Some("auto") && detect_available_browser().is_none()
                {
                    eprintln!(
                        "{}",
                        "INFO: Cookies is set to 'auto', but no supported browser installation was found.\n      Configure a specific browser or cookies file via 'dwdr config set cookies <browser>'."
                            .yellow()
                    );
                }
                print_engine_failure_hints(&e, &yt_dlp_path, &ffmpeg_path);
                return None;
            }
        }
    };

    if info.formats.is_empty() {
        eprintln!(
            "{}",
            "\nERROR: No downloadable formats found for this media stream.\n"
                .red()
                .bold()
        );
        return None;
    }

    let (chosen_id, is_audio, format_desc) = if info.formats.len() == 1 {
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
        if let Some(format_id) = select_format_ext(&info.formats, &config.default_quality) {
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
    } else if let Some((format_id, is_fallback)) =
        find_format_with_fallback(&info.formats, &config.default_quality)
    {
        let (is_aud, desc) = if let Some(f) = info.formats.iter().find(|f| f.format_id == format_id)
        {
            (f.is_audio(), f.summary())
        } else {
            (false, format_id.clone())
        };
        if is_fallback {
            if !ctx.quiet {
                println!(
                    "{}",
                    format!(
                        "INFO: Requested quality ({}) not available. Smart Fallback selected nearest lower tier: {}. Auto-starting download...",
                        config.default_quality, desc
                    )
                    .yellow()
                );
            }
        } else if !ctx.quiet {
            println!(
                "{}",
                format!(
                    "INFO: Exact quality ({}) found: {}. Auto-starting download...",
                    config.default_quality, desc
                )
                .green()
            );
        }
        (format_id, is_aud, desc)
    } else {
        if !ctx.quiet {
            println!("{}", "INFO: Manual selection requested...".cyan());
        }
        if let Some(format_id) = select_format_ext(&info.formats, &config.default_quality) {
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

    let has_audio = !is_audio
        && info
            .formats
            .iter()
            .any(|f| f.format_id == chosen_id && f.acodec != "none" && !f.acodec.is_empty());

    let format_id = if is_audio {
        format!("audio:{}", chosen_id)
    } else if has_audio {
        format!("direct:{}", chosen_id)
    } else {
        format!("{}+bestaudio", chosen_id)
    };

    loop {
        match validate_download_path(&config.download_path, true) {
            Ok(valid_dir) => {
                config.download_path = valid_dir.to_string_lossy().to_string();
                break;
            }
            Err(e) => {
                println!(
                    "{}",
                    format!(
                        "\nWARNING: Invalid download path: {}\nOpening folder selector...",
                        e
                    )
                    .yellow()
                );
                if let Some(folder) = prompt_folder_path(&config.download_path) {
                    match validate_download_path(&folder.to_string_lossy(), true) {
                        Ok(valid_dir) => {
                            config.download_path = valid_dir.to_string_lossy().to_string();
                            if let Err(save_err) = ConfigManager::save(config) {
                                eprintln!(
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
                    eprintln!(
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
        cookies: used_cookies,
    })
}

/// Direct execution entry point for single-URL downloads via CLI arguments.
pub fn process_url(url: &str, config: &mut DwdrConfig, force: bool) -> Result<(), String> {
    let media = match resolve_media_to_download(url, config, force) {
        Some(m) => m,
        None => return Err("Failed to resolve media".to_string()),
    };

    let ctx = UiContext::get();
    if !ctx.quiet {
        println!(
            "{}",
            format!(
                "INFO: Starting download for: \"{}\" [{}]",
                media.title, media.format_desc
            )
            .cyan()
        );
    }
    let download_res = download_media_full(
        url,
        &media.format_id,
        &media.yt_dlp_path,
        &media.ffmpeg_path,
        &config.download_path,
        Some(&media.title),
        media.custom_output_template.as_deref(),
        media.force_overwrite,
        media.cookies.as_deref(),
    );

    let download_res = match download_res {
        Err(e)
            if (is_auth_or_bot_error(&e) || is_rate_limit_error(&e))
                && media.cookies.is_none()
                && config.cookies.is_some() =>
        {
            let cookie_source = config.cookies.as_deref().unwrap_or("configured browser");
            let reason = if is_rate_limit_error(&e) {
                "Rate limit encountered on download stream"
            } else {
                "Media download stream requires authentication"
            };
            println!(
                "{}",
                format!(
                    "INFO: {} ({}).\n      Retrying download with session cookies ('{}')...",
                    reason,
                    sanitize_display(&e),
                    cookie_source
                )
                .cyan()
            );
            if is_rate_limit_error(&e) {
                std::thread::sleep(Duration::from_millis(1500));
            }
            download_media_full(
                url,
                &media.format_id,
                &media.yt_dlp_path,
                &media.ffmpeg_path,
                &config.download_path,
                Some(&media.title),
                media.custom_output_template.as_deref(),
                media.force_overwrite,
                config.cookies.as_deref(),
            )
        }
        other => other,
    };

    match download_res {
        Ok(outcome) => {
            let captured_path = outcome.path;
            let stats = outcome.stats;
            let ctx = UiContext::get();
            if !ctx.is_plain() {
                let size_bytes = if stats.size_bytes > 0 {
                    stats.size_bytes
                } else {
                    captured_path
                        .as_ref()
                        .and_then(|p| std::fs::metadata(p).ok())
                        .map(|m| m.len())
                        .unwrap_or(0)
                };
                let display_path = captured_path
                    .as_ref()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|| config.download_path.clone());
                let card = render_completion_card(
                    &media.title,
                    size_bytes,
                    stats.elapsed,
                    stats.avg_speed,
                    stats.peak_speed,
                    &display_path,
                    &stats.samples,
                    &ctx.theme,
                    ctx.color_level,
                );
                print!("\r\x1b[2K");
                println!("{}", card);
            } else {
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
            if ctx.bell {
                terminal_bell();
            }
            clear_taskbar_progress();
            Ok(())
        }
        Err(e) => {
            let ctx = UiContext::get();
            if !ctx.is_plain() {
                print!("\r\x1b[2K");
                let err_card = render_error_card(&media.title, &e, &ctx.theme, ctx.color_level);
                eprintln!("{}", err_card);
                if let Some(msg) = check_core_engines_update(&media.yt_dlp_path, &media.ffmpeg_path)
                {
                    eprintln!("{}\n", msg.yellow());
                }
            } else {
                eprintln!(
                    "{}",
                    format!("\nERROR: Download failed: {}\n", e).red().bold()
                );
                print_engine_failure_hints(&e, &media.yt_dlp_path, &media.ffmpeg_path);
            }
            clear_taskbar_progress();
            reset_terminal_state();
            Err(e)
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
    set_in_interactive_prompt(true);
    let result = if io::stdin().is_terminal() {
        match Text::new(prompt).prompt() {
            Ok(input) => Some(input),
            Err(inquire::InquireError::OperationCanceled) => {
                // Esc key pressed: cancel current prompt line without quitting shell
                reset_terminal_state();
                Some(String::new())
            }
            Err(inquire::InquireError::OperationInterrupted) => {
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
    };
    set_in_interactive_prompt(false);
    flush_notifications();
    result
}

fn sync_queue_with_config(queue_mgr: &Arc<Mutex<QueueManager>>, config: &DwdrConfig) {
    QueueManager::set_max_parallel(queue_mgr, config.max_parallel_downloads as usize);
    queue_mgr
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .set_cookies(config.cookies.clone());
}

fn warn_if_downloads_active(queue_mgr: &Arc<Mutex<QueueManager>>) -> bool {
    let is_active = queue_mgr
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .is_active();
    if is_active {
        println!(
            "{}",
            "\nWARNING: Configuration changes are disabled while downloads are actively in progress. Please wait for downloads to complete or use 'cancel' first.\n"
                .yellow()
        );
    }
    is_active
}

/// Interactive shell loop managing concurrent background downloads via the FIFO queue.
pub fn start_repl(config: &mut DwdrConfig) {
    let ctx = UiContext::get();
    let is_tty = io::stdout().is_terminal();
    if is_tty && !ctx.quiet {
        let banner = render_banner(&ctx.theme, ctx.color_level, false);
        if !banner.is_empty() {
            println!("{}", banner);
        }
    } else if !ctx.quiet {
        println!(
            "{}",
            "INFO: Welcome to DWDR Interactive Shell. Type a URL to download, 'status' for live progress, 'help' for commands, or 'exit' to quit."
                .cyan()
        );
    }

    let yt_dlp_path = get_ytdlp_path();
    let ffmpeg_path = get_ffmpeg_path();
    let queue_manager = QueueManager::new(
        config.max_parallel_downloads as usize,
        yt_dlp_path,
        ffmpeg_path,
    );
    sync_queue_with_config(&queue_manager, config);
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
                        "  status [live]     - Live real-time progress monitor of active downloads"
                    );
                    println!(
                        "  list              - Session report of all downloads (active, completed, queued)"
                    );
                    println!(
                        "  queue [live]      - Inspect pending queue (use 'queue live' for real-time view)"
                    );
                    println!("  active            - View currently active downloads snapshot");
                    println!(
                        "  wait              - Wait for all active and queued downloads to finish"
                    );
                    println!("  cancel            - Cancel all active downloads and clear queue");
                    println!(
                        "  config / settings - Configure download path, quality, and parallel limits"
                    );
                    println!("  cls / clear       - Clear terminal screen");
                    println!("  exit / quit       - Cancel tasks and exit shell\n");
                    continue;
                }
                if input.eq_ignore_ascii_case("status")
                    || input.eq_ignore_ascii_case("status live")
                    || input.eq_ignore_ascii_case("status -l")
                {
                    QueueManager::print_status_live(&queue_manager);
                    continue;
                }
                if input.eq_ignore_ascii_case("list") {
                    queue_manager
                        .lock()
                        .unwrap_or_else(|p| p.into_inner())
                        .print_list();
                    continue;
                }
                if input.eq_ignore_ascii_case("queue") {
                    queue_manager
                        .lock()
                        .unwrap_or_else(|p| p.into_inner())
                        .print_queue();
                    continue;
                }
                if input.eq_ignore_ascii_case("queue live")
                    || input.eq_ignore_ascii_case("queue -l")
                {
                    QueueManager::print_queue_live(&queue_manager);
                    continue;
                }
                if input.eq_ignore_ascii_case("active") {
                    queue_manager
                        .lock()
                        .unwrap_or_else(|p| p.into_inner())
                        .print_status();
                    continue;
                }
                if input.eq_ignore_ascii_case("wait") {
                    println!(
                        "{}",
                        "INFO: Waiting for all queued and active downloads to complete...".cyan()
                    );
                    QueueManager::join_workers(&queue_manager);
                    println!("{}", "SUCCESS: All downloads completed.".green());
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
                    if warn_if_downloads_active(&queue_manager) {
                        continue;
                    }
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
                    sync_queue_with_config(&queue_manager, config);
                    continue;
                }
                let is_config_cmd = input.to_ascii_lowercase().starts_with("config ")
                    || input.to_ascii_lowercase().starts_with("settings ");
                if is_config_cmd {
                    if warn_if_downloads_active(&queue_manager) {
                        continue;
                    }
                    let parts: Vec<&str> = input.split_whitespace().collect();
                    let action = parts.get(1).map(|s| s.to_ascii_lowercase());
                    let key = parts.get(2).map(|s| s.to_ascii_lowercase());
                    let value = if parts.len() > 3 {
                        Some(
                            parts[3..]
                                .join(" ")
                                .trim_matches(|c| c == '"' || c == '\'')
                                .to_string(),
                        )
                    } else {
                        None
                    };
                    ConfigManager::handle_cli(config, action, key, value, false);
                    sync_queue_with_config(&queue_manager, config);
                    continue;
                }
                if input.eq_ignore_ascii_case("cls") || input.eq_ignore_ascii_case("clear") {
                    print!("{}[2J{}[1;1H", 27 as char, 27 as char);
                    let _ = io::stdout().flush();
                    continue;
                }
                if input.eq_ignore_ascii_case("exit") || input.eq_ignore_ascii_case("quit") {
                    if queue_manager
                        .lock()
                        .unwrap_or_else(|p| p.into_inner())
                        .is_active()
                        && io::stdin().is_terminal()
                    {
                        match Confirm::new(
                            "Active downloads are running. Are you sure you want to cancel them and exit?",
                        )
                        .with_default(false)
                        .prompt()
                        {
                            Ok(true) => {}
                            _ => {
                                println!(
                                    "{}",
                                    "INFO: Exit cancelled. Active downloads continue running."
                                        .cyan()
                                );
                                continue;
                            }
                        }
                    }
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
                            sanitize_display(input)
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
                        media.cookies,
                    );
                    let max_p = queue_manager
                        .lock()
                        .unwrap_or_else(|p| p.into_inner())
                        .max_parallel;
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

                    let is_first_active = matches!(
                        result,
                        EnqueueResult::Started {
                            active_count: 1,
                            ..
                        }
                    );

                    if config.auto_watch
                        && is_first_active
                        && io::stdin().is_terminal()
                        && io::stdout().is_terminal()
                    {
                        QueueManager::print_status_live(&queue_manager);
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
    #[cfg(target_os = "windows")]
    restore_windows_console();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_windows_console_mode_enables_quickedit_and_extended_flags() {
        let initial_mode = 0;
        let mode = compute_windows_console_mode(initial_mode);
        assert_ne!(mode & ENABLE_QUICK_EDIT_MODE, 0);
        assert_ne!(mode & ENABLE_EXTENDED_FLAGS, 0);

        let with_other_flags = 0x0001;
        let mode2 = compute_windows_console_mode(with_other_flags);
        assert_ne!(mode2 & 0x0001, 0);
        assert_ne!(mode2 & ENABLE_QUICK_EDIT_MODE, 0);
        assert_ne!(mode2 & ENABLE_EXTENDED_FLAGS, 0);
    }
}
