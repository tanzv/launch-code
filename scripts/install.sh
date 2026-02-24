#!/usr/bin/env bash

set -euo pipefail

INSTALL_DEBUG_DEPS=1
STRICT_DEBUG_DEPS=0

usage() {
  cat <<'EOF'
One-click installer for launch-code/lcode.

Usage:
  ./scripts/install.sh [OPTIONS]

Options:
  --no-debug-deps      Install CLI only, skip debug dependency setup.
  --strict-debug-deps  Fail if debug dependency setup is incomplete.
  -h, --help           Show this help message.

Default behavior:
  1) Ensure Rust/Cargo is available (install via rustup if missing).
  2) Install CLI binaries (lcode, launch-code) via cargo.
  3) Best-effort install debug dependencies (debugpy, js-debug-adapter).
EOF
}

log_info() {
  printf '[INFO] %s\n' "$*"
}

log_warn() {
  printf '[WARN] %s\n' "$*" >&2
}

log_error() {
  printf '[ERROR] %s\n' "$*" >&2
}

command_exists() {
  command -v "$1" >/dev/null 2>&1
}

discover_vscode_js_debug_server_script() {
  local root
  local candidate
  local newest=""
  local newest_mtime=0

  for root in \
    "${HOME}/.vscode/extensions" \
    "${HOME}/.vscode-insiders/extensions" \
    "${HOME}/.cursor/extensions"; do
    [[ -d "${root}" ]] || continue

    while IFS= read -r -d '' candidate; do
      local mtime
      mtime="$(stat -f '%m' "${candidate}" 2>/dev/null || echo 0)"
      if [[ "${mtime}" -gt "${newest_mtime}" ]]; then
        newest_mtime="${mtime}"
        newest="${candidate}"
      fi
    done < <(find "${root}" -type f -path '*/ms-vscode.js-debug*/dist/src/dapDebugServer.js' -print0 2>/dev/null)
  done

  if [[ -n "${newest}" ]]; then
    printf '%s\n' "${newest}"
    return 0
  fi
  return 1
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --no-debug-deps)
      INSTALL_DEBUG_DEPS=0
      ;;
    --strict-debug-deps)
      STRICT_DEBUG_DEPS=1
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      log_error "Unknown argument: $1"
      usage
      exit 2
      ;;
  esac
  shift
done

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

ensure_rust_toolchain() {
  if command -v cargo >/dev/null 2>&1; then
    log_info "Rust toolchain detected."
    return 0
  fi

  log_info "Rust/Cargo not found. Installing rustup toolchain..."
  if ! command -v curl >/dev/null 2>&1; then
    log_error "curl is required to bootstrap rustup."
    return 1
  fi

  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal

  if [[ -f "${HOME}/.cargo/env" ]]; then
    # shellcheck disable=SC1090
    source "${HOME}/.cargo/env"
  fi

  if ! command -v cargo >/dev/null 2>&1; then
    log_error "cargo is still unavailable after rustup install."
    return 1
  fi
}

install_cli() {
  log_info "Installing launch-code from ${REPO_ROOT} ..."
  cargo install --path "${REPO_ROOT}" --force
}

try_install_debugpy() {
  local python_bin=""
  if command -v python3 >/dev/null 2>&1; then
    python_bin="python3"
  elif command -v python >/dev/null 2>&1; then
    python_bin="python"
  fi

  if [[ -z "${python_bin}" ]]; then
    log_warn "Python interpreter not found; skipping debugpy install."
    return 1
  fi

  log_info "Installing Python debug dependency: debugpy"
  if "${python_bin}" -m pip install --user debugpy; then
    return 0
  fi

  log_warn "Failed to install debugpy with ${python_bin} -m pip."
  return 1
}

try_install_node_adapter() {
  if command_exists js-debug-adapter; then
    log_info "Node debug adapter already available in PATH."
    return 0
  fi

  if command_exists node; then
    local vscode_script=""
    if vscode_script="$(discover_vscode_js_debug_server_script)"; then
      log_info "Detected VSCode/Cursor js-debug script: ${vscode_script}"
      log_info "Node adapter auto-discovery should work without extra setup."
      return 0
    fi
  fi

  if ! command_exists npm; then
    log_warn "npm not found; skipping js-debug-adapter install."
    return 1
  fi

  log_info "Installing Node debug dependency: js-debug-adapter"
  if npm install -g js-debug-adapter && command_exists js-debug-adapter; then
    return 0
  fi

  log_warn "npm package js-debug-adapter unavailable; trying @vscode/js-debug."
  if npm install -g @vscode/js-debug; then
    if command_exists js-debug-adapter; then
      return 0
    fi
    local npm_root=""
    npm_root="$(npm root -g 2>/dev/null || true)"
    if [[ -n "${npm_root}" ]]; then
      local js_debug_script="${npm_root}/@vscode/js-debug/dist/src/dapDebugServer.js"
      if [[ -f "${js_debug_script}" ]]; then
        log_info "Installed @vscode/js-debug script: ${js_debug_script}"
        log_warn "Set LCODE_NODE_DAP_ADAPTER_CMD to enable node debug adapter:"
        log_warn "export LCODE_NODE_DAP_ADAPTER_CMD='[\"node\",\"${js_debug_script}\"]'"
        return 0
      fi
    fi
  fi

  if command_exists node; then
    local vscode_script=""
    if vscode_script="$(discover_vscode_js_debug_server_script)"; then
      log_info "Detected VSCode/Cursor js-debug script after npm fallback: ${vscode_script}"
      return 0
    fi
  fi

  log_warn "Failed to install or discover node debug adapter."
  log_warn "Set LCODE_NODE_DAP_ADAPTER_CMD manually, example:"
  log_warn "export LCODE_NODE_DAP_ADAPTER_CMD='[\"node\",\"/path/to/js-debug/src/dapDebugServer.js\"]'"
  return 1
}

verify_install() {
  if command_exists lcode; then
    log_info "lcode binary detected in PATH: $(command -v lcode)"
    lcode --version
    return 0
  fi

  if [[ -x "${HOME}/.cargo/bin/lcode" ]]; then
    log_warn "lcode installed at ${HOME}/.cargo/bin/lcode but not in PATH."
    log_warn "Add this to your shell profile: export PATH=\"${HOME}/.cargo/bin:\$PATH\""
    "${HOME}/.cargo/bin/lcode" --version
    return 0
  fi

  log_error "lcode was not found after installation."
  return 1
}

main() {
  ensure_rust_toolchain
  install_cli
  verify_install

  if [[ "${INSTALL_DEBUG_DEPS}" -eq 1 ]]; then
    local debugpy_ok=0
    local node_adapter_ok=0

    if try_install_debugpy; then
      debugpy_ok=1
    fi
    if try_install_node_adapter; then
      node_adapter_ok=1
    fi

    if [[ "${STRICT_DEBUG_DEPS}" -eq 1 && ( "${debugpy_ok}" -ne 1 || "${node_adapter_ok}" -ne 1 ) ]]; then
      log_error "Strict debug dependency setup failed."
      exit 1
    fi
  else
    log_info "Skipping debug dependency setup (--no-debug-deps)."
  fi

  log_info "Installation completed."
  log_info "Run: lcode doctor runtime --json"
}

main
