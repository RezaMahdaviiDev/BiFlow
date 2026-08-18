# ADR 0057: GitHub Releases in-app update

## Status

Accepted (supersedes the in-app path of [0024](./0024-signed-github-release-updater.md) and [0040](./0040-reliable-update-check.md))

## Context

The Tauri updater plugin plus signed `latest.json` still failed for operators:
Debian packages opened a browser instead of installing, background polls raced
the About page, and signing-key or `latest.json` layout mistakes looked like
"cannot reach". DBack's About-only GitHub Releases flow works without a
private updater endpoint.

## Decision

- Check only from About → **Check for updates**. Do not poll on startup.
- Fetch `GET https://api.github.com/repos/devlifeX/BiFlow/releases/latest`
  with `Accept: application/vnd.github+json` and `User-Agent: BiFlow/{version}`.
  Compare normalized semver; a newer tag without a platform asset is an error.
- Pick `BiFlow_{version}_amd64.deb`, `BiFlow_{version}_amd64.AppImage`, or
  `BiFlow_{version}_x64-setup.exe`. Download to `{temp}/biflow-update/` via a
  `.part` file. Do not log asset URLs.
- Linux `.deb`: `pkexec env DEBIAN_FRONTEND=noninteractive apt-get install -y`
  the file. Emit phase `installed` and tell the operator to quit and reopen.
  AppImage: wait-for-PID helper, replace `$APPIMAGE`, `exec`, then `app.exit(0)`.
  Windows NSIS: wait-for-PID helper, elevated silent `/S`, relaunch the current
  exe, then `app.exit(0)`. Do not copy `BiFlow.exe` over itself.
- Keep `UpdateCoordinator` idempotent Check/Install and ADR 0039 sidecar work.
  Cache the last `UpdateInfo` in the coordinator so Install does not call the
  plugin. The Tauri updater plugin and `latest.json` remain in the release
  pipeline for older clients; this client does not use them.

## Consequences

- A Debian install no longer depends on minisign or `latest.json`.
- Operators must reopen after a `.deb` update; AppImage and NSIS still relaunch.
- Public GitHub rate limits apply (no token). Packages are not signature-verified
  in-app; GitHub HTTPS and the Release asset name are the trust boundary.
