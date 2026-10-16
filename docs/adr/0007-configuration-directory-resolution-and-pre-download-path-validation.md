# ADR-0007: Configuration Directory Resolution and Pre-Download Path Validation

**Date**: 2026-10-16
**Status**: accepted
**Deciders**: DWDR Core Team

## Context

Several configuration handling and download path edge cases remained in DWDR:
1. When `UserDirs::new()` and `BaseDirs::new()` both returned `None` (such as in minimal container or restricted user environments), `DwdrConfig::default()` fell back to `PathBuf::from(".")`, producing relative download paths (`./Downloads/DWDR`) tied to the working directory at launch.
2. If `DWDR_CONFIG_PATH` or `load_from_path` pointed to a directory (e.g. `/etc/dwdr` or `/tmp/dwdr_dir`), `path.is_file()` evaluated to `false`. Self-healing attempted to call `save_config(dir_path, &default_config)`, which failed because the destination was a directory. Consequently, `load_from_path` failed silently on every invocation and returned defaults without saving.
3. Neither `download_media`, `download_media_background`, nor `resolve_media_to_download` performed pre-flight directory validation. If a configured path was non-existent (and could not be created), pointed to a file, or was mounted read-only, `yt-dlp` was spawned blindly and failed mid-stream with raw OS errors (`[Errno 13]` / `[Errno 2]`).
4. `save_config` emitted raw `eprintln` logs rather than returning structured error strings, and `shell.rs` ignored the returned `Result` from `ConfigManager::save(config)`.

## Decision

1. **Absolute Default Path Fallback**:
   - Replaced `PathBuf::from(".")` in `DwdrConfig::default()` with `std::env::current_dir().unwrap_or_else(|_| std::env::temp_dir())`, guaranteeing that default download directories are always absolute paths.

2. **Directory Configuration Resolution**:
   - Updated `ConfigManager::config_path` and `ConfigManager::load_from_path`: if the target path is a directory (`path.is_dir()`), it automatically resolves to `path.join("config.toml")`. This enables self-healing and configuration persistence directly within directory-targeted environments.

3. **Pre-Download Directory Validation**:
   - Extracted unified path verification into `validate_download_path(path: &str, create_if_missing: bool) -> Result<PathBuf, String>`.
   - Validates existence (auto-creating when requested), directory type, and write access via a temporary probe file (`.dwdr_write_test_<pid>.tmp`), canonicalizing paths and stripping Windows UNC prefixes.
   - Reused across `ConfigManager::handle_cli`, `resolve_media_to_download`, `download_media`, and `download_media_background`, preventing process spawning when target directories are invalid.

4. **Structured Error Return**:
   - Removed unformatted `eprintln` calls from `save_config`, propagating error messages through `Result<(), String>` to allow callers in the CLI and interactive shell to display formatted error feedback.

## Consequences

### Positive
- Guaranteed absolute download directories across all operating environments.
- Support for directory-targeted configuration paths without self-healing write failures.
- Fast failure with user-friendly errors before network resources or child processes are consumed.
- Clean error reporting without raw stderr output leaking into stdout.

### Negative
- None identified.
