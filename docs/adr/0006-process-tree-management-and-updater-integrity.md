# ADR-0006: Process Tree Management, Downloader Resilience, and Updater Integrity

**Date**: 2026-10-16
**Status**: accepted
**Deciders**: DWDR Core Team

## Context

A comprehensive audit of DWDR's download engine, child process lifecycle, updater resilience, and terminal rendering surfaced several reliability and safety issues:

1. **Unix Child Process Tree Leaks**: `kill_process_by_pid` issued `kill -9 <pid>` targeting solely the parent `yt-dlp` PID. On Unix, `ffmpeg` processes spawned by `yt-dlp` for stream remuxing/merging were not killed and became orphaned background processes.
2. **Empty Title Wildcard Deletion**: Direct downloads in `download_media` registered with an empty title string (`""`). On cancellation, `clean_partial_files` evaluated `Some("")` as a match for any file containing `""`, resulting in unintended deletion of partial files from concurrent or unrelated downloads.
3. **Overbroad Error Cleanup**: Direct download error handlers invoked `clean_partial_files(download_dir, None)`, purging all partial files in the target directory rather than scoping cleanup to the failed task.
4. **Temporary Directory Leakage**: `clean_partial_files` checked only `path.is_file()`, leaving behind `.temp`, `.tmp`, and `.part` directories created by format mergers.
5. **Indefinite Socket Hangs**: `yt-dlp` invocations lacked socket timeout parameters, allowing network probe and download commands to stall indefinitely on dead TCP connections.
6. **Binary Verification and Corrupted Binary Healing**: `ensure_dependencies()` and `ensure_ffmpeg()` previously checked only `.exists()`. 0-byte, truncated, or invalid binaries were never detected or redownloaded automatically.
7. **Extraction Atomicity and Windows Updater Rollback**: Binary updates and extractions wrote directly to destination paths. If extraction failed halfway, corrupted binaries were left in place. On Windows, `update_self` failed if `.old` existed from a prior update, and offered no rollback if renaming the temporary binary to the executable path failed.
8. **Signal Handler Race Conditions**: Rapidly pressing Ctrl+C triggered re-entrant signal handler execution, and mutex poisoning during sudden process termination caused subsequent cleanup handlers to abort silently.
9. **Queue Max Parallel Dynamic Reduction**: When reducing `max_parallel_downloads` at runtime, `on_task_finished` unconditionally popped pending tasks from the FIFO queue without validating that `active_count < max_parallel`.
10. **Terminal Resize and In-Place Status Drift**: Long media titles in `print_status_live` wrapped across multiple terminal lines, desynchronizing `MoveUp(lines_to_clear)` cursor movements and corrupting terminal display output.

## Decision

1. **Process Group Isolation and Sweeping**:
   - Spawns `yt-dlp` child processes in a dedicated process group on Unix via `std::os::unix::process::CommandExt::process_group(0)`.
   - `kill_process_by_pid` issues `kill -9 -<pid>` to kill the entire process group, simultaneously terminating `yt-dlp` and any `ffmpeg` child processes.
2. **Safe Partial File and Directory Cleanup**:
   - `download_media` accepts `title: Option<&str>` and forwards sanitized task titles to `register_direct_download` and cleanup handlers.
   - `clean_partial_files` explicitly rejects empty titles (`Some("") => false`), preventing wildcard deletions.
   - Recursively removes temporary directories matching task titles and temporary suffixes (`.temp`, `.tmp`, `.part`).
3. **Socket Timeout Enforcement**:
   - Added `--socket-timeout 30` to both `fetch_video_info` and `build_ytdlp_download_cmd`, preventing hanging downloads on dead connections.
4. **Binary Validation and Self-Healing**:
   - Implemented `is_valid_executable` enforcing a minimum 100 KB size check, executable magic headers (PE `MZ` on Windows; ELF `\x7fELF`, shebang `#!`, or zipapp on Linux; Mach-O on macOS), and successful `--version` / `-version` probe execution.
   - Detects corrupted binaries on startup and automatically purges and re-fetches them.
5. **Atomic Staging and Windows Rollback**:
   - Downloads and extractions write to `.tmp` files and atomically rename to destination paths only after passing integrity validation.
   - On Windows, `update_self` purges stale `.old` binaries before backup and automatically restores `.old` if the temporary executable replacement fails.
6. **Re-Entrancy Latch and Poisoned Mutex Recovery**:
   - Added `SIGNAL_RECEIVED: AtomicBool` latch to prevent concurrent signal handler execution on multiple Ctrl+C inputs.
   - Mutex locks recover poisoned guards via `unwrap_or_else(|p| p.into_inner())` to guarantee cleanup execution.
7. **Strict Parallel Bounds on Queue Promotion**:
   - `on_task_finished` promotes pending FIFO tasks only when `mgr.active_count < mgr.max_parallel`.
8. **Responsive Status Bar Line Clamping**:
   - `print_status_live` clamps displayed titles to `terminal_width - 25`, preventing line wrapping and cursor desynchronization.

## Consequences

### Positive
- Orphaned `ffmpeg` processes are eliminated on Unix when downloads are cancelled.
- Cancellation and error handlers cannot delete unrelated files or downloads.
- Corrupted or partial dependency downloads are automatically detected and healed.
- Safe atomic binary self-updating with rollback guarantees on Windows.
- Consistent single-line terminal progress rendering without cursor tearing.

### Negative
- Killing entire process groups on Unix requires process group creation at spawn time (`process_group(0)`).
