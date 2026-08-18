# ADR 0055: Clear Hiddify system proxy on Pause and Disconnect

## Status

Accepted

## Context

Hiddify writes `127.0.0.1:12334` into the OS HTTP/HTTPS/SOCKS proxy when it
starts. Disconnect only killed the Hiddify child (`stop_user_proxy`). Pause
never touched that child (ADR 0025). After either action the desktop still
sent browser and app traffic to a dead or idle local proxy.

Corporate proxies that do not point at Hiddify must stay untouched. Connect
must not write the OS proxy; Hiddify already does.

## Decision

- Add `PlatformBackend::clear_hiddify_system_proxy` and
  `restore_hiddify_system_proxy`. Both backends implement them; they are not
  default no-ops.
- Clear only when the current OS proxy host:port matches the configured
  Hiddify endpoint (default `127.0.0.1:12334`). Persist a snapshot under the
  user data directory as `system-proxy-snapshot.json`. Do not log the
  endpoint or registry values.
- **Disconnect** always clears after `stop_user_proxy`.
- **Pause** clears and leaves Hiddify running.
- **Resume** restores the snapshot. A restore failure is a warning; the stack
  stays running.
- **Connect / start** never sets or restores the OS proxy.
- Linux uses `gsettings` (GNOME) and, when present, `kreadconfig5` /
  `kwriteconfig5` (KDE) via `Command`. Missing tools are a no-op `Ok`.
- Windows reads and writes `HKCU\...\Internet Settings` (`ProxyEnable` /
  `ProxyServer`) through `PowerShell` with `CREATE_NO_WINDOW`, then notifies
  WinInet (`InternetSetOption` 39 then 37) without `unsafe` in this crate.

## Consequences

- Pause still does not stop Hiddify. If Hiddify re-enables the OS proxy while
  paused, traffic can return to `127.0.0.1:12334`. This ADR does not rewrite
  Hiddify `shared_preferences.json` unless a later session proves that is the
  only fix.
- Core FakeBackend tests cover pause-clears-without-stop, stop-clears,
  resume-restores, and connect-does-not-set. CLI `DemoBackend` no-ops both
  methods; it has no OS proxy. A source contract must require those methods
  on `DemoBackend` as well as the Windows backend, because Windows workspace
  Clippy compiles `iran-split-cli` and a missing impl fails there first.
