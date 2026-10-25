use std::io::Write;

use clap::Parser;
use colored::Colorize;
use dwdr::cli::router::{Cli, Commands};
use dwdr::cli::shell::{process_url, reset_terminal_state, start_repl};
use dwdr::config::manager::{ConfigManager, normalize_cookie_value};
use dwdr::engine::downloader::cancel_direct_download;
use dwdr::engine::queue::cancel_global_downloads;
use dwdr::engine::updater::{update_core_engines, update_self};
use dwdr::ui::UiContext;
use dwdr::ui::terminal_title::{clear_taskbar_progress, set_terminal_title};

fn main() {
    #[cfg(target_os = "windows")]
    {
        // Enable VT100 emulation on Windows CMD/PowerShell so ANSI colors do not render as raw escape codes.
        if colored::control::set_virtual_terminal(true).is_err() {
            colored::control::set_override(false);
        }
        // Windows locks running executables; self-updates rename the running binary to .old, purged here on next boot.
        if let Ok(current_exe) = std::env::current_exe() {
            let old_exe = current_exe.with_extension("old");
            if old_exe.exists()
                && let Err(e) = std::fs::remove_file(&old_exe)
            {
                eprintln!(
                    "WARNING: Failed to remove obsolete backup executable '{}': {}",
                    old_exe.display(),
                    e
                );
            }
        }
    }

    // Terminate child processes and purge partial files on interrupt.
    static SIGNAL_RECEIVED: std::sync::atomic::AtomicBool =
        std::sync::atomic::AtomicBool::new(false);
    let _ = ctrlc::set_handler(move || {
        if SIGNAL_RECEIVED.swap(true, std::sync::atomic::Ordering::SeqCst) {
            std::process::exit(130);
        }
        let _ = crossterm::terminal::disable_raw_mode();
        let _ = crossterm::execute!(
            std::io::stdout(),
            crossterm::style::ResetColor,
            crossterm::cursor::Show
        );
        let _ = crossterm::execute!(std::io::stderr(), crossterm::style::ResetColor);
        cancel_direct_download();
        cancel_global_downloads();
        clear_taskbar_progress();
        set_terminal_title("");
        eprintln!(
            "{}",
            "\nINFO: Interrupted by user. Cleaned up partial and downloaded files.".yellow()
        );
        let _ = crossterm::execute!(
            std::io::stdout(),
            crossterm::style::ResetColor,
            crossterm::cursor::Show
        );
        let _ = crossterm::execute!(std::io::stderr(), crossterm::style::ResetColor);
        let _ = std::io::stdout().flush();
        let _ = std::io::stderr().flush();
        std::process::exit(130);
    });

    let cli = Cli::parse();
    let mut config = ConfigManager::load();
    if let Some(ref cookies_override) = cli.cookies {
        match normalize_cookie_value(cookies_override) {
            Ok(normalized) => config.cookies = normalized,
            Err(suggestion) => {
                eprintln!(
                    "{}",
                    format!(
                        "\nERROR: Unknown browser or cookie source '{}'.",
                        cookies_override.trim()
                    )
                    .red()
                );
                if let Some(closest) = suggestion {
                    eprintln!("{}", format!("Tip: Did you mean '{}'?", closest).yellow());
                }
                eprintln!(
                    "{}",
                    "     Supported browsers: brave, chrome, edge, firefox, opera, vivaldi, chromium, safari, auto\n     Or provide a file path: /path/to/cookies.txt\n     To disable cookies: none\n"
                        .yellow()
                );
                std::process::exit(1);
            }
        }
    }
    UiContext::init(&config, cli.no_animation, cli.quiet);

    if let Some(url) = cli.url {
        let result = process_url(&url, &mut config, cli.force);
        reset_terminal_state();
        if result.is_err() {
            std::process::exit(1);
        }
        return;
    }

    if let Some(command) = cli.command {
        match command {
            Commands::Config {
                action,
                key,
                value,
                force,
            } => {
                ConfigManager::handle_cli(&mut config, action, key, value, force);
            }
            Commands::Update => {
                println!("{}", "INFO: Updating core engines...".cyan());
                if let Err(e) = update_core_engines() {
                    eprintln!("{}", format!("\nERROR: {}\n", e).red().bold());
                    reset_terminal_state();
                    std::process::exit(1);
                } else {
                    println!(
                        "{}",
                        "SUCCESS: All core engines are updated to the latest versions!".green()
                    );
                }
            }
            Commands::SelfUpdate => {
                println!(
                    "{}",
                    "INFO: Updating DWDR CLI to the latest version...".cyan()
                );
                if let Err(e) = update_self() {
                    eprintln!("{}", format!("\nERROR: {}\n", e).red().bold());
                    reset_terminal_state();
                    std::process::exit(1);
                }
            }
        }
        reset_terminal_state();
        return;
    }

    start_repl(&mut config);
    reset_terminal_state();
}
