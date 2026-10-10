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
                println!("{}", "Update command triggered".yellow());
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
