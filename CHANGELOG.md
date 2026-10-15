# Changelog

All notable changes to the DWDR project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [0.1.0] - 2026-10-15

### Added
- **Multi-Tier Filesize Calculation**: Added dynamic 5-tier size resolution engine (`calculate_media_filesize`) supporting exact server sizes, approximate engine values, bitrate-duration extrapolation, standard resolution bitrate profiles, and stream pending fallbacks. Formats without server metadata (Instagram, Facebook, X) now display user-friendly estimates (`~X.X MB (Est.)` or `Stream Size Pending`) rather than ambiguous unknown values.
- **Responsive Terminal Progress Bar**: Added dynamic column width adaptation (`render_progress_bar_with_width` and `get_terminal_width`) via `crossterm`. Automatically scales progress bar width on 80-column displays and collapses to clean compact formats on narrow split panes (< 75 columns), eliminating terminal line-wrapping and cursor tearing.
- **Modular Engine Error Extraction**: Decoupled engine stderr parsing into `extract_engine_error` to reliably isolate and format HTTP 403 Forbidden, HTTP 429 Too Many Requests, and video unavailability errors across all download flows.
- **Offline Engine Mock Fixtures**: Extracted `parse_video_info_json` from process spawning to allow unit testing of metadata parsing and engine failure modes offline without requiring external network connectivity.
- **Signal Handling & Partial File Cleanup**: Integrated cross-platform interrupt handler (`ctrlc`) that traps `SIGINT` (Unix) and `CTRL_C_EVENT` (Windows). Automatically purges incomplete `.part`, `.ytdl`, `.temp`, and unmerged intermediate stream chunks (`.f<id>.*`) from target directories upon user cancellation.
- **Concurrent Worker Pool & FIFO Download Queue**: Integrated `QueueManager` into the interactive REPL shell. Supports background concurrent downloads bounded by `config.max_parallel_downloads`, automated FIFO queue scheduling for excess tasks, and shell inspection commands (`queue`, `status`, `list`, `cancel`).
- **Real-Time Interactive Status Monitor & Differentiated Commands**: Added live in-place terminal status monitor (`status`) refreshing at 200ms intervals, comprehensive session history report (`list`), queue position inspector (`queue`), and fast active snapshot (`active`).
- **Non-TTY Automation & Integration Test Harness**: Added terminal detection fallback in `read_shell_input` allowing scripted and piped input to execute without blocking, backed by automated integration tests.
- **Interactive Shell Help Documentation**: Documented all interactive shell commands directly within `dwdr --help` and the shell startup banner.
- **Thread-Safe Configuration Testing**: Added `ConfigManager::load_from_path` for deterministic, race-free self-healing configuration verification across multi-threaded test runners.
- **Comprehensive Test Suite**: Expanded test coverage to 47 automated tests (34 unit tests and 13 integration tests) covering vertical video aspect ratios, queue cancellation, CLI subcommands, progress parsing, and simulated engine mock errors.

### Fixed
- **Unprobed Media Codec Support & Generic Fallback**: Fixed format extraction failures on non-YouTube platforms where `vcodec` and `acodec` are returned as `null` or `none`. Media candidates are now recognized via file extension and spatial dimensions, with fallback to format notes or `"Default"` synthesis.
- **Single-Stream Auto-Selection & False Cancellation Fix**: Resolved false `WARNING: Download cancelled by user.` messages when downloading videos with 0 or 1 format available. Single streams now download immediately without redundant interactive prompts.
- **Windows Filesystem Path & Filename Sanitization**: Fixed `[Errno 22] Invalid argument` crashes on Windows by separating output directory (`-P`) from filename template (`-o`), enforcing `--windows-filenames` character sanitization, and clamping filename lengths via `--trim-filenames 160`.
- **Nested Error Formatting**: Stripped duplicate `ERROR:` tags in `extract_engine_error` to ensure clean error messages across CLI and queue workers.
- **Unrecognized Shell Command Handling**: Replaced misleading `Invalid URL provided` errors with guidance to type `help` when unknown commands are entered into the interactive shell.
- **Direct Progressive Stream Prioritization**: Fixed format selection logic in `parse_video_info_json` that previously favored HLS adaptive playlists (`m3u8_native`) over direct progressive HTTPS streams due to inflated theoretical bitrate metadata. Direct streams with server-verified filesizes are now strictly prioritized over HLS manifests, restoring exact Megabyte displays on YouTube.
- **HTTP 403 Forbidden Prevention**: Removed aggressive `-N 8` multi-connection fragment flags from `download_media` and `download_media_background`. Sequential progressive streaming prevents triggering YouTube CDN bot-mitigation rate limits and mid-stream HTTP 403 disconnections.
- **Vertical Video Resolution Mapping**: Fixed aspect ratio calculation in `resolve_resolution` for portrait and YouTube Shorts videos (e.g., 1080x1920) so they correctly resolve to `1080p` instead of falling back to horizontal width classifications.
- **Global Environment Variable Race Condition**: Resolved parallel test race conditions caused by process-wide `std::env::set_var` mutations by isolating test file paths directly.

### Changed
- Refactored `download_media` and `download_media_background` to share unified error sanitization via `extract_engine_error`.
- Updated CLI interactive shell prompt and queue status displays to provide real-time task progress metrics and queued slot counters.
