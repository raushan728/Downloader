#!/usr/bin/env bash
set -e

REPO="raushan728/Downloader"
OS="$(uname -s)"
ARCH="$(uname -m)"

case "${OS}" in
    Linux*)
        if [[ "${ARCH}" == "aarch64" ]] || [[ "${ARCH}" == "arm64" ]]; then
            SUFFIX="linux-arm64"
        else
            SUFFIX="linux"
        fi
        ;;
    Darwin*)
        if [[ "${ARCH}" == "aarch64" ]] || [[ "${ARCH}" == "arm64" ]]; then
            SUFFIX="mac-arm64"F
        else
            SUFFIX="mac"
        fi
        ;;
    *)
        echo "ERROR: Unsupported OS: ${OS}"
        exit 1
        ;;
esac

URL="https://github.com/${REPO}/releases/download/latest/dwdr-${SUFFIX}"
CHECKSUM_URL="https://github.com/${REPO}/releases/download/latest/SHA256SUMS"

BIN_DIR="${HOME}/.dwdr/bin"
BIN_PATH="${BIN_DIR}/dwdr"

echo "INFO: Downloading DWDR for ${OS} (${ARCH})..."

mkdir -p "${BIN_DIR}"
TMP_BIN="${BIN_PATH}.tmp.$$"
trap 'rm -f "${TMP_BIN}" "${TMP_BIN}.sums"' EXIT
curl -fsSL "${URL}" -o "${TMP_BIN}"

# Verify SHA256 checksum if release manifest is available
if curl -fsSL "${CHECKSUM_URL}" -o "${TMP_BIN}.sums" 2>/dev/null; then
    EXPECTED_HASH=$(grep "dwdr-${SUFFIX}\$" "${TMP_BIN}.sums" | awk '{print $1}')
    if [ -n "${EXPECTED_HASH}" ]; then
        ACTUAL_HASH=""
        if command -v sha256sum >/dev/null 2>&1; then
            ACTUAL_HASH=$(sha256sum "${TMP_BIN}" | awk '{print $1}')
        elif command -v shasum >/dev/null 2>&1; then
            ACTUAL_HASH=$(shasum -a 256 "${TMP_BIN}" | awk '{print $1}')
        fi

        if [ -n "${ACTUAL_HASH}" ]; then
            if [ "${ACTUAL_HASH}" != "${EXPECTED_HASH}" ]; then
                echo "ERROR: SHA256 checksum mismatch for dwdr-${SUFFIX}. Download may be corrupted."
                exit 1
            fi
            echo "INFO: SHA256 checksum verified."
        fi
    fi
    rm -f "${TMP_BIN}.sums"
fi

mv "${TMP_BIN}" "${BIN_PATH}"
chmod +x "${BIN_PATH}"
trap - EXIT

# Shell PATH configuration
if [[ ":$PATH:" != *":${BIN_DIR}:"* ]]; then
    echo "INFO: Adding ${BIN_DIR} to your PATH..."

    CURRENT_SHELL="$(basename "${SHELL:-bash}")"

    # Support Bash
    BASH_RC="${HOME}/.bashrc"
    if [ -f "${BASH_RC}" ] || [ "${CURRENT_SHELL}" = "bash" ]; then
        touch "${BASH_RC}"
        if ! grep -q "\.dwdr/bin" "${BASH_RC}"; then
            printf "\n# Added by DWDR installer\nexport PATH=\"%s:\$PATH\"\n" "${BIN_DIR}" >> "${BASH_RC}"
        fi
    fi

    # Support Zsh
    ZSH_RC="${HOME}/.zshrc"
    if [ -f "${ZSH_RC}" ] || [ "${CURRENT_SHELL}" = "zsh" ]; then
        touch "${ZSH_RC}"
        if ! grep -q "\.dwdr/bin" "${ZSH_RC}"; then
            printf "\n# Added by DWDR installer\nexport PATH=\"%s:\$PATH\"\n" "${BIN_DIR}" >> "${ZSH_RC}"
        fi
    fi

    # Support Fish
    FISH_CONFIG="${HOME}/.config/fish/config.fish"
    if command -v fish >/dev/null 2>&1 || [ -d "${HOME}/.config/fish" ] || [ "${CURRENT_SHELL}" = "fish" ]; then
        mkdir -p "${HOME}/.config/fish"
        touch "${FISH_CONFIG}"
        if ! grep -q "\.dwdr/bin" "${FISH_CONFIG}"; then
            printf "\n# Added by DWDR installer\nfish_add_path \"%s\"\n" "${BIN_DIR}" >> "${FISH_CONFIG}"
        fi
    fi

    # Support Nushell
    NU_CONFIG_DIR="${HOME}/.config/nushell"
    if command -v nu >/dev/null 2>&1 || [ -d "${NU_CONFIG_DIR}" ] || [ "${CURRENT_SHELL}" = "nu" ]; then
        mkdir -p "${NU_CONFIG_DIR}"
        NU_ENV="${NU_CONFIG_DIR}/env.nu"
        touch "${NU_ENV}"
        if ! grep -q "\.dwdr/bin" "${NU_ENV}"; then
            printf "\n# Added by DWDR installer\n\$env.PATH = (\$env.PATH | split row (char esep) | prepend '%s')\n" "${BIN_DIR}" >> "${NU_ENV}"
        fi
    fi
fi
echo "SUCCESS: DWDR installed successfully! Restart your terminal to use it."
