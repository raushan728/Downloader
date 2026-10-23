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
compat_mode = false
animations = true
auto_watch = true
bell = false
theme = "auto"
```

---

## Configuration Keys Reference

| Key | Type | Default | Description | Valid Values |
| --- | --- | --- | --- | --- |
| `download_path` | string | `~/Downloads/DWDR` | Destination directory where downloaded media files are saved. Must be an existing directory with write permissions. | Valid existing directory path |
| `default_quality` | string | `"1080p"` | Preferred video resolution matched during Smart Fallback downloads, or manual prompt. | `"always ask"`, `"best"`, `"8k"`, `"4k"`, `"2k"`, `"1080p"`, `"720p"`, `"480p"`, `"360p"`, `"240p"`, `"144p"`, `"audio"` |
| `max_parallel_downloads` | integer | `3` | Maximum number of concurrent media downloads allowed simultaneously in interactive mode (additional tasks wait in queue). | `1` to `16` |
| `compat_mode` | boolean | `false` | When enabled, prioritizes AVC (H.264) video and AAC audio codecs over AV1/VP9 for broad player compatibility. | `true`, `false` (also accepts `"1"`/`"0"`, `"yes"`/`"no"`, `"on"`/`"off"`) |
| `animations` | boolean | `true` | Toggles rich terminal animations, Braille spinners, and responsive sub-character progress bar. | `true`, `false` (also accepts `"1"`/`"0"`, `"yes"`/`"no"`, `"on"`/`"off"`) |
| `auto_watch` | boolean | `true` | When enqueuing downloads in the REPL shell, automatically launches the live download monitor. | `true`, `false` (also accepts `"1"`/`"0"`, `"yes"`/`"no"`, `"on"`/`"off"`) |
| `bell` | boolean | `false` | Rings the terminal bell character (`\x07`) upon download completion. | `true`, `false` (also accepts `"1"`/`"0"`, `"yes"`/`"no"`, `"on"`/`"off"`) |
| `theme` | string | `"auto"` | Terminal character set mode: auto-detected, Unicode (box-drawing, Braille, block glyphs), or ASCII fallback. | `"auto"`, `"unicode"`, `"ascii"` |

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
# View all current persistent configuration values
dwdr config show

# Set specific settings (enforces validation bounds; supports aliases)
dwdr config set quality 720p
dwdr config set parallel 5
dwdr config set path /media/downloads
dwdr config set compat true
dwdr config set animations false
dwdr config set theme unicode
dwdr config set bell true

# Force update bypassing active download concurrency locks
dwdr config set parallel 16 --force
# (Note: --force overrides active download locks only; validation bounds remain enforced)

# Reset configuration to defaults (prompts for confirmation [y/N] unless --force is passed)
dwdr config reset
dwdr config reset --force
```

### 2. Via Interactive CLI Menu

Run `dwdr config` (or type `config` / `settings` inside the interactive shell) to step through guided prompts with native OS folder selection (`rfd`) and resolution pickers.

In non-TTY or headless environments (e.g., automated scripts, CI), `dwdr config` fails fast with an error message instructing the user to use `dwdr config set` or edit the file directly, preventing interactive infinite loops.

### 3. Direct File Editing

You can edit `~/.dwdr/config.toml` directly using any text editor. If syntax errors occur, DWDR automatically restores valid defaults without crashing.
