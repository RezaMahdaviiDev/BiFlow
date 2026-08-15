# ADR 0029: Packaged privileged helper install

## Status

Accepted

## Context

`./dev.sh` starts a transient systemd helper, but packaged Linux `.deb` / AppImage
and Windows NSIS / portable builds never shipped `iran-split-helper` or started
it. Release builds therefore showed Helper as **Unavailable** ("Helper service
is not installed or running") and Connect could not use TUN.

The helper must not run a user-writable AppImage or portable binary as root.
Windows named pipes created by SYSTEM default to a System integrity label, which
blocks the unelevated desktop process (UIPI).

## Decision

- Ship `iran-split-helper` and a pinned Mihomo copy in Linux and Windows
  packages. Linux places them at `/usr/lib/biflow`. Windows copies them to
  `C:\ProgramData\iran-split\bin` at install time.
- Linux uses a POSIX `install-helper.sh` invoked by `pkexec` (AppImage / in-app
  Install) or `.deb` `postinst` when `SUDO_UID` is present. The script verifies
  SHA-256, writes root-owned `/etc/iran-split/helper.toml`, installs a systemd
  unit, and starts `iran-split-helper.service`.
- The helper socket stays at `/run/iran-split/helper.sock`. The helper chowns
  the socket to `root:authorized_gid` mode `660` so the installing user can
  connect. `authorized_gid` is recorded in helper.toml.
- Windows serves `\\.\pipe\iran-split-helper-v1` with an SDDL that allows local
  Users at Medium integrity. Elevation uses `Start-Process -Verb RunAs` or NSIS
  `perMachine` hooks. The helper runs as a SYSTEM scheduled task (`ONSTART`) so
  this crate can keep workspace `unsafe_code = "forbid"`; pipe ACL construction
  lives in `iran-split-helper-winacl`.
- The Advanced dashboard Install button on the Helper card runs
  `install_helper`. Hiddify and Mihomo remain separate in-app installs.
- The helper authorizes exactly one non-root uid, so `install_linux` rejects
  uid 0 before spawning `pkexec` and names the cause: a `sudo`-launched UI can
  only ever fail the script's `authorized-uid must not be root` check. Script
  failures carry the last stderr line into the returned error and the debug log
  so the dialog states which check stopped the install.
- An AppImage FUSE-mounts itself as the calling user. Without `allow_other` that
  mount denies every other uid including root, so `pkexec` cannot even stat a
  program under `/tmp/.mount_*`. `install_linux` copies the script, helper,
  Mihomo, and unit into `<data>/runtime/helper-install` (mode 0700, a normal
  exec-capable filesystem) and elevates those paths, then deletes the copies.
  SHA-256 digests are taken from the packaged originals and re-verified by the
  elevated script, so a substituted copy is rejected. A `.deb` install keeps
  running `/usr/lib/biflow/install-helper.sh` in place so the polkit action's
  `exec.path` annotation still matches. pkexec exit 126 means the operator
  dismissed the dialog; 127 is a real execution failure and is reported as one.
- Windows elevation runs `Start-Process -Verb RunAs -Wait -PassThru` under
  `$ErrorActionPreference = 'Stop'` and exits with `$process.ExitCode`. Without
  `-PassThru` the status is PowerShell's own, so a failed install or a refused
  UAC prompt looked like success and only surfaced as the unreachable-helper
  timeout. A refused prompt maps to `ERROR_CANCELLED` (1223).

## Consequences

- First launch of an AppImage or a `.deb` installed without `SUDO_UID` shows
  Install on the Helper card and prompts for polkit/UAC.
- `./build.sh` and GitHub Release stage `packaging/staged/iran-split-helper`
  before bundling. That directory is gitignored. Linux cross-compiles of the
  Windows helper must use `cargo xwin build` for `x86_64-pc-windows-msvc`;
  plain `cargo build --target` looks for `link.exe` and fails on Linux.
  Linux-only helper-install items (`LINUX_HELPER_ROOT`, binary candidates,
  `/proc` uid parsing) stay behind `#[cfg(target_os = "linux")]` so a
  Windows `dead_code` build of the desktop lib stays clean.
- Windows Connect/TUN is still implemented by the Windows platform backend;
  this change makes the helper reachable so status is accurate and future TUN
  work has a running service.
