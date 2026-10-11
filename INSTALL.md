# Installation Guide

## Automated Scripts (Recommended)

### macOS / Linux / Android (Termux)
Open your terminal and run the following command:
```bash
curl -fsSL https://raw.githubusercontent.com/raushan728/Downloader/main/scripts/install.sh | bash
```

### Windows
Open PowerShell and run the following command:
```powershell
irm https://raw.githubusercontent.com/raushan728/Downloader/main/scripts/install.ps1 | iex
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

## Android (Termux) Installation

The automated installation script shown above is fully compatible with Termux! It will dynamically detect your Android ARM environment and fetch a statically linked binary customized for Termux, preventing any compilation headaches.

If you still wish to compile it manually from source on Termux:
1. Update your packages:
```bash
pkg update && pkg upgrade
```
2. Install the necessary build dependencies:
```bash
pkg install rust binutils clang make
```
3. Clone the repository:
```bash
git clone https://github.com/raushan728/Downloader.git
cd Downloader
```
4. Build the release binary:
```bash
cargo build --release
```
5. Move the compiled binary to your local bin directory:
```bash
cp target/release/dwdr $PREFIX/bin/
```

## Uninstallation

### macOS / Linux / Android (Termux)
Run the uninstaller to remove DWDR, its dependencies, and path entries:
```bash
curl -fsSL https://raw.githubusercontent.com/raushan728/Downloader/main/scripts/uninstall.sh | bash
```

### Windows
Run the uninstaller in PowerShell:
```powershell
irm https://raw.githubusercontent.com/raushan728/Downloader/main/scripts/uninstall.ps1 | iex
```

