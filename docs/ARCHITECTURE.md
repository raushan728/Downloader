# Architecture & Technical Deep Dive

DWDR is a high-performance terminal media downloader designed to provide reliable media extraction and downloading across multiple platforms with zero external system dependencies.

---

## Core Engineering Principles

### 1. Zero External Dependencies
DWDR isolates its third-party engines into `~/.dwdr/deps/`:
- `yt-dlp`: Automatically fetched on first run directly from official GitHub releases.
- `ffmpeg`: Automatically fetched on first run from official prebuilt releases into `~/.dwdr/deps/` for media merging.
- No global Python, yt-dlp, or FFmpeg installations required.
- TLS networking is powered by `rustls` instead of native OpenSSL, guaranteeing seamless cross-compilation across desktop environments.

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
Configuration and folder selection workflows integrate with `rfd::FileDialog`, enabling desktop users to browse and select download directories visually while preserving a headless terminal fallback.

### 6. Concurrent Worker Pool & FIFO Task Queue
In interactive shell mode, DWDR orchestrates multi-media downloads via `QueueManager`:
- **Worker Concurrency Limit**: Active downloads running simultaneously are bounded by `config.max_parallel_downloads`.
- **FIFO Waiting Queue**: Tasks exceeding parallel capacity enter a waiting queue; as worker threads finish, queued tasks automatically transition to active downloading.
- **Process Isolation & Clean Cancellation**: Worker threads track child process IDs (`u32`). On shell exit, `Ctrl+C`, or explicit `cancel`, DWDR purges the waiting queue and executes atomic process tree termination (`taskkill /F /T` on Windows, `kill -9` on Unix), eliminating zombie background processes.
- **State Protection**: `is_download_active()` queries the global queue to prevent destructive settings edits while downloads are in flight unless overridden by `--force`.

### 7. Signal Handling & Zero System Debris
DWDR registers cross-platform interrupt signal handlers (`ctrlc`) across both direct CLI and interactive shell workflows:
- **Interruption Trapping**: Traps `SIGINT` (Unix) and `CTRL_C_EVENT` (Windows) during active downloads.
- **Process Tree Killing**: Child processes (`yt-dlp` and `ffmpeg`) are forcefully terminated immediately.
- **Partial File Deletion**: Scans the target download directory to purge all `.part`, `.ytdl`, `.temp`, and unmerged intermediate stream chunks (`.f<id>.*`), preventing broken or orphaned partial files from lingering on user storage.

### 8. Windows Path Separation & Filename Sanitization
To prevent filesystem failures and `[Errno 22] Invalid argument` errors on Windows:
- **Separated Path and Output Template**: `yt-dlp` commands pass the download folder explicitly via `-P <download_dir>` and the filename template via `-o "%(title)s.%(ext)s"`. This avoids mixing Windows backslashes with forward slashes inside a single `-o` string.
- **Windows-Compatible Character Sanitization**: `--windows-filenames` forces sanitization of forbidden NTFS characters, reserved names (`CON`, `PRN`, `AUX`), and accented unicode characters across all environments.
- **Path Length Clamping**: `--trim-filenames 160` restricts generated filename lengths to prevent exceeding the standard 260-character Windows `MAX_PATH` limit.

### 9. Real-Time In-Place Shell Monitoring & Automation Fallback
Interactive shell commands provide differentiated observability into background downloads:
- **`status` Live View**: Dynamically refreshes in-place using ANSI cursor controls (`MoveUp` and `Clear`) at 200ms intervals, giving real-time feedback without terminal flicker, returning cleanly to the REPL upon pressing Enter or task completion.
- **Non-TTY Automation Fallback**: Both the interactive prompt (`read_shell_input`) and the status monitor detect terminal capabilities via `is_terminal()`. In automated pipelines, tests, or piped stdin scripts, the shell falls back to buffered line reading and static snapshot outputs without blocking or hanging.
- **Session Reporting (`list`) & Queue Inspection (`queue`)**: `list` summarizes full task lifecycles (active progress, completed file locations, failed error details) while `queue` exposes waiting positions and worker slots.
