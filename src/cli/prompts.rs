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

use crate::config::manager::{ConfigManager, DwdrConfig};
use crate::engine::downloader::MediaFormat;

/// Interactive raw-mode terminal selector with clamped index navigation.
pub struct Select<T: Display> {
    message: String,
    options: Vec<T>,
}

impl<T: Display> Select<T> {
    pub fn new(message: &str, options: Vec<T>) -> Self {
        Self {
            message: message.to_string(),
            options,
        }
    }

    pub fn prompt(self) -> Result<T, ()> {
        if self.options.is_empty() {
            return Err(());
        }

        // Fall back to first option in non-interactive/piped environments where raw mode is unavailable.
        if !std::io::stdin().is_terminal() || !stdout().is_terminal() {
            return Ok(self.options.into_iter().next().unwrap());
        }

        let mut selected = 0;
        let count = self.options.len();
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

        let render =
            |stdout: &mut std::io::Stdout, selected: usize, first: bool| -> Result<(), ()> {
                if !first {
                    let _ = execute!(stdout, cursor::MoveUp(count as u16));
                }
                for (i, opt) in self.options.iter().enumerate() {
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

        let _ = execute!(stdout, cursor::MoveUp(count as u16));
        for _ in 0..count {
            let _ = execute!(stdout, Clear(ClearType::CurrentLine), cursor::MoveDown(1));
        }
        let _ = execute!(stdout, cursor::MoveUp(count as u16));

        let _ = execute!(stdout, cursor::Show);
        let _ = terminal::disable_raw_mode();

        match selection_result {
            Ok(idx) => Ok(self.options.into_iter().nth(idx).unwrap()),
            Err(()) => Err(()),
        }
    }
}

pub fn select_format(formats: &[MediaFormat]) -> Option<String> {
    let mut options = Vec::new();

    for format in formats {
        let tag = if format.resolution == "Audio" {
            "[AUDIO]"
        } else {
            "[VIDEO]"
        };

        let option_str = format!(
            "{:<7} | {:<6} | {:<4} | {}",
            tag,
            format.resolution,
            format.ext,
            format.format_filesize()
        );
        options.push(option_str);
    }

    match Select::new(
        "Select a format to download (Use arrow keys):",
        options.clone(),
    )
    .prompt()
    {
        Ok(choice) => options
            .iter()
            .position(|r| r == &choice)
            .map(|index| formats[index].format_id.clone()),
        Err(_) => None,
    }
}

pub fn interactive_config(config: &mut DwdrConfig) {
    loop {
        let options = vec![
            format!("1. Download Path (Current: {})", config.download_path),
            format!("2. Default Quality (Current: {})", config.default_quality),
            format!(
                "3. Max Parallel Downloads (Current: {})",
                config.max_parallel_downloads
            ),
            "4. Save & Exit".to_string(),
            "5. Cancel".to_string(),
        ];

        let choice = Select::new("Configuration Menu (Use arrow keys):", options).prompt();

        match choice {
            Ok(ans) => {
                if ans.starts_with("1") {
                    println!("{}", "INFO: Opening native file browser...".cyan());
                    if let Some(path) = FileDialog::new()
                        .set_title("Select Download Folder")
                        .pick_folder()
                    {
                        config.download_path = path.to_string_lossy().to_string();
                    }
                } else if ans.starts_with("2") {
                    let quality_opts = vec![
                        "always ask",
                        "best",
                        "4k",
                        "2k",
                        "1080p",
                        "720p",
                        "480p",
                        "audio",
                    ];
                    if let Ok(quality) =
                        Select::new("Select Default Quality:", quality_opts).prompt()
                    {
                        config.default_quality = quality.to_string();
                    }
                } else if ans.starts_with("3") {
                    let parallel_opts = vec!["1", "3", "5", "8"];
                    if let Ok(Ok(parallel)) =
                        Select::new("Select Max Parallel Downloads:", parallel_opts)
                            .prompt()
                            .map(|s| s.parse::<u8>())
                    {
                        config.max_parallel_downloads = parallel;
                    }
                } else if ans.starts_with("4") {
                    ConfigManager::save(config);
                    println!("{}", "SUCCESS: Settings saved successfully!".green());
                    break;
                } else if ans.starts_with("5") {
                    println!("{}", "WARNING: Settings update cancelled.".red());
                    break;
                }
            }
            Err(_) => {
                println!("{}", "WARNING: Settings update cancelled.".red());
                break;
            }
        }
    }
}
