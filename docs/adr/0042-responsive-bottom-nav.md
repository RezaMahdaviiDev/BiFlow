# ADR 0042: Responsive bottom navigation

## Status

Accepted

## Context

The Advanced shell used a 15rem sidebar and a fixed 1120×760 window. Mobile
and tablet viewports need the same five destinations without a hamburger menu,
and the status bar must not sit under the nav.

## Decision

- Treat `max-width: 767px` as mobile. Mobile always renders the bottom
  navigation bar (icons + labels), including Basic mode. Tapping Rules,
  Diagnostics, or Settings from Basic writes Advanced and opens that page.
  Desktop/tablet keep the sidebar.
- Stack `main` (scroll), optional bottom nav, then the status bar as flex
  siblings so they never overlap.
- Do not introduce a hamburger control.

## Consequences

- Playwright and browser checks at 390×844 exercise the bottom bar.
- 768px and above keep the sidebar so tablet/desktop layouts stay familiar.
