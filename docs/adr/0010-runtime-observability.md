# ADR 0010: Runtime and network observability

## Status

Accepted

## Context

The dashboard previously initialized connection components as `unknown` and a
missing privileged helper stopped startup reconciliation before Hiddify,
Mihomo, TUN, or DNS could be inspected. Users also could not see whether the
machine had internet access or which public IP the current route exposed.

## Decision

- Probe helper, Hiddify listener/discovery, authenticated Mihomo controller,
  owned TUN interface, and DNS listener independently at bootstrap and every
  ten seconds while idle or stable.
- Report each component as checking, stopped, running, degraded, unavailable,
  or error, with a concrete detail. `unknown` remains only for compatibility
  with older serialized data and is not an initial UI state.
- A missing helper is a component prerequisite, not a startup-reconciliation
  failure. Connect still fails safely when privileged work is requested.
- Rust checks public IP and ISO country code through Country.is, with ipify as
  a reachability/IP fallback. Requests are time-bounded and retry once without
  environment proxies. React polls the typed command every 30 seconds and
  renders internet state, IP, city/country, and a Unicode country flag in the
  application status bar.
- When the stack is running or degraded, an accessible inline SVG animates the
  two routing branches. Reduced-motion preferences collapse the animation.

## Consequences

- The dashboard explains what is actually absent, stopped, or running instead
  of showing a row of unknown states.
- Location is approximate public-IP metadata, not device GPS location.
- Internet status depends on two external HTTPS checks but a failure is
  contained to the status bar and never controls routing.
