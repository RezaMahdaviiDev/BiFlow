# ADR 0023: Basic mode UI persistence

Date: 2026-08-13  
Status: Accepted

## Context

BiFlow needs a distraction-free Basic interface alongside the existing Advanced
dashboard. Users must be able to switch modes without losing access to connect,
pause, resume, and disconnect controls. The preference must survive restarts.

## Decision

- Store the UI mode under the versioned localStorage key `biflow-ui-mode-v1`.
- Default missing or invalid values to **Advanced** so existing installs keep
  every current screen and capability until they opt into Basic.
- Render Basic mode as a dedicated minimal dashboard with only the segmented
  mode control, lifecycle actions, progress/cancel, and concise inline errors.
- Keep About reachable from the tray menu even when Basic mode hides
  navigation. Selecting Basic from About (or any other page) returns to the
  Basic dashboard so the control matches every other screen.

## Consequences

- Frontend tests cover persistence, keyboard navigation, and Basic visibility.
- A future mode redesign can migrate users by introducing `biflow-ui-mode-v2`
  without rewriting lifecycle commands.
