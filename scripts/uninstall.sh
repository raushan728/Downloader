#!/usr/bin/env bash
set -e

echo "INFO: Starting DWDR uninstallation..."

# Check if any DWDR processes or child engines are running
for PROC in dwdr yt-dlp ffmpeg; do
	if pgrep -x "${PROC}" >/dev/null 2>&1 || pgrep -f "\.dwdr/deps/${PROC}" >/dev/null 2>&1; then
		echo "WARNING: ${PROC} process is currently running. Please terminate running instances before uninstalling."
	fi
done

DWDR_DIR="${HOME}/.dwdr"
if [ -d "${DWDR_DIR}" ]; then
	echo "INFO: Removing DWDR directory at ${DWDR_DIR}..."
	rm -rf "${DWDR_DIR}"
fi

# Clean up possible .old backup binaries left by updates
for OLD in "${HOME}/.cargo/bin/dwdr.old" "/usr/local/bin/dwdr.old"; do
	if [ -f "${OLD}" ]; then
		rm -f "${OLD}"
	fi
done

# Clean PATH entries safely across supported shells
TMP_RC="${HOME}/.dwdr_tmp_rc.$$"
trap 'rm -f "${TMP_RC}"' EXIT

for RC in "${HOME}/.bashrc" "${HOME}/.zshrc" "${HOME}/.config/fish/config.fish" "${HOME}/.config/nushell/env.nu"; do
	if [ -f "${RC}" ]; then
		if grep -q "\.dwdr/bin" "${RC}"; then
			echo "INFO: Removing DWDR PATH entry from ${RC}..."
			grep -v "# Added by DWDR installer" "${RC}" | grep -v "\.dwdr/bin" >"${TMP_RC}" || true
			mv "${TMP_RC}" "${RC}"
		fi
	fi
done

echo "SUCCESS: DWDR has been completely uninstalled."
