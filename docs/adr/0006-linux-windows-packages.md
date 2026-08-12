# ADR 0006: Linux and Windows release packages

## Status

Accepted

## Context

Operators need installable artifacts: a Debian package on Linux, and on Windows both a runnable `.exe` and a setup installer. `dev.sh package` only produced Linux `.deb`/AppImage and required a sibling Mihomo asset.

## Decision

- `./build.sh` is the release entry point. Default `all` builds Linux and Windows.
- Linux output is a `.deb` at `artifacts/linux/BiFlow_<version>_amd64.deb`.
- Windows output is the app binary `artifacts/windows/BiFlow.exe` and an NSIS installer `artifacts/windows/BiFlow_<version>_x64-setup.exe`.
- Artifact names are derived from the root `version` file via `scripts/build-plan.mjs`.
- Linux builds natively. Windows builds natively on Windows, or from Linux with `cargo-xwin` (MSVC) or MinGW-w64 plus NSIS (`makensis`).
- In-app Hiddify/Mihomo install remains the way third-party binaries are obtained; packaging does not vendor Mihomo from a sibling repo.

## Consequences

- `./dev.sh package` delegates to `./build.sh linux`.
- Cross-compiling Windows from Linux fails with install instructions when NSIS and a Windows Rust toolchain are missing.
- Signed production releases still use the OS matrix in `.github/workflows/release.yml`.
