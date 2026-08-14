#!/usr/bin/env bash
# Prefetch Tauri 2.5 AppImage tools with curl/OpenSSL so `tauri build --bundles
# appimage` does not download them through ureq/rustls. GitHub and its CDN often
# close TLS without close_notify, which rustls treats as a hard error.
set -Eeuo pipefail

log() {
  command printf '%s\n' "$*"
}

die() {
  command printf 'error: %s\n' "$*" >&2
  exit 1
}

CACHE="${XDG_CACHE_HOME:-${HOME}/.cache}/tauri"
command mkdir -p -- "${CACHE}"

curl_retry_flags=(--retry 5 --retry-delay 2)
if command curl --help 2>/dev/null | command grep -q -- '--retry-all-errors'; then
  curl_retry_flags+=(--retry-all-errors)
fi

download_if_missing() {
  local dest="$1"
  shift
  if [[ -s "${dest}" ]]; then
    log "AppImage tool already present: ${dest}"
    return 0
  fi

  local tmp="${dest}.partial"
  local url
  command rm -f -- "${tmp}"
  for url in "$@"; do
    log "Downloading $(basename -- "${dest}") with curl..."
    if command curl -fL "${curl_retry_flags[@]}" \
      --connect-timeout 30 --max-time 180 \
      -o "${tmp}" "${url}" && [[ -s "${tmp}" ]]; then
      command chmod 770 -- "${tmp}"
      command mv -f -- "${tmp}" "${dest}"
      return 0
    fi
    command rm -f -- "${tmp}"
  done
  die "failed to download $(basename -- "${dest}") into ${CACHE}"
}

# Same filenames Tauri 2.5 looks up in ~/.cache/tauri. Prefer the stable
# tauri-apps mirror; AppImageKit's "continuous" tag is the rustls failure path.
download_if_missing "${CACHE}/AppRun-x86_64" \
  "https://github.com/tauri-apps/binary-releases/releases/download/apprun-old/AppRun-x86_64" \
  "https://github.com/AppImage/AppImageKit/releases/download/continuous/AppRun-x86_64"
download_if_missing "${CACHE}/linuxdeploy-x86_64.AppImage" \
  "https://github.com/tauri-apps/binary-releases/releases/download/linuxdeploy/linuxdeploy-x86_64.AppImage"
download_if_missing "${CACHE}/linuxdeploy-plugin-gtk.sh" \
  "https://raw.githubusercontent.com/tauri-apps/linuxdeploy-plugin-gtk/master/linuxdeploy-plugin-gtk.sh"
download_if_missing "${CACHE}/linuxdeploy-plugin-gstreamer.sh" \
  "https://raw.githubusercontent.com/tauri-apps/linuxdeploy-plugin-gstreamer/master/linuxdeploy-plugin-gstreamer.sh"

log "AppImage bundler tools are ready in ${CACHE}"
