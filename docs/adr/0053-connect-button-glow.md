# ADR 0053: Connect button availability glow

## Status

Accepted

## Context

The window ring (ADR 0045) shows a live stack. Operators also need a cue that
Connect itself is the next action, without fighting the in-button progress
fill added in ADR 0052.

## Decision

- Apply `.connect-button-glow` only to the Connect control, and only while it
  is enabled and idle.
- Animate `box-shadow` on the button. The progress fill stays on an inner
  clipped layer so the two motions do not share the same property.
- Disable the class as soon as the control is disabled or processing.
- Honor `prefers-reduced-motion` with a static outline and no pulse.
- Linux and Windows share the same CSS.

## Consequences

- Playwright asserts `data-connect-glow` rather than computed shadow colour.
- Resume and Disconnect stay un-glowed so Connect remains the invitation.
