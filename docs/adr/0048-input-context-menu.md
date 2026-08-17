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
- Paste captures the field **before** the async clipboard read, uses the native
  `HTMLInputElement`/`HTMLTextAreaElement` value setter, and dispatches a
  bubbling `InputEvent` with `inputType` so controlled React inputs update.
  Unmount or disable after the read is a no-op.
- `addRule` / `pinRoute` rethrow after recording the store error so Direct
  Rules clears the input only on success.

## Consequences

- Playwright and Vitest cover the menu on the diagnostics target field.
- Clipboard paste uses the standard `navigator.clipboard` API.
