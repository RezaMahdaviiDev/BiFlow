# ADR 0064: Windows machine-wide helper staging

## Status

Accepted

## Context

A 4.2.0 Windows `debug.log` showed Connect dying at
`register_runtime_generation` with:

`staged generation … is unreadable at C:\ProgramData\biflow\runtime\generations\…: The system cannot find the file specified. (os error 2)`

The desktop stages generations under the user's
`%LOCALAPPDATA%\biflow\runtime\generations`. The SYSTEM helper only reads
`helper.toml`'s `staging_dir`, which is recorded once at install time.

Two install paths wrote the wrong root:

1. NSIS `perMachine` runs with `SetShellVarContext all`. `$LOCALAPPDATA` then
   expands to `C:\ProgramData`, so the post-install hook recorded
   `C:\ProgramData\biflow\runtime\generations` — not the user's profile.
2. ADR 0035 told NSIS to use `$LOCALAPPDATA\biflow\runtime\generations` and
   forbade `$PROGRAMDATA\iran-split\staging` because the desktop still wrote
   under LocalAppData. That left helper and desktop looking at different trees
   on every packaged Windows install.

A Medium-integrity user also cannot write into a SYSTEM-created `ProgramData`
directory unless Builtin\Users is granted modify.

`staging_dir` must not nest inside `runtime_dir`
(`C:\ProgramData\iran-split\runtime`). A sibling `staging` directory is valid.

## Decision

- Packaged Windows helper staging is always
  `C:\ProgramData\iran-split\staging`. The elevated installer ignores a caller
  `--staging-dir` that points somewhere else and writes that path into
  `helper.toml`.
- NSIS uses `$PROGRAMDATA\iran-split\staging`. `$PROGRAMDATA` is `C:\ProgramData`
  under both current and all-users shell context. Do not use `$LOCALAPPDATA` or
  `$COMMONPROGRAMDATA`.
- In-app Install and `WindowsPaths.generation_staging_dir` use the same
  `ProgramData` root. Tests keep a temp `generation_staging_dir`.
- After creating the directory, the elevated installer runs
  `icacls /grant *S-1-5-32-545:(OI)(CI)M` so unelevated Users can write
  generation files that SYSTEM later publishes.
- Linux is unchanged: the root helper can read the installing user's data
  directory.

## Consequences

A 4.3 NSIS install or in-app Helper Install rewrites `helper.toml` and the
staging ACL. Connect then stages where the helper already looks. A leftover
4.2 helper that still points at `C:\ProgramData\biflow\runtime\generations`
starts working only after that reinstall, because the 4.2 binary cannot see
the new directory.

A source contract must not `include_str!` the whole crate and then
`!contains` a contiguous production snippet: that assertion matches itself.
Scan the production half before `mod tests {`, and keep the same check in
`scripts/tauri-contract.test.mjs` so Linux CI covers the `#![cfg(windows)]`
crate.
