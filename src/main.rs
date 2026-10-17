mod cli;
mod config;
pub mod engine;
pub mod utils;

use clap::Parser;
use cli::router::{Cli, Commands};
use cli::shell::{process_url, start_repl};
use colored::Colorize;
use config::manager::ConfigManager;
use engine::downloader::cancel_direct_download;
use engine::paths::{get_ffmpeg_path, get_ytdlp_path};
use engine::queue::cancel_global_downloads;
use engine::updater::{ensure_dependencies, ensure_ffmpeg, update_self};

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
            if old_exe.exists() {
                let _ = std::fs::remove_file(old_exe);
            }
        }
    }

    // Trap SIGINT/Ctrl+C to terminate child process trees and sweep orphaned partial download artifacts.
    static SIGNAL_RECEIVED: std::sync::atomic::AtomicBool =
        std::sync::atomic::AtomicBool::new(false);
    let _ = ctrlc::set_handler(move || {
        if SIGNAL_RECEIVED.swap(true, std::sync::atomic::Ordering::SeqCst) {
            std::process::exit(130);
        }
        let _ = crossterm::terminal::disable_raw_mode();
        let _ = crossterm::execute!(std::io::stdout(), crossterm::cursor::Show);
        cancel_direct_download();
        cancel_global_downloads();
        eprintln!(
            "{}",
            "\nINFO: Interrupted by user. Cleaned up partial files.".yellow()
        );
        std::process::exit(130);
    });

    let cli = Cli::parse();
    let mut config = ConfigManager::load();

    if let Some(url) = cli.url {
        process_url(&url, &mut config);
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
                println!("{}", "INFO: Forcing update for core engines...".cyan());
                let yt = get_ytdlp_path();
                let ff = get_ffmpeg_path();
                let _ = std::fs::remove_file(yt);
                let _ = std::fs::remove_file(ff);

                if let Err(e) = ensure_dependencies() {
                    println!("{}", format!("\nERROR: {}\n", e).red().bold());
                } else if let Err(e) = ensure_ffmpeg() {
                    println!("{}", format!("\nERROR: {}\n", e).red().bold());
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
                    println!("{}", format!("\nERROR: {}\n", e).red().bold());
                }
            }
        }
        return;
    }

    start_repl(&mut config);
}
