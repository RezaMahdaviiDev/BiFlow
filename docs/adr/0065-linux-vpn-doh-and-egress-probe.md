# ADR 0065: Linux VPN DoH and multi-path Hiddify egress probe

## Status

Accepted (amends [0018](./0018-hiddify-egress-before-tun.md) and
[0038](./0038-windows-tun-readiness.md))

## Context

A 4.4.1 Linux `debug.log` showed Connect stuck on Start Hiddify for ~45s while
the Hiddify window was already open, then `hiddify.egress_probe_exhausted`.
The probe error was labeled `Mihomo controller request failed` because
`probe_hiddify_egress` reused `MihomoError::Http`. Failures were ~200ms apart,
so the SOCKS handshake or a single `gstatic.com/generate_204` died immediately
— not a 5s timeout. The same session also reported Google hosts failing after
a successful connect: Linux DoH was unpinned (`https://1.1.1.1/dns-query`),
which Iranian WANs often block, leaving VPN destinations on fake-ip with no
usable mapping.

## Decision

- Probe Hiddify over `socks5h` then `http` on the configured mixed port.
  Try Cloudflare then gstatic `generate_204` (HTTPS and HTTP). Ignore
  environment proxies. Log `EgressProbe` with the proxy kind and status, never
  a URL.
- Pin Mihomo `nameserver` to `#VPN` on Linux as well as Windows so Google and
  other MATCH hosts resolve through Hiddify.

## Consequences

- Connect still requires a working Hiddify node. The UI says so instead of
  blaming Mihomo.
- If a future node relays QUIC, ADR 0063 stays independent of this DNS pin.
