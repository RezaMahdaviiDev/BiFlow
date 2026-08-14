#!/usr/bin/env bash
# Prefetch the Visual Studio channel manifest, VisualStudio.vsman, and every
# CRT/SDK payload cargo-xwin 0.19.2 / xwin 0.6.6 will look up under
# ~/.cache/cargo-xwin/xwin/dl. Microsoft CDNs often close the body early;
# ureq reports `io: unexpected end of file` as `Failed to setup MSVC CRT`
# and does not retry. curl/OpenSSL does, and xwin skips a GET when the
# cached file exists and its SHA-256 matches.
set -Eeuo pipefail

log() {
  command printf '%s\n' "$*"
}

die() {
  command printf 'error: %s\n' "$*" >&2
  exit 1
}

have() {
  command -v "$1" >/dev/null 2>&1
}

XWIN_VERSION="${XWIN_VERSION:-16}"
XWIN_ARCH="${XWIN_ARCH:-x86_64}"
CACHE="${XDG_CACHE_HOME:-${HOME}/.cache}/cargo-xwin/xwin"
DL="${CACHE}/dl"
SCRIPT_DIR="$(command cd -- "$(command dirname -- "${BASH_SOURCE[0]}")" && command pwd)"

command mkdir -p -- "${DL}"
have python3 || die "python3 is required to list xwin CRT/SDK payloads"
have sha256sum || die "sha256sum is required to verify xwin CRT/SDK payloads"

curl_retry_flags=(--retry 8 --retry-delay 2)
if command curl --help 2>/dev/null | command grep -q -- '--retry-all-errors'; then
  curl_retry_flags+=(--retry-all-errors)
fi

sha256_ok() {
  local dest="$1"
  local expected="$2"
  [[ -s "${dest}" ]] || return 1
  local actual
  actual="$(command sha256sum -- "${dest}" | command awk '{print $1}')"
  [[ "${actual}" == "${expected}" ]]
}

download() {
  local dest="$1"
  local url="$2"
  local expected="${3:-}"
  if [[ -n "${expected}" ]] && sha256_ok "${dest}" "${expected}"; then
    log "already present: ${dest}"
    return 0
  fi
  if [[ -e "${dest}" ]]; then
    log "checksum mismatch or incomplete, re-downloading $(basename -- "${dest}")"
    command rm -f -- "${dest}"
  fi
  command mkdir -p -- "$(command dirname -- "${dest}")"
  local tmp="${dest}.partial"
  command rm -f -- "${tmp}"
  log "Downloading $(basename -- "${dest}") with curl..."
  command curl -fL "${curl_retry_flags[@]}" \
    --connect-timeout 30 --max-time 900 \
    -o "${tmp}" "${url}"
  [[ -s "${tmp}" ]] || die "empty download: ${url}"
  if [[ -n "${expected}" ]] && ! sha256_ok "${tmp}" "${expected}"; then
    command rm -f -- "${tmp}"
    die "SHA-256 mismatch after download: ${dest}"
  fi
  command mv -f -- "${tmp}" "${dest}"
}

extract_vsman() {
  local manifest="$1"
  python3 - "${manifest}" <<'PY'
import json, sys
manifest = json.load(open(sys.argv[1], encoding="utf-8"))
for item in manifest.get("channelItems", []):
    if item.get("type") == "Manifest" and item.get("payloads"):
        payload = item["payloads"][0]
        print(payload["url"])
        print(payload["sha256"])
        raise SystemExit(0)
raise SystemExit("VisualStudio.vsman payload missing from channel manifest")
PY
}

download "${DL}/manifest_${XWIN_VERSION}.json" \
  "https://aka.ms/vs/${XWIN_VERSION}/release/channel"

mapfile -t vsman < <(extract_vsman "${DL}/manifest_${XWIN_VERSION}.json")
[[ "${#vsman[@]}" -ge 2 && -n "${vsman[0]}" && -n "${vsman[1]}" ]] || \
  die "could not read VisualStudio.vsman URL from the channel manifest"
download "${DL}/pkg_manifest_${vsman[1]}.vsman" "${vsman[0]}"

log "Listing ${XWIN_ARCH} CRT/SDK payloads from VisualStudio.vsman..."
while IFS=$'\t' read -r sha256 filename url size; do
  [[ -n "${sha256}" && -n "${filename}" && -n "${url}" ]] || continue
  download "${DL}/${filename}" "${url}" "${sha256}"
done < <(python3 "${SCRIPT_DIR}/list-xwin-payloads.py" \
  "${DL}/pkg_manifest_${vsman[1]}.vsman" "${XWIN_ARCH}")

log "MSVC CRT payloads are ready in ${DL}"
