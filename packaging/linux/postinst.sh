#!/bin/sh
set -e
if [ -z "${SUDO_UID:-}" ] || [ -z "${SUDO_GID:-}" ] || [ -z "${SUDO_USER:-}" ]; then
  exit 0
fi
home="$(getent passwd "${SUDO_USER}" | cut -d: -f6)"
[ -n "${home}" ] || exit 0
staging="${home}/.local/share/biflow/runtime/generations"
helper="/usr/lib/biflow/iran-split-helper"
mihomo="/usr/lib/biflow/mihomo"
unit="/usr/lib/biflow/iran-split-helper.service"
[ -f "${helper}" ] && [ -f "${mihomo}" ] && [ -f "${unit}" ] || exit 0
helper_hash="$(sha256sum -- "${helper}" | awk '{print $1}')"
mihomo_hash="$(sha256sum -- "${mihomo}" | awk '{print $1}')"
mkdir -p -- "${staging}"
/usr/lib/biflow/install-helper.sh \
  --authorized-uid "${SUDO_UID}" \
  --authorized-gid "${SUDO_GID}" \
  --staging-dir "${staging}" \
  --helper-src "${helper}" \
  --mihomo-src "${mihomo}" \
  --helper-sha256 "${helper_hash}" \
  --mihomo-sha256 "${mihomo_hash}" \
  --tun-name clash-iran \
  --unit-src "${unit}"
