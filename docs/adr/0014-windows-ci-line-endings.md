# ADR 0014: Windows CI line endings and config fsync

## Status

Accepted

## Context

Windows GitHub Actions failed in two places:

1. `pnpm rules:check` during Tauri `beforeBuildCommand` because Git converted LF
   to CRLF on checkout, changing bundled rule file SHA-256 values.
2. `iran-split-config` test `persists_atomically_and_checks_revision` because
   `sync_directory` opened a directory with Unix-style `OpenOptions::read`, which
   returns `PermissionDenied` on Windows.

## Decision

- Add root `.gitattributes` with `-text` for `resources/rules/*.txt`,
  `manifest.json`, and `SNAPSHOT.md`.
- Set `git config --global core.autocrlf false` on Windows runners **before**
  `actions/checkout`. Setting it after checkout leaves already-converted files
  in the working tree.
- Make `sync_directory` Unix-only; Windows uses a no-op while keeping atomic
  file writes via `NamedTempFile::persist`.

## Consequences

- `rules:check` passes on Windows release builds without re-downloading rules.
- `cargo test --workspace` on `windows-2025` passes for config persistence.
- New release tags must include this commit; older tags without `.gitattributes`
  still fail on Windows.
