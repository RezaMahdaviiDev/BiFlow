# ADR 0043: Persistent traffic totals

## Status

Superseded by session-scoped in-memory totals (this document).

## Context

The status bar showed no sent or received counters. Mihomo's `/connections`
totals reset whenever the core restarts. Folding every poll into
`traffic-totals.json` made Quit/relaunch keep a lifetime total and double-counted
when the same snapshot was polled twice.

## Decision

- Read Clash `uploadTotal` / `downloadTotal` from the loopback controller while
  the stack is `running` or `degraded` (`uploadTotal` → sent, `downloadTotal` →
  received).
- Keep an in-memory `SessionAccumulator` on `AppServices` whose lifetime is the
  desktop process. Each generation contributes only the delta from the last
  snapshot of that generation. A repeated poll of the same counters adds zero.
  A counter decrease folds the new baseline as a fresh generation.
- Disconnect keeps the displayed session total and clears the generation
  cursor. Hide-to-tray does not reset. Quit/relaunch starts at zero.
- Do not read or write `traffic-totals.json` on the normal path. A leftover
  file from an older version is ignored.

## Consequences

- The status bar formats session bytes with kibibyte-based units through
  pebibytes. A failed probe keeps the last displayed total.
- Lifetime persistence is no longer a product requirement.
