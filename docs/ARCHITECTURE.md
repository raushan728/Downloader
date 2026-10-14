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

### 3. Accelerated Download Engine
All download invocations execute with `-N 8`, splitting downloads into 8 concurrent fragmented streams for maximum network throughput.

### 4. Real-Time Visual Progress Bar & MP4 Merging
`download_media` spawns `yt-dlp` with piped stdout and stderr, `--newline`, and structured `--progress-template` arguments. DWDR captures process streams to eliminate background engine noise (webpage extraction, stream notices, and intermediate file deletions) while draining stderr in a dedicated worker thread. The stream is parsed in real time into an active in-place visual progress bar displaying completion percentage, downloaded and total file sizes (e.g. `[ 16.74MiB / 83.68MiB ]`), download speed, and ETA. When downloading concludes, DWDR transitions cleanly to a finalizing phase for audio/video merging via `--merge-output-format mp4`, ensuring clean single-file outputs named after the media title.

### 5. Native OS File System Integration
Configuration and folder selection workflows integrate with `rfd::FileDialog`, enabling desktop users to browse and select download directories visually while preserving a headless terminal fallback.
