#!/usr/bin/env bash
set -e

REPO="raushan728/Downloader"
OS="$(uname -s)"
ARCH="$(uname -m)"

case "${OS}" in
    Linux*)
        if [[ "${ARCH}" == "aarch64" ]] || [[ "${ARCH}" == "arm64" ]]; then
            if [[ -n "${PREFIX}" ]] && [[ "${PREFIX}" == *"/com.termux/"* ]]; then
                SUFFIX="termux"
            else
                echo "ERROR: General Linux ARM64 is not currently pre-built. Please compile from source."
                exit 1
            fi
        else
            SUFFIX="linux"
        fi
        ;;
    Darwin*)
        SUFFIX="mac"
        ;;
    *)
        echo "ERROR: Unsupported OS: ${OS}"
        exit 1
        ;;
esac

URL="https://github.com/${REPO}/releases/download/latest/dwdr-${SUFFIX}"

if [[ "${SUFFIX}" == "termux" ]]; then
    BIN_DIR="${PREFIX}/bin"
else
    BIN_DIR="${HOME}/.dwdr/bin"
fi

BIN_PATH="${BIN_DIR}/dwdr"

echo "INFO: Downloading DWDR for ${OS} (${ARCH})..."

mkdir -p "${BIN_DIR}"
curl -fsSL "${URL}" -o "${BIN_PATH}"
chmod +x "${BIN_PATH}"

if [[ "${SUFFIX}" != "termux" ]]; then
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
else
    echo "SUCCESS: DWDR installed successfully in Termux! You can now run 'dwdr'."
fi
