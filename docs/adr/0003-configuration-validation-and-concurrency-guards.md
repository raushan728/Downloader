# ADR-0003: Configuration Validation, Directory Resolution, and Concurrency Guards

**Date**: 2026-10-16  
**Status**: Accepted  

## Context

During early development, configuration persistence and validation presented several issues:
1. **Unbounded Input**: Commands like `dwdr config set parallel 0` or invalid quality strings were stored directly, corrupting `config.toml`.
2. **Directory Configuration Paths**: Pointing `DWDR_CONFIG_PATH` to an existing directory caused self-healing saves to fail silently because `save_config` expected a file target.
3. **Headless Prompt Loops**: Invoking `dwdr config` in automated or non-TTY environments triggered infinite selection loops due to raw terminal unavailability.
4. **Cross-Process Concurrency Races**: Multiple DWDR processes in separate terminals could overwrite settings while background downloads were in flight.
5. **Mid-Stream Filesystem Failures**: Spawning downloads into read-only, non-existent, or invalid directories resulted in raw OS errors (`[Errno 13]`, `[Errno 2]`) mid-download.

## Decision

1. **Strict Input Validation and Bounds**:
   - `quality` must match supported resolution profiles (`always ask`, `best`, `8k`, `4k`, `2k`, `1080p`, `720p`, `480p`, `360p`, `240p`, `144p`, `audio`).
   - `parallel` is bounded between `1` and `16`.
   - `update-days` is bounded between `0` and `365`.
   - `dwdr config reset` prompts for `[y/N]` confirmation unless `--force` is provided.

2. **Directory Configuration Resolution**:
   - If a configured or environment path points to a directory (`path.is_dir()`), automatically resolve to `path.join("config.toml")`.
   - Default download path falls back to absolute path resolution (`std::env::current_dir()`), avoiding relative path issues in minimal environments.

3. **Headless Execution Guard**:
   - In non-TTY environments, `dwdr config` fails fast with an error directing users to `dwdr config set` or manual file editing.

4. **Cross-Process Concurrency Lock**:
   - Active downloads record their process IDs in `active_downloads.lock`.
   - Configuration edits inspect active PIDs via `is_pid_alive`, rejecting modifications while downloads are in flight unless overridden by `--force`. Stale locks from killed processes are automatically purged.

5. **Pre-Flight Path Validation**:
   - Validate destination directories with a temporary write probe (`.dwdr_write_test_<pid>.tmp`) via `validate_download_path` before spawning engine processes.
   - Strip Windows UNC `\\?\` prefixes for command compatibility.

## Consequences

- Prevents corrupted settings from being written to disk.
- Protects active downloads across multiple terminals from destructive configuration edits.
- Fails fast with clear error messages before network or child process resources are consumed.
