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

                let chosen_format = if let Some(format_id) =
                    engine::downloader::find_format_by_quality(
                        &info.formats,
                        &config.default_quality,
                    ) {
                    println!(
                        "{}",
                        format!(
                            "✨ Exact quality ({}) found! Auto-starting download...",
                            config.default_quality
                        )
                        .green()
                    );
                    Some(format_id)
                } else {
                    println!(
                        "{}",
                        format!(
                            "⚠️ Quality ({}) not found. Falling back to manual selection...",
                            config.default_quality
                        )
                        .yellow()
                    );
                    if let Some(format_id) = cli::prompts::select_format(&info.formats) {
                        println!(
                            "{}",
                            format!(
                                "✅ Selected Format ID: {}. Download sequence will start here.",
                                format_id
                            )
                            .green()
                        );
                        Some(format_id)
                    } else {
                        println!("{}", "🚫 Download cancelled by user.".red());
                        None
                    }
                };

                if let Some(format_id) = chosen_format {
                    match engine::downloader::download_media(
                        &url,
                        &format_id,
                        &yt_dlp_path,
                        &ffmpeg_path,
                        &config.download_path,
                    ) {
                        Ok(()) => {
                            println!(
                                "{}",
                                format!(
                                    "🎉 Download completed successfully! File saved to: {}",
                                    config.download_path
                                )
                                .green()
                                .bold()
                            );
                        }
                        Err(e) => {
                            println!("{}", format!("❌ Download failed: {}", e).red().bold());
                        }
                    }
                }
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
