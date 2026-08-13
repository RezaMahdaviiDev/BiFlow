# 0017: Empty custom rule providers are ready

## Status

Accepted

## Context

After Mihomo accepted the generated configuration, Connect still failed with
`Mihomo readiness check timed out`. `debug.log` showed `start_mihomo`
succeeding, then a 20-second wait, then rollback.

Readiness required every rule provider to report `ruleCount > 0`. Custom
direct-domain and direct-IP files are often empty. Those providers stay at
count 0 with no error, so `ready` never equalled `total` and the wait always
timed out. The timeout error also discarded whether the controller was up.

`env_clear()` on the helper-started Mihomo process also removed `PATH`, which
can prevent TUN/`ip` helpers from running.

## Decision

- Treat `custom-direct-domains` and `custom-direct-ips` as ready when they have
  no error, including `ruleCount == 0`.
- Keep bundled providers (`iran-domains`, `iran-networks`, `private-networks`)
  required to load at least one rule.
- Include the last controller/provider status in the readiness timeout.
- Give the helper-started Mihomo process a minimal `PATH` after `env_clear()`.
- Allow `/dev/net/tun` on the transient development helper unit.

## Consequences

- Connect can succeed when the user has no custom direct rules.
- `debug.log` records `mihomo.readiness_wait_*` with a concrete timeout cause.
- Bundled Iran/private providers that fail to load still block readiness.
