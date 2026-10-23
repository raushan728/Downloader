# Changelog

All notable changes to the DWDR project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [0.1.0] - 2026-10-24

### Added

- **Direct & Interactive Media Downloading**: Single-command downloads (`dwdr <URL>`) with Smart Quality Fallback, and an interactive REPL shell (`dwdr`) with real-time progress streaming.
- **Concurrent Task Queue**: Multi-download worker pool bounded by `max_parallel_downloads` with FIFO queue scheduling, slot tracking, and live monitoring (`status`, `queue live`).
- **Zero External Dependencies**: Isolated runtime engines (`yt-dlp` and `ffmpeg`) stored in `~/.dwdr/deps/` with multi-architecture detection (x86_64 and ARM64 on Linux, macOS, and Windows).
- **Modular Terminal UI Subsystem (`src/ui`)**: Pure renderers with Unicode and ASCII themes (`ThemeMode`), automatic color depth detection (`ColorLevel`), and plain-mode linear output fallback.
- **Display Sanitization & Escape-Sequence Protection (`ui::text`)**: Security sanitization stripping ANSI escape sequences, OSC/CSI controls, and C1 bytes from untrusted media titles and engine errors.
- **Time-Based Speed Smoothing & Sparkline Visuals (`ui::smooth`, `ui::progress`)**: EMA speed smoother with fixed-interval sampling, 0-to-max sparkline scaling with baseline glyphs, and eased progress interpolation.
- **Dynamic Formatting & Responsive Cards (`ui::card`, `ui::format_table`)**: Responsive completion cards and auto-wrapping error cards with context-aware troubleshooting hints (HTTP 429, bot checks, disk space, geo-blocks), plus responsive column truncation for format selection tables.
- **Live Queue Dashboard & Header Metrics (`engine::queue`)**: In-place status monitoring with synchronized terminal output sequences (`\x1b[?2026h`), frame deduplication, aggregate download speeds, and non-blocking cancellation confirmation dialog.
- **Structured Numeric Progress Parsing (`engine::downloader`)**: Numeric progress template support with combined video/audio progress computation and distinct phase step transitions.
- **Terminal Title & Taskbar Progress Safeguards**: OSC 0 title and taskbar progress writes deduplicated, sanitized against control injection, and restored on exit or interruption.
- **Terminal State Restoration & Clean Redraws**: Robust terminal cleanup on error, signal, and exit, preventing line wrapping and broken raw mode.
- **New CLI Flags & Configuration Keys**: `--no-animation` and `-q`/`--quiet` CLI flags; `animations`, `auto_watch`, `bell`, and `theme` configuration keys with lenient boolean parsing (`true`/`false`, `yes`/`no`, `on`/`off`, `1`/`0`).
- **Network Resilience & Proxy Routing**: Proxy support (`DWDR_PROXY`, `HTTPS_PROXY`, `ALL_PROXY`, `HTTP_PROXY`) and exponential backoff retry on transient network errors.
- **Process Group Isolation & Clean Signal Handling**: Unix process group isolation (`process_group(0)`) and Windows process tree termination (`taskkill /F /T`) to cleanly terminate subprocesses and purge partial files on cancellation.
- **Dynamic Filesize Estimation & Responsive Terminal UI**: Multi-tier filesize estimation for platforms lacking explicit metadata, and terminal progress bars that scale dynamically with window width.
- **Duplicate Collision Protection & Overwrite Controls**: Media collision prevention via exact and normalized stem detection, interactive duplicate resolution dialogs (overwrite or save as unique copy), and `--force` (`-f`) flag for automated overwrites and configuration concurrency lock overrides.
- **Self-Healing Configuration**: TOML configuration with validation bounds, custom path directory resolution, cross-process download locks, and non-TTY safety guards.
- **Hermetic Testing Suite**: Offline test suite with centralized mock binary harnesses covering CLI subcommands, queue concurrency, and engine error recovery.
