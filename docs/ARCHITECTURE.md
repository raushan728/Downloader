# Architecture & Technical Deep Dive

DWDR is a high-performance terminal media downloader designed to provide reliable media extraction and downloading across multiple platforms with zero external system dependencies.

---

## Core Engineering Principles

### 1. Zero External Dependencies
DWDR isolates its third-party engines into `~/.dwdr/deps/`:
- `yt-dlp`: Automatically fetched on first run directly from official GitHub releases.
- `ffmpeg`: Dynamically detected and linked if present, with graceful audio/video demux fallback.
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

### 4. Real-Time Terminal Progress Streaming
`download_media` spawns `yt-dlp` with piped stdio and `--newline`. Output streams are parsed in real time to feed speed, percentage, and ETA metrics into an `indicatif::ProgressBar` spinner, providing a responsive terminal interface without polling overhead.

### 5. Native OS File System Integration
Configuration and folder selection workflows integrate with `rfd::FileDialog`, enabling desktop users to browse and select download directories visually while preserving a headless terminal fallback.
