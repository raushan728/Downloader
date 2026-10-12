<div align="center">
  <h1>DWDR (Downloader)</h1>
  <p>A high-performance, zero-dependency terminal media downloader.</p>
  <p>
    <a href="INSTALL.md">Installation</a> •
    <a href="docs/CLI.md">CLI Guide</a> •
    <a href="docs/COMMANDS.md">Commands</a> •
    <a href="docs/EXAMPLES.md">Examples</a> •
    <a href="docs/CONFIGURATION.md">Configuration</a> •
    <a href="docs/ARCHITECTURE.md">Architecture</a> •
    <a href="CONTRIBUTING.md">Contributing</a> •
    <a href="SECURITY.md">Security</a>
  </p>
  <p>
    <img src="https://img.shields.io/badge/build-passing-brightgreen" alt="Build Passing" />
    <img src="https://img.shields.io/badge/license-MIT-blue" alt="License MIT" />
    <img src="https://img.shields.io/badge/version-1.0-blue" alt="Version 1.0" />
  </p>
</div>

## Capabilities

| Subsystem | Feature | Operational Summary |
| :--- | :--- | :--- |
| **Engine Isolation** | Zero System Pollution | Automatically provisions isolated `yt-dlp` and `ffmpeg` binaries to `~/.dwdr/deps/` without altering system package managers. |
| **Terminal UX** | Native Pickers & REPL | Interactive prompt with `inquire`, configuration shortcuts (`config`, `settings`), and desktop folder browsing via `rfd`. |
| **Performance** | 8x Segmented Streams | Multi-threaded fragmented downloads (`-N 8`) coupled with real-time speed, percentage, and ETA metrics via `indicatif`. |
| **Stream Safety** | Smart Fallback | Automatically matches configured preferred resolution and enforces `--no-playlist` single-stream safeguards on all URLs. |
| **Portability** | Universal Cross-Platform | First-class binaries for Linux, macOS, Windows, and Android (Termux via musl/rustls with zero OpenSSL dependencies). |

## Documentation Matrix

Detailed documentation is organized into modular topic guides:

| Guide | Scope | Key Topics Covered |
| :--- | :--- | :--- |
| [Installation & Uninstallation](INSTALL.md) | Deployment & Setup | Automated curl/irm scripts, source build instructions, Android/Termux setup, and complete uninstaller routines. |
| [CLI Reference Guide](docs/CLI.md) | Syntax & Workflows | Argument rules, direct downloads, interactive REPL shell, quality fallbacks, and subcommands (`config`, `update`). |
| [Command Catalog](docs/COMMANDS.md) | Command Matrix | Complete catalog and horizontal lookup table of all 10 CLI options, subcommands, and REPL shell actions. |
| [Usage Examples](docs/EXAMPLES.md) | Practical Walkthroughs | Step-by-step real-world scenarios: single downloads, audio extraction, GUI folder pickers, and troubleshooting. |
| [Configuration Guide](docs/CONFIGURATION.md) | Settings & Schema | Configuration file locations, schema table, default resolutions, thread counts, and self-healing corruption recovery. |
| [Architecture Deep Dive](docs/ARCHITECTURE.md) | Internal Engineering | Process streaming, dependency lifecycle, single-video safeguards, and cross-platform compilation architecture. |
| [Contributing Guide](CONTRIBUTING.md) | Developer Workflow | Issue proposal requirements, fork/PR protocol, local verification commands (`fmt`, `test`, `clippy`), and coding standards. |
| [Security Policy](SECURITY.md) | Vulnerability Reporting | Supported release versions and coordinated responsible disclosure contact channels. |

## Getting Help

Feel free to contact me by email at `raushansinghrajpoot687@gmail.com`, on [`LinkedIn`](https://www.linkedin.com/in/raushan-singh-807916390/) or on [`Twitter`](https://twitter.com/Raushan_090).

## License

Distributed under the [MIT License](LICENSE).