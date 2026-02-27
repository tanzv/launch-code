#!/usr/bin/env bash

set -euo pipefail

usage() {
  cat <<'USAGE'
Run the full BPMN npm package release workflow.

Usage:
  ./scripts/release_bpmn_package.sh [OPTIONS]

Options:
  --package-dir <path>    BPMN npm package directory.
  --source-file <path>    BPMN source file path.
  --env-file <path>       Optional env file with publish defaults.
  --version <semver>      Set package version before publishing.
  --registry <url>        Publish registry URL override.
  --tag <name>            npm dist-tag (default: latest).
  --access <mode>         npm access mode (public|restricted).
  --dry-run               Run publish in dry-run mode.
  --skip-verification     Skip cargo test/clippy verification steps.
  --allow-dirty           Allow publishing with dirty git workspace.
  -h, --help              Show this help message.

Environment variables:
  LCODE_REPO_ROOT              Repository root override.
  LCODE_BPMN_PACKAGE_DIR       Default package directory.
  LCODE_BPMN_SOURCE_FILE       Default BPMN source file path.
  LCODE_BPMN_RELEASE_ENV_FILE  Default env file path.
  LCODE_NPM_BPMN_VERSION       Default version when --version is not set.
  LCODE_NPM_PUBLISH_REGISTRY   Publish registry source for npm publish.
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

is_valid_semver() {
  local value="$1"
  [[ "${value}" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?(\+[0-9A-Za-z.-]+)?$ ]]
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

trim_whitespace() {
  local value="$1"
  value="${value#"${value%%[![:space:]]*}"}"
  value="${value%"${value##*[![:space:]]}"}"
  printf '%s' "${value}"
}

load_env_file() {
  local path="$1"
  local raw_line=""
  local line=""
  local key=""
  local value=""
  local line_no=0

  while IFS= read -r raw_line || [[ -n "${raw_line}" ]]; do
    line_no=$((line_no + 1))
    line="$(trim_whitespace "${raw_line}")"

    if [[ -z "${line}" ]]; then
      continue
    fi

    case "${line}" in
      \#*)
        continue
        ;;
    esac

    case "${line}" in
      export[[:space:]]*)
        line="$(trim_whitespace "${line#export}")"
        ;;
    esac

    if [[ "${line}" != *=* ]]; then
      log_error "Invalid env file line ${line_no} in ${path}: ${raw_line}"
      return 1
    fi

    key="$(trim_whitespace "${line%%=*}")"
    value="$(trim_whitespace "${line#*=}")"

    if [[ ! "${key}" =~ ^[A-Za-z_][A-Za-z0-9_]*$ ]]; then
      log_error "Invalid env key at line ${line_no} in ${path}: ${key}"
      return 1
    fi

    if [[ "${#value}" -ge 2 ]]; then
      if [[ "${value:0:1}" == "\"" && "${value: -1}" == "\"" ]]; then
        value="${value#\"}"
        value="${value%\"}"
      elif [[ "${value:0:1}" == "'" && "${value: -1}" == "'" ]]; then
        value="${value#\'}"
        value="${value%\'}"
      fi
    fi

    printf -v "${key}" '%s' "${value}"
    export "${key}"
  done < "${path}"

  return 0
}

validate_publish_inputs() {
  if [[ -z "${TAG_OVERRIDE}" ]]; then
    log_error "Publish tag cannot be empty"
    return 1
  fi

  if [[ -n "${ACCESS_OVERRIDE}" ]]; then
    case "${ACCESS_OVERRIDE}" in
      public|restricted) ;;
      *)
        log_error "Invalid --access value: ${ACCESS_OVERRIDE}. Allowed values: public, restricted."
        return 1
        ;;
    esac
  fi

  if [[ -n "${VERSION_OVERRIDE}" ]] && ! is_valid_semver "${VERSION_OVERRIDE}"; then
    log_error "Invalid --version value: ${VERSION_OVERRIDE}. Expected semver format, e.g. 1.2.3"
    return 1
  fi

  if [[ ! -d "${PACKAGE_DIR}" ]]; then
    log_error "Package directory does not exist: ${PACKAGE_DIR}"
    return 1
  fi

  if [[ ! -f "${PACKAGE_DIR}/package.json" ]]; then
    log_error "package.json is missing under: ${PACKAGE_DIR}"
    return 1
  fi

  if [[ ! -f "${SOURCE_FILE}" ]]; then
    log_error "BPMN source file does not exist: ${SOURCE_FILE}"
    return 1
  fi

  return 0
}

VERSION_BACKUP=""

restore_package_version_on_exit() {
  if [[ -n "${VERSION_BACKUP}" && -f "${VERSION_BACKUP}" ]]; then
    cp "${VERSION_BACKUP}" "${PACKAGE_DIR}/package.json" >/dev/null 2>&1 || true
    rm -f "${VERSION_BACKUP}" >/dev/null 2>&1 || true
    log_info "Restored package.json after dry-run version update"
  fi
}

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="${LCODE_REPO_ROOT:-$(cd "${SCRIPT_DIR}/.." && pwd)}"

DEFAULT_PACKAGE_DIR="${REPO_ROOT}/packages/npm/bpmn"
DEFAULT_SOURCE_FILE="${REPO_ROOT}/docs/release/npm-publish.bpmn"

PACKAGE_DIR="${LCODE_BPMN_PACKAGE_DIR:-${DEFAULT_PACKAGE_DIR}}"
SOURCE_FILE="${LCODE_BPMN_SOURCE_FILE:-${DEFAULT_SOURCE_FILE}}"
ENV_FILE_PATH=""
ENV_FILE_FROM_CLI=0
PACKAGE_DIR_FROM_CLI=0
SOURCE_FILE_FROM_CLI=0
VERSION_OVERRIDE=""
VERSION_FROM_CLI=0
REGISTRY_OVERRIDE=""
REGISTRY_FROM_CLI=0
TAG_OVERRIDE=""
TAG_FROM_CLI=0
ACCESS_OVERRIDE=""
ACCESS_FROM_CLI=0
DRY_RUN=0
DRY_RUN_FROM_CLI=0
SKIP_VERIFICATION=0
ALLOW_DIRTY=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --package-dir)
      [[ $# -ge 2 ]] || {
        log_error "--package-dir requires a value"
        exit 2
      }
      PACKAGE_DIR="$2"
      PACKAGE_DIR_FROM_CLI=1
      shift 2
      ;;
    --source-file)
      [[ $# -ge 2 ]] || {
        log_error "--source-file requires a value"
        exit 2
      }
      SOURCE_FILE="$2"
      SOURCE_FILE_FROM_CLI=1
      shift 2
      ;;
    --env-file)
      [[ $# -ge 2 ]] || {
        log_error "--env-file requires a value"
        exit 2
      }
      ENV_FILE_PATH="$2"
      ENV_FILE_FROM_CLI=1
      shift 2
      ;;
    --version)
      [[ $# -ge 2 ]] || {
        log_error "--version requires a value"
        exit 2
      }
      VERSION_OVERRIDE="$2"
      VERSION_FROM_CLI=1
      shift 2
      ;;
    --registry)
      [[ $# -ge 2 ]] || {
        log_error "--registry requires a value"
        exit 2
      }
      REGISTRY_OVERRIDE="$2"
      REGISTRY_FROM_CLI=1
      shift 2
      ;;
    --tag)
      [[ $# -ge 2 ]] || {
        log_error "--tag requires a value"
        exit 2
      }
      TAG_OVERRIDE="$2"
      TAG_FROM_CLI=1
      shift 2
      ;;
    --access)
      [[ $# -ge 2 ]] || {
        log_error "--access requires a value"
        exit 2
      }
      ACCESS_OVERRIDE="$2"
      ACCESS_FROM_CLI=1
      shift 2
      ;;
    --dry-run)
      DRY_RUN=1
      DRY_RUN_FROM_CLI=1
      shift
      ;;
    --skip-verification)
      SKIP_VERIFICATION=1
      shift
      ;;
    --allow-dirty)
      ALLOW_DIRTY=1
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

if [[ "${ENV_FILE_FROM_CLI}" -eq 1 ]]; then
  if [[ ! -f "${ENV_FILE_PATH}" ]]; then
    log_error "Env file does not exist: ${ENV_FILE_PATH}"
    exit 2
  fi
  load_env_file "${ENV_FILE_PATH}" || exit 2
else
  DEFAULT_ENV_FILE="${LCODE_BPMN_RELEASE_ENV_FILE:-${PACKAGE_DIR}/.env.release.local}"
  if [[ -f "${DEFAULT_ENV_FILE}" ]]; then
    ENV_FILE_PATH="${DEFAULT_ENV_FILE}"
    load_env_file "${ENV_FILE_PATH}" || exit 2
    log_info "Loaded local release env file: ${ENV_FILE_PATH}"
  fi
fi

if [[ "${PACKAGE_DIR_FROM_CLI}" -ne 1 && -n "${LCODE_BPMN_PACKAGE_DIR:-}" ]]; then
  PACKAGE_DIR="${LCODE_BPMN_PACKAGE_DIR}"
fi

if [[ "${SOURCE_FILE_FROM_CLI}" -ne 1 && -n "${LCODE_BPMN_SOURCE_FILE:-}" ]]; then
  SOURCE_FILE="${LCODE_BPMN_SOURCE_FILE}"
fi

if [[ "${VERSION_FROM_CLI}" -ne 1 && -n "${LCODE_NPM_BPMN_VERSION:-}" ]]; then
  VERSION_OVERRIDE="${LCODE_NPM_BPMN_VERSION}"
fi

if [[ "${REGISTRY_FROM_CLI}" -ne 1 && -n "${LCODE_NPM_PUBLISH_REGISTRY:-}" ]]; then
  REGISTRY_OVERRIDE="${LCODE_NPM_PUBLISH_REGISTRY}"
fi

if [[ "${TAG_FROM_CLI}" -ne 1 ]]; then
  TAG_OVERRIDE="${LCODE_NPM_PUBLISH_TAG:-latest}"
fi

if [[ "${ACCESS_FROM_CLI}" -ne 1 ]]; then
  ACCESS_OVERRIDE="${LCODE_NPM_PUBLISH_ACCESS:-}"
fi

if [[ "${DRY_RUN_FROM_CLI}" -ne 1 ]] && is_truthy "${LCODE_NPM_PUBLISH_DRY_RUN:-}"; then
  DRY_RUN=1
fi

validate_publish_inputs || exit 2

if ! command_exists npm; then
  log_error "npm is required for package release"
  exit 2
fi

if ! command_exists git; then
  log_error "git is required for workspace checks"
  exit 2
fi

if [[ "${ALLOW_DIRTY}" -ne 1 ]]; then
  if [[ -n "$(git -C "${REPO_ROOT}" status --porcelain)" ]]; then
    log_error "Workspace has uncommitted changes. Use --allow-dirty to bypass."
    exit 2
  fi
fi

if [[ "${DRY_RUN}" -eq 1 && -n "${VERSION_OVERRIDE}" ]]; then
  VERSION_BACKUP="$(mktemp)"
  cp "${PACKAGE_DIR}/package.json" "${VERSION_BACKUP}"
  trap restore_package_version_on_exit EXIT
fi

if [[ "${SKIP_VERIFICATION}" -ne 1 ]]; then
  if ! command_exists cargo; then
    log_error "cargo is required for verification steps"
    exit 2
  fi

  log_info "Running repository verification: cargo test -q"
  cargo test -q

  log_info "Running repository verification: cargo clippy --all-targets --all-features -- -D warnings"
  cargo clippy --all-targets --all-features -- -D warnings
fi

log_info "Preparing BPMN package artifacts"
"${SCRIPT_DIR}/prepare_bpmn_package.sh" --package-dir "${PACKAGE_DIR}" --source-file "${SOURCE_FILE}"

if [[ -n "${VERSION_OVERRIDE}" ]]; then
  log_info "Updating package version to ${VERSION_OVERRIDE}"
  (
    cd "${PACKAGE_DIR}"
    npm version "${VERSION_OVERRIDE}" --no-git-tag-version
  )
fi

log_info "Running npm pack dry-run preflight"
(
  cd "${PACKAGE_DIR}"
  npm pack --dry-run
)

publish_cmd=("${SCRIPT_DIR}/npm_publish.sh" "--package-dir" "${PACKAGE_DIR}" "--tag" "${TAG_OVERRIDE}")

if [[ -n "${REGISTRY_OVERRIDE}" ]]; then
  publish_cmd+=("--registry" "${REGISTRY_OVERRIDE}")
fi

if [[ -n "${ACCESS_OVERRIDE}" ]]; then
  publish_cmd+=("--access" "${ACCESS_OVERRIDE}")
fi

if [[ "${DRY_RUN}" -eq 1 ]]; then
  publish_cmd+=("--dry-run")
fi

log_info "Publishing BPMN npm package"
"${publish_cmd[@]}"

log_info "BPMN release workflow completed"
