# ADR 0047: Three representative viewport layouts

## Status

Accepted

## Context

The shell now resizes. Mobile, tablet, and small-desktop sizes need automated
coverage so bottom navigation, the status bar, and page scroll stay correct.

## Decision

- Treat 390×844 as mobile: bottom navigation (Basic and Advanced), no
  hamburger, status bar below the nav. Dashboard content scrolls in the
  shell (`flex flex-col gap-4 pb-2`, no nested `h-full overflow-y-auto`).
  Mobile shows one provider summary card and one five-light connection strip
  instead of the desktop metric and component grids. After Connect, the live
  routing SVG must be reachable with `scrollIntoView`.
- Treat 768×1024 as tablet and 1024×768 as small desktop: sidebar navigation.
- Playwright records a screenshot of each size and asserts no horizontal
  overflow and no nav/status overlap.

## Consequences

- Visual review of the three screenshots is part of the done gate for layout
  work.
