use colored::Colorize;
use inquire::Select;
use rfd::FileDialog;

use crate::config::manager::{ConfigManager, DwdrConfig};
use crate::engine::downloader::MediaFormat;

pub fn select_format(formats: &[MediaFormat]) -> Option<String> {
    let mut options = Vec::new();

    // Reverse iterate so highest quality is top of the list
    for format in formats.iter().rev() {
        let tag = if format.vcodec == "none" {
            "[AUDIO]"
        } else {
            "[VIDEO]"
        };

        let option_str = format!(
            "{:<7} | {:<10} | {:<5} | {}",
            tag,
            format.resolution,
            format.ext,
            format.size_in_mb()
        );
        options.push(option_str);
    }

    match Select::new(
        "Select a format to download (Use arrow keys):",
        options.clone(),
    )
    .prompt()
    {
        Ok(choice) => {
            if let Some(index) = options.iter().position(|r| r == &choice) {
                let format_index = formats.len() - 1 - index;
                Some(formats[format_index].format_id.clone())
            } else {
                None
            }
        }
        Err(_) => None,
    }
}

pub fn interactive_config(config: &mut DwdrConfig) {
    loop {
        let options = vec![
            format!("1. Download Path (Current: {})", config.download_path),
            format!("2. Default Quality (Current: {})", config.default_quality),
            format!(
                "3. Max Parallel Downloads (Current: {})",
                config.max_parallel_downloads
            ),
            "4. Save & Exit".to_string(),
            "5. Cancel".to_string(),
        ];

        let choice = Select::new("Configuration Menu (Use arrow keys):", options).prompt();

        match choice {
            Ok(ans) => {
                if ans.starts_with("1") {
                    println!("{}", "INFO: Opening native file browser...".cyan());
                    if let Some(path) = FileDialog::new()
                        .set_title("Select Download Folder")
                        .pick_folder()
                    {
                        config.download_path = path.to_string_lossy().to_string();
                    }
                } else if ans.starts_with("2") {
                    let quality_opts = vec!["best", "1080p", "720p", "480p", "audio"];
                    if let Ok(quality) =
                        Select::new("Select Default Quality:", quality_opts).prompt()
                    {
                        config.default_quality = quality.to_string();
                    }
                } else if ans.starts_with("3") {
                    let parallel_opts = vec!["1", "3", "5", "8"];
                    if let Ok(parallel_str) =
                        Select::new("Select Max Parallel Downloads:", parallel_opts).prompt()
                    {
                        if let Ok(parallel) = parallel_str.parse::<u8>() {
                            config.max_parallel_downloads = parallel;
                        }
                    }
                } else if ans.starts_with("4") {
                    ConfigManager::save(config);
                    println!("{}", "SUCCESS: Settings saved successfully!".green());
                    break;
                } else if ans.starts_with("5") {
                    println!("{}", "WARNING: Settings update cancelled.".red());
                    break;
                }
            }
            Err(_) => {
                println!("{}", "WARNING: Settings update cancelled.".red());
                break;
            }
        }
    }
}
