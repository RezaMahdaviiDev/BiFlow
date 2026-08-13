# ADR 0006: Linux and Windows release packages

## Status

Accepted

## Context

Operators need installable artifacts: a Debian package on Linux, and on Windows both a runnable `.exe` and a setup installer. `dev.sh package` only produced Linux `.deb`/AppImage and required a sibling Mihomo asset.

## Decision

- `./build.sh` is the release entry point. Default `all` builds Linux and Windows.
- Linux output is a `.deb` and an AppImage at `artifacts/linux/BiFlow_<version>_amd64.deb` and `artifacts/linux/BiFlow_<version>_amd64.AppImage`.
- Windows output is the app binary `artifacts/windows/BiFlow.exe` and an NSIS installer `artifacts/windows/BiFlow_<version>_x64-setup.exe`.
- Artifact names are derived from the root `version` file via `scripts/build-plan.mjs`.
- `./build.sh` is one-shot: it checks for Node 24, pnpm, Rust (from `rust-toolchain.toml`), Linux desktop libraries, NSIS, and `cargo-xwin`, and installs anything missing before building. See [0021](./0021-shared-build-contract.md).
- Requirement checks do not update apt indexes or request root access when every required Debian package is already installed. Package index updates are deferred until a missing package must be installed.
- Linux builds natively. Windows builds natively on Windows, or from Linux with `cargo-xwin` (MSVC) or MinGW-w64 plus NSIS (`makensis`). The Linux Tauri CLI must not receive `--bundles nsis`; it derives NSIS from the Windows target triple and then invokes the host `makensis`.
- `cargo-xwin` is pinned to `0.19.2`. Newer releases require rustc 1.89, which is above this repo's 1.88 toolchain.
- The Tauri CLI is a root workspace development dependency and runs from the repository root. This keeps `src-tauri/tauri.conf.json` discoverable; the frontend package must not proxy Tauri through a pnpm filter because filtered scripts run from `apps/desktop`. Tauri's shell hooks are also workspace-root-relative (`apps/desktop`), while `frontendDist` is configuration-file-relative (`../apps/desktop/dist`).
- `build.sh` snapshots the synchronized version before compiling, selects exact versioned bundle paths, verifies the Debian package's embedded version, and aborts if the root version changes during the build.
- In-app Hiddify/Mihomo install remains the way third-party binaries are obtained; packaging does not vendor Mihomo from a sibling repo.

## Consequences

- `./dev.sh package` delegates to `./build.sh linux`.
- Cross-compiling Windows from Linux installs NSIS and `cargo-xwin` (or MinGW) instead of printing a manual install recipe.
- Signed production releases use the OS matrix in `.github/workflows/release.yml`, triggered only when a `v*` tag is pushed. Verification runs first, Linux builds `.deb` and AppImage, and Windows builds the portable `.exe` and NSIS installer on a native runner. A final job publishes all four workflow artifacts together. The Linux Tauri CLI must not receive `--bundles nsis`. GitHub `windows-2025` does not ship NSIS; the release job installs it before `tauri-action`.
