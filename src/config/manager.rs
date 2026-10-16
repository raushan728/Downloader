use colored::Colorize;
use directories::{BaseDirs, UserDirs};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

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
                    .unwrap_or_else(|| {
                        std::env::current_dir().unwrap_or_else(|_| std::env::temp_dir())
                    });
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

static PROBE_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Validates existence, directory type, and write access for a target download path.
pub fn validate_download_path(path: &str, create_if_missing: bool) -> Result<PathBuf, String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err("Download path is not configured.".to_string());
    }

    let p = std::path::Path::new(trimmed);
    if !p.exists() {
        if create_if_missing {
            fs::create_dir_all(p).map_err(|e| {
                format!(
                    "Download directory \"{}\" does not exist and could not be created: {}",
                    trimmed, e
                )
            })?;
        } else {
            return Err("Download path does not exist.".to_string());
        }
    }

    if !p.is_dir() {
        return Err("Download path must be a directory, not a file.".to_string());
    }

    let probe_id = PROBE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let probe_file = p.join(format!(
        ".dwdr_write_test_{}_{}.tmp",
        std::process::id(),
        probe_id
    ));
    match fs::write(&probe_file, b"") {
        Ok(_) => {
            let _ = fs::remove_file(&probe_file);
        }
        Err(e) => {
            return Err(format!("Download path is not writable: {}", e));
        }
    }

    let canonical = p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
    #[cfg(windows)]
    let canonical = canonical
        .to_str()
        .map(|s| {
            if let Some(unc) = s.strip_prefix(r"\\?\UNC\") {
                PathBuf::from(format!(r"\\{}", unc))
            } else if let Some(local) = s.strip_prefix(r"\\?\") {
                PathBuf::from(local)
            } else {
                PathBuf::from(s)
            }
        })
        .unwrap_or(canonical);

    Ok(canonical)
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
                let final_path = if abs_path.is_dir() {
                    abs_path.join("config.toml")
                } else {
                    abs_path
                };
                return Some(final_path);
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
        let file_path = if path.is_dir() {
            path.join("config.toml")
        } else {
            path.to_path_buf()
        };

        if file_path.is_file()
            && let Ok(content) = fs::read_to_string(&file_path)
            && let Ok(config) = toml::from_str(&content)
        {
            return config;
        }

        let default_config = DwdrConfig::default();
        let _ = Self::save_config(&file_path, &default_config);
        default_config
    }

    pub fn save(config: &DwdrConfig) -> Result<(), String> {
        match Self::config_path() {
            Some(path) => Self::save_config(&path, config),
            None => Err("Failed to determine home directory for configuration.".to_string()),
        }
    }

    pub fn save_config(path: &std::path::Path, config: &DwdrConfig) -> Result<(), String> {
        if path.is_dir() {
            return Err(format!(
                "Config path points to a directory, expected a file: {}",
                path.display()
            ));
        }

        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
            && !parent.exists()
            && let Err(e) = fs::create_dir_all(parent)
        {
            return Err(format!("Failed to create config directory: {}", e));
        }

        match toml::to_string(config) {
            Ok(toml_str) => {
                fs::write(path, toml_str).map_err(|e| format!("Failed to write config file: {}", e))
            }
            Err(e) => Err(format!("Failed to serialize config: {}", e)),
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
                        "path" => match validate_download_path(&value, false) {
                            Ok(canonical) => {
                                config.download_path = canonical.to_string_lossy().to_string();
                            }
                            Err(e) => {
                                println!("{}", format!("\nERROR: {}\n", e).red());
                                return;
                            }
                        },
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

    #[test]
    fn test_default_download_path_is_absolute() {
        let config = DwdrConfig::default();
        let path = std::path::Path::new(&config.download_path);
        assert!(path.is_absolute());
        assert!(!config.download_path.starts_with('.'));
    }

    #[test]
    fn test_load_from_path_directory_resolves_config_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let dir_path = temp_dir.path();

        let loaded = ConfigManager::load_from_path(dir_path);
        assert_eq!(loaded.default_quality, "1080p");
        assert!(dir_path.join("config.toml").is_file());

        let mut modified = loaded;
        modified.default_quality = "4k".to_string();
        ConfigManager::save_config(&dir_path.join("config.toml"), &modified).unwrap();

        let reloaded = ConfigManager::load_from_path(dir_path);
        assert_eq!(reloaded.default_quality, "4k");
    }

    #[test]
    fn test_validate_download_path() {
        let temp_dir = tempfile::tempdir().unwrap();
        let dir_path = temp_dir.path();

        let valid = validate_download_path(dir_path.to_str().unwrap(), false);
        assert!(valid.is_ok());

        let new_subdir = dir_path.join("new_downloads_dir");
        let created = validate_download_path(new_subdir.to_str().unwrap(), true);
        assert!(created.is_ok());
        assert!(new_subdir.is_dir());

        let non_existent = dir_path.join("does_not_exist");
        let missing = validate_download_path(non_existent.to_str().unwrap(), false);
        assert!(missing.is_err());
        assert!(missing.unwrap_err().contains("does not exist"));

        let file_path = dir_path.join("test_file.txt");
        std::fs::write(&file_path, b"data").unwrap();
        let file_res = validate_download_path(file_path.to_str().unwrap(), false);
        assert!(file_res.is_err());
        assert!(file_res.unwrap_err().contains("not a file"));

        let empty_res = validate_download_path("   ", false);
        assert!(empty_res.is_err());
        assert!(empty_res.unwrap_err().contains("not configured"));
    }
}
