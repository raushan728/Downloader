use directories::{BaseDirs, UserDirs};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize)]
pub struct DwdrConfig {
    pub default_ui: String,
    pub download_path: String,
    pub port: u16,
    pub update_check_days: u32,
    pub max_parallel_downloads: u8,
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
            default_ui: "cli".to_string(),
            download_path,
            port: 5500,
            update_check_days: 0,
            max_parallel_downloads: 3,
        }
    }
}

pub struct ConfigManager;

impl ConfigManager {
    // DWDR strictly isolates its data from system paths by using a dedicated hidden directory.
    pub fn config_path() -> Option<PathBuf> {
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

        if path.exists() {
            match fs::read_to_string(&path) {
                Ok(content) => match toml::from_str(&content) {
                    Ok(config) => return config,
                    Err(_) => {}
                },
                Err(_) => {}
            }
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
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                if let Err(e) = fs::create_dir_all(parent) {
                    eprintln!("Failed to create config directory: {}", e);
                    return;
                }
            }
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
}
