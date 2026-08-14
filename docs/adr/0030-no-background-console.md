# ADR 0030: No background console or terminal

## Status

Accepted

## Context

Launching the packaged app opened a second window behind the UI: a `cmd.exe`
console on Windows, and a leftover terminal on Linux. Operators do not want
that window. Application logs already go to the per-user `debug.log` file, not
to stdout.

Two independent causes:

1. The desktop and helper binaries were Windows **console** programs. Explorer
   and NSIS therefore allocated a console for `BiFlow.exe` (and for the helper
   if it was started interactively).
2. Linux WebKit workarounds spawned a child and `Command::status()`-waited.
   The waiting parent stayed alive for the whole GUI session and kept whatever
   terminal the launcher attached. Installed `.desktop` files must also set
   `Terminal=false` so a desktop environment does not open a terminal on
   purpose.

## Decision

- Mark `src-tauri/src/main.rs` and `crates/iran-split-helper/src/main.rs` with
  `#![cfg_attr(windows, windows_subsystem = "windows")]`. CLI tools such as
  `iran-split-cli` stay console binaries.
- Replace the Linux WebKit relaunch with `std::os::unix::process::CommandExt::exec`
  so the first process becomes the GUI instead of waiting on it. See
  [0026](./0026-linux-webkit-blank-view.md).
- Ship `packaging/linux/app.desktop` with `Terminal=false` as
  `bundle.linux.deb.desktopTemplate`.

## Consequences

Rebuilt NSIS, portable Windows, `.deb`, and AppImage packages start only the
GUI window. `./dev.sh` still uses the operator's existing terminal because that
is how the native debug session is launched. Helper install UAC/pkexec prompts
remain; they are not a persistent background console.
