use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "dwdr",
    version,
    about = "DWDR - The Ultimate Terminal Media Downloader",
    long_about = "EXAMPLES:\n\
    dwdr \"https://youtube.com/...\" # Quick download with Smart Fallback\n\
    dwdr                            # Open Interactive Terminal Shell\n\
    dwdr config                     # Open Interactive Settings Menu\n\
    dwdr config set quality 720p    # Change default download quality\n\
    dwdr update                     # Force update dependencies\n\
    dwdr self-update                # Update the DWDR CLI tool itself"
)]
pub struct Cli {
    /// Bypasses the UI and immediately starts downloading the provided resource.
    pub url: Option<String>,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Modify `~/.dwdr/config.toml` programmatically.
    Config {
        action: Option<String>,
        key: Option<String>,
        value: Option<String>,
        #[arg(short, long)]
        force: bool,
    },
    /// Fetch and update isolated yt-dlp and ffmpeg binaries in `~/.dwdr/deps/`.
    Update,
    /// Update the DWDR tool itself to the latest version.
    SelfUpdate,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn verify_cli() {
        Cli::command().debug_assert();
    }
}
