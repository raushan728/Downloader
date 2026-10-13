use colored::Colorize;
use directories::{BaseDirs, UserDirs};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

use crate::cli::prompts::interactive_config;
use crate::engine::queue::is_download_active;

#[derive(Debug, Serialize, Deserialize)]
pub struct DwdrConfig {
    pub download_path: String,
    pub update_check_days: u32,
    pub max_parallel_downloads: u8,
    pub default_quality: String,
}

impl Default for DwdrConfig {
    fn default() -> Self {
        let download_path = UserDirs::new()
            .and_then(|dirs| {
                dirs.download_dir()
                    .map(|d| d.join("DWDR").to_string_lossy().into_owned())
            })
            .unwrap_or_else(|| {
                // Headless or minimal environments may lack standard XDG user directories.
                let mut path = BaseDirs::new()
                    .map(|d| d.home_dir().to_path_buf())
                    .unwrap_or_else(|| PathBuf::from("."));
                path.push("Downloads");
                path.push("DWDR");
                path.to_string_lossy().into_owned()
            });

        Self {
            download_path,
            update_check_days: 0,
            max_parallel_downloads: 3,
            default_quality: "1080p".to_string(),
        }
    }
}

pub struct ConfigManager;

impl ConfigManager {
    // DWDR strictly isolates its data from system paths by using a dedicated hidden directory.
    pub fn config_path() -> Option<PathBuf> {
        if let Ok(env_path) = std::env::var("DWDR_CONFIG_PATH") {
            return Some(PathBuf::from(env_path));
        }
        BaseDirs::new().map(|dirs| dirs.home_dir().join(".dwdr").join("config.toml"))
    }

    // Prefer self-healing over crashing so the user's workflow isn't interrupted by a corrupted config.
    pub fn load() -> DwdrConfig {
        let path = match Self::config_path() {
            Some(p) => p,
            None => {
                eprintln!("Failed to determine home directory. Using default config.");
                return DwdrConfig::default();
            }
        };

        if path.exists()
            && let Ok(content) = fs::read_to_string(&path)
            && let Ok(config) = toml::from_str(&content)
        {
            return config;
        }

        let default_config = DwdrConfig::default();
        Self::save_config(&path, &default_config);
        default_config
    }

    pub fn save(config: &DwdrConfig) {
        if let Some(path) = Self::config_path() {
            Self::save_config(&path, config);
        }
    }

    fn save_config(path: &PathBuf, config: &DwdrConfig) {
        if let Some(parent) = path.parent()
            && !parent.exists()
            && let Err(e) = fs::create_dir_all(parent)
        {
            eprintln!("Failed to create config directory: {}", e);
            return;
        }

        match toml::to_string(config) {
            Ok(toml_str) => {
                if let Err(e) = fs::write(path, toml_str) {
                    eprintln!("Failed to write config file: {}", e);
                }
            }
            Err(e) => {
                eprintln!("Failed to serialize config: {}", e);
            }
        }
    }

    pub fn reset_to_default() -> DwdrConfig {
        let default_config = DwdrConfig::default();
        Self::save(&default_config);
        default_config
    }

    pub fn handle_cli(
        config: &mut DwdrConfig,
        action: Option<String>,
        key: Option<String>,
        value: Option<String>,
        force: bool,
    ) {
        match action {
            None => {
                interactive_config(config);
            }
            Some(act) => match act.as_str() {
                "show" => {
                    println!("{}", "Current Configuration:".cyan().bold());
                    println!(
                        "{}: {}",
                        "download_path".cyan(),
                        config.download_path.yellow()
                    );
                    println!(
                        "{}: {}",
                        "update_check_days".cyan(),
                        config.update_check_days.to_string().yellow()
                    );
                    println!(
                        "{}: {}",
                        "max_parallel_downloads".cyan(),
                        config.max_parallel_downloads.to_string().yellow()
                    );
                    println!(
                        "{}: {}",
                        "default_quality".cyan(),
                        config.default_quality.yellow()
                    );
                }
                "reset" => {
                    *config = Self::reset_to_default();
                    println!("{}", "SUCCESS: Configuration reset to defaults.".green());
                }
                "set" => {
                    let key = match key {
                        Some(k) => k,
                        None => {
                            println!("{}", "\nERROR: Missing 'key' for set action.\n".red());
                            return;
                        }
                    };
                    let value = match value {
                        Some(v) => v,
                        None => {
                            println!("{}", "\nERROR: Missing 'value' for set action.\n".red());
                            return;
                        }
                    };

                    if is_download_active() && !force {
                        println!(
                            "{}",
                            "\nERROR: Cannot change settings while downloads are active. Use --force to override.\n"
                                .red()
                                .bold()
                        );
                        return;
                    }

                    match key.as_str() {
                        "path" => {
                            config.download_path = value.clone();
                        }
                        "quality" => {
                            config.default_quality = value.clone();
                        }
                        "parallel" => match value.parse::<u8>() {
                            Ok(v) => config.max_parallel_downloads = v,
                            Err(_) => {
                                println!(
                                    "{}",
                                    "\nERROR: Invalid parallel downloads value.\n".red()
                                );
                                return;
                            }
                        },
                        "update-days" => match value.parse::<u32>() {
                            Ok(v) => config.update_check_days = v,
                            Err(_) => {
                                println!("{}", "\nERROR: Invalid update-days value.\n".red());
                                return;
                            }
                        },
                        _ => {
                            println!("{}", "\nERROR: Unknown config key.\n".red());
                            return;
                        }
                    }

                    Self::save(config);
                    println!(
                        "{}",
                        format!("SUCCESS: Successfully updated {} to {}", key, value).green()
                    );
                }
                _ => {
                    println!(
                        "{}",
                        "\nERROR: Unknown action. Use 'show', 'reset', or 'set'.\n".red()
                    );
                }
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = DwdrConfig::default();
        assert_eq!(config.update_check_days, 0);
        assert_eq!(config.max_parallel_downloads, 3);
        assert_eq!(config.default_quality, "1080p");
        assert!(!config.download_path.is_empty());
    }
}
