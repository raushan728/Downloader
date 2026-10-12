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
- If no download folder has been configured, a native OS folder dialog (`rfd`) prompts you to select one.

### 2. Interactive Terminal Shell (REPL)

Run `dwdr` with no arguments:

```bash
dwdr
```

You are greeted with the interactive prompt:

```text
INFO: Welcome to DWDR Interactive Shell. Type a URL to download, or 'exit' to quit.
dwdr >
```

- Enter any supported media URL to inspect and download.
- Type `config` or `settings` to launch the interactive settings menu.
- Type `exit` or `quit` (or press `Ctrl+C`) to terminate the session.

---

## Subcommands

### `dwdr config`

Manage user configuration stored in `~/.dwdr/config.toml`.

#### Interactive Mode
Run without arguments to launch the guided interactive configuration prompt:

```bash
dwdr config
```

Features native OS folder picker (`rfd`) and interactive selection menus.

#### Show Current Configuration
Display active settings and the physical configuration file path:

```bash
dwdr config show
```

#### Set Configuration Keys Programmatically
Update specific keys from scripts or terminal commands:

```bash
dwdr config set default_quality 720p
dwdr config set download_path /path/to/downloads
dwdr config set parallel 5
```

To bypass validation bounds, append `--force`:

```bash
dwdr config set parallel 16 --force
```

---

### `dwdr update`

Forces an immediate redownload and verification of internal engine dependencies (`yt-dlp` and `ffmpeg`) stored in `~/.dwdr/deps/`:

```bash
dwdr update
```
