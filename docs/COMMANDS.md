# Command Reference Catalog

DWDR provides a total of **12 primary commands and subcommands** across three execution contexts: Direct CLI invocation, Subcommand dispatch, and the Interactive REPL Shell.

---

## Command Summary Table

| Command | Execution Context | Arguments / Flags | Purpose | Example |
| :--- | :--- | :--- | :--- | :--- |
| `dwdr <URL>` | Direct CLI | `<URL>` (string) | Downloads media immediately using Smart Quality Fallback without opening interactive prompts. | `dwdr "https://youtu.be/VIDEO_ID"` |
| `dwdr` | Direct CLI | None | Launches the interactive terminal REPL shell for continuous URL inspection and downloading. | `dwdr` |
| `dwdr -h`, `--help` | Direct CLI | None | Prints command syntax, global flags, and usage examples. | `dwdr --help` |
| `dwdr -V`, `--version` | Direct CLI | None | Prints the compiled DWDR binary version string. | `dwdr --version` |
| `dwdr config` | Subcommand | None | Launches the guided terminal menu to configure download folders, resolutions, and parallel limits. | `dwdr config` |
| `dwdr config show` | Subcommand | None | Displays current persistent configuration values from `~/.dwdr/config.toml`. | `dwdr config show` |
| `dwdr config set` | Subcommand | `<key>` `<value>` `[--force]` | Programmatically updates a configuration key (`path`, `quality`, `parallel`, `update-days`). | `dwdr config set quality 720p` |
| `dwdr config reset` | Subcommand | None | Restores `~/.dwdr/config.toml` to clean factory defaults. | `dwdr config reset` |
| `dwdr update` | Subcommand | None | Forces an immediate redownload and verification of internal `yt-dlp` and `ffmpeg` engine binaries. | `dwdr update` |
| `dwdr self-update` | Subcommand | None | Downloads and atomically replaces the running DWDR CLI executable with the latest GitHub release. | `dwdr self-update` |
| `config` / `settings` | Interactive Shell (REPL) | None | Opens the interactive configuration menu directly from within an active shell session. | `dwdr > settings` |
| `cls` / `clear` | Interactive Shell (REPL) | None | Clears the terminal screen buffer within an active shell session. | `dwdr > clear` |
| `exit` / `quit` | Interactive Shell (REPL) | None | Gracefully exits the interactive shell. | `dwdr > exit` |

---

## Detailed Command Specifications

### 1. Direct Download (`dwdr <URL>`)
- **Behavior**: Validates the URL protocol (`http://` or `https://`), resolves engine binaries in `~/.dwdr/deps/`, extracts metadata via `yt-dlp` with `--no-playlist`, and checks if a stream matches `default_quality`.
- **Smart Fallback**: If an exact match exists, downloading begins automatically. If no exact match exists, prompts the user to select from available audio/video streams.
- **Fallback Directory Prompt**: If `download_path` is not configured, triggers a native OS folder dialog (`rfd`) before writing files.

### 2. Interactive Terminal Shell (`dwdr`)
- **Behavior**: Starts an persistent read-eval-print loop (`dwdr >`).
- **Input Handling**:
  - Entering a valid URL runs the extraction and download sequence.
  - Typing `config` or `settings` opens the interactive settings menu.
  - Typing `cls` or `clear` clears the terminal screen buffer.
  - Typing `exit`, `quit`, or pressing `Ctrl+C` terminates the shell.

### 3. Configuration Subcommands (`dwdr config`)
- **Interactive Mode** (`dwdr config`):
  - Option 1: Launches native OS file browser (`rfd`) to select a download directory.
  - Option 2: Selects default resolution (`always ask`, `best`, `4k`, `2k`, `1080p`, `720p`, `480p`, `audio`).
  - Option 3: Selects max parallel download threads (`1`, `3`, `5`, `8`).
  - Option 4: Saves to `~/.dwdr/config.toml` and exits.
  - Option 5: Cancels without saving.
- **Programmatic Set** (`dwdr config set <key> <value>`):
  - Supported keys: `path`, `quality`, `parallel`, `update-days`.
  - Flags: `--force` overrides safety bounds if downloads are running.

### 4. Engine Updater (`dwdr update`)
- **Behavior**: Deletes cached binaries in `~/.dwdr/deps/` and fetches fresh pre-compiled releases from official upstream repositories over TLS (`rustls`).

### 5. CLI Self-Updater (`dwdr self-update`)
- **Behavior**: Downloads and replaces the current DWDR binary with the latest release from GitHub.
- **OS-Native File Handling**:
  - Unix (Linux & macOS): Preserves `0o755` executable permissions and atomically replaces the binary without lingering files.
  - Windows: Bypasses file locking by renaming the running executable to `.old` before replacing it, with automated silent cleanup on subsequent launches.
