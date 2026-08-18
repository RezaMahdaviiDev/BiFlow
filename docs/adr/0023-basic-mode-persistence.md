# ADR 0023: Basic mode UI persistence

Date: 2026-08-13  
Status: Accepted

## Context

BiFlow needs a distraction-free Basic interface alongside the existing Advanced
dashboard. Users must be able to switch modes without losing access to connect,
pause, resume, and disconnect controls. The preference must survive restarts.

## Decision

- Store the UI mode under the versioned localStorage key `biflow-ui-mode-v1`.
- Default missing or invalid values to **Basic** so a first launch shows the
  connect-only dashboard. Existing installs that already stored Advanced keep
  that preference. Connect still installs missing services in Basic (ADR 0044).
- Render Basic mode as a dedicated minimal dashboard with only the segmented
  mode control, lifecycle actions, progress/cancel, and concise inline errors.
  On mobile the five-item bottom navigation stays visible. Rules, Diagnostics,
  and Settings switch the stored mode to Advanced and open that page.
- Keep About reachable from the tray menu even when Basic mode hides
  desktop navigation. Selecting Basic from About (or any other page) returns to the
  Basic dashboard so the control matches every other screen.

## Consequences

- Frontend tests cover persistence, keyboard navigation, and Basic visibility.
- A future mode redesign can migrate users by introducing `biflow-ui-mode-v2`
  without rewriting lifecycle commands.
