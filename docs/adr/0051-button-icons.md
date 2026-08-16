# ADR 0051: Icons on every button

## Status

Accepted

## Context

Lifecycle and several dialog buttons were text-only, while nav, diagnostics,
and install actions already used Lucide. Icon-only chrome (theme, language,
rule pins) had accessible names but no tooltip.

## Decision

- Use Lucide for every button, with a shared 18px icon size and `gap-2`
  alignment via `AppButton`.
- Icon-only controls use `IconOnlyButton` so `aria-label` and `title` stay in
  sync.
- Connection actions use Power / PowerOff / Pause / Play / X so the later
  in-button progress work can keep the same glyphs.

## Consequences

- Accessible names stay on the visible label; decorative SVGs are `aria-hidden`.
