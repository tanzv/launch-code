#!/usr/bin/env bash

set -euo pipefail

usage() {
  cat <<'USAGE'
Prepare the BPMN npm package content.

Usage:
  ./scripts/prepare_bpmn_package.sh [OPTIONS]

Options:
  --package-dir <path>   Package directory that contains package.json.
  --source-file <path>   BPMN source file to copy into package.
  -h, --help             Show this help message.

Environment variables:
  LCODE_REPO_ROOT        Repository root override.
  LCODE_BPMN_PACKAGE_DIR Default package directory (fallback path).
  LCODE_BPMN_SOURCE_FILE Default BPMN source file path.
  LCODE_BPMN_DOC_SOURCE  Optional markdown source copied into package docs.
USAGE
}

log_info() {
  printf '[INFO] %s\n' "$*"
}

log_error() {
  printf '[ERROR] %s\n' "$*" >&2
}

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="${LCODE_REPO_ROOT:-$(cd "${SCRIPT_DIR}/.." && pwd)}"

PACKAGE_DIR="${LCODE_BPMN_PACKAGE_DIR:-${REPO_ROOT}/packages/npm/bpmn}"
SOURCE_FILE="${LCODE_BPMN_SOURCE_FILE:-${REPO_ROOT}/docs/release/npm-publish.bpmn}"
DOC_SOURCE="${LCODE_BPMN_DOC_SOURCE:-${REPO_ROOT}/docs/release/npm-publish.md}"

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
    --source-file)
      [[ $# -ge 2 ]] || {
        log_error "--source-file requires a value"
        exit 2
      }
      SOURCE_FILE="$2"
      shift 2
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

if [[ ! -d "${PACKAGE_DIR}" ]]; then
  log_error "Package directory does not exist: ${PACKAGE_DIR}"
  exit 2
fi

if [[ ! -f "${PACKAGE_DIR}/package.json" ]]; then
  log_error "package.json is missing under: ${PACKAGE_DIR}"
  exit 2
fi

if [[ ! -f "${SOURCE_FILE}" ]]; then
  log_error "BPMN source file does not exist: ${SOURCE_FILE}"
  exit 2
fi

mkdir -p "${PACKAGE_DIR}/bpmn"
cp "${SOURCE_FILE}" "${PACKAGE_DIR}/bpmn/npm-publish.bpmn"

if [[ -f "${DOC_SOURCE}" ]]; then
  mkdir -p "${PACKAGE_DIR}/docs"
  cp "${DOC_SOURCE}" "${PACKAGE_DIR}/docs/npm-publish.md"
fi

log_info "Prepared BPMN package content in ${PACKAGE_DIR}"
