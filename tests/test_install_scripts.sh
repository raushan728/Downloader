#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

echo "=== DWDR Script Test Runner ==="

# Check mode: --native runs tests on the host system (macOS / local), otherwise uses Docker
MODE="docker"
if [[ "${1:-}" == "--native" ]]; then
  MODE="native"
fi

# Prepare mock assets
TMP_STAGE="$(mktemp -d)"
trap 'rm -rf "${TMP_STAGE}"; kill "${SERVER_PID:-}" 2>/dev/null || true' EXIT

MOCK_BIN="${TMP_STAGE}/mock-bin"
cat << 'EOF' > "${MOCK_BIN}"
#!/usr/bin/env bash
echo "dwdr mock version 0.1.0"
exit 0
EOF
chmod +x "${MOCK_BIN}"

cp "${MOCK_BIN}" "${TMP_STAGE}/dwdr-linux"
cp "${MOCK_BIN}" "${TMP_STAGE}/dwdr-linux-arm64"
cp "${MOCK_BIN}" "${TMP_STAGE}/dwdr-mac"
cp "${MOCK_BIN}" "${TMP_STAGE}/dwdr-mac-arm64"
cp "${MOCK_BIN}" "${TMP_STAGE}/dwdr-windows.exe"

# Generate SHA256SUMS
(
  cd "${TMP_STAGE}"
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum dwdr-* > SHA256SUMS
  else
    shasum -a 256 dwdr-* > SHA256SUMS
  fi
)

# Detect host OS and architecture for native platform target matching
HOST_OS="$(uname -s)"
HOST_ARCH="$(uname -m)"
case "${HOST_OS}" in
Linux*)
  if [[ "${HOST_ARCH}" == "aarch64" ]] || [[ "${HOST_ARCH}" == "arm64" ]]; then
    HOST_SUFFIX="linux-arm64"
  else
    HOST_SUFFIX="linux"
  fi
  ;;
Darwin*)
  if [[ "${HOST_ARCH}" == "aarch64" ]] || [[ "${HOST_ARCH}" == "arm64" ]]; then
    HOST_SUFFIX="mac-arm64"
  else
    HOST_SUFFIX="mac"
  fi
  ;;
*)
  HOST_SUFFIX="linux"
  ;;
esac

# Start local mock HTTP server on a random port
PORT=$(( 18000 + RANDOM % 1000 ))
python3 -m http.server "${PORT}" --directory "${TMP_STAGE}" >/dev/null 2>&1 &
SERVER_PID=$!
sleep 1

# Detect host IP reachable from docker containers
HOST_IP="127.0.0.1"
if [[ "${MODE}" == "docker" ]]; then
  # On Linux Docker, host.docker.internal or docker0 IP is needed
  DOCKER_GATEWAY=$(docker network inspect bridge --format='{{range .IPAM.Config}}{{.Gateway}}{{end}}' 2>/dev/null || echo "172.17.0.1")
  HOST_IP="${DOCKER_GATEWAY:-172.17.0.1}"
fi

MOCK_BASE_URL="http://${HOST_IP}:${PORT}"

run_lifecycle_test() {
  local test_name="$1"
  local home_dir="$2"
  local shell_rc="$3"
  local test_env_prefix="$4"

  echo "--> Running: ${test_name}"
  eval "${test_env_prefix}"

  # 1. Test clean installation
  DWDR_INSTALL_URL="${MOCK_BASE_URL}/dwdr-${HOST_SUFFIX}" \
  DWDR_CHECKSUM_URL="${MOCK_BASE_URL}/SHA256SUMS" \
  bash "${REPO_ROOT}/scripts/install.sh"

  if [[ ! -x "${home_dir}/.dwdr/bin/dwdr" ]]; then
    echo "FAIL: Expected binary at ${home_dir}/.dwdr/bin/dwdr not found or not executable"
    exit 1
  fi

  if ! grep -q "\.dwdr/bin" "${shell_rc}"; then
    echo "FAIL: Shell RC (${shell_rc}) does not contain .dwdr/bin PATH entry"
    exit 1
  fi

  # 2. Test uninstallation
  bash "${REPO_ROOT}/scripts/uninstall.sh"

  if [[ -d "${home_dir}/.dwdr" ]]; then
    echo "FAIL: Directory ${home_dir}/.dwdr was not removed"
    exit 1
  fi

  if grep -q "\.dwdr/bin" "${shell_rc}"; then
    echo "FAIL: Shell RC (${shell_rc}) still contains .dwdr/bin PATH entry after uninstall"
    exit 1
  fi

  # 3. Test idempotent double uninstallation
  bash "${REPO_ROOT}/scripts/uninstall.sh"

  echo "PASS: ${test_name}"
}

if [[ "${MODE}" == "native" ]]; then
  echo "--- Testing natively on host ---"
  NATIVE_HOME="$(mktemp -d)"
  export HOME="${NATIVE_HOME}"
  touch "${HOME}/.bashrc" "${HOME}/.zshrc"

  run_lifecycle_test "Native Host Lifecycle (Bash & Zsh)" "${HOME}" "${HOME}/.bashrc" ""

  # Test checksum mismatch abort
  echo "--> Testing Checksum Mismatch Guard"
  BAD_STAGE="$(mktemp -d)"
  for target in dwdr-linux dwdr-linux-arm64 dwdr-mac dwdr-mac-arm64; do
    echo "corrupt content" > "${BAD_STAGE}/${target}"
    echo "0000000000000000000000000000000000000000000000000000000000000000  ${target}" >> "${BAD_STAGE}/SHA256SUMS"
  done
  BAD_PORT=$(( 19000 + RANDOM % 1000 ))
  python3 -m http.server "${BAD_PORT}" --directory "${BAD_STAGE}" >/dev/null 2>&1 &
  BAD_PID=$!
  sleep 1

  SET_E_STATE="$-"
  set +e
  DWDR_INSTALL_URL="http://127.0.0.1:${BAD_PORT}/dwdr-${HOST_SUFFIX}" \
  DWDR_CHECKSUM_URL="http://127.0.0.1:${BAD_PORT}/SHA256SUMS" \
  bash "${REPO_ROOT}/scripts/install.sh" >/dev/null 2>&1
  EXIT_CODE=$?
  kill "${BAD_PID}" 2>/dev/null || true
  rm -rf "${BAD_STAGE}"
  if [[ "${SET_E_STATE}" =~ e ]]; then set -e; fi

  if [[ "${EXIT_CODE}" -eq 0 ]]; then
    echo "FAIL: Checksum mismatch was expected to exit with non-zero error code"
    exit 1
  fi
  echo "PASS: Checksum mismatch detected and halted installation"

  rm -rf "${NATIVE_HOME}"
else
  echo "--- Testing via Docker Containers ---"

  # Container Test 1: Ubuntu as Root
  echo "--> Testing Container: Ubuntu (Root User)"
  docker run --rm \
    --add-host=host.docker.internal:host-gateway \
    -v "${REPO_ROOT}:/dwdr:ro" \
    -e DWDR_INSTALL_URL="http://host.docker.internal:${PORT}/dwdr-linux" \
    -e DWDR_CHECKSUM_URL="http://host.docker.internal:${PORT}/SHA256SUMS" \
    ubuntu:latest bash -c "
      set -euo pipefail
      apt-get update -qq && apt-get install -y -qq curl ca-certificates >/dev/null 2>&1
      touch /root/.bashrc
      bash /dwdr/scripts/install.sh
      test -x /root/.dwdr/bin/dwdr
      grep -q '\.dwdr/bin' /root/.bashrc
      bash /dwdr/scripts/uninstall.sh
      test ! -d /root/.dwdr
      ! grep -q '\.dwdr/bin' /root/.bashrc
      bash /dwdr/scripts/uninstall.sh
      echo 'PASS: Ubuntu Root Lifecycle'
    "

  # Container Test 2: Ubuntu as Non-Root User
  echo "--> Testing Container: Ubuntu (Non-Root User)"
  docker run --rm \
    --add-host=host.docker.internal:host-gateway \
    -v "${REPO_ROOT}:/dwdr:ro" \
    -e DWDR_INSTALL_URL="http://host.docker.internal:${PORT}/dwdr-linux" \
    -e DWDR_CHECKSUM_URL="http://host.docker.internal:${PORT}/SHA256SUMS" \
    ubuntu:latest bash -c "
      set -euo pipefail
      apt-get update -qq && apt-get install -y -qq curl ca-certificates >/dev/null 2>&1
      useradd -m -s /bin/bash testuser
      su testuser -c '
        set -euo pipefail
        touch /home/testuser/.bashrc
        bash /dwdr/scripts/install.sh
        test -x /home/testuser/.dwdr/bin/dwdr
        grep -q \"\.dwdr/bin\" /home/testuser/.bashrc
        bash /dwdr/scripts/uninstall.sh
        test ! -d /home/testuser/.dwdr
        ! grep -q \"\.dwdr/bin\" /home/testuser/.bashrc
        bash /dwdr/scripts/uninstall.sh
      '
      echo 'PASS: Ubuntu Non-Root User Lifecycle'
    "

  # Container Test 3: Alpine Linux (Minimal POSIX / BusyBox)
  echo "--> Testing Container: Alpine Linux (Busybox / Minimal)"
  docker run --rm \
    --add-host=host.docker.internal:host-gateway \
    -v "${REPO_ROOT}:/dwdr:ro" \
    -e DWDR_INSTALL_URL="http://host.docker.internal:${PORT}/dwdr-linux" \
    -e DWDR_CHECKSUM_URL="http://host.docker.internal:${PORT}/SHA256SUMS" \
    alpine:latest sh -c "
      set -e
      apk add --no-cache curl bash >/dev/null 2>&1
      touch /root/.bashrc
      bash /dwdr/scripts/install.sh
      test -x /root/.dwdr/bin/dwdr
      bash /dwdr/scripts/uninstall.sh
      test ! -d /root/.dwdr
      bash /dwdr/scripts/uninstall.sh
      echo 'PASS: Alpine Linux Lifecycle'
    "

  # Container Test 4: PowerShell AST and Script Verification
  echo "--> Testing Container: PowerShell Script Parser & User Scope"
  docker run --rm \
    -v "${REPO_ROOT}:/dwdr:ro" \
    mcr.microsoft.com/powershell:latest pwsh -Command "
      \$ErrorActionPreference = 'Stop'
      \$tokens = \$null
      \$errors = \$null
      \$null = [System.Management.Automation.Language.Parser]::ParseFile('/dwdr/scripts/uninstall.ps1', [ref]\$tokens, [ref]\$errors)
      if (\$errors.Count -gt 0) {
        Write-Error \"uninstall.ps1 AST error count: \$(\$errors.Count)\"
        exit 1
      }
      \$null = [System.Management.Automation.Language.Parser]::ParseFile('/dwdr/scripts/install.ps1', [ref]\$tokens, [ref]\$errors)
      if (\$errors.Count -gt 0) {
        Write-Error \"install.ps1 AST error count: \$(\$errors.Count)\"
        exit 1
      }
      Write-Host 'PASS: PowerShell Scripts AST Parse Verified'
    "
fi

echo "=== All script tests completed successfully! ==="
