#!/bin/sh
set -e
if command -v systemctl >/dev/null 2>&1; then
  systemctl disable --now iran-split-helper.service >/dev/null 2>&1 || true
fi
