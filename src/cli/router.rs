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
    dwdr self-update                # Update the DWDR CLI tool itself\n\n\
INTERACTIVE SHELL COMMANDS:\n\
    <URL>             Start downloading media stream (managed via queue)\n\
    status            Real-time live progress monitor of active downloads\n\
    list              Summary status report of all downloads in session\n\
    queue             Inspect queued tasks and concurrency limits (use 'queue live' for real-time monitor)\n\
    wait              Wait for all active and queued downloads to finish\n\
    cancel            Cancel active downloads and clear queue\n\
    config / settings Configure download directory, default quality, and limits\n\
    cls / clear       Clear terminal screen\n\
    help              Display available interactive shell commands\n\
    exit / quit       Cancel pending tasks and exit shell"
)]
pub struct Cli {
    /// Starts downloading the specified media URL directly without launching the interactive shell.
    pub url: Option<String>,

    /// Force overwrite existing media files without interactive confirmation.
    #[arg(short, long)]
    pub force: bool,

    /// Disable spinners and animated progress.
    #[arg(long)]
    pub no_animation: bool,

    /// Suppress banner and non-essential output.
    #[arg(short, long)]
    pub quiet: bool,

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
