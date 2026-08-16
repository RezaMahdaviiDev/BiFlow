# ADR 0050: Connection operation lock

## Status

Accepted

## Context

Connect, Disconnect, Pause, and Resume could be started from the dashboard,
Basic mode, and the tray at the same time. The engine only deduplicated the
same operation kind, so a stop could queue while a start was still running and
the UI re-enabled on every intermediate snapshot.

## Decision

- Publish `StackSnapshot.busy` as `connecting`, `disconnecting`, `pausing`,
  `resuming`, or `reconciling`.
- `Engine::accept` holds one shared reservation. A second kind returns
  `OperationInProgress`. The same kind is reused.
- Reserve the lock before Connect's dependency install so tray and UI cannot
  start a second prepare step.
- Bound each worker item with a timeout (120s start/resume, 45s stop/pause).
  Timeout cancels the token, sets `Error`, and clears `busy`.
- Disable the tray Connect/Disconnect and Pause/Resume items while `busy` is
  set. Quit stays available.
- The React store ignores a second click and keeps controls locked until
  `busy` clears, a command error, or a 130s watchdog.

## Consequences

- Disconnect no longer cancels an in-flight Connect; use Cancel operation.
- Restart still waits for Stopped, then starts, so it never overlaps.
