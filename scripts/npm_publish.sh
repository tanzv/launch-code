#!/usr/bin/env bash

set -euo pipefail

usage() {
  cat <<'USAGE'
Publish an npm package for launch-code.

Usage:
  ./scripts/npm_publish.sh [OPTIONS]

Options:
  --package-dir <path>   Package directory that contains package.json.
  --registry <url>       Override npm publish registry URL.
  --tag <name>           Dist-tag for npm publish (default: latest).
  --access <mode>        npm access mode (public|restricted).
  --dry-run              Run npm publish in dry-run mode.
  -h, --help             Show this help message.

Environment variables:
  LCODE_NPM_PACKAGE_DIR        Default package directory when --package-dir is not set.
  LCODE_NPM_PUBLISH_REGISTRY   Publish registry source (higher priority than NPM_CONFIG_REGISTRY).
  NPM_CONFIG_REGISTRY          Fallback publish registry source.
  LCODE_NPM_PUBLISH_TAG        Default publish tag when --tag is not set.
  LCODE_NPM_PUBLISH_ACCESS     Default publish access when --access is not set.
  LCODE_NPM_PUBLISH_DRY_RUN    If set to 1/true/yes, enables --dry-run.
  NPM_BIN                      npm executable name or absolute path (default: npm).
USAGE
}

log_info() {
  printf '[INFO] %s\n' "$*"
}

log_error() {
  printf '[ERROR] %s\n' "$*" >&2
}

command_exists() {
  command -v "$1" >/dev/null 2>&1
}

is_truthy() {
  local raw="${1:-}"
  local normalized
  normalized="$(printf '%s' "${raw}" | tr '[:upper:]' '[:lower:]')"
  case "${normalized}" in
    1|true|yes|on) return 0 ;;
    *) return 1 ;;
  esac
}

PACKAGE_DIR="${LCODE_NPM_PACKAGE_DIR:-}"
REGISTRY_OVERRIDE=""
TAG_OVERRIDE=""
ACCESS_OVERRIDE=""
CLI_DRY_RUN=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --package-dir)
      [[ $# -ge 2 ]] || {
        log_error "--package-dir requires a value"
        exit 2
      }
      PACKAGE_DIR="$2"
      shift 2
      ;;
    --registry)
      [[ $# -ge 2 ]] || {
        log_error "--registry requires a value"
        exit 2
      }
      REGISTRY_OVERRIDE="$2"
      shift 2
      ;;
    --tag)
      [[ $# -ge 2 ]] || {
        log_error "--tag requires a value"
        exit 2
      }
      TAG_OVERRIDE="$2"
      shift 2
      ;;
    --access)
      [[ $# -ge 2 ]] || {
        log_error "--access requires a value"
        exit 2
      }
      ACCESS_OVERRIDE="$2"
      shift 2
      ;;
    --dry-run)
      CLI_DRY_RUN=1
      shift
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
done

if [[ -z "${PACKAGE_DIR}" ]]; then
  PACKAGE_DIR="$(pwd)"
fi

if [[ ! -d "${PACKAGE_DIR}" ]]; then
  log_error "Package directory does not exist: ${PACKAGE_DIR}"
  exit 2
fi

if [[ ! -f "${PACKAGE_DIR}/package.json" ]]; then
  log_error "package.json is missing under: ${PACKAGE_DIR}"
  exit 2
fi

REGISTRY="${REGISTRY_OVERRIDE:-${LCODE_NPM_PUBLISH_REGISTRY:-${NPM_CONFIG_REGISTRY:-}}}"
TAG="${TAG_OVERRIDE:-${LCODE_NPM_PUBLISH_TAG:-latest}}"
ACCESS="${ACCESS_OVERRIDE:-${LCODE_NPM_PUBLISH_ACCESS:-}}"
NPM_BIN="${NPM_BIN:-npm}"

if [[ -z "${TAG}" ]]; then
  log_error "Publish tag cannot be empty"
  exit 2
fi

if [[ -n "${ACCESS}" ]]; then
  case "${ACCESS}" in
    public|restricted) ;;
    *)
      log_error "Invalid --access value: ${ACCESS}. Allowed values: public, restricted."
      exit 2
      ;;
  esac
fi

if ! command_exists "${NPM_BIN}"; then
  log_error "npm executable not found: ${NPM_BIN}"
  exit 2
fi

DRY_RUN=0
if [[ "${CLI_DRY_RUN}" -eq 1 ]] || is_truthy "${LCODE_NPM_PUBLISH_DRY_RUN:-}"; then
  DRY_RUN=1
fi

publish_cmd=("${NPM_BIN}" "publish" "--tag" "${TAG}")

if [[ -n "${REGISTRY}" ]]; then
  publish_cmd+=("--registry" "${REGISTRY}")
fi

if [[ -n "${ACCESS}" ]]; then
  publish_cmd+=("--access" "${ACCESS}")
fi

if [[ "${DRY_RUN}" -eq 1 ]]; then
  publish_cmd+=("--dry-run")
fi

log_info "Publishing npm package from ${PACKAGE_DIR}"
if [[ -n "${REGISTRY}" ]]; then
  log_info "Using publish registry: ${REGISTRY}"
else
  log_info "Using npm default registry resolution"
fi

(
  cd "${PACKAGE_DIR}"
  "${publish_cmd[@]}"
)
