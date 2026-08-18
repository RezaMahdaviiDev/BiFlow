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
- Native Tauri builds read and write through `tauri-plugin-clipboard-manager`
  (`clipboard-manager:allow-read-text` / `allow-write-text`). Vite and
  Playwright keep `navigator.clipboard`. Copy, cut, and paste surface a short
  error instead of swallowing `NotAllowedError`.
- Direct Rules and `pinRoute` run `extractHost` before the API call so a pasted
  URL becomes a host.
- `addRule` / `pinRoute` rethrow after recording the store error so Direct
  Rules clears the input only on success.

## Consequences

- Playwright grants clipboard permissions and clicks Paste on Diagnostics and
  Direct Rules. Vitest covers a rejected clipboard read.
