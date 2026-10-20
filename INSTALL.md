# Installation Guide

DWDR is built for Windows, macOS, and Linux desktop operating systems across both **x86_64** and **ARM64** (Apple Silicon and Linux `aarch64`) architectures.

## Automated Scripts (Recommended)

### macOS / Linux
Open your terminal and run the following command (verifies SHA256 release checksums and configures Bash, Zsh, Fish, or Nushell automatically):
```bash
curl -fsSL https://raw.githubusercontent.com/raushan728/Downloader/master/scripts/install.sh | bash
```

### Windows
Open PowerShell and run the installer (auto-detects administrator privileges and verifies SHA256 release checksums):
```powershell
irm https://raw.githubusercontent.com/raushan728/Downloader/master/scripts/install.ps1 | iex
```

> **Tip:** By default, DWDR installs to your user profile (`~/.dwdr/bin`). To install system-wide for all users, open PowerShell as Administrator and run:
> ```powershell
> & ([scriptblock]::Create((irm https://raw.githubusercontent.com/raushan728/Downloader/master/scripts/install.ps1))) -Scope Machine
> ```

## Build from Source (Advanced Users)

### Prerequisites

Building DWDR requires **Rust and Cargo**. If Rust is not installed on your system, follow the official installation guide at [rust-lang.org/tools/install](https://www.rust-lang.org/tools/install).

Once Rust is installed, the repository's `rust-toolchain.toml` automatically downloads and configures the exact build toolchain upon running any Cargo command.

### Build-time Proxy Setup (Optional)

If you are installing `rustup` or fetching Cargo dependencies behind an enterprise proxy or firewall:

- **Unix (Linux / macOS):**
  ```bash
  export HTTP_PROXY="http://proxy.example.com:8080"
  export HTTPS_PROXY="http://proxy.example.com:8080"
  ```

- **Windows (PowerShell):**
  ```powershell
  $env:HTTP_PROXY = "http://proxy.example.com:8080"
  $env:HTTPS_PROXY = "http://proxy.example.com:8080"
  ```

- **Cargo Configuration (`~/.cargo/config.toml`):**
  You can also permanently configure Cargo to route network requests through your proxy:
  ```toml
  [http]
  proxy = "http://proxy.example.com:8080"
  ```

### Building and Installing

1. Clone the repository:
   ```bash
   git clone https://github.com/raushan728/Downloader.git
   cd Downloader
   ```

2. Build and install globally via Cargo (`rustup` will automatically activate the configured toolchain):
   ```bash
   cargo install --path .
   ```

## Network Proxy Configuration

If you are running DWDR in an enterprise environment or behind a firewall, configure your HTTP, HTTPS, or SOCKS5 proxy before running media downloads, dependency updates, or self-updates.

DWDR inspects standard proxy environment variables in the following order of precedence:
1. `DWDR_PROXY` (application-specific override)
2. `HTTPS_PROXY` / `https_proxy` (secure HTTPS requests)
3. `ALL_PROXY` / `all_proxy` (general network traffic)
4. `HTTP_PROXY` / `http_proxy` (plain HTTP requests)

### Runtime Proxy Examples

- **Unix (Linux / macOS - Bash, Zsh):**
  ```bash
  export DWDR_PROXY="http://proxy.example.com:8080"
  # or export DWDR_PROXY="socks5://127.0.0.1:1080"
  # or export HTTPS_PROXY="http://proxy.example.com:8080"
  # or export ALL_PROXY="socks5://127.0.0.1:1080"
  ```

- **Windows (PowerShell):**
  ```powershell
  $env:DWDR_PROXY = "http://proxy.example.com:8080"
  # or $env:DWDR_PROXY = "socks5://127.0.0.1:1080"
  # or $env:HTTPS_PROXY = "http://proxy.example.com:8080"
  # or $env:ALL_PROXY = "socks5://127.0.0.1:1080"
  ```

- **Windows (Command Prompt):**
  ```cmd
  set DWDR_PROXY=http://proxy.example.com:8080
  rem or set DWDR_PROXY=socks5://127.0.0.1:1080
  rem or set HTTPS_PROXY=http://proxy.example.com:8080
  rem or set ALL_PROXY=socks5://127.0.0.1:1080
  ```

## Uninstallation

### macOS / Linux
Run the uninstaller to remove DWDR, its dependencies, and path entries:
```bash
curl -fsSL https://raw.githubusercontent.com/raushan728/Downloader/master/scripts/uninstall.sh | bash
```

### Windows
Run the uninstaller in PowerShell:
```powershell
irm https://raw.githubusercontent.com/raushan728/Downloader/master/scripts/uninstall.ps1 | iex
```
