# Configuration Guide

DWDR stores user configuration in an isolated TOML file with automatic self-healing capabilities if the file is missing or corrupted.

---

## File Location

- **Linux / macOS / Termux**: `~/.dwdr/config.toml`
- **Windows**: `%USERPROFILE%\.dwdr\config.toml`

---

## Default Configuration File

```toml
default_ui = "cli"
download_path = "/home/user/Downloads/DWDR"
port = 5500
update_check_days = 0
max_parallel_downloads = 3
default_quality = "1080p"
```

---

## Configuration Keys Reference

| Key | Type | Default | Description | Valid Values |
| --- | --- | --- | --- | --- |
| `default_ui` | string | `"cli"` | Primary interface launched when running `dwdr` without flags. | `"cli"`, `"browser"` |
| `download_path` | string | `~/Downloads/DWDR` | Destination folder where downloaded audio and video files are saved. | Any valid directory path |
| `port` | integer | `5500` | Local port number bound by the embedded Web UI HTTP server. | `1` to `65535` |
| `default_quality` | string | `"1080p"` | Preferred video resolution matched during Smart Fallback downloads. | `"1080p"`, `"720p"`, `"480p"`, `"360p"`, `"best"` |
| `max_parallel_downloads` | integer | `3` | Maximum number of concurrent media tasks allowed simultaneously. | `1` to `16` |
| `update_check_days` | integer | `0` | Frequency in days to check for dependency updates (`0` disables check). | Non-negative integer |

---

## How to Update Configuration

### 1. Via Command Line

```bash
# View all current values
dwdr config show

# Set specific settings
dwdr config set port 8080
dwdr config set default_quality 720p
dwdr config set default_ui browser
```

### 2. Via Interactive CLI Menu

Run `dwdr config` without arguments to step through an interactive selection prompt using arrow keys and text inputs.

### 3. Via Web UI

Open the Web UI (`dwdr --web`), click the gear icon in the top right corner, modify the Download Path or Default Quality, and click **Save**.

### 4. Direct File Editing

You can edit `~/.dwdr/config.toml` directly using any text editor. If syntax errors occur, DWDR automatically restores valid defaults without crashing.
