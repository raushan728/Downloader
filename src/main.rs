mod cli;
mod config;

use clap::Parser;
use cli::router::{Cli, Commands};
use colored::Colorize;
use config::manager::ConfigManager;

fn main() {
    let config = ConfigManager::load();
    let cli = Cli::parse();

    if let Some(url) = cli.url {
        println!("{}", format!("Starting download for {}...", url).cyan());
        return;
    }

    if let Some(command) = cli.command {
        match command {
            Commands::Config { action, .. } => {
                println!(
                    "{}",
                    format!("Config command triggered: {}", action).yellow()
                );
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
