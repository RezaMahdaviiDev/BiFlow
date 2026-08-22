# ADR 0066: Reachability section in Diagnostics

## Status

Accepted

## Context

A connected stack can still fail per-destination: a Hiddify node with a
working egress (Cloudflare 204, foreign exit IP) closed every TLS handshake
whose SNI was a Google domain, so Chrome showed `ERR_CONNECTION_CLOSED` for
gmail.com while the tunnel looked healthy. Browsers cannot render a custom
error page for HTTPS destinations (no trusted certificate, HSTS preload), so
the explanation has to live in the app.

## Decision

- Add a Reachability card to Diagnostics that probes three fixed domains:
  `google.com` and `facebook.com` (VPN path) and `iran.ir` (DIRECT path).
- Probe VPN-path domains through the Hiddify SOCKS port, not the Mihomo mixed
  port: the desktop process is PROCESS-NAME-bypassed to DIRECT in the
  generated rules, so a mixed-port probe would silently test the wrong path.
  When Hiddify is not running, fall back to a direct request and report
  `via_proxy: false` so the UI explains the failure as "connect first".
- Any HTTP status counts as reachable; the check is whether the TLS handshake
  survives SNI filtering. Green under 2.5 s, yellow above it, red on a
  transport error.
- Yellow and red rows open a modal listing likely causes (node closes this
  SNI / node ISP blocks it / no egress; DIRECT DNS or internet trouble for
  `iran.ir`) with a Try again button. Probe failures log `without_url()`
  details only.

## Consequences

- Users see "the node kills Google" inside BiFlow instead of a bare Chrome
  error, with the fix (switch nodes in Hiddify) one click away.
- The fixed probe domains are app-owned, so logging their ids does not leak
  user browsing targets.
