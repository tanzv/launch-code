#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEFAULT_PACKAGE_DIR="${SCRIPT_DIR}/../packages/npm/bpmn"

has_package_dir=0
for arg in "$@"; do
  if [[ "${arg}" == "--package-dir" ]]; then
    has_package_dir=1
    break
  fi
done

cmd=("${SCRIPT_DIR}/release_bpmn_package.sh")
if [[ "${has_package_dir}" -eq 0 ]]; then
  cmd+=("--package-dir" "${DEFAULT_PACKAGE_DIR}")
fi

cmd+=("$@")
exec "${cmd[@]}"
