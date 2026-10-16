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
- **Configuration Input Validation**: Added strict validation rules for `dwdr config set` commands: `quality` is bounded to standard resolution tiers (`always ask`, `best`, `8k`, `4k`, `2k`, `1080p`, `720p`, `480p`, `360p`, `240p`, `144p`, `audio`); `parallel` is bounded between `1` and `16`; `update-days` is bounded between `0` and `365`; and `path` requires an existing, writable directory with per-process collision probes and Windows UNC `\\?\` prefix stripping.
- **Cross-Process Download State Guard**: Enhanced `is_download_active()` to check in-process queues, direct download threads, and PID-tracked cross-process lockfiles (`active_downloads.lock`). Blocks concurrent destructive configuration edits across multiple terminals while downloads are in flight unless `--force` is specified.
- **CLI Discovery Side-Effect Prevention**: Deferred configuration file loading in `main.rs` until after `Cli::parse()`, preventing `--help` and `--version` invocations from creating unnecessary `config.toml` files.
- **Headless Interactive Config Safety**: Guarded `interactive_config` against headless/non-TTY execution, returning explicit errors directing users to `dwdr config set` and preventing infinite selection loops.
- **Unprobed Media Codec Support & Generic Fallback**: Fixed format extraction failures on non-YouTube platforms where `vcodec` and `acodec` are returned as `null` or `none`. Media candidates are now recognized via file extension and spatial dimensions, with fallback to format notes or `"Default"` synthesis.
- **Single-Stream Auto-Selection & False Cancellation Fix**: Resolved false `WARNING: Download cancelled by user.` messages when downloading videos with 0 or 1 format available. Single streams now download immediately without redundant interactive prompts.
- **Windows Filesystem Path & Filename Sanitization**: Fixed `[Errno 22] Invalid argument` crashes on Windows by separating output directory (`-P`) from filename template (`-o`), enforcing `--windows-filenames` character sanitization, and clamping filename lengths via `--trim-filenames 160`.
- **Nested Error Formatting**: Stripped duplicate `ERROR:` tags in `extract_engine_error` to ensure clean error messages across CLI and queue workers.
- **Unrecognized Shell Command Handling**: Replaced misleading `Invalid URL provided` errors with guidance to type `help` when unknown commands are entered into the interactive shell.
- **Case-Insensitive Shell URL Detection**: Replaced prefix checks in `src/cli/shell.rs` with `is_valid_url`, ensuring uppercase schemes (`HTTP://...`, `Https://...`) are properly treated as media URLs rather than unrecognized shell commands.
- **Differentiated Audio Command Generation**: Prevented invalid audio-audio merges and empty MP4 video containers by tagging audio formats with an `audio:` prefix in `resolve_media_to_download` and avoiding `+bestaudio/best` and `--merge-output-format mp4` in `build_ytdlp_download_cmd`.
- **8K Quality Matching & Prompt Parity**: Added `"8k"` to interactive configuration prompt options in `src/cli/prompts.rs` and added explicit `target == "8k"` matching in `find_format_by_quality` to recognize both `"4320p"` and `"8K"` streams.
- **Accurate Empty Formats Error Handling**: Eliminated misleading `WARNING: Download cancelled by user.` messages when a media URL provides zero downloadable formats, emitting an explicit `ERROR: No downloadable formats found for this media stream.` instead.
- **Square and VR/360 Video Preservation**: Removed the blanket `w == h` skip guard in `parse_video_info_json` to retain 1:1 aspect ratio videos (Instagram, VR 180/360 SBS/square frames), and updated `resolve_resolution` to map square dimensions directly via `map_height_to_resolution(h)`.
- **Fallback Resolution String Parsing**: Supported video streams lacking integer `width` and `height` properties by parsing the `resolution` string (e.g. `"1920x1080"` or `"720p"`) and falling back to format notes/IDs to prevent collision deduplication.
- **HDR and Modern Codec Prioritization**: Factored HDR detection (`dynamic_range == "HDR"`) and video/audio codec quality scores (AV1 > VP9 > HEVC > AVC; Opus > AAC > MP3) into candidate deduplication rankings in `parse_video_info_json`, ensuring superior visual/audio quality streams are preferred over bloated legacy SDR streams while strictly preserving progressive direct stream priority.
- **Direct Progressive Stream Prioritization**: Fixed format selection logic in `parse_video_info_json` that previously favored HLS adaptive playlists (`m3u8_native`) over direct progressive HTTPS streams due to inflated theoretical bitrate metadata. Direct streams with server-verified filesizes are now strictly prioritized over HLS manifests, restoring exact Megabyte displays on YouTube.
- **HTTP 403 Forbidden Prevention**: Removed aggressive `-N 8` multi-connection fragment flags from `download_media` and `download_media_background`. Sequential progressive streaming prevents triggering YouTube CDN bot-mitigation rate limits and mid-stream HTTP 403 disconnections.
- **Vertical Video Resolution Mapping**: Fixed aspect ratio calculation in `resolve_resolution` for portrait and YouTube Shorts videos (e.g., 1080x1920) so they correctly resolve to `1080p` instead of falling back to horizontal width classifications.
- **Global Environment Variable Race Condition**: Resolved parallel test race conditions caused by process-wide `std::env::set_var` mutations by isolating test file paths directly.

### Changed
- Refactored `download_media` and `download_media_background` to share unified error sanitization via `extract_engine_error`.
- Updated CLI interactive shell prompt and queue status displays to provide real-time task progress metrics and queued slot counters.
