#!/usr/bin/env bash
set -Eeuo pipefail

PROJECT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
TARGET_DIR="${PROJECT_DIR}/target"
CRATE_BIN="iran-split-desktop"
WINDOWS_TARGET=""
WINDOWS_TAURI_ARGS=()
NODE_VERSION="22.15.2"
PNPM_VERSION="9.0.1"
# cargo-xwin 0.20+ requires rustc 1.89; pin the last release that builds on 1.88.
CARGO_XWIN_VERSION="0.19.2"
TOOL_PREFIX="${HOME}/.local/share/biflow-tools"
RUST_VERSION="$(sed -n 's/^channel = "\([^"]*\)"/\1/p' "${PROJECT_DIR}/rust-toolchain.toml" | head -n 1)"
[[ -n "${RUST_VERSION}" ]] || RUST_VERSION="1.88.0"

die() {
  command printf 'error: %s\n' "$*" >&2
  exit 1
}

log() {
  command printf '%s\n' "$*"
}

have() {
  command -v "$1" >/dev/null 2>&1
}

plan() {
  node "${PROJECT_DIR}/scripts/build-plan.mjs" "$1"
}

usage() {
  command cat <<'EOF'
BiFlow release builder

Usage: ./build.sh [linux|windows|all]

  linux     Native Linux .deb  →  artifacts/linux/BiFlow_<version>_amd64.deb
  windows   Windows app .exe and NSIS installer
            →  artifacts/windows/BiFlow.exe
            →  artifacts/windows/BiFlow_<version>_x64-setup.exe
  all       Linux and Windows (default)

One-shot: missing tools are installed, then packages are built.
Installs Node.js 22, pnpm, Rust (from rust-toolchain.toml), Linux desktop
libraries, NSIS, and cargo-xwin as needed.

Version is read from the root version file. Do not edit package.json,
Cargo.toml, or tauri.conf.json by hand.
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

host_arch() {
  case "$(uname -m)" in
    x86_64|amd64) echo x64 ;;
    aarch64|arm64) echo arm64 ;;
    *) die "unsupported architecture: $(uname -m)" ;;
  esac
}

refresh_path() {
  export PATH="${HOME}/.cargo/bin:${TOOL_PREFIX}/node/bin:${HOME}/.local/share/pnpm:${PATH}"
  if [[ -f "${HOME}/.cargo/env" ]]; then
    set +u
    # shellcheck disable=SC1091
    source "${HOME}/.cargo/env"
    set -u
  fi
}

run_root() {
  if [[ "$(id -u)" -eq 0 ]]; then
    "$@"
  elif have sudo; then
    sudo "$@"
  else
    die "root or sudo is required to install: $*"
  fi
}

APT_UPDATED=0

apt_update_once() {
  [[ "${APT_UPDATED}" -eq 1 ]] && return 0
  run_root env DEBIAN_FRONTEND=noninteractive apt-get update -y
  APT_UPDATED=1
}

ensure_curl() {
  have curl && return 0
  log "Installing curl..."
  case "$(host_os)" in
    linux)
      apt_update_once
      run_root env DEBIAN_FRONTEND=noninteractive apt-get install -y curl ca-certificates
      ;;
    *) die "curl is required to install build tools" ;;
  esac
}

node_major() {
  have node || return 1
  node -p "process.versions.node.split('.')[0]"
}

install_node() {
  local os arch tarball name url
  os="$(host_os)"
  arch="$(host_arch)"
  [[ "${os}" == "linux" ]] || die "automatic Node.js install is supported on Linux; install Node.js ${NODE_VERSION}+ and re-run"
  ensure_curl
  have tar || run_root env DEBIAN_FRONTEND=noninteractive apt-get install -y tar xz-utils
  name="node-v${NODE_VERSION}-linux-${arch}"
  tarball="${name}.tar.xz"
  url="https://nodejs.org/dist/v${NODE_VERSION}/${tarball}"
  log "Installing Node.js ${NODE_VERSION} into ${TOOL_PREFIX}/node..."
  command mkdir -p -- "${TOOL_PREFIX}"
  command curl -fsSL "${url}" | command tar -xJ -C "${TOOL_PREFIX}"
  command rm -rf -- "${TOOL_PREFIX}/node"
  command mv -- "${TOOL_PREFIX}/${name}" "${TOOL_PREFIX}/node"
  refresh_path
  have node || die "Node.js install finished but node is not on PATH"
}

ensure_node() {
  refresh_path
  local major
  if have node; then
    major="$(node_major)"
    if [[ "${major}" -ge 22 ]]; then
      log "Node.js $(node -v) is ready"
      return 0
    fi
    log "Node.js $(node -v) is too old; installing ${NODE_VERSION}"
  else
    log "Node.js is missing; installing ${NODE_VERSION}"
  fi
  install_node
  log "Node.js $(node -v) is ready"
}

ensure_pnpm() {
  refresh_path
  if have pnpm; then
    log "pnpm $(pnpm --version) is ready"
    return 0
  fi
  log "Installing pnpm ${PNPM_VERSION}..."
  if have corepack; then
    corepack enable
    corepack prepare "pnpm@${PNPM_VERSION}" --activate
  else
    ensure_curl
    command curl -fsSL https://get.pnpm.io/install.sh | env PNPM_VERSION="${PNPM_VERSION}" SHELL="$(command -v bash)" sh -
  fi
  refresh_path
  have pnpm || die "pnpm install finished but pnpm is not on PATH"
  log "pnpm $(pnpm --version) is ready"
}

ensure_node_modules() {
  if [[ ! -d "${PROJECT_DIR}/node_modules" ]]; then
    log "Installing pinned frontend dependencies..."
    (cd -- "${PROJECT_DIR}" && pnpm install --frozen-lockfile)
  fi
}

ensure_rust() {
  refresh_path
  if ! have rustup || ! have cargo || ! have rustc; then
    ensure_curl
    log "Installing Rust ${RUST_VERSION} with rustup..."
    command curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | \
      sh -s -- -y --default-toolchain "${RUST_VERSION}" --profile minimal -c rustfmt -c clippy
    refresh_path
  fi
  have cargo && have rustc || die "Rust install finished but cargo is not on PATH"
  rustup toolchain install "${RUST_VERSION}" --profile minimal --component rustfmt,clippy
  local actual
  actual="$(cd -- "${PROJECT_DIR}" && rustc --version | command awk '{print $2}')"
  [[ "${actual}" == "${RUST_VERSION}" ]] || \
    die "Rust ${RUST_VERSION} is required by rust-toolchain.toml; active version is ${actual}"
  log "Rust ${actual} is ready"
}

dpkg_installed() {
  dpkg-query -W -f '${Status}' "$1" 2>/dev/null | grep -q 'install ok installed'
}

apt_install_missing() {
  local packages=()
  local pkg
  for pkg in "$@"; do
    dpkg_installed "${pkg}" || packages+=("${pkg}")
  done
  if [[ "${#packages[@]}" -eq 0 ]]; then
    return 0
  fi
  log "Installing packages: ${packages[*]}"
  apt_update_once
  run_root env DEBIAN_FRONTEND=noninteractive apt-get install -y "${packages[@]}"
}

ensure_linux_desktop_dependencies() {
  [[ "$(host_os)" == "linux" ]] || return 0
  [[ -r /etc/os-release ]] || die "cannot detect Linux distribution"
  # shellcheck disable=SC1091
  source /etc/os-release
  case "${ID:-}" in
    ubuntu|debian|linuxmint|pop)
      apt_update_once
      local indicator="libappindicator3-dev"
      if apt-cache show libayatana-appindicator3-dev >/dev/null 2>&1; then
        indicator="libayatana-appindicator3-dev"
      fi
      apt_install_missing \
        build-essential \
        curl \
        pkg-config \
        libssl-dev \
        libwebkit2gtk-4.1-dev \
        libgtk-3-dev \
        "${indicator}" \
        librsvg2-dev \
        patchelf
      ;;
    *)
      die "automatic desktop-library install supports Debian/Ubuntu; install WebKitGTK 4.1 and GTK 3 development packages"
      ;;
  esac
  have pkg-config || die "pkg-config is missing after package install"
  pkg-config --exists webkit2gtk-4.1 || die "webkit2gtk-4.1 development files are still missing"
  pkg-config --exists gtk+-3.0 || die "GTK 3 development files are still missing"
  log "Linux desktop libraries are ready"
}

ensure_rust_target() {
  local triple="$1"
  refresh_path
  if rustup target list --installed | grep -qx "${triple}"; then
    return 0
  fi
  log "Adding Rust target ${triple}..."
  rustup target add "${triple}"
}

ensure_windows_cross_from_linux() {
  [[ "$(host_os)" == "linux" ]] || return 0
  apt_install_missing nsis llvm clang lld gcc-mingw-w64
  have makensis || die "NSIS (makensis) is missing after package install"
  ensure_rust_target x86_64-pc-windows-msvc
  if ! have cargo-xwin; then
    log "Installing cargo-xwin ${CARGO_XWIN_VERSION} (compatible with rustc ${RUST_VERSION})..."
    cargo install cargo-xwin --locked --version "${CARGO_XWIN_VERSION}"
    refresh_path
  fi
  if have cargo-xwin; then
    WINDOWS_TARGET="x86_64-pc-windows-msvc"
    WINDOWS_TAURI_ARGS=(--runner cargo-xwin --target x86_64-pc-windows-msvc --bundles nsis)
    log "Windows cross-compile toolchain (cargo-xwin) is ready"
    return 0
  fi
  ensure_rust_target x86_64-pc-windows-gnu
  have x86_64-w64-mingw32-gcc || die "MinGW-w64 is missing after package install"
  export CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=x86_64-w64-mingw32-gcc
  WINDOWS_TARGET="x86_64-pc-windows-gnu"
  WINDOWS_TAURI_ARGS=(--target x86_64-pc-windows-gnu --bundles nsis)
  log "Windows cross-compile toolchain (MinGW-w64) is ready"
}

copy_one() {
  local source="$1"
  local destination="$2"
  [[ -f "${source}" ]] || die "expected artifact is missing: ${source}"
  command mkdir -p -- "$(dirname -- "${destination}")"
  command cp -f -- "${source}" "${destination}"
  log "wrote ${destination}"
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
  log "Building Linux .deb for BiFlow $(plan version)"
  (cd -- "${PROJECT_DIR}" && pnpm tauri build --bundles deb)
  collect_linux
}

build_windows() {
  local os
  os="$(host_os)"
  if [[ "${os}" == "windows" ]]; then
    WINDOWS_TARGET=""
    WINDOWS_TAURI_ARGS=(--bundles nsis)
    log "Building Windows .exe and NSIS installer for BiFlow $(plan version)"
  elif [[ "${os}" == "linux" ]]; then
    [[ -n "${WINDOWS_TARGET:-}" ]] || die "Windows cross-compile toolchain was not prepared"
    log "Cross-compiling Windows .exe and NSIS installer for BiFlow $(plan version) (${WINDOWS_TARGET})"
  else
    die "Windows packages require Windows or Linux"
  fi
  (cd -- "${PROJECT_DIR}" && pnpm tauri build "${WINDOWS_TAURI_ARGS[@]}")
  collect_windows "${WINDOWS_TARGET}"
}

print_summary() {
  log ""
  log "BiFlow $(plan version) artifacts:"
  [[ -f "${PROJECT_DIR}/$(plan linux.dir)/$(plan linux.deb)" ]] && \
    log "  Linux deb:          $(plan linux.dir)/$(plan linux.deb)"
  [[ -f "${PROJECT_DIR}/$(plan windows.dir)/$(plan windows.exe)" ]] && \
    log "  Windows app:        $(plan windows.dir)/$(plan windows.exe)"
  [[ -f "${PROJECT_DIR}/$(plan windows.dir)/$(plan windows.installer)" ]] && \
    log "  Windows installer:  $(plan windows.dir)/$(plan windows.installer)"
}

ensure_requirements() {
  local target="$1"
  log "Checking build requirements..."
  refresh_path
  ensure_node
  ensure_pnpm
  ensure_node_modules
  ensure_rust
  case "${target}" in
    linux)
      ensure_linux_desktop_dependencies
      ;;
    windows)
      if [[ "$(host_os)" == "linux" ]]; then
        ensure_windows_cross_from_linux
      fi
      ;;
    all)
      ensure_linux_desktop_dependencies
      if [[ "$(host_os)" == "linux" ]]; then
        ensure_windows_cross_from_linux
      fi
      ;;
  esac
  log "All build requirements are ready"
}

main() {
  local target="${1:-all}"
  case "${target}" in
    -h|--help|help) usage; return 0 ;;
    linux|windows|all) ;;
    *) usage >&2; die "unknown target: ${target}" ;;
  esac

  ensure_requirements "${target}"
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
