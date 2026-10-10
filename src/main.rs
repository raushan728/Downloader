mod cli;
mod config;
pub mod engine;
pub mod utils;

use clap::Parser;
use cli::router::{Cli, Commands};
use colored::Colorize;
use config::manager::ConfigManager;

fn main() {
    let mut config = ConfigManager::load();
    let cli = Cli::parse();

    if let Some(url) = cli.url {
        if !utils::validator::is_valid_url(&url) {
            println!(
                "{}",
                "ERROR: Invalid URL provided. It must start with http:// or https://"
                    .red()
                    .bold()
            );
            return;
        }

        println!("{}", "Valid URL detected. Resolving engine paths...".cyan());

        let yt_dlp_path = engine::paths::get_ytdlp_path();
        let ffmpeg_path = engine::paths::get_ffmpeg_path();

        println!("{}", format!("Engine path: {:?}", yt_dlp_path).blue());
        println!("{}", format!("Engine path: {:?}", ffmpeg_path).blue());

        if let Err(e) = engine::updater::ensure_dependencies() {
            println!(
                "{}",
                format!("ERROR: Failed to setup engine: {}", e).red().bold()
            );
            return;
        }

        println!("{}", "Fetching media details from the server... ⏳".cyan());

        match engine::downloader::fetch_video_info(&url, &yt_dlp_path) {
            Ok(info) => {
                println!(
                    "{}",
                    format!("Successfully fetched data for: {}", info.title).green()
                );
                println!(
                    "{}",
                    format!("Extracted {} formats.", info.formats.len()).green()
                );
            }
            Err(e) => {
                println!(
                    "{}",
                    format!("ERROR: Failed to extract video info: {}", e).red()
                );
            }
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
                println!("{}", "Update command triggered".yellow());
            }
        }
        return;
    }

    if cli.web || (!cli.cli && config.default_ui == "browser") {
        println!("{}", "Starting Web UI...".green());
    } else {
        println!("{}", "Starting CLI Interactive Shell...".green());
    }
}
