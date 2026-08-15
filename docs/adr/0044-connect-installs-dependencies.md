# ADR 0044: Connect installs required services

## Status

Accepted

## Context

Connect previously started the stack immediately. A missing helper, Hiddify
binary, or Mihomo binary then failed mid-start and left the operator to press
separate Install buttons.

## Decision

- Before `start_stack` (UI Connect, tray Connect, and the Tauri command),
  install missing services in a fixed order: helper, then Hiddify, then Mihomo.
- Each step is idempotent. Already-present services are skipped. A failed step
  aborts before the stack starts and keeps the last useful error or install
  guide.
- The UI sets `installingId` so the matching Install control shows progress
  while Connect is preparing.

## Consequences

- First-run Connect in Basic or Advanced can finish setup without a separate
  install pass.
- Helper install still needs an interactive polkit or UAC prompt; that failure
  is reported instead of hanging the start path.
