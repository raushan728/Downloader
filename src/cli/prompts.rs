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
