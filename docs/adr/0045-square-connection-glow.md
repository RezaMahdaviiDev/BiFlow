# ADR 0045: Square connection glow

## Status

Accepted

## Context

The connected-state ring used a 2px border and `0.5rem` radius. That left a
visible gap at the window corners and looked thin on scaled Linux and Windows
displays.

## Decision

- Keep the glow on a `pointer-events: none` overlay (`::after`), not the
  shell border, so layout size does not change.
- Use `border-radius: 0` so the ring meets the window edges.
- Increase the border to 3px and strengthen the inset/outer shadows. Lengths
  stay in CSS pixels so 125%/150% display scaling thickens the ring with the
  rest of the UI.

## Consequences

- Playwright still asserts `data-connection-glow` rather than computed colour.
- Reduced-motion still disables the pulse.
