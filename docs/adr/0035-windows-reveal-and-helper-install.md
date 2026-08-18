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

2.7.2 fixed the reveal path and stopped elevating ProgramData, but
`debug (4).log` still showed Install dying with `exit_code: 2` and empty
`detail`. `Start-Process -ArgumentList @('--mihomo', 'C:\Program Files\…')`
concatenates array entries without quoting, so clap sees `C:\Program` and
`Files\…` as extra arguments and calls `process::exit(2)` inside
`Arguments::parse()` — before `persist_install_error`. The same session still
logged Mihomo as `biflow\bin/mihomo.exe` because `mihomo_file_name()` returned
`"bin/mihomo.exe"`.

## Decision

- Build Windows paths from separate components (`.join("biflow").join("debug.log")`,
  `.join("runtime").join("generations")`, `.join("bin").join("mihomo.exe")`) so
  stored paths use `\`.
- Reveal with `explorer.exe` and `CommandExt::raw_arg("/select," + normalized
path)` so the switch is not quoted. Mixed `/` in an old `debug.log` path is
  rewritten to `\` before `/select,`.
- Elevate only a packaged `helper/iran-split-helper.exe` and
  `dependencies/mihomo.exe`. ProgramData is the install destination, not the
  source.
- Skip `fs::copy` when source and destination already resolve to the same file.
- Pass `Start-Process -ArgumentList` a single Windows-quoted command line
  (`--install --mihomo "C:\Program Files\…"`). Do not pass a PowerShell array.
  Keep `-PassThru` and the UAC 1223 mapping (ADR 0029).
- Parse helper flags with `Arguments::try_parse()`. On a usage error, write a
  redacted line to `C:\ProgramData\iran-split\install.log` (no argv or paths)
  and then exit with clap's code. Runtime install/uninstall failures still
  persist their own message. The desktop reads that file after a non-zero exit
  so the dialog is not a bare exit code.
- NSIS and in-app Install use the same helper `staging_dir`:
  `%LOCALAPPDATA%\biflow\runtime\generations` (`$LOCALAPPDATA\biflow\runtime\generations`
  in the NSIS hook). A packaged NSIS run is not part of this repository's
  done gate; the installer-hook contract test is the proof. Do not point
  `--staging-dir` at `$PROGRAMDATA\iran-split\staging` — the desktop stages
  generations under the user LocalAppData tree, and a mismatch leaves
  providers at `0 / 0`.

## Consequences

Open location selects the real `debug.log`. A leftover ProgramData helper no
longer poisons in-app Install. A `Program Files` path stays one argv token, so
clap no longer exits 2. A failed elevate reports the last install.log line,
including clap usage errors. Linux paths and pkexec install are unchanged.
