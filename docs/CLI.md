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
| `-h`, `--help` | Prints help information, commands, and examples. |
| `-V`, `--version` | Prints compiled binary version information. |

### Environment Overrides & Proxies

DWDR respects the following environment variables:
- `DWDR_CONFIG_PATH`: Custom path or directory for `config.toml`.
- `DWDR_PROXY`: Top-priority proxy URL for all dependency and binary downloads (e.g., `http://127.0.0.1:8080`).
- `HTTPS_PROXY` / `https_proxy`: Standard HTTPS proxy.
- `ALL_PROXY` / `all_proxy`: General fallback proxy.
- `HTTP_PROXY` / `http_proxy`: Plain HTTP proxy.

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
INFO: Welcome to DWDR Interactive Shell. Type a URL to download, 'status' for live progress, 'help' for commands, or 'exit' to quit.
dwdr >
```

- Enter any supported media URL to inspect, resolve format, and enqueue.
- Downloads run concurrently in the background up to `max_parallel_downloads` without blocking the cursor.
- Tasks exceeding the parallel limit wait in a FIFO queue and start automatically when slots free up.
- Type `status` (or `status live` / `status -l`) to launch the live in-place progress monitor of active downloads and queued counts (press Enter or `q` to exit).
- Type `list` to view a complete session report with active download metrics, completed file locations (verifying physical existence on disk), and failure reasons.
- Type `queue` to inspect queue limits and waiting tasks in FIFO order.
- Type `queue live` (or `queue -l`) to launch an in-place real-time visual monitor of queue slots and active task progress.
- Type `active` to take a fast static snapshot of active downloads without entering live view.
- Type `cancel` to abort running downloads and flush the waiting queue.
- Type `config` or `settings` to launch the interactive settings menu.
- Type `cls` or `clear` to clear the terminal screen buffer.
- Type `help` to display interactive shell command syntax.
- Type `exit` or `quit` (or press `Ctrl+C`) to terminate active tasks and exit the session.

---

## Subcommands

### `dwdr config`

Manage user configuration stored in `~/.dwdr/config.toml`.

#### Interactive Mode
Run without arguments to launch the guided interactive configuration prompt:

```bash
dwdr config
```

Features native OS folder picker (`rfd`) and interactive selection menus. In non-TTY or automated environments, this command exits with an error directing users to `dwdr config set` or direct file editing.

#### Show Current Configuration
Display active settings and the physical configuration file path:

```bash
dwdr config show
```

#### Set Configuration Keys Programmatically
Update specific keys from scripts or terminal commands:

```bash
dwdr config set quality 720p
dwdr config set path /path/to/downloads
dwdr config set parallel 5
dwdr config set update-days 14
```

Validation bounds enforced:
- `quality`: Must be one of `"always ask"`, `"best"`, `"8k"`, `"4k"`, `"2k"`, `"1080p"`, `"720p"`, `"480p"`, `"360p"`, `"240p"`, `"144p"`, `"audio"`.
- `parallel`: Integer between `1` and `16`.
- `update-days`: Integer between `0` and `365` (`0` disables check).
- `path`: Must point to an existing or creatable directory with write permissions.

If a download is actively running in another DWDR process, configuration changes are blocked by `~/.dwdr/active_downloads.lock`. To bypass validation bounds or concurrency locks, append `--force`:

```bash
dwdr config set parallel 16 --force
```

#### Reset Configuration
Restore settings to factory defaults:

```bash
dwdr config reset
```

Prompts for confirmation (`[y/N]`) before resetting. Use `--force` to bypass confirmation in automated scripts:

```bash
dwdr config reset --force
```

---

### `dwdr update`

Forces an immediate redownload and verification of internal engine dependencies (`yt-dlp` and `ffmpeg`) stored in `~/.dwdr/deps/`:

```bash
dwdr update
```

- Platform detection automatically resolves appropriate architecture binaries: Linux x86_64, Linux ARM64 (`aarch64`), macOS x86_64, macOS Apple Silicon ARM64, and Windows x86_64.
- Network requests route through proxy environment variables when configured.
- Implements exponential backoff retry logic (up to 3 attempts with 1s, 2s, 4s delay) on transient network failures.
- Downloads are staged in temporary `.tmp` files and validated before replacing active binaries.

---

### `dwdr self-update`

Updates the DWDR command-line tool itself to the latest release published on GitHub:

```bash
dwdr self-update
```

- Verifies release assets, routes through proxies, and applies retry backoff.
- On Unix, replaces the running binary atomically with executable permissions (`0o755`).
- On Windows, renames the active executable to `.old` to bypass file locking, replacing it with the new binary and cleaning up `.old` on startup.

