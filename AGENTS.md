# Agent Guidelines for DWDR

Engineering conventions, core invariants, architectural guidelines, and codebase map for AI coding agents and contributors.

---

## Codebase Map

| Subsystem | Key Files | Responsibility |
| --- | --- | --- |
| **CLI & Entry** | [`src/main.rs`](src/main.rs)<br>[`src/cli/router.rs`](src/cli/router.rs) | CLI entry point, signal handling (`ctrlc`), terminal state cleanup, Clap argument parsing (`[URL]`, `--force`, subcommands). |
| **Interactive Shell** | [`src/cli/shell.rs`](src/cli/shell.rs)<br>[`src/cli/prompts.rs`](src/cli/prompts.rs) | Non-blocking REPL shell, interactive format selection, duplicate conflict resolution dialog, native folder picker dialogs (`rfd`). |
| **Configuration** | [`src/config/manager.rs`](src/config/manager.rs) | Self-healing `DwdrConfig`, atomic file replacement (`NamedTempFile`), pre-download write permission validation, path canonicalization, cross-process lock (`active_downloads.lock`). |
| **Download Engine** | [`src/engine/downloader.rs`](src/engine/downloader.rs) | `yt-dlp` stream extraction, direct progressive stream priority, command builder, progress parsing (`download:PROGRESS`), in-place finalizing transition, duplicate file detection, cancel cleanup. |
| **Task Queue** | [`src/engine/queue.rs`](src/engine/queue.rs) | Concurrent worker thread pool (`max_parallel_downloads`), in-place live progress rendering (`status`, `queue live`), process tree termination (`kill_process_by_pid`). |
| **Paths & Engines** | [`src/engine/paths.rs`](src/engine/paths.rs)<br>[`src/engine/updater.rs`](src/engine/updater.rs) | Isolated dependency resolution (`~/.dwdr/deps/`), executable binary validation, retry with exponential backoff, SHA256 checksums, proxy routing, self-update. |
| **URL Validator** | [`src/utils/validator.rs`](src/utils/validator.rs) | Strict HTTP/HTTPS URL syntax verification. |
| **Test Mocks** | [`tests/common/`](tests/common/) | Centralized offline test infrastructure: `MockServer`, `MockYtDlpBuilder`, `MockFfmpegBuilder`. |
| **Integration Tests** | [`tests/cli.rs`](tests/cli.rs)<br>[`tests/real_integration.rs`](tests/real_integration.rs) | Fast hermetic CLI tests (`tests/cli.rs`) and full edge-case integration suite (`tests/real_integration.rs`, marked `#[ignore]`). |

---

## Core Invariants

1. **Zero System Pollution**:
   - Internal dependencies (`yt-dlp`, `ffmpeg`) reside exclusively within `~/.dwdr/deps/` (or `%USERPROFILE%\.dwdr\deps\` on Windows).
   - Never write to system package directories (`/usr/bin`, `/usr/local/bin`) or invoke external system package managers (`apt`, `brew`, `choco`).

2. **Single-Video Safeguard**:
   - Every `yt-dlp` command builder (`fetch_video_info`, `build_ytdlp_download_cmd_ext`) MUST include `--no-playlist`.
   - Never assume user-provided URLs lack playlist query parameters (`&list=...`).

3. **Output & Tone Discipline**:
   - Do NOT use emojis anywhere in CLI logs, error messages, or internal code comments.
   - Use standard log format tags: `INFO: ...`, `WARNING: ...`, `ERROR: ...`, `SUCCESS: ...`.

4. **Terminal State Restoration**:
   - Any routine modifying terminal cursor visibility (`cursor::Hide`), raw mode (`enable_raw_mode`), or ANSI colors must guarantee restoration via `reset_terminal_state()` on exit, fatal error, or `Ctrl+C`.

5. **Process Tree & Signal Isolation**:
   - Unix: Child processes must spawn in isolated process groups (`cmd.process_group(0)`) and be terminated via negative PID group signaling (`kill -9 -<pid>`).
   - Windows: Process termination must use `taskkill /F /T /PID <pid>` to sweep `yt-dlp` and any spawned `ffmpeg` mergers.
   - Child process stdin must be detached (`Stdio::null()`) to prevent ffmpeg or yt-dlp from blocking on interactive stdin prompts.

6. **Self-Healing Configuration & Atomic Writes**:
   - `ConfigManager::load()` must handle missing or unparseable configuration files gracefully by falling back to safe defaults and reserializing without crashing.
   - Config directory paths must resolve to a valid file target (e.g. `path.join("config.toml")`).
   - Config files are saved atomically using sibling temporary files before replacing the target.

7. **Direct Progressive Stream Priority**:
   - When candidate streams are extracted for a resolution tier, prioritize direct progressive HTTPS streams with verified server filesizes over adaptive HLS manifests (`m3u8_native`) to avoid CDN HTTP 403 Forbidden errors.

8. **Duplicate File Protection & Interruption Cleanup**:
   - Before downloading, check for existing media stems in the download destination via `find_existing_media_file`.
   - Protect existing files unless `--force` is supplied or user chooses Overwrite in interactive mode.
   - When a download is interrupted (`Ctrl+C`), clean up newly created partial/completed media files for that task using exact stem matching to protect original files.

9. **Hermetic Offline Testing**:
   - CI and automated integration tests must never issue live external network requests or invoke unmocked external binaries.
   - Centralized mocks in `tests/common/` (`MockServer`, `MockYtDlpBuilder`, `MockFfmpegBuilder`) must be used for deterministic test coverage.
   - Tests modifying shared process state (`DWDR_CONFIG_PATH`, `GLOBAL_QUEUE`, `active_downloads.lock`) must be annotated with `#[serial]`.

10. **Top-Level Import Placement**:
    - All `use` statements must reside at the top of the file in the designated import block matching standard grouping (`std`, external crates, local modules).
    - Never insert inline `use` statements inside function bodies or test routines.

---

## Verification Checklist

Every code modification must pass the following checks before being submitted:

```bash
# 1. Enforce official Rust formatting
cargo fmt --check

# 2. Run unit and offline integration tests
cargo test

# 3. Enforce strict compiler warnings and lints
cargo clippy --all-targets --all-features -- -D warnings

# 4. Run real end-to-end integration tests (on-demand)
cargo test --test real_integration -- --ignored
```

---

## Commit Guidelines

- Use conventional commits: `type(scope): brief description` (e.g., `feat(downloader): ...`, `fix(cli): ...`, `docs: ...`).
- Ensure commits are atomic: exactly one logical change per commit.
