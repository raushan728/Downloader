# ADR-0001: Process Lifecycle, Signal Isolation, and Terminal State Restoration

**Date**: 2026-10-15  
**Status**: Accepted  

## Context

During early development, several child process and terminal handling bugs occurred:
1. **Orphaned Subprocesses**: Killing `yt-dlp` on Unix left spawned `ffmpeg` merging processes running indefinitely in the background. On Windows, terminating the parent process did not sweep child worker processes.
2. **Windows Path and Filename Errors**: Combining target directories and templates in `-o` produced mixed path separators and `[Errno 22] Invalid argument` on special characters.
3. **Corrupted Terminal State**: Interrupting the interactive shell with Ctrl+C during live status monitoring left terminal raw mode enabled and the cursor hidden.
4. **Detached Worker Threads**: Background threads were spawned without retaining `JoinHandle`s, causing the process to exit before filesystem cleanup finished.
5. **Wildcard Deletion Bug**: Empty title strings in direct download cancellation caused `clean_partial_files` to delete all partial files across unrelated downloads.

## Decision

1. **Process Group Isolation (Unix)**:
   - Spawn child processes in an isolated process group via `process_group(0)`.
   - Terminate the entire process tree using `kill -9 -- -<pid>`, ensuring all spawned `ffmpeg` mergers are terminated.

2. **Process Tree Termination (Windows)**:
   - Use `taskkill /F /T /PID <pid>` to terminate child processes recursively.
   - Separate path and filename arguments: `-P <dir>` and `-o "%(title)s.%(ext)s"`.
   - Enforce `--windows-filenames` and clamp filename lengths with `--trim-filenames 160`.

3. **Terminal State Restoration**:
   - The signal handler calls `crossterm::terminal::disable_raw_mode()` and `crossterm::cursor::Show` before exiting, guaranteeing cursor visibility and clean terminal state.

4. **Worker Thread Joining**:
   - Track `JoinHandle`s in `QueueManager` and join all active threads before `exit`, `quit`, or `cancel` returns.

5. **Scoped File Cleanup**:
   - Explicitly reject empty titles in `clean_partial_files` to prevent wildcard deletions.
   - Recursively delete task-specific temporary directories (`.temp`, `.tmp`, `.part`).

## Alternatives Considered

### Alternative 1: Direct Child PID Termination (`std::process::Child::kill`)
- **Pros**: Standard library call without OS-specific process group code.
- **Cons**: Only signals the immediate `yt-dlp` parent; child processes spawned by `yt-dlp` (such as `ffmpeg` mergers) remain orphaned in the background.
- **Why not**: Orphaned `ffmpeg` processes consume CPU and hold file locks after user cancellation.

### Alternative 2: Polling Process Tree via External Utilities (`pgrep` / `wmic`)
- **Pros**: Dynamically discovers process hierarchies before signaling.
- **Cons**: Introduces external shell dependencies, high latency, and platform variance.
- **Why not**: Process group isolation (`process_group(0)` on Unix, `taskkill /F /T` on Windows) is instant, reliable, and requires zero external tooling.

## Consequences

- Zero orphaned `ffmpeg` or `yt-dlp` processes on Unix and Windows.
- No terminal corruption or hidden cursors on Ctrl+C.
- Clean and scoped deletion of partial files on cancellation.
