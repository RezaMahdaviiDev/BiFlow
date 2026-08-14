# ADR 0027: Windows debug.log clear and test fixtures

## Status

Accepted

## Context

After Windows Clippy started passing, `cargo test --workspace` on
`windows-2025` failed. Two host-cfg gaps were invisible on Linux:

1. Diagnostics keeps `debug.log` open with `.append(true)`. The delete action
   called `File::set_len(0)` on that handle. Windows `SetEndOfFile` returns
   `PermissionDenied` for `FILE_APPEND_DATA`, so both the unit test and the
   real Diagnostics clear path failed.
2. `existing_binaries_in_data_dir_hide_install` wrote `bin/mihomo`, but
   Windows looks for `bin/mihomo.exe`. The reveal-folder test also hardcoded
   a Unix `/tmp/...` string that Windows `Path::to_string_lossy` does not
   produce.

## Decision

- Close the append handle, truncate the file, then reopen it in append mode
  when the user clears `debug.log`. Keep the in-memory file optional so the
  old handle can drop before the truncate open.
- Write dependency fixtures with `mihomo_file_name()` and assert reveal
  arguments from `path.to_string_lossy()` instead of a Unix path literal.

## Consequences

Windows CI exercises the same clear and install-detection contracts as Linux.
The Diagnostics delete action works on Windows without dropping later events.
