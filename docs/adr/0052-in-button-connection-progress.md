# ADR 0052: In-button connection progress

## Status

Accepted

## Context

Connect, Disconnect, Pause, and Resume showed a standalone progress card
driven by stack phase names. That hid the current action, used a second
status region, and did not describe stop or pause milestones.

## Decision

- Publish `StackSnapshot.operation_stage` from the engine at each real
  milestone (prepare, start Hiddify/runtime/config/Mihomo/readiness, stop
  core, stop proxy, clean up, recover).
- Remove the standalone progress card. The active connection button shows
  the stage label and an animated fill whose width is the published percent.
- Progress is never a fake timer. The mock transport emits the same stages
  the engine does so UI tests stay honest.
- Keep labels short and wrapping (`break-words`, no `truncate`) so they fit
  the 390, 768, and 1024 viewports without clipping or shifting layout.
- Linux and Windows share the same React control and CSS.

## Consequences

- Accessible names change to the current stage while an operation runs.
- Cancel remains a separate control. The About updater still owns
  `role="progressbar"`.
