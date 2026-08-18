# ADR 0061: DIRECT domains always skip fake-ip

## Status

Accepted (amends [0060](./0060-fake-ip-default-direct-dns.md), reaffirms the
[0058](./0058-direct-domain-nameserver-policy.md) `fake-ip-filter` fix)

## Context

ADR 0060 made `fake_ip` the default DIRECT DNS and, to avoid Shecan, stopped
emitting the DIRECT rule-sets in `fake-ip-filter` together with
`nameserver-policy`. That coupled two independent concerns:

1. **Which resolver** answers a DIRECT domain (`nameserver-policy` /
   `direct-nameserver`).
2. **Whether a DIRECT domain is allowed to receive a fake IP at all**
   (`fake-ip-filter`).

Removing the rule-sets from `fake-ip-filter` reintroduced the exact regression
ADR 0058 fixed: a DIRECT domain resolves to a `198.18.0.0/16` fake IP, which is
covered by `private.txt`'s `198.18.0.0/15`. The `RULE-SET,private-networks,
DIRECT,no-resolve` rule sits above the domain rules, so the connection pins to
the fake IP and never reaches the real host. Witnesses: `console.kavenegar.com`,
`iran.ir`, and any operator-pinned DIRECT domain — not a fixed set of hosts.

Independent DNS checks (Cloudflare, Google, Shecan, Electro) returned identical
real IPs for the witnesses, so the failure is the fake-ip round trip itself, not
the resolver. Skipping fake-ip is therefore correct for **every** DIRECT domain,
regardless of the selected resolver.

## Decision

- `fake-ip-filter` always contains `rule-set:custom-direct-domains`,
  `rule-set:iran-domains`, and `rule-set:iran-business-domains`, for every
  `direct_dns_preset` including `fake_ip`. These three rule-sets are the
  complete set of DIRECT **domain** sources, so this covers all DIRECT domains,
  not just Iranian ones.
- `nameserver-policy` / `direct-nameserver` stay gated on
  `applies_direct_policy()` (i.e. only for the opt-in Iranian/custom presets).
  The `fake_ip` default keeps Cloudflare DoH as `nameserver` — no Shecan
  slowdown returns.
- IP-based DIRECT sources (`private-networks`, `iran-networks`,
  `custom-direct-ips`) are unaffected: they never go through fake-ip.

## Consequences

- Pause/Connect after the update so Mihomo reloads DNS and drops stale fake-ip
  cache entries. A pin alone does not rewrite an existing fake-ip cache entry.
- DIRECT domains resolve to real addresses and connect over the local path even
  on the default preset. Operators only pick an Iranian resolver when a host
  additionally needs Iranian-resolver answers, not to make DIRECT work at all.
