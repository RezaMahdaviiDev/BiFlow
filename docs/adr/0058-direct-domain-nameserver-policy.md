# ADR 0058: DIRECT domain nameserver policy

## Status

Accepted

## Context

`test_route` only runs `RuleSet::decide`. A custom DIRECT pin can show
`console.kavenegar.com → DIRECT` while the OS resolver still returns a Mihomo
fake-ip (`198.18.0.0/16`). `private.txt` includes `198.18.0.0/15`, and DNS for
unpinned lookups uses Cloudflare DoH, so Iranian-only hosts often fail even
after a pin. ADR 0056 deferred `nameserver-policy` until a witness; this is
that witness.

The clash-master fixture already maps `rule-set:custom-direct-domains` and
`rule-set:iran-domains` to Shecan.

## Decision

- Generate `dns.nameserver-policy` for `rule-set:custom-direct-domains`,
  `rule-set:iran-domains`, and `rule-set:iran-business-domains` using the
  operator-selected DIRECT DNS (default fake-ip; Iranian resolvers are opt-in,
  see [0060](./0060-fake-ip-default-direct-dns.md)).
- Put the same three rule-sets in `fake-ip-filter` so those names skip fake-ip
  and the browser talks to the real address.
- Keep Cloudflare DoH as the default `nameserver` (Windows still `#VPN`). Do
  not log resolved hosts or IPs.

## Consequences

- Test flow stays a pin planner. Live connections remain the session authority.
- Operators must Pause/Connect (or relaunch) after this config change so Mihomo
  reloads DNS. A pin alone does not rewrite an existing fake-ip cache entry.
