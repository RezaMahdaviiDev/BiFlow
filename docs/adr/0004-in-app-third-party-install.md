# ADR 0004: In-app Hiddify and Mihomo install

## Status

Accepted

## Context

BiFlow requires Hiddify (upstream proxy) and Mihomo (split core). Users should not hunt for installers. GitHub may be blocked, so automatic install can fail.

## Decision

- Discover binaries in the user data directory first (`…/biflow/apps`, `…/biflow/bin`), then `PATH`, `~/.local/bin`, and common system paths (`hiddify`, `hiddify-app`, `mihomo`, `clash-meta`, Hiddify AppImages).
- The Install button is shown only when that discovery finds no binary. A present install must not show Install.
- Package a pinned, SHA-256 verified Mihomo executable for each release target and
  install that local resource first. This makes the normal Mihomo install path
  independent of GitHub availability.
- If the bundled resource is absent, download only allowlisted GitHub release
  URLs for the current OS. Verify the compressed archive and extracted binary,
  and retry once without environment proxy settings when the proxy-aware
  request fails.
- On failure, show a step-by-step modal and an official download link.
- Linux prefers the Hiddify AppImage (no root). Windows prefers the portable zip, then the silent setup.

## Consequences

- Connect can offer Install instead of a dead-end error.
- Mihomo installation works on first launch without network access; packages are
  larger because they carry one target-specific executable.
- Helper still starts Mihomo from its configured path; user-local Mihomo is used for discovery/validation first.
