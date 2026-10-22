# Configuration Guide

DWDR stores user configuration in an isolated TOML file with automatic self-healing capabilities if the file is missing or corrupted.

---

## File Location

- **Linux / macOS**: `~/.dwdr/config.toml`
- **Windows**: `%USERPROFILE%\.dwdr\config.toml`
- **Environment Override**: Set `DWDR_CONFIG_PATH` to specify a custom configuration file destination (useful for isolated CI or testing environments).

---

## Default Configuration File

```toml
download_path = "/home/user/Downloads/DWDR"
max_parallel_downloads = 3
default_quality = "1080p"
```

---

## Configuration Keys Reference

| Key | Type | Default | Description | Valid Values |
| --- | --- | --- | --- | --- |
| `download_path` | string | `~/Downloads/DWDR` | Destination directory where downloaded media files are saved. Verified for write permissions. | Valid directory path |
| `default_quality` | string | `"1080p"` | Preferred video resolution matched during Smart Fallback downloads, or manual prompt. | `"always ask"`, `"best"`, `"8k"`, `"4k"`, `"2k"`, `"1080p"`, `"720p"`, `"480p"`, `"360p"`, `"240p"`, `"144p"`, `"audio"` |
| `max_parallel_downloads` | integer | `3` | Maximum number of concurrent media downloads allowed simultaneously in interactive mode (additional tasks wait in queue). | `1` to `16` |

---

## Environment Variables

### Custom Configuration Path (`DWDR_CONFIG_PATH`)
Set `DWDR_CONFIG_PATH` to specify a custom configuration destination:
- If pointing to an existing directory, DWDR resolves to `<directory>/config.toml`.
- Relative paths are automatically resolved to absolute paths.
- Leading and trailing whitespace is trimmed. Empty or whitespace-only values are ignored and fall back to the default path.
- Parent directories are automatically created if they do not exist.

### Custom Engine Binary Paths (`DWDR_YTDLP_PATH`, `DWDR_FFMPEG_PATH`)
Set these variables to override internal engine resolution and use custom external binaries:
- `DWDR_YTDLP_PATH`: Full path to custom `yt-dlp` executable.
- `DWDR_FFMPEG_PATH`: Full path to custom `ffmpeg` executable.
When set, DWDR uses the specified executable paths directly without downloading or validating internal binaries in `~/.dwdr/deps/`, which also enables hermetic offline test suites.

### Network Proxy Routing
DWDR inspects standard proxy environment variables in the following order of precedence:
1. `DWDR_PROXY` (application-specific override)
2. `HTTPS_PROXY` / `https_proxy` (secure HTTPS requests)
3. `ALL_PROXY` / `all_proxy` (general network traffic)
4. `HTTP_PROXY` / `http_proxy` (plain HTTP requests)

When set, all binary dependency downloads (`yt-dlp`, `ffmpeg`) and self-updates are routed through the configured HTTP, HTTPS, or SOCKS5 proxy (e.g. `socks5://127.0.0.1:1080` or `socks5h://...`).

---

## Concurrency Protection & Locks

To prevent race conditions and configuration corruption, DWDR tracks active downloads using an isolated lock file (`~/.dwdr/active_downloads.lock`):
- Modifying settings via `dwdr config set` while another DWDR process is downloading is rejected.
- To bypass this lock, pass `--force`.
- DWDR validates PID liveness to automatically discard stale locks from terminated processes.

---

## How to Update Configuration

### 1. Via Command Line

```bash
# View all current values and file path
dwdr config show

# Set specific settings (enforces validation bounds)
dwdr config set quality 720p
dwdr config set parallel 5
dwdr config set path /media/downloads

# Force update bypassing validation and concurrency locks
dwdr config set parallel 16 --force

# Reset configuration to defaults (prompts for confirmation [y/N] unless --force is passed)
dwdr config reset
dwdr config reset --force
```

### 2. Via Interactive CLI Menu

Run `dwdr config` (or type `config` / `settings` inside the interactive shell) to step through guided prompts with native OS folder selection (`rfd`) and resolution pickers.

In non-TTY or headless environments (e.g., automated scripts, CI), `dwdr config` fails fast with an error message instructing the user to use `dwdr config set` or edit the file directly, preventing interactive infinite loops.

### 3. Direct File Editing

You can edit `~/.dwdr/config.toml` directly using any text editor. If syntax errors occur, DWDR automatically restores valid defaults without crashing.
