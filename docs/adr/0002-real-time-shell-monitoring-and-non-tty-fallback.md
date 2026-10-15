# ADR-0002: Real-Time In-Place Shell Monitoring and Non-TTY Automation Fallback

**Date**: 2026-10-15
**Status**: accepted
**Deciders**: Engineering Team

## Context

In interactive REPL mode, background downloads ran silently while the shell prompt sat idle. Users had to repeatedly type commands to check progress. Commands `status`, `list`, and `queue` were identical aliases returning static one-line snapshots. Furthermore, `inquire::Text` required a TTY and blocked indefinitely when stdin was piped or scripted, preventing automated integration testing.

## Decision

1. Differentiate shell inspection commands:
   - `status`: In-place live monitor refreshing at 200ms intervals via `crossterm` cursor controls; exits on Enter/q.
   - `list`: Session-wide report showing all tasks (active metrics, completed paths, failure reasons).
   - `queue`: Queue slots and waiting positions.
   - `active`: Fast static snapshot of active downloads.
2. Implement TTY detection (`is_terminal()`): fall back from `inquire::Text` to buffered line reading on non-interactive stdin, and fall back from live cursor updates to static snapshots on non-interactive stdout.

## Alternatives Considered

### Alternative 1: Print live progress directly while at the input prompt
- **Pros**: No separate command needed.
- **Cons**: Disrupts the user while typing new URLs or commands into the prompt.
- **Why not**: Degrades terminal usability and breaks clean command input.

### Alternative 2: Adopt a full TUI framework (e.g. `ratatui`)
- **Pros**: Rich multi-window interface.
- **Cons**: Heavy dependency footprint, complex terminal resizing handling, and loss of simple command-line scriptability.
- **Why not**: DWDR aims for zero bloat, high responsiveness, and seamless shell ergonomics.

## Consequences

### Positive
- Users can watch live progress in real time or view complete session reports on demand.
- Automated tests and CI pipelines can pipe commands to the interactive shell without hanging.
- Unrecognized commands provide friendly guidance rather than failing with "Invalid URL".

### Negative
- Entering `status` blocks the prompt until Enter is pressed or active downloads finish.
