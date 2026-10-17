# ADR-0008: REPL Runtime Integrity, Thread Joining, and Live Queue Monitoring

**Date**: 2026-10-17
**Status**: accepted
**Deciders**: DWDR Core Team

## Context

Several interactive REPL shell and runtime lifecycle issues were identified in DWDR:
1. `read_shell_input` used `inquire::Text` when a TTY was detected. When `inquire` encountered a raw terminal initialization error or non-standard environment (`InquireError`), it returned `None`, which `start_repl` interpreted as EOF and abruptly terminated the interactive session.
2. Background downloads were spawned via detached threads (`thread::spawn`) without retaining their `JoinHandle`s. When users exited via `exit` or `quit`, the process terminated immediately without joining active worker threads, aborting filesystem cleanup and lock releases mid-operation.
3. Invoking `cancel` in the shell cleared the queue and sent kill signals to child processes, but returned immediately while background worker threads were still cleaning partial files and updating state.
4. Entering `config` or `settings` in the REPL unconditionally invoked `interactive_config`, which in non-TTY or piped environments risked raw mode errors or repeated prompts.
5. Live status views (`print_status_live`) hid the terminal cursor via `crossterm::cursor::Hide`. If interrupted with `Ctrl+C`, the process-level signal handler exited immediately with `exit(130)` without executing `cursor::Show` or `disable_raw_mode`, leaving the user's terminal cursor invisible and raw mode enabled.
6. The `list` session history command displayed completed file destination directories without checking if the downloaded files still existed on disk.
7. The `queue` command only displayed a static snapshot without real-time updates as tasks progressed.

## Decision

1. **Input Fallback on Inquire Error**:
   - Extracted `read_line_fallback(prompt: &str) -> Option<String>` in `src/cli/shell.rs`.
   - In `read_shell_input`, if `Text::prompt()` fails due to terminal or environment errors (non-cancellation errors), it falls back seamlessly to `read_line_fallback`, allowing users to continue issuing commands without session termination.

2. **Worker Thread Tracking and Joining**:
   - Added `worker_handles: Vec<thread::JoinHandle<()>>` to `QueueManager`.
   - Captured and stored `JoinHandle`s in `spawn_worker`, automatically pruning finished handles (`!h.is_finished()`).
   - Implemented `QueueManager::cancel_and_join(&Arc<Mutex<Self>>)` and `join_workers(&Arc<Mutex<Self>>)`: drops the `QueueManager` mutex lock before blocking on `handle.join()`, preventing deadlocks and guaranteeing all worker threads terminate cleanly before `cancel`, `exit`, `quit`, or signal cancellation returns.

3. **Terminal Cursor and Raw Mode Signal Restoration**:
   - Updated the global `ctrlc::set_handler` in `src/main.rs` to call `crossterm::terminal::disable_raw_mode()` and `crossterm::execute!(stdout, crossterm::cursor::Show)` before exiting, ensuring the user's terminal state and cursor are always restored upon Ctrl+C.
   - Enhanced `print_status_live` to hide cursor during active redraws and restore on exit (`q`, `Enter`, `Esc`), while including helpful tips for re-opening live view.

4. **Non-TTY Interactive Shell Guards**:
   - Guarded `config` and `settings` commands in `start_repl` with `io::stdin().is_terminal() && io::stdout().is_terminal()`. In non-interactive environments, prints clear error guidance and suggests `dwdr config set <key> <val>` rather than invoking terminal UI routines.

5. **Completed File Verification in Session Report**:
   - Implemented `find_completed_file(download_dir: &str, title: &str) -> Option<PathBuf>` in `src/engine/queue.rs`. Sanitizes titles, filters temp/partial files, and verifies presence on disk via title and token matching.
   - Updated `print_list()` to display the verified file path for present files and `COMPLETED (file not found)` with explanatory guidance for deleted or moved files.

6. **Live Queue Monitoring (`queue live`) and Actionable Guidance**:
   - Implemented `print_queue_live(&self)` in `src/engine/queue.rs` with 200ms real-time polling, active slot tracking, cursor management, and clear-line rendering.
   - Wired `queue live` / `queue -l` in `start_repl` and added actionable guidance tips across commands (`queue`, `status`, `list`, `cancel`, and unrecognized inputs).

## Consequences

### Positive
- Guaranteed clean termination of all worker threads and child processes on exit and cancel without orphaned cleanup tasks or deadlocks.
- Terminal cursor and raw mode are reliably restored regardless of how DWDR terminates (normal exit, `q`/`Enter`, or `Ctrl+C`).
- Interactive configuration is safely guarded in automated, piped, and script environments.
- Session reports accurately reflect on-disk reality when files are deleted or moved.
- Real-time monitoring capability for queued tasks with `queue live`.
- Clear, actionable tips provided when users encounter ambiguous states or errors.

### Negative
- None identified.
