# ADR 0039: Complete update channels

## Status

Accepted

## Context

`check_for_update` and `install_update` only covered the signed BiFlow package
(AppImage / NSIS). Iran rule snapshots and the bundled Mihomo binary are also
versioned assets, but they lived on separate manual buttons. Operators expected
one Install action to refresh the app and those sidecars.

## Decision

- Keep the Tauri updater plugin as the only path that replaces the application
  binary. Linux `.deb` still opens the Release page (ADR 0024).
- `check_for_update` also compares the cached rule revision to the BiFlow
  manifest and reports a missing bundled Mihomo as a third-party channel.
- `install_update` first syncs cloud rules and installs Mihomo when it is
  missing, then runs the signed self-replace when an app update exists.
- A rule-sync failure keeps the last good snapshot and does not block an
  application update. Mihomo install failure is fatal for that step.
- When only sidecars change, Install finishes without restarting.

## Consequences

- About can show pending rule and Mihomo work beside a signed app version.
- Background polling uses the same combined status so a sidecar-only refresh
  can surface the Install button without an error banner.
