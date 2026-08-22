# ADR 0067: OpenVPN as a side tunnel that never owns the default route

## Status

Accepted

## Context

Users who already run BiFlow's DIRECT / Hiddify split also need a second,
unrelated tunnel: a corporate or private OpenVPN whose internal networks are
unreachable any other way. The obvious approach — launch the `.ovpn` profile
and let it do what it wants — fails badly here. A stock client profile carries
`redirect-gateway def1`, or the server pushes one, and OpenVPN installs itself
as the system gateway. From that moment every packet the machine sends depends
on a tunnel BiFlow does not manage: when it drops, or the server is
unreachable, the whole machine loses its internet, Hiddify included.

There is a second problem. `.ovpn` files are user-supplied input, and BiFlow's
privileged helper runs as root. OpenVPN executes arbitrary commands through
`up`, `down`, `route-up`, and `plugin` directives, so handing an unaudited
profile to a root process is a straightforward local privilege escalation.

## Decision

Run OpenVPN as a **side tunnel**, started after Hiddify and before the Mihomo
generation, under four rules.

1. **OpenVPN never touches the routing table.** The helper always passes
   `--route-noexec`. Every route in the kernel is installed by the helper from
   a list it validated: the tunnel's own network (read from the interface once
   it is up), the scoped routes parsed out of the server's `PUSH_REPLY`, and
   the CIDRs the user listed. A default route is rejected in the config
   validator, in the IPC validator, and again in the helper.
2. **A profile that can run code is refused.** The helper scans for `up`,
   `down`, `plugin`, `script-security` and their relatives and refuses to
   start, then pins `--script-security 0` after `--config` as a second
   barrier. `--pull-filter ignore` entries drop `redirect-gateway`, pushed
   default routes, and pushed DNS.
3. **Selected traffic reaches the tunnel through Mihomo, not the main table.**
   The generated configuration gains an `OpenVPN` outbound of type `direct`,
   bound to the helper-owned device and stamped with a firewall mark on Linux;
   the helper puts the tunnel default in its own policy table that only marked
   packets can reach. Windows has no marks, so the interface binding is the
   whole mechanism there. Hosts are pinned to the side tunnel from the rules
   table, exactly like the existing DIRECT and VPN pins.
4. **The side tunnel is optional and fails alone.** `openvpn.required` is off
   by default: a tunnel that will not start marks its own component degraded
   and Connect proceeds with the existing split. The engine still tears a
   started tunnel down when a later step fails.

Two ordering choices follow from this and are load-bearing:

- The OpenVPN server's own address is emitted as `IP-CIDR,<server>/32,DIRECT`
  above every other rule. Without it Mihomo hands the tunnel's own transport
  packets back to the split TUN and the side connection never establishes.
- The OpenVPN rule-sets sit **above** `private-networks`. Reaching an RFC1918
  range that lives behind the tunnel is the usual reason to run one, and a pin
  below that line would be answered by the local LAN instead. Loopback stays
  above both, so the machine can always reach itself.

## Consequences

- A dropped side tunnel costs the user the networks behind it and nothing
  else. The DIRECT and Hiddify paths are untouched by design.
- Profiles that rely on `up`/`down` scripts do not work and cannot be made to
  work without weakening the root boundary. That is the intended trade.
- The rules document and the Mihomo generation both carry a third route, so
  `custom-openvpn-domains.txt` and `custom-openvpn-ips.txt` are always
  written, empty included — Mihomo refuses to load a rule-set whose file is
  missing.
- With no tunnel running, the `OPENVPN` proxy group resolves to `DIRECT`, so a
  stale pin degrades to the local internet rather than a black hole.
