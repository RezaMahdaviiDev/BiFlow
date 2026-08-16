# ADR 0046: Persist window size

## Status

Accepted

## Context

The main window was locked at 1120×760. Operators on smaller or larger
displays could not keep a chosen size across launches.

## Decision

- Make the window resizable with a 390×640 minimum and a 1120×760 default.
- Persist logical width/height to `window-size.json` on every resize.
- Restore on startup, clamped to the current monitor work area so a saved
  size cannot open off-screen or below the supported minimum when the
  display allows it.

## Consequences

- Mobile, tablet, and small-desktop viewports can be used in the packaged
  app, not only in the mock browser.
- A work area smaller than 390×640 wins so the window stays on the display.
