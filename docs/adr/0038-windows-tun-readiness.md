# ADR 0038: Windows TUN readiness and clash-master alignment

## Status

Accepted

## Context

A 3.0.0 Windows field log (`debug (3).log`) showed Connect getting all the
way through helper install, Hiddify SOCKS egress, `start_mihomo`, and
controller readiness (`ready: 7`, `rules_loaded: 65734`). Two milliseconds
later the engine rolled Mihomo back with:

`Mihomo failed to start: process or TUN disappeared during readiness checks`.

The Windows backend cannot enumerate adapters (`unsafe_code = "forbid"`,
ADR 0033), so it treats Mihomo `GET /configs` as the TUN authority. That
probe was a false negative:

1. It required `tun.device` to equal `clash-iran`. Windows Mihomo commonly
   reports `Meta`, an empty name, or a Wintun path while the tunnel is up.
2. It accepted only a JSON boolean `enable`. Some builds echo `1` or
   `"true"`.
3. A `/configs` error was swallowed as "TUN down", so a single failed
   probe killed a working core.
4. The engine checked process and TUN once, with no settle window, then
   used one combined error for both failures.

The proven clash-master Windows stack (`start.ps1` / `config.yaml`) treats
controller + providers as start success, sets `find-process-mode: always`,
disables IPv6, pins DoH to `#VPN`, and never fails start because the
adapter name in `/configs` differs from `clash-iran`.

## Decision

- Treat `tun.enable` as truthy (`true` / `1` / `"true"`). A present tunnel
  is enough; a device-name mismatch is not a Connect failure.
- Log the `/configs` error or `enable`/`device` pair when the probe is
  negative. Do not log the rest of `/configs` (it can carry the
  controller secret).
- After providers are ready, retry process + TUN for 5 seconds and name
  which of the two failed.
- Generate Windows Mihomo YAML like the working clash-master profile:
  `find-process-mode: always`, `auto-redirect: false`, `ipv6: false`, and
  `https://…/dns-query#VPN`. Linux keeps IPv6 and unpinned DoH.

## Consequences

Windows Connect no longer tears down a ready Mihomo because Wintun
reported a different device name. The next field log can tell process
death from TUN-not-enabled. Linux routing is unchanged except for
`find-process-mode: always` and `auto-redirect: false`, which match the
original clash stack and the existing PROCESS-NAME bypass rules.
