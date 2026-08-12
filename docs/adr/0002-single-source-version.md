# ADR 0002: Single-source application version

## Status

Accepted

## Context

Version strings were duplicated in `package.json`, `Cargo.toml`, `tauri.conf.json`, and the mock bootstrap payload. They drifted.

## Decision

- The file `version` at the repository root is the only place humans edit (`X.Y.Z`).
- Initial version is `1.0.0`. Every subsequent change increments it.
- `scripts/sync-version.mjs` copies that value into npm, Cargo workspace, and Tauri manifests.
- The UI reads it through Vite `__APP_VERSION__`. The Tauri host reads it with `include_str!("../../version")`.

## Consequences

- Changing one file updates the running app even before manifests are synced.
- CI can fail on `--check` if a manifest was edited by hand.
