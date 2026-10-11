<div align="center">
  <h1>DWDR (Downloader)</h1>
  <p>The Ultimate Media Downloader with an Interactive CLI and a Premium Web UI.</p>
  <p>
    <a href="INSTALL.md">Installation</a> |
    <a href="docs/CLI.md">CLI Guide</a> |
    <a href="docs/WEB_UI.md">Web UI & API</a> |
    <a href="docs/CONFIGURATION.md">Configuration</a> |
    <a href="docs/ARCHITECTURE.md">Architecture</a> |
    <a href="CONTRIBUTING.md">Contributing</a> |
    <a href="AGENTS.md">Agents</a>
  </p>
  <p>
    <img src="https://img.shields.io/badge/build-passing-brightgreen" alt="Build Passing" />
    <img src="https://img.shields.io/badge/license-MIT-blue" alt="License MIT" />
    <img src="https://img.shields.io/badge/version-1.0-blue" alt="Version 1.0" />
  </p>
</div>

## Key Features

- **Zero External Dependencies**: Automatically manages isolated `yt-dlp` and `ffmpeg` engine binaries in `~/.dwdr/deps/`.
- **Dual Interfaces**: Full-featured Interactive Terminal REPL Shell and modern embedded Web Browser Interface.
- **Turbo Speed**: Accelerated 8-thread fragmented downloading (`-N 8`).
- **Smart Quality Fallback**: Directly downloads configured preferred resolutions (`default_quality`) or falls back to an interactive stream selector.
- **Cross-Platform**: First-class support for Linux, macOS, Android (Termux), and Windows.

## Documentation Index

Explore our dedicated documentation guides for complete details:

- [Installation & Uninstallation Guide](INSTALL.md) - Automated one-liners, source compilation, and uninstallation across Linux, macOS, Termux, and Windows.
- [CLI Reference Guide](docs/CLI.md) - Flags, options, REPL commands, and subcommands.
- [Web UI & REST API Guide](docs/WEB_UI.md) - Web dashboard features and REST API specifications.
- [Configuration Guide](docs/CONFIGURATION.md) - `config.toml` structure, options, and defaults.
- [Architecture Deep Dive](docs/ARCHITECTURE.md) - Internal engine mechanics, threading, and safety.
- [Contributing Guide](CONTRIBUTING.md) - Development setup, testing, and contribution workflow.
- [Agent Guidelines](AGENTS.md) - Invariants and coding conventions for autonomous agents and tools.
- [Security Policy](SECURITY.md) - Vulnerability disclosure and supported versions.

## Getting Help

Feel free to contact me by email at `raushansinghrajpoot687@gmail.com`, on [`LinkedIn`](https://www.linkedin.com/in/raushan-singh-807916390/) or on [`Twitter`](https://twitter.com/Raushan_090).

## License

Distributed under the [MIT License](LICENSE).