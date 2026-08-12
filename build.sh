#!/usr/bin/env bash
set -Eeuo pipefail

PROJECT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
TARGET_DIR="${PROJECT_DIR}/target"
CRATE_BIN="iran-split-desktop"

die() {
  command printf 'error: %s\n' "$*" >&2
  exit 1
}

need_command() {
  command -v "$1" >/dev/null 2>&1 || die "required command not found: $1"
}

plan() {
  node "${PROJECT_DIR}/scripts/build-plan.mjs" "$1"
}

usage() {
  command cat <<EOF
BiFlow release builder

Usage: ./build.sh [linux|windows|all]

  linux     Native Linux .deb  →  $(plan linux.dir)/$(plan linux.deb)
  windows   Windows app .exe and NSIS installer
            →  $(plan windows.dir)/$(plan windows.exe)
            →  $(plan windows.dir)/$(plan windows.installer)
  all       Linux and Windows (default)

Version $(plan version) is read from the root version file. Do not edit
package.json, Cargo.toml, or tauri.conf.json by hand.

Linux packages are built on Linux. Windows packages are built natively on
Windows, or cross-compiled from Linux with cargo-xwin (preferred) or MinGW-w64
plus NSIS (makensis).
EOF
}

host_os() {
  case "$(uname -s)" in
    Linux*) echo linux ;;
    Darwin*) echo macos ;;
    MINGW*|MSYS*|CYGWIN*|Windows_NT) echo windows ;;
    *) echo unknown ;;
  esac
}

ensure_node_dependencies() {
  need_command node
  need_command pnpm
  if [[ ! -d "${PROJECT_DIR}/node_modules" ]]; then
    command printf 'Installing pinned frontend dependencies...\n'
    (cd -- "${PROJECT_DIR}" && pnpm install --frozen-lockfile)
  fi
}

ensure_rust() {
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

ensure_rust_target() {
  local triple="$1"
  if rustup target list --installed 2>/dev/null | grep -qx "${triple}"; then
    return 0
  fi
  if command -v rustup >/dev/null 2>&1; then
    rustup target add "${triple}"
    return 0
  fi
  die "Rust target ${triple} is not installed and rustup is unavailable"
}

windows_cross_from_linux() {
  need_command makensis
  if command -v cargo-xwin >/dev/null 2>&1; then
    ensure_rust_target x86_64-pc-windows-msvc
    WINDOWS_TARGET="x86_64-pc-windows-msvc"
    WINDOWS_TAURI_ARGS=(--runner cargo-xwin --target x86_64-pc-windows-msvc --bundles nsis)
    return 0
  fi
  if command -v x86_64-w64-mingw32-gcc >/dev/null 2>&1; then
    ensure_rust_target x86_64-pc-windows-gnu
    export CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=x86_64-w64-mingw32-gcc
    WINDOWS_TARGET="x86_64-pc-windows-gnu"
    WINDOWS_TAURI_ARGS=(--target x86_64-pc-windows-gnu --bundles nsis)
    return 0
  fi
  die "Windows cross-compile from Linux needs NSIS (makensis) and either cargo-xwin or gcc-mingw-w64.
Install:
  cargo install cargo-xwin && rustup target add x86_64-pc-windows-msvc
  sudo apt install nsis llvm
or:
  sudo apt install nsis gcc-mingw-w64
  rustup target add x86_64-pc-windows-gnu"
}

copy_one() {
  local source="$1"
  local destination="$2"
  [[ -f "${source}" ]] || die "expected artifact is missing: ${source}"
  command mkdir -p -- "$(dirname -- "${destination}")"
  command cp -f -- "${source}" "${destination}"
  command printf 'wrote %s\n' "${destination}"
}

first_match() {
  local directory="$1"
  local pattern="$2"
  [[ -d "${directory}" ]] || return 1
  local match
  match="$(command find "${directory}" -maxdepth 1 -type f -name "${pattern}" | command sort | command head -n 1)"
  [[ -n "${match}" ]] || return 1
  command printf '%s\n' "${match}"
}

collect_linux() {
  local source dest
  source="$(first_match "${TARGET_DIR}/release/bundle/deb" "*.deb")" || \
    die "Linux .deb was not produced under ${TARGET_DIR}/release/bundle/deb"
  dest="${PROJECT_DIR}/$(plan linux.dir)/$(plan linux.deb)"
  copy_one "${source}" "${dest}"
}

collect_windows() {
  local triple="${1:-}"
  local prefix="${TARGET_DIR}/release"
  if [[ -n "${triple}" ]]; then
    prefix="${TARGET_DIR}/${triple}/release"
  fi
  local exe setup dest_exe dest_setup
  exe="${prefix}/${CRATE_BIN}.exe"
  setup="$(first_match "${prefix}/bundle/nsis" "*setup.exe")" || \
    die "Windows NSIS installer was not produced under ${prefix}/bundle/nsis"
  dest_exe="${PROJECT_DIR}/$(plan windows.dir)/$(plan windows.exe)"
  dest_setup="${PROJECT_DIR}/$(plan windows.dir)/$(plan windows.installer)"
  copy_one "${exe}" "${dest_exe}"
  copy_one "${setup}" "${dest_setup}"
}

build_linux() {
  [[ "$(host_os)" == "linux" ]] || die "Linux .deb packages must be built on Linux"
  ensure_linux_desktop_dependencies
  command printf 'Building Linux .deb for BiFlow %s\n' "$(plan version)"
  (cd -- "${PROJECT_DIR}" && pnpm tauri build --bundles deb)
  collect_linux
}

build_windows() {
  local os
  os="$(host_os)"
  WINDOWS_TARGET=""
  WINDOWS_TAURI_ARGS=(--bundles nsis)
  if [[ "${os}" == "windows" ]]; then
    command printf 'Building Windows .exe and NSIS installer for BiFlow %s\n' "$(plan version)"
  elif [[ "${os}" == "linux" ]]; then
    windows_cross_from_linux
    command printf 'Cross-compiling Windows .exe and NSIS installer for BiFlow %s (%s)\n' \
      "$(plan version)" "${WINDOWS_TARGET}"
  else
    die "Windows packages require Windows or a Linux host with cargo-xwin/MinGW"
  fi
  (cd -- "${PROJECT_DIR}" && pnpm tauri build "${WINDOWS_TAURI_ARGS[@]}")
  collect_windows "${WINDOWS_TARGET}"
}

print_summary() {
  command printf '\nBiFlow %s artifacts:\n' "$(plan version)"
  [[ -f "${PROJECT_DIR}/$(plan linux.dir)/$(plan linux.deb)" ]] && \
    command printf '  Linux deb:          %s/%s\n' "$(plan linux.dir)" "$(plan linux.deb)"
  [[ -f "${PROJECT_DIR}/$(plan windows.dir)/$(plan windows.exe)" ]] && \
    command printf '  Windows app:        %s/%s\n' "$(plan windows.dir)" "$(plan windows.exe)"
  [[ -f "${PROJECT_DIR}/$(plan windows.dir)/$(plan windows.installer)" ]] && \
    command printf '  Windows installer:  %s/%s\n' "$(plan windows.dir)" "$(plan windows.installer)"
}

main() {
  local target="${1:-all}"
  case "${target}" in
    -h|--help|help) usage; return 0 ;;
    linux|windows|all) ;;
    *) usage >&2; die "unknown target: ${target}" ;;
  esac

  ensure_node_dependencies
  ensure_rust
  (cd -- "${PROJECT_DIR}" && pnpm version:sync)

  case "${target}" in
    linux) build_linux ;;
    windows) build_windows ;;
    all)
      build_linux
      build_windows
      ;;
  esac
  print_summary
}

main "${1:-all}"
