use colored::Colorize;
use directories::{BaseDirs, UserDirs};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::PathBuf;

use crate::cli::prompts::interactive_config;
use crate::engine::queue::is_download_active;

/// Global runtime configuration serialized in `~/.dwdr/config.toml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
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

pub fn is_valid_quality(quality: &str) -> bool {
    matches!(
        quality.trim().to_lowercase().as_str(),
        "always ask"
            | "best"
            | "8k"
            | "4k"
            | "2k"
            | "1080p"
            | "720p"
            | "480p"
            | "360p"
            | "240p"
            | "144p"
            | "audio"
    )
}

/// Thread-safe configuration manager enforcing filesystem isolation and recovery.
pub struct ConfigManager;

impl ConfigManager {
    // DWDR strictly isolates its data from system paths by using a dedicated hidden directory.
    pub fn config_path() -> Option<PathBuf> {
        if let Ok(env_path) = std::env::var("DWDR_CONFIG_PATH") {
            let trimmed = env_path.trim();
            if !trimmed.is_empty() {
                let path = PathBuf::from(trimmed);
                let abs_path = if path.is_relative() {
                    std::env::current_dir()
                        .map(|cwd| cwd.join(&path))
                        .unwrap_or(path)
                } else {
                    path
                };
                return Some(abs_path);
            }
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

        Self::load_from_path(&path)
    }

    /// Loads configuration from disk, self-healing with defaults if missing or corrupted.
    pub fn load_from_path(path: &std::path::Path) -> DwdrConfig {
        if path.is_file()
            && let Ok(content) = fs::read_to_string(path)
            && let Ok(config) = toml::from_str(&content)
        {
            return config;
        }

        let default_config = DwdrConfig::default();
        let _ = Self::save_config(path, &default_config);
        default_config
    }

    pub fn save(config: &DwdrConfig) -> Result<(), String> {
        match Self::config_path() {
            Some(path) => Self::save_config(&path, config),
            None => {
                let err = "Failed to determine home directory for configuration.".to_string();
                eprintln!("{}", err);
                Err(err)
            }
        }
    }

    pub fn save_config(path: &std::path::Path, config: &DwdrConfig) -> Result<(), String> {
        if path.is_dir() {
            let err = format!(
                "Config path points to a directory, expected a file: {}",
                path.display()
            );
            eprintln!("{}", err);
            return Err(err);
        }

        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
            && !parent.exists()
            && let Err(e) = fs::create_dir_all(parent)
        {
            let err = format!("Failed to create config directory: {}", e);
            eprintln!("{}", err);
            return Err(err);
        }

        match toml::to_string(config) {
            Ok(toml_str) => fs::write(path, toml_str).map_err(|e| {
                let err = format!("Failed to write config file: {}", e);
                eprintln!("{}", err);
                err
            }),
            Err(e) => {
                let err = format!("Failed to serialize config: {}", e);
                eprintln!("{}", err);
                Err(err)
            }
        }
    }

    pub fn reset_to_default() -> Result<DwdrConfig, String> {
        let default_config = DwdrConfig::default();
        Self::save(&default_config)?;
        Ok(default_config)
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
                if is_download_active() && !force {
                    println!(
                        "{}",
                        "\nERROR: Cannot change settings while downloads are active. Use --force to override.\n"
                            .red()
                            .bold()
                    );
                    return;
                }
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
                    if is_download_active() && !force {
                        println!(
                            "{}",
                            "\nERROR: Cannot change settings while downloads are active. Use --force to override.\n"
                                .red()
                                .bold()
                        );
                        return;
                    }

                    if !force {
                        print!("Are you sure you want to reset configuration to defaults? [y/N]: ");
                        let _ = std::io::stdout().flush();
                        let mut input = String::new();
                        if std::io::stdin().read_line(&mut input).is_err()
                            || (!input.trim().eq_ignore_ascii_case("y")
                                && !input.trim().eq_ignore_ascii_case("yes"))
                        {
                            println!("{}", "WARNING: Configuration reset cancelled.".yellow());
                            return;
                        }
                    }

                    match Self::reset_to_default() {
                        Ok(default_cfg) => {
                            *config = default_cfg;
                            println!("{}", "SUCCESS: Configuration reset to defaults.".green());
                        }
                        Err(e) => {
                            println!(
                                "{}",
                                format!("\nERROR: Failed to reset configuration: {}\n", e).red()
                            );
                        }
                    }
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
                            let p = std::path::Path::new(value.trim());
                            if value.trim().is_empty() || !p.exists() {
                                println!("{}", "\nERROR: Download path does not exist.\n".red());
                                return;
                            }
                            if !p.is_dir() {
                                println!(
                                    "{}",
                                    "\nERROR: Download path must be a directory, not a file.\n"
                                        .red()
                                );
                                return;
                            }
                            let probe_file =
                                p.join(format!(".dwdr_write_test_{}.tmp", std::process::id()));
                            match fs::write(&probe_file, b"") {
                                Ok(_) => {
                                    let _ = fs::remove_file(&probe_file);
                                }
                                Err(e) => {
                                    println!(
                                        "{}",
                                        format!("\nERROR: Download path is not writable: {}\n", e)
                                            .red()
                                    );
                                    return;
                                }
                            }
                            let canonical = p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
                            #[cfg(windows)]
                            let canonical = canonical
                                .to_str()
                                .and_then(|s| s.strip_prefix(r"\\?\"))
                                .map(PathBuf::from)
                                .unwrap_or(canonical);
                            config.download_path = canonical.to_string_lossy().to_string();
                        }
                        "quality" => {
                            if !is_valid_quality(&value) {
                                println!(
                                    "{}",
                                    "\nERROR: Invalid quality value. Valid options: always ask, best, 8k, 4k, 2k, 1080p, 720p, 480p, 360p, 240p, 144p, audio.\n"
                                        .red()
                                );
                                return;
                            }
                            config.default_quality = value.trim().to_lowercase();
                        }
                        "parallel" => match value.parse::<u8>() {
                            Ok(v) if (1..=16).contains(&v) => config.max_parallel_downloads = v,
                            Ok(_) | Err(_) => {
                                println!(
                                    "{}",
                                    "\nERROR: Invalid parallel downloads value. Must be between 1 and 16.\n"
                                        .red()
                                );
                                return;
                            }
                        },
                        "update-days" => match value.parse::<u32>() {
                            Ok(v) if v <= 365 => config.update_check_days = v,
                            Ok(_) | Err(_) => {
                                println!(
                                    "{}",
                                    "\nERROR: Invalid update-days value. Must be between 0 and 365.\n"
                                        .red()
                                );
                                return;
                            }
                        },
                        _ => {
                            println!("{}", "\nERROR: Unknown config key.\n".red());
                            return;
                        }
                    }

                    if let Err(e) = Self::save(config) {
                        println!(
                            "{}",
                            format!("\nERROR: Failed to save configuration: {}\n", e).red()
                        );
                        return;
                    }
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

    #[test]
    fn test_config_self_healing_corrupted_file() {
        let temp = tempfile::NamedTempFile::new().unwrap();
        let path = temp.path().to_path_buf();
        std::fs::write(&path, "this is corrupt toml! {{{]]] :::").unwrap();

        let config = ConfigManager::load_from_path(&path);
        assert_eq!(config.default_quality, "1080p");
        assert_eq!(config.max_parallel_downloads, 3);

        // Verify the file was healed with valid TOML
        let healed_content = std::fs::read_to_string(&path).unwrap();
        let parsed: Result<DwdrConfig, _> = toml::from_str(&healed_content);
        assert!(parsed.is_ok());
    }

    #[test]
    fn test_config_handle_cli_edge_cases() {
        let mut config = DwdrConfig::default();

        // Unknown action
        ConfigManager::handle_cli(
            &mut config,
            Some("invalid_action".to_string()),
            None,
            None,
            false,
        );

        // Missing key for set
        ConfigManager::handle_cli(&mut config, Some("set".to_string()), None, None, false);

        // Missing value for set
        ConfigManager::handle_cli(
            &mut config,
            Some("set".to_string()),
            Some("quality".to_string()),
            None,
            false,
        );

        // Unknown key
        ConfigManager::handle_cli(
            &mut config,
            Some("set".to_string()),
            Some("foo".to_string()),
            Some("bar".to_string()),
            false,
        );

        // Invalid parallel
        ConfigManager::handle_cli(
            &mut config,
            Some("set".to_string()),
            Some("parallel".to_string()),
            Some("abc".to_string()),
            false,
        );

        // Invalid update-days
        ConfigManager::handle_cli(
            &mut config,
            Some("set".to_string()),
            Some("update-days".to_string()),
            Some("invalid_num".to_string()),
            false,
        );

        // Invalid parallel (out of 1..=16 bounds)
        ConfigManager::handle_cli(
            &mut config,
            Some("set".to_string()),
            Some("parallel".to_string()),
            Some("0".to_string()),
            false,
        );
        ConfigManager::handle_cli(
            &mut config,
            Some("set".to_string()),
            Some("parallel".to_string()),
            Some("17".to_string()),
            false,
        );
        ConfigManager::handle_cli(
            &mut config,
            Some("set".to_string()),
            Some("parallel".to_string()),
            Some("255".to_string()),
            false,
        );

        // Invalid quality
        ConfigManager::handle_cli(
            &mut config,
            Some("set".to_string()),
            Some("quality".to_string()),
            Some("invalid_quality_str".to_string()),
            false,
        );

        // Invalid update-days (out of 0..=365 bounds)
        ConfigManager::handle_cli(
            &mut config,
            Some("set".to_string()),
            Some("update-days".to_string()),
            Some("366".to_string()),
            false,
        );
        ConfigManager::handle_cli(
            &mut config,
            Some("set".to_string()),
            Some("update-days".to_string()),
            Some("10000".to_string()),
            false,
        );

        // Invalid path (does not exist or is a file)
        ConfigManager::handle_cli(
            &mut config,
            Some("set".to_string()),
            Some("path".to_string()),
            Some("/nonexistent/directory/for/dwdr".to_string()),
            false,
        );

        // Values remain unaffected by erroneous input
        assert_eq!(config.default_quality, "1080p");
        assert_eq!(config.max_parallel_downloads, 3);
        assert_eq!(config.update_check_days, 0);

        // Valid updates succeed
        let temp_dir = tempfile::tempdir().unwrap();
        let valid_path = temp_dir.path().to_str().unwrap().to_string();
        ConfigManager::handle_cli(
            &mut config,
            Some("set".to_string()),
            Some("path".to_string()),
            Some(valid_path.clone()),
            true,
        );
        assert_eq!(
            std::path::Path::new(&config.download_path),
            std::path::Path::new(&valid_path).canonicalize().unwrap()
        );

        ConfigManager::handle_cli(
            &mut config,
            Some("set".to_string()),
            Some("quality".to_string()),
            Some("4k".to_string()),
            true,
        );
        assert_eq!(config.default_quality, "4k");

        ConfigManager::handle_cli(
            &mut config,
            Some("set".to_string()),
            Some("parallel".to_string()),
            Some("16".to_string()),
            true,
        );
        assert_eq!(config.max_parallel_downloads, 16);

        ConfigManager::handle_cli(
            &mut config,
            Some("set".to_string()),
            Some("update-days".to_string()),
            Some("30".to_string()),
            true,
        );
        assert_eq!(config.update_check_days, 30);
    }

    #[test]
    fn test_save_config_fails_when_path_is_directory() {
        let temp_dir = tempfile::tempdir().unwrap();
        let config = DwdrConfig::default();
        let result = ConfigManager::save_config(temp_dir.path(), &config);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("expected a file"));
    }

    #[test]
    fn test_is_valid_quality() {
        assert!(is_valid_quality("always ask"));
        assert!(is_valid_quality("best"));
        assert!(is_valid_quality("8k"));
        assert!(is_valid_quality("4K"));
        assert!(is_valid_quality("2k"));
        assert!(is_valid_quality("1080p"));
        assert!(is_valid_quality("720p"));
        assert!(is_valid_quality("480p"));
        assert!(is_valid_quality("audio"));
        assert!(!is_valid_quality("arbitrary_string"));
        assert!(!is_valid_quality(""));
    }
}
