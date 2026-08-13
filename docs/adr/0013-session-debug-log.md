# ADR 0013: Permanent structured debug log

## Status

Accepted

## Context

Rust operations emitted a small number of `tracing` events, but the desktop
process did not install a subscriber. Those events therefore disappeared in
development and packaged builds, and user support bundles could not connect a
reported failure to its initiator or sequence of internal operations.

The diagnostic record must remain useful after the application exits, make its
growth visible and user-controlled, and avoid exposing settings secrets,
subscription links, or other credentials.

## Decision

- Initialize one newline-delimited JSON `debug.log` before Tauri setup in the
  per-user BiFlow data directory on development and packaged builds. Open it in
  append mode so evidence remains available across launches and shutdowns.
- Flush every complete event and append lifecycle open/close records. Closing
  the visible window only hides it to the tray and does not end the diagnostic
  session.
- Do not automatically truncate, rotate, cap, or delete the file. Diagnostics
  displays its live byte size and full path. The user can reveal its containing
  folder or explicitly clear its contents after confirmation; the open handle
  remains valid and immediately records a new `log.cleared` event.
- Record UTC time, severity, section, event, initiator, cause, trace ID, trace
  route, source location, thread, and active spans. Tauri commands establish a
  new trace; engine operations and helper IPC add their operation/request IDs.
- Redact sensitive field names, credential assignments, and URLs in the final
  writer before data reaches disk. Callers still avoid logging raw settings,
  rules, targets, URLs, or user content.
- Capture Rust panics and ignored background/tray failures. The privileged
  helper keeps its service-journal audit, while the desktop records the request
  and result at its side of the IPC boundary.
- Include an immediately flushed copy of `debug.log` in every support bundle.

## Consequences

The desktop has one persistent causal trail that a user can locate and send for
debugging. A crash can omit the normal close event, but earlier records are
already flushed. The file may grow until the user clears it, so its size is
visible in the app. The log never leaves the device automatically and
intentionally excludes sensitive payloads.
