# ADR 0004: In-app Hiddify and Mihomo install

## Status

Accepted

## Context

BiFlow requires Hiddify (upstream proxy) and Mihomo (split core). Users should not hunt for installers. GitHub may be blocked, so automatic install can fail.

## Decision

- Discover binaries in the user data directory first (`…/biflow/apps`, `…/biflow/bin`), then common system paths.
- Download only allowlisted GitHub release URLs for the current OS.
- On failure, show a step-by-step modal and an official download link.
- Linux prefers the Hiddify AppImage (no root). Windows prefers the portable zip, then the silent setup.

## Consequences

- Connect can offer Install instead of a dead-end error.
- Helper still starts Mihomo from its configured path; user-local Mihomo is used for discovery/validation first.
