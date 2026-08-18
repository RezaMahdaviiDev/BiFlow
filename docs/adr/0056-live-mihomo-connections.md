# ADR 0056: Live Mihomo connections in Diagnostics

## Status

Accepted

## Context

`test_route` only runs `RuleSet::decide` and returns `reachable: None`. The UI
can say DIRECT for a host while the browser still uses fake-ip or a VPN
chain. Operators asked which hosts are actually DIRECT vs VPN right now.
Crawling pages from the app is the wrong tool.

## Decision

- Deserialize Mihomo `GET /connections` `connections[]` (`metadata.host`,
  `destinationIP`, `chains`, `rule`).
- Expose `list_active_connections` when the stack is `running` or
  `degraded`; otherwise return an empty list. Classify outbound from the last
  `chains` entry (`DIRECT` → direct, anything else → VPN).
- Diagnostics polls about every two seconds and shows host or IP, outbound,
  and matched rule. `debug.log` records only the row count, never hosts or
  URLs.
- Do **not** add a Mihomo `nameserver-policy` for DIRECT rule-sets in this
  change. Fake-ip addresses while connected are expected until a test witness
  shows Windows DoH `#VPN` skipping `direct-nameserver` for DIRECT
  destinations.

## Consequences

- Static Test flow stays for pin decisions. The live table is the authority
  for what Mihomo is handling in this session.
- Mock and Playwright cover two fixture rows after Connect.
