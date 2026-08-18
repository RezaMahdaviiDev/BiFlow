# ADR 0060: Fake-ip is the default DIRECT DNS

## Status

Accepted (amends [0058](./0058-direct-domain-nameserver-policy.md) and
[0059](./0059-user-selected-direct-dns.md))

## Context

ADR 0058/0059 defaulted DIRECT domains to Shecan and skipped fake-ip. On live
networks that made Iranian and other DIRECT hosts slow or unreachable, so the
fix became the regression.

## Decision

- Default `direct_dns_preset` is `fake_ip`: Mihomo fake-ip plus Cloudflare DoH.
  Do not emit `nameserver-policy`, `direct-nameserver`, or DIRECT rule-sets in
  `fake-ip-filter`.
- Shecan, Electro, Radar, Mokhaberat, and Custom stay in Settings as opt-in.
  They still apply ADR 0058 policy only when selected.
- Schema 2 rewrites a stored `shecan` value from 3.9.0–3.9.1 (the implicit
  default) to `fake_ip`. A stored Mokhaberat/Electro/Radar/Custom value is kept.

## Consequences

- Pause/Connect after the update so Mihomo reloads DNS.
- Operators who still need Shecan for a specific DIRECT host can select it
  again in Settings.
