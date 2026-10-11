# CLI Reference Guide

DWDR features a flexible command-line interface powered by Clap, providing direct downloads, an interactive terminal shell, and configuration management.

---

## Command Syntax

```bash
dwdr [OPTIONS] [URL]
dwdr [COMMAND]
```

---

## Global Options & Flags

| Flag | Description |
| --- | --- |
| `[URL]` | Directly downloads the provided media URL using Smart Fallback quality matching. |
| `-w`, `--web` | Starts the embedded Web UI server, overriding the config's default UI setting. |
| `-c`, `--cli` | Launches the interactive terminal shell, overriding `default_ui = "browser"`. |
| `-h`, `--help` | Prints help information and examples. |
| `-V`, `--version` | Prints version information. |

---

## Core Workflows

### 1. Direct Download (Smart Fallback)

Provide a valid URL directly as an argument:

```bash
dwdr "https://www.youtube.com/watch?v=VIDEO_ID"
```

- DWDR extracts available streams without fetching unnecessary playlist items (`--no-playlist`).
- If a format matching your configured `default_quality` (e.g. `1080p`) exists, it begins downloading immediately.
- If the exact quality is unavailable, it automatically prompts you to choose from available streams.

### 2. Interactive Terminal Shell (REPL)

Run `dwdr` with no arguments (when `default_ui = "cli"`):

```bash
dwdr
```

You are greeted with the interactive prompt:

```text
INFO: Welcome to DWDR Interactive Shell. Type a URL to download, or 'exit' to quit.
dwdr >
```

- Enter any supported media URL to inspect and download.
- Type `exit` or `quit` (or press `Ctrl+C`) to terminate the session.

### 3. Web UI Launcher

Launch the local web dashboard directly:

```bash
dwdr --web
```

- Binds to `0.0.0.0:<port>` (default: `5500`).
- Automatically opens your default web browser to `http://localhost:<port>`.

---

## Subcommands

### `dwdr config`

Manage user configuration stored in `~/.dwdr/config.toml`.

#### Interactive Mode
Run without arguments to launch the guided interactive configuration prompt:

```bash
dwdr config
```

#### Show Current Configuration
Display active settings and the physical configuration file path:

```bash
dwdr config show
```

#### Set Configuration Keys Programmatically
Update specific keys from scripts or terminal commands:

```bash
dwdr config set port 8080
dwdr config set default_quality 720p
dwdr config set default_ui browser
dwdr config set download_path /path/to/downloads
```

To bypass validation bounds, append `--force`:

```bash
dwdr config set port 9090 --force
```

---

### `dwdr update`

Forces an immediate redownload and verification of internal engine dependencies (`yt-dlp` and `ffmpeg`) stored in `~/.dwdr/deps/`:

```bash
dwdr update
```
