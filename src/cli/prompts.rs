use std::fmt::Display;
use std::io::{IsTerminal, Write, stdout};

use colored::Colorize;
use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    style::{Color, Print, ResetColor, SetForegroundColor},
    terminal::{self, Clear, ClearType},
};
use rfd::FileDialog;

use crate::config::manager::{
    ConfigManager, DwdrConfig, normalize_cookie_value, validate_download_path,
};
use crate::engine::downloader::MediaFormat;
use crate::ui::UiContext;
use crate::ui::format_table::{
    build_format_table_options, find_recommended_index, format_table_header,
};

/// Interactive raw-mode terminal selector with clamped index navigation.
pub struct Select<T: Display> {
    message: String,
    options: Vec<T>,
    initial_index: usize,
}

impl<T: Display> Select<T> {
    pub fn new(message: &str, options: Vec<T>) -> Self {
        Self {
            message: message.to_string(),
            options,
            initial_index: 0,
        }
    }

    pub fn with_initial_index(mut self, index: usize) -> Self {
        self.initial_index = index;
        self
    }

    #[allow(clippy::result_unit_err)]
    pub fn prompt_index(&self) -> Result<usize, ()> {
        if self.options.is_empty() {
            return Err(());
        }

        // Prevent hanging or unintended default selections in non-interactive/piped environments.
        if !std::io::stdin().is_terminal() || !stdout().is_terminal() {
            return Err(());
        }

        let count = self.options.len();
        let mut selected = self.initial_index.min(count.saturating_sub(1));
        let mut stdout = stdout();

        terminal::enable_raw_mode().map_err(|_| ())?;
        let _ = execute!(stdout, cursor::Hide);

        let _ = execute!(
            stdout,
            SetForegroundColor(Color::Cyan),
            Print("? "),
            Print(&self.message),
            Print("\r\n"),
            ResetColor
        );

        let term_height = terminal::size().map(|(_, h)| h as usize).unwrap_or(24);
        let page_size = term_height.saturating_sub(4).clamp(3, 10);
        let window_size = std::cmp::min(count, page_size);

        let render =
            |stdout: &mut std::io::Stdout, selected: usize, first: bool| -> Result<(), ()> {
                if !first {
                    let _ = execute!(stdout, cursor::MoveUp(window_size as u16));
                }

                let half = window_size / 2;
                let start = if selected <= half {
                    0
                } else if selected + (window_size - half) >= count {
                    count.saturating_sub(window_size)
                } else {
                    selected.saturating_sub(half)
                };
                let end = std::cmp::min(start + window_size, count);

                for (idx, opt) in self.options[start..end].iter().enumerate() {
                    let i = start + idx;
                    let _ = execute!(stdout, Clear(ClearType::CurrentLine));
                    if i == selected {
                        let _ = execute!(
                            stdout,
                            SetForegroundColor(Color::Cyan),
                            Print(format!(" > {}\r\n", opt)),
                            ResetColor
                        );
                    } else {
                        let _ = execute!(stdout, Print(format!("   {}\r\n", opt)));
                    }
                }
                stdout.flush().map_err(|_| ())?;
                Ok(())
            };

        if render(&mut stdout, selected, true).is_err() {
            let _ = terminal::disable_raw_mode();
            let _ = execute!(stdout, cursor::Show);
            return Err(());
        }

        let selection_result = loop {
            match event::read() {
                Ok(Event::Key(KeyEvent {
                    code,
                    modifiers,
                    kind,
                    ..
                })) => {
                    if kind == KeyEventKind::Release {
                        continue;
                    }
                    // Non-wrapping navigation bounds cursor to top/bottom to prevent accidental cycle-around selections.
                    match code {
                        KeyCode::Up | KeyCode::Char('k') => {
                            if selected > 0 {
                                selected -= 1;
                                let _ = render(&mut stdout, selected, false);
                            }
                        }
                        KeyCode::Down | KeyCode::Char('j') => {
                            if selected + 1 < count {
                                selected += 1;
                                let _ = render(&mut stdout, selected, false);
                            }
                        }
                        KeyCode::Home => {
                            if selected != 0 {
                                selected = 0;
                                let _ = render(&mut stdout, selected, false);
                            }
                        }
                        KeyCode::End => {
                            if selected != count - 1 {
                                selected = count - 1;
                                let _ = render(&mut stdout, selected, false);
                            }
                        }
                        KeyCode::Enter => {
                            break Ok(selected);
                        }
                        KeyCode::Esc => {
                            break Err(());
                        }
                        KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => {
                            break Err(());
                        }
                        _ => {}
                    }
                }
                Err(_) => {
                    break Err(());
                }
                _ => {}
            }
        };

        let _ = execute!(stdout, cursor::MoveUp(window_size as u16));
        for _ in 0..window_size {
            let _ = execute!(stdout, Clear(ClearType::CurrentLine), cursor::MoveDown(1));
        }
        let _ = execute!(stdout, cursor::MoveUp(window_size as u16));

        let _ = execute!(stdout, cursor::Show);
        let _ = terminal::disable_raw_mode();

        selection_result
    }

    #[allow(clippy::result_unit_err)]
    pub fn prompt(self) -> Result<T, ()> {
        let idx = self.prompt_index()?;
        self.options.into_iter().nth(idx).ok_or(())
    }
}

pub fn select_format(formats: &[MediaFormat]) -> Option<String> {
    select_format_ext(formats, "best")
}

pub fn select_format_ext(formats: &[MediaFormat], default_quality: &str) -> Option<String> {
    if formats.is_empty() {
        return None;
    }

    if !std::io::stdin().is_terminal() || !stdout().is_terminal() {
        let first = &formats[0];
        println!(
            "{}",
            format!(
                "INFO: Non-interactive terminal detected. Auto-selecting format: {} ({})",
                first.resolution, first.format_id
            )
            .cyan()
        );
        return Some(first.format_id.clone());
    }

    let ctx = UiContext::get();
    let options = build_format_table_options(
        formats,
        default_quality,
        ctx.term_width,
        &ctx.theme,
        ctx.color_level,
    );
    let rec_idx = find_recommended_index(formats, default_quality);

    let message = if let Some(header) = format_table_header(ctx.term_width, ctx.color_level) {
        format!(
            "Select a format to download (Use arrow keys):\r\n   {}",
            header
        )
    } else {
        "Select a format to download (Use arrow keys):".to_string()
    };

    match Select::new(&message, options)
        .with_initial_index(rec_idx)
        .prompt_index()
    {
        Ok(index) => Some(formats[index].format_id.clone()),
        Err(_) => None,
    }
}

pub fn prompt_folder_path(default_path: &str) -> Option<std::path::PathBuf> {
    #[cfg(unix)]
    let has_gui = std::env::var("DISPLAY").is_ok() || std::env::var("WAYLAND_DISPLAY").is_ok();
    #[cfg(not(unix))]
    let has_gui = true;

    if has_gui {
        println!("{}", "INFO: Opening native file browser...".cyan());
        if let Some(path) = FileDialog::new()
            .set_title("Select Download Folder")
            .pick_folder()
        {
            return Some(path);
        }
    }

    println!(
        "{}",
        "INFO: Native file dialog unavailable or dismissed. Enter directory path manually:".cyan()
    );
    match inquire::Text::new("Download directory path:")
        .with_default(default_path)
        .prompt()
    {
        Ok(typed) if !typed.trim().is_empty() => Some(std::path::PathBuf::from(typed.trim())),
        _ => None,
    }
}

pub fn interactive_config(config: &mut DwdrConfig) {
    if !std::io::stdin().is_terminal() || !stdout().is_terminal() {
        println!(
            "{}",
            "\nERROR: Interactive configuration requires an interactive terminal (TTY).\nUse 'dwdr config set <key> <value>' for non-interactive configuration.\n"
                .red()
        );
        return;
    }

    let mut working = config.clone();
    let mut cursor_idx: usize = 0;

    loop {
        let options = vec![
            format!("1. Download Path (Current: {})", working.download_path),
            format!("2. Default Quality (Current: {})", working.default_quality),
            format!(
                "3. Max Parallel Downloads (Current: {})",
                working.max_parallel_downloads
            ),
            format!(
                "4. Compatibility Mode (Current: {})",
                if working.compat_mode {
                    "Enabled (AVC+AAC)"
                } else {
                    "Disabled (Max Quality)"
                }
            ),
            format!(
                "5. UI Animations (Current: {})",
                if working.animations {
                    "Enabled"
                } else {
                    "Disabled"
                }
            ),
            format!(
                "6. Auto-watch Downloads (Current: {})",
                if working.auto_watch {
                    "Enabled"
                } else {
                    "Disabled"
                }
            ),
            format!(
                "7. Terminal Bell (Current: {})",
                if working.bell { "Enabled" } else { "Disabled" }
            ),
            format!("8. UI Theme (Current: {})", working.theme),
            format!(
                "9. Cookies / Browser Auth (Current: {})",
                working.cookies.as_deref().unwrap_or("None (Disabled)")
            ),
            "10. Save & Exit".to_string(),
            "11. Cancel".to_string(),
        ];

        let choice = Select::new("Configuration Menu (Use arrow keys):", options)
            .with_initial_index(cursor_idx)
            .prompt();

        match choice {
            Ok(ans) => {
                if ans.starts_with("1.") {
                    cursor_idx = 0;
                    if let Some(path) = prompt_folder_path(&working.download_path) {
                        let path_str = path.to_string_lossy();
                        match validate_download_path(&path_str, true) {
                            Ok(valid_dir) => {
                                working.download_path = valid_dir.to_string_lossy().to_string();
                                println!("{}", "SUCCESS: Valid download path selected.".green());
                            }
                            Err(e) => {
                                println!(
                                    "{}",
                                    format!("\nERROR: Invalid download path: {}\n", e).red()
                                );
                            }
                        }
                    }
                } else if ans.starts_with("2.") {
                    cursor_idx = 1;
                    let quality_opts = vec![
                        "always ask",
                        "best",
                        "8k",
                        "4k",
                        "2k",
                        "1080p",
                        "720p",
                        "480p",
                        "360p",
                        "240p",
                        "144p",
                        "audio",
                    ];
                    if let Ok(quality) =
                        Select::new("Select Default Quality:", quality_opts).prompt()
                    {
                        working.default_quality = quality.to_string();
                    }
                } else if ans.starts_with("3.") {
                    cursor_idx = 2;
                    let parallel_opts: Vec<String> = (1..=16).map(|n| n.to_string()).collect();
                    if let Ok(Ok(parallel)) =
                        Select::new("Select Max Parallel Downloads:", parallel_opts)
                            .prompt()
                            .map(|s| s.parse::<u8>())
                    {
                        working.max_parallel_downloads = parallel;
                    }
                } else if ans.starts_with("4.") {
                    cursor_idx = 3;
                    let mode_opts = vec![
                        "Enabled (AVC/H.264 + AAC for older players/QuickTime)",
                        "Disabled (Default: prioritize AV1/VP9 for highest fidelity)",
                    ];
                    if let Ok(choice) =
                        Select::new("Select Compatibility Mode:", mode_opts).prompt()
                    {
                        working.compat_mode = choice.starts_with("Enabled");
                    }
                } else if ans.starts_with("5.") {
                    cursor_idx = 4;
                    let anim_opts = vec!["Enabled", "Disabled"];
                    if let Ok(choice) = Select::new("Select UI Animations:", anim_opts).prompt() {
                        working.animations = choice == "Enabled";
                    }
                } else if ans.starts_with("6.") {
                    cursor_idx = 5;
                    let watch_opts = vec!["Enabled", "Disabled"];
                    if let Ok(choice) =
                        Select::new("Select Auto-watch Downloads:", watch_opts).prompt()
                    {
                        working.auto_watch = choice == "Enabled";
                    }
                } else if ans.starts_with("7.") {
                    cursor_idx = 6;
                    let bell_opts = vec!["Enabled", "Disabled"];
                    if let Ok(choice) =
                        Select::new("Select Terminal Bell on completion:", bell_opts).prompt()
                    {
                        working.bell = choice == "Enabled";
                    }
                } else if ans.starts_with("8.") {
                    cursor_idx = 7;
                    let theme_opts = vec!["auto", "unicode", "ascii"];
                    if let Ok(choice) = Select::new("Select UI Theme Mode:", theme_opts).prompt() {
                        working.theme = choice.to_string();
                    }
                } else if ans.starts_with("9.") {
                    cursor_idx = 8;
                    println!(
                        "{}",
                        "\nINFO: Setting your primary browser connects your active login session to DWDR.\n      This prevents bot verification and 'Sign in' challenges across platforms\n      and allows downloading private or follower-only media on Instagram, Facebook, and X.\n"
                            .cyan()
                    );
                    let cookie_opts = vec![
                        "None (Disabled)",
                        "Brave",
                        "Chrome",
                        "Edge",
                        "Firefox",
                        "Opera",
                        "Vivaldi",
                        "Chromium",
                        "Safari",
                        "Auto-detect installed browser",
                        "Custom cookies file or profile path...",
                    ];
                    if let Ok(choice) = Select::new(
                        "Select Browser for Cookies & Anti-Bot Authentication:",
                        cookie_opts,
                    )
                    .prompt()
                    {
                        match choice {
                            "None (Disabled)" => working.cookies = None,
                            "Brave" => working.cookies = Some("brave".to_string()),
                            "Chrome" => working.cookies = Some("chrome".to_string()),
                            "Edge" => working.cookies = Some("edge".to_string()),
                            "Firefox" => working.cookies = Some("firefox".to_string()),
                            "Opera" => working.cookies = Some("opera".to_string()),
                            "Vivaldi" => working.cookies = Some("vivaldi".to_string()),
                            "Chromium" => working.cookies = Some("chromium".to_string()),
                            "Safari" => working.cookies = Some("safari".to_string()),
                            "Auto-detect installed browser" => {
                                working.cookies = Some("auto".to_string())
                            }
                            "Custom cookies file or profile path..." => {
                                if let Ok(path) =
                                    inquire::Text::new("Path to cookies.txt or browser profile:")
                                        .prompt()
                                {
                                    match normalize_cookie_value(&path) {
                                        Ok(normalized) => working.cookies = normalized,
                                        Err(suggestion) => {
                                            println!(
                                                "{}",
                                                format!(
                                                    "\nERROR: Unknown browser or cookie source '{}'.",
                                                    path.trim()
                                                )
                                                .red()
                                            );
                                            if let Some(closest) = suggestion {
                                                println!(
                                                    "{}",
                                                    format!("Tip: Did you mean '{}'?", closest)
                                                        .yellow()
                                                );
                                            }
                                        }
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                } else if ans.starts_with("10.") {
                    *config = working;
                    if let Err(e) = ConfigManager::save(config) {
                        println!(
                            "{}",
                            format!("\nERROR: Failed to save settings: {}\n", e).red()
                        );
                    } else {
                        println!("{}", "SUCCESS: Settings saved successfully!".green());
                    }
                    break;
                } else if ans.starts_with("11.") {
                    println!("{}", "WARNING: Settings update cancelled.".yellow());
                    break;
                }
            }
            Err(_) => {
                println!("{}", "WARNING: Settings update cancelled.".yellow());
                break;
            }
        }
    }
}
