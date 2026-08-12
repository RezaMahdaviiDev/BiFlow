#!/usr/bin/env bash
set -Eeuo pipefail

PROJECT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
SOURCE_STACK_DIR="/home/devlife/dev/dariush/clash"
SOURCE_MIHOMO="${SOURCE_STACK_DIR}/.tools/mihomo"
VENDORED_MIHOMO="${PROJECT_DIR}/vendor/mihomo/linux-x86_64/mihomo"
EXPECTED_MIHOMO_SHA256="9c397be7489538628fae781bc005e4c5b8cd7b0961b8bb2ca815c8150f193577"
TARGET_DIR="${PROJECT_DIR}/target"

usage() {
  command cat <<'EOF'
BiFlow development helper

Usage: ./dev.sh [command]

Commands:
  dev       Compile and start the complete native Tauri application (default)
  desktop   Alias for dev
  web       Start only the React UI with the safe in-browser mock backend
  check     Run frontend and Rust formatting, linting, type checks, and tests
  e2e       Run Playwright primary-flow tests against the mock UI
  build     Build optimized frontend, helper, CLI, and Tauri desktop binaries
  package   Build the Linux .deb via ./build.sh linux
  assets    Refresh the local Mihomo development asset after checksum validation
  paths     Print expected Linux output paths
  clean     Remove generated build outputs after an explicit confirmation
  help      Show this message

Release packages for Linux (.deb) and Windows (.exe + NSIS installer):

  ./build.sh
  ./build.sh linux
  ./build.sh windows

The dev/desktop/package commands require Rust 1.88 and WebKitGTK 4.1
development packages. Windows packages also need NSIS and a Windows Rust
target. The web command only requires Node.js and pnpm and never touches TUN,
routes, DNS, or system services.
EOF
}

die() {
  command printf 'error: %s\n' "$*" >&2
  exit 1
}

need_command() {
  command -v "$1" >/dev/null 2>&1 || die "required command not found: $1"
}

sha256_of() {
  command sha256sum "$1" | command awk '{print $1}'
}

verify_mihomo() {
  local candidate="$1"
  [[ -x "${candidate}" ]] || die "Mihomo is missing or not executable: ${candidate}"
  local actual
  actual="$(sha256_of "${candidate}")"
  [[ "${actual}" == "${EXPECTED_MIHOMO_SHA256}" ]] || \
    die "Mihomo checksum mismatch: expected ${EXPECTED_MIHOMO_SHA256}, got ${actual}"
  "${candidate}" -v | command sed -n '1,2p'
}

refresh_assets() {
  need_command rsync
  need_command sha256sum
  verify_mihomo "${SOURCE_MIHOMO}"
  command mkdir -p -- "$(dirname -- "${VENDORED_MIHOMO}")"
  command rsync --checksum --chmod=F755 "${SOURCE_MIHOMO}" "${VENDORED_MIHOMO}"
  verify_mihomo "${VENDORED_MIHOMO}"
  command printf 'Mihomo development asset is ready: %s\n' "${VENDORED_MIHOMO}"
}

ensure_node_dependencies() {
  need_command node
  need_command pnpm
  if [[ ! -d "${PROJECT_DIR}/node_modules" ]]; then
    command printf 'Installing pinned frontend dependencies...\n'
    (cd -- "${PROJECT_DIR}" && pnpm install --frozen-lockfile)
  fi
}

activate_rust_path() {
  local account_home rust_bin_dir
  account_home="$(command getent passwd "$(command id -u)" | command cut -d: -f6)"
  [[ -n "${account_home}" ]] || die "could not determine the current account home directory"
  rust_bin_dir="${account_home}/.cargo/bin"
  if [[ -x "${rust_bin_dir}/cargo" ]]; then
    export PATH="${rust_bin_dir}:${PATH}"
  fi
}

ensure_rust() {
  activate_rust_path
  need_command cargo
  need_command rustc
  local actual
  actual="$(rustc --version | command awk '{print $2}')"
  [[ "${actual}" == "1.88.0" ]] || \
    die "Rust 1.88.0 is required by rust-toolchain.toml; active version is ${actual}"
}

ensure_linux_desktop_dependencies() {
  need_command pkg-config
  pkg-config --exists webkit2gtk-4.1 || die \
    "webkit2gtk-4.1 development files are missing; install libwebkit2gtk-4.1-dev"
  pkg-config --exists gtk+-3.0 || die \
    "GTK 3 development files are missing; install libgtk-3-dev"
}

print_paths() {
  command printf '%s\n' \
    "Frontend: ${PROJECT_DIR}/apps/desktop/dist/" \
    "Desktop:  ${TARGET_DIR}/release/iran-split-desktop" \
    "Helper:   ${TARGET_DIR}/release/iran-split-helper" \
    "CLI:      ${TARGET_DIR}/release/iran-split-cli" \
    "Debian:   ${PROJECT_DIR}/artifacts/linux/" \
    "Windows:  ${PROJECT_DIR}/artifacts/windows/"
}

run_web() {
  ensure_node_dependencies
  command printf 'Starting the safe UI mock at http://127.0.0.1:1420\n'
  command printf 'This mode does not start Mihomo, create a TUN, or change routes.\n'
  cd -- "${PROJECT_DIR}"
  exec pnpm dev --host 127.0.0.1
}

run_dev() {
  ensure_node_dependencies
  ensure_rust
  ensure_linux_desktop_dependencies
  verify_mihomo "${VENDORED_MIHOMO}"
  cd -- "${PROJECT_DIR}"
  exec pnpm tauri dev
}

run_check() {
  ensure_node_dependencies
  ensure_rust
  cd -- "${PROJECT_DIR}"
  pnpm check
  cargo fmt --all --check
  cargo clippy --workspace --all-targets -- -D warnings
  cargo test --workspace
}

run_e2e() {
  ensure_node_dependencies
  cd -- "${PROJECT_DIR}"
  pnpm exec playwright install chromium
  pnpm test:e2e
}

run_build() {
  ensure_node_dependencies
  ensure_rust
  ensure_linux_desktop_dependencies
  verify_mihomo "${VENDORED_MIHOMO}"
  cd -- "${PROJECT_DIR}"
  pnpm build
  cargo build --release -p iran-split-helper -p iran-split-cli
  cargo build --release -p iran-split-desktop
  print_paths
}

run_package() {
  exec "${PROJECT_DIR}/build.sh" linux
}

run_clean() {
  command printf 'This removes only generated target, dist, and coverage directories. Continue? [y/N] '
  local answer
  IFS= read -r answer
  [[ "${answer}" == "y" || "${answer}" == "Y" ]] || die "clean cancelled"
  command rm -rf -- \
    "${PROJECT_DIR}/target" \
    "${PROJECT_DIR}/apps/desktop/dist" \
    "${PROJECT_DIR}/apps/desktop/coverage"
}

main() {
  cd -- "${PROJECT_DIR}"
  case "${1:-dev}" in
    dev) run_dev ;;
    desktop) run_dev ;;
    web) run_web ;;
    check) run_check ;;
    e2e) run_e2e ;;
    build) run_build ;;
    package) run_package ;;
    assets) refresh_assets ;;
    paths) print_paths ;;
    clean) run_clean ;;
    help|-h|--help) usage ;;
    *) usage >&2; die "unknown command: $1" ;;
  esac
}

main "$@"
