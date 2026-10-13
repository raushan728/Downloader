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
update_check_days = 0
max_parallel_downloads = 3
default_quality = "1080p"
```

---

## Configuration Keys Reference

| Key | Type | Default | Description | Valid Values |
| --- | --- | --- | --- | --- |
| `download_path` | string | `~/Downloads/DWDR` | Destination folder where downloaded audio and video files are saved. | Any valid directory path |
| `default_quality` | string | `"1080p"` | Preferred video resolution matched during Smart Fallback downloads, or manual prompt. | `"always ask"`, `"best"`, `"4k"`, `"2k"`, `"1080p"`, `"720p"`, `"480p"`, `"audio"` |
| `max_parallel_downloads` | integer | `3` | Maximum number of concurrent media tasks allowed simultaneously. | `1` to `16` |
| `update_check_days` | integer | `0` | Frequency in days to check for dependency updates (`0` disables check). | Non-negative integer |

---

## How to Update Configuration

### 1. Via Command Line

```bash
# View all current values
dwdr config show

# Set specific settings
dwdr config set default_quality 720p
dwdr config set parallel 5
```

### 2. Via Interactive CLI Menu

Run `dwdr config` (or type `config` / `settings` inside the interactive shell) to step through guided prompts with native OS folder selection (`rfd`) and resolution pickers.

### 3. Direct File Editing

You can edit `~/.dwdr/config.toml` directly using any text editor. If syntax errors occur, DWDR automatically restores valid defaults without crashing.
