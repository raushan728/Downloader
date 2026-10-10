use crate::engine::downloader::MediaFormat;
use inquire::Select;

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

pub fn interactive_config(config: &mut crate::config::manager::DwdrConfig) {
    use colored::Colorize;
    use inquire::Text;

    loop {
        let options = vec![
            format!("1. Default UI (Current: {})", config.default_ui),
            format!("2. Download Path (Current: {})", config.download_path),
            format!("3. Web Server Port (Current: {})", config.port),
            format!(
                "4. Max Parallel Downloads (Current: {})",
                config.max_parallel_downloads
            ),
            "5. Exit & Save".to_string(),
            "6. Cancel without saving".to_string(),
        ];

        let choice = Select::new("Configuration Menu (Use arrow keys):", options).prompt();

        match choice {
            Ok(ans) => {
                if ans.starts_with("1") {
                    let ui_opts = vec!["cli".to_string(), "browser".to_string()];
                    if let Ok(ui) = Select::new("Select Default UI:", ui_opts).prompt() {
                        config.default_ui = ui;
                    }
                } else if ans.starts_with("2") {
                    if let Ok(path) = Text::new("Enter new Download Path:")
                        .with_default(&config.download_path)
                        .prompt()
                    {
                        config.download_path = path;
                    }
                } else if ans.starts_with("3") {
                    if let Ok(port_str) = Text::new("Enter new Web Server Port:")
                        .with_default(&config.port.to_string())
                        .prompt()
                    {
                        if let Ok(port) = port_str.parse::<u16>() {
                            config.port = port;
                        } else {
                            println!(
                                "{}",
                                "ERROR: Invalid port value. Must be a number between 0 and 65535."
                                    .red()
                            );
                        }
                    }
                } else if ans.starts_with("4") {
                    if let Ok(max_str) = Text::new("Enter Max Parallel Downloads:")
                        .with_default(&config.max_parallel_downloads.to_string())
                        .prompt()
                    {
                        if let Ok(max) = max_str.parse::<u8>() {
                            config.max_parallel_downloads = max;
                        } else {
                            println!(
                                "{}",
                                "ERROR: Invalid number. Must be between 1 and 255.".red()
                            );
                        }
                    }
                } else if ans.starts_with("5") {
                    if let Ok(quality) = Text::new("Enter new Default Quality:")
                        .with_default(&config.default_quality)
                        .prompt()
                    {
                        config.default_quality = quality;
                    }
                } else if ans.starts_with("6") {
                    crate::config::manager::ConfigManager::save(config);
                    println!("{}", "✅ Settings saved successfully!".green());
                    break;
                } else if ans.starts_with("7") {
                    println!("{}", "🚫 Settings update cancelled.".red());
                    break;
                }
            }
            Err(_) => {
                println!("{}", "🚫 Settings update cancelled.".red());
                break;
            }
        }
    }
}
