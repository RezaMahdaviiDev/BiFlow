# ADR 0040: Reliable update check

## Status

Accepted

## Context

`check_for_update` could sit on a hung GitHub request until the operator
force-quit. A second Check or Install could start another plugin call and
surface `"an update is already in progress"` on About. Install paused the
stack, then called plugin `check()` again (three to nine times) before
downloading.

## Decision

- Bound each plugin `check()` with an 8-second timeout and keep the existing
  four attempts plus exponential backoff.
- `UpdateCoordinator` holds a queryable snapshot (`operation_id`, initiator,
  phase, percent, version, channel flags, final error). `get_update_state`
  and `update-progress` events share that `operation_id`. The UI discards
  events from a different operation while download/install/restart is live.
- A busy `try_begin` is **idempotent**: return the existing snapshot. Never
  send `"an update is already in progress"` to the UI. A background poll that
  finds the lock busy re-emits the current snapshot and must not set `failed`.
- Install uses **one** signed candidate from `fetch_signed_update`. Download
  and signature verify run while the tunnel is up (proxy-aware, then one
  `no_proxy` retry). Pause only after the bytes are verified. Debian packages
  (`APPIMAGE` unset) enter phase `manual` and open the GitHub Release; they
  never pause for a self-replace. Download failure leaves the stack running;
  install failure resumes a paused stack when the process is still alive.
- Record a content-free `direct-rules.upgrade-guard` before replacing the
  binary. After relaunch, a missing `direct-rules.json` when the guard said
  `present` is an upgrade failure.
- Bound download/install with a ten-minute timeout. `cancel_operation` still
  sets the update cancel flag.

## Consequences

- About renders only the coordinator snapshot. Overlapping Check/Install
  clicks no longer produce a red busy banner.
- Sidecar work from ADR 0039 shares the same lock as the signed package.
- Packaged AppImage/NSIS upgrades are covered by this path; a real Debian
  self-replace remains out of scope (manual Release download).
