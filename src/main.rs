mod config;

use colored::Colorize;
use config::manager::ConfigManager;

fn main() {
    let config = ConfigManager::load();
    println!("{}", "DWDR Configuration Loaded:".cyan().bold());
    println!("{}", format!("{:#?}", config).cyan());
}
