mod cli;
mod config;
pub mod engine;
pub mod utils;

use clap::Parser;
use cli::router::{Cli, Commands};
use cli::shell::{process_url, start_repl};
use colored::Colorize;
use config::manager::ConfigManager;
use engine::paths::{get_ffmpeg_path, get_ytdlp_path};
use engine::updater::{ensure_dependencies, ensure_ffmpeg, update_self};

fn main() {
    #[cfg(target_os = "windows")]
    {
        if colored::control::set_virtual_terminal(true).is_err() {
            colored::control::set_override(false);
        }
        if let Ok(current_exe) = std::env::current_exe() {
            let old_exe = current_exe.with_extension("old");
            if old_exe.exists() {
                let _ = std::fs::remove_file(old_exe);
            }
        }
    }

    let _ = ctrlc::set_handler(move || {
        crate::engine::downloader::cancel_direct_download();
        crate::engine::queue::cancel_global_downloads();
        eprintln!(
            "{}",
            "\nINFO: Interrupted by user. Cleaned up partial files.".yellow()
        );
        std::process::exit(130);
    });

    let mut config = ConfigManager::load();
    let cli = Cli::parse();

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
