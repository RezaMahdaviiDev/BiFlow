# ADR 0063: Reject QUIC toward the VPN

## Status

Accepted

## Context

Live connections through the Hiddify egress showed UDP 443 flows with upload
but zero download (`beacons2.gvt2.com udp up:7500 down:0`): QUIC packets leave
and nothing comes back. Browsers that try HTTP/3 hang on that blackhole
instead of falling back to TCP, which reads as "YouTube does not open" while
plain TCP through the same egress returns 200.

## Decision

Emit `AND,((NETWORK,udp),(DST-PORT,443)),REJECT` immediately before
`MATCH,VPN`. Every DIRECT source (process, loopback, LAN, custom pins, Iran
rule-sets) sits above it, so DIRECT destinations keep QUIC; only VPN-bound
UDP 443 is rejected, and the browser retries over TCP at once. Validated with
`mihomo -t` against the bundled binary.

## Consequences

- HTTP/3 to foreign sites downgrades to TCP; that is the working path.
- If a future egress relays UDP correctly, remove the rule to restore QUIC.
