# ADR 0011: Logo-derived palette

## Status

Accepted

## Context

The desktop interface used a green primary color while the BiFlow logo uses
navy, route blue, and flow teal. The request is a color change only; the
existing layout, component markup, and light/dark toggle already work.

## Decision

- Keep the existing React components and light/dark behavior unchanged.
- Define the recolor through the existing RGB channel tokens in
  `apps/desktop/src/index.css`; do not introduce another stylesheet, theme
  runtime, component class system, or theme mode.
- Use accessible `#0068e6` for the primary blue. It has at least 4.5:1
  contrast with the white text already used by primary buttons.
- Use logo-aligned navy surfaces and teal success/route accents in both modes.
- Keep a focused unit test for the palette values and primary-button contrast.

## Consequences

Tailwind utilities and the traffic visualization inherit the new palette
without any TSX changes. Future color-only work remains localized to the same
tokens.
