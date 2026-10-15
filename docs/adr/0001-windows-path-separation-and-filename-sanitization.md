# ADR-0001: Windows File Path Separation and Output Filename Sanitization

**Date**: 2026-10-15
**Status**: accepted
**Deciders**: Engineering Team

## Context

When downloading media on Windows, combining the target directory and filename template into a single `-o` argument (e.g. `C:\Users\...\/%(title)s.%(ext)s`) produced mixed path separators. During format-merging operations (such as video format 302 + audio format 251) and on titles containing special unicode characters (e.g., `Í`, commas, brackets), Python's `CreateFileW` aborted with `[Errno 22] Invalid argument`.

## Decision

Separate path directory specification from filename templates by passing `-P <dir>` and `-o "%(title)s.%(ext)s"` to `yt-dlp`. Enforce `--windows-filenames` and `--trim-filenames 160` across all downloads.

## Alternatives Considered

### Alternative 1: Manually replace `/` with `\` in Rust before passing `-o`
- **Pros**: Minimal diff.
- **Cons**: Does not sanitize NTFS forbidden characters, emojis, or handle `MAX_PATH` overflow.
- **Why not**: Failed to prevent errors on titles with special characters or long paths.

### Alternative 2: Slugify all filenames to ASCII (`--restrict-filenames`)
- **Pros**: Strips non-ASCII characters entirely.
- **Cons**: Destroys non-English titles and accents (e.g., Spanish, Hindi, Japanese).
- **Why not**: Severely degrades user experience and media title readability.

## Consequences

### Positive
- Completely eliminates Windows `[Errno 22] Invalid argument` errors.
- Cleanly separates directory destination from filename templating.
- Protects against Windows 260-character path limit violations.

### Negative
- Titles exceeding 160 characters will have their file basenames truncated.
