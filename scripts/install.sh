#!/usr/bin/env bash
set -e

REPO="raushan728/Downloader"
OS="$(uname -s)"
case "${OS}" in
    Linux*)     SUFFIX="linux";;
    Darwin*)    SUFFIX="mac";;
    *)          echo "ERROR: Unsupported OS: ${OS}"; exit 1;;
esac

URL="https://github.com/${REPO}/releases/download/latest/dwdr-${SUFFIX}"
BIN_DIR="${HOME}/.dwdr/bin"
BIN_PATH="${BIN_DIR}/dwdr"

echo "INFO: Downloading DWDR for ${OS}..."

mkdir -p "${BIN_DIR}"
curl -fsSL "${URL}" -o "${BIN_PATH}"
chmod +x "${BIN_PATH}"

if [[ ":$PATH:" != *":${BIN_DIR}:"* ]]; then
    echo "INFO: Adding ${BIN_DIR} to your PATH..."
    if [ -f "${HOME}/.bashrc" ]; then
        echo "export PATH=\"${BIN_DIR}:\$PATH\"" >> "${HOME}/.bashrc"
    fi
    if [ -f "${HOME}/.zshrc" ]; then
        echo "export PATH=\"${BIN_DIR}:\$PATH\"" >> "${HOME}/.zshrc"
    fi
fi

echo "SUCCESS: DWDR installed successfully! Restart your terminal to use it."
