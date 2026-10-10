mod cli;
mod config;
pub mod engine;
pub mod utils;
pub mod web;

use clap::Parser;
use cli::router::{Cli, Commands};
use colored::Colorize;
use config::manager::ConfigManager;

fn main() {
    let mut config = ConfigManager::load();
    let cli = Cli::parse();

    if let Some(url) = cli.url {
        crate::cli::shell::process_url(&url, &config);
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
                let yt = crate::engine::paths::get_ytdlp_path();
                let ff = crate::engine::paths::get_ffmpeg_path();
                let _ = std::fs::remove_file(yt);
                let _ = std::fs::remove_file(ff);

                if let Err(e) = crate::engine::updater::ensure_dependencies() {
                    println!("{}", format!("\nERROR: {}\n", e).red().bold());
                } else if let Err(e) = crate::engine::updater::ensure_ffmpeg() {
                    println!("{}", format!("\nERROR: {}\n", e).red().bold());
                } else {
                    println!(
                        "{}",
                        "SUCCESS: All core engines are updated to the latest versions!".green()
                    );
                }
            }
        }
        return;
    }

    if cli.web || (!cli.cli && config.default_ui == "browser") {
        crate::web::server::start_server(&config);
    } else {
        crate::cli::shell::start_repl(&config);
    }
}
