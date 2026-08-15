# ADR 0035: Windows debug.log reveal and helper install

## Status

Accepted

## Context

A Windows 2.7.0 `debug.log` (`debug (3).log`) showed two packaged-app failures
that do not occur on Linux:

1. Diagnostics **Open location** spawned Explorer in 12ms but did not select
   `debug.log`. `session.opened` recorded
   `C:\Users\…\AppData\Local\biflow/debug.log` because `.join("biflow/debug.log")`
   keeps the `/` on Windows. `explorer.exe /select,C:\…\biflow/debug.log` is
   treated as an unknown switch, so Explorer opens This PC.
2. In-app **Install** failed three times with `exit_code: 1` and `detail: ""`.
   Startup had already resolved Mihomo to
   `C:\ProgramData\iran-split\bin\mihomo.exe` while
   `\\.\pipe\iran-split-helper-v1` was missing (`os error 2`). Elevation
   preferred that leftover ProgramData helper, which then `fs::copy`'d itself
   onto itself. The helper is a GUI-subsystem binary (ADR 0030), and
   `Start-Process -Verb RunAs` cannot capture elevated stderr, so the UI had
   no reason. The NSIS hook used `$COMMONPROGRAMDATA`, which NSIS 3 does not
   define, so a first-run install could leave those leftovers.

## Decision

- Build Windows paths from separate components (`.join("biflow").join("debug.log")`,
  `.join("runtime").join("generations")`) so stored paths use `\`.
- Reveal with `explorer.exe` and `CommandExt::raw_arg("/select," + normalized
path)` so the switch is not quoted. Mixed `/` in an old `debug.log` path is
  rewritten to `\` before `/select,`.
- Elevate only a packaged `helper/iran-split-helper.exe` and
  `dependencies/mihomo.exe`. ProgramData is the install destination, not the
  source.
- Skip `fs::copy` when source and destination already resolve to the same file.
- Pass `Start-Process -ArgumentList` as a PowerShell array of separate flags
  and values. Keep `-PassThru` and the UAC 1223 mapping (ADR 0029).
- On install/uninstall failure the elevated helper writes
  `C:\ProgramData\iran-split\install.log` (error text only). The desktop reads
  that file after a non-zero exit so the dialog is not a bare exit code.
- NSIS uses `$PROGRAMDATA\iran-split\staging` so `HelperSettings::validate`
  sees an absolute path. In-app Install still records
  `%LOCALAPPDATA%\biflow\runtime\generations` in `helper.toml` and overwrites a
  failed NSIS hook.

## Consequences

Open location selects the real `debug.log`. A leftover ProgramData helper no
longer poisons in-app Install. A failed elevate reports the last install.log
line. Linux paths and pkexec install are unchanged.
