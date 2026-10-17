# Contributing to DWDR

Thank you for contributing to DWDR. This guide outlines conventions for submitting code.

---

## Development Workflow

- **Features**: Open an issue first to discuss the proposal and scope before implementing a new feature.
- **Bug Fixes**: Add and commit a test reproducing or capturing the issue first (`test(scope): ...`), then implement and commit the fix (`fix(scope): ...`).
- **Refactoring Before Fixes**: If refactoring is needed to support a fix, commit the refactor first (`refactor(scope): ...`) with no behavior change before applying the fix.
- **Atomic Commits**: Every commit must represent exactly one logical change. Do not combine refactoring, fixes, or unrelated cleanups in a single commit.
- **Commit Format**: Follow Conventional Commits: `type(scope): brief description` (e.g., `fix(engine): ...`, `feat(cli): ...`, `test(config): ...`).

---

## Codebase Overview

- `src/cli`: Command-line interface, interactive REPL shell, prompts, and argument parsing.
- `src/engine`: Media downloader, process lifecycle management (`yt-dlp`, `ffmpeg`), and background download queue.
- `src/config`: Persistent user configuration (`config.toml`), path resolution, and concurrency locks.
- `src/utils`: URL validation and string helpers.
- `tests/common/`: Mock binary harness for offline, network-free integration testing.
