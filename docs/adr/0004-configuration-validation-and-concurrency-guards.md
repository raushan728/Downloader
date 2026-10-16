# ADR-0004: Configuration Validation, Error Propagation, and Concurrency Guards

**Date**: 2026-10-15
**Status**: accepted
**Deciders**: DWDR Core Team

## Context

Prior to this change, several configuration edge cases and vulnerabilities existed across DWDR:
1. `dwdr config set quality` allowed arbitrary invalid strings.
2. `dwdr config set parallel` accepted out-of-bound values (`0`, `17`, `255`).
3. `dwdr config set path` accepted non-existent, non-directory, or un-writable paths.
4. `dwdr config set update-days` accepted unreasonably high numbers without bound checking.
5. `dwdr config reset` lacked a confirmation prompt and bypassed the `--force` flag.
6. `DWDR_CONFIG_PATH` accepted empty/whitespace strings and failed on relative paths or empty parents.
7. `DWDR_CONFIG_PATH` pointing to a directory silently failed with an eprintln while falsely reporting `SUCCESS`.
8. `is_download_active()` only inspected the calling process's in-memory queue, allowing concurrent processes to modify settings while downloads were active in another terminal.
9. `dwdr --help` and `dwdr --version` created side-effect configuration files because `ConfigManager::load()` was called prior to `Cli::parse()`.
10. In non-TTY/headless environments, `interactive_config` could loop infinitely due to `Select::prompt` returning default index 0 and `FileDialog` returning `None`.

## Decision

1. **Strict Input Validation**:
   - `quality` is validated against allowed resolution profiles (`always ask`, `best`, `8k`, `4k`, `2k`, `1080p`, `720p`, `480p`, `360p`, `240p`, `144p`, `audio`).
   - `parallel` is bounded to `1..=16`.
   - `update-days` is bounded to `0..=365`.
   - `path` must exist, be a directory, and pass a temporary file write probe with per-process unique naming. Windows verbatim `\\?\` prefixes are stripped from canonicalized paths to maintain FFmpeg compatibility.
2. **Error Propagation**:
   - `ConfigManager::save` and `save_config` return `Result<(), String>`, alerting callers to filesystem write errors.
3. **Interactive Confirmation with Force Bypass**:
   - `dwdr config reset` prompts for interactive `[y/N]` confirmation unless `--force` is supplied.
4. **Deferred Configuration Loading**:
   - `Cli::parse()` is executed before `ConfigManager::load()` to prevent configuration side-effects on `--help` and `--version`.
5. **Headless Execution Guards**:
   - `interactive_config` rejects non-TTY environments with an informative error directing users to `dwdr config set`.
   - `Select::prompt` returns `Err(())` when stdin/stdout are not terminals.
   - `interactive_config` operates on an in-memory clone, committing mutations only upon option 4 ("Save & Exit").
6. **Cross-Process Concurrency Guard**:
   - Active downloads register their PIDs in `active_downloads.lock`.
   - `is_download_active()` checks in-process queues, direct download trackers, and verifies the liveness of PIDs recorded in the cross-process lockfile, automatically purging stale entries from terminated processes.
   - Both CLI actions (`set`, `reset`) and interactive configuration (`dwdr config`) enforce the active download guard unless overridden by `--force`.

## Consequences

### Positive
- Prevents invalid or corrupt settings from entering `config.toml`.
- Protects in-flight downloads across multiple terminals from destructive configuration mutations.
- Eliminates side-effect file creation during CLI discovery (`--help`, `--version`).
- Prevents headless hangs and infinite prompt loops.

### Negative
- Scripts attempting to run `dwdr config reset` non-interactively must now include `--force` or pipe confirmation.
