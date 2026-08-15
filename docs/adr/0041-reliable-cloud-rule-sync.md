# ADR 0041: Reliable cloud rule sync

## Status

Accepted

## Context

`CloudRuleStore::sync` downloaded each file once and wrote it straight into
the cache. A hung or proxied request left the operator on a spinning button,
and a mid-write failure could mix a new file with old metadata.

## Decision

- Retry each fetch three times with backoff. The HTTP client tries the
  environment proxy first, then a `no_proxy` client.
- Hold a store mutex so two syncs cannot interleave.
- Write the validated generation into `.staging`, then atomically persist
  each rule file and `sync-meta.json` last. Remove staging on success or
  failure. The previous cache stays the last known good set when publish
  never starts.
- The UI ignores a second Update-from-cloud click while `actionPending`.

## Consequences

- Transient GitHub or proxy failures no longer fail the first attempt.
- A leftover `.staging` directory cannot poison the next sync.
