# ADR 0037: Windows Mihomo controller reachability

## Status

Accepted

## Context

A 2.7.4 Windows field log (`debug (5).log`) showed Connect succeeding through
Hiddify egress, generation publish, and `start_mihomo`, then failing every
time at readiness:

`Mihomo readiness check timed out: controller unavailable: Mihomo controller
request failed: error sending request for url`.

The UI mapped `CoreError::Platform(...)` to `errors.internal` ("An internal
error occurred" / "خطای داخلی رخ داد").

Three independent Windows conditions produced that same timeout:

1. `ControllerClient` followed `HTTP_PROXY` / `HTTPS_PROXY` / `ALL_PROXY`.
   After Hiddify is up those variables (or a local HTTP proxy) intercept
   `http://127.0.0.1:19090` and fail the request, often as HTTP 500.
2. The helper spawned Mihomo with `env_clear()` and only `PATH`. A
   SYSTEM scheduled task without `SYSTEMROOT` commonly exits before the
   controller binds. The helper still reported `running` because it returned
   from `spawn()`.
3. `wintun.dll` was listed in `resources/external-assets.toml` but never
   vendored, bundled, or copied next to `mihomo.exe`. TUN cannot start
   without it.

## Decision

- Build the controller HTTP client with `no_proxy()`, matching the existing
  public-IP and dependency download clients.
- Map a readiness timeout to `CoreError::ControllerTimeout` (and other
  wait failures to `MihomoStartFailed` / `Cancelled`) so the dashboard
  shows the typed message instead of an internal error.
- On Windows, restore `SYSTEMROOT` / `WINDIR` / `SYSTEMDRIVE` / `PATHEXT`
  after `env_clear()`, put Mihomo's own directory first on `PATH`, and
  spawn with `CREATE_NO_WINDOW`. Wait 400 ms and fail `start` if the
  process has already exited, including the last captured Mihomo line.
- Vendor `wintun.dll` 0.14.1, ship it as `dependencies/wintun.dll`, and
  copy it next to the ProgramData Mihomo during helper install.

## Consequences

Connect on Windows can reach the loopback controller even when Hiddify has
set a system or environment proxy. A dead Mihomo child is reported as a
start failure with its own log line instead of a 20-second internal error.
TUN can load Wintun from the same directory as the helper-owned `mihomo.exe`.
