# Architecture & Technical Deep Dive

DWDR is a high-performance terminal media downloader designed to provide reliable media extraction and downloading across multiple platforms with zero external system dependencies.

---

## Core Engineering Principles

### 1. Zero External Dependencies & Multi-Architecture Detection
DWDR isolates its third-party engines into `~/.dwdr/deps/`:
- `yt-dlp`: Automatically fetched on first run directly from official GitHub releases. Supports Linux x86_64, Linux ARM64 (`yt-dlp_linux_aarch64`), macOS (universal), and Windows x86_64 (`yt-dlp.exe`).
- `ffmpeg`: Automatically fetched on first run from official prebuilt releases into `~/.dwdr/deps/` for media merging. Supports Linux x86_64, Linux ARM64 (`ffmpeg-4.4.1-linux-arm-64.zip`), macOS x86_64, macOS Apple Silicon ARM64 (`ffmpeg-darwin-arm64`), and Windows x86_64.
- No global Python, yt-dlp, or FFmpeg installations required. Users and test suites can override binary locations via `DWDR_YTDLP_PATH` and `DWDR_FFMPEG_PATH`.
- TLS networking is powered by `rustls` instead of native OpenSSL, guaranteeing seamless cross-compilation across desktop environments.
- Network requests automatically route through proxy configurations (`DWDR_PROXY`, `HTTPS_PROXY`, `ALL_PROXY`, `HTTP_PROXY`), supporting HTTP, HTTPS, and SOCKS5 endpoints.
- Exponential backoff retry (up to 3 attempts with 1s, 2s, 4s delay) covers both initial connection (30s connect timeout) and full body streaming on transient network errors.
- Dependency binaries are validated for integrity on startup (`is_valid_executable` checks executable magic headers, file size, and runs `--version`). Corrupted or partial binaries are automatically healed.

### 2. Single-Video Protection (`--no-playlist`)
Standard YouTube URLs copied from recommendation mixes or radio feeds often contain query parameters like `&list=RD...`. When passed to `yt-dlp` without safeguards, extraction blocks while scraping all 50+ playlist tracks. 

DWDR enforces `--no-playlist` across:
- `fetch_video_info` (metadata extraction)
- `download_media` (stream downloading)

This guarantees immediate single-video resolution regardless of query parameters.

### 3. Direct Progressive Stream Prioritization
DWDR inspects format metadata and strictly prioritizes direct progressive streams (HTTPS byte ranges with server-verified filesizes) over adaptive HLS manifests (`m3u8_native`). This ensures maximum throughput without triggering anti-scraping rate limits or CDN `HTTP Error 403: Forbidden` errors, falling back to multi-tier dynamic estimation only for platforms lacking explicit size metadata.

### 4. Real-Time Responsive Visual Progress Bar & MP4 Merging
`download_media` spawns `yt-dlp` with piped stdout and stderr, `--newline`, and structured `--progress-template` arguments. DWDR captures process streams to eliminate background engine noise (webpage extraction, stream notices, and intermediate file deletions) while draining stderr in a dedicated worker thread. The stream is parsed in real time into an active in-place visual progress bar displaying completion percentage, downloaded and total file sizes (e.g. `[ 16.74MiB / 83.68MiB ]`), download speed, and ETA. The visual progress bar dynamically adapts to terminal dimensions, scaling bar width down on standard 80-column terminals and collapsing to compact/ultra-compact layouts on narrow split panes (< 75 columns) to prevent line wrapping and cursor tearing. When downloading concludes, DWDR transitions cleanly to a finalizing phase for audio/video merging via `--merge-output-format mp4`, ensuring clean single-file outputs named after the media title.

### 5. Native OS File System Integration
Configuration and folder selection workflows integrate with `rfd::FileDialog`, enabling desktop users to browse and select download directories visually while preserving a headless terminal fallback. Target download paths are pre-validated before downloading starts (`validate_download_path`) using temporary probe files to ensure writable permissions and clean failure reporting.

### 6. Concurrent Worker Pool & FIFO Task Queue
In interactive shell mode, DWDR orchestrates multi-media downloads via `QueueManager`:
- **Worker Concurrency Limit**: Active downloads running simultaneously are bounded by `config.max_parallel_downloads` (enforced between `1` and `16`).
- **FIFO Waiting Queue**: Tasks exceeding parallel capacity enter a waiting queue; as worker threads finish, queued tasks automatically transition to active downloading.
- **Process Group Isolation & Clean Cancellation**: Subprocesses on Unix are spawned in an isolated process group (`process_group(0)`). On cancellation or shell exit, DWDR terminates the entire process group using negative PID signals (`kill -9 -<pid>`), guaranteeing that intermediate child processes (e.g., `ffmpeg` mergers spawned by `yt-dlp`) cannot be orphaned. On Windows, `taskkill /F /T /PID <pid>` terminates the full child process hierarchy.
- **State Protection**: `is_download_active()` queries the global queue and cross-process lock (`active_downloads.lock`) to prevent destructive settings edits while downloads are in flight unless overridden by `--force`.

### 7. Signal Handling & Zero System Debris
DWDR registers cross-platform interrupt signal handlers (`ctrlc`) across both direct CLI and interactive shell workflows:
- **Interruption Trapping**: Traps `SIGINT` (Unix) and `CTRL_C_EVENT` (Windows) during active downloads with re-entrancy protection (`AtomicBool` latch) and poisoned mutex recovery.
- **Process Tree Killing**: Child processes (`yt-dlp` and `ffmpeg`) are forcefully terminated immediately.
- **Partial File Deletion**: Scans the target download directory to purge all `.part`, `.ytdl`, `.temp`, and unmerged intermediate stream chunks (`.f<id>.*`), preventing broken or orphaned partial files from lingering on user storage.

### 8. Windows Path Separation & Filename Sanitization
To prevent filesystem failures and `[Errno 22] Invalid argument` errors on Windows:
- **Separated Path and Output Template**: `yt-dlp` commands pass the download folder explicitly via `-P <download_dir>` and the filename template via `-o "%(title)s.%(ext)s"`. This avoids mixing Windows backslashes with forward slashes inside a single `-o` string.
- **Windows-Compatible Character Sanitization**: `--windows-filenames` forces sanitization of forbidden NTFS characters, reserved names (`CON`, `PRN`, `AUX`), and accented unicode characters across all environments.
- **Path Length Clamping**: `--trim-filenames 160` restricts generated filename lengths to prevent exceeding the standard 260-character Windows `MAX_PATH` limit.

### 9. Real-Time In-Place Shell Monitoring & Automation Fallback
Interactive shell commands provide differentiated observability into background downloads:
- **`status` & `queue live` Live View**: Dynamically refreshes in-place using ANSI cursor controls (`MoveUp` and `Clear`) at 200ms intervals, giving real-time feedback without terminal flicker, returning cleanly to the REPL upon pressing Enter or task completion.
- **Non-TTY Automation Fallback**: Both the interactive prompt (`read_shell_input`), the status monitor, and configuration menus detect terminal capabilities via `is_terminal()`. In automated pipelines, tests, or piped stdin scripts, the shell falls back to buffered line reading, static snapshot outputs, or fails fast without blocking or hanging.
- **Session Reporting (`list`)**: Summarizes full task lifecycles (active progress, completed file locations, failed error details) and verifies physical existence on disk.

### 10. Self-Healing Configuration & Concurrency Protection
Configuration management (`ConfigManager`) provides robust multi-process and error recovery guarantees:
- **Atomic Persistence**: Configurations write to a sibling temporary file (`config.toml.tmp`), flush with `sync_all()`, and atomically replace the destination file to prevent partial or corrupted settings on sudden interruption.
- **Directory Path Resolution**: If `DWDR_CONFIG_PATH` points to a directory, it automatically resolves to `<directory>/config.toml`.
- **Validation Bounds**: CLI commands enforce bounds on values (`parallel 1..=16`, `update_check_days 0..=365`, valid quality resolution tiers).
- **Concurrency Locks**: An isolated lock file (`~/.dwdr/active_downloads.lock`) tracks active download PIDs across processes, preventing race conditions or concurrent overwrite of user settings unless explicitly bypassed with `--force`.

### 11. Hermetic Testing Harness & Multi-OS CI Matrix
DWDR maintains strict hermetic testing practices:
- **Mock Binary Injection**: The test suite in `tests/common/` constructs mock `yt-dlp` and `ffmpeg` binary shims injected via `DWDR_YTDLP_PATH` and `DWDR_FFMPEG_PATH` environment overrides for deterministic, offline testing without network calls or external binary downloads.
- **Cross-Platform PR Validation**: The GitHub Actions PR workflow validates all commits across a three-OS matrix (`ubuntu-latest`, `macos-latest`, `windows-latest`), enforcing compilation, test execution, formatting (`cargo fmt`), and strict lints (`cargo clippy -- -D warnings`).
