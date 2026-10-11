# Architecture & Technical Deep Dive

DWDR is an asynchronous Rust application designed to provide reliable media extraction and downloading across multiple platforms with zero external system dependencies.

---

## Core Engineering Principles

### 1. Zero External Dependencies
DWDR isolates its third-party engines into `~/.dwdr/deps/`:
- `yt-dlp`: Automatically fetched on first run directly from official GitHub releases.
- `ffmpeg`: Dynamically detected and linked if present, with graceful audio/video demux fallback.
- The user does not need to install Python, yt-dlp, or ffmpeg globally.

### 2. Single-Video Protection (`--no-playlist`)
Standard YouTube URLs copied from recommendation mixes or radio feeds often contain query parameters like `&list=RD...`. When passed to `yt-dlp` without safeguards, extraction blocks while scraping all 50+ playlist tracks. 

DWDR enforces `--no-playlist` across:
- `fetch_video_info` (metadata extraction)
- `download_media` (synchronous CLI downloads)
- `download_media_background` (asynchronous Web UI downloads)

This guarantees immediate single-video resolution regardless of query parameters.

### 3. Accelerated Download Engine
All download invocations execute with `-N 8`, splitting downloads into 8 concurrent fragmented streams for maximum network throughput.

### 4. Concurrency & State Management
Background download jobs initiated from the Web UI execute in isolated threads. Progress states (`Starting`, `Completed`, `Error: ...`) are registered in a thread-safe global registry guarded by `std::sync::Mutex` in `src/engine/state.rs` and polled via `/api/status`.

### 5. Embedded HTTP Server
Rather than requiring Node.js or a separate frontend runtime, DWDR bundles HTML and JavaScript directly into the compiled binary via `include_str!` macros and serves them using the lightweight `tiny_http` crate.
