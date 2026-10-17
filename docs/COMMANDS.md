# Command Reference Catalog

DWDR provides a total of **20 primary commands and subcommands** across three execution contexts: Direct CLI invocation, Subcommand dispatch, and the Interactive REPL Shell.

---

## Command Summary Table

| Command | Execution Context | Arguments / Flags | Purpose | Example |
| :--- | :--- | :--- | :--- | :--- |
| `dwdr <URL>` | Direct CLI | `<URL>` (string) | Downloads media immediately using Smart Quality Fallback without opening interactive prompts. | `dwdr "https://youtu.be/VIDEO_ID"` |
| `dwdr` | Direct CLI | None | Launches the interactive terminal REPL shell for continuous URL inspection and downloading. | `dwdr` |
| `dwdr -h`, `--help` | Direct CLI | None | Prints command syntax, global flags, usage examples, and interactive shell commands. | `dwdr --help` |
| `dwdr -V`, `--version` | Direct CLI | None | Prints the compiled DWDR binary version string. | `dwdr --version` |
| `dwdr config` | Subcommand | None | Launches the guided terminal menu to configure download folders, resolutions, and parallel limits. | `dwdr config` |
| `dwdr config show` | Subcommand | None | Displays current persistent configuration values from `~/.dwdr/config.toml`. | `dwdr config show` |
| `dwdr config set` | Subcommand | `<key>` `<value>` `[--force]` | Programmatically updates a configuration key (`path`, `quality`, `parallel`, `update-days`). | `dwdr config set quality 720p` |
| `dwdr config reset` | Subcommand | `[--force]` | Restores `~/.dwdr/config.toml` to clean factory defaults (prompts confirmation unless `--force`). | `dwdr config reset` |
| `dwdr update` | Subcommand | None | Forces an immediate redownload and verification of internal `yt-dlp` and `ffmpeg` engine binaries. | `dwdr update` |
| `dwdr self-update` | Subcommand | None | Downloads and atomically replaces the running DWDR CLI executable with the latest GitHub release. | `dwdr self-update` |
| `config` / `settings` | Interactive Shell (REPL) | None | Opens the interactive configuration menu directly from within an active shell session. | `dwdr > settings` |
| `status` | Interactive Shell (REPL) | None | Launches an in-place real-time visual progress monitor of active downloads and queued items. | `dwdr > status` |
| `list` | Interactive Shell (REPL) | None | Displays a clean session report of all downloads, verifying physical file existence on disk. | `dwdr > list` |
| `queue` | Interactive Shell (REPL) | None | Inspects queued waiting tasks, ordered queue positions, and parallel concurrency limits. | `dwdr > queue` |
| `queue live` / `queue -l` | Interactive Shell (REPL) | None | Launches an in-place real-time visual monitor of queue slots and active task progress. | `dwdr > queue live` |
| `active` | Interactive Shell (REPL) | None | Displays an instantaneous snapshot of currently downloading tasks without entering live view. | `dwdr > active` |
| `cancel` | Interactive Shell (REPL) | None | Cancels all currently running background downloads and flushes the waiting queue. | `dwdr > cancel` |
| `cls` / `clear` | Interactive Shell (REPL) | None | Clears the terminal screen buffer within an active shell session. | `dwdr > clear` |
| `help` | Interactive Shell (REPL) | None | Displays interactive shell command summary and usage tips. | `dwdr > help` |
| `exit` / `quit` | Interactive Shell (REPL) | None | Cancels active downloads and gracefully exits the interactive shell. | `dwdr > exit` |

---

## Detailed Command Specifications

### 1. Direct Download (`dwdr <URL>`)
- **Behavior**: Validates the URL protocol (`http://` or `https://`), resolves engine binaries in `~/.dwdr/deps/`, extracts metadata via `yt-dlp` with `--no-playlist`, and checks if a stream matches `default_quality`.
- **Smart Fallback**: If an exact match exists, downloading begins automatically. If no exact match exists, prompts the user to select from available audio/video streams.
- **Fallback Directory Prompt**: If `download_path` is not configured, triggers a native OS folder dialog (`rfd`) before writing files.

### 2. Interactive Terminal Shell (`dwdr`)
- **Behavior**: Starts a persistent, non-blocking read-eval-print loop (`dwdr >`).
- **Concurrent Task Queue**:
  - Downloads run concurrently in the background up to `config.max_parallel_downloads` (default: `3`).
  - Submitting more URLs automatically enqueues them into a FIFO queue.
  - As soon as an active download finishes, the next queued download begins automatically.
  - The shell prompt (`dwdr >`) returns immediately after submitting a URL so you can continue typing.
- **Input Handling & Commands**:
  - Entering a valid media URL (`http://` or `https://`) resolves formats and submits the task to the queue worker pool.
  - Typing `status` (or `status live` / `status -l`) launches an in-place live progress monitor that refreshes every 200ms until Enter or `q` is pressed (or all tasks complete).
  - Typing `list` outputs a session history report detailing active progress, completed destinations, queued items, and failure reasons. Completed downloads are checked against the disk; missing files are marked as `COMPLETED (file not found)`.
  - Typing `queue` displays queue slot capacity (`Active Slots: X/Y`) and ordered waiting positions.
  - Typing `queue live` (or `queue -l`) launches an in-place real-time visual monitor of queue slots and active task progress.
  - Typing `active` prints a fast static progress snapshot of currently downloading tasks.
  - Typing unrecognized commands produces helpful guidance to type `help` rather than treating input as an invalid URL.
  - Typing `cancel` stops all running downloads and clears the queue immediately.
  - Typing `config` or `settings` opens the interactive settings menu.
  - Typing `cls` or `clear` clears the terminal screen buffer.
  - Typing `exit`, `quit`, or pressing `Ctrl+C` terminates active tasks and exits the shell.
  - Automated/piped input gracefully falls back from TTY raw-mode to standard line buffered reading.

### 3. Configuration Subcommands (`dwdr config`)
- **Interactive Mode** (`dwdr config`):
  - Option 1: Launches native OS file browser (`rfd`) to select a download directory.
  - Option 2: Selects default resolution (`always ask`, `best`, `8k`, `4k`, `2k`, `1080p`, `720p`, `480p`, `360p`, `240p`, `144p`, `audio`).
  - Option 3: Selects max parallel download threads (`1` to `16`).
  - Option 4: Saves to `~/.dwdr/config.toml` and exits.
  - Option 5: Cancels without saving.
  - In non-TTY environments, exits cleanly with an informative error rather than looping.
- **Programmatic Set** (`dwdr config set <key> <value>`):
  - Supported keys: `path`, `quality`, `parallel`, `update-days`.
  - Validation bounds: `parallel` (`1..=16`), `update-days` (`0..=365`), valid quality tiers, write-permission directory check.
  - Concurrency safety: Rejects mutations when downloads are running in another process (`active_downloads.lock`).
  - Flags: `--force` overrides safety bounds and active download locks.
- **Reset Configuration** (`dwdr config reset`):
  - Restores clean factory defaults. Prompts for `[y/N]` confirmation; `--force` skips confirmation.

### 4. Engine Updater (`dwdr update`)
- **Behavior**: Deletes cached binaries in `~/.dwdr/deps/` and fetches fresh pre-compiled releases from official upstream repositories over TLS (`rustls`).
- **Platform Detection**: Automatically resolves appropriate binary archives for x86_64 and ARM64 on Linux, macOS, and Windows.
- **Resilience**: Respects proxy configuration (`DWDR_PROXY`, `HTTPS_PROXY`, `ALL_PROXY`, `HTTP_PROXY`) and applies exponential backoff retry (up to 3 attempts).

### 5. CLI Self-Updater (`dwdr self-update`)
- **Behavior**: Downloads and replaces the current DWDR binary with the latest release from GitHub.
- **Resilience**: Supports proxy routing and exponential backoff retry.
- **OS-Native File Handling**:
  - Unix (Linux & macOS): Preserves `0o755` executable permissions and atomically replaces the binary without lingering files.
  - Windows: Bypasses file locking by renaming the running executable to `.old` before replacing it, with automated silent cleanup on subsequent launches.
