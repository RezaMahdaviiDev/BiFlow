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

## Consequences

- First launch of an AppImage or a `.deb` installed without `SUDO_UID` shows
  Install on the Helper card and prompts for polkit/UAC.
- `./build.sh` and GitHub Release stage `packaging/staged/iran-split-helper`
  before bundling. That directory is gitignored.
- Windows Connect/TUN is still implemented by the Windows platform backend;
  this change makes the helper reachable so status is accurate and future TUN
  work has a running service.
