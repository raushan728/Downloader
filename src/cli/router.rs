use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "dwdr", version, about = "Enterprise-grade CLI and Web media downloader", long_about = None)]
pub struct Cli {
    /// Bypasses the UI and immediately starts downloading the provided resource.
    pub url: Option<String>,

    /// Overrides the config's default UI to force the embedded web server.
    #[arg(short = 'w', long)]
    pub web: bool,

    /// Overrides the config's default UI to force the interactive terminal.
    #[arg(short = 'c', long)]
    pub cli: bool,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Modify `~/.dwdr/config.toml` programmatically.
    Config {
        action: String,
        key: Option<String>,
        value: Option<String>,
        #[arg(short, long)]
        force: bool,
    },
    /// Fetch and update isolated yt-dlp and ffmpeg binaries in `~/.dwdr/deps/`.
    Update,
}
