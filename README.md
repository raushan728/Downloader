<div align="center">
  <h1>DWDR (Downloader)</h1>
  <p>A high-performance, zero-dependency terminal media downloader for Windows, macOS, and Linux.</p>

  [Installation] | [Commands] | [Examples] | [Configuration] | [Architecture] | [Contributing]
</div>

This is the main source code repository for [DWDR]. It contains the CLI engine, interactive terminal shell, and configuration manager.

[DWDR]: https://github.com/raushan728/Downloader
[Installation]: INSTALL.md
[Commands]: docs/COMMANDS.md
[Examples]: docs/EXAMPLES.md
[Configuration]: docs/CONFIGURATION.md
[Architecture]: docs/ARCHITECTURE.md
[Contributing]: CONTRIBUTING.md

## Why DWDR?

- **Performance:** Accelerated 8-thread fragmented downloading (`-N 8`) coupled with real-time visual progress bar tracking downloaded/total sizes, speed, percentage, and ETA metrics, plus clean background execution and strict MP4 audio/video merging.

- **Reliability:** Zero system pollution. Isolated engines ([yt-dlp], [FFmpeg]) reside exclusively in `~/.dwdr/deps/` without modifying system package managers. Single-video safeguards (`--no-playlist`) prevent accidental multi-track playlist dumps, self-healing configuration handles corrupted settings automatically, and automated cleanup purges incomplete `.part` files on cancellation.

- **Productivity:** Pure terminal ergonomics featuring an interactive non-blocking REPL shell ([inquire]), concurrent background download queue with parallel capacity controls (`max_parallel_downloads`), clean format selection, native desktop folder selection dialogs ([rfd]), and smart quality fallback.

- **Portability:** Built on pure Rust TLS ([rustls]) via [reqwest], enabling zero-effort compilation across Linux, macOS, and Windows without OpenSSL dependencies.

[yt-dlp]: https://github.com/yt-dlp/yt-dlp
[FFmpeg]: https://ffmpeg.org/
[inquire]: https://github.com/mikaelmello/inquire
[rfd]: https://github.com/PolyMeilex/rfd
[rustls]: https://github.com/rustls/rustls
[reqwest]: https://github.com/seanmonstar/reqwest

## Quick Start

Download any media stream directly:

```bash
dwdr "https://www.youtube.com/watch?v=VIDEO_ID"
```

Or open the interactive shell to inspect formats and configure settings:

```bash
dwdr
```

For practical walkthroughs and real-world scenarios, see the [Usage Examples][Examples].

## Installing from Source

If you want to build and install from source, see [INSTALL.md][Installation].

## Documentation

Comprehensive topic guides covering every subsystem:

| Guide | Scope | Description |
| :--- | :--- | :--- |
| [CLI Reference Guide][cli-guide] | Syntax & Flags | CLI arguments, options, direct downloads, and exit codes. |
| [Command Catalog][Commands] | Command Matrix | Complete reference table of all 12 CLI options, subcommands, and REPL actions. |
| [Usage Examples][Examples] | Practical Walkthroughs | Step-by-step scenarios: single downloads, audio extraction, GUI folder pickers, and troubleshooting. |
| [Configuration Guide][Configuration] | Settings & Schema | `config.toml` structure, default resolutions, parallel thread limits, and self-healing recovery. |
| [Architecture Deep Dive][Architecture] | Core Mechanics | Process streaming, dependency lifecycle, single-video safeguards, and cross-platform compilation architecture. |

[cli-guide]: docs/CLI.md

## Getting Help

Feel free to contact me by email at `raushansinghrajpoot687@gmail.com`, on [`LinkedIn`](https://www.linkedin.com/in/raushan-singh-807916390/) or on [`Twitter`](https://twitter.com/Raushan_090).

## Contributing

See [CONTRIBUTING.md][Contributing].

Please open an issue first to discuss proposed features or architectural modifications before submitting a pull request.

## License

DWDR is distributed under the terms of the MIT license. See [LICENSE](LICENSE) for details.