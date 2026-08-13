# ADR 0025: Paused lifecycle and Hiddify ownership

## Status

Accepted

## Context

Disconnect tears down the owned TUN, routes, DNS, and Mihomo, and may also stop
Hiddify when `stop_with_stack` is set. Users need a lighter Pause that stops
BiFlow interception without dropping the already-running Hiddify process, then
Resume to rebuild a fresh Mihomo generation. App updates and quit must choose
Pause versus Disconnect explicitly.

This extends [0018](./0018-hiddify-egress-before-tun.md), which already leaves
Hiddify running on Mihomo rollback.

## Decision

- Add `StackPhase::Paused` and dedicated `pause_stack` / `resume_stack`
  operations. Do not overload `stop_stack`.
- Pause stops Mihomo, removes owned TUN/routes/DNS, verifies the OS is no longer
  intercepting, and **never** calls `stop_user_proxy`. Hiddify stays running.
- Resume reuses the Connect start path from `Paused`. A failed resume rolls
  owned state back and remains `Paused` with `last_error` set.
- Repeated Pause or Resume while already in the matching phase is idempotent.
- The Advanced dashboard, Basic dashboard, and tray expose Pause + Disconnect
  while running and Resume + Disconnect while paused.
- Signed in-app updates pause the owned stack before installing so Hiddify can
  keep running unless the installer itself requires exit.

## Consequences

- Direct egress during pause is a first-class state, not a half-disconnected
  error.
- Structured `debug.log` events record pause/resume without URLs, secrets, or
  rule values.
- Core unit tests cover Hiddify preservation, resume, and failed-resume rollback.
