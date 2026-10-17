# Agent Guidelines for DWDR

This document establishes engineering conventions, core invariants, architectural guidelines, and future roadmap priorities for automated agents and contributors working on DWDR.

---

## Core Invariants

1. **Zero System Pollution**:
   - Internal dependencies (`yt-dlp` and `ffmpeg`) must reside exclusively within the isolated directory `~/.dwdr/deps/` (or `%USERPROFILE%\.dwdr\deps\` on Windows).
   - Never write to system package directories (`/usr/bin`, `/usr/local/bin`, etc.) or invoke external system package managers (`apt`, `brew`, `choco`).

2. **Single-Video Safeguard**:
   - All `yt-dlp` command builders (`fetch_video_info`, `download_media`) must include `--no-playlist`.
   - Never assume user-provided URLs lack playlist query parameters (`&list=...`).

3. **Output & Tone Discipline**:
   - Do not use emojis in CLI logs, error messages, or internal code comments.
   - Match existing log format tags (`INFO: ...`, `WARNING: ...`, `ERROR: ...`, `SUCCESS: ...`).

4. **Self-Healing Configuration**:
   - `ConfigManager::load()` must always handle missing or unparseable configuration files gracefully by falling back to safe defaults and reserializing without crashing.
   - All directory paths must resolve to a valid file target (e.g. `path.join("config.toml")`) to prevent directory write errors.

5. **Thread Safety & Process Handling**:
   - Media downloads run synchronously with piped stdout streaming into terminal progress indicators.
   - Never block user input loops with unhandled zombie child processes or leaked file descriptors.
   - All spawned worker threads must retain their `JoinHandle` and be gracefully joined before process termination.

6. **Direct Progressive Stream Priority**:
   - When candidate streams are extracted for a given resolution, always prioritize direct progressive HTTPS streams with verified server filesizes over adaptive HLS manifests (`m3u8_native`) to avoid triggering CDN `HTTP Error 403: Forbidden` rate limits.

7. **Process Tree & Signal Isolation**:
   - Child processes on Unix must spawn in isolated process groups (`cmd.process_group(0)`) and be terminated via negative PID group signaling (`kill -9 -<pid>`).
   - On Windows, process termination must use `/F /T` (`taskkill /F /T /PID <pid>`) to sweep all sub-processes (such as spawned `ffmpeg` mergers) and prevent background orphan processes.

8. **Cross-Platform Architecture Awareness**:
   - Binary resolution must inspect both operating system and CPU architecture (`aarch64` / `arm64`, `x86_64`).
   - Support Linux ARM64, macOS Apple Silicon, and Windows ARM64 without assuming generic x86_64 binaries.

9. **Network Resilience & Proxy Routing**:
   - All HTTP client construction must check environment proxies (`DWDR_PROXY`, `HTTPS_PROXY`, `HTTP_PROXY`, `ALL_PROXY`).
   - All dependency and update downloads must implement exponential backoff retry loops for transient network and server errors (HTTP 408, 429, 500, 502, 503, 504), while failing fast on permanent errors (HTTP 404).

10. **Hermetic Offline Testing**:
    - CI and automated integration tests must never issue live external network requests or invoke unmocked external binaries.
    - Centralized mock infrastructure in `tests/common/` (`MockServer`, `MockYtDlpBuilder`, `MockFfmpegBuilder`) must be used for deterministic test coverage.
    - Tests touching global state (`DWDR_CONFIG_PATH`, `GLOBAL_QUEUE`, `active_downloads.lock`) must be annotated with `#[serial]`.

11. **Terminal State Restoration**:
    - Any routine that modifies terminal cursor visibility (`crossterm::cursor::Hide`) or raw mode (`enable_raw_mode`) must guarantee restoration via `disable_raw_mode()` and `cursor::Show`, including when interrupted by `Ctrl+C` or fatal errors.

12. **Top-Level Import Placement**:
    - All imports must reside at the top of the file in the designated import block matching standard grouping (`std`, external crates, local modules).
    - Never insert inline `use` statements inside functions or test bodies.

---

## Future Roadmap & Priorities

When designing new features or extending DWDR, prioritize the following roadmap items:

1. **Bandwidth Throttling (`--limit-rate`)**:
   - Support user-specified download rate limits (e.g. `dwdr --limit-rate 5M <url>` or configurable via `dwdr config set rate-limit 5M`).
   - Propagate `--limit-rate` to `yt-dlp` download command builders.

2. **Dedicated Playlist Subcommand (`dwdr playlist <url>`)**:
   - While single-video mode strictly enforces `--no-playlist`, a dedicated `dwdr playlist` subcommand should support bounded batch downloads with explicit range limits (e.g. `--items 1-10`) and progress tracking per playlist item.

3. **Subtitle Extraction & Embedding (`--subtitles`)**:
   - Optional subtitle extraction flags (`--write-sub`, `--sub-lang <lang>`, `--embed-subs`).
   - Allow users to configure default subtitle language in `config.toml`.

4. **Audio Format Transcoding (`--audio-format`)**:
   - When extracting audio (`--quality audio`), allow specifying target codec/container (`mp3`, `m4a`, `flac`, `wav`, `opus`).
   - Pass appropriate post-processing arguments to `ffmpeg` via `yt-dlp` (`-x --audio-format <format>`).

5. **Authentication & Cookie Integration**:
   - Safe cookie file support (`--cookies <file>` and `--cookies-from-browser <browser>`) to enable downloading age-restricted, private, or subscription-gated media.
   - Store cookie paths securely without leaking sensitive tokens in logs or error traces.

6. **Interrupted Download Resumption**:
   - Leverage HTTP Range requests and temporary staging files (`.part`) to allow resuming interrupted downloads instead of restarting from byte zero when explicitly requested.

---

## Verification Checklist

Every code modification must pass the following checks before being submitted:

```bash
# 1. Enforce official Rust formatting
cargo fmt --check

# 2. Run unit and integration tests
cargo test

# 3. Enforce strict compiler warnings and lints
cargo clippy --all-targets -- -D warnings
```

---

## Commit Guidelines

- Use conventional commits: `type(scope): brief description` (e.g., `feat(engine): ...`, `fix(cli): ...`, `docs: ...`).
- Ensure commits are atomic: exactly one logical change per commit.
