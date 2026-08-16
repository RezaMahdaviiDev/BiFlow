# ADR 0048: Input context menu

## Status

Accepted

## Context

A capture-phase `contextmenu` guard blocked the native menu everywhere,
including text and number fields. Operators could not select, copy, cut, or
paste from a right-click.

## Decision

- Keep blocking `contextmenu` on chrome.
- Allow the event on text, number, and textarea fields, then show a custom
  menu: Select All, Copy, Cut, Paste.
- Disable Copy/Cut when there is no selection, and disable Cut/Paste on
  read-only or disabled fields.

## Consequences

- Playwright and Vitest cover the menu on the diagnostics target field.
- Clipboard paste uses the standard `navigator.clipboard` API.
