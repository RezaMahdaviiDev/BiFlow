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
- Linux packaging is staged as `compile` → `deb` → `appimage` → `collect`. Windows is `compile` → `nsis` → `collect`. A re-run skips stages that already produced this version (matching `.deb` metadata, a non-empty versioned AppImage, or the NSIS installer). `./build.sh linux --from appimage` starts at the failed stage; `--force` rebuilds every stage. Compile uses `tauri build --no-bundle`; later stages skip `beforeBuildCommand` when `apps/desktop/dist/index.html` already exists.
- Before Linux AppImage bundling, prefetch Tauri's tools (`AppRun-x86_64`, linuxdeploy, gtk/gstreamer plugins) into `~/.cache/tauri` with `curl` and retries. Prefetch runs only for the AppImage stage so a resume of `.deb` or collect does not download again. Tauri 2.5 downloads those files with ureq/rustls; GitHub and its CDN often close TLS without `close_notify`, which rustls treats as a hard error. Prefer the stable `tauri-apps/binary-releases` AppRun over AppImageKit `continuous`.
- Requirement checks do not update apt indexes or request root access when every required Debian package is already installed. Package index updates are deferred until a missing package must be installed.
- Linux builds natively. Windows builds natively on Windows, or from Linux with `cargo-xwin` (MSVC) or MinGW-w64 plus NSIS (`makensis`). The Linux Tauri CLI must not receive `--bundles nsis`; it derives NSIS from the Windows target triple and then invokes the host `makensis`.
- `cargo-xwin` is pinned to `0.19.2`. Newer releases require rustc 1.89, which is above this repo's 1.88 toolchain. Local Linux cross-builds set `XWIN_ARCH=x86_64` and `RAYON_NUM_THREADS=1`, then prefetch the VS 16 channel manifest, `VisualStudio.vsman`, CRT vsix files, SDK MSI files, and UCRT cabs with `curl` (`scripts/prefetch-msvc-crt.sh` + `scripts/list-xwin-payloads.py`) into `~/.cache/cargo-xwin/xwin/dl` using the filenames xwin 0.6.6 expects. cargo-xwin's ureq client treats a truncated Microsoft CDN body as a hard `Failed to setup MSVC CRT` error and does not retry that GET ([xwin#141](https://github.com/Jake-Shadle/xwin/issues/141)). Windows compile/NSIS stages retry that prefetch-and-build loop. Do not `cargo install xwin --version 0.6.6`; that crate version is yanked.
- The Tauri CLI is a root workspace development dependency and runs from the repository root. This keeps `src-tauri/tauri.conf.json` discoverable; the frontend package must not proxy Tauri through a pnpm filter because filtered scripts run from `apps/desktop`. Tauri's shell hooks are also workspace-root-relative (`apps/desktop`), while `frontendDist` is configuration-file-relative (`../apps/desktop/dist`).
- `build.sh` snapshots the synchronized version before compiling, selects exact versioned bundle paths, verifies the Debian package's embedded version, and aborts if the root version changes during the build.
- In-app Hiddify/Mihomo install remains the way third-party binaries are obtained; packaging does not vendor Mihomo from a sibling repo.

## Consequences

- `./dev.sh package` delegates to `./build.sh linux`.
- A Linux-only package run prints only the artifacts that exist. `print_summary` uses `if` so a missing Windows file does not make `set -e` fail the script.
- Cross-compiling Windows from Linux installs NSIS and `cargo-xwin` (or MinGW) instead of printing a manual install recipe.
- Signed production releases use the OS matrix in `.github/workflows/release.yml`, triggered only when a `v*` tag is pushed. Verification runs first, Linux builds `.deb` and AppImage, and Windows builds the portable `.exe` and NSIS installer on a native runner. A final job publishes all four workflow artifacts together. The Linux Tauri CLI must not receive `--bundles nsis`. GitHub `windows-2025` does not ship NSIS; the release job installs it before `tauri-action`. Linux packaging jobs run `scripts/prefetch-appimage-tools.sh` before `tauri-action` so rustls never has to fetch AppRun.
