# Contributing to DWDR

Thank you for your interest in contributing to DWDR. This document outlines the contribution process and high-level architecture.

## Pull Request Process

1. **Open an Issue First**: If you plan to add a new feature, report a bug, or introduce architectural changes, please open an issue first to discuss the proposal with maintainers before implementing.
2. **Fork** the repository.
3. **Branch** off from `main` (`git checkout -b feature/your-feature`).
4. **Commit** your changes with clear, descriptive messages following conventional commits.
5. **Push** your branch to your fork.
6. **Open a Pull Request** against the `main` branch. 

All pull requests are automatically validated by our CI/CD pipeline. 
Code must pass standard formatting checks (`cargo fmt`), strict linting (`cargo clippy`), and the test suite (`cargo test`).

Automated agents and pair programming assistants must review [AGENTS.md](AGENTS.md) for architectural invariants and code conventions.

## Local Development & Verification

Before submitting changes, ensure your environment is clean:

```bash
# Format codebase
cargo fmt

# Run unit and integration tests
cargo test

# Run strict linter checks
cargo clippy --all-targets -- -D warnings
```

## High-Level Architecture

The project is structured into distinct, decoupled modules:

- `src/cli`: Manages the Interactive Shell, TUI prompts (using `inquire`), and general argument routing.
- `src/engine`: The core background daemon. Handles dynamic dependencies, binary invocations (`yt-dlp`, `ffmpeg`), and thread-safe background task state orchestration.
- `src/web`: An embedded, zero-cost HTTP server (`tiny_http`). Includes the native backend API router and statically-injected Tailwind CSS frontend (`app.js`).
- `src/config`: Manages persistent user configurations (`config.toml`).

For full technical details on threading, safety, and process management, see the [Architecture Deep Dive](docs/ARCHITECTURE.md).

