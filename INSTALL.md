# Installation Guide

DWDR is built for Windows, macOS, and Linux desktop operating systems across both **x86_64** and **ARM64** (Apple Silicon and Linux `aarch64`) architectures.

## Automated Scripts (Recommended)

### macOS / Linux
Open your terminal and run the following command:
```bash
curl -fsSL https://raw.githubusercontent.com/raushan728/Downloader/master/scripts/install.sh | bash
```

### Windows
Open PowerShell and run the following command:
```powershell
irm https://raw.githubusercontent.com/raushan728/Downloader/master/scripts/install.ps1 | iex
```

## Build from Source (Advanced Users)

To compile and install DWDR natively on your machine using Rust:
1. Clone the repository:
```bash
git clone https://github.com/raushan728/Downloader.git
cd Downloader
```
2. Build and install globally via Cargo:
```bash
cargo install --path .
```

## Network Proxy Configuration

If you are running DWDR in an enterprise environment or behind a firewall, configure your proxy before running downloads or updates:

```bash
# Unix (Linux / macOS)
export DWDR_PROXY="http://proxy.example.com:8080"
# or export HTTPS_PROXY="http://proxy.example.com:8080"

# Windows (PowerShell)
$env:DWDR_PROXY = "http://proxy.example.com:8080"
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

