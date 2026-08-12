# ADR 0005: Fail-safe Iran rule updates

## Status

Accepted

## Context

DIRECT routing depends on Iranian domains and CIDRs. The live source is [Chocolate4U/Iran-clash-rules](https://github.com/Chocolate4U/Iran-clash-rules). GitHub raw is often unreachable from Iran.

## Decision

Refresh `ir.txt`, `ircidr.txt`, and `private.txt` in this order:

1. GitHub raw `release` branch
2. jsDelivr
3. Fastly jsDelivr
4. GitHub `releases/latest/download`

Reject truncated files (minimum entry counts). Keep the last good cache; if none exists, keep the bundled snapshot. The Direct rules screen shows domain count, IP count, last sync time, and an update button.

## Consequences

- Routing still works offline.
- `test_route` and runtime generation read cache before bundled files.
