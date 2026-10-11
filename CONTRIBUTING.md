# Contributing to DWDR

Thank you for your interest in contributing to DWDR. This document outlines the contribution process and high-level architecture.

## Pull Request Process

1. **Fork** the repository.
2. **Branch** off from `main` (`git checkout -b feature/your-feature`).
3. **Commit** your changes with clear, descriptive messages.
4. **Push** your branch to your fork.
5. **Open a Pull Request** against the `main` branch. 

All pull requests are automatically validated by our CI/CD pipeline. 
Code must pass standard formatting checks (`cargo fmt`) and strict linting (`cargo clippy`).

## High-Level Architecture

The project is structured into distinct, decoupled modules:

- `src/cli`: Manages the Interactive Shell, TUI prompts (using `inquire`), and general argument routing.
- `src/engine`: The core background daemon. Handles dynamic dependencies, binary invocations (`yt-dlp`, `ffmpeg`), and thread-safe background task state orchestration.
- `src/web`: An embedded, zero-cost HTTP server (`tiny_http`). Includes the native backend API router and statically-injected Tailwind CSS frontend (`app.js`).
- `src/config`: Manages persistent user configurations (`config.toml`).
