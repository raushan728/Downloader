#!/usr/bin/env bash
set -e

echo "INFO: Starting DWDR uninstallation..."

DWDR_DIR="${HOME}/.dwdr"
if [ -d "${DWDR_DIR}" ]; then
    echo "INFO: Removing DWDR directory at ${DWDR_DIR}..."
    rm -rf "${DWDR_DIR}"
fi

for RC in "${HOME}/.bashrc" "${HOME}/.zshrc"; do
    if [ -f "${RC}" ]; then
        if grep -q "\.dwdr/bin" "${RC}"; then
            echo "INFO: Removing DWDR PATH entry from ${RC}..."
            grep -v "\.dwdr/bin" "${RC}" > "${RC}.dwdr_tmp" && mv "${RC}.dwdr_tmp" "${RC}"
        fi
    fi
done

echo "SUCCESS: DWDR has been completely uninstalled."
