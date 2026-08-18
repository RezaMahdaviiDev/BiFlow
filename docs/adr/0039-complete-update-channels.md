# ADR 0039: Complete update channels

## Status

Accepted

## Context

`check_for_update` and `install_update` only covered the signed BiFlow package
(AppImage / NSIS). Iran rule snapshots and the bundled Mihomo binary are also
versioned assets, but they lived on separate manual buttons. Operators expected
one Install action to refresh the app and those sidecars.

## Decision

- Replace the application binary with the GitHub Releases package from ADR 0057
  (`.deb` via `pkexec apt-get`, AppImage replace helper, Windows NSIS `/S`).
  Linux `.deb` no longer opens the Release page.
- `check_for_update` also compares the cached rule revision to the BiFlow
  manifest and reports a missing bundled Mihomo as a third-party channel.
- `install_update` first syncs cloud rules and installs Mihomo when it is
  missing, then applies the GitHub package when an app update exists.
- A rule-sync failure keeps the last good snapshot and does not block an
  application update. Mihomo install failure is fatal for that step.
- When only sidecars change, Install finishes without restarting. There is no
  background update poll.

## Consequences

- About can show pending rule and Mihomo work beside a GitHub app version.
