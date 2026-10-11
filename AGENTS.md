# Agent Guidelines for DWDR

This document establishes engineering conventions, core invariants, and workflows for automated agents and contributors working on DWDR.

---

## Core Invariants

1. **Zero System Pollution**:
   - Internal dependencies (`yt-dlp` and `ffmpeg`) must reside exclusively within the isolated directory `~/.dwdr/deps/` (or `%USERPROFILE%\.dwdr\deps\` on Windows).
   - Never write to system package directories (`/usr/bin`, `/usr/local/bin`, etc.) or invoke external system package managers (`apt`, `brew`, `choco`).

2. **Single-Video Safeguard**:
   - All `yt-dlp` command builders (`fetch_video_info`, `download_media`, `download_media_background`) must include `--no-playlist`.
   - Never assume user-provided URLs lack playlist query parameters (`&list=...`).

3. **Output & Tone Discipline**:
   - Do not use emojis in CLI logs, error messages, or internal code comments.
   - Match existing log format tags (`INFO: ...`, `WARNING: ...`, `ERROR: ...`, `SUCCESS: ...`).

4. **Self-Healing Configuration**:
   - `ConfigManager::load()` must always handle missing or unparseable configuration files gracefully by falling back to safe defaults and reserializing without crashing.

5. **Thread Safety**:
   - Background tasks triggered from the Web UI or daemon must register updates via `crate::engine::state::update_task()`.
   - Never hold mutex guards across long-running process waits or network calls.

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
