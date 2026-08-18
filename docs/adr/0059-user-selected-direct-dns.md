# ADR 0059: User-selected DIRECT DNS resolvers

## Status

Accepted

## Context

ADR 0058 maps Iranian and user-pinned DIRECT domains to Shecan so those names
skip Mihomo fake-ip. Operators still need other Iranian resolvers (Electro,
Radar Game, Mokhaberat `5.200.200.200`) or their own IPs. Hardcoding Shecan
made that change a rebuild.

## Decision

- Store `mihomo.direct_dns_preset` (`shecan`, `electro`, `radar`,
  `mokhaberat`, `custom`) and `mihomo.direct_dns_servers` (custom only) on
  `MihomoConfig`. Missing keys keep Shecan. Do not bump `schema_version`.
- Named presets ignore the custom list. Custom accepts one to four IP
  addresses, not host names. Reject loopback, unspecified, multicast, and
  `198.18.0.0/15`. Allow RFC1918 so Radar (`10.202.10.10`) stays valid.
- Settings → Mihomo exposes the presets and a custom field. Saving updates
  the in-memory backend config for the **next** Connect; it does not live-reload
  Mihomo DNS. Log the preset name only, never custom IPs.
- VPN `nameserver` stays Cloudflare DoH (Windows still `#VPN`).

## Consequences

- Pause then Connect (or relaunch) after a DNS change so fake-ip cache and
  `direct-nameserver` reload.
- Radar's RFC1918 resolvers only work on networks that route them.
