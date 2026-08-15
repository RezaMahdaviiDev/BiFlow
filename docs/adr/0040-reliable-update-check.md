# ADR 0040: Reliable update check

## Status

Accepted

## Context

`check_for_update` could sit on a hung GitHub request until the operator
force-quit. A second Check or Install could start another plugin call. Cancel
only stopped the routing stack.

## Decision

- Bound each plugin `check()` with an 8-second timeout and keep the existing
  four attempts plus exponential backoff.
- Hold a process-wide update lock (`try_lock`) so Check, Install, and the
  background poll never overlap. A busy lock returns a clear error (or is
  skipped for the background poll).
- `cancel_operation` also sets the update cancel flag so a retry loop exits
  instead of sleeping out the remaining attempts.
- Bound `download_and_install` with a ten-minute timeout.
- The About store ignores a second Check/Install while a phase is already
  in flight, so the UI does not stack progress states.

## Consequences

- A dead resolver no longer freezes About; the operator sees a timeout after
  the retry budget.
- Sidecar work from ADR 0039 shares the same lock as the signed package.
