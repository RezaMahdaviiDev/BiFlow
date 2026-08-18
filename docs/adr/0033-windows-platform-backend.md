# ADR 0033: Windows platform backend

## Status

Accepted

## Context

`WindowsBackend` implemented only `helper_status()`. Every other
`PlatformBackend` method returned one canned
`CoreError::Platform("Windows backend must run on a signed Windows build with
the installed helper service")`, and `runtime_health()` hardcoded Hiddify,
Mihomo, TUN, and DNS to `Unavailable`. Those `unavailable()` calls were not
behind `#[cfg]`, so that was the behaviour on real Windows too.

A 2.0.0 field log showed the consequence: Helper reported Running because the
named-pipe probe is real, every other card read Unavailable, and nine Connect
attempts across three sessions each failed instantly at `ensure_hiddify`.

## Decision

- `iran-split-platform-win` implements the whole `PlatformBackend` contract,
  mirroring `iran-split-platform-linux` step for step so both platforms fail in
  the same places for the same reasons. The stub `unavailable()` is gone.
- Privileged calls go over the versioned local named pipe to the SYSTEM
  scheduled task from ADR 0029, replacing the Unix socket. `ClientOptions::open`
  is synchronous, so the retry loop lives inside a timeout and retries only
  `ERROR_PIPE_BUSY`. A `NotFound` / `ConnectionRefused` / timeout on the pipe
  reports an uninstalled helper rather than an error banner.
- Runtime generations for packaged Windows stage into
  `C:\ProgramData\iran-split\staging\<id>` — the same root the elevated
  installer records as `staging_dir` in `helper.toml` (ADR 0064). Tests pass a
  temp `generation_staging_dir`. `generate_config` is called with
  `Platform::Windows`, which is what sets `strict-route`.
- TUN state is read from Mihomo's own `/configs`, not from the adapter list.
  Enumerating adapters needs `GetAdaptersAddresses`, and this crate is under the
  workspace `unsafe_code = "forbid"`; Mihomo owns the Wintun adapter, so its
  running config is the authority on whether the tunnel is up. A stale adapter
  left by a crashed Mihomo therefore reads as Stopped, which is the honest
  answer for routing purposes.
- The crate carries `#![cfg(windows)]` as its first item, mirroring
  `#![cfg(target_os = "linux")]` on the Linux crate, so neither host compiles
  the other's platform code.
- Generation staging helpers (`write_atomic`, `copy_rule_file`,
  `write_custom_provider_files`) are duplicated per platform rather than shared.
  Extracting them would edit the working Linux backend for no runtime benefit;
  a script contract test asserts both backends stage the same six files and pin
  their own `Platform` value, which is what drift would actually break.

## Consequences

- Windows Connect now runs the real sequence: start Hiddify, probe its SOCKS
  egress, stage and validate a generation, start Mihomo under the helper, wait
  for the controller and providers. Each step can fail with its own message
  instead of one blanket platform error.
- Everything is verified by `cargo xwin clippy` plus `cargo xwin test` under
  wine (ADR 0031). wine is not Windows: the named pipe, the Wintun adapter,
  the scheduled task, and UAC are exercised only on a real machine, so the
  first run on Windows hardware remains the real test.
- `cargo test --workspace` on a Linux host no longer runs this crate's tests at
  all; they run in the `windows-2025` job and in the local wine step.
