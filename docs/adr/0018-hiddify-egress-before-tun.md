# 0018: Probe Hiddify egress before TUN

## Status

Accepted

## Context

Connect reached Mihomo readiness (`providers 5/5`) and then failed within
~200ms with `Hiddify egress did not become ready`. The SOCKS probe ran *after*
TUN was up. Hiddify AppImage traffic uses `/proc` comm `Hiddify-Linux-x`,
which was not in the PROCESS-NAME DIRECT list, so outbound proxy packets could
be captured by TUN and the probe failed immediately. Rollback then SIGKILL'd
the Hiddify child.

A direct SOCKS probe to `127.0.0.1:12334` succeeds when TUN is not capturing
that path.

## Decision

The Linux clash stack already solved this:

- `PROCESS-NAME-WILDCARD,*Hiddify*,DIRECT` so AppImage comms such as
  `Hiddify-Linux-x` leave the physical interface instead of re-entering TUN.
- Probe Hiddify with `https://www.gstatic.com/generate_204` over SOCKS, with
  retries (up to 45s).
- On a failed Mihomo start, stop only Mihomo; leave Hiddify running.

BiFlow follows that layout:

- Emit the same wildcard process rules, plus desktop `comm` names.
- Confirm SOCKS egress **before** TUN, using generate_204 then optional IP
  lookup, with the probe error in `debug.log`.
- Rollback stops Mihomo and owned routes only. Hiddify is stopped solely on
  explicit Disconnect when `stop_with_stack` is set.

## Consequences

- Connect no longer starts TUN and then tears the stack down because a
  post-TUN SOCKS probe raced the new routes.
- `debug.log` shows `hiddify.egress_probe_failed` with the underlying cause
  instead of a bare unit error.
- Hiddify must still have a working outbound before Connect can succeed; that
  failure now happens in the Starting Hiddify phase.
