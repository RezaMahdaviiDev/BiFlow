# ADR 0036: Windows helper scheduled task XML

## Status

Accepted

## Context

2.7.3 fixed clap exit code 2 (`Program Files` quoting). A Windows
`debug (4).log` session at `13:25:09Z` then showed in-app Install succeeding
(no `helper.install_failed`) and immediately failing with **the helper was
installed but is not reachable yet**. Every `get_service_status` after that
was `The system cannot find the file specified. (os error 2)` —
`\\.\pipe\iran-split-helper-v1` never existed.

`install_inner` registered the task with

`schtasks /TR "\"C:\ProgramData\iran-split\bin\iran-split-helper.exe\" --config \"…\helper.toml\""`.

`/Create` and `/Run` return 0 for a stored action that does not start a
process. The helper is a GUI-subsystem binary (ADR 0030), so a failed
`--config` launch writes nothing to the desktop. `wait_for_helper` then
timed out after five seconds.

The log narrows the failure to "the task was registered and started but no
process appeared"; it cannot say whether the stored action was malformed or
the process started and died, because nothing read the task back.

## Decision

- Register `BiFlowHelper` from UTF-16 LE Task Scheduler XML. `Command` is the
  ProgramData helper; `Arguments` is `--config` plus a quoted `helper.toml`.
  Do not pass a quoted `/TR` string.
- `ExecutionTimeLimit` is `PT0S` so the scheduler does not treat the helper
  as a 72-hour job. `AllowHardTerminate` is `true`, otherwise `schtasks /End`
  cannot stop the helper and a reinstall fails copying over its own image.
- End a previous task and wait for its pipe to close before copying the
  helper and Mihomo into `ProgramData`.
- After `/Run`, the elevated installer waits up to 15s for the named pipe.
  Probe it by opening it the way the desktop does — `Path::exists` calls
  `fs::metadata`, which an NPFS object cannot answer, so a stat-based check
  fails on a healthy helper. Only `ERROR_FILE_NOT_FOUND` counts as "not
  serving"; every other outcome proves the pipe exists.
- When the pipe never appears, record why in `install.log`: the scheduled
  helper's own startup error if it wrote one, otherwise the `Command` and
  `Arguments` read back from `schtasks /Query /XML`. Element names are the
  same on every locale, unlike `/FO LIST` labels.
- `run_named_pipe` failures also write `install.log`. The desktop timeout
  appends that line so the dialog is not a bare “not reachable yet”.
- Install failures use `HelperServiceError::Install`, which prints its message
  unprefixed. `Process` would have blamed Mihomo for a Task Scheduler problem.

## Consequences

A successful Install means the pipe is already serving. A broken task action
fails during elevation with a reason, and the reason names the action the
scheduler actually stored, so the next `debug.log` distinguishes a malformed
registration from a helper that started and exited. Linux pkexec install is
unchanged.
