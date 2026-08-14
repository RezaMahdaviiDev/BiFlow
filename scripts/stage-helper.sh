#!/usr/bin/env bash
# Build iran-split-helper and copy it to packaging/staged for Tauri bundles.
set -euo pipefail

ROOT="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
# shellcheck disable=SC1091
if [[ -f "${HOME}/.cargo/env" ]]; then
  # shellcheck source=/dev/null
  source "${HOME}/.cargo/env"
fi
export PATH="${HOME}/.cargo/bin:${PATH}"

STAGED="${ROOT}/packaging/staged"
TARGET_DIR="${CARGO_TARGET_DIR:-${ROOT}/target}"
mkdir -p -- "${STAGED}"
cd -- "${ROOT}"

if [[ "${1:-}" == "windows" ]]; then
  if [[ -n "${STAGE_HELPER_TARGET:-}" ]]; then
    cargo build --release -p iran-split-helper --target "${STAGE_HELPER_TARGET}"
    src="${TARGET_DIR}/${STAGE_HELPER_TARGET}/release/iran-split-helper.exe"
  else
    cargo build --release -p iran-split-helper
    src="${TARGET_DIR}/release/iran-split-helper.exe"
  fi
  [[ -f "${src}" ]] || { echo "missing Windows helper: ${src}" >&2; exit 1; }
  cp -- "${src}" "${STAGED}/iran-split-helper.exe"
  echo "Staged ${STAGED}/iran-split-helper.exe"
  exit 0
fi

cargo build --release -p iran-split-helper
src="${TARGET_DIR}/release/iran-split-helper"
[[ -x "${src}" ]] || { echo "missing Linux helper: ${src}" >&2; exit 1; }
cp -- "${src}" "${STAGED}/iran-split-helper"
chmod 0755 -- "${STAGED}/iran-split-helper"
echo "Staged ${STAGED}/iran-split-helper"
