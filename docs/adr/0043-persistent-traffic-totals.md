# ADR 0043: Persistent traffic totals

## Status

Accepted

## Context

The status bar showed no sent or received counters. Mihomo's `/connections`
totals reset whenever the core restarts, so displaying that session snapshot
alone would zero the bar on every disconnect or reconnect.

## Decision

- Read Clash `uploadTotal` / `downloadTotal` from the loopback controller while
  the stack is `running` or `degraded`.
- Fold each session into a lifetime file (`traffic-totals.json`) so a later
  session adds to the previous total. A mid-session Mihomo restart that drops
  the counters also folds the previous session first.
- The status bar formats lifetime bytes with kibibyte-based units through
  pebibytes. A failed probe keeps the last displayed total.

## Consequences

- Disconnect and reconnect no longer reset the counters.
- The file lives in the per-user data directory and is not a diagnostic secret.
